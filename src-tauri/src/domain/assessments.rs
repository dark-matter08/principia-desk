//! Authoritative frozen rounds, resumable answer drafts and immutable submissions.
//! Subject adapters own question/rubric interpretation and evidence projections.
//! This runtime never starts a lesson, consumes an occurrence or changes focus.
use crate::db::{DbError, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AttemptId(pub String);
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RoundId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum Owner {
    /// Compatibility adapter only; replaced with shared session ownership at cutover.
    LegacyPrimary(String),
    EnrollmentDraft(String),
    Class(String),
    StudySession(String),
}
impl Owner {
    fn parts(&self) -> (&str, &str) {
        match self {
            Self::LegacyPrimary(key) => ("legacy_primary", key),
            Self::EnrollmentDraft(key) => ("enrollment_draft", key),
            Self::Class(key) => ("class", key),
            Self::StudySession(key) => ("study_session", key),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Purpose {
    Retrieval,
    Diagnostic,
    UnitChallenge,
    ExitCheck,
    Lesson,
}
impl Purpose {
    fn key(self) -> &'static str {
        match self {
            Self::Retrieval => "retrieval",
            Self::Diagnostic => "diagnostic",
            Self::UnitChallenge => "unit_challenge",
            Self::ExitCheck => "exit_check",
            Self::Lesson => "lesson",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    /// Adapter-authored immutable question, answer key, coverage and rubric.
    /// Never deserialize this body from an untrusted learner request.
    pub body: Value,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseStatus {
    Draft,
    Answered,
    Skipped,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Response {
    pub answer: String,
    pub status: ResponseStatus,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Submission {
    pub answer_revision: u32,
    pub responses: BTreeMap<String, Response>,
    pub result: Value,
    pub submitted_at: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Round {
    pub id: RoundId,
    pub attempt_id: AttemptId,
    pub ordinal: u32,
    pub rubric_version: String,
    pub items: Vec<Item>,
    pub revision: u32,
    pub responses: BTreeMap<String, Response>,
    pub submission: Option<Submission>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attempt {
    pub id: AttemptId,
    pub owner: Owner,
    pub purpose: Purpose,
    pub context: Value,
    pub status: String,
    pub rounds: Vec<Round>,
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}
fn new_id(conn: &Connection, prefix: &str) -> Result<String> {
    Ok(
        conn.query_row("SELECT ?1 || lower(hex(randomblob(16)))", [prefix], |row| {
            row.get(0)
        })?,
    )
}
fn invalid(message: &str) -> DbError {
    DbError::Invalid(message.into())
}

pub fn round(conn: &Connection, id: &RoundId) -> Result<Round> {
    let (attempt, ordinal, rubric, items, revision, responses): (String, u32, String, String, u32, String) = conn.query_row(
        "SELECT r.attempt_id, r.ordinal, r.rubric_version, r.items_json, w.revision, w.responses_json FROM assessment_rounds r JOIN assessment_work w ON w.round_id = r.id WHERE r.id = ?1",
        [&id.0], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))?;
    let stored: Option<(u32, String, String, String)> = conn.query_row(
        "SELECT answer_revision, responses_json, result_json, submitted_at FROM assessment_submissions WHERE round_id = ?1",
        [&id.0], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).optional()?;
    Ok(Round {
        id: id.clone(),
        attempt_id: AttemptId(attempt),
        ordinal,
        rubric_version: rubric,
        items: serde_json::from_str(&items)?,
        revision,
        responses: serde_json::from_str(&responses)?,
        submission: stored
            .map(
                |(answer_revision, responses, result, submitted_at)| -> Result<_> {
                    Ok(Submission {
                        answer_revision,
                        responses: serde_json::from_str(&responses)?,
                        result: serde_json::from_str(&result)?,
                        submitted_at,
                    })
                },
            )
            .transpose()?,
    })
}

pub fn latest(conn: &Connection, owner: &Owner, purpose: Purpose) -> Result<Option<Attempt>> {
    let (kind, key) = owner.parts();
    let row: Option<(String, String, String)> = conn.query_row(
        "SELECT id, context_json, status FROM assessment_attempts WHERE owner_kind = ?1 AND owner_key = ?2 AND purpose = ?3 ORDER BY rowid DESC LIMIT 1",
        params![kind, key, purpose.key()], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).optional()?;
    let Some((id, context, status)) = row else {
        return Ok(None);
    };
    let round_ids = conn
        .prepare("SELECT id FROM assessment_rounds WHERE attempt_id = ?1 ORDER BY ordinal")?
        .query_map([&id], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(Some(Attempt {
        id: AttemptId(id),
        owner: owner.clone(),
        purpose,
        context: serde_json::from_str(&context)?,
        status,
        rounds: round_ids
            .into_iter()
            .map(|id| round(conn, &RoundId(id)))
            .collect::<Result<_>>()?,
    }))
}

fn validate_owner(conn: &Connection, owner: &Owner) -> Result<()> {
    let exists: bool = match owner {
        Owner::LegacyPrimary(date) => conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sessions WHERE date = ?1)",
            [date],
            |r| r.get(0),
        )?,
        Owner::EnrollmentDraft(id) => conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM enrollment_drafts WHERE id = ?1 AND status = 'draft')",
            [id],
            |r| r.get(0),
        )?,
        Owner::Class(id) => conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM classes WHERE id = ?1)",
            [id],
            |r| r.get(0),
        )?,
        Owner::StudySession(id) => conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM study_sessions WHERE id=?1 AND status IN ('ready','active','paused'))",
            [id],
            |r| r.get(0),
        )?,
    };
    if !exists {
        return Err(invalid("assessment owner is unavailable"));
    }
    Ok(())
}

/// Caller supplies authoritative, frozen context. Starting twice resumes the
/// active attempt only when its context and first round are unchanged.
pub fn start(
    conn: &Connection,
    owner: &Owner,
    purpose: Purpose,
    context: &Value,
    rubric: &str,
    items: &[Item],
) -> Result<Round> {
    if conn.is_autocommit() {
        let tx = conn.unchecked_transaction()?;
        let round = start(&tx, owner, purpose, context, rubric, items)?;
        tx.commit()?;
        return Ok(round);
    }
    validate_owner(conn, owner)?;
    if let Some(existing) = latest(conn, owner, purpose)?.filter(|a| a.status == "active") {
        let first = existing
            .rounds
            .first()
            .ok_or_else(|| invalid("assessment has no frozen round"))?;
        if existing.context != *context || first.rubric_version != rubric || first.items != items {
            return Err(invalid(
                "an assessment is already in progress; resume its frozen version",
            ));
        }
        return Ok(existing.rounds.last().unwrap().clone());
    }
    let id = AttemptId(new_id(conn, "assessment-")?);
    let (kind, key) = owner.parts();
    conn.execute("INSERT INTO assessment_attempts(id, owner_kind, owner_key, purpose, context_json, status, created_at) VALUES (?1, ?2, ?3, ?4, ?5, 'active', ?6)",
        params![id.0, kind, key, purpose.key(), serde_json::to_string(context)?, now()])?;
    append_round_in_transaction(conn, &id, rubric, items)
}

fn append_round_in_transaction(
    conn: &Connection,
    attempt: &AttemptId,
    rubric: &str,
    items: &[Item],
) -> Result<Round> {
    if rubric.trim().is_empty() || items.len() > 256 {
        return Err(invalid("invalid assessment round definition"));
    }
    let mut ids = HashSet::new();
    if items
        .iter()
        .any(|item| item.id.is_empty() || item.id.len() > 200 || !ids.insert(&item.id))
    {
        return Err(invalid("assessment item IDs must be nonempty and unique"));
    }
    let status: String = conn.query_row(
        "SELECT status FROM assessment_attempts WHERE id = ?1",
        [&attempt.0],
        |r| r.get(0),
    )?;
    if status != "active" {
        return Err(invalid("assessment is already terminal"));
    }
    let previous: Option<(u32, bool)> = conn.query_row(
        "SELECT ordinal, EXISTS(SELECT 1 FROM assessment_submissions s WHERE s.round_id = r.id) FROM assessment_rounds r WHERE attempt_id = ?1 ORDER BY ordinal DESC LIMIT 1",
        [&attempt.0], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
    if previous.is_some_and(|(_, submitted)| !submitted) {
        return Err(invalid(
            "submit the current round before creating a follow-up",
        ));
    }
    let id = RoundId(new_id(conn, "round-")?);
    conn.execute("INSERT INTO assessment_rounds(id, attempt_id, ordinal, rubric_version, items_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id.0, attempt.0, previous.map_or(1, |(n, _)| n + 1), rubric, serde_json::to_string(items)?, now()])?;
    conn.execute(
        "INSERT INTO assessment_work VALUES (?1, 0, '{}', ?2)",
        params![id.0, now()],
    )?;
    round(conn, &id)
}

pub fn append_round(
    conn: &Connection,
    attempt: &AttemptId,
    rubric: &str,
    items: &[Item],
) -> Result<Round> {
    if !conn.is_autocommit() {
        return append_round_in_transaction(conn, attempt, rubric, items);
    }
    let tx = conn.unchecked_transaction()?;
    let round = append_round_in_transaction(&tx, attempt, rubric, items)?;
    tx.commit()?;
    Ok(round)
}

/// Finish after the learner declines optional follow-up work. The submitted
/// round remains immutable; only the active attempt lifecycle changes.
pub fn finish_attempt(conn: &Connection, owner: &Owner, id: &AttemptId) -> Result<()> {
    if conn.is_autocommit() {
        let tx = conn.unchecked_transaction()?;
        finish_attempt(&tx, owner, id)?;
        tx.commit()?;
        return Ok(());
    }
    let (kind, key) = owner.parts();
    let status: String = conn.query_row(
        "SELECT status FROM assessment_attempts WHERE id = ?1 AND owner_kind = ?2 AND owner_key = ?3",
        params![id.0, kind, key], |r| r.get(0))?;
    if status == "completed" {
        return Ok(());
    }
    if status != "active" {
        return Err(invalid("assessment is no longer active"));
    }
    let submitted: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM assessment_submissions WHERE round_id = (SELECT id FROM assessment_rounds WHERE attempt_id = ?1 ORDER BY ordinal DESC LIMIT 1))",
        [&id.0], |r| r.get(0))?;
    if !submitted {
        return Err(invalid("submit the current round before finishing"));
    }
    conn.execute(
        "UPDATE assessment_attempts SET status = 'completed', finished_at = ?2 WHERE id = ?1",
        params![id.0, now()],
    )?;
    Ok(())
}

fn check_owner_and_status(conn: &Connection, round: &Round, owner: &Owner) -> Result<()> {
    if matches!(owner, Owner::StudySession(_)) {
        validate_owner(conn, owner)?;
    }
    let (kind, key) = owner.parts();
    let valid: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM assessment_attempts WHERE id = ?1 AND owner_kind = ?2 AND owner_key = ?3 AND status = 'active')",
        params![round.attempt_id.0, kind, key], |r| r.get(0))?;
    if !valid {
        return Err(invalid(
            "assessment owner changed or the attempt is no longer active",
        ));
    }
    Ok(())
}

