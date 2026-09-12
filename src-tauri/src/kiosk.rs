use crate::state::AppState;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Manager};

/// User home directory, cross-platform (`HOME` on unix, `USERPROFILE` on Windows).
fn home_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from)
}

/// Name of the release token. Creating a file with this name in any of the
/// places below frees a locked desk within one refocus tick.
pub const UNLOCK_TOKEN: &str = "principia-unlock";
/// The token name used before the rename. A stick prepared then still works.
pub const LEGACY_UNLOCK_TOKEN: &str = "sdr-unlock";

/// Longest a lock may hold before it releases itself. A study session lasts
/// well under an hour; a lock still standing after this is a stuck app, not a
/// lesson, and a machine must never be held hostage by one.
pub const MAX_LOCK: std::time::Duration = std::time::Duration::from_secs(3 * 3600);

/// Places a release token is accepted. The home directory is the documented
/// one and needs a shell; the removable volumes exist so a locked machine can
/// be freed with no terminal at all, by plugging in a stick that carries a
/// file named `principia-unlock` at its root.
fn token_roots() -> Vec<std::path::PathBuf> {
    let mut roots: Vec<std::path::PathBuf> = home_dir().into_iter().collect();
    roots.push(std::env::temp_dir());
    #[cfg(target_os = "macos")]
    let mounts = ["/Volumes"];
    #[cfg(target_os = "linux")]
    let mounts = ["/media", "/run/media", "/mnt"];
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let mounts: [&str; 0] = [];
    for mount in mounts {
        if let Ok(entries) = std::fs::read_dir(mount) {
            for entry in entries.flatten() {
                let path = entry.path();
                // A per-user layer (/run/media/<user>/<stick>) sits one deeper.
                if let Ok(inner) = std::fs::read_dir(&path) {
                    roots.extend(inner.flatten().map(|e| e.path()));
                }
                roots.push(path);
            }
        }
    }
    #[cfg(target_os = "windows")]
    roots.extend(('D'..='Z').map(|drive| std::path::PathBuf::from(format!("{drive}:\\"))));
    roots
}

/// The first release token that exists, if any.
fn token_in(roots: &[std::path::PathBuf]) -> Option<std::path::PathBuf> {
    roots
        .iter()
        .flat_map(|root| [root.join(UNLOCK_TOKEN), root.join(LEGACY_UNLOCK_TOKEN)])
        .find(|path| path.exists())
}

/// Whether the desk is disarmed by a release token right now.
pub fn release_token() -> Option<std::path::PathBuf> {
    token_in(&token_roots())
}

/// Silence the room when the lock engages: pause every scriptable media
/// player that's running, remember the system mute state, then mute.
/// Best-effort and platform-specific — failures are logged and ignored.
/// `prev_muted` records whether audio was *already* muted so `restore_media`
/// only un-mutes if we were the ones who muted.
fn pause_media(state: &AppState) {
    #[cfg(target_os = "macos")]
    {
        let was_muted = std::process::Command::new("osascript")
            .args(["-e", "output muted of (get volume settings)"])
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "true");
        *state.prev_muted.lock().unwrap() = was_muted;

        for app_name in ["Music", "Spotify", "TV", "QuickTime Player", "VLC", "IINA"] {
            let script = format!(
                "if application \"{app_name}\" is running then tell application \"{app_name}\" to pause"
            );
            let _ = std::process::Command::new("osascript")
                .args(["-e", &script])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
        }
        let _ = std::process::Command::new("osascript")
            .args(["-e", "set volume output muted true"])
            .spawn();
    }

    #[cfg(target_os = "linux")]
    {
        // PulseAudio/PipeWire via pactl; pause players via playerctl. Both are
        // optional — missing binaries just no-op.
        let was_muted = std::process::Command::new("pactl")
            .args(["get-sink-mute", "@DEFAULT_SINK@"])
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains("yes"));
        *state.prev_muted.lock().unwrap() = was_muted;

        let _ = std::process::Command::new("playerctl")
            .args(["--all-players", "pause"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        let _ = std::process::Command::new("pactl")
            .args(["set-sink-mute", "@DEFAULT_SINK@", "1"])
            .spawn();
    }

    // Other platforms (Windows): no scriptable mute; leave state untouched.
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        *state.prev_muted.lock().unwrap() = None;
    }
}

