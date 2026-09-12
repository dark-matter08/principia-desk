//! The study alarm.
//!
//! When an appointment comes due the desk notifies the system, rings, and
//! keeps ringing until the lesson starts. Snoozing is the only relief and it
//! re-rings; there is no dismiss. The break-glass phrase lives inside a
//! started lesson, so a learner who wants out has to start first. Two things
//! outrank the alarm, because nothing here may hold a machine hostage: a
//! release token silences it, and pausing the schedule prevents it.

use crate::state::AppState;
use serde::Serialize;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager};

/// Snooze lengths the interface offers, in minutes. Anything else is refused
/// so a "snooze" cannot quietly become a dismissal.
pub const SNOOZE_MINUTES: &[u32] = &[5, 10, 15];
/// How often the ring repeats while an alarm stands, and how often once it
/// has stood for a while. An alarm that can only be ended by starting the
/// lesson has to be impossible to tune out.
const RING_EVERY: std::time::Duration = std::time::Duration::from_secs(6);
const RING_EVERY_LATER: std::time::Duration = std::time::Duration::from_secs(3);
const ESCALATE_AFTER: std::time::Duration = std::time::Duration::from_secs(120);
/// Every this many rings the desk speaks, and every this many it comes to the
/// front, bounces the Dock icon and notifies again.
const SPEAK_EVERY: u32 = 3;
const NAG_EVERY: u32 = 5;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AlarmView {
    pub occurrence_id: String,
    pub course_id: String,
    pub label: String,
    /// Set while a snooze holds; the alarm rings again when it passes.
    pub snoozed_until: Option<String>,
    /// Other appointments due behind this one.
    pub queued: usize,
    /// Whether the lesson is ready to open. The alarm only rings when it is;
    /// while the tutor is still writing the desk shows the delay instead.
    pub readiness: crate::readiness::Readiness,
    /// Why the last preparation failed, when it did.
    pub error: Option<String>,
}

impl AlarmView {
    pub fn ringing(&self) -> bool {
        self.snoozed_until.is_none() && self.readiness == crate::readiness::Readiness::Ready
    }
}

fn snooze_key(occurrence_id: &str) -> String {
    format!("alarm_snooze:{occurrence_id}")
}

fn now() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc::now()
}

fn snoozed_until(conn: &rusqlite::Connection, occurrence_id: &str) -> Option<String> {
    let until = crate::db::get_config(conn, &snooze_key(occurrence_id)).ok()??;
    let parsed = chrono::DateTime::parse_from_rfc3339(&until).ok()?;
    (parsed > now()).then_some(until)
}

/// What the desk owes right now, snoozes applied: the first due appointment
/// that has no snooze standing, or the earliest snoozed one if all are.
pub fn current(state: &AppState) -> Option<AlarmView> {
    if state.debug_day && std::env::var_os("PRINCIPIA_ALARM_IN_DEBUG").is_none() {
        return None;
    }
    if crate::kiosk::release_token().is_some() {
        return None;
    }
    let conn = state.db.0.lock().unwrap();
    if matches!(crate::db::get_config(&conn, "schedule_paused"), Ok(Some(v)) if v == "1") {
        return None;
    }
    let today = state.today();
    let _ = crate::classroom::refresh_appointments(&conn, &today);
    let slots = crate::classroom::slot_views(&conn, &today, state.debug_day).ok()?;
    let mut due: Vec<_> = slots
        .into_iter()
        .filter(|slot| slot.owed && !slot.in_progress)
        .filter_map(|slot| slot.occurrence_id.clone().map(|id| (id, slot)))
        // A class whose lesson is already open, or paused, is being studied:
        // the appointment is served by it and there is nothing to ring for.
        .filter(|(id, slot)| {
            !crate::readiness::for_occurrence(&conn, id, &slot.subject_id).in_progress
        })
        .collect();
    if due.is_empty() {
        return None;
    }
    let queued = due.len() - 1;
    // Ring for the first appointment that is not snoozed; otherwise report the
    // snooze so the interface can say when the ringing resumes.
    let position = due
        .iter()
        .position(|(id, _)| snoozed_until(&conn, id).is_none())
        .unwrap_or(0);
    let (occurrence_id, slot) = due.swap_remove(position);
    let snoozed = snoozed_until(&conn, &occurrence_id);
    let lesson = crate::readiness::for_occurrence(&conn, &occurrence_id, &slot.subject_id);
    Some(AlarmView {
        occurrence_id,
        course_id: slot.subject_id,
        label: slot.label,
        snoozed_until: snoozed,
        queued,
        readiness: lesson.readiness,
        error: lesson.error,
    })
}

