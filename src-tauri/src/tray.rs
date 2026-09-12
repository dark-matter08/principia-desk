//! The menu bar desk.
//!
//! The app stays resident: closing or hiding the window leaves this icon and
//! its menu, which say what is due, what is next, what today holds, and offer
//! the only two things an alarm accepts: starting the lesson, or a snooze.
//! Quit lives here too, and is withheld while an alarm rings or a focused
//! session holds the desk, so the background runner cannot be dismissed as a
//! way around either.

use crate::alarm::{self, AlarmView};
use crate::state::AppState;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{
    menu::{Menu, MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, LogicalSize, Manager, WebviewUrl, WebviewWindowBuilder,
};

/// The card is playing its exit; the window goes once it has.
static LEAVING: AtomicBool = AtomicBool::new(false);
/// How long the card takes to leave (`TrayPanel.svelte` matches it).
const LEAVE_MS: u64 = 170;
/// Where the panel's top-left corner sits, so a resize keeps it under the icon.
static ORIGIN: std::sync::Mutex<(f64, f64)> = std::sync::Mutex::new((0.0, 0.0));

pub const TRAY_ID: &str = "desk";
/// The popover under the icon. A left click toggles it; the native menu stays
/// on the right button so Quit and the plain list are never more than a click
/// away if the panel misbehaves.
pub const PANEL_LABEL: &str = "tray";
const PANEL_WIDTH: f64 = 380.0;
const PANEL_HEIGHT: f64 = 600.0;

fn label_item(
    app: &AppHandle,
    id: &str,
    text: &str,
) -> tauri::Result<tauri::menu::MenuItem<tauri::Wry>> {
    MenuItemBuilder::with_id(id, text).enabled(false).build(app)
}

fn clock(value: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|t| t.with_timezone(&chrono::Local).format("%H:%M").to_string())
        .unwrap_or_else(|_| value.to_string())
}

/// Build the menu for the current state. Rebuilt whole on every refresh, which
/// is simpler than editing items and never leaves a stale line behind.
fn build(app: &AppHandle, alarm: Option<&AlarmView>) -> tauri::Result<Menu<tauri::Wry>> {
    let state = app.state::<AppState>();
    let (paused, slots, appointments) = {
        let conn = state.db.0.lock().unwrap();
        let today = state.today();
        (
            matches!(crate::db::get_config(&conn, "schedule_paused"), Ok(Some(v)) if v == "1"),
            crate::classroom::slot_views(&conn, &today, state.debug_day).unwrap_or_default(),
            crate::classroom::refresh_appointments(&conn, &today).unwrap_or_default(),
        )
    };
    let mut menu = MenuBuilder::new(app);

    if let Some(alarm) = alarm {
        let headline = match (&alarm.snoozed_until, alarm.readiness) {
            (Some(until), _) => format!("{} · due, snoozed until {}", alarm.label, clock(until)),
            (None, crate::readiness::Readiness::Ready) => {
                format!("{} · ready, due now", alarm.label)
            }
            (None, crate::readiness::Readiness::Preparing) => {
                format!("{} · due, lesson preparing…", alarm.label)
            }
            (None, crate::readiness::Readiness::Failed) => {
                format!("{} · due, preparation failed", alarm.label)
            }
        };
        menu = menu.item(&label_item(app, "alarm", &headline)?);
        if alarm.queued > 0 {
            menu = menu.item(&label_item(
                app,
                "queued",
                &format!("{} more waiting behind it", alarm.queued),
            )?);
        }
        menu = menu.item(
            &MenuItemBuilder::with_id(
                format!("start:{}", alarm.occurrence_id),
                format!("Start {}", alarm.label),
            )
            .build(app)?,
        );
        if alarm.ringing() {
            for minutes in alarm::SNOOZE_MINUTES {
                menu = menu.item(
                    &MenuItemBuilder::with_id(
                        format!("snooze:{}:{minutes}", alarm.occurrence_id),
                        format!("Snooze {minutes} min"),
                    )
                    .build(app)?,
                );
            }
        }
        menu = menu.separator();
    } else if paused {
        menu = menu.item(&label_item(app, "next", "Appointments paused")?);
        menu = menu.separator();
    } else {
        let next = slots
            .iter()
            .filter(|slot| slot.enabled && !slot.owed && !slot.next_fire_at.is_empty())
            .min_by(|a, b| a.next_fire_at.cmp(&b.next_fire_at));
        let text = match next {
            Some(slot) => {
                let when = chrono::DateTime::parse_from_rfc3339(&slot.next_fire_at)
                    .map(|t| {
                        let local = t.with_timezone(&chrono::Local);
                        if local.date_naive() == chrono::Local::now().date_naive() {
                            format!("today {}", local.format("%H:%M"))
                        } else {
                            local.format("%a %H:%M").to_string()
                        }
                    })
                    .unwrap_or_else(|_| slot.next_fire_at.clone());
                format!("Next: {} · {when}", slot.label)
            }
            None => "No study times set".to_string(),
        };
        menu = menu.item(&label_item(app, "next", &text)?);
        menu = menu.separator();
    }

    if !appointments.is_empty() {
        menu = menu.item(&label_item(app, "today", "Today")?);
        for appointment in appointments.iter().take(6) {
            let line = format!(
                "  {} {} · {}",
                appointment.local_time, appointment.label, appointment.disposition
            );
            menu = menu.item(&label_item(
                app,
                &format!("appt:{}", appointment.id),
                &line,
            )?);
        }
        menu = menu.separator();
    }

    menu = menu.item(&MenuItemBuilder::with_id("open", "Open Principia Desk").build(app)?);
    menu = if paused {
        menu.item(&MenuItemBuilder::with_id("resume", "Resume appointments").build(app)?)
    } else {
        menu.item(&MenuItemBuilder::with_id("pause", "Pause appointments").build(app)?)
    };
    menu = menu.separator();
    let ringing = alarm.is_some_and(AlarmView::ringing);
    let locked = state.locked.load(Ordering::SeqCst);
    menu = menu.item(
        &MenuItemBuilder::with_id("quit", "Quit Principia Desk")
            .enabled(!ringing && !locked)
            .build(app)?,
    );
    menu.build()
}