/// Restore the pre-lock mute state (only unmute if WE muted).
fn restore_media(state: &AppState) {
    let we_muted = matches!(*state.prev_muted.lock().unwrap(), Some(false));
    if we_muted {
        #[cfg(target_os = "macos")]
        let _ = std::process::Command::new("osascript")
            .args(["-e", "set volume output muted false"])
            .spawn();

        #[cfg(target_os = "linux")]
        let _ = std::process::Command::new("pactl")
            .args(["set-sink-mute", "@DEFAULT_SINK@", "0"])
            .spawn();
    }
    *state.prev_muted.lock().unwrap() = None;
}

#[cfg(target_os = "macos")]
pub(crate) mod mac {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    // NSApplicationPresentationOptions bits
    pub const HIDE_DOCK: usize = 1 << 1;
    pub const HIDE_MENU_BAR: usize = 1 << 3;
    pub const DISABLE_PROCESS_SWITCHING: usize = 1 << 5;
    pub const DISABLE_FORCE_QUIT: usize = 1 << 6;
    pub const DISABLE_SESSION_TERMINATION: usize = 1 << 7;
    pub const DISABLE_HIDE_APPLICATION: usize = 1 << 8;

    // NSWindowCollectionBehavior bits
    pub const CAN_JOIN_ALL_SPACES: usize = 1 << 0;
    pub const STATIONARY: usize = 1 << 4;
    pub const FULL_SCREEN_AUXILIARY: usize = 1 << 8;

    pub const SCREEN_SAVER_WINDOW_LEVEL: isize = 1000;

    pub unsafe fn ns_app() -> *mut AnyObject {
        let cls = objc2::class!(NSApplication);
        msg_send![cls, sharedApplication]
    }

    pub unsafe fn set_presentation_options(options: usize) {
        let app = ns_app();
        let _: () = msg_send![app, setPresentationOptions: options];
    }

    pub unsafe fn raise_window(ns_window: *mut AnyObject) {
        let _: () = msg_send![ns_window, setLevel: SCREEN_SAVER_WINDOW_LEVEL];
        let behavior = CAN_JOIN_ALL_SPACES | STATIONARY | FULL_SCREEN_AUXILIARY;
        let _: () = msg_send![ns_window, setCollectionBehavior: behavior];
        let _: () = msg_send![ns_window, orderFrontRegardless];
    }

    /// One level above the kiosk, so the recovery console shows over a
    /// locked desk and its blankers.
    pub unsafe fn raise_console(ns_window: *mut AnyObject) {
        let _: () = msg_send![ns_window, setLevel: SCREEN_SAVER_WINDOW_LEVEL + 1];
        let behavior = CAN_JOIN_ALL_SPACES | STATIONARY | FULL_SCREEN_AUXILIARY;
        let _: () = msg_send![ns_window, setCollectionBehavior: behavior];
        let _: () = msg_send![ns_window, orderFrontRegardless];
    }

    pub unsafe fn reset_window(ns_window: *mut AnyObject) {
        let _: () = msg_send![ns_window, setLevel: 0isize];
    }

    pub unsafe fn frontmost_pid() -> i32 {
        let cls = objc2::class!(NSWorkspace);
        let ws: *mut AnyObject = msg_send![cls, sharedWorkspace];
        let front: *mut AnyObject = msg_send![ws, frontmostApplication];
        if front.is_null() {
            return -1;
        }
        msg_send![front, processIdentifier]
    }

    pub unsafe fn activate_self() {
        let app = ns_app();
        let _: () = msg_send![app, activateIgnoringOtherApps: true];
    }
}

/// Configurable strictness (config `kiosk_level`).
#[derive(PartialEq, Clone, Copy, Debug)]
pub enum KioskLevel {
    /// Window stays on top and nags; nothing is blocked.
    Advisory,
    /// Full-screen + refocus + blankers + mute, but Force Quit still works.
    Firm,
    /// Everything disabled. Finish, exit check, or break glass.
    Hard,
}

