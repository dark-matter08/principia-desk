//! The learner model: per-concept mastery ledger + the dossier the Teacher
//! reads before every generation call. See docs/TEACHER.md §2.
//!
//! Lifecycle: unseen → introduced → practicing → mastered → maintenance,
//! with struggling/decayed detours. Transitions are computed here, at
//! grading/completion time, from data the app already records.

use crate::db::{DbError, Result};
use rusqlite::{params, Connection};
use serde::Serialize;

/// Mastered requires this score on the current AND running (ema) record...
const MASTER_SCORE: f64 = 0.8;
/// ...across at least this many quiz encounters...
const MASTER_ENCOUNTERS: i64 = 2;
/// ...with at least this many days between the last two encounters.
const MASTER_GAP_DAYS: i64 = 7;
/// Below this, a quiz encounter marks the concept struggling.
const STRUGGLE_SCORE: f64 = 0.5;
/// Spaced-repetition review intervals once mastered.
const REVIEW_INTERVALS: &[i64] = &[7, 21, 60];

#[derive(Debug, Clone, Serialize)]
pub struct Mastery {
    pub concept_id: i64,
    pub state: String,
    pub score_ema: f64,
    pub encounters: i64,
    pub last_seen_date: Option<String>,
    pub last_assessed_date: Option<String>,
    pub next_review_date: Option<String>,
    pub review_interval_days: i64,
    pub teacher_notes: String,
}

fn default_row(concept_id: i64) -> Mastery {
    Mastery {
        concept_id,
        state: "unseen".into(),
        score_ema: 0.0,
        encounters: 0,
        last_seen_date: None,
        last_assessed_date: None,
        next_review_date: None,
        review_interval_days: 7,
        teacher_notes: String::new(),
    }
}

pub fn get(conn: &Connection, concept_id: i64) -> Result<Mastery> {
    let mut stmt = conn.prepare(
        "SELECT concept_id, state, score_ema, encounters, last_seen_date,
                next_review_date, review_interval_days, teacher_notes, last_assessed_date
         FROM mastery WHERE concept_id = ?1",
    )?;
    let mut rows = stmt.query(params![concept_id])?;
    Ok(match rows.next()? {
        Some(r) => Mastery {
            concept_id: r.get(0)?,
            state: r.get(1)?,
            score_ema: r.get(2)?,
            encounters: r.get(3)?,
            last_seen_date: r.get(4)?,
            next_review_date: r.get(5)?,
            review_interval_days: r.get(6)?,
            teacher_notes: r.get(7)?,
            last_assessed_date: r.get(8)?,
        },
        None => default_row(concept_id),
    })
}

fn upsert(conn: &Connection, m: &Mastery) -> Result<()> {
    conn.execute(
        "INSERT INTO mastery (concept_id, state, score_ema, encounters, last_seen_date,
                              next_review_date, review_interval_days, teacher_notes, last_assessed_date)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(concept_id) DO UPDATE SET
            state = excluded.state,
            score_ema = excluded.score_ema,
            encounters = excluded.encounters,
            last_seen_date = excluded.last_seen_date,
            last_assessed_date = excluded.last_assessed_date,
            next_review_date = excluded.next_review_date,
            review_interval_days = excluded.review_interval_days,
            teacher_notes = excluded.teacher_notes",
        params![
            m.concept_id,
            m.state,
            m.score_ema,
            m.encounters,
            m.last_seen_date,
            m.next_review_date,
            m.review_interval_days,
            m.teacher_notes,
            m.last_assessed_date
        ],
    )?;
    Ok(())
}

fn days_between(earlier: &str, later: &str) -> i64 {
    let parse = |s: &str| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d");
    match (parse(earlier), parse(later)) {
        (Ok(a), Ok(b)) => (b - a).num_days(),
        _ => 0,
    }
}

fn add_days(date: &str, days: i64) -> String {
    chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map(|d| {
            (d + chrono::Duration::days(days))
                .format("%Y-%m-%d")
                .to_string()
        })
        .unwrap_or_else(|_| date.to_string())
}

/// Course read to completion: unseen → introduced. Later states unaffected
/// (re-reading a topic never demotes it).
pub fn record_course_read(conn: &Connection, concept_id: i64, date: &str) -> Result<()> {
    let mut m = get(conn, concept_id)?;
    if m.state == "unseen" {
        m.state = "introduced".into();
    }
    m.last_seen_date = Some(date.to_string());
    upsert(conn, &m)
}

