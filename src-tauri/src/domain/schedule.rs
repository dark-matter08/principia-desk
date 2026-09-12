//! Durable appointments. Recurring study times (rules) produce one occurrence
//! per rule per local service date. An occurrence snapshots its rule revision,
//! intended local time and resolved instant; its disposition moves
//! `scheduled → due → started → completed/skipped`, or to `missed` once its
//! day has passed unstarted. A missed appointment can still be made up.
use crate::db::{DbError, Result};
use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};

/// Missed appointments stay visible for make-up this long.
pub const MISSED_HISTORY_DAYS: i64 = 14;

pub enum LocalInstant {
    Single(DateTime<Utc>),
    Ambiguous(DateTime<Utc>, DateTime<Utc>),
    Nonexistent,
}

/// Resolves wall-clock times in the learner's zone. Production uses the system
/// zone; tests script transitions.
pub trait ZoneResolver {
    fn resolve(&self, local: NaiveDateTime) -> LocalInstant;
    fn name(&self) -> String;
}

pub struct SystemZone;
impl ZoneResolver for SystemZone {
    fn resolve(&self, local: NaiveDateTime) -> LocalInstant {
        match chrono::Local.from_local_datetime(&local) {
            chrono::LocalResult::Single(t) => LocalInstant::Single(t.with_timezone(&Utc)),
            chrono::LocalResult::Ambiguous(a, b) => {
                LocalInstant::Ambiguous(a.with_timezone(&Utc), b.with_timezone(&Utc))
            }
            chrono::LocalResult::None => LocalInstant::Nonexistent,
        }
    }
    fn name(&self) -> String {
        chrono::Local::now().offset().to_string()
    }
}

/// A repeated wall-clock time (clocks going back) fires once, at its first
/// instant; a nonexistent time (clocks going forward) moves to the next valid
/// local minute.
pub fn resolve_local(
    zone: &dyn ZoneResolver,
    date: NaiveDate,
    time: NaiveTime,
) -> Result<DateTime<Utc>> {
    let mut candidate = date.and_time(time);
    for _ in 0..=180 {
        match zone.resolve(candidate) {
            LocalInstant::Single(instant) | LocalInstant::Ambiguous(instant, _) => {
                return Ok(instant)
            }
            LocalInstant::Nonexistent => candidate += Duration::minutes(1),
        }
    }
    Err(DbError::Invalid(
        "could not resolve a local study time in this timezone".into(),
    ))
}

/// The learner's clock: the resolver, the current instant and today's local
/// service date, so decisions and countdowns share one source.
pub struct Clock<'a> {
    pub zone: &'a dyn ZoneResolver,
    pub now: DateTime<Utc>,
    pub today: NaiveDate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Occurrence {
    pub id: String,
    pub rule_id: Option<i64>,
    pub course_id: String,
    pub rule_revision: i64,
    pub local_date: String,
    pub local_time: String,
    pub timezone: String,
    pub fires_at: String,
    pub duration_minutes: i64,
    pub disposition: String,
    pub session_ref: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub resolved_at: Option<String>,
}
impl Occurrence {
    pub fn consumed(&self) -> bool {
        matches!(
            self.disposition.as_str(),
            "started" | "completed" | "skipped"
        )
    }
    pub fn open(&self) -> bool {
        matches!(self.disposition.as_str(), "scheduled" | "due" | "missed")
    }
}