pub fn save_response(
    conn: &Connection,
    owner: &Owner,
    id: &RoundId,
    expected_revision: u32,
    item: &str,
    response: Response,
) -> Result<Round> {
    if response.answer.len() > 65_536 {
        return Err(invalid("assessment answer exceeds 64 KiB"));
    }
    if conn.is_autocommit() {
        let tx = conn.unchecked_transaction()?;
        let round = save_response(&tx, owner, id, expected_revision, item, response)?;
        tx.commit()?;
        return Ok(round);
    }
    let mut current = round(conn, id)?;
    check_owner_and_status(conn, &current, owner)?;
    if current.submission.is_some() {
        return Err(invalid("this assessment round is already submitted"));
    }
    if !current.items.iter().any(|q| q.id == item) {
        return Err(invalid("question does not belong to this frozen round"));
    }
    // Lost-response retries are safe, including when a later item has been saved.
    if current.responses.get(item) == Some(&response) {
        return Ok(current);
    }
    if current.revision != expected_revision {
        return Err(invalid(
            "assessment answers changed; reload the saved round before editing",
        ));
    }
    current.responses.insert(item.into(), response);
    conn.execute("UPDATE assessment_work SET revision = revision + 1, responses_json = ?2, updated_at = ?3 WHERE round_id = ?1 AND revision = ?4",
        params![id.0, serde_json::to_string(&current.responses)?, now(), expected_revision])?;
    let updated = round(conn, id)?;
    Ok(updated)
}

