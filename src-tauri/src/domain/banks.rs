//! Practice questions: extra cited questions the tutor writes for a class,
//! stage by stage, that the learner can read and try at any time. They are
//! kept apart from the bank the placement check and the unit challenges
//! sample, so seeing them spoils nothing. One row per class in
//! `course_banks`, in the diagnostic bank's shape.

use crate::catalog;
use crate::db::{DbError, Result};
use crate::domain::placement::{self, Bank, Question};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

fn invalid(message: impl Into<String>) -> DbError {
    DbError::Invalid(message.into())
}

/// The practice questions of a class, for the Curriculum tab.
#[derive(Debug, Clone, Serialize)]
pub struct PracticeView {
    pub course_id: String,
    /// Every question, voided ones included and marked, so a disputed key
    /// stays visible as such.
    pub questions: Vec<Question>,
    /// Usable questions per stage, in stage order.
    pub per_stage: Vec<StageCount>,
    pub usable: usize,
    pub written_at: Option<String>,
    /// The bank the checks sample, described without its questions.
    pub checks_bank: ChecksBank,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StageCount {
    pub stage: String,
    pub label: String,
    pub count: usize,
}

/// What the placement check and unit challenges draw on: how many
/// questions and where from, never the questions themselves.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ChecksBank {
    /// `bundled`, `written` (a learner's own class) or `none`.
    pub source: String,
    pub questions: usize,
}

fn stored(conn: &Connection, course_id: &str) -> Result<Option<(Bank, String)>> {
    let row: Option<(String, String)> = conn
        .query_row(
            "SELECT bank_json, written_at FROM course_banks WHERE course_id=?1",
            [course_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    match row {
        Some((json, at)) => Ok(Some((serde_json::from_str(&json)?, at))),
        None => Ok(None),
    }
}

fn store(conn: &Connection, course_id: &str, bank: &Bank) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO course_banks (course_id, bank_json, written_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(course_id) DO UPDATE SET bank_json=excluded.bank_json, written_at=excluded.written_at",
        params![course_id, serde_json::to_string(bank)?, now],
    )?;
    Ok(())
}

/// Add questions the tutor wrote. One that repeats a prompt already held
/// is left out, so writing a stage again only adds what is new.
pub fn add(conn: &Connection, course_id: &str, fresh: Vec<Question>) -> Result<PracticeView> {
    if catalog::course(course_id).is_none() {
        return Err(invalid("no such course"));
    }
    let mut bank = stored(conn, course_id)?
        .map(|(bank, _)| bank)
        .unwrap_or_else(|| Bank {
            course_id: course_id.into(),
            version: "practice".into(),
            estimated_minutes: 0,
            scope_note: "Extra questions to practise with, apart from the ones the checks draw on."
                .into(),
            questions: Vec::new(),
        });
    let known: std::collections::HashSet<String> = bank
        .questions
        .iter()
        .map(|q| q.prompt.trim().to_lowercase())
        .collect();
    let mut added = 0;
    for question in fresh {
        if known.contains(&question.prompt.trim().to_lowercase()) {
            continue;
        }
        bank.questions.push(question);
        added += 1;
    }
    if added == 0 {
        return Err(invalid(
            "every question the tutor wrote repeats one already held",
        ));
    }
    store(conn, course_id, &bank)?;
    view(conn, course_id)
}

/// A learner disputes a key: the question stays listed as disputed and is
/// left out of the count.
pub fn void_question(
    conn: &Connection,
    course_id: &str,
    question_id: &str,
    reason: &str,
) -> Result<PracticeView> {
    let Some((mut bank, _)) = stored(conn, course_id)? else {
        return Err(invalid("this class has no practice questions"));
    };
    let question = bank
        .questions
        .iter_mut()
        .find(|q| q.id == question_id)
        .ok_or_else(|| invalid("that question is not among the practice questions"))?;
    question.voided = true;
    question.void_reason = reason.trim().chars().take(400).collect();
    store(conn, course_id, &bank)?;
    view(conn, course_id)
}

/// Drop every practice question of a class.
pub fn clear(conn: &Connection, course_id: &str) -> Result<PracticeView> {
    conn.execute("DELETE FROM course_banks WHERE course_id=?1", [course_id])?;
    view(conn, course_id)
}

pub fn view(conn: &Connection, course_id: &str) -> Result<PracticeView> {
    let course = catalog::course(course_id).ok_or_else(|| invalid("no such course"))?;
    let (questions, written_at) = match stored(conn, course_id)? {
        Some((bank, at)) => (bank.questions, Some(at)),
        None => (Vec::new(), None),
    };
    let per_stage = course
        .entry_points
        .iter()
        .map(|entry| StageCount {
            stage: entry.id.into(),
            label: entry.label.into(),
            count: questions
                .iter()
                .filter(|q| !q.voided && q.entry_point == entry.id)
                .count(),
        })
        .collect();
    let checks_bank = if catalog::is_custom(course_id) {
        match catalog::custom_bank(course_id)
            .and_then(|value| serde_json::from_value::<Bank>(value).ok())
        {
            Some(bank) => ChecksBank {
                source: "written".into(),
                questions: bank.questions.iter().filter(|q| !q.voided).count(),
            },
            None => ChecksBank {
                source: "none".into(),
                questions: 0,
            },
        }
    } else {
        match placement::authored_bank(course_id) {
            Some(bank) => ChecksBank {
                source: "bundled".into(),
                questions: bank.questions.len(),
            },
            None => ChecksBank {
                source: "none".into(),
                questions: 0,
            },
        }
    };
    Ok(PracticeView {
        course_id: course_id.into(),
        usable: questions.iter().filter(|q| !q.voided).count(),
        questions,
        per_stage,
        written_at,
        checks_bank,
    })
}
