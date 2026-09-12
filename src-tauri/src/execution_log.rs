//! The execution log: every line the tutor's runner reports, kept per run.
//!
//! The live feed used to vanish with the banner that showed it. A failed
//! preparation is only debuggable from what the runner said, so lines are
//! stored as they arrive, grouped by the session being prepared, and the desk
//! has a page to read them back.

use crate::db::Result;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::sync::{Arc, Mutex};

/// A line as the runner said it, stamped with the run it belonged to at that
/// moment. Stamping happens on the sending side: a line that waits in the
/// channel while the run ends must still land in that run.
#[derive(Debug, Clone)]
pub struct Reported {
    /// (session id, course id) of the preparation under way, if any.
    pub run: Option<(String, String)>,
    pub line: String,
}

/// The runner's side of the live feed. Names the run in progress and reports
/// lines under it. Cloning shares the run, so the generator and its runner
/// see the same name.
#[derive(Clone, Default)]
pub struct Feed {
    tx: Option<tokio::sync::broadcast::Sender<Reported>>,
    run: Arc<Mutex<Option<(String, String)>>>,
}

/// Names the run for as long as it lives; dropping it ends the run, so a
/// preparation that fails or is cancelled part way never leaves its name on
/// lines said later.
pub struct RunGuard {
    run: Arc<Mutex<Option<(String, String)>>>,
}

impl Drop for RunGuard {
    fn drop(&mut self) {
        *self.run.lock().unwrap() = None;
    }
}

impl Feed {
    pub fn new(tx: tokio::sync::broadcast::Sender<Reported>) -> Self {
        Self {
            tx: Some(tx),
            run: Arc::default(),
        }
    }

    /// Report one line under the run in progress.
    pub fn say(&self, line: impl Into<String>) {
        if let Some(tx) = &self.tx {
            let run = self.run.lock().unwrap().clone();
            let _ = tx.send(Reported {
                run,
                line: line.into(),
            });
        }
    }

    /// Name the run whose lines follow, until the guard drops.
    pub fn begin(&self, session_id: &str, course_id: &str) -> RunGuard {
        *self.run.lock().unwrap() = Some((session_id.to_string(), course_id.to_string()));
        RunGuard {
            run: Arc::clone(&self.run),
        }
    }

    /// The run in progress, if any.
    pub fn current(&self) -> Option<(String, String)> {
        self.run.lock().unwrap().clone()
    }
}

/// Lines older than this are pruned at startup.
const KEEP_DAYS: i64 = 30;
/// A single run keeps at most this many lines; older ones in the run go first.
const LINES_PER_RUN: i64 = 4_000;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LogLine {
    pub id: i64,
    pub at: String,
    pub run_id: Option<String>,
    pub course_id: Option<String>,
    pub line: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RunSummary {
    pub run_id: String,
    pub course_id: Option<String>,
    pub label: String,
    /// What the run was doing: `lesson` for a session preparation, or the
    /// class builder's `draft`, `review`, `sources`, `bank` or `fix`.
    pub activity: String,
    pub started_at: String,
    pub last_at: String,
    pub lines: i64,
    /// `running`, `ready`, `failed` or `unknown` from the preparation job
    /// when the run was a session preparation; for the class builder's runs,
    /// `running` while the feed names the run, then `done` or `failed` from
    /// how its last line reads.
    pub outcome: String,
    pub error: Option<String>,
}

/// What a class builder run's last line says about how it ended.
fn ended(last_line: &str) -> &'static str {
    let line = last_line.to_lowercase();
    if line.contains("failed") || line.contains("could not") || line.contains("nothing was applied")
    {
        "failed"
    } else if line.contains("drafted")
        || line.contains("finding(s)")
        || line.contains("checked")
        || line.contains("written")
        || line.contains("applied")
        || line.contains("issue(s) remain")
    {
        "done"
    } else {
        "unknown"
    }
}

pub fn append(
    conn: &Connection,
    run: Option<(&str, &str)>,
    line: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<()> {
    let (run_id, course_id) = match run {
        Some((run, course)) => (Some(run), Some(course)),
        None => (None, None),
    };
    conn.execute(
        "INSERT INTO execution_log_lines (at, run_id, course_id, line) VALUES (?1, ?2, ?3, ?4)",
        params![now.to_rfc3339(), run_id, course_id, line],
    )?;
    if let Some(run) = run_id {
        conn.execute(
            "DELETE FROM execution_log_lines WHERE run_id = ?1 AND id NOT IN (
                SELECT id FROM execution_log_lines WHERE run_id = ?1 ORDER BY id DESC LIMIT ?2)",
            params![run, LINES_PER_RUN],
        )?;
    }
    Ok(())
}