/// Join the adapter's existing transaction so result, evidence and lifecycle
/// changes commit together. Returns the first result on an identical retry.
pub fn submit_in_transaction(
    conn: &Connection,
    owner: &Owner,
    expected: &Round,
    result: &Value,
    complete_attempt: bool,
) -> Result<Submission> {
    if conn.is_autocommit() {
        return Err(invalid("assessment submission requires a transaction"));
    }
    let current = round(conn, &expected.id)?;
    if current.revision != expected.revision
        || current.responses != expected.responses
        || current.items != expected.items
    {
        return Err(invalid(
            "assessment answers or round changed while grading; submit the saved answers again",
        ));
    }
    let (kind, key) = owner.parts();
    let owns: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM assessment_attempts WHERE id = ?1 AND owner_kind = ?2 AND owner_key = ?3)",
        params![current.attempt_id.0, kind, key], |r| r.get(0))?;
    if !owns {
        return Err(invalid("assessment belongs to another owner"));
    }
    if let Some(submission) = current.submission {
        return Ok(submission);
    }
    check_owner_and_status(conn, &current, owner)?;
    if current.items.iter().any(|item| {
        current
            .responses
            .get(&item.id)
            .is_none_or(|response| response.status == ResponseStatus::Draft)
    }) {
        return Err(invalid(
            "answer or explicitly skip every item before submitting",
        ));
    }
    let submitted_at = now();
    conn.execute(
        "INSERT INTO assessment_submissions VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            current.id.0,
            current.revision,
            serde_json::to_string(&current.responses)?,
            serde_json::to_string(result)?,
            submitted_at
        ],
    )?;
    if complete_attempt {
        conn.execute(
            "UPDATE assessment_attempts SET status = 'completed', finished_at = ?2 WHERE id = ?1",
            params![current.attempt_id.0, submitted_at],
        )?;
    }
    Ok(Submission {
        answer_revision: current.revision,
        responses: current.responses,
        result: result.clone(),
        submitted_at,
    })
}
