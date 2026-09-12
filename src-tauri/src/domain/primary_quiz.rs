//! Temporary primary-loop adapter. Dates remain compatibility lookup keys here;
//! every published quiz exposes a stable, immutable shared-runtime round ID.
use super::assessments::{self, Item, Owner, Purpose, Response, ResponseStatus, Round, RoundId};
use crate::db::{self, DbError, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::collections::HashMap;

fn legacy(conn: &Connection, prefix: &str, date: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT value FROM legacy_assessment_config WHERE key = ?1",
            [format!("{prefix}:{date}")],
            |r| r.get(0),
        )
        .optional()?)
}
pub fn current(conn: &Connection, date: &str) -> Result<Option<Round>> {
    Ok(
        assessments::latest(conn, &Owner::LegacyPrimary(date.into()), Purpose::Retrieval)?
            .and_then(|attempt| attempt.rounds.last().cloned()),
    )
}
pub fn questions(round: &Round) -> Result<Vec<db::Question>> {
    round
        .items
        .iter()
        .map(|item| Ok(serde_json::from_value(item.body.clone())?))
        .collect()
}
pub fn frozen_questions(conn: &Connection, date: &str) -> Result<Option<Vec<db::Question>>> {
    if let Some(round) = current(conn, date)? {
        return Ok(Some(questions(&round)?));
    }
    if let Some(raw) = legacy(conn, "quiz_round", date)? {
        let stored: Vec<db::Question> = serde_json::from_str(&raw).map_err(|_| DbError::Invalid("The older quiz snapshot has an unknown format. Its original questions and answers are preserved; automatic replacement is disabled.".into()))?;
        return Ok(Some(questions(&freeze(conn, date, &stored)?)?));
    }
    Ok(None)
}
pub fn freeze(conn: &Connection, date: &str, questions: &[db::Question]) -> Result<Round> {
    let tx = conn.unchecked_transaction()?;
    if let Some(round) = current(&tx, date)? {
        return Ok(round);
    }
    let pending: HashMap<String, String> = legacy(&tx, "pending_answers", date)?
        .map(|raw| serde_json::from_str(&raw))
        .transpose()?
        .unwrap_or_default();
    let old_result: Option<Value> = legacy(&tx, "quiz_result", date)?
        .map(|raw| serde_json::from_str(&raw))
        .transpose()?;
    let items = questions
        .iter()
        .map(|question| {
            Ok(Item {
                id: question.id.to_string(),
                body: serde_json::to_value(question)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let focus = db::get_session(&tx, date)?
        .map(|s| s.focus)
        .unwrap_or_default();
    let owner = Owner::LegacyPrimary(date.into());
    let unknown = pending
        .keys()
        .filter(|id| !items.iter().any(|item| &item.id == *id))
        .count();
    let context = json!({"legacy_session_date":date,"focus":focus,"content_provenance":"stored_primary_questions","curriculum_version":null,"legacy_unmatched_answers":unknown});
    let mut round = assessments::start(
        &tx,
        &owner,
        Purpose::Retrieval,
        &context,
        "primary-retrieval-v1",
        &items,
    )?;
    for item in &items {
        let answer = old_result
            .as_ref()
            .and_then(|result| result["items"].as_array())
            .and_then(|rows| {
                rows.iter().find(|row| {
                    row["question_id"]
                        .as_i64()
                        .map(|id| id.to_string())
                        .as_deref()
                        == Some(&item.id)
                })
            })
            .and_then(|row| row["user_answer"].as_str())
            .map(String::from)
            .or_else(|| pending.get(&item.id).cloned());
        if let Some(answer) = answer {
            let status = if answer.trim().is_empty() && old_result.is_none() {
                ResponseStatus::Draft
            } else {
                ResponseStatus::Answered
            };
            round = assessments::save_response(
                &tx,
                &owner,
                &round.id,
                round.revision,
                &item.id,
                Response { answer, status },
            )?;
        }
    }
    if let Some(result) = old_result {
        assessments::submit_in_transaction(&tx, &owner, &round, &result, true)?;
    } else if items.is_empty() {
        assessments::submit_in_transaction(
            &tx,
            &owner,
            &round,
            &json!({"items":[],"score":1.0,"self_assess":false,"disposition":"no_retrieval_due"}),
            true,
        )?;
    }
    tx.execute("INSERT INTO legacy_crosswalk(legacy_table, legacy_key, entity_kind, entity_id, imported_at) VALUES ('primary_quiz', ?1, 'assessment_round', ?2, ?3)",
        params![date, round.id.0, chrono::Utc::now().to_rfc3339()])?;
    let round = assessments::round(&tx, &round.id)?;
    tx.commit()?;
    Ok(round)
}
pub fn pending(round: &Round) -> HashMap<i64, String> {
    round
        .responses
        .iter()
        .filter_map(|(id, response)| id.parse().ok().map(|id| (id, response.answer.clone())))
        .collect()
}
pub fn result(conn: &Connection, date: &str) -> Result<Option<Value>> {
    if let Some(round) = current(conn, date)? {
        return Ok(round.submission.map(|submission| submission.result));
    }
    legacy(conn, "quiz_result", date)?
        .map(|json| Ok(serde_json::from_str(&json)?))
        .transpose()
}
pub fn checked_round(conn: &Connection, date: &str, id: &RoundId) -> Result<Round> {
    let round = current(conn, date)?.ok_or_else(|| {
        DbError::Invalid("Open the quiz before saving or submitting answers.".into())
    })?;
    if round.id != *id {
        return Err(DbError::Invalid(
            "This is no longer the displayed quiz round; reload the saved session.".into(),
        ));
    }
    Ok(round)
}
pub fn save_answer(
    conn: &Connection,
    date: &str,
    id: &RoundId,
    expected_revision: u32,
    question_id: i64,
    answer: String,
    confirmed: bool,
) -> Result<Round> {
    let tx = conn.unchecked_transaction()?;
    let session = db::get_session(&tx, date)?
        .ok_or_else(|| DbError::Invalid("No active study session.".into()))?;
    if session.status != "in_progress" || session.current_step != "quiz" {
        return Err(DbError::Invalid(
            "This session is not awaiting quiz answers.".into(),
        ));
    }
    let round = checked_round(&tx, date, id)?;
    let question = questions(&round)?
        .into_iter()
        .find(|q| q.id == question_id)
        .ok_or_else(|| {
            DbError::Invalid("Question does not belong to the displayed round.".into())
        })?;
    if confirmed && answer.trim().is_empty() {
        return Err(DbError::Invalid(
            "Enter an answer before continuing.".into(),
        ));
    }
    if question.kind == "mcq" && !answer.is_empty() {
        let choices: Vec<String> =
            serde_json::from_str(question.choices_json.as_deref().unwrap_or("[]"))?;
        if !choices.contains(&answer) {
            return Err(DbError::Invalid(
                "Choose one of the displayed answers.".into(),
            ));
        }
    }
    let saved = assessments::save_response(
        &tx,
        &Owner::LegacyPrimary(date.into()),
        id,
        expected_revision,
        &question_id.to_string(),
        Response {
            answer,
            status: if confirmed {
                ResponseStatus::Answered
            } else {
                ResponseStatus::Draft
            },
        },
    )?;
    tx.commit()?;
    Ok(saved)
}