pub fn configured_level(state: &AppState) -> KioskLevel {
    let conn = state.db.0.lock().unwrap();
    match crate::db::get_config(&conn, "kiosk_level")
        .ok()
        .flatten()
        .as_deref()
    {
        Some("advisory") => KioskLevel::Advisory,
        Some("firm") => KioskLevel::Firm,
        _ => KioskLevel::Hard,
    }
}

/// Engage the kiosk lock at the globally configured level.
pub fn engage(app: &AppHandle, state: &AppState) {
    engage_at(app, state, configured_level(state))
}

/// Engage the kiosk lock on the main window at an explicit level and start
/// the refocus loop. The focus coordinator chooses the level per session.
pub fn engage_at(app: &AppHandle, state: &AppState, level: KioskLevel) {
    if state.debug_day {
        log::info!("debug-day: kiosk engagement skipped");
        state.locked.store(true, Ordering::SeqCst);
        return;
    }
    // Safety interlock: never lock behind a webview that hasn't booted —
    // the escape hatch lives in it. The owed watcher retries once it's up.
    if !state.frontend_ready.load(Ordering::SeqCst) {
        log::warn!("kiosk engage refused: frontend not ready (white-screen guard)");
        return;
    }
    if let Some(token) = release_token() {
        log::warn!("kiosk engage refused: release token at {}", token.display());
        return;
    }
    if state.locked.swap(true, Ordering::SeqCst) {
        return;
    }
    log::info!("kiosk engaging at level {level:?}");
    let Some(window) = app.get_webview_window("main") else {
        state.locked.store(false, Ordering::SeqCst);
        log::warn!("kiosk engage deferred: main window unavailable");
        // A focus coordinator retries from its captured class/session owner.
        // Never fall back to the retired daily routine after a missing window.
        return;
    };
    if level == KioskLevel::Advisory {
        // Honor system: surface and pin the window, block nothing.
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_always_on_top(true);
        let _ = window.set_focus();
        return;
    }
    pause_media(state);
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_resizable(false);
    let _ = window.set_decorations(false);
    let _ = window.set_always_on_top(true);
    let _ = window.set_visible_on_all_workspaces(true);
    if let Ok(Some(monitor)) = window.primary_monitor() {
        let size = monitor.size();
        let pos = monitor.position();
        let _ = window.set_position(tauri::PhysicalPosition::new(pos.x, pos.y));
        let _ = window.set_size(tauri::PhysicalSize::new(size.width, size.height));
    }
    let _ = window.set_focus();

    #[cfg(target_os = "macos")]
    {
        let win = window.clone();
        let _ = window.run_on_main_thread(move || {
            // Activate first: presentation options only apply (and may throw)
            // when set by the active application.
            let r = objc2::exception::catch(std::panic::AssertUnwindSafe(|| unsafe {
                mac::activate_self();
                if let Ok(ptr) = win.ns_window() {
                    mac::raise_window(ptr as *mut objc2::runtime::AnyObject);
                }
            }));
            if let Err(e) = r {
                log::error!("kiosk raise failed: {e:?}");
            }
            let r = objc2::exception::catch(std::panic::AssertUnwindSafe(|| unsafe {
                // FIRM keeps Cmd+Tab/Force Quit functional (the refocus loop
                // makes switching useless but never traps); HARD disables them.
                let opts = if level == KioskLevel::Hard {
                    mac::HIDE_DOCK
                        | mac::HIDE_MENU_BAR
                        | mac::DISABLE_PROCESS_SWITCHING
                        | mac::DISABLE_FORCE_QUIT
                        | mac::DISABLE_SESSION_TERMINATION
                        | mac::DISABLE_HIDE_APPLICATION
                } else {
                    mac::HIDE_DOCK | mac::HIDE_MENU_BAR
                };
                mac::set_presentation_options(opts);
            }));
            if let Err(e) = r {
                log::error!("kiosk presentation options failed: {e:?}");
            }
            log::info!("kiosk engaged");
        });
    }

    // Black out every other display with blanker windows.
    if let Ok(monitors) = window.available_monitors() {
        let primary = window.primary_monitor().ok().flatten();
        for (i, m) in monitors.iter().enumerate() {
            if let Some(p) = &primary {
                if p.position() == m.position() {
                    continue;
                }
            }
            let label = format!("blanker-{i}");
            if app.get_webview_window(&label).is_some() {
                continue;
            }
            let pos = m.position();
            let size = m.size();
            let scale = m.scale_factor();
            // NOTE: must be a real SvelteKit route — "index.html?x" makes the
            // SPA router resolve pathname /index.html and render its 404 page.
            let built = tauri::WebviewWindowBuilder::new(
                app,
                &label,
                tauri::WebviewUrl::App("blanker".into()),
            )
            .decorations(false)
            .resizable(false)
            .always_on_top(true)
            .visible_on_all_workspaces(true)
            .position(pos.x as f64 / scale, pos.y as f64 / scale)
            .inner_size(size.width as f64 / scale, size.height as f64 / scale)
            .build();
            match built {
                Ok(_window) => {
                    #[cfg(target_os = "macos")]
                    {
                        let w2 = _window.clone();
                        let _ = _window.run_on_main_thread(move || {
                            let _ =
                                objc2::exception::catch(std::panic::AssertUnwindSafe(|| unsafe {
                                    if let Ok(ptr) = w2.ns_window() {
                                        mac::raise_window(ptr as *mut objc2::runtime::AnyObject);
                                    }
                                }));
                        });
                    }
                }
                Err(e) => log::warn!("blanker {label} failed: {e}"),
            }
        }
    }

    // Refocus loop: snap focus back while locked.
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        let my_pid = std::process::id() as i32;
        let engaged_at = std::time::Instant::now();
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            let state = app2.state::<AppState>();
            if !state.locked.load(Ordering::SeqCst) {
                break;
            }
            // Release token: a file named principia-unlock in the home directory, the
            // temporary directory or at the root of any mounted volume.
            if let Some(token) = release_token() {
                log::warn!("release token at {}, releasing kiosk", token.display());
                release(&app2, &state);
                break;
            }
            // Dead man's switch: no lesson runs this long, so a lock still
            // standing is a stuck app. Let the machine go.
            if engaged_at.elapsed() >= MAX_LOCK {
                log::error!(
                    "kiosk held for {} minutes, releasing on the dead man's switch",
                    engaged_at.elapsed().as_secs() / 60
                );
                release(&app2, &state);
                break;
            }
            let Some(window) = app2.get_webview_window("main") else {
                continue;
            };
            // The recovery console is ours and sits above the desk on
            // purpose; leave the keyboard with it.
            if app2
                .get_webview_window(crate::recovery::WINDOW)
                .and_then(|console| console.is_focused().ok())
                .unwrap_or(false)
            {
                continue;
            }
            #[cfg(target_os = "macos")]
            {
                let win = window.clone();
                let _ = app2.run_on_main_thread(move || {
                    let _ = objc2::exception::catch(std::panic::AssertUnwindSafe(|| unsafe {
                        let front = mac::frontmost_pid();
                        let visible = win.is_visible().unwrap_or(false);
                        if front != my_pid || !visible {
                            let _ = win.show();
                            mac::activate_self();
                            if let Ok(ptr) = win.ns_window() {
                                mac::raise_window(ptr as *mut objc2::runtime::AnyObject);
                            }
                            let _ = win.set_focus();
                        }
                    }));
                });
            }
            #[cfg(not(target_os = "macos"))]
            {
                let _ = window.set_focus();
                let _ = my_pid;
            }
        }
    });
}