/// Hold the alarm for a fixed number of minutes. Never a dismissal: the
/// appointment stays due and the alarm returns when the snooze passes.
pub fn snooze(state: &AppState, occurrence_id: &str, minutes: u32) -> Result<String, String> {
    if !SNOOZE_MINUTES.contains(&minutes) {
        return Err(format!(
            "Snooze for {} minutes.",
            SNOOZE_MINUTES
                .iter()
                .map(|m| m.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let current = current(state).ok_or("Nothing is ringing right now.")?;
    if current.occurrence_id != occurrence_id && current.queued == 0 {
        return Err("That appointment is not the one ringing.".into());
    }
    let until = (now() + chrono::Duration::minutes(i64::from(minutes))).to_rfc3339();
    let conn = state.db.0.lock().unwrap();
    crate::db::set_config(&conn, &snooze_key(occurrence_id), &until).map_err(|e| e.to_string())?;
    Ok(until)
}

/// Forget snoozes for appointments that are no longer due, so the config
/// table does not collect a key per past appointment.
fn prune_snoozes(state: &AppState, keep: Option<&str>) {
    let conn = state.db.0.lock().unwrap();
    let Ok(mut statement) = conn.prepare("SELECT key FROM config WHERE key LIKE 'alarm_snooze:%'")
    else {
        return;
    };
    let keys: Vec<String> = statement
        .query_map([], |row| row.get(0))
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default();
    for key in keys {
        if keep.is_some_and(|id| key == snooze_key(id)) {
            continue;
        }
        let _ = conn.execute("DELETE FROM config WHERE key = ?1", [&key]);
    }
}

fn notify(app: &AppHandle, view: &AlarmView) {
    use crate::readiness::Readiness;
    use tauri_plugin_notification::NotificationExt;
    let (title, body) = match view.readiness {
        Readiness::Ready => (
            "Time to study",
            if view.queued > 0 {
                format!(
                    "Your {} lesson is ready, with {} more waiting. The alarm stops when you start.",
                    view.label, view.queued
                )
            } else {
                format!(
                    "Your {} lesson is ready. The alarm stops when you start.",
                    view.label
                )
            },
        ),
        Readiness::Preparing => (
            "Your lesson is on its way",
            format!(
                "{} is due, and your tutor is still preparing the lesson. The alarm will ring when it is ready.",
                view.label
            ),
        ),
        Readiness::Failed => (
            "Lesson preparation failed",
            format!(
                "{} is due, but the tutor could not prepare the lesson. Open the desk to retry.",
                view.label
            ),
        ),
    };
    if let Err(error) = app.notification().builder().title(title).body(body).show() {
        log::warn!("study notification failed: {error}");
    }
}

/// Play one ring through whatever the platform has. Best effort: a machine
/// with no sound still gets the notification, the panel and the nagging.
fn ring_once() {
    #[cfg(target_os = "macos")]
    {
        // Two loud bursts back to back, amplified above the sound's own level.
        for _ in 0..2 {
            let _ = std::process::Command::new("afplay")
                .args(["-v", "4", "/System/Library/Sounds/Sosumi.aiff"])
                .status();
        }
    }
    #[cfg(target_os = "linux")]
    {
        let played = std::process::Command::new("paplay")
            .arg("/usr/share/sounds/freedesktop/stereo/alarm-clock-elapsed.oga")
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !played {
            let _ = std::process::Command::new("printf").arg("\x07").status();
        }
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", "[console]::beep(880,700)"])
            .status();
    }
}

/// Say it out loud. Speech is the one sound a person cannot mistake for a
/// message ping.
fn speak(label: &str) {
    let line =
        format!("Time to study. Your {label} lesson is waiting. Start it to stop this alarm.");
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("say").arg(&line).status();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("espeak").arg(&line).status();
    }
    #[cfg(target_os = "windows")]
    {
        let script = format!(
            "Add-Type -AssemblyName System.Speech; (New-Object System.Speech.Synthesis.SpeechSynthesizer).Speak('{}')",
            line.replace('\'', "")
        );
        let _ = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", &script])
            .status();
    }
}

/// Put the desk in front of whatever the person is doing, bounce the Dock
/// icon until they look, and notify again.
fn nag(app: &AppHandle, view: &AlarmView) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        let _ = window.request_user_attention(Some(tauri::UserAttentionType::Critical));
    }
    notify(app, view);
}

/// Re-read what is due and bring the alarm, the notification, the menu bar and
/// the interface into line with it. Called by the minute watcher and after
/// every command that changes an appointment.
pub fn evaluate(app: &AppHandle) {
    let state = app.state::<AppState>();
    let view = current(&state);
    prune_snoozes(&state, view.as_ref().map(|v| v.occurrence_id.as_str()));
    let ringing_now = view.as_ref().is_some_and(AlarmView::ringing);
    let was_ringing = state.alarm_ringing.swap(ringing_now, Ordering::SeqCst);
    // The appointment and its readiness together: a lesson becoming ready,
    // or failing, is a change worth telling the learner about.
    let identity = view
        .as_ref()
        .map(|v| format!("{}:{:?}", v.occurrence_id, v.readiness));
    let changed = {
        let mut last = state.alarm_for.lock().unwrap();
        let changed = *last != identity || (ringing_now && !was_ringing);
        *last = identity;
        changed
    };
    // A delay or a failure is announced once; a ready lesson starts the ring.
    if changed && !ringing_now {
        if let Some(view) = view.as_ref().filter(|v| v.snoozed_until.is_none()) {
            notify(app, view);
        }
    }
    if ringing_now && (!was_ringing || changed) {
        if let Some(view) = &view {
            notify(app, view);
        }
        let handle = app.clone();
        let label = view.as_ref().map(|v| v.label.clone()).unwrap_or_default();
        tauri::async_runtime::spawn(async move {
            let started = std::time::Instant::now();
            let mut rings: u32 = 0;
            loop {
                let state = handle.state::<AppState>();
                if !state.alarm_ringing.load(Ordering::SeqCst) {
                    break;
                }
                rings += 1;
                tokio::task::spawn_blocking(ring_once).await.ok();
                if rings.is_multiple_of(SPEAK_EVERY) {
                    let spoken = label.clone();
                    tokio::task::spawn_blocking(move || speak(&spoken))
                        .await
                        .ok();
                }
                if rings.is_multiple_of(NAG_EVERY) {
                    if let Some(view) = current(&state) {
                        nag(&handle, &view);
                    }
                }
                let pause = if started.elapsed() > ESCALATE_AFTER {
                    RING_EVERY_LATER
                } else {
                    RING_EVERY
                };
                tokio::time::sleep(pause).await;
            }
        });
    }
    let _ = app.emit("alarm:state", &view);
    crate::tray::refresh(app, view.as_ref());
}