/// Forget lines older than the retention window.
pub fn prune(conn: &Connection, now: chrono::DateTime<chrono::Utc>) -> Result<usize> {
    let cutoff = (now - chrono::Duration::days(KEEP_DAYS)).to_rfc3339();
    Ok(conn.execute("DELETE FROM execution_log_lines WHERE at < ?1", [cutoff])?)
}

/// Runs, newest first. `current` is the run the feed names right now, so a
/// class builder run in flight reads as running.
pub fn runs(conn: &Connection, limit: i64, current: Option<&str>) -> Result<Vec<RunSummary>> {
    let mut statement = conn.prepare(
        "SELECT l.run_id, l.course_id, MIN(l.at), MAX(l.at), COUNT(*),
                j.status, j.error,
                COALESCE(p.label, json_extract(c.draft_json, '$.label'), l.course_id, 'desk'),
                (SELECT line FROM execution_log_lines z WHERE z.run_id = l.run_id ORDER BY z.id DESC LIMIT 1)
         FROM execution_log_lines l
         LEFT JOIN study_preparation_jobs j ON j.session_id = l.run_id
         LEFT JOIN classroom_programs p ON p.subject_id = l.course_id
         LEFT JOIN custom_courses c ON c.id = l.course_id
         WHERE l.run_id IS NOT NULL
         GROUP BY l.run_id
         ORDER BY MAX(l.at) DESC
         LIMIT ?1",
    )?;
    let rows = statement
        .query_map([limit], |r| {
            let run_id: String = r.get(0)?;
            let job: Option<String> = r.get(5)?;
            let last_line: Option<String> = r.get(8)?;
            let activity = run_id
                .split_once(':')
                .map(|(kind, _)| kind)
                .filter(|kind| matches!(*kind, "draft" | "review" | "sources" | "bank" | "fix"))
                .unwrap_or("lesson")
                .to_string();
            let outcome = match job {
                Some(status) => status,
                None if current == Some(run_id.as_str()) => "running".into(),
                None if activity != "lesson" => {
                    ended(last_line.as_deref().unwrap_or_default()).into()
                }
                None => "unknown".into(),
            };
            Ok(RunSummary {
                run_id,
                course_id: r.get(1)?,
                started_at: r.get(2)?,
                last_at: r.get(3)?,
                lines: r.get(4)?,
                outcome,
                error: r.get(6)?,
                label: r.get(7)?,
                activity,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Every line of one run, oldest first.
pub fn lines(conn: &Connection, run_id: &str) -> Result<Vec<LogLine>> {
    let mut statement = conn.prepare(
        "SELECT id, at, run_id, course_id, line FROM execution_log_lines WHERE run_id = ?1 ORDER BY id",
    )?;
    let rows = statement
        .query_map([run_id], |r| {
            Ok(LogLine {
                id: r.get(0)?,
                at: r.get(1)?,
                run_id: r.get(2)?,
                course_id: r.get(3)?,
                line: r.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// The most recent lines regardless of run, oldest first, for a live tail.
pub fn recent(conn: &Connection, limit: i64) -> Result<Vec<LogLine>> {
    let mut statement = conn.prepare(
        "SELECT id, at, run_id, course_id, line FROM (
            SELECT id, at, run_id, course_id, line FROM execution_log_lines ORDER BY id DESC LIMIT ?1
         ) ORDER BY id",
    )?;
    let rows = statement
        .query_map([limit], |r| {
            Ok(LogLine {
                id: r.get(0)?,
                at: r.get(1)?,
                run_id: r.get(2)?,
                course_id: r.get(3)?,
                line: r.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::Feed;

    #[test]
    fn a_line_keeps_the_run_it_was_said_under_however_late_it_is_read() {
        let (tx, mut rx) = tokio::sync::broadcast::channel(8);
        let feed = Feed::new(tx);
        let runner = feed.clone();
        {
            let _run = feed.begin("study-1", "typescript");
            runner.say("Claude Code · drafting");
            runner.say("Claude Code · finished in 74.3s");
        }
        feed.say("answering a course-chat question");
        assert_eq!(feed.current(), None, "the guard ended the run");

        // The drain reads only now, after the run ended: the stamps hold.
        let first = rx.try_recv().unwrap();
        assert_eq!(
            first.run,
            Some(("study-1".to_string(), "typescript".to_string()))
        );
        assert_eq!(first.line, "Claude Code · drafting");
        assert!(rx.try_recv().unwrap().run.is_some());
        let idle = rx.try_recv().unwrap();
        assert_eq!(idle.run, None);
        assert_eq!(idle.line, "answering a course-chat question");
    }

    #[test]
    fn a_silent_feed_reports_nothing_and_still_names_runs() {
        let feed = Feed::default();
        let _run = feed.begin("study-2", "rust");
        feed.say("dropped on the floor");
        assert_eq!(
            feed.current(),
            Some(("study-2".to_string(), "rust".to_string()))
        );
    }
}