/// Release the kiosk lock and restore normal window behavior.
pub fn release(app: &AppHandle, state: &AppState) {
    state.locked.store(false, Ordering::SeqCst);
    restore_media(state);
    for (label, w) in app.webview_windows() {
        if label.starts_with("blanker-") {
            let _ = w.close();
        }
    }
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let _ = window.set_always_on_top(false);
    let _ = window.set_visible_on_all_workspaces(false);
    let _ = window.set_resizable(true);
    let _ = window.set_decorations(true);
    #[cfg(target_os = "macos")]
    {
        let win = window.clone();
        let _ = window.run_on_main_thread(move || unsafe {
            if let Ok(ptr) = win.ns_window() {
                mac::reset_window(ptr as *mut objc2::runtime::AnyObject);
            }
            mac::set_presentation_options(0);
        });
    }
}

/// Verify the escape phrase (constant-time-ish), enforce attempt lockout.
fn retain_recent_failures(failures: &mut Vec<i64>, now: i64) -> usize {
    failures.retain(|timestamp| now - *timestamp < 60);
    failures.len()
}

fn escape_phrase_matches(typed: &str, expected: &str) -> bool {
    let a = typed.trim().as_bytes();
    let b = expected.trim().as_bytes();
    let mut diff = a.len() ^ b.len();
    for i in 0..a.len().min(b.len()) {
        diff |= (a[i] ^ b[i]) as usize;
    }
    diff == 0
}

