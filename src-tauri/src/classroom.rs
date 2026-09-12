use crate::generator::{Exercise, GeneratedCourse, GenerationProfile, Resource};
use crate::{db, language, mastery, selection, state::AppState};
use chrono::{Datelike, Duration, Local, NaiveDateTime, Timelike, Weekday};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

type Result<T> = std::result::Result<T, String>;

pub use crate::catalog::{CourseDefinition as SubjectSpec, SubjectKind};

/// Every subject the desk teaches: the bundled courses and the learner's own.
pub fn subjects() -> Vec<&'static SubjectSpec> {
    crate::catalog::all()
}

pub fn subject(subject_id: &str) -> Result<&'static SubjectSpec> {
    crate::catalog::course(subject_id)
        .ok_or_else(|| format!("unknown classroom subject: {subject_id}"))
}

/// A program row for a subject, with the tutor to start with; an existing
/// row keeps its preferences and takes the catalog's metadata.
pub fn ensure_program(
    conn: &Connection,
    spec: &SubjectSpec,
    agent: &str,
    model: &str,
    custom_bin: &str,
) -> Result<()> {
    let (enabled, minutes) = if spec.kind == SubjectKind::Language {
        conn.query_row(
            "SELECT enabled, session_minutes FROM language_programs WHERE language = ?1",
            [spec.id],
            |row| Ok((row.get::<_, i64>(0)? != 0, row.get::<_, i64>(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .unwrap_or((false, 30))
    } else {
        (false, 30)
    };
    conn.execute(
        "INSERT OR IGNORE INTO classroom_programs
            (subject_id, kind, label, native_label, short_code, enabled, agent,
             model, custom_agent_bin, prompt_profile, prompt_version,
             session_minutes, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?13, ?11, ?12)",
        params![
            spec.id,
            spec.kind.as_str(),
            spec.label,
            spec.native_label,
            spec.short_code,
            i64::from(enabled),
            agent,
            model,
            custom_bin,
            spec.prompt_profile,
            minutes,
            language::now_iso(),
            spec.version,
        ],
    )
    .map_err(|error| error.to_string())?;
    // Refresh catalog metadata while retaining enrollment preferences.
    conn.execute(
        "UPDATE classroom_programs SET label = ?2, native_label = ?3,
        short_code = ?4, prompt_profile = ?5, prompt_version = ?6 WHERE subject_id = ?1",
        params![
            spec.id,
            spec.label,
            spec.native_label,
            spec.short_code,
            spec.prompt_profile,
            spec.version
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn initialize(conn: &Connection) -> Result<()> {
    crate::catalog::validate()?;
    // The learner's own courses join the catalog before programs are read.
    crate::domain::custom::load_published(conn).map_err(|error| error.to_string())?;
    let global_agent = db::get_config(conn, "agent")
        .map_err(|error| error.to_string())?
        .unwrap_or_else(|| "claude".into());
    let global_model = db::get_config(conn, "model")
        .map_err(|error| error.to_string())?
        .unwrap_or_else(|| "opus".into());
    let global_custom = db::get_config(conn, "custom_agent_bin")
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    for spec in subjects() {
        ensure_program(conn, spec, &global_agent, &global_model, &global_custom)?;
    }
    migrate_language_slots(conn)?;
    Ok(())
}

fn migrate_language_slots(conn: &Connection) -> Result<()> {
    let key = "migration:classroom_language_slots:v1";
    if db::get_config(conn, key)
        .map_err(|error| error.to_string())?
        .is_some()
    {
        return Ok(());
    }
    let transaction = conn
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
    let conn = &*transaction;
    conn.execute(
        "INSERT OR IGNORE INTO classroom_schedule_slots
            (subject_id, hour, minute, weekdays_json, enabled, created_at)
         SELECT language, hour, minute, weekdays_json, enabled, created_at
         FROM language_schedule_slots",
        [],
    )
    .map_err(|error| error.to_string())?;
    conn.execute(
        "UPDATE language_sessions
         SET classroom_slot_id = (
             SELECT c.id
             FROM language_schedule_slots old
             JOIN classroom_schedule_slots c
               ON c.subject_id = old.language
              AND c.hour = old.hour
              AND c.minute = old.minute
             WHERE old.id = language_sessions.slot_id
         )
         WHERE classroom_slot_id IS NULL AND slot_id IS NOT NULL",
        [],
    )
    .map_err(|error| error.to_string())?;
    db::set_config(conn, key, "1").map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct ProgramRow {
    pub subject_id: String,
    pub kind: String,
    pub label: String,
    pub native_label: String,
    pub short_code: String,
    pub enabled: bool,
    pub agent: String,
    pub model: String,
    pub custom_agent_bin: String,
    pub prompt_profile: String,
    pub prompt_version: String,
    pub session_minutes: i64,
    pub learning_goal: String,
    pub target_weekly_minutes: i64,
}

pub fn program_row(conn: &Connection, subject_id: &str) -> Result<ProgramRow> {
    conn.query_row(
        "SELECT subject_id, kind, label, native_label, short_code, enabled,
                agent, model, custom_agent_bin, prompt_profile, prompt_version,
                session_minutes, learning_goal, target_weekly_minutes
         FROM classroom_programs WHERE subject_id = ?1",
        [subject_id],
        |row| {
            Ok(ProgramRow {
                subject_id: row.get(0)?,
                kind: row.get(1)?,
                label: row.get(2)?,
                native_label: row.get(3)?,
                short_code: row.get(4)?,
                enabled: row.get::<_, i64>(5)? != 0,
                agent: row.get(6)?,
                model: row.get(7)?,
                custom_agent_bin: row.get(8)?,
                prompt_profile: row.get(9)?,
                prompt_version: row.get(10)?,
                session_minutes: row.get(11)?,
                learning_goal: row.get(12)?,
                target_weekly_minutes: row.get(13)?,
            })
        },
    )
    .map_err(|error| error.to_string())
}

#[derive(Debug, Clone, Serialize)]
pub struct ClassroomProgramView {
    pub subject_id: String,
    pub kind: String,
    pub label: String,
    pub native_label: String,
    pub short_code: String,
    pub enabled: bool,
    pub agent: String,
    pub model: String,
    pub custom_agent_bin: String,
    pub prompt_profile: String,
    pub prompt_version: String,
    pub session_minutes: i64,
    pub learning_goal: String,
    pub target_weekly_minutes: i64,
    pub progress: f64,
    pub progress_label: String,
    /// Every module in the track (or the CEFR goal level) is finished.
    /// Completed subjects never start a fresh lesson automatically; only an
    /// explicit revisit is served.
    pub completed: bool,
    pub language_progress: Option<language::LanguageProgramView>,
    pub accepted_path: Option<crate::domain::classes::PathSummary>,
    /// Enforcement for future sessions: advisory, focused or strict.
    pub focus_policy: String,
    /// What the accepted route says about completion, demonstrated knowledge,
    /// review and what comes next. Engineering classes with a path only.
    pub route: Option<RouteSummary>,
    /// Topics whose spaced review is due today (engineering classes).
    pub review_due: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct NextTopic {
    pub slug: String,
    pub title: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RouteSummary {
    pub revision: u32,
    pub entry_label: String,
    pub required_total: usize,
    pub required_done: usize,
    pub coverage_total: usize,
    pub coverage_done: usize,
    /// Topics checked out with evidence plus placement samples demonstrated.
    pub demonstrated: usize,
    /// Refreshers, accepted bridges and topics whose mastery decayed or struggles.
    pub needs_review: usize,
    pub next: Option<NextTopic>,
    /// The curriculum changed since this path was accepted: review it before the next lesson.
    pub stale: bool,
}

/// The five answers a class overview owes: goal, completed, demonstrated,
/// review and next. Denominators come from the curriculum map.
fn route_summary(conn: &Connection, subject_id: &str, kind: &str) -> Result<Option<RouteSummary>> {
    if kind != "engineering" {
        return Ok(None);
    }
    let Some(path) =
        crate::domain::classes::current_path(conn, subject_id).map_err(|e| e.to_string())?
    else {
        return Ok(None);
    };
    let map = curriculum_map(conn, subject_id)?;
    let Some(coverage) = map.path else {
        return Ok(None);
    };
    let plan = &path.recommendation;
    // A changed curriculum must not take the whole app state down: the route
    // reports itself stale and the overview points at the starting-point review.
    let (peeked, stale) = match crate::domain::classes::peek_next_concept(conn, subject_id) {
        Ok(next) => (next, false),
        Err(crate::db::DbError::Invalid(message)) if message.contains("curriculum changed") => {
            (None, true)
        }
        Err(error) => return Err(error.to_string()),
    };
    let next = peeked.map(|(concept, reason)| NextTopic {
        reason: match reason {
            crate::domain::classes::NextReason::Bridge => plan
                .bridges
                .iter()
                .find(|t| t.id == concept.slug)
                .map(|t| format!("Bridge lesson: {}", t.reason))
                .unwrap_or_else(|| "Bridge lesson before dependent work.".into()),
            crate::domain::classes::NextReason::Route => {
                "Next required topic on your accepted route.".into()
            }
        },
        slug: concept.slug,
        title: concept.title,
    });
    Ok(Some(RouteSummary {
        revision: coverage.revision,
        entry_label: coverage.entry_label,
        required_total: coverage.required_total,
        required_done: coverage.required_done,
        coverage_total: coverage.coverage_total,
        coverage_done: coverage.coverage_done,
        demonstrated: coverage.checked
            + plan
                .criteria
                .iter()
                .filter(|row| row.verdict == crate::domain::placement::Verdict::Passed)
                .count(),
        needs_review: coverage.refreshers
            + coverage.bridges
            + map
                .concepts
                .iter()
                .filter(|c| matches!(c.mastery_state.as_str(), "decayed" | "struggling"))
                .count(),
        next,
        stale,
    }))
}

pub fn program_views(conn: &Connection, today: &str) -> Result<Vec<ClassroomProgramView>> {
    // Only subjects this profile has a program for: a custom course
    // registered in the process but published from another profile (as
    // happens in tests) is not this desk's class.
    let mut statement = conn
        .prepare("SELECT subject_id FROM classroom_programs")
        .map_err(|error| error.to_string())?;
    let known: std::collections::HashSet<String> = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .collect::<std::result::Result<_, _>>()
        .map_err(|error| error.to_string())?;
    subjects()
        .into_iter()
        .filter(|spec| known.contains(spec.id))
        .map(|spec| program_view(conn, spec.id, today))
        .collect()
}

pub fn program_view(
    conn: &Connection,
    subject_id: &str,
    today: &str,
) -> Result<ClassroomProgramView> {
    let row = program_row(conn, subject_id)?;
    let kind_for_route = row.kind.clone();
    let language_progress = if row.kind == "language" {
        Some(language::program_view(conn, subject_id, today)?)
    } else {
        None
    };
    let (progress, progress_label, completed) = if let Some(language) = &language_progress {
        let level_done =
            crate::language::level_complete(conn, subject_id, &language.current_level)?;
        let goal_met =
            language.current_level == language.target_level || language.current_level == "B2";
        (
            language.progress,
            format!(
                "{} / {} evidence steps in {}",
                language.completed_steps, language.required_steps, language.current_level
            ),
            level_done && goal_met,
        )
    } else {
        let (total, covered, done): (i64, i64, i64) = conn
            .query_row(
                "SELECT COUNT(*),
                        COALESCE(SUM(CASE WHEN COALESCE(m.state, 'unseen') != 'unseen' THEN 1 ELSE 0 END), 0),
                        COALESCE(SUM(CASE WHEN m.state IN ('mastered','maintenance') THEN 1 ELSE 0 END), 0)
                 FROM concepts c
                 LEFT JOIN mastery m ON m.concept_id = c.id
                 WHERE c.active = 1 AND c.focus = ?1",
                [subject_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(|error| error.to_string())?;
        (
            if total == 0 {
                0.0
            } else {
                covered as f64 / total as f64
            },
            format!("{covered} / {total} concepts practiced"),
            total > 0 && done == total,
        )
    };
    Ok(ClassroomProgramView {
        subject_id: row.subject_id,
        kind: row.kind,
        label: row.label,
        native_label: row.native_label,
        short_code: row.short_code,
        enabled: row.enabled && has_enabled_schedule(conn, subject_id)?,
        agent: row.agent,
        model: row.model,
        custom_agent_bin: row.custom_agent_bin,
        prompt_profile: row.prompt_profile,
        prompt_version: row.prompt_version,
        session_minutes: row.session_minutes,
        learning_goal: row.learning_goal,
        target_weekly_minutes: row.target_weekly_minutes,
        progress,
        progress_label,
        completed,
        language_progress,
        accepted_path: crate::domain::classes::current_path(conn, subject_id)
            .map_err(|e| e.to_string())?
            .map(|path| path.summary()),
        focus_policy: crate::domain::classes::current_configuration(conn, subject_id)
            .map_err(|e| e.to_string())?
            .map(|config| {
                serde_json::to_value(config.focus_policy)
                    .ok()
                    .and_then(|value| value.as_str().map(String::from))
                    .unwrap_or_else(|| "advisory".into())
            })
            .unwrap_or_else(|| "advisory".into()),
        route: route_summary(conn, subject_id, &kind_for_route)?,
        review_due: if kind_for_route == "engineering" {
            crate::subjects::engineering::review_due(conn, subject_id, today)?.len()
        } else {
            0
        },
    })
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConfigureClassroomInput {
    pub subject_id: String,
    pub enabled: bool,
    pub agent: String,
    pub model: String,
    #[serde(default)]
    pub custom_agent_bin: String,
    pub session_minutes: i64,
    #[serde(default)]
    pub start_level: Option<String>,
    #[serde(default)]
    pub target_level: Option<String>,
    #[serde(default)]
    pub weekly_minutes: Option<i64>,
}

fn valid_agent(agent: &str) -> bool {
    crate::agents::RunnerId::parse(agent).is_some()
}

fn valid_model(model: &str) -> bool {
    crate::agents::valid_model(model)
}

/// Activation requires a persisted class appointment, including in native IPC.
pub fn has_enabled_schedule(conn: &Connection, subject_id: &str) -> Result<bool> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM classroom_schedule_slots WHERE subject_id=?1 AND enabled=1)",
        [subject_id],
        |row| row.get(0),
    )
    .map_err(|error| error.to_string())
}

pub fn configure_program(
    conn: &Connection,
    input: &ConfigureClassroomInput,
    today: &str,
) -> Result<()> {
    let transaction =
        rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
    let spec = subject(&input.subject_id)?;
    if input.enabled && !has_enabled_schedule(conn, spec.id)? {
        return Err("Add a study time in this class's Schedule tab before activating it.".into());
    }
    if !valid_agent(&input.agent) || !valid_model(&input.model) {
        return Err("class agent or model is invalid".into());
    }
    if input.agent == "custom" {
        if input.custom_agent_bin.trim().is_empty() {
            return Err("custom class agent needs a binary command".into());
        }
        let words = crate::agents::process::command_words(&input.custom_agent_bin)
            .map_err(|e| e.to_string())?;
        if crate::agents::process::resolve(&words[0]).is_none() {
            return Err(format!("custom class agent binary not found: {}", words[0]));
        }
    }
    if !DAY_MINUTES.contains(&input.session_minutes) {
        return Err(format!(
            "class session length must be between {} and {} minutes",
            DAY_MINUTES.start(),
            DAY_MINUTES.end()
        ));
    }
    if !input.enabled && has_active_session(conn, &input.subject_id)? {
        return Err("finish the active class session before disabling it".into());
    }
    let current = program_row(conn, spec.id)?;
    if input.enabled && (!current.enabled || input.session_minutes > current.session_minutes) {
        let conflicts = activation_conflicts(conn, spec.id, input.session_minutes)?;
        if !conflicts.is_empty() {
            return Err(conflict_message(&conflicts));
        }
    }
    if spec.kind == SubjectKind::Engineering
        && (input.start_level.is_some()
            || input.target_level.is_some()
            || input.weekly_minutes.is_some())
    {
        return Err("language-only settings cannot be applied to an engineering subject".into());
    }
    conn.execute(
        "UPDATE classroom_programs
         SET enabled = ?2, agent = ?3, model = ?4, custom_agent_bin = ?5,
             session_minutes = ?6, updated_at = ?7
         WHERE subject_id = ?1",
        params![
            input.subject_id,
            i64::from(input.enabled),
            input.agent,
            input.model,
            input.custom_agent_bin.trim(),
            input.session_minutes,
            language::now_iso(),
        ],
    )
    .map_err(|error| error.to_string())?;
    if spec.kind == SubjectKind::Language {
        let existing = language::program_view(conn, spec.id, today)?;
        language::configure_program(
            conn,
            &language::ConfigureProgramInput {
                language: spec.id.into(),
                enabled: input.enabled,
                start_level: input.start_level.clone().unwrap_or(existing.start_level),
                target_level: input.target_level.clone().unwrap_or(existing.target_level),
                weekly_minutes: input.weekly_minutes.unwrap_or(existing.weekly_minutes),
                session_minutes: input.session_minutes,
            },
            today,
        )?;
    }
    // With study times saved, the session length is theirs to set.
    follow_schedule_length(conn, spec.id)?;
    crate::domain::classes::sync_configuration(conn, spec.id, today).map_err(|e| e.to_string())?;
    transaction.commit().map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpsertClassroomSlotInput {
    pub id: Option<i64>,
    pub subject_id: String,
    pub hour: u32,
    pub minute: u32,
    pub weekdays: Vec<u8>,
    pub enabled: bool,
    /// Minutes for particular weekdays. A day not listed takes the class's
    /// session length as it stands when the rule is saved, so an older
    /// caller changes nothing.
    #[serde(default)]
    pub durations: std::collections::BTreeMap<u8, i64>,
    /// "HH:MM" for particular weekdays. A day not listed starts at the
    /// rule's own hour and minute.
    #[serde(default)]
    pub starts: std::collections::BTreeMap<u8, String>,
}

/// Shortest and longest a single day's study time may be, in minutes.
pub const DAY_MINUTES: std::ops::RangeInclusive<i64> = 10..=480;

/// The minutes a rule lasts on `weekday`: its own figure for that day if it
/// has one, otherwise the class default.
pub fn day_minutes(
    durations: &std::collections::BTreeMap<u8, i64>,
    weekday: u8,
    default: i64,
) -> i64 {
    durations.get(&weekday).copied().unwrap_or(default)
}

fn parse_durations(raw: Option<String>) -> std::collections::BTreeMap<u8, i64> {
    raw.and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}

/// When a rule starts on particular weekdays, as (hour, minute).
pub type DayStarts = std::collections::BTreeMap<u8, (u32, u32)>;

/// The time a rule starts on `weekday`: its own time for that day if it has
/// one, otherwise the rule's hour and minute.
pub fn day_start(starts: &DayStarts, weekday: u8, hour: u32, minute: u32) -> (u32, u32) {
    starts.get(&weekday).copied().unwrap_or((hour, minute))
}

/// "HH:MM" as (hour, minute), if it is a time of day.
pub fn parse_clock(value: &str) -> Option<(u32, u32)> {
    let (hour, minute) = value.trim().split_once(':')?;
    let hour: u32 = hour.parse().ok()?;
    let minute: u32 = minute.parse().ok()?;
    (hour <= 23 && minute <= 59).then_some((hour, minute))
}

pub fn parse_starts(raw: Option<String>) -> DayStarts {
    raw.and_then(|json| serde_json::from_str::<std::collections::BTreeMap<u8, String>>(&json).ok())
        .map(|map| {
            map.into_iter()
                .filter_map(|(day, time)| parse_clock(&time).map(|at| (day, at)))
                .collect()
        })
        .unwrap_or_default()
}

fn clock_text(starts: &DayStarts) -> std::collections::BTreeMap<u8, String> {
    starts
        .iter()
        .map(|(day, (hour, minute))| (*day, format!("{hour:02}:{minute:02}")))
        .collect()
}

/// Minutes in one recurring week. Study times occupy a circular interval, so a
/// late Sunday session can overlap an early Monday appointment.
const WEEK_MINUTES: i64 = 7 * 24 * 60;
const WEEKDAY_NAMES: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

/// One proposed appointment that overlaps an existing enabled study time.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ScheduleConflict {
    pub weekday: u8,
    pub hour: u32,
    pub minute: u32,
    pub subject_id: String,
    pub label: String,
    pub with_slot_id: i64,
    pub with_subject_id: String,
    pub with_label: String,
    pub with_weekday: u8,
    pub with_hour: u32,
    pub with_minute: u32,
    pub with_session_minutes: i64,
}

/// A recurring study time to validate before it is saved or activated.
#[derive(Debug, Clone)]
pub struct ScheduleCandidate {
    pub subject_id: String,
    pub hour: u32,
    pub minute: u32,
    pub weekdays: Vec<u8>,
    pub session_minutes: i64,
    /// Minutes for particular weekdays; others use `session_minutes`.
    pub durations: std::collections::BTreeMap<u8, i64>,
    /// Start times for particular weekdays; others use `hour` and `minute`.
    pub starts: DayStarts,
}

#[derive(Debug, Clone, Default)]
pub struct ConflictScope {
    /// Rules being edited or replaced never conflict with themselves.
    pub excluded_slot_ids: Vec<i64>,
    /// Planning replaces this class's planned rules; ignore them.
    pub exclude_planned_for: Option<String>,
}

struct Occupancy {
    slot_id: i64,
    subject_id: String,
    label: String,
    weekday: u8,
    hour: u32,
    minute: u32,
    session_minutes: i64,
    interval: (i64, i64),
}

fn week_interval(weekday: u8, hour: u32, minute: u32, session_minutes: i64) -> (i64, i64) {
    let start = (i64::from(weekday) - 1) * 1440 + i64::from(hour) * 60 + i64::from(minute);
    (start, start + session_minutes.max(1))
}

/// Half-open intervals on a circular week: compare against the neighbouring
/// week copies so wraparound past Sunday midnight is detected.
fn week_intervals_overlap(a: (i64, i64), b: (i64, i64)) -> bool {
    [-WEEK_MINUTES, 0, WEEK_MINUTES]
        .into_iter()
        .any(|shift| a.0 < b.1 + shift && b.0 + shift < a.1)
}

/// Enabled study times that would fire alongside `subject_id`'s appointments:
/// every active class plus this class's own other times.
fn occupancies(
    conn: &Connection,
    subject_id: &str,
    scope: &ConflictScope,
) -> Result<Vec<Occupancy>> {
    let mut stmt = conn
        .prepare(
            "SELECT s.id, s.subject_id, p.label, s.hour, s.minute, s.weekdays_json,
                    p.session_minutes, p.enabled, s.source, s.durations_json, s.starts_json
             FROM classroom_schedule_slots s
             JOIN classroom_programs p ON p.subject_id = s.subject_id
             WHERE s.enabled = 1
             ORDER BY s.id",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)? as u32,
                row.get::<_, i64>(4)? as u32,
                row.get::<_, String>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, i64>(7)? != 0,
                row.get::<_, String>(8)?,
                row.get::<_, Option<String>>(9)?,
                row.get::<_, Option<String>>(10)?,
            ))
        })
        .map_err(|error| error.to_string())?;
    let mut occupied = Vec::new();
    for row in rows {
        let (
            id,
            owner,
            label,
            hour,
            minute,
            weekdays_json,
            session_minutes,
            program_enabled,
            source,
            durations_json,
            starts_json,
        ) = row.map_err(|error| error.to_string())?;
        let durations = parse_durations(durations_json);
        let starts = parse_starts(starts_json);
        let own = owner == subject_id;
        if (!program_enabled && !own)
            || scope.excluded_slot_ids.contains(&id)
            || (own
                && source == "planned"
                && scope.exclude_planned_for.as_deref() == Some(subject_id))
        {
            continue;
        }
        let weekdays: Vec<u8> = serde_json::from_str(&weekdays_json).unwrap_or_default();
        for weekday in weekdays {
            let minutes = day_minutes(&durations, weekday, session_minutes);
            let (hour, minute) = day_start(&starts, weekday, hour, minute);
            occupied.push(Occupancy {
                slot_id: id,
                subject_id: owner.clone(),
                label: label.clone(),
                weekday,
                hour,
                minute,
                session_minutes: minutes,
                interval: week_interval(weekday, hour, minute, minutes),
            });
        }
    }
    Ok(occupied)
}

/// Overlaps between proposed study times and the appointments that would fire
/// alongside them. All candidates belong to one class. Paused classes do not
/// block other classes, but their own saved times are checked again on activation.
pub fn schedule_conflicts(
    conn: &Connection,
    candidates: &[ScheduleCandidate],
    scope: &ConflictScope,
) -> Result<Vec<ScheduleConflict>> {
    let Some(first) = candidates.first() else {
        return Ok(Vec::new());
    };
    if candidates.iter().any(|c| c.subject_id != first.subject_id) {
        return Err("schedule conflicts are validated for one class at a time".into());
    }
    let label = program_row(conn, &first.subject_id)?.label;
    let existing = occupancies(conn, &first.subject_id, scope)?;
    let mut conflicts = Vec::new();
    for candidate in candidates {
        for weekday in &candidate.weekdays {
            let (hour, minute) = day_start(
                &candidate.starts,
                *weekday,
                candidate.hour,
                candidate.minute,
            );
            let interval = week_interval(
                *weekday,
                hour,
                minute,
                day_minutes(&candidate.durations, *weekday, candidate.session_minutes),
            );
            for other in &existing {
                if week_intervals_overlap(interval, other.interval) {
                    conflicts.push(ScheduleConflict {
                        weekday: *weekday,
                        hour,
                        minute,
                        subject_id: candidate.subject_id.clone(),
                        label: label.clone(),
                        with_slot_id: other.slot_id,
                        with_subject_id: other.subject_id.clone(),
                        with_label: other.label.clone(),
                        with_weekday: other.weekday,
                        with_hour: other.hour,
                        with_minute: other.minute,
                        with_session_minutes: other.session_minutes,
                    });
                }
            }
        }
    }
    conflicts.sort_by_key(|c| (c.weekday, c.hour, c.minute, c.with_slot_id, c.with_weekday));
    conflicts.dedup();
    Ok(conflicts)
}

/// This class's saved study times checked against every active class, using
/// the session length that activation would apply.
pub fn activation_conflicts(
    conn: &Connection,
    subject_id: &str,
    session_minutes: i64,
) -> Result<Vec<ScheduleConflict>> {
    let own: Vec<SlotRow> = slot_rows(conn)?
        .into_iter()
        .filter(|slot| slot.subject_id == subject_id && slot.enabled)
        .collect();
    let candidates: Vec<ScheduleCandidate> = own
        .iter()
        .map(|slot| ScheduleCandidate {
            subject_id: subject_id.into(),
            hour: slot.hour,
            minute: slot.minute,
            weekdays: slot.weekdays.clone(),
            session_minutes,
            durations: slot.durations.clone(),
            starts: slot.starts.clone(),
        })
        .collect();
    schedule_conflicts(
        conn,
        &candidates,
        &ConflictScope {
            excluded_slot_ids: own.iter().map(|slot| slot.id).collect(),
            exclude_planned_for: None,
        },
    )
}

fn weekday_name(weekday: u8) -> &'static str {
    WEEKDAY_NAMES
        .get(usize::from(weekday.saturating_sub(1)))
        .copied()
        .unwrap_or("that day")
}

pub fn conflict_message(conflicts: &[ScheduleConflict]) -> String {
    let Some(first) = conflicts.first() else {
        return String::new();
    };
    let more = match conflicts.len() {
        1 => String::new(),
        n => format!(
            " and {} other overlap{}",
            n - 1,
            if n == 2 { "" } else { "s" }
        ),
    };
    format!(
        "{} on {} at {:02}:{:02} overlaps {} on {} at {:02}:{:02} ({} min){}. Choose another time or shorten a session.",
        first.label,
        weekday_name(first.weekday),
        first.hour,
        first.minute,
        first.with_label,
        weekday_name(first.with_weekday),
        first.with_hour,
        first.with_minute,
        first.with_session_minutes,
        more
    )
}

pub fn upsert_slot(conn: &Connection, input: &UpsertClassroomSlotInput) -> Result<i64> {
    let transaction =
        rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
    subject(&input.subject_id)?;
    if input.hour > 23 || input.minute > 59 {
        return Err("classroom slot time is invalid".into());
    }
    let mut weekdays = input.weekdays.clone();
    weekdays.sort_unstable();
    weekdays.dedup();
    if weekdays.is_empty() || weekdays.iter().any(|day| !(1..=7).contains(day)) {
        return Err("select at least one valid weekday".into());
    }
    let weekdays_json = serde_json::to_string(&weekdays).map_err(|error| error.to_string())?;
    let program = program_row(conn, &input.subject_id)?;
    // Every day the rule fires on gets its minutes written down, so the
    // class's session length can follow the schedule without moving a day
    // that was left at the old default.
    let durations: std::collections::BTreeMap<u8, i64> = weekdays
        .iter()
        .map(|day| {
            (
                *day,
                input
                    .durations
                    .get(day)
                    .copied()
                    .unwrap_or(program.session_minutes),
            )
        })
        .collect();
    if durations
        .values()
        .any(|minutes| !DAY_MINUTES.contains(minutes))
    {
        return Err(format!(
            "a day's study time must last between {} and {} minutes",
            DAY_MINUTES.start(),
            DAY_MINUTES.end()
        ));
    }
    let durations_json =
        Some(serde_json::to_string(&durations).map_err(|error| error.to_string())?);
    // A day's own start is kept only when it differs from the rule's.
    let mut starts = DayStarts::new();
    for (day, time) in &input.starts {
        if !weekdays.contains(day) {
            continue;
        }
        let at =
            parse_clock(time).ok_or_else(|| format!("{time} is not a time of day (use HH:MM)"))?;
        if at != (input.hour, input.minute) {
            starts.insert(*day, at);
        }
    }
    let starts_json = if starts.is_empty() {
        None
    } else {
        Some(serde_json::to_string(&clock_text(&starts)).map_err(|error| error.to_string())?)
    };
    if input.enabled {
        let conflicts = schedule_conflicts(
            conn,
            &[ScheduleCandidate {
                subject_id: input.subject_id.clone(),
                hour: input.hour,
                minute: input.minute,
                weekdays: weekdays.clone(),
                session_minutes: program.session_minutes,
                durations: durations.clone(),
                starts: starts.clone(),
            }],
            &ConflictScope {
                excluded_slot_ids: input.id.into_iter().collect(),
                exclude_planned_for: None,
            },
        )?;
        if !conflicts.is_empty() {
            return Err(conflict_message(&conflicts));
        }
    }
    let id = if let Some(id) = input.id {
        // Hand-editing a slot "claims" it as manual, even if the schedule
        // planner originally created it — re-planning never touches it again.
        let changed = conn
            .execute(
                "UPDATE classroom_schedule_slots
                 SET hour = ?3, minute = ?4,
                     weekdays_json = ?5, enabled = ?6, source = 'manual',
                     durations_json = ?7, starts_json = ?8,
                     revision = revision + 1
                 WHERE id = ?1 AND subject_id = ?2",
                params![
                    id,
                    input.subject_id,
                    input.hour,
                    input.minute,
                    weekdays_json,
                    i64::from(input.enabled),
                    durations_json,
                    starts_json,
                ],
            )
            .map_err(|error| {
                if error.to_string().contains("UNIQUE constraint failed") {
                    "this class already has a slot at that time".to_string()
                } else {
                    error.to_string()
                }
            })?;
        if changed == 0 {
            return Err("classroom slot was not found".into());
        }
        id
    } else {
        conn.execute(
            "INSERT INTO classroom_schedule_slots
                (subject_id, hour, minute, weekdays_json, enabled, source, created_at, durations_json, starts_json)
             VALUES (?1, ?2, ?3, ?4, ?5, 'manual', ?6, ?7, ?8)",
            params![
                input.subject_id,
                input.hour,
                input.minute,
                weekdays_json,
                i64::from(input.enabled),
                language::now_iso(),
                durations_json,
                starts_json,
            ],
        )
        .map_err(|error| {
            if error.to_string().contains("UNIQUE constraint failed") {
                "this class already has a slot at that time".to_string()
            } else {
                error.to_string()
            }
        })?;
        conn.last_insert_rowid()
    };
    pause_without_schedule(conn, &input.subject_id)?;
    follow_schedule_length(conn, &input.subject_id)?;
    transaction.commit().map_err(|e| e.to_string())?;
    Ok(id)
}

/// Minutes the class's enabled study times add up to in one recurring week,
/// each day counted at its own length.
pub fn weekly_minutes_scheduled(conn: &Connection, subject_id: &str) -> Result<i64> {
    let default = program_row(conn, subject_id)
        .map(|program| program.session_minutes)
        .unwrap_or(30);
    Ok(slot_rows(conn)?
        .into_iter()
        .filter(|slot| slot.subject_id == subject_id && slot.enabled)
        .map(|slot| {
            slot.weekdays
                .iter()
                .map(|day| day_minutes(&slot.durations, *day, default))
                .sum::<i64>()
        })
        .sum())
}

/// The length the class's study times mostly have, in minutes: the most
/// common day length across its enabled rules, the shorter one on a tie.
/// None without a scheduled day.
pub fn usual_day_minutes(conn: &Connection, subject_id: &str) -> Result<Option<i64>> {
    let program = program_row(conn, subject_id)?;
    let mut tally = std::collections::BTreeMap::<i64, usize>::new();
    for slot in slot_rows(conn)?
        .into_iter()
        .filter(|slot| slot.subject_id == subject_id && slot.enabled)
    {
        for day in &slot.weekdays {
            *tally
                .entry(day_minutes(&slot.durations, *day, program.session_minutes))
                .or_default() += 1;
        }
    }
    Ok(tally
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))
        .map(|(minutes, _)| minutes))
}

/// The class's session length is not asked for: it follows the schedule, so
/// a lesson started by hand is sized like the ones the week is made of.
/// Every saved rule carries each day's minutes, so this changes nothing
/// already scheduled.
pub fn follow_schedule_length(conn: &Connection, subject_id: &str) -> Result<()> {
    let Some(minutes) = usual_day_minutes(conn, subject_id)? else {
        return Ok(());
    };
    conn.execute(
        "UPDATE classroom_programs SET session_minutes=?2 WHERE subject_id=?1 AND session_minutes<>?2",
        params![subject_id, minutes],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE language_programs SET session_minutes=?2 WHERE language=?1 AND session_minutes<>?2",
        params![subject_id, minutes],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn pause_without_schedule(conn: &Connection, subject_id: &str) -> Result<()> {
    if !has_enabled_schedule(conn, subject_id)? {
        conn.execute(
            "UPDATE classroom_programs SET enabled=0 WHERE subject_id=?1",
            [subject_id],
        )
        .map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE language_programs SET enabled=0 WHERE language=?1",
            [subject_id],
        )
        .map_err(|e| e.to_string())?;
        conn.execute("UPDATE classes SET status='paused',updated_at=?2 WHERE course_id=?1 AND status='active'", params![subject_id, language::now_iso()])
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn delete_slot(conn: &Connection, id: i64) -> Result<()> {
    let transaction =
        rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
    let subject_id: Option<String> = conn
        .query_row(
            "SELECT subject_id FROM classroom_schedule_slots WHERE id=?1",
            [id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let active_engineering: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM classroom_sessions
             WHERE slot_id = ?1 AND status = 'in_progress'",
            [id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let active_language: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM language_sessions
             WHERE classroom_slot_id = ?1 AND status = 'in_progress'",
            [id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let active_study: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM study_sessions
             WHERE status NOT IN ('completed','skipped')
               AND json_extract(context_json, '$.selection.slot_id') = ?1)",
            [id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if active_engineering + active_language > 0 || active_study {
        return Err("finish the active class before deleting this slot".into());
    }
    // Completed sessions keep their history; only the schedule link is cut.
    conn.execute(
        "UPDATE classroom_sessions SET slot_id = NULL
         WHERE slot_id = ?1 AND status != 'in_progress'",
        [id],
    )
    .map_err(|error| error.to_string())?;
    conn.execute(
        "UPDATE language_sessions SET classroom_slot_id = NULL
         WHERE classroom_slot_id = ?1 AND status != 'in_progress'",
        [id],
    )
    .map_err(|error| error.to_string())?;
    crate::domain::schedule::detach_rule(conn, id).map_err(|error| error.to_string())?;
    conn.execute("DELETE FROM classroom_schedule_slots WHERE id = ?1", [id])
        .map_err(|error| error.to_string())?;
    if let Some(subject_id) = subject_id {
        pause_without_schedule(conn, &subject_id)?;
        follow_schedule_length(conn, &subject_id)?;
    }
    transaction.commit().map_err(|e| e.to_string())?;
    Ok(())
}

/// One window of real-world availability the learner supplied to the
/// schedule planner (e.g. "Mon/Wed/Fri 7:00-8:00").
#[derive(Debug, Clone, Deserialize)]
pub struct AvailabilityWindowInput {
    pub weekdays: Vec<u8>,
    pub start_hour: u32,
    pub start_minute: u32,
    pub end_hour: u32,
    pub end_minute: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlanClassroomScheduleInput {
    pub subject_id: String,
    #[serde(default)]
    pub learning_goal: String,
    pub target_weekly_minutes: i64,
    pub windows: Vec<AvailabilityWindowInput>,
    /// false = preview only (no writes); true = persist the plan.
    pub commit: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlannedSlotView {
    pub hour: u32,
    pub minute: u32,
    pub weekdays: Vec<u8>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClassroomPlanView {
    pub slots: Vec<PlannedSlotView>,
    pub total_weekly_minutes: i64,
    pub target_weekly_minutes: i64,
    pub meets_target: bool,
    /// Overlaps with other active classes or this class's manual times. A
    /// preview shows them; committing a conflicting plan is refused.
    pub conflicts: Vec<ScheduleConflict>,
    pub program: Option<ClassroomProgramView>,
    pub schedule: Option<Vec<ClassroomSlotView>>,
}

/// One proposed recurring slot per stated availability window, at that
/// window's start time. Windows that resolve to the exact same time merge
/// their weekdays into a single slot instead of colliding.
fn plan_windows(windows: &[AvailabilityWindowInput]) -> Result<Vec<PlannedSlotView>> {
    if windows.is_empty() {
        return Err("add at least one availability window".into());
    }
    let mut slots: Vec<PlannedSlotView> = Vec::new();
    for window in windows {
        if window.start_hour > 23
            || window.end_hour > 23
            || window.start_minute > 59
            || window.end_minute > 59
        {
            return Err("availability window time is invalid".into());
        }
        let mut weekdays = window.weekdays.clone();
        weekdays.sort_unstable();
        weekdays.dedup();
        if weekdays.is_empty() || weekdays.iter().any(|day| !(1..=7).contains(day)) {
            return Err("every availability window needs at least one valid weekday".into());
        }
        if (window.start_hour, window.start_minute) >= (window.end_hour, window.end_minute) {
            return Err("availability window end time must be after its start time".into());
        }
        if let Some(existing) = slots
            .iter_mut()
            .find(|slot| slot.hour == window.start_hour && slot.minute == window.start_minute)
        {
            existing.weekdays.extend(weekdays);
            existing.weekdays.sort_unstable();
            existing.weekdays.dedup();
        } else {
            slots.push(PlannedSlotView {
                hour: window.start_hour,
                minute: window.start_minute,
                weekdays,
            });
        }
    }
    slots.sort_unstable_by_key(|slot| (slot.hour, slot.minute));
    Ok(slots)
}

/// Preview or persist a schedule generated from a learner's stated goal,
/// weekly-minutes target, and availability. Additive to the manual slot
/// editor: committing only replaces this subject's previously *planned*
/// slots, never slots a learner hand-placed with `upsert_slot`.
pub fn plan_schedule(
    conn: &Connection,
    input: &PlanClassroomScheduleInput,
    today: &str,
) -> Result<ClassroomPlanView> {
    let spec = subject(&input.subject_id)?;
    if input.target_weekly_minutes < 0 {
        return Err("target weekly minutes cannot be negative".into());
    }
    let slots = plan_windows(&input.windows)?;
    // Preview and commit share the same collision check. A manual slot owns
    // its time even when its weekdays differ from the proposed recurrence.
    let transaction = conn
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
    let conn = &*transaction;
    for slot in &slots {
        let manual_collision: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM classroom_schedule_slots
             WHERE subject_id = ?1 AND hour = ?2 AND minute = ?3 AND source = 'manual')",
                params![input.subject_id, slot.hour, slot.minute],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if manual_collision {
            return Err(format!("A manual class already uses {:02}:{:02}. Choose another planning window or edit that class time first.", slot.hour, slot.minute));
        }
    }
    let program = program_row(conn, &input.subject_id)?;
    let total_weekly_minutes: i64 = slots
        .iter()
        .map(|slot| slot.weekdays.len() as i64 * program.session_minutes)
        .sum();
    let meets_target =
        input.target_weekly_minutes == 0 || total_weekly_minutes >= input.target_weekly_minutes;
    let candidates: Vec<ScheduleCandidate> = slots
        .iter()
        .map(|slot| ScheduleCandidate {
            subject_id: input.subject_id.clone(),
            hour: slot.hour,
            minute: slot.minute,
            weekdays: slot.weekdays.clone(),
            session_minutes: program.session_minutes,
            durations: Default::default(),
            starts: Default::default(),
        })
        .collect();
    let conflicts = schedule_conflicts(
        conn,
        &candidates,
        &ConflictScope {
            excluded_slot_ids: Vec::new(),
            exclude_planned_for: Some(input.subject_id.clone()),
        },
    )?;
    if input.commit && !conflicts.is_empty() {
        return Err(conflict_message(&conflicts));
    }

    let (view_program, view_schedule) = if input.commit {
        conn.execute(
            "UPDATE classroom_programs
             SET learning_goal = ?2, target_weekly_minutes = ?3, updated_at = ?4
             WHERE subject_id = ?1",
            params![
                input.subject_id,
                input.learning_goal.trim(),
                input.target_weekly_minutes,
                language::now_iso(),
            ],
        )
        .map_err(|error| error.to_string())?;
        if spec.kind == SubjectKind::Language {
            let existing = language::program_view(conn, spec.id, today)?;
            language::configure_program(
                conn,
                &language::ConfigureProgramInput {
                    language: spec.id.into(),
                    enabled: program.enabled,
                    start_level: existing.start_level,
                    target_level: existing.target_level,
                    weekly_minutes: input.target_weekly_minutes,
                    session_minutes: program.session_minutes,
                },
                today,
            )?;
        }
        conn.execute(
            "DELETE FROM classroom_schedule_slots WHERE subject_id = ?1 AND source = 'planned'",
            [&input.subject_id],
        )
        .map_err(|error| error.to_string())?;
        for slot in &slots {
            let weekdays_json =
                serde_json::to_string(&slot.weekdays).map_err(|error| error.to_string())?;
            conn.execute(
                "INSERT INTO classroom_schedule_slots
                    (subject_id, hour, minute, weekdays_json, enabled, source, created_at)
                 VALUES (?1, ?2, ?3, ?4, 1, 'planned', ?5)
                 ON CONFLICT(subject_id, hour, minute) DO NOTHING",
                params![
                    input.subject_id,
                    slot.hour,
                    slot.minute,
                    weekdays_json,
                    language::now_iso(),
                ],
            )
            .map_err(|error| error.to_string())?;
        }
        crate::domain::classes::sync_configuration(conn, spec.id, today)
            .map_err(|e| e.to_string())?;
        let schedule = slot_views(conn, today, false)?
            .into_iter()
            .filter(|view| view.subject_id == input.subject_id)
            .collect();
        (
            Some(program_view(conn, &input.subject_id, today)?),
            Some(schedule),
        )
    } else {
        (None, None)
    };

    transaction.commit().map_err(|error| error.to_string())?;
    Ok(ClassroomPlanView {
        slots,
        total_weekly_minutes,
        target_weekly_minutes: input.target_weekly_minutes,
        meets_target,
        conflicts,
        program: view_program,
        schedule: view_schedule,
    })
}

#[derive(Debug, Clone)]
struct SlotRow {
    id: i64,
    subject_id: String,
    label: String,
    short_code: String,
    kind: String,
    hour: u32,
    minute: u32,
    weekdays: Vec<u8>,
    enabled: bool,
    program_enabled: bool,
    durations: std::collections::BTreeMap<u8, i64>,
    starts: DayStarts,
    source: String,
}

impl SlotRow {
    fn start_on(&self, weekday: u8) -> (u32, u32) {
        day_start(&self.starts, weekday, self.hour, self.minute)
    }
}

fn slot_rows(conn: &Connection) -> Result<Vec<SlotRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT s.id, s.subject_id, p.label, p.short_code, p.kind,
                    s.hour, s.minute, s.weekdays_json, s.enabled, p.enabled, s.source, s.durations_json,
                    s.starts_json
             FROM classroom_schedule_slots s
             JOIN classroom_programs p ON p.subject_id = s.subject_id
             ORDER BY s.hour, s.minute, s.id",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            let weekdays_json: String = row.get(7)?;
            Ok(SlotRow {
                id: row.get(0)?,
                subject_id: row.get(1)?,
                label: row.get(2)?,
                short_code: row.get(3)?,
                kind: row.get(4)?,
                hour: row.get::<_, i64>(5)? as u32,
                minute: row.get::<_, i64>(6)? as u32,
                weekdays: serde_json::from_str(&weekdays_json).unwrap_or_default(),
                enabled: row.get::<_, i64>(8)? != 0,
                program_enabled: row.get::<_, i64>(9)? != 0,
                source: row.get(10)?,
                durations: parse_durations(row.get::<_, Option<String>>(11)?),
                starts: parse_starts(row.get::<_, Option<String>>(12)?),
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn weekday_number(weekday: Weekday) -> u8 {
    weekday.number_from_monday() as u8
}

pub fn slot_due_at(
    hour: u32,
    minute: u32,
    weekdays: &[u8],
    now: NaiveDateTime,
    consumed_today: bool,
) -> bool {
    weekdays.contains(&weekday_number(now.weekday()))
        && !consumed_today
        && (now.hour(), now.minute()) >= (hour, minute)
}

fn slot_state(conn: &Connection, slot: &SlotRow, today: &str) -> Result<(bool, bool)> {
    let status = if slot.kind == "language" {
        conn.query_row(
            "SELECT status FROM language_sessions WHERE classroom_slot_id = ?1
             AND session_date = ?2 ORDER BY id DESC LIMIT 1",
            params![slot.id, today],
            |row| row.get::<_, String>(0),
        )
        .optional()
    } else {
        conn.query_row(
            "SELECT status FROM classroom_sessions WHERE slot_id = ?1
             AND session_date = ?2 ORDER BY id DESC LIMIT 1",
            params![slot.id, today],
            |row| row.get::<_, String>(0),
        )
        .optional()
    };
    let status = status.map_err(|error| error.to_string())?;
    if let Some(status) = status {
        return Ok((true, status == "in_progress"));
    }
    // A started, completed or skipped appointment consumes the rule for the day.
    if let Some(occurrence) = crate::domain::schedule::today_for_rule(conn, slot.id, today)
        .map_err(|error| error.to_string())?
    {
        if occurrence.consumed() {
            return Ok((true, occurrence.disposition == "started"));
        }
    }
    // Shared-runtime lessons planned for this rule consume it for the day.
    match crate::subjects::engineering::slot_session_status(conn, slot.id, today)? {
        Some(status) => Ok((true, !matches!(status.as_str(), "completed" | "skipped"))),
        None => Ok((false, false)),
    }
}

fn next_fire_at(slot: &SlotRow, now: NaiveDateTime, consumed_today: bool) -> String {
    for offset in 0..=8 {
        let date = now.date() + Duration::days(offset);
        let weekday = weekday_number(date.weekday());
        if !slot.weekdays.contains(&weekday) {
            continue;
        }
        let (hour, minute) = slot.start_on(weekday);
        let Some(candidate) = date.and_hms_opt(hour, minute, 0) else {
            continue;
        };
        if (offset == 0 && consumed_today) || candidate <= now {
            continue;
        }
        return candidate.format("%Y-%m-%dT%H:%M:%S").to_string();
    }
    now.format("%Y-%m-%dT%H:%M:%S").to_string()
}

#[derive(Debug, Clone, Serialize)]
pub struct CurriculumConceptView {
    pub id: i64,
    pub slug: String,
    pub title: String,
    pub category: String,
    pub tier: i64,
    pub phase: String,
    pub core: bool,
    pub prerequisites: Vec<String>,
    pub mastery_state: String,
    pub times_picked: i64,
    pub last_picked_date: Option<String>,
    pub learner_outcome: String,
    pub artifact: String,
    pub related_concepts: Vec<String>,
    /// Route status against the accepted path: completed_here, prior_knowledge_checked,
    /// bypassed_by_choice, needs_refresher, not_assessed, bridge, in_progress (a lesson
    /// open right now), taught (a lesson behind it, not yet mastered) or upcoming.
    pub path_status: String,
    /// Core topic still required by the accepted route.
    pub required: bool,
}

/// Remaining required work for the accepted route beside full-course coverage,
/// each with its own denominator.
#[derive(Debug, Clone, Serialize)]
pub struct PathCoverage {
    pub revision: u32,
    pub entry_label: String,
    pub required_total: usize,
    pub required_done: usize,
    pub coverage_total: usize,
    pub coverage_done: usize,
    /// Core topics with a lesson behind them, mastered or not: what the
    /// overview calls practised.
    pub taught: usize,
    pub bypassed: usize,
    pub checked: usize,
    pub refreshers: usize,
    pub bridges: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct CurriculumMapView {
    pub focus: String,
    pub label: String,
    pub month_outcome: String,
    pub completed_sessions: i64,
    pub current_phase: String,
    pub concepts: Vec<CurriculumConceptView>,
    pub path: Option<PathCoverage>,
    /// Gaps seen in practice that a short bridge lesson would close.
    pub bridge_proposals: Vec<crate::domain::classes::BridgeProposal>,
    /// Unit challenges sample the course's question bank; a learner's own
    /// class has none until it is written.
    pub challenges_available: bool,
}

pub fn curriculum_map(conn: &Connection, focus: &str) -> Result<CurriculumMapView> {
    crate::focus::validate_selectable(focus).map_err(|error| error.to_string())?;
    let mastery_by_id = mastery::overview(conn, focus)
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|entry| (entry.concept_id, entry.state))
        .collect::<std::collections::HashMap<_, _>>();
    let mut concepts = Vec::new();
    for concept in crate::db::all_concepts(conn, focus).map_err(|error| error.to_string())? {
        let prerequisites_json: String = conn
            .query_row(
                "SELECT prereqs_json FROM concepts WHERE id = ?1",
                [concept.id],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        concepts.push(CurriculumConceptView {
            id: concept.id,
            slug: concept.slug,
            title: concept.title,
            category: concept.category,
            tier: concept.tier,
            phase: concept.curriculum.phase,
            core: concept.curriculum.core,
            prerequisites: serde_json::from_str(&prerequisites_json).unwrap_or_default(),
            mastery_state: mastery_by_id
                .get(&concept.id)
                .cloned()
                .unwrap_or_else(|| "unseen".into()),
            times_picked: concept.times_picked,
            last_picked_date: concept.last_picked_date,
            learner_outcome: concept.curriculum.learner_outcome,
            artifact: concept.curriculum.artifact,
            related_concepts: concept.curriculum.related_concepts,
            path_status: String::new(),
            required: false,
        });
    }
    let path =
        crate::domain::classes::current_path(conn, focus).map_err(|error| error.to_string())?;
    let plan = path.as_ref().map(|path| &path.recommendation);
    let listed = |list: &[crate::domain::placement::PathTopic], slug: &str| {
        list.iter().any(|t| t.id == slug)
    };
    // The topic a lesson is open on right now, if any: the one topic that
    // reads as in progress. Every other topic with a lesson behind it reads
    // as taught until mastery says otherwise.
    let open_slugs: std::collections::HashSet<String> = conn
        .prepare(
            "SELECT json_extract(context_json, '$.selection.slug') FROM study_sessions
             WHERE status = 'active' AND owner_kind = 'class' AND owner_id = ?1",
        )
        .and_then(|mut statement| {
            statement
                .query_map([focus], |row| row.get::<_, Option<String>>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .map(|rows| rows.into_iter().flatten().collect())
        .unwrap_or_default();
    for concept in &mut concepts {
        let completed = crate::selection::is_completed(&concept.mastery_state);
        let slug = concept.slug.as_str();
        let (status, on_route) = match plan {
            _ if completed => ("completed_here", true),
            _ if open_slugs.contains(slug) => ("in_progress", true),
            Some(plan) if listed(&plan.bridges, slug) => ("bridge", true),
            Some(plan) if listed(&plan.checked, slug) => ("prior_knowledge_checked", false),
            Some(plan) if listed(&plan.bypassed, slug) => ("bypassed_by_choice", false),
            Some(plan) if listed(&plan.refreshers, slug) => ("needs_refresher", false),
            Some(plan) if listed(&plan.earlier_topics, slug) => (
                if plan
                    .earlier_topics
                    .iter()
                    .any(|t| t.id == slug && t.reason.starts_with("Declared familiar"))
                {
                    "bypassed_by_choice"
                } else {
                    "not_assessed"
                },
                false,
            ),
            _ if concept.mastery_state != "unseen" => ("taught", true),
            _ => ("upcoming", true),
        };
        concept.path_status = status.into();
        concept.required = concept.core && on_route && !completed;
    }
    let path_coverage = path.as_ref().map(|path| {
        let plan = &path.recommendation;
        let core = concepts.iter().filter(|c| c.core);
        let on_route = |c: &&CurriculumConceptView| {
            crate::selection::is_completed(&c.mastery_state)
                || !(listed(&plan.earlier_topics, &c.slug)
                    || listed(&plan.bypassed, &c.slug)
                    || listed(&plan.checked, &c.slug))
        };
        PathCoverage {
            revision: path.revision,
            entry_label: plan.entry_label.clone(),
            required_total: core.clone().filter(on_route).count(),
            required_done: core
                .clone()
                .filter(on_route)
                .filter(|c| crate::selection::is_completed(&c.mastery_state))
                .count(),
            coverage_total: core.clone().count(),
            coverage_done: core
                .clone()
                .filter(|c| crate::selection::is_completed(&c.mastery_state))
                .count(),
            taught: core.clone().filter(|c| c.mastery_state != "unseen").count(),
            bypassed: concepts
                .iter()
                .filter(|c| c.path_status == "bypassed_by_choice")
                .count(),
            checked: concepts
                .iter()
                .filter(|c| c.path_status == "prior_knowledge_checked")
                .count(),
            refreshers: concepts
                .iter()
                .filter(|c| c.path_status == "needs_refresher")
                .count(),
            bridges: concepts
                .iter()
                .filter(|c| c.path_status == "bridge")
                .count(),
        }
    });
    let phase_order = [
        "foundations",
        "mechanisms",
        "production",
        "synthesis",
        "elective",
    ];
    let current_phase = phase_order
        .into_iter()
        .find(|phase| {
            concepts.iter().any(|concept| {
                concept.core
                    && concept.phase == *phase
                    && !matches!(concept.mastery_state.as_str(), "mastered" | "maintenance")
            })
        })
        .unwrap_or("elective")
        .to_string();
    let completed_sessions = completed_lesson_count(conn, focus)?;
    Ok(CurriculumMapView {
        focus: focus.into(),
        label: crate::focus::label(focus).into(),
        month_outcome: crate::focus::month_outcome(focus).into(),
        completed_sessions,
        current_phase,
        concepts,
        path: path_coverage,
        bridge_proposals: crate::domain::classes::bridge_proposals(conn, focus)
            .map_err(|error| error.to_string())?,
        challenges_available: crate::domain::placement::has_bank(focus),
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct ClassroomSlotView {
    pub id: i64,
    pub subject_id: String,
    /// Minutes on particular weekdays; a day not listed uses the class default.
    pub durations: std::collections::BTreeMap<u8, i64>,
    /// "HH:MM" on particular weekdays; a day not listed starts at `hour:minute`.
    pub starts: std::collections::BTreeMap<u8, String>,
    pub label: String,
    pub short_code: String,
    pub kind: String,
    pub hour: u32,
    pub minute: u32,
    pub weekdays: Vec<u8>,
    pub enabled: bool,
    pub owed: bool,
    pub next_fire_at: String,
    pub in_progress: bool,
    pub source: String,
    /// Today's durable appointment for this rule, once materialized.
    pub occurrence_id: Option<String>,
    pub disposition: Option<String>,
}

/// An appointment as Today and the class schedule show it.
#[derive(Debug, Clone, Serialize)]
pub struct AppointmentView {
    pub id: String,
    pub course_id: String,
    pub label: String,
    pub short_code: String,
    pub kind: String,
    pub rule_id: Option<i64>,
    pub local_date: String,
    pub local_time: String,
    pub fires_at: String,
    pub duration_minutes: i64,
    pub disposition: String,
    pub session_ref: Option<String>,
    /// A missed appointment can still be started as a make-up session.
    pub make_up: bool,
}

pub fn appointment_view(
    conn: &Connection,
    occurrence: crate::domain::schedule::Occurrence,
) -> Result<AppointmentView> {
    let program = program_row(conn, &occurrence.course_id)?;
    Ok(AppointmentView {
        make_up: occurrence.disposition == "missed",
        id: occurrence.id,
        course_id: occurrence.course_id,
        label: program.label,
        short_code: program.short_code,
        kind: program.kind,
        rule_id: occurrence.rule_id,
        local_date: occurrence.local_date,
        local_time: occurrence.local_time,
        fires_at: occurrence.fires_at,
        duration_minutes: occurrence.duration_minutes,
        disposition: occurrence.disposition,
        session_ref: occurrence.session_ref,
    })
}

/// Materialize today's appointments and mark missed ones for the learner's
/// clock. Paused scheduling creates no new appointments.
pub fn refresh_appointments(conn: &Connection, today: &str) -> Result<Vec<AppointmentView>> {
    use crate::domain::schedule::{self, Clock, SystemZone};
    let paused = matches!(db::get_config(conn, "schedule_paused"), Ok(Some(value)) if value == "1");
    let date =
        chrono::NaiveDate::parse_from_str(today, "%Y-%m-%d").map_err(|error| error.to_string())?;
    let clock = Clock {
        zone: &SystemZone,
        now: chrono::Utc::now(),
        today: date,
    };
    schedule::materialize(conn, &clock, paused)
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|occurrence| appointment_view(conn, occurrence))
        .collect()
}

/// Today's appointments plus recent missed ones, without materializing.
pub fn appointment_views(conn: &Connection, today: &str) -> Result<Vec<AppointmentView>> {
    crate::domain::schedule::agenda(conn, today)
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|occurrence| appointment_view(conn, occurrence))
        .collect()
}

/// Appointment history of one class, newest first.
pub fn appointment_history(conn: &Connection, course_id: &str) -> Result<Vec<AppointmentView>> {
    crate::domain::schedule::history(conn, course_id, 30)
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|occurrence| appointment_view(conn, occurrence))
        .collect()
}

pub fn slot_views(
    conn: &Connection,
    today: &str,
    debug_day: bool,
) -> Result<Vec<ClassroomSlotView>> {
    let paused = matches!(db::get_config(conn, "schedule_paused"), Ok(Some(value)) if value == "1");
    let now = Local::now().naive_local();
    slot_rows(conn)?
        .into_iter()
        .map(|slot| {
            let (consumed, in_progress) = slot_state(conn, &slot, today)?;
            let available = slot.enabled && slot.program_enabled && !paused;
            let occurrence = crate::domain::schedule::today_for_rule(conn, slot.id, today)
                .map_err(|error| error.to_string())?;
            let owed = available
                && !consumed
                && match &occurrence {
                    Some(occurrence) => {
                        occurrence.disposition == "due"
                            || (debug_day && occurrence.disposition == "scheduled")
                    }
                    None => {
                        let (hour, minute) = slot.start_on(weekday_number(now.weekday()));
                        debug_day || slot_due_at(hour, minute, &slot.weekdays, now, false)
                    }
                };
            let next_fire = next_fire_at(&slot, now, consumed);
            Ok(ClassroomSlotView {
                occurrence_id: occurrence.as_ref().map(|o| o.id.clone()),
                disposition: occurrence.as_ref().map(|o| o.disposition.clone()),
                id: slot.id,
                durations: slot.durations,
                starts: clock_text(&slot.starts),
                subject_id: slot.subject_id,
                label: slot.label,
                short_code: slot.short_code,
                kind: slot.kind,
                hour: slot.hour,
                minute: slot.minute,
                weekdays: slot.weekdays,
                enabled: slot.enabled,
                owed,
                next_fire_at: next_fire,
                in_progress,
                source: slot.source,
            })
        })
        .collect()
}

pub fn all_schedule_times(conn: &Connection) -> Result<Vec<(u32, u32)>> {
    // The retired once-a-day routine is not an appointment source. Its saved
    // time remains legacy provenance; only enabled class rules wake the app.
    let mut times = slot_rows(conn)?
        .into_iter()
        .filter(|slot| slot.enabled && slot.program_enabled)
        .flat_map(|slot| {
            slot.weekdays
                .iter()
                .map(|day| slot.start_on(*day))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    times.sort_unstable();
    times.dedup();
    Ok(times)
}

fn has_active_session(conn: &Connection, subject_id: &str) -> Result<bool> {
    let engineering: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM classroom_sessions
             WHERE subject_id = ?1 AND status = 'in_progress'",
            [subject_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let language: i64 = if matches!(subject(subject_id)?.kind, SubjectKind::Language) {
        conn.query_row(
            "SELECT COUNT(*) FROM language_sessions
             WHERE language = ?1 AND status = 'in_progress'",
            [subject_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?
    } else {
        0
    };
    Ok(engineering + language > 0
        || crate::subjects::engineering::has_open_session(conn, subject_id)?)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredQuestion {
    pub id: usize,
    pub prompt: String,
    pub choices: Vec<String>,
    pub correct_index: usize,
    pub explanation: String,
    #[serde(default)]
    pub section: String,
    pub learning_objective: String,
}

pub fn standard_level() -> String {
    "standard".into()
}

/// Immutable lesson payload shared by legacy classroom rows and lesson versions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredEngineeringLesson {
    pub concept_id: i64,
    pub concept_title: String,
    pub category: String,
    pub title: String,
    pub markdown: String,
    pub resources: Vec<Resource>,
    /// Points the editorial audit still wanted changed, shown to the learner.
    #[serde(default)]
    pub review_notes: Vec<String>,
    /// Set when no documentation could be retrieved while writing: the
    /// lesson's claims were not checked against the sources.
    #[serde(default)]
    pub research_note: Option<String>,
    /// `beginner` when written for a learner starting from scratch on a
    /// foundations topic, else `standard`. The reader labels sections for
    /// the level; the Markdown keeps the canonical headings.
    #[serde(default = "standard_level")]
    pub level: String,
    pub questions: Vec<StoredQuestion>,
    pub exercise: Option<Exercise>,
    pub source: String,
    #[serde(default)]
    pub path: Option<crate::domain::classes::PathReference>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClassroomQuestionView {
    pub id: usize,
    pub prompt: String,
    pub choices: Vec<String>,
    pub section: String,
    pub learning_objective: String,
}

/// Saved knowledge-check answers for a shared-runtime lesson.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckView {
    pub round_id: crate::domain::assessments::RoundId,
    pub revision: u32,
    pub responses: std::collections::BTreeMap<String, crate::domain::assessments::Response>,
    pub submitted: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct EngineeringLessonView {
    /// Legacy classroom row ID or shared-runtime `study-…` ID; see `runtime`.
    pub session_id: String,
    /// `legacy` rows finish through the compatibility submit; `study` lessons
    /// carry saved work, a frozen check round and their lifecycle.
    pub runtime: String,
    pub lifecycle: String,
    pub revision: u32,
    pub checkpoint: Option<crate::domain::sessions::Checkpoint>,
    pub check: Option<CheckView>,
    pub outcome: Option<EngineeringSessionResult>,
    pub subject_id: String,
    pub label: String,
    pub short_code: String,
    pub title: String,
    pub concept_slug: String,
    pub concept_title: String,
    pub category: String,
    pub curriculum: crate::db::CurriculumBrief,
    pub prerequisites: Vec<String>,
    pub session_index: i64,
    pub why_now: String,
    pub markdown: String,
    pub resources: Vec<Resource>,
    /// Points the editorial audit still wanted changed, shown to the learner.
    #[serde(default)]
    pub review_notes: Vec<String>,
    /// Why the lesson's claims were not checked against documentation, when
    /// they were not.
    #[serde(default)]
    pub research_note: Option<String>,
    /// `beginner` or `standard`; the reader names sections for the level.
    #[serde(default = "standard_level")]
    pub level: String,
    pub questions: Vec<ClassroomQuestionView>,
    pub exercise: Option<Exercise>,
    /// `lesson` or `retrieval` (delayed review without a new lesson).
    pub kind: String,
    /// False when a retrieval repeats the last lesson's questions.
    pub fresh_sample: bool,
    pub agent_used: String,
    pub prompt_profile: String,
    pub prompt_version: String,
    pub estimated_minutes: i64,
    /// How the session's minutes divide between reading, practice and check.
    pub plan: crate::lesson_shape::SessionPlan,
    pub status: String,
}

fn engineering_view(conn: &Connection, session_id: i64) -> Result<Option<EngineeringLessonView>> {
    let row = conn
        .query_row(
            "SELECT s.subject_id, p.label, p.short_code, s.payload_json,
                    s.agent_used, p.prompt_profile, s.prompt_version,
                    p.session_minutes, s.status
             FROM classroom_sessions s
             JOIN classroom_programs p ON p.subject_id = s.subject_id
             WHERE s.id = ?1",
            [session_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, String>(8)?,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?;
    row.map(
        |(
            subject_id,
            label,
            short_code,
            payload_json,
            agent_used,
            prompt_profile,
            prompt_version,
            minutes,
            status,
        )| {
            let stored: StoredEngineeringLesson =
                serde_json::from_str(&payload_json).map_err(|error| error.to_string())?;
            let concept = crate::db::get_concept(conn, stored.concept_id)
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "classroom concept no longer exists".to_string())?;
            let prerequisites_json: String = conn
                .query_row(
                    "SELECT prereqs_json FROM concepts WHERE id = ?1",
                    [concept.id],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())?;
            let session_index = completed_lesson_count(conn, &subject_id)? + 1;
            let why_now = format!(
                "Session {session_index} advances the {} phase: {}",
                concept.curriculum.phase, concept.curriculum.learner_outcome
            );
            Ok(EngineeringLessonView {
                session_id: session_id.to_string(),
                runtime: "legacy".into(),
                kind: "lesson".into(),
                fresh_sample: true,
                lifecycle: status.clone(),
                revision: 0,
                checkpoint: None,
                check: None,
                outcome: None,
                subject_id,
                label,
                short_code,
                title: stored.title,
                concept_slug: concept.slug,
                concept_title: stored.concept_title,
                category: stored.category,
                curriculum: concept.curriculum,
                prerequisites: serde_json::from_str(&prerequisites_json).unwrap_or_default(),
                session_index,
                why_now,
                markdown: stored.markdown,
                resources: stored.resources,
                review_notes: stored.review_notes,
                research_note: stored.research_note,
                level: stored.level,
                questions: stored
                    .questions
                    .into_iter()
                    .map(|question| ClassroomQuestionView {
                        id: question.id,
                        prompt: question.prompt,
                        choices: question.choices,
                        section: question.section,
                        learning_objective: question.learning_objective,
                    })
                    .collect(),
                exercise: stored.exercise,
                agent_used,
                prompt_profile,
                prompt_version,
                estimated_minutes: minutes,
                plan: crate::generator::LessonBudget::for_minutes(minutes).plan(),
                status,
            })
        },
    )
    .transpose()
}

pub fn engineering_chat_context(
    conn: &Connection,
    session_id: i64,
) -> Result<crate::generator::CourseChatContext> {
    let (subject_id, payload_json) = conn
        .query_row(
            "SELECT subject_id, payload_json
             FROM classroom_sessions WHERE id = ?1",
            [session_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "classroom session not found".to_string())?;
    let stored: StoredEngineeringLesson =
        serde_json::from_str(&payload_json).map_err(|error| error.to_string())?;
    let concept = db::get_concept(conn, stored.concept_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "classroom chat concept not found".to_string())?;
    let exercise = stored
        .exercise
        .as_ref()
        .map(serde_json::to_string_pretty)
        .transpose()
        .map_err(|error| error.to_string())?
        .unwrap_or_else(|| "(this course has no separate exercise)".into());
    Ok(crate::generator::CourseChatContext {
        title: stored.title,
        focus: subject_id,
        markdown: stored.markdown,
        learner_outcome: concept.curriculum.learner_outcome,
        cumulative_artifact: concept.curriculum.artifact,
        exercise,
    })
}

pub fn classroom_exercise(conn: &Connection, session_id: i64) -> Result<Option<db::ExerciseView>> {
    let payload_json: Option<String> = conn
        .query_row(
            "SELECT payload_json FROM classroom_sessions WHERE id = ?1",
            [session_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let Some(payload_json) = payload_json else {
        return Ok(None);
    };
    let stored: StoredEngineeringLesson =
        serde_json::from_str(&payload_json).map_err(|error| error.to_string())?;
    let draft =
        db::get_exercise_draft(conn, None, Some(session_id)).map_err(|error| error.to_string())?;
    let (completed, reflection) = db::get_exercise_completion(conn, None, Some(session_id))
        .map_err(|error| error.to_string())?;
    Ok(stored.exercise.map(|exercise| db::ExerciseView {
        course_id: None,
        classroom_session_id: Some(session_id),
        study_session_id: None,
        title: exercise.title,
        instructions: exercise.instructions,
        starter_code: exercise.starter_code,
        deliverable: exercise.deliverable,
        hints: exercise.hints,
        draft,
        completed,
        reflection,
    }))
}

fn active_engineering_for(
    conn: &Connection,
    subject_id: &str,
) -> Result<Option<EngineeringLessonView>> {
    let id = conn
        .query_row(
            "SELECT id FROM classroom_sessions
             WHERE subject_id = ?1 AND status = 'in_progress'
             ORDER BY id DESC LIMIT 1",
            [subject_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    id.map(|id| engineering_view(conn, id))
        .transpose()
        .map(|value| value.flatten())
}

pub fn active_engineering_session(
    conn: &Connection,
    subject_id: &str,
) -> Result<Option<EngineeringLessonView>> {
    active_engineering_for(conn, subject_id)
}

pub fn generation_profile(row: &ProgramRow) -> GenerationProfile {
    GenerationProfile {
        subject_id: row.subject_id.clone(),
        agent: row.agent.clone(),
        model: row.model.clone(),
        custom_bin: row.custom_agent_bin.clone(),
        prompt_version: format!("{}.{}", row.prompt_profile, row.prompt_version),
    }
}

/// Prepend the learner's stated goal (from the schedule planner) to a
/// subject's static prompt contract. The goal only nudges tone/emphasis —
/// it never overrides the contract's required subject lens or, for
/// languages, the frozen CEFR objective and assessment gates.
pub fn contract_with_goal(program: &ProgramRow, base_contract: &str) -> String {
    let goal = program.learning_goal.trim();
    if goal.is_empty() {
        base_contract.to_string()
    } else {
        format!("LEARNER GOAL FOR THIS CLASS: {goal}\n\n{base_contract}")
    }
}

/// A scheduled start must name one of this class's own rules, and a rule fires
/// at most once per service date across both session stores.
pub fn validate_slot_start(
    conn: &Connection,
    subject_id: &str,
    slot_id: i64,
    today: &str,
) -> Result<()> {
    let owner: Option<String> = conn
        .query_row(
            "SELECT subject_id FROM classroom_schedule_slots WHERE id = ?1",
            [slot_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if owner.as_deref() != Some(subject_id) {
        return Err("classroom slot does not belong to this subject".into());
    }
    let consumed: i64 = conn
        .query_row(
            "SELECT (SELECT COUNT(*) FROM classroom_sessions WHERE slot_id = ?1 AND session_date = ?2)
                  + (SELECT COUNT(*) FROM language_sessions WHERE classroom_slot_id = ?1 AND session_date = ?2)",
            params![slot_id, today],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if consumed > 0 || crate::subjects::engineering::slot_consumed(conn, slot_id, today)? {
        return Err("this class slot is already complete for today".into());
    }
    if crate::domain::schedule::today_for_rule(conn, slot_id, today)
        .map_err(|error| error.to_string())?
        .is_some_and(|occurrence| occurrence.consumed())
    {
        return Err("this class slot is already complete for today".into());
    }
    Ok(())
}

/// A block in progress, labelled for the desk.
#[derive(Debug, Clone, Serialize)]
pub struct BlockView {
    #[serde(flatten)]
    pub progress: crate::domain::schedule::BlockProgress,
    pub label: String,
    /// Whether a session of this block is open right now.
    pub in_session: bool,
}

/// Today's appointments that are blocks with a step still ahead of them.
pub fn block_views(
    conn: &Connection,
    today: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<Vec<BlockView>> {
    let mut views = Vec::new();
    for appointment in refresh_appointments(conn, today)? {
        if appointment.disposition != "started" {
            continue;
        }
        let Some(progress) = crate::domain::schedule::block_progress(conn, &appointment.id, now)
            .map_err(|e| e.to_string())?
        else {
            continue;
        };
        let in_session = appointment
            .session_ref
            .as_deref()
            .and_then(|r| r.strip_prefix("study:"))
            .map(|id| {
                conn.query_row(
                    "SELECT EXISTS(SELECT 1 FROM study_sessions WHERE id=?1 AND status NOT IN ('completed','skipped'))",
                    [id],
                    |r| r.get::<_, bool>(0),
                )
                .unwrap_or(false)
            })
            .unwrap_or(false);
        views.push(BlockView {
            progress,
            label: appointment.label,
            in_session,
        });
    }
    Ok(views)
}

/// Completed lessons for a class across the retired daily routine, legacy
/// classroom rows and shared-runtime sessions.
pub fn completed_lesson_count(conn: &Connection, subject_id: &str) -> Result<i64> {
    let legacy: i64 = conn
        .query_row(
            "SELECT
                (SELECT COUNT(*) FROM sessions
                 WHERE status = 'completed' AND focus = ?1)
              + (SELECT COUNT(*) FROM classroom_sessions
                 WHERE status = 'completed' AND subject_id = ?1)",
            [subject_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    Ok(legacy + crate::subjects::engineering::completed_count(conn, subject_id)?)
}

pub async fn start_engineering_session(
    state: &AppState,
    subject_id: &str,
    slot_id: Option<i64>,
    revisit: bool,
) -> Result<EngineeringLessonView> {
    let (program, concept, dossier, contract, path) = {
        let conn = state.db.0.lock().unwrap();
        let spec = subject(subject_id)?;
        if spec.kind != SubjectKind::Engineering {
            return Err(format!("{subject_id} is not an engineering class"));
        }
        let program = program_row(&conn, subject_id)?;
        if !program.enabled {
            return Err(format!("{} is not enabled", program.label));
        }
        if let Some(active) = active_engineering_for(&conn, subject_id)? {
            return Ok(active);
        }
        if let Some(id) = slot_id {
            validate_slot_start(&conn, subject_id, id, &state.today())?;
        }
        let concept = {
            let drawn = if revisit {
                selection::draw_completed(&conn, &state.today(), subject_id)
            } else {
                crate::domain::classes::next_concept(&conn, subject_id, &state.today())
            }
            .map_err(|error| error.to_string())?;
            drawn.ok_or_else(|| {
                if revisit {
                    format!("no completed modules to revisit in {}", program.label)
                } else {
                    format!(
                        "every module in {} is completed; use revisit or pick another subject",
                        program.label
                    )
                }
            })?
        };
        let dossier =
            mastery::build_dossier(&conn, &state.today(), subject_id).map_err(|e| e.to_string())?;
        (
            program,
            concept,
            dossier,
            spec.prompt,
            crate::domain::classes::current_path(&conn, subject_id).map_err(|e| e.to_string())?,
        )
    };
    let generation_profile = generation_profile(&program);
    let mut contract = contract_with_goal(&program, contract);
    if let Some(path) = &path {
        contract.push_str(&format!("\nACCEPTED PERSONAL PATH: {} ({}) · revision {}. {}\nEarlier material is optional, with no completion or mastery credit. Check relevant prerequisites inside this lesson and offer concise refreshers instead of restarting the course. The required outcome remains: {}.\nPrerequisite advice: {}",
            path.recommendation.entry_label, path.recommendation.route, path.revision, path.recommendation.explanation, path.recommendation.required_outcome,
            serde_json::to_string(&path.recommendation.refreshers).map_err(|e| e.to_string())?));
    }
    let (course, source) = state
        .generator
        .generate_classroom_course(
            crate::generator::CourseRequest {
                title: &concept.title,
                category: &concept.category,
                dossier: &dossier,
                focus: subject_id,
                curriculum: &concept.curriculum,
                budget: crate::generator::LessonBudget::for_minutes(program.session_minutes),
            },
            &contract,
            &generation_profile,
        )
        .await
        .map_err(|error| error.to_string())?;
    insert_engineering_session(
        state,
        &program,
        EngineeringSessionMeta {
            concept_id: concept.id,
            concept_title: &concept.title,
            category: &concept.category,
            slot_id,
            path: path.map(|p| p.reference),
        },
        course,
        source,
    )
}

struct EngineeringSessionMeta<'a> {
    path: Option<crate::domain::classes::PathReference>,
    concept_id: i64,
    concept_title: &'a str,
    category: &'a str,
    slot_id: Option<i64>,
}

/// Map generated exit questions to stored questions, resolving each
/// `correct_answer` string to exactly one choice position. Fails closed: a
/// generated check whose correct answer matches zero or several choices is a
/// validation error, never a silently mis-graded session.
pub fn stored_questions(course: &GeneratedCourse) -> Result<Vec<StoredQuestion>> {
    let questions = course
        .exit_questions
        .iter()
        .enumerate()
        .map(|(index, question)| {
            let matches: Vec<usize> = question
                .choices
                .iter()
                .enumerate()
                .filter(|(_, choice)| choice.trim() == question.correct_answer.trim())
                .map(|(position, _)| position)
                .collect();
            let correct_index = match matches.as_slice() {
                [single] => *single,
                [] => {
                    return Err(format!(
                        "classroom course check {} has no matching correct answer",
                        index + 1
                    ));
                }
                _ => {
                    return Err(format!(
                        "classroom course check {} has an ambiguous correct answer",
                        index + 1
                    ));
                }
            };
            Ok(StoredQuestion {
                id: index + 1,
                prompt: question.prompt.clone(),
                choices: question.choices.clone(),
                correct_index,
                explanation: question.explanation.clone(),
                section: question.section.clone(),
                learning_objective: question.learning_objective.clone(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    if questions.len() != 5 {
        return Err(format!(
            "classroom course produced {} validated checks; expected 5",
            questions.len()
        ));
    }
    Ok(questions)
}

fn insert_engineering_session(
    state: &AppState,
    program: &ProgramRow,
    meta: EngineeringSessionMeta<'_>,
    course: GeneratedCourse,
    source: String,
) -> Result<EngineeringLessonView> {
    let questions = stored_questions(&course)?;
    let stored = StoredEngineeringLesson {
        concept_id: meta.concept_id,
        concept_title: meta.concept_title.into(),
        category: meta.category.into(),
        title: course.title,
        markdown: course.markdown,
        resources: course.resources,
        review_notes: course.review_notes,
        research_note: course.research_note,
        level: standard_level(),
        questions,
        exercise: course.exercise,
        source: source.clone(),
        path: meta.path,
    };
    let payload_json = serde_json::to_string(&stored).map_err(|error| error.to_string())?;
    let conn = state.db.0.lock().unwrap();
    if let Some(active) = active_engineering_for(&conn, &program.subject_id)? {
        return Ok(active);
    }
    conn.execute(
        "INSERT INTO classroom_sessions
            (subject_id, slot_id, session_date, status, title, payload_json,
             agent_used, prompt_version, started_at)
         VALUES (?1, ?2, ?3, 'in_progress', ?4, ?5, ?6, ?7, ?8)",
        params![
            program.subject_id,
            meta.slot_id,
            state.today(),
            stored.title,
            payload_json,
            source,
            format!("{}.{}", program.prompt_profile, program.prompt_version),
            language::now_iso(),
        ],
    )
    .map_err(|error| error.to_string())?;
    engineering_view(&conn, conn.last_insert_rowid())?
        .ok_or_else(|| "new classroom session could not be read".into())
}

#[derive(Debug, Clone, Deserialize)]
pub struct SubmitEngineeringInput {
    pub session_id: i64,
    pub answers: Vec<usize>,
    #[serde(default)]
    pub reflection: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassroomCorrectionView {
    pub question_id: usize,
    pub prompt: String,
    pub selected_answer: String,
    pub correct_answer: String,
    pub correct: bool,
    pub explanation: String,
}

fn lesson_kind() -> String {
    "lesson".into()
}
fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineeringSessionResult {
    pub session_id: String,
    pub subject_id: String,
    pub passed: bool,
    pub score: f64,
    pub corrections: Vec<ClassroomCorrectionView>,
    #[serde(default = "lesson_kind")]
    pub kind: String,
    #[serde(default = "default_true")]
    pub fresh_sample: bool,
}

pub fn submit_engineering_session(
    conn: &Connection,
    input: &SubmitEngineeringInput,
    today: &str,
) -> Result<EngineeringSessionResult> {
    let transaction = conn
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
    let conn = &*transaction;
    let row = conn
        .query_row(
            "SELECT subject_id, status, payload_json FROM classroom_sessions WHERE id = ?1",
            [input.session_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .map_err(|error| error.to_string())?;
    let (subject_id, status, payload_json) = row;
    if status != "in_progress" {
        return Err("this classroom session is already finished".into());
    }
    let stored: StoredEngineeringLesson =
        serde_json::from_str(&payload_json).map_err(|error| error.to_string())?;
    if input.answers.len() != stored.questions.len() {
        return Err("answer every classroom knowledge check".into());
    }
    let mut correct_count = 0;
    let mut corrections = Vec::new();
    for (index, question) in stored.questions.iter().enumerate() {
        let selected = input.answers[index];
        if selected >= question.choices.len() {
            return Err(format!("answer {} is invalid", index + 1));
        }
        let correct = selected == question.correct_index;
        correct_count += usize::from(correct);
        corrections.push(ClassroomCorrectionView {
            question_id: question.id,
            prompt: question.prompt.clone(),
            selected_answer: question.choices[selected].clone(),
            correct_answer: question.choices[question.correct_index].clone(),
            correct,
            explanation: question.explanation.clone(),
        });
    }
    let score = correct_count as f64 / stored.questions.len() as f64;
    for (question, correction) in stored.questions.iter().zip(&corrections) {
        let misconception = if correction.correct {
            String::new()
        } else {
            format!(
                "Selected “{}” instead of “{}”. {}",
                correction.selected_answer, correction.correct_answer, correction.explanation
            )
        };
        conn.execute(
            "INSERT INTO classroom_exit_attempts
                (session_id, concept_id, question_id, section, learning_objective,
                 misconception, correct, attempted_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                input.session_id,
                stored.concept_id,
                question.id as i64,
                question.section,
                question.learning_objective,
                misconception,
                correction.correct as i64,
                language::now_iso(),
            ],
        )
        .map_err(|error| error.to_string())?;
    }
    mastery::record_course_read(conn, stored.concept_id, today).map_err(|e| e.to_string())?;
    mastery::record_quiz_outcome(conn, stored.concept_id, today, score)
        .map_err(|e| e.to_string())?;
    let response_json = serde_json::json!({
        "answers": input.answers,
        "reflection": input.reflection.trim(),
    })
    .to_string();
    conn.execute(
        "UPDATE classroom_sessions
         SET status = 'completed', score = ?2, response_json = ?3,
             completed_at = ?4
         WHERE id = ?1",
        params![input.session_id, score, response_json, language::now_iso(),],
    )
    .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(EngineeringSessionResult {
        session_id: input.session_id.to_string(),
        subject_id,
        passed: score >= 0.8,
        score,
        corrections,
        kind: "lesson".into(),
        fresh_sample: true,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct ActiveClassroomSessionView {
    /// Legacy row ID or shared-runtime `study-…` ID; see `runtime`.
    pub session_id: String,
    pub subject_id: String,
    pub kind: String,
    pub label: String,
    pub title: String,
    pub runtime: String,
    /// Shared-runtime lifecycle, or `in_progress` for legacy rows.
    pub lifecycle: String,
}

pub fn active_sessions(conn: &Connection) -> Result<Vec<ActiveClassroomSessionView>> {
    let mut active = Vec::new();
    let mut stmt = conn
        .prepare(
            "SELECT s.id, s.subject_id, p.label, s.title
             FROM classroom_sessions s
             JOIN classroom_programs p ON p.subject_id = s.subject_id
             WHERE s.status = 'in_progress'
             ORDER BY s.id",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(ActiveClassroomSessionView {
                session_id: row.get::<_, i64>(0)?.to_string(),
                subject_id: row.get(1)?,
                kind: "engineering".into(),
                label: row.get(2)?,
                title: row.get(3)?,
                runtime: "legacy".into(),
                lifecycle: "in_progress".into(),
            })
        })
        .map_err(|error| error.to_string())?;
    for row in rows {
        active.push(row.map_err(|error| error.to_string())?);
    }
    for language in language::active_summaries(conn)? {
        active.push(ActiveClassroomSessionView {
            session_id: language.session_id,
            subject_id: language.language,
            kind: "language".into(),
            label: language.label,
            title: language.title,
            runtime: "legacy".into(),
            lifecycle: "in_progress".into(),
        });
    }
    active.extend(crate::subjects::engineering::active_summaries(conn)?);
    active.extend(crate::subjects::language::active_summaries(conn)?);
    Ok(active)
}

pub fn prompt_contracts_are_isolated() -> Result<()> {
    let mut seen = HashMap::new();
    for spec in subjects() {
        if !spec
            .prompt
            .contains(&format!("PROMPT PROFILE: {}.v", spec.prompt_profile))
        {
            return Err(format!("{} prompt has the wrong version marker", spec.id));
        }
        if seen.insert(spec.prompt, spec.id).is_some() {
            return Err(format!("{} reuses another subject prompt", spec.id));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generator::ExitCheck;

    fn question(correct_answer: &str, choices: &[&str]) -> ExitCheck {
        ExitCheck {
            prompt: "what is it?".into(),
            choices: choices.iter().map(|choice| choice.to_string()).collect(),
            correct_answer: correct_answer.into(),
            explanation: "because".into(),
            section: "## Core mechanics".into(),
            learning_objective: "explain the mechanism".into(),
        }
    }

    fn course(questions: Vec<ExitCheck>) -> GeneratedCourse {
        GeneratedCourse {
            title: "test".into(),
            markdown: String::new(),
            resources: vec![],
            key_takeaways: vec![],
            exit_questions: questions,
            exercise: None,
            review_notes: Vec::new(),
            research_note: None,
        }
    }

    #[test]
    fn stored_questions_resolve_the_correct_choice_position() {
        let stored = stored_questions(&course(vec![
            question("b", &["a", "b", "c"]),
            question("c", &["a", "b", "c"]),
            question("a", &["a", "b", "c"]),
            question("b", &["a", "b", "c"]),
            question("a", &["a", "b", "c"]),
        ]))
        .expect("matching answers must map");
        let positions: Vec<usize> = stored.iter().map(|q| q.correct_index).collect();
        assert_eq!(positions, vec![1, 2, 0, 1, 0]);
    }

    #[test]
    fn stored_questions_trim_before_matching() {
        let stored = stored_questions(&course(vec![
            question(" b ", &["a", "b", "c"]),
            question("c", &["a", "b", "c"]),
            question("a", &["a", "b", "c"]),
            question("b", &["a", "b", "c"]),
            question("a", &["a", "b", "c"]),
        ]))
        .expect("whitespace-padded answers must match");
        assert_eq!(stored[0].correct_index, 1);
    }

    #[test]
    fn stored_questions_fail_closed_when_no_choice_matches() {
        let error = stored_questions(&course(vec![
            question("z", &["a", "b", "c"]),
            question("b", &["a", "b", "c"]),
            question("a", &["a", "b", "c"]),
            question("b", &["a", "b", "c"]),
            question("a", &["a", "b", "c"]),
        ]))
        .expect_err("unmatched answer must be a validation error");
        assert!(
            error.contains("check 1 has no matching correct answer"),
            "{error}"
        );
    }

    #[test]
    fn stored_questions_fail_closed_when_answer_is_ambiguous() {
        let error = stored_questions(&course(vec![
            question("b", &["a", "b", "b"]),
            question("b", &["a", "b", "c"]),
            question("a", &["a", "b", "c"]),
            question("b", &["a", "b", "c"]),
            question("a", &["a", "b", "c"]),
        ]))
        .expect_err("duplicated choice must be a validation error");
        assert!(
            error.contains("check 1 has an ambiguous correct answer"),
            "{error}"
        );
    }

    #[test]
    fn stored_questions_require_exactly_five() {
        let error = stored_questions(&course(vec![
            question("b", &["a", "b", "c"]),
            question("a", &["a", "b", "c"]),
        ]))
        .expect_err("four checks are not a classroom course");
        assert!(error.contains("expected 5"), "{error}");
    }
}