/// A quiz encounter for this concept: `score` is the fraction correct of
/// today's questions belonging to it. Drives all state transitions.
pub fn record_quiz_outcome(
    conn: &Connection,
    concept_id: i64,
    date: &str,
    score: f64,
) -> Result<Mastery> {
    let mut m = get(conn, concept_id)?;
    let prev_seen = m.last_assessed_date.clone();
    m.encounters += 1;
    m.score_ema = if m.encounters <= 1 {
        score
    } else {
        0.6 * score + 0.4 * m.score_ema
    };

    let gap_ok = prev_seen
        .as_deref()
        .map(|p| days_between(p, date) >= MASTER_GAP_DAYS)
        .unwrap_or(false);

    m.state = match m.state.as_str() {
        "mastered" | "maintenance" => {
            if score < MASTER_SCORE {
                // Maintenance check failed: knowledge decayed, re-enters practice.
                m.review_interval_days = REVIEW_INTERVALS[0];
                m.next_review_date = None;
                "decayed".into()
            } else if prev_seen.as_deref() == Some(date) {
                // Additional practice today is useful, but is not a later
                // retention check and cannot lengthen the review interval.
                m.state.clone()
            } else {
                // Passed review: advance the spaced-repetition interval.
                let next_interval = REVIEW_INTERVALS
                    .iter()
                    .find(|&&i| i > m.review_interval_days)
                    .copied()
                    .unwrap_or(*REVIEW_INTERVALS.last().unwrap());
                m.review_interval_days = next_interval;
                m.next_review_date = Some(add_days(date, next_interval));
                "maintenance".into()
            }
        }
        _ => {
            if score >= MASTER_SCORE
                && m.score_ema >= MASTER_SCORE
                && m.encounters >= MASTER_ENCOUNTERS
                && gap_ok
            {
                m.review_interval_days = REVIEW_INTERVALS[0];
                m.next_review_date = Some(add_days(date, REVIEW_INTERVALS[0]));
                "mastered".into()
            } else if score < STRUGGLE_SCORE {
                "struggling".into()
            } else {
                "practicing".into()
            }
        }
    };
    m.last_seen_date = Some(date.to_string());
    m.last_assessed_date = Some(date.to_string());
    upsert(conn, &m)?;
    Ok(m)
}

/// Persist the Teacher's observation about the student on this concept
/// (written by the grading call; injected back on the next encounter).
pub fn set_teacher_note(conn: &Connection, concept_id: i64, note: &str) -> Result<()> {
    let note = note.trim();
    if note.is_empty() {
        return Ok(());
    }
    let mut m = get(conn, concept_id)?;
    m.teacher_notes = note.chars().take(280).collect();
    upsert(conn, &m)
}

#[derive(Debug, Clone, Serialize)]
pub struct MasteryEntry {
    pub concept_id: i64,
    pub slug: String,
    pub title: String,
    pub category: String,
    pub state: String,
    pub score_ema: f64,
}

/// Every active concept in a focus track with its mastery state.
pub fn overview(conn: &Connection, focus: &str) -> Result<Vec<MasteryEntry>> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.slug, c.title, c.category,
                COALESCE(m.state, 'unseen'), COALESCE(m.score_ema, 0)
         FROM concepts c LEFT JOIN mastery m ON m.concept_id = c.id
         WHERE c.active = 1 AND c.focus = ?1 ORDER BY c.category, c.title",
    )?;
    let rows = stmt.query_map(params![focus], |r| {
        Ok(MasteryEntry {
            concept_id: r.get(0)?,
            slug: r.get(1)?,
            title: r.get(2)?,
            category: r.get(3)?,
            state: r.get(4)?,
            score_ema: r.get(5)?,
        })
    })?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(DbError::from)
}

pub fn get_profile(conn: &Connection, key: &str) -> Result<Option<String>> {
    let mut stmt = conn.prepare("SELECT value FROM profile WHERE key = ?1")?;
    let mut rows = stmt.query(params![key])?;
    Ok(match rows.next()? {
        Some(r) => Some(r.get(0)?),
        None => None,
    })
}