pub fn verify_escape(state: &AppState, typed: &str) -> Result<bool, String> {
    let now = chrono::Utc::now().timestamp();
    {
        let mut fails = state.escape_failures.lock().unwrap();
        if retain_recent_failures(&mut fails, now) >= 3 {
            return Err("too many attempts, wait 60 seconds".into());
        }
    }
    let expected = {
        let conn = state.db.0.lock().unwrap();
        crate::db::get_config(&conn, "escape_phrase")
            .ok()
            .flatten()
            .unwrap_or_default()
    };
    if expected.trim().len() < 40 {
        return Err("escape phrase is not safely configured".into());
    }
    if escape_phrase_matches(typed, &expected) {
        state.escape_failures.lock().unwrap().clear();
        Ok(true)
    } else {
        state.escape_failures.lock().unwrap().push(now);
        Ok(false)
    }
}

#[cfg(test)]
mod escape_tests {
    use super::{escape_phrase_matches, retain_recent_failures};

    #[test]
    fn phrase_comparison_requires_exact_trimmed_content() {
        assert!(escape_phrase_matches(" exact phrase ", "exact phrase"));
        assert!(!escape_phrase_matches("exact phrase!", "exact phrase"));
        assert!(!escape_phrase_matches("", "exact phrase"));
    }

    #[test]
    fn escape_rate_limit_only_counts_the_last_minute() {
        let mut failures = vec![10, 50, 89, 90];
        assert_eq!(retain_recent_failures(&mut failures, 100), 3);
        assert_eq!(failures, vec![50, 89, 90]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "principia-kiosk-{}-{}-{:x}",
            name,
            std::process::id(),
            rand::random::<u64>()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_release_token_is_found_in_any_accepted_place() {
        let home = scratch("home");
        let stick = scratch("stick");
        let roots = vec![home.clone(), stick.clone()];
        assert!(token_in(&roots).is_none());

        // A stick plugged into a locked machine frees it without a terminal.
        fs::write(stick.join(UNLOCK_TOKEN), b"").unwrap();
        assert_eq!(token_in(&roots), Some(stick.join(UNLOCK_TOKEN)));

        // The documented home file keeps working and wins when both exist.
        fs::write(home.join(UNLOCK_TOKEN), b"").unwrap();
        assert_eq!(token_in(&roots), Some(home.join(UNLOCK_TOKEN)));

        // Removing the tokens re-arms the desk.
        fs::remove_file(home.join(UNLOCK_TOKEN)).unwrap();
        fs::remove_file(stick.join(UNLOCK_TOKEN)).unwrap();
        assert!(token_in(&roots).is_none());

        // A directory of that name counts too: it is still an explicit signal.
        fs::create_dir(stick.join(UNLOCK_TOKEN)).unwrap();
        assert!(token_in(&roots).is_some());
        fs::remove_dir_all(home).unwrap();
        fs::remove_dir_all(stick).unwrap();
    }

    #[test]
    fn the_searched_places_include_the_home_directory_and_removable_media() {
        let roots = token_roots();
        assert!(roots.contains(&std::env::temp_dir()));
        if let Some(home) = home_dir() {
            assert!(roots.contains(&home), "the documented home file must work");
        }
        assert!(
            MAX_LOCK <= std::time::Duration::from_secs(6 * 3600),
            "a lock may never outlive a working day"
        );
    }
}