/// Bring the desk window back from the menu bar.
pub fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Start a due class from outside the desk window: bring the desk up and let
/// its store run the start, so preparation shows exactly as it would in-app.
pub fn start_from_outside(app: &AppHandle, occurrence: &str) -> Result<(), String> {
    let state = app.state::<AppState>();
    let alarm = alarm::current(&state).ok_or("Nothing is due right now.")?;
    if alarm.occurrence_id != occurrence && alarm.queued == 0 {
        return Err("That appointment is not the one due.".into());
    }
    hide_panel(app);
    show_window(app);
    app.emit(
        "tray:start",
        serde_json::json!({ "course_id": alarm.course_id, "occurrence_id": occurrence }),
    )
    .map_err(|e| e.to_string())
}

/// Quit from the menu bar: refused while an alarm rings or a session holds the desk.
pub fn quit(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    if state.alarm_ringing.load(Ordering::SeqCst) {
        return Err("The study alarm is ringing. Start the lesson first.".into());
    }
    if state.locked.load(Ordering::SeqCst) {
        return Err("A focused session holds the desk. Finish or break the glass first.".into());
    }
    app.exit(0);
    Ok(())
}

fn handle(app: &AppHandle, id: &str) {
    let state = app.state::<AppState>();
    match id {
        "open" => show_window(app),
        "pause" => {
            if let Err(error) = crate::commands::pause_schedule(app.clone(), app.state()) {
                log::warn!("pause from the menu bar failed: {error}");
            }
            alarm::evaluate(app);
        }
        "resume" => {
            if let Err(error) = crate::commands::resume_schedule(app.clone(), app.state()) {
                log::warn!("resume from the menu bar failed: {error}");
            }
            alarm::evaluate(app);
        }
        "quit" => {
            if let Err(error) = quit(app) {
                log::warn!("quit refused: {error}");
            }
        }
        other => {
            if let Some(occurrence) = other.strip_prefix("start:") {
                if let Err(error) = start_from_outside(app, occurrence) {
                    log::warn!("start from the menu bar failed: {error}");
                }
            } else if let Some(rest) = other.strip_prefix("snooze:") {
                if let Some((occurrence, minutes)) = rest.rsplit_once(':') {
                    match minutes
                        .parse::<u32>()
                        .map_err(|_| "bad snooze".to_string())
                        .and_then(|m| alarm::snooze(&state, occurrence, m))
                    {
                        Ok(_) => alarm::evaluate(app),
                        Err(error) => log::warn!("snooze from the menu bar failed: {error}"),
                    }
                }
            }
        }
    }
}

/// The panel window, created on first use and hidden rather than closed.
fn panel(app: &AppHandle) -> tauri::Result<tauri::WebviewWindow> {
    if let Some(window) = app.get_webview_window(PANEL_LABEL) {
        return Ok(window);
    }
    let window = WebviewWindowBuilder::new(app, PANEL_LABEL, WebviewUrl::App("tray".into()))
        .title("Principia Desk")
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .visible_on_all_workspaces(true)
        .skip_taskbar(true)
        .visible(false)
        .inner_size(PANEL_WIDTH, PANEL_HEIGHT)
        .build()?;
    Ok(window)
}