pub fn set_profile(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO profile (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

/// The learner dossier: ~1 page of markdown compiled fresh from the ledger,
/// prepended to every Teacher call. This is the agent's entire memory.
pub fn build_dossier(conn: &Connection, today: &str, focus: &str) -> Result<String> {
    let days_taught: i64 =
        crate::classroom::completed_lesson_count(conn, focus).map_err(DbError::Invalid)?;
    let streak = crate::db::streak(conn, today).unwrap_or(0);
    let all = overview(conn, focus)?;

    let list =
        |state: &str| -> Vec<&MasteryEntry> { all.iter().filter(|e| e.state == state).collect() };
    let titles = |es: &[&MasteryEntry]| -> String {
        es.iter()
            .map(|e| e.slug.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };

    let mastered: Vec<&MasteryEntry> = all
        .iter()
        .filter(|e| e.state == "mastered" || e.state == "maintenance")
        .collect();
    let practicing = list("practicing");
    let introduced = list("introduced");

    let mut out = String::new();
    out.push_str(&format!(
        "Focus track: {}. Day {} of teaching this student in this track. Current streak: {} day(s).\n",
        crate::focus::label(focus),
        days_taught + 1,
        streak
    ));
    let session_index = days_taught + 1;
    if matches!(session_index, 7 | 14 | 21 | 30) {
        out.push_str(
            "MILESTONE SESSION: the exercise must integrate at least two named earlier concepts or artifacts and produce cumulative evidence for the 30-day outcome.\n",
        );
    }
    if !mastered.is_empty() {
        out.push_str(&format!(
            "MASTERED ({}): {}\n",
            mastered.len(),
            titles(&mastered)
        ));
    }
    let related_slugs: std::collections::HashSet<String> = crate::db::all_concepts(conn, focus)?
        .into_iter()
        .flat_map(|concept| concept.curriculum.related_concepts)
        .collect();
    let mut transferable = Vec::new();
    for other_focus in crate::focus::selectable()
        .into_iter()
        .filter(|candidate| *candidate != focus)
    {
        for entry in overview(conn, other_focus)? {
            if related_slugs.contains(&entry.slug)
                && matches!(entry.state.as_str(), "mastered" | "maintenance")
            {
                transferable.push(format!(
                    "{} (mastered in {})",
                    entry.slug,
                    crate::focus::label(other_focus)
                ));
            }
        }
    }
    if !transferable.is_empty() {
        transferable.sort();
        transferable.dedup();
        out.push_str(&format!(
            "TRANSFERABLE CROSS-TRACK MASTERY: {}\n",
            transferable.join(", ")
        ));
    }

    // Struggling + decayed carry their notes — this is what the Teacher must address.
    let mut needs_work: Vec<String> = Vec::new();
    for e in all
        .iter()
        .filter(|e| e.state == "struggling" || e.state == "decayed")
    {
        let m = get(conn, e.concept_id)?;
        let mut line = format!("{} ({}, score {:.0}%", e.slug, e.state, m.score_ema * 100.0);
        if !m.teacher_notes.is_empty() {
            line.push_str(&format!(", notes: \"{}\"", m.teacher_notes));
        }
        line.push(')');
        needs_work.push(line);
    }
    if !needs_work.is_empty() {
        out.push_str(&format!(
            "STRUGGLING ({}): {}\n",
            needs_work.len(),
            needs_work.join("; ")
        ));
    }

    // Same-day exit checks expose misconceptions before the next-day quiz.
    // Keep the most recent distinct misses in the dossier so future courses
    // can repair the actual mental model rather than only seeing a score.
    let mut stmt = conn.prepare(
        "SELECT session_date, slug, section, learning_objective, misconception
         FROM (
             SELECT co.session_date, c.slug, ea.section, ea.learning_objective,
                    ea.misconception, ea.created_at AS attempted_at
             FROM exit_attempts ea
             JOIN courses co ON co.id = ea.course_id
             JOIN concepts c ON c.id = co.concept_id
             WHERE c.focus = ?1 AND ea.correct = 0
             UNION ALL
             SELECT cs.session_date, c.slug, cea.section, cea.learning_objective,
                    cea.misconception, cea.attempted_at
             FROM classroom_exit_attempts cea
             JOIN classroom_sessions cs ON cs.id = cea.session_id
             JOIN concepts c ON c.id = cea.concept_id
             WHERE cs.subject_id = ?1 AND cea.correct = 0
             UNION ALL
             SELECT json_extract(ss.context_json, '$.selection.service_date'), c.slug,
                    cea.section, cea.learning_objective, cea.misconception, cea.attempted_at
             FROM classroom_exit_attempts cea
             JOIN study_sessions ss ON ss.id = cea.study_session_id
             JOIN classes cl ON cl.id = ss.class_id
             JOIN concepts c ON c.id = cea.concept_id
             WHERE cl.course_id = ?1 AND cea.correct = 0
         )
         ORDER BY attempted_at DESC
         LIMIT 20",
    )?;
    let exit_misses = stmt.query_map(params![focus], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
        ))
    })?;
    let mut seen = std::collections::HashSet::new();
    let mut recent_misses = Vec::new();
    for miss in exit_misses {
        let (date, slug, section, objective, misconception) = miss?;
        let key = (slug.clone(), objective.clone());
        if !seen.insert(key) {
            continue;
        }
        let area = match (section.trim(), objective.trim()) {
            ("", "") => "unspecified learning objective".to_string(),
            ("", objective) => objective.to_string(),
            (section, "") => section.to_string(),
            (section, objective) => format!("{section}: {objective}"),
        };
        let misconception = misconception.trim();
        let detail = if misconception.is_empty() {
            area
        } else {
            format!("{area}; misconception: {misconception}")
        };
        recent_misses.push(format!("{date} {slug}: {detail}"));
        if recent_misses.len() == 6 {
            break;
        }
    }
    if !recent_misses.is_empty() {
        out.push_str("RECENT EXIT-CHECK MISCONCEPTIONS:\n");
        for miss in recent_misses {
            out.push_str(&format!("  {miss}\n"));
        }
    }

    if !practicing.is_empty() {
        out.push_str(&format!(
            "PRACTICING ({}): {}\n",
            practicing.len(),
            titles(&practicing)
        ));
    }
    if !introduced.is_empty() {
        out.push_str(&format!(
            "INTRODUCED, NOT YET QUIZZED ({}): {}\n",
            introduced.len(),
            titles(&introduced)
        ));
    }

    // Due for spaced review.
    let mut stmt = conn.prepare(
        "SELECT c.slug, m.last_seen_date FROM mastery m JOIN concepts c ON c.id = m.concept_id
         WHERE c.focus = ?2 AND m.next_review_date IS NOT NULL AND m.next_review_date <= ?1",
    )?;
    let due: Vec<String> = stmt
        .query_map(params![today, focus], |r| {
            let slug: String = r.get(0)?;
            let last: Option<String> = r.get(1)?;
            Ok(match last {
                Some(l) => format!("{slug} (last seen {l})"),
                None => slug,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if !due.is_empty() {
        out.push_str(&format!(
            "DUE FOR REVIEW ({}): {}\n",
            due.len(),
            due.join(", ")
        ));
    }

    // Recent courses give continuity ("as we saw when we covered X").
    let mut stmt = conn.prepare(
        "SELECT session_date, title FROM (
             SELECT co.session_date, c.title, co.generated_at AS occurred_at
             FROM courses co JOIN concepts c ON c.id = co.concept_id
             WHERE c.focus = ?1
             UNION ALL
             SELECT cs.session_date, cs.title, COALESCE(cs.completed_at, cs.started_at)
             FROM classroom_sessions cs
             WHERE cs.subject_id = ?1 AND cs.status = 'completed'
         )
         ORDER BY occurred_at DESC LIMIT 5",
    )?;
    let recent: Vec<String> = stmt
        .query_map(params![focus], |r| {
            Ok(format!(
                "{}: {}",
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if !recent.is_empty() {
        out.push_str("RECENT COURSES:\n");
        for l in recent {
            out.push_str(&format!("  {l}\n"));
        }
    }
    let completed_exercises: i64 = conn.query_row(
        "SELECT
            (SELECT COUNT(*)
             FROM exercise_drafts ed
             JOIN courses co ON co.id = ed.course_id
             JOIN concepts c ON c.id = co.concept_id
             WHERE c.focus = ?1 AND ed.completed = 1)
          + (SELECT COUNT(*)
             FROM exercise_drafts ed
             JOIN classroom_sessions cs ON cs.id = ed.classroom_session_id
             WHERE cs.subject_id = ?1 AND ed.completed = 1)",
        params![focus],
        |row| row.get(0),
    )?;
    out.push_str(&format!(
        "PRACTICAL WORK: {completed_exercises} exercise(s) completed with evidence.\n"
    ));
    let mut stmt = conn.prepare(
        "SELECT session_date, slug, reflection FROM (
             SELECT co.session_date, c.slug, ed.reflection, ed.updated_at
             FROM exercise_drafts ed
             JOIN courses co ON co.id = ed.course_id
             JOIN concepts c ON c.id = co.concept_id
             WHERE c.focus = ?1 AND ed.completed = 1 AND trim(ed.reflection) <> ''
             UNION ALL
             SELECT cs.session_date, c.slug, ed.reflection, ed.updated_at
             FROM exercise_drafts ed
             JOIN classroom_sessions cs ON cs.id = ed.classroom_session_id
             JOIN concepts c ON c.id = CAST(json_extract(cs.payload_json, '$.concept_id') AS INTEGER)
             WHERE cs.subject_id = ?1 AND ed.completed = 1
               AND trim(ed.reflection) <> ''
         )
         ORDER BY updated_at DESC
         LIMIT 3",
    )?;
    let evidence = stmt
        .query_map(params![focus], |row| {
            Ok(format!(
                "{} {}: {}",
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if !evidence.is_empty() {
        out.push_str("RECENT PRACTICE EVIDENCE:\n");
        for item in evidence {
            out.push_str(&format!("  {item}\n"));
        }
    }
    if let Ok(Some(weak)) = get_profile(conn, "learning_notes") {
        out.push_str(&format!("PROFILE NOTES: {weak}\n"));
    }
    Ok(out)
}
