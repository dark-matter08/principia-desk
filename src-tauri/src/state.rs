use crate::db::Db;
use crate::generator::{ChatTurn, Generator};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

pub struct AppState {
    pub db: Db,
    pub generator: Generator,
    pub data_dir: PathBuf,
    /// Kiosk lock currently engaged.
    pub locked: AtomicBool,
    /// The study alarm is sounding. Cleared only when nothing is due.
    pub alarm_ringing: AtomicBool,
    /// The appointment the alarm last stood for, so a change re-notifies.
    pub alarm_for: Mutex<Option<String>>,
    /// Height the menu bar panel last reported for its card, in logical points.
    pub panel_height: Mutex<f64>,
    /// A lesson is being prepared ahead of its appointment right now.
    pub preparing_ahead: AtomicBool,
    /// --debug-day: shortened timer, no kiosk, ignore schedule.
    pub debug_day: bool,
    /// Failed escape attempts (rate limiting the hatch).
    pub escape_failures: Mutex<Vec<i64>>,
    /// System mute state before the lock engaged (None = not captured).
    pub prev_muted: Mutex<Option<bool>>,
    /// Webview has booted and called mark_frontend_ready. The kiosk NEVER
    /// engages before this: a dead webview has no escape hatch, and locking
    /// behind one bricks the machine at every login.
    pub frontend_ready: AtomicBool,
    /// Only one class Start request may prepare work at a time. A cancelled or
    /// failed request releases the guard; it never leaves a persisted blocker.
    pub class_start_gate: tokio::sync::Mutex<()>,
    /// Session-only, course-grounded chat threads keyed by owner key
    /// ("course:{id}" or "classroom:{id}"). Never written to the database —
    /// cleared on completion, skip, and implicitly on every app restart
    /// (this is memory-only). Namespaced keys keep independently allocated
    /// course and classroom ids from colliding.
    pub chat_threads: Mutex<HashMap<String, Vec<ChatTurn>>>,
    /// Owner of foreground enforcement for class sessions, if any.
    pub focus: crate::enforcement::Coordinator,
}

impl AppState {
    /// Drop every in-memory chat thread. Called at every session-lifecycle
    /// boundary so a new or reopened course never inherits stale Q&A.
    pub fn clear_chat_threads(&self) {
        self.chat_threads.lock().unwrap().clear();
    }

    /// Today's date, overridable for testing via PRINCIPIA_DATE=YYYY-MM-DD.
    pub fn today(&self) -> String {
        if let Ok(d) = std::env::var("PRINCIPIA_DATE") {
            if chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").is_ok() {
                return d;
            }
        }
        chrono::Local::now().format("%Y-%m-%d").to_string()
    }

    pub fn yesterday(&self) -> String {
        let t = chrono::NaiveDate::parse_from_str(&self.today(), "%Y-%m-%d")
            .unwrap_or_else(|_| chrono::Local::now().date_naive());
        t.pred_opt().unwrap_or(t).format("%Y-%m-%d").to_string()
    }

    pub fn tomorrow(&self) -> String {
        let t = chrono::NaiveDate::parse_from_str(&self.today(), "%Y-%m-%d")
            .unwrap_or_else(|_| chrono::Local::now().date_naive());
        t.succ_opt().unwrap_or(t).format("%Y-%m-%d").to_string()
    }

    pub fn course_duration_secs(&self) -> i64 {
        if self.debug_day {
            30
        } else {
            30 * 60
        }
    }
}
