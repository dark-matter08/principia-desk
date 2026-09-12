//! Unit challenges: class-owned assessment attempts that sample one unit's
//! entry questions. Demonstrated samples can check the learner out of exactly
//! those topics. Nothing here consumes a study session, an appointment or a
//! focus lock, and nothing awards completion, mastery or a streak.
use super::{
    assessments::{
        self, Attempt, AttemptId, Item, Owner, Purpose, Response, ResponseStatus, RoundId,
    },
    classes::{self, AcceptedPath, PathChange, RevisePath},
    enrollment,
    placement::{
        self, CriterionEvidence, CriterionResult, PathTopic, Question, QuestionView, Verdict,
    },
};
use crate::{
    catalog,
    db::{self, DbError, Result},
};
use rusqlite::{Connection, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

fn invalid(message: &str) -> DbError {
    DbError::Invalid(message.into())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Frozen {
    class_id: String,
    course_id: String,
    unit: String,
    unit_label: String,
    path_revision: u32,
    bank_version: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct UnitChallengeView {
    pub attempt_id: AttemptId,
    pub round_id: RoundId,
    pub revision: u32,
    pub course_id: String,
    pub unit: String,
    pub unit_label: String,
    pub path_revision: u32,
    pub questions: Vec<QuestionView>,
    pub responses: BTreeMap<String, Response>,
    pub criteria: Vec<CriterionResult>,
    pub submitted: bool,
    pub demonstrated: Vec<PathTopic>,
    pub needs_practice: Vec<PathTopic>,
    pub applied_revision: Option<u32>,
    pub estimated_minutes: u32,
}

fn owner(class_id: &str) -> Owner {
    Owner::Class(class_id.into())
}
fn accepted(conn: &Connection, course_id: &str) -> Result<AcceptedPath> {
    let course =
        catalog::course(course_id).ok_or_else(|| DbError::InvalidFocus(course_id.into()))?;
    if course.kind != catalog::SubjectKind::Engineering {
        return Err(invalid(
            "Unit challenges are available for engineering classes.",
        ));
    }
    classes::current_path(conn, course_id)?
        .ok_or_else(|| invalid("Accept a learning path before taking a unit challenge."))
}
fn latest(conn: &Connection, class_id: &str) -> Result<Option<Attempt>> {
    assessments::latest(conn, &owner(class_id), Purpose::UnitChallenge)
}
fn context(attempt: &Attempt) -> Result<Frozen> {
    Ok(serde_json::from_value(attempt.context.clone())?)
}
fn questions(attempt: &Attempt) -> Result<Vec<Question>> {
    attempt
        .rounds
        .last()
        .ok_or_else(|| invalid("challenge has no round"))?
        .items
        .iter()
        .map(|item| Ok(serde_json::from_value(item.body.clone())?))
        .collect()
}
fn criteria(attempt: &Attempt) -> Result<Vec<CriterionResult>> {
    match attempt
        .rounds
        .last()
        .and_then(|round| round.submission.as_ref())
    {
        Some(submission) => Ok(serde_json::from_value(submission.result.clone())?),
        None => Ok(Vec::new()),
    }
}
fn topics(
    conn: &Connection,
    course_id: &str,
    rows: &[CriterionResult],
    passed: bool,
) -> Result<Vec<PathTopic>> {
    let concepts = db::all_concepts(conn, course_id)?;
    let mut out: Vec<PathTopic> = Vec::new();
    for row in rows
        .iter()
        .filter(|row| (row.verdict == Verdict::Passed) == passed)
    {
        if out.iter().any(|t| t.id == row.competency) {
            continue;
        }
        out.push(PathTopic {
            id: row.competency.clone(),
            label: concepts
                .iter()
                .find(|c| c.slug == row.competency)
                .map(|c| c.title.clone())
                .unwrap_or_else(|| row.competency.clone()),
            reason: match row.verdict {
                Verdict::Passed => "Sample demonstrated in this unit challenge.".into(),
                Verdict::NeedsPractice => "Sample needs practice.".into(),
                Verdict::Unknown => "Skipped in this unit challenge.".into(),
            },
        });
    }
    Ok(out)
}
fn applied_revision(conn: &Connection, class_id: &str, attempt: &AttemptId) -> Result<Option<u32>> {
    let mut statement = conn.prepare(
        "SELECT revision, plan_json FROM path_revisions WHERE class_id = ?1 ORDER BY revision",
    )?;
    let rows = statement.query_map([class_id], |r| {
        Ok((r.get::<_, u32>(0)?, r.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (revision, plan) = row?;
        let plan: placement::Recommendation = serde_json::from_str(&plan)?;
        if plan.checked.iter().any(|t| t.reason.contains(&attempt.0)) {
            return Ok(Some(revision));
        }
    }
    Ok(None)
}
fn view(conn: &Connection, attempt: &Attempt) -> Result<UnitChallengeView> {
    let frozen = context(attempt)?;
    let round = attempt
        .rounds
        .last()
        .ok_or_else(|| invalid("challenge has no round"))?;
    let questions = questions(attempt)?;
    let rows = criteria(attempt)?;
    Ok(UnitChallengeView {
        attempt_id: attempt.id.clone(),
        round_id: round.id.clone(),
        revision: round.revision,
        course_id: frozen.course_id.clone(),
        unit: frozen.unit.clone(),
        unit_label: frozen.unit_label.clone(),
        path_revision: frozen.path_revision,
        responses: questions
            .iter()
            .map(|q| {
                (
                    q.id.clone(),
                    round.responses.get(&q.id).cloned().unwrap_or(Response {
                        answer: String::new(),
                        status: ResponseStatus::Draft,
                    }),
                )
            })
            .collect(),
        estimated_minutes: questions.len() as u32 + 1,
        questions: questions
            .into_iter()
            .map(|q| QuestionView {
                id: q.id,
                label: q.label,
                prompt: q.prompt,
                choices: q.choices,
            })
            .collect(),
        submitted: round.submission.is_some(),
        demonstrated: topics(conn, &frozen.course_id, &rows, true)?,
        needs_practice: topics(conn, &frozen.course_id, &rows, false)?,
        applied_revision: applied_revision(conn, &frozen.class_id, &attempt.id)?,
        criteria: rows,
    })
}

pub fn get(conn: &Connection, course_id: &str) -> Result<Option<UnitChallengeView>> {
    let Some(path) = classes::current_path(conn, course_id)? else {
        return Ok(None);
    };
    latest(conn, &path.reference.class_id)?
        .map(|attempt| view(conn, &attempt))
        .transpose()
}

pub fn start(
    conn: &Connection,
    course_id: &str,
    unit: &str,
    restart: bool,
) -> Result<UnitChallengeView> {
    let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let path = accepted(&tx, course_id)?;
    let class_id = path.reference.class_id.clone();
    let options = enrollment::options(course_id)?;
    let unit_label = options
        .entry_points
        .iter()
        .find(|e| e.id == unit)
        .map(|e| e.label.clone())
        .ok_or_else(|| invalid("That unit is not part of this course."))?;
    if let Some(attempt) = latest(&tx, &class_id)? {
        let frozen = context(&attempt)?;
        if attempt.status == "active" {
            if frozen.unit != unit {
                return Err(DbError::Invalid(format!(
                    "Finish the open {} challenge before starting another unit.",
                    frozen.unit_label
                )));
            }
            return view(&tx, &attempt);
        }
        if !restart
            && frozen.unit == unit
            && applied_revision(&tx, &class_id, &attempt.id)?.is_none()
        {
            return view(&tx, &attempt);
        }
    }
    let bank = placement::bank(course_id)?;
    let selected: Vec<_> = bank
        .questions
        .iter()
        .filter(|q| q.followup_for.is_none() && q.entry_point == unit)
        .cloned()
        .collect();
    if selected.is_empty() {
        return Err(invalid(
            "No challenge questions are authored for this unit yet.",
        ));
    }
    let items = selected
        .iter()
        .map(|q| {
            Ok(Item {
                id: q.id.clone(),
                body: serde_json::to_value(q)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let frozen = Frozen {
        class_id: class_id.clone(),
        course_id: course_id.into(),
        unit: unit.into(),
        unit_label,
        path_revision: path.revision,
        bank_version: bank.version.clone(),
    };
    assessments::start(
        &tx,
        &owner(&class_id),
        Purpose::UnitChallenge,
        &serde_json::to_value(&frozen)?,
        &bank.version,
        &items,
    )?;
    let attempt = latest(&tx, &class_id)?.ok_or_else(|| invalid("challenge did not start"))?;
    let result = view(&tx, &attempt)?;
    tx.commit()?;
    Ok(result)
}

fn checked_attempt(
    conn: &Connection,
    course_id: &str,
    round: &RoundId,
) -> Result<(String, Attempt)> {
    let path = accepted(conn, course_id)?;
    let attempt = latest(conn, &path.reference.class_id)?
        .ok_or_else(|| invalid("start a unit challenge first"))?;
    if attempt.rounds.last().is_none_or(|r| &r.id != round) {
        return Err(invalid(
            "the challenge round changed; reopen the saved challenge",
        ));
    }
    Ok((path.reference.class_id, attempt))
}

pub fn save_response(
    conn: &Connection,
    course_id: &str,
    round: &RoundId,
    revision: u32,
    question_id: &str,
    response: Response,
) -> Result<u32> {
    let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let (class_id, attempt) = checked_attempt(&tx, course_id, round)?;
    let question = questions(&attempt)?
        .into_iter()
        .find(|q| q.id == question_id)
        .ok_or_else(|| invalid("question is not in this challenge"))?;
    if (response.status == ResponseStatus::Answered && response.answer.is_empty())
        || (!response.answer.is_empty()
            && !question.choices.iter().any(|c| c.id == response.answer))
        || (response.status == ResponseStatus::Skipped && !response.answer.is_empty())
    {
        return Err(invalid(
            "select an available answer or explicitly skip this question",
        ));
    }
    let saved = assessments::save_response(
        &tx,
        &owner(&class_id),
        round,
        revision,
        question_id,
        response,
    )?;
    tx.commit()?;
    Ok(saved.revision)
}

pub fn submit(
    conn: &Connection,
    course_id: &str,
    round: &RoundId,
    revision: u32,
) -> Result<UnitChallengeView> {
    let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let (class_id, attempt) = checked_attempt(&tx, course_id, round)?;
    let current = attempt.rounds.last().unwrap();
    if current.submission.is_some() {
        return view(&tx, &attempt);
    }
    if current.revision != revision {
        return Err(invalid("answers changed; submit the saved revision"));
    }
    let mut result = Vec::new();
    for question in questions(&attempt)? {
        let response = current
            .responses
            .get(&question.id)
            .ok_or_else(|| invalid("answer or skip every question"))?;
        if response.status == ResponseStatus::Draft {
            return Err(invalid("confirm or skip every question"));
        }
        let verdict = if response.status == ResponseStatus::Skipped {
            Verdict::Unknown
        } else if response.answer == question.answer {
            Verdict::Passed
        } else {
            Verdict::NeedsPractice
        };
        let expected = question
            .choices
            .iter()
            .find(|c| c.id == question.answer)
            .unwrap()
            .text
            .clone();
        result.push(CriterionResult {
            id: question.criterion,
            competency: question.competency,
            entry_point: question.entry_point,
            label: question.label,
            verdict: verdict.clone(),
            evidence: vec![CriterionEvidence {
                question_id: question.id,
                response: response.clone(),
                verdict,
                explanation: question.explanation,
                expected_answer: expected,
            }],
        });
    }
    assessments::submit_in_transaction(
        &tx,
        &owner(&class_id),
        current,
        &serde_json::to_value(result)?,
        true,
    )?;
    let attempt = latest(&tx, &class_id)?.ok_or_else(|| invalid("challenge vanished"))?;
    let result = view(&tx, &attempt)?;
    tx.commit()?;
    Ok(result)
}

/// Check the learner out of the topics this challenge demonstrated. Idempotent:
/// a challenge already recorded in a path revision returns the current path.
pub fn apply(
    conn: &Connection,
    course_id: &str,
    attempt_id: &str,
    expected_revision: u32,
    today: &str,
) -> Result<AcceptedPath> {
    let path = accepted(conn, course_id)?;
    let attempt = latest(conn, &path.reference.class_id)?
        .filter(|attempt| attempt.id.0 == attempt_id)
        .ok_or_else(|| invalid("That challenge is no longer the latest for this class."))?;
    if attempt
        .rounds
        .last()
        .and_then(|r| r.submission.as_ref())
        .is_none()
    {
        return Err(invalid(
            "Submit the challenge before checking out of its topics.",
        ));
    }
    if applied_revision(conn, &path.reference.class_id, &attempt.id)?.is_some() {
        return Ok(path);
    }
    let demonstrated: Vec<String> = topics(conn, course_id, &criteria(&attempt)?, true)?
        .into_iter()
        .map(|t| t.id)
        .collect();
    if demonstrated.is_empty() {
        return Err(invalid(
            "No sample was demonstrated; the unit stays on your route.",
        ));
    }
    classes::revise(
        conn,
        &RevisePath {
            course_id: course_id.into(),
            expected_revision,
            change: PathChange::CheckOut {
                topics: demonstrated,
                attempt_id: attempt_id.into(),
            },
        },
        today,
    )
}
