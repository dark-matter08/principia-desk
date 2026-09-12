//! Lessons are prepared before they are due, and the alarm waits for them.
//!
//! Generation is slow. Ringing the alarm while the tutor is still writing
//! made the learner sit through minutes of "preparing" after they answered
//! it, and a crash mid-generation left them trapped. The desk now starts
//! preparing a lesson ahead of its appointment, or at once if the appointment
//! is already due, and the alarm rings only when the lesson is ready to open.
//! Starting a ready lesson is then immediate.

use crate::state::AppState;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

/// How long before an appointment its lesson starts preparing.
pub const LEAD_MINUTES: i64 = 20;

/// Where a due appointment's lesson stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Readiness {
    /// Nothing planned yet, or the tutor is still writing.
    Preparing,
    /// The lesson is published and can open at once.
    Ready,
    /// The last preparation failed; a start retries it.
    Failed,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LessonReadiness {
    pub readiness: Readiness,
    pub session_id: Option<String>,
    pub error: Option<String>,
    /// The learner is already in this lesson, or paused it: nothing to ring for.
    pub in_progress: bool,
}

/// The lesson that serves an appointment, with its state.
///
/// A lesson planned for the appointment is preferred. Failing that, the
/// class's current lesson counts, because a class holds one live lesson at a
/// time: one started by hand before the appointment came due is the lesson
/// the appointment will open, and preparing a second would only be refused.
pub fn for_occurrence(
    conn: &rusqlite::Connection,
    occurrence_id: &str,
    course_id: &str,
) -> LessonReadiness {
    let row: Option<(String, String)> = conn
        .query_row(
            "SELECT id, status FROM study_sessions
             WHERE json_extract(context_json,'$.selection.occurrence_id') = ?1
               AND status NOT IN ('completed','skipped')
             ORDER BY created_at DESC LIMIT 1",
            [occurrence_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok()
        .or_else(|| {
            conn.query_row(
                "SELECT id, status FROM study_sessions
                 WHERE json_extract(context_json,'$.course.course_id') = ?1
                   AND status NOT IN ('completed','skipped')
                 ORDER BY created_at DESC LIMIT 1",
                [course_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok()
        });
    let Some((session_id, status)) = row else {
        return LessonReadiness {
            readiness: Readiness::Preparing,
            session_id: None,
            error: None,
            in_progress: false,
        };
    };
    let in_progress = matches!(status.as_str(), "active" | "paused");
    let readiness = match status.as_str() {
        "ready" | "active" | "paused" => Readiness::Ready,
        _ => {
            let (job_status, error): (String, Option<String>) = conn
                .query_row(
                    "SELECT status, error FROM study_preparation_jobs WHERE session_id = ?1",
                    [&session_id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap_or(("queued".into(), None));
            if job_status == "failed" {
                return LessonReadiness {
                    readiness: Readiness::Failed,
                    session_id: Some(session_id),
                    error,
                    in_progress: false,
                };
            }
            Readiness::Preparing
        }
    };
    LessonReadiness {
        readiness,
        session_id: Some(session_id),
        error: None,
        in_progress,
    }
}

/// Appointments whose lessons should be preparing now: open ones for active
/// classes that fire within the lead time or are already due, and that have
/// no lesson planned, prepared or failed for them yet.
pub fn wanting_preparation(
    state: &AppState,
    now: chrono::DateTime<chrono::Utc>,
) -> Vec<(String, String)> {
    let conn = state.db.0.lock().unwrap();
    if matches!(crate::db::get_config(&conn, "schedule_paused"), Ok(Some(v)) if v == "1") {
        return Vec::new();
    }
    let today = state.today();
    let Ok(appointments) = crate::classroom::refresh_appointments(&conn, &today) else {
        return Vec::new();
    };
    let horizon = now + chrono::Duration::minutes(LEAD_MINUTES);
    appointments
        .into_iter()
        .filter(|a| matches!(a.disposition.as_str(), "scheduled" | "due"))
        .filter(|a| {
            chrono::DateTime::parse_from_rfc3339(&a.fires_at)
                .map(|t| t.with_timezone(&chrono::Utc) <= horizon)
                .unwrap_or(false)
        })
        .filter(|a| {
            for_occurrence(&conn, &a.id, &a.course_id)
                .session_id
                .is_none()
        })
        .map(|a| (a.id, a.course_id))
        .collect()
}

/// Plan and prepare the lesson for one appointment without opening it or
/// consuming the appointment. Runs under the class start gate, so a learner's
/// own Start waits for it rather than racing it.
pub async fn prepare(app: &AppHandle, occurrence_id: &str, course_id: &str) -> Result<(), String> {
    let state = app.state::<AppState>();
    let _gate = state.class_start_gate.lock().await;
    let spec = crate::classroom::subject(course_id)?;
    // Someone may have prepared or started it while we waited for the gate.
    {
        let conn = state.db.0.lock().unwrap();
        if for_occurrence(&conn, occurrence_id, spec.id)
            .session_id
            .is_some()
        {
            return Ok(());
        }
        let appointment =
            crate::domain::schedule::get(&conn, occurrence_id).map_err(|e| e.to_string())?;
        if appointment.consumed() {
            return Ok(());
        }
    }
    let planned = {
        let conn = state.db.0.lock().unwrap();
        let program = crate::classroom::program_row(&conn, spec.id)?;
        match spec.kind {
            crate::classroom::SubjectKind::Language => crate::subjects::language::plan(
                &conn,
                &program,
                None,
                Some(occurrence_id.to_string()),
                &state.today(),
                false,
            )?,
            crate::classroom::SubjectKind::Engineering => crate::subjects::engineering::plan(
                &conn,
                &program,
                None,
                Some(occurrence_id.to_string()),
                &state.today(),
                false,
            )?,
        }
    };
    let _ = app.emit(
        "preparation:state",
        serde_json::json!({ "phase": "started", "course_id": spec.id, "session_id": planned.id, "occurrence_id": occurrence_id }),
    );
    let outcome = {
        let _run = state.generator.feed.begin(&planned.id.0, spec.id);
        match spec.kind {
            crate::classroom::SubjectKind::Language => {
                crate::subjects::language::prepare(&state, &planned.id)
                    .await
                    .map(|_| ())
            }
            crate::classroom::SubjectKind::Engineering => {
                crate::subjects::engineering::prepare(&state, &planned.id)
                    .await
                    .map(|_| ())
            }
        }
    };
    let _ = app.emit(
        "preparation:state",
        serde_json::json!({
            "phase": if outcome.is_ok() { "ready" } else { "failed" },
            "course_id": spec.id,
            "session_id": planned.id,
            "occurrence_id": occurrence_id,
            "error": outcome.as_ref().err(),
        }),
    );
    crate::alarm::evaluate(app);
    outcome
}

/// The watcher's tick: start preparing whatever is wanted, one at a time.
pub fn tick(app: &AppHandle) {
    let state = app.state::<AppState>();
    if state.debug_day && std::env::var_os("PRINCIPIA_ALARM_IN_DEBUG").is_none() {
        return;
    }
    if state
        .preparing_ahead
        .swap(true, std::sync::atomic::Ordering::SeqCst)
    {
        return;
    }
    let wanted = wanting_preparation(&state, chrono::Utc::now());
    let Some((occurrence_id, course_id)) = wanted.into_iter().next() else {
        state
            .preparing_ahead
            .store(false, std::sync::atomic::Ordering::SeqCst);
        return;
    };
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = prepare(&handle, &occurrence_id, &course_id).await {
            log::warn!("preparing ahead for {course_id} failed: {error}");
        }
        handle
            .state::<AppState>()
            .preparing_ahead
            .store(false, std::sync::atomic::Ordering::SeqCst);
    });
}