const COLUMNS: &str = "id,rule_id,course_id,rule_revision,local_date,local_time,timezone,fires_at,duration_minutes,disposition,session_ref,created_at,updated_at,resolved_at";

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Occurrence> {
    Ok(Occurrence {
        id: r.get(0)?,
        rule_id: r.get(1)?,
        course_id: r.get(2)?,
        rule_revision: r.get(3)?,
        local_date: r.get(4)?,
        local_time: r.get(5)?,
        timezone: r.get(6)?,
        fires_at: r.get(7)?,
        duration_minutes: r.get(8)?,
        disposition: r.get(9)?,
        session_ref: r.get(10)?,
        created_at: r.get(11)?,
        updated_at: r.get(12)?,
        resolved_at: r.get(13)?,
    })
}
fn stamp(now: DateTime<Utc>) -> String {
    now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
fn invalid(message: &str) -> DbError {
    DbError::Invalid(message.into())
}

pub fn get(conn: &Connection, id: &str) -> Result<Occurrence> {
    Ok(conn.query_row(
        &format!("SELECT {COLUMNS} FROM schedule_occurrences WHERE id=?1"),
        [id],
        row,
    )?)
}
pub fn today_for_rule(conn: &Connection, rule_id: i64, today: &str) -> Result<Option<Occurrence>> {
    Ok(conn
        .query_row(
            &format!(
                "SELECT {COLUMNS} FROM schedule_occurrences WHERE rule_id=?1 AND local_date=?2"
            ),
            params![rule_id, today],
            row,
        )
        .optional()?)
}
pub fn for_session(conn: &Connection, session_ref: &str) -> Result<Option<Occurrence>> {
    Ok(conn
        .query_row(
            &format!("SELECT {COLUMNS} FROM schedule_occurrences WHERE session_ref=?1"),
            [session_ref],
            row,
        )
        .optional()?)
}

/// Today's appointments plus missed ones from the recent past, oldest first.
pub fn agenda(conn: &Connection, today: &str) -> Result<Vec<Occurrence>> {
    let floor = NaiveDate::parse_from_str(today, "%Y-%m-%d")
        .map_err(|_| invalid("invalid service date"))?
        - Duration::days(MISSED_HISTORY_DAYS);
    let mut statement = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM schedule_occurrences
         WHERE local_date=?1 OR (disposition='missed' AND local_date>=?2 AND local_date<?1)
         ORDER BY local_date, fires_at, id"
    ))?;
    let rows = statement
        .query_map(params![today, floor.to_string()], row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Appointment history for one class, newest first.
pub fn history(conn: &Connection, course_id: &str, limit: i64) -> Result<Vec<Occurrence>> {
    let mut statement = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM schedule_occurrences WHERE course_id=?1
         ORDER BY local_date DESC, fires_at DESC LIMIT ?2"
    ))?;
    let rows = statement
        .query_map(params![course_id, limit], row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

struct RuleRow {
    id: i64,
    course_id: String,
    hour: u32,
    minute: u32,
    weekdays: Vec<u8>,
    revision: i64,
    session_minutes: i64,
    /// Minutes for particular weekdays; the class default covers the rest.
    durations: std::collections::BTreeMap<u8, i64>,
    /// Start times for particular weekdays; `hour:minute` covers the rest.
    starts: crate::classroom::DayStarts,
}

impl RuleRow {
    fn minutes_on(&self, weekday: u8) -> i64 {
        crate::classroom::day_minutes(&self.durations, weekday, self.session_minutes)
    }
    fn start_on(&self, weekday: u8) -> (u32, u32) {
        crate::classroom::day_start(&self.starts, weekday, self.hour, self.minute)
    }
}

fn active_rules(conn: &Connection) -> Result<Vec<RuleRow>> {
    let mut statement = conn.prepare(
        "SELECT s.id, s.subject_id, s.hour, s.minute, s.weekdays_json, s.revision, p.session_minutes, s.durations_json, s.starts_json
         FROM classroom_schedule_slots s JOIN classroom_programs p ON p.subject_id = s.subject_id
         WHERE s.enabled = 1 AND p.enabled = 1 ORDER BY s.id",
    )?;
    let rows = statement
        .query_map([], |r| {
            Ok(RuleRow {
                id: r.get(0)?,
                course_id: r.get(1)?,
                hour: r.get::<_, i64>(2)? as u32,
                minute: r.get::<_, i64>(3)? as u32,
                weekdays: serde_json::from_str(&r.get::<_, String>(4)?).unwrap_or_default(),
                revision: r.get(5)?,
                session_minutes: r.get(6)?,
                durations: r
                    .get::<_, Option<String>>(7)?
                    .and_then(|json| serde_json::from_str(&json).ok())
                    .unwrap_or_default(),
                starts: crate::classroom::parse_starts(r.get::<_, Option<String>>(8)?),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Legacy session rows still consume today's appointment for their rule.
fn legacy_consumption(
    conn: &Connection,
    rule_id: i64,
    today: &str,
) -> Result<Option<(String, String)>> {
    let engineering: Option<(i64, String)> = conn
        .query_row(
            "SELECT id, status FROM classroom_sessions WHERE slot_id=?1 AND session_date=?2 ORDER BY id DESC LIMIT 1",
            params![rule_id, today],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    if let Some((id, status)) = engineering {
        return Ok(Some((format!("classroom:{id}"), status)));
    }
    let language: Option<(i64, String)> = conn
        .query_row(
            "SELECT id, status FROM language_sessions WHERE classroom_slot_id=?1 AND session_date=?2 ORDER BY id DESC LIMIT 1",
            params![rule_id, today],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    Ok(language.map(|(id, status)| (format!("language:{id}"), status)))
}

/// Bring appointments up to date for the learner's clock: past unstarted
/// appointments become missed history, today's appointments exist once per
/// enabled rule of an active class (none while scheduling is paused), unfired
/// ones follow rule edits, and due ones are marked. Returns today's agenda.
pub fn materialize(conn: &Connection, clock: &Clock<'_>, paused: bool) -> Result<Vec<Occurrence>> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let today = clock.today.to_string();
    let now = stamp(clock.now);
    tx.execute(
        "UPDATE schedule_occurrences SET disposition='missed', resolved_at=?2, updated_at=?2
         WHERE disposition IN ('scheduled','due') AND local_date < ?1",
        params![today, now],
    )?;
    if !paused {
        let weekday = clock.today.weekday().number_from_monday() as u8;
        let rules = active_rules(&tx)?;
        for rule in &rules {
            let existing = today_for_rule(&tx, rule.id, &today)?;
            if !rule.weekdays.contains(&weekday) {
                if let Some(existing) =
                    existing.filter(|o| !o.consumed() && o.disposition != "missed")
                {
                    tx.execute(
                        "DELETE FROM schedule_occurrences WHERE id=?1",
                        [&existing.id],
                    )?;
                }
                continue;
            }
            let (hour, minute) = rule.start_on(weekday);
            let time = NaiveTime::from_hms_opt(hour, minute, 0)
                .ok_or_else(|| invalid("invalid rule time"))?;
            let fires_at = resolve_local(clock.zone, clock.today, time)?;
            let local_time = format!("{hour:02}:{minute:02}");
            match existing {
                None => {
                    let id = format!("occurrence-{:032x}", rand::random::<u128>());
                    tx.execute(
                        "INSERT INTO schedule_occurrences (id,rule_id,course_id,rule_revision,local_date,local_time,timezone,fires_at,duration_minutes,disposition,created_at,updated_at)
                         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'scheduled',?10,?10)",
                        params![id, rule.id, rule.course_id, rule.revision, today, local_time, clock.zone.name(), stamp(fires_at), rule.minutes_on(weekday), now],
                    )?;
                    if let Some((session_ref, status)) = legacy_consumption(&tx, rule.id, &today)? {
                        let disposition = match status.as_str() {
                            "in_progress" => "started",
                            "skipped" => "skipped",
                            _ => "completed",
                        };
                        tx.execute(
                            "UPDATE schedule_occurrences SET disposition=?2, session_ref=?3, resolved_at=?4, updated_at=?4 WHERE id=?1",
                            params![id, disposition, session_ref, now],
                        )?;
                    }
                }
                Some(existing)
                    if !existing.consumed() && existing.rule_revision != rule.revision =>
                {
                    // Unfired appointments follow the edited rule; consumed ones keep their snapshot.
                    tx.execute(
                        "UPDATE schedule_occurrences SET rule_revision=?2, local_time=?3, fires_at=?4, duration_minutes=?5, timezone=?6, disposition=CASE WHEN disposition='due' THEN 'scheduled' ELSE disposition END, updated_at=?7 WHERE id=?1",
                        params![existing.id, rule.revision, local_time, stamp(fires_at), rule.minutes_on(weekday), clock.zone.name(), now],
                    )?;
                }
                Some(_) => {}
            }
        }
        tx.execute(
            "UPDATE schedule_occurrences SET disposition='due', updated_at=?2
             WHERE disposition='scheduled' AND local_date=?1 AND fires_at<=?2",
            params![today, now],
        )?;
    }
    let result = agenda(&tx, &today)?;
    tx.commit()?;
    Ok(result)
}

/// Start (or make up) an appointment with a study session. Consumes it once.
pub fn claim(
    conn: &Connection,
    occurrence_id: &str,
    course_id: &str,
    session_ref: &str,
    now: DateTime<Utc>,
) -> Result<Occurrence> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let occurrence = get(&tx, occurrence_id)?;
    if occurrence.course_id != course_id {
        return Err(invalid("This appointment belongs to another class."));
    }
    if occurrence.session_ref.as_deref() == Some(session_ref) {
        return Ok(occurrence);
    }
    if occurrence.consumed() {
        return Err(invalid("This appointment was already started or resolved."));
    }
    tx.execute(
        "UPDATE schedule_occurrences SET disposition='started', session_ref=?2, resolved_at=?3, updated_at=?3 WHERE id=?1",
        params![occurrence_id, session_ref, stamp(now)],
    )?;
    let updated = get(&tx, occurrence_id)?;
    tx.commit()?;
    Ok(updated)
}

/// Record the outcome of the session that started an appointment. Idempotent;
/// a session without an appointment resolves nothing.
pub fn resolve(
    conn: &Connection,
    session_ref: &str,
    completed: bool,
    now: DateTime<Utc>,
) -> Result<()> {
    let disposition = if completed { "completed" } else { "skipped" };
    conn.execute(
        "UPDATE schedule_occurrences SET disposition=?2, resolved_at=?3, updated_at=?3
         WHERE session_ref=?1 AND disposition='started'",
        params![session_ref, disposition, stamp(now)],
    )?;
    Ok(())
}

/// A study block: an appointment longer than one lesson, filled with whole
/// topics in route order and finished with retrieval practice.
///
/// A slot up to an hour is one lesson, however deep. Past that the
/// appointment stays `started` after a lesson completes, and the desk offers
/// the next topic while at least a whole topic's worth of time remains, then
/// retrieval practice on earlier topics for anything shorter than a topic but
/// worth a pass, and resolves the appointment when neither fits. A topic
/// therefore always ends on the day it starts.
pub const LESSON_CAP_MINUTES: i64 = 60;
pub const BLOCK_TOPIC_MINUTES: i64 = 30;
pub const BLOCK_RETRIEVAL_MINUTES: i64 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockStep {
    /// Enough time for a whole topic.
    Topic,
    /// Less than a topic, more than a moment: retrieval on earlier topics.
    Retrieval,
    /// The block is spent.
    Done,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockProgress {
    pub occurrence_id: String,
    pub course_id: String,
    pub started_at: String,
    pub duration_minutes: i64,
    pub elapsed_minutes: i64,
    pub remaining_minutes: i64,
    pub lessons_completed: i64,
    pub next: BlockStep,
    /// Minutes the next step should be sized to.
    pub next_minutes: i64,
}

fn next_step(remaining: i64) -> (BlockStep, i64) {
    if remaining >= BLOCK_TOPIC_MINUTES {
        (BlockStep::Topic, remaining.min(LESSON_CAP_MINUTES))
    } else if remaining >= BLOCK_RETRIEVAL_MINUTES {
        (BlockStep::Retrieval, remaining)
    } else {
        (BlockStep::Done, 0)
    }
}

/// Where a started appointment stands, or None when it is not a block: an
/// appointment of an hour or less is one lesson and resolves with it.
pub fn block_progress(
    conn: &Connection,
    occurrence_id: &str,
    now: DateTime<Utc>,
) -> Result<Option<BlockProgress>> {
    let occurrence = get(conn, occurrence_id)?;
    if occurrence.disposition != "started" || occurrence.duration_minutes <= LESSON_CAP_MINUTES {
        return Ok(None);
    }
    let started_at = occurrence
        .resolved_at
        .clone()
        .ok_or_else(|| invalid("a started appointment records when it started"))?;
    let started = DateTime::parse_from_rfc3339(&started_at)
        .map_err(|_| invalid("appointment start time is unreadable"))?
        .with_timezone(&Utc);
    let elapsed = (now - started).num_minutes().max(0);
    let remaining = (occurrence.duration_minutes - elapsed).max(0);
    let lessons_completed: i64 = conn.query_row(
        "SELECT COUNT(*) FROM study_sessions WHERE status='completed' AND json_extract(context_json,'$.selection.occurrence_id') = ?1",
        [occurrence_id],
        |r| r.get(0),
    )?;
    let (next, next_minutes) = next_step(remaining);
    Ok(Some(BlockProgress {
        occurrence_id: occurrence.id,
        course_id: occurrence.course_id,
        started_at,
        duration_minutes: occurrence.duration_minutes,
        elapsed_minutes: elapsed,
        remaining_minutes: remaining,
        lessons_completed,
        next,
        next_minutes,
    }))
}

/// Attach the next session of a block to its appointment. Only a started
/// block whose previous session is finished may continue, and only while its
/// next step is still a topic or a retrieval pass.
pub fn continue_block(
    conn: &Connection,
    occurrence_id: &str,
    course_id: &str,
    session_ref: &str,
    now: DateTime<Utc>,
) -> Result<Occurrence> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let occurrence = get(&tx, occurrence_id)?;
    if occurrence.course_id != course_id {
        return Err(invalid("This appointment belongs to another class."));
    }
    if occurrence.session_ref.as_deref() == Some(session_ref) {
        tx.commit()?;
        return Ok(occurrence);
    }
    let Some(progress) = block_progress(&tx, occurrence_id, now)? else {
        return Err(invalid("This appointment is not a block in progress."));
    };
    if progress.next == BlockStep::Done {
        return Err(invalid("This block's time is spent."));
    }
    if let Some(previous) = occurrence
        .session_ref
        .as_deref()
        .and_then(|r| r.strip_prefix("study:"))
    {
        let open: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM study_sessions WHERE id=?1 AND status NOT IN ('completed','skipped'))",
            [previous],
            |r| r.get(0),
        )?;
        if open {
            return Err(invalid(
                "Finish the current lesson before starting the next one.",
            ));
        }
    }
    tx.execute(
        "UPDATE schedule_occurrences SET session_ref=?2, updated_at=?3 WHERE id=?1 AND disposition='started'",
        params![occurrence_id, session_ref, stamp(now)],
    )?;
    let updated = get(&tx, occurrence_id)?;
    tx.commit()?;
    Ok(updated)
}

/// A session that started or continued an appointment has ended. An ordinary
/// appointment resolves at once. A block resolves only when its time is spent
/// or it is ended on purpose; otherwise it stays started for its next step.
/// Returns whether the appointment was resolved.
pub fn finish_step(
    conn: &Connection,
    session_ref: &str,
    completed: bool,
    now: DateTime<Utc>,
) -> Result<bool> {
    let occurrence: Option<Occurrence> = conn
        .query_row(
            &format!("SELECT {COLUMNS} FROM schedule_occurrences WHERE session_ref=?1 AND disposition='started'"),
            [session_ref],
            row,
        )
        .optional()?;
    let Some(occurrence) = occurrence else {
        return Ok(false);
    };
    if let Some(progress) = block_progress(conn, &occurrence.id, now)? {
        // Count this session's own completion, which the row may not show yet
        // when this runs inside the finishing transaction.
        let done_so_far = progress.lessons_completed + i64::from(completed);
        if progress.next != BlockStep::Done {
            return Ok(false);
        }
        resolve(conn, session_ref, done_so_far > 0, now)?;
        return Ok(true);
    }
    resolve(conn, session_ref, completed, now)?;
    Ok(true)
}

/// End a block before its time is spent. Completed when any lesson finished,
/// skipped when none did.
pub fn end_block(conn: &Connection, occurrence_id: &str, now: DateTime<Utc>) -> Result<Occurrence> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let Some(progress) = block_progress(&tx, occurrence_id, now)? else {
        return Err(invalid("This appointment is not a block in progress."));
    };
    let disposition = if progress.lessons_completed > 0 {
        "completed"
    } else {
        "skipped"
    };
    tx.execute(
        "UPDATE schedule_occurrences SET disposition=?2, resolved_at=?3, updated_at=?3 WHERE id=?1 AND disposition='started'",
        params![occurrence_id, disposition, stamp(now)],
    )?;
    let updated = get(&tx, occurrence_id)?;
    tx.commit()?;
    Ok(updated)
}

/// Explicitly skip an open appointment without starting a session.
pub fn skip(conn: &Connection, occurrence_id: &str, now: DateTime<Utc>) -> Result<Occurrence> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let occurrence = get(&tx, occurrence_id)?;
    if occurrence.disposition == "skipped" {
        return Ok(occurrence);
    }
    if !occurrence.open() {
        return Err(invalid("This appointment already has a session."));
    }
    tx.execute(
        "UPDATE schedule_occurrences SET disposition='skipped', resolved_at=?2, updated_at=?2 WHERE id=?1",
        params![occurrence_id, stamp(now)],
    )?;
    let updated = get(&tx, occurrence_id)?;
    tx.commit()?;
    Ok(updated)
}

/// Move an unfired appointment to another time on its own day. Consumed and
/// missed appointments keep their history. The moved time survives
/// materialization until the rule itself is edited, when the edit wins.
pub fn reschedule(
    conn: &Connection,
    occurrence_id: &str,
    local_time: &str,
    zone: &dyn ZoneResolver,
    now: DateTime<Utc>,
) -> Result<Occurrence> {
    let time = NaiveTime::parse_from_str(local_time.trim(), "%H:%M")
        .map_err(|_| invalid("Choose a time as HH:MM."))?;
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let occurrence = get(&tx, occurrence_id)?;
    if !matches!(occurrence.disposition.as_str(), "scheduled" | "due") {
        return Err(invalid(
            "Only an appointment that has not started, been skipped or been missed can be moved.",
        ));
    }
    let date = NaiveDate::parse_from_str(&occurrence.local_date, "%Y-%m-%d")
        .map_err(|_| invalid("appointment date is unreadable"))?;
    let fires_at = resolve_local(zone, date, time)?;
    let disposition = if fires_at <= now { "due" } else { "scheduled" };
    tx.execute(
        "UPDATE schedule_occurrences SET local_time=?2, fires_at=?3, timezone=?4, disposition=?5, updated_at=?6 WHERE id=?1",
        params![
            occurrence_id,
            time.format("%H:%M").to_string(),
            stamp(fires_at),
            zone.name(),
            disposition,
            stamp(now)
        ],
    )?;
    let updated = get(&tx, occurrence_id)?;
    tx.commit()?;
    Ok(updated)
}

/// Today's unfired appointment times, so a moved appointment can also wake the
/// app at the operating-system level.
pub fn unfired_times_today(conn: &Connection, today: &str) -> Result<Vec<(u32, u32)>> {
    let mut statement = conn.prepare(
        "SELECT DISTINCT local_time FROM schedule_occurrences WHERE local_date = ?1 AND disposition IN ('scheduled','due')",
    )?;
    let rows = statement.query_map([today], |r| r.get::<_, String>(0))?;
    let mut out = Vec::new();
    for row in rows {
        let value = row?;
        if let Ok(time) = NaiveTime::parse_from_str(&value, "%H:%M") {
            use chrono::Timelike;
            out.push((time.hour(), time.minute()));
        }
    }
    Ok(out)
}

/// Unfired appointments of a rule being deleted disappear; consumed and missed
/// ones remain as history without the rule link.
pub fn detach_rule(conn: &Connection, rule_id: i64) -> Result<()> {
    conn.execute(
        "DELETE FROM schedule_occurrences WHERE rule_id=?1 AND disposition IN ('scheduled','due')",
        [rule_id],
    )?;
    Ok(())
}