/// Show the panel centred under the icon, or hide it if it is showing.
///
/// The window appears in place, at once; the card inside it plays the
/// entrance (a drop from under the menu bar), which is what a person sees
/// and what stays smooth. On macOS the window is a non-activating panel,
/// so it shows over whatever is frontmost, on whatever space, without the
/// desk coming forward or the space changing.
fn toggle_panel(app: &AppHandle, rect: tauri::Rect) {
    let Ok(window) = panel(app) else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        hide_panel(app);
        return;
    }
    LEAVING.store(false, Ordering::SeqCst);
    let scale = window.scale_factor().unwrap_or(1.0);
    let position = rect.position.to_logical::<f64>(scale);
    let size = rect.size.to_logical::<f64>(scale);
    let height = *app.state::<AppState>().panel_height.lock().unwrap();
    let x = (position.x + size.width / 2.0 - PANEL_WIDTH / 2.0).max(8.0);
    let y = position.y + size.height + 4.0;
    *ORIGIN.lock().unwrap() = (x, y);
    // The card starts its entrance before the window is on screen, so the
    // first frame is already in motion.
    let _ = app.emit("tray:refresh", ());
    #[cfg(target_os = "macos")]
    {
        let w = window.clone();
        let _ = window.run_on_main_thread(move || {
            let _ = objc2::exception::catch(std::panic::AssertUnwindSafe(|| unsafe {
                if let Ok(ptr) = w.ns_window() {
                    let ptr = ptr as *mut objc2::runtime::AnyObject;
                    crate::menu_panel::adopt(ptr);
                    crate::menu_panel::present(ptr, x, y, PANEL_WIDTH, height);
                }
            }));
        });
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = window.set_size(LogicalSize::new(PANEL_WIDTH, height));
        let _ = window.set_position(tauri::LogicalPosition::new(x, y));
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// The panel reports the height of its card so the window carries no dead
/// space. The top edge stays put: the card grows downward from the icon.
pub fn size_panel(app: &AppHandle, height: f64) {
    let bounded = height.clamp(160.0, 900.0);
    *app.state::<AppState>().panel_height.lock().unwrap() = bounded;
    let Some(window) = app.get_webview_window(PANEL_LABEL) else {
        return;
    };
    #[cfg(target_os = "macos")]
    {
        if window.is_visible().unwrap_or(false) {
            let (x, y) = *ORIGIN.lock().unwrap();
            let w = window.clone();
            let _ = window.run_on_main_thread(move || {
                let _ = objc2::exception::catch(std::panic::AssertUnwindSafe(|| unsafe {
                    if let Ok(ptr) = w.ns_window() {
                        crate::menu_panel::place(
                            ptr as *mut objc2::runtime::AnyObject,
                            x,
                            y,
                            PANEL_WIDTH,
                            bounded,
                        );
                    }
                }));
            });
            return;
        }
    }
    let _ = window.set_size(LogicalSize::new(PANEL_WIDTH, bounded));
}

/// Hide the panel, for instance when it loses focus or after an action:
/// the card plays its exit first, then the window goes.
pub fn hide_panel(app: &AppHandle) {
    let Some(window) = app.get_webview_window(PANEL_LABEL) else {
        return;
    };
    if !window.is_visible().unwrap_or(false) || LEAVING.swap(true, Ordering::SeqCst) {
        return;
    }
    let _ = app.emit("tray:leave", ());
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(LEAVE_MS)).await;
        if LEAVING.load(Ordering::SeqCst) {
            let _ = window.hide();
            LEAVING.store(false, Ordering::SeqCst);
        }
    });
}

/// Install the icon once at startup. Later state changes go through `refresh`.
pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let menu = build(app, None)?;
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Principia Desk")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| handle(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } = event
            {
                toggle_panel(tray.app_handle(), rect);
            }
        });
    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }
    builder.build(app)?;
    Ok(())
}

/// Rebuild the menu and the menu bar title from the current state.
pub fn refresh(app: &AppHandle, alarm: Option<&AlarmView>) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    match build(app, alarm) {
        Ok(menu) => {
            let _ = tray.set_menu(Some(menu));
        }
        Err(error) => log::warn!("menu bar refresh failed: {error}"),
    }
    // The icon stands alone in the menu bar; the state lives in the tooltip
    // and the panel rather than as text beside it.
    let _ = tray.set_title(None::<&str>);
    let tooltip = alarm
        .map(|a| match (a.snoozed_until.is_some(), a.readiness) {
            (true, _) => format!("Principia Desk · {} snoozed", a.label),
            (false, crate::readiness::Readiness::Ready) => {
                format!("Principia Desk · {} due", a.label)
            }
            (false, crate::readiness::Readiness::Preparing) => {
                format!("Principia Desk · preparing {}", a.label)
            }
            (false, crate::readiness::Readiness::Failed) => {
                format!("Principia Desk · {} needs a retry", a.label)
            }
        })
        .unwrap_or_else(|| "Principia Desk".into());
    let _ = tray.set_tooltip(Some(tooltip));
}
