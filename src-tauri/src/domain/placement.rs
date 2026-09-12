//! Placement is an assessment purpose and a path recommendation, never a
//! separate teaching engine. This module creates no class, lesson or mastery.
use super::{
    assessments::{
        self, Attempt, AttemptId, Item, Owner, Purpose, Response, ResponseStatus, RoundId,
    },
    enrollment::{
        self, CourseReference, DraftStatus, EnrollmentDraft, EnrollmentDraftId, EntryChoice,
        LearningGoal,
    },
};
use crate::catalog;
use crate::db::{DbError, Result};
use rusqlite::{Connection, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Choice {
    pub id: String,
    pub text: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Question {
    pub id: String,
    pub criterion: String,
    pub competency: String,
    pub entry_point: String,
    pub label: String,
    pub prompt: String,
    pub choices: Vec<Choice>,
    pub answer: String,
    pub explanation: String,
    pub followup_for: Option<String>,
    /// The primary source a written question cites; authored banks have none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
    /// A learner disputed the key; the question stops counting and is left
    /// out of every sample until it is corrected.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub voided: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub void_reason: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bank {
    pub course_id: String,
    pub version: String,
    pub estimated_minutes: u32,
    pub scope_note: String,
    pub questions: Vec<Question>,
}
#[derive(Deserialize)]
struct Banks {
    schema_version: u32,
    courses: Vec<Bank>,
}
fn invalid(message: &str) -> DbError {
    DbError::Invalid(message.into())
}
fn digest(value: &impl Serialize) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}

/// Whether a diagnostic bank exists for the course, authored or written
/// for a learner's own class, without validating it (that happens when it
/// is used).
/// A written bank counts only while it still samples every stage three
/// times; a disputed question can take it below that. This is the
/// structural part of the check only: the full validation reads the
/// enrollment options, which ask this question.
fn written_bank_stands(bank: &Bank) -> bool {
    ["foundations", "mechanisms", "production", "synthesis"]
        .iter()
        .all(|stage| {
            bank.questions
                .iter()
                .filter(|q| !q.voided && q.followup_for.is_none() && q.entry_point == *stage)
                .count()
                >= CRITERIA_PER_ENTRY_POINT
        })
}

/// The bank the tutor wrote and the desk registered for a learner's own
/// class.
fn written_bank(course_id: &str) -> Option<Bank> {
    if !catalog::is_custom(course_id) {
        return None;
    }
    catalog::custom_bank(course_id).and_then(|value| serde_json::from_value::<Bank>(value).ok())
}

/// The bank a bundled class ships with, unvalidated.
pub fn authored_bank(course_id: &str) -> Option<Bank> {
    let banks: Banks = serde_json::from_str(include_str!("../../seed/diagnostics.json")).ok()?;
    if banks.schema_version != 1 {
        return None;
    }
    banks.courses.into_iter().find(|b| b.course_id == course_id)
}

pub fn has_bank(course_id: &str) -> bool {
    if let Some(bank) = written_bank(course_id) {
        return written_bank_stands(&bank);
    }
    !catalog::is_custom(course_id) && authored_bank(course_id).is_some()
}

pub fn bank(course_id: &str) -> Result<Bank> {
    // A learner's own class samples the bank the tutor wrote for it, voided
    // questions left out; a bundled class samples its authored bank. The
    // practice questions (`domain::banks`) are never sampled here.
    let bank = if let Some(mut bank) = written_bank(course_id) {
        bank.questions.retain(|q| !q.voided);
        bank
    } else if catalog::is_custom(course_id) {
        return Err(invalid("this class has no question bank yet"));
    } else {
        authored_bank(course_id).ok_or_else(|| invalid("diagnostic unavailable for this course"))?
    };
    validate_bank(course_id, bank)
}

/// The same checks for every bank, authored or written.
pub fn validate_bank(course_id: &str, bank: Bank) -> Result<Bank> {
    let options = enrollment::options(course_id)?;
    let mut ids = HashSet::new();
    let mut criteria = HashSet::new();
    for question in &bank.questions {
        if question.id.is_empty()
            || !ids.insert(&question.id)
            || question.prompt.trim().is_empty()
            || question.label.trim().is_empty()
            || question.explanation.trim().is_empty()
            || !options
                .familiarity_options
                .iter()
                .any(|c| c.id == question.competency)
            || !options
                .entry_points
                .iter()
                .any(|e| e.id == question.entry_point)
            || question.choices.len() < 2
            || question.choices.len() > 6
            || !question
                .choices
                .iter()
                .any(|choice| choice.id == question.answer)
            || question
                .choices
                .iter()
                .any(|choice| choice.id.is_empty() || choice.text.trim().is_empty())
            || question
                .choices
                .iter()
                .map(|c| &c.id)
                .collect::<HashSet<_>>()
                .len()
                != question.choices.len()
        {
            return Err(invalid("invalid authored diagnostic question"));
        }
        if question.followup_for.is_none() && !criteria.insert(&question.criterion) {
            return Err(invalid("duplicate diagnostic criterion"));
        }
    }
    for question in bank.questions.iter().filter(|q| q.followup_for.is_some()) {
        let initial = bank
            .questions
            .iter()
            .find(|q| {
                q.followup_for.is_none() && Some(&q.criterion) == question.followup_for.as_ref()
            })
            .ok_or_else(|| invalid("unknown follow-up criterion"))?;
        if initial.criterion != question.criterion
            || initial.competency != question.competency
            || initial.entry_point != question.entry_point
        {
            return Err(invalid("follow-up coverage changed"));
        }
    }
    // A check that skips a stage cannot place anyone in it, and a stage
    // sampled once cannot justify skipping it. Every entry point carries the
    // same weight of evidence.
    let per_stage = CRITERIA_PER_ENTRY_POINT;
    for entry in &options.entry_points {
        let covering = bank
            .questions
            .iter()
            .filter(|q| q.followup_for.is_none() && q.entry_point == entry.id)
            .count();
        if covering != per_stage {
            return Err(invalid(
                "a diagnostic bank must sample every entry point of its course",
            ));
        }
    }
    if criteria.len() != per_stage * options.entry_points.len() || bank.version.is_empty() {
        return Err(invalid("incomplete diagnostic bank"));
    }
    Ok(bank)
}

/// Criteria sampled per entry point. Three is the smallest number that lets a
/// stage be skipped on evidence rather than on a single lucky answer.
pub const CRITERIA_PER_ENTRY_POINT: usize = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Frozen {
    draft: EnrollmentDraft,
    bank: Bank,
    bank_fingerprint: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionView {
    pub id: String,
    pub label: String,
    pub prompt: String,
    pub choices: Vec<Choice>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Passed,
    NeedsPractice,
    Unknown,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriterionEvidence {
    pub question_id: String,
    pub response: Response,
    pub verdict: Verdict,
    pub explanation: String,
    pub expected_answer: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriterionResult {
    pub id: String,
    pub competency: String,
    pub entry_point: String,
    pub label: String,
    pub verdict: Verdict,
    pub evidence: Vec<CriterionEvidence>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticView {
    pub attempt_id: AttemptId,
    pub round_id: RoundId,
    pub ordinal: u32,
    pub revision: u32,
    pub questions: Vec<QuestionView>,
    pub responses: BTreeMap<String, Response>,
    pub criteria: Vec<CriterionResult>,
    pub submitted: bool,
    pub completed: bool,
    pub can_follow_up: bool,
    pub matches_draft: bool,
    pub course: CourseReference,
    pub scope_note: String,
    pub estimated_minutes: u32,
    /// What the first round samples, stage by stage, so a learner is told
    /// what is coming before the first question rather than after the last.
    pub stages: Vec<StageSample>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StageSample {
    pub id: String,
    pub label: String,
    pub questions: usize,
    /// The skills the questions in this stage touch, in their own words.
    pub samples: Vec<String>,
}
fn owner(id: &EnrollmentDraftId) -> Owner {
    Owner::EnrollmentDraft(id.0.clone())
}
fn latest(conn: &Connection, id: &EnrollmentDraftId) -> Result<Attempt> {
    assessments::latest(conn, &owner(id), Purpose::Diagnostic)?
        .ok_or_else(|| invalid("start a diagnostic first"))
}
fn context(attempt: &Attempt) -> Result<Frozen> {
    Ok(serde_json::from_value(attempt.context.clone())?)
}
fn questions(attempt: &Attempt) -> Result<Vec<Question>> {
    attempt
        .rounds
        .last()
        .ok_or_else(|| invalid("diagnostic has no round"))?
        .items
        .iter()
        .map(|item| Ok(serde_json::from_value(item.body.clone())?))
        .collect()
}
fn criteria(attempt: &Attempt) -> Result<Vec<CriterionResult>> {
    let mut out: Vec<CriterionResult> = Vec::new();
    for round in &attempt.rounds {
        if let Some(submission) = &round.submission {
            let rows: Vec<CriterionResult> = serde_json::from_value(submission.result.clone())?;
            for mut row in rows {
                if let Some(previous) = out.iter_mut().find(|r| r.id == row.id) {
                    // An unanswered follow-up supplies no replacement evidence.
                    if row.verdict != Verdict::Unknown {
                        previous.verdict = row.verdict;
                    }
                    previous.evidence.append(&mut row.evidence);
                } else {
                    out.push(row);
                }
            }
        }
    }
    Ok(out)
}
fn followups(attempt: &Attempt, frozen: &Frozen) -> Result<Vec<Question>> {
    if attempt.status != "active"
        || attempt.rounds.len() != 1
        || attempt.rounds[0].submission.is_none()
    {
        return Ok(Vec::new());
    }
    let results = criteria(attempt)?;
    Ok(frozen
        .bank
        .questions
        .iter()
        .filter(|q| {
            q.followup_for.as_ref().is_some_and(|id| {
                results
                    .iter()
                    .any(|r| &r.id == id && r.verdict != Verdict::Passed)
            })
        })
        .take(2)
        .cloned()
        .collect())
}
fn matches_draft(draft: &EnrollmentDraft, frozen: &Frozen) -> Result<bool> {
    Ok(draft.status == DraftStatus::Draft
        && draft == &frozen.draft
        && enrollment::course_snapshot(&draft.course.course_id)?.0 == draft.course
        && digest(&bank(&draft.course.course_id)?)? == frozen.bank_fingerprint)
}
fn view(conn: &Connection, id: &EnrollmentDraftId, attempt: &Attempt) -> Result<DiagnosticView> {
    let frozen = context(attempt)?;
    let round = attempt
        .rounds
        .last()
        .ok_or_else(|| invalid("diagnostic has no round"))?;
    let questions = questions(attempt)?;
    let responses = questions
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
        .collect();
    let stages = stage_samples(&frozen, attempt)?;
    Ok(DiagnosticView {
        attempt_id: attempt.id.clone(),
        round_id: round.id.clone(),
        ordinal: round.ordinal,
        revision: round.revision,
        questions: questions
            .into_iter()
            .map(|q| QuestionView {
                id: q.id,
                label: q.label,
                prompt: q.prompt,
                choices: q.choices,
            })
            .collect(),
        responses,
        criteria: criteria(attempt)?,
        submitted: round.submission.is_some(),
        completed: attempt.status == "completed",
        can_follow_up: !followups(attempt, &frozen)?.is_empty(),
        matches_draft: matches_draft(&enrollment::draft(conn, id)?, &frozen)?,
        course: frozen.draft.course,
        scope_note: frozen.bank.scope_note,
        estimated_minutes: (attempt.rounds[0].items.len() as u32 + 2)
            .min(frozen.bank.estimated_minutes),
        stages,
    })
}

/// The stages the first round covers, in course order, with the label of
/// every sampled skill. Follow-up rounds re-ask a criterion; they add no stage.
fn stage_samples(frozen: &Frozen, attempt: &Attempt) -> Result<Vec<StageSample>> {
    let entries = enrollment::options(&frozen.draft.course.course_id)?.entry_points;
    let first: HashSet<&str> = attempt.rounds[0]
        .items
        .iter()
        .map(|item| item.id.as_str())
        .collect();
    Ok(entries
        .iter()
        .filter_map(|entry| {
            let sampled: Vec<String> = frozen
                .bank
                .questions
                .iter()
                .filter(|q| q.entry_point == entry.id && first.contains(q.id.as_str()))
                .map(|q| q.label.clone())
                .collect();
            (!sampled.is_empty()).then(|| StageSample {
                id: entry.id.clone(),
                label: entry.label.clone(),
                questions: sampled.len(),
                samples: sampled,
            })
        })
        .collect())
}
pub fn get(conn: &Connection, id: &EnrollmentDraftId) -> Result<Option<DiagnosticView>> {
    enrollment::draft(conn, id)?;
    assessments::latest(conn, &owner(id), Purpose::Diagnostic)?
        .map(|attempt| view(conn, id, &attempt))
        .transpose()
}
fn checked_draft(
    conn: &Connection,
    id: &EnrollmentDraftId,
    revision: u32,
) -> Result<EnrollmentDraft> {
    let draft = enrollment::draft(conn, id)?;
    if draft.status != DraftStatus::Draft || draft.revision != revision {
        return Err(invalid("setup changed; save and reload the current draft"));
    }
    if enrollment::course_snapshot(&draft.course.course_id)?.0 != draft.course {
        return Err(invalid(
            "curriculum changed; update the setup before continuing",
        ));
    }
    Ok(draft)
}
fn items(questions: &[Question]) -> Result<Vec<Item>> {
    questions
        .iter()
        .map(|q| {
            Ok(Item {
                id: q.id.clone(),
                body: serde_json::to_value(q)?,
            })
        })
        .collect()
}

pub fn start(
    conn: &Connection,
    id: &EnrollmentDraftId,
    revision: u32,
    restart: bool,
) -> Result<DiagnosticView> {
    let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let draft = checked_draft(&tx, id, revision)?;
    if draft.configuration.entry != EntryChoice::Diagnostic {
        return Err(invalid("choose the diagnostic entry route first"));
    }
    if let Some(attempt) = assessments::latest(&tx, &owner(id), Purpose::Diagnostic)? {
        if !restart || attempt.status == "active" {
            return view(&tx, id, &attempt);
        }
    }
    let bank = bank(&draft.course.course_id)?;
    let entries = enrollment::options(&draft.course.course_id)?.entry_points;
    let target = match &draft.configuration.goal {
        LearningGoal::LanguageLevel { target_level, .. } => {
            entries.iter().position(|e| &e.id == target_level).unwrap()
        }
        _ => entries.len() - 1,
    };
    let selected: Vec<_> = bank
        .questions
        .iter()
        .filter(|q| {
            q.followup_for.is_none()
                && entries.iter().position(|e| e.id == q.entry_point).unwrap() <= target
        })
        .cloned()
        .collect();
    let frozen = Frozen {
        draft,
        bank_fingerprint: digest(&bank)?,
        bank,
    };
    assessments::start(
        &tx,
        &owner(id),
        Purpose::Diagnostic,
        &serde_json::to_value(&frozen)?,
        &frozen.bank.version,
        &items(&selected)?,
    )?;
    let result = view(&tx, id, &latest(&tx, id)?)?;
    tx.commit()?;
    Ok(result)
}
fn checked_attempt(conn: &Connection, id: &EnrollmentDraftId, round: &RoundId) -> Result<Attempt> {
    let attempt = latest(conn, id)?;
    if attempt.rounds.last().is_none_or(|r| &r.id != round) {
        return Err(invalid(
            "the diagnostic round changed; reopen the saved check",
        ));
    }
    Ok(attempt)
}
pub fn save_response(
    conn: &Connection,
    id: &EnrollmentDraftId,
    round: &RoundId,
    revision: u32,
    question_id: &str,
    response: Response,
) -> Result<u32> {
    let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let attempt = checked_attempt(&tx, id, round)?;
    let question = questions(&attempt)?
        .into_iter()
        .find(|q| q.id == question_id)
        .ok_or_else(|| invalid("question is not in this diagnostic round"))?;
    if (response.status == ResponseStatus::Answered && response.answer.is_empty())
        || (!response.answer.is_empty()
            && !question
                .choices
                .iter()
                .any(|choice| choice.id == response.answer))
        || (response.status == ResponseStatus::Skipped && !response.answer.is_empty())
    {
        return Err(invalid(
            "select an available answer or explicitly skip this question",
        ));
    }
    let saved =
        assessments::save_response(&tx, &owner(id), round, revision, question_id, response)?;
    tx.commit()?;
    Ok(saved.revision)
}
pub fn submit(
    conn: &Connection,
    id: &EnrollmentDraftId,
    round: &RoundId,
    revision: u32,
) -> Result<DiagnosticView> {
    let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let attempt = checked_attempt(&tx, id, round)?;
    let current = attempt.rounds.last().unwrap();
    if current.submission.is_some() {
        return view(&tx, id, &attempt);
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
        &owner(id),
        current,
        &serde_json::to_value(result)?,
        current.ordinal > 1,
    )?;
    let result = view(&tx, id, &latest(&tx, id)?)?;
    tx.commit()?;
    Ok(result)
}
pub fn follow_up(
    conn: &Connection,
    id: &EnrollmentDraftId,
    round: &RoundId,
) -> Result<DiagnosticView> {
    let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let attempt = latest(&tx, id)?;
    // A repeated click after the response was lost resumes the one follow-up.
    if attempt.rounds.len() == 2 && attempt.rounds[0].id == *round {
        return view(&tx, id, &attempt);
    }
    if attempt.rounds.last().is_none_or(|r| &r.id != round) {
        return Err(invalid("diagnostic round changed"));
    }
    let frozen = context(&attempt)?;
    let selected = followups(&attempt, &frozen)?;
    if selected.is_empty() {
        return Err(invalid(
            "no further prerequisite questions are available for this check",
        ));
    }
    assessments::append_round(&tx, &attempt.id, &frozen.bank.version, &items(&selected)?)?;
    let result = view(&tx, id, &latest(&tx, id)?)?;
    tx.commit()?;
    Ok(result)
}
pub fn finish(
    conn: &Connection,
    id: &EnrollmentDraftId,
    round: &RoundId,
) -> Result<DiagnosticView> {
    let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let attempt = checked_attempt(&tx, id, round)?;
    assessments::finish_attempt(&tx, &owner(id), &attempt.id)?;
    let result = view(&tx, id, &latest(&tx, id)?)?;
    tx.commit()?;
    Ok(result)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathTopic {
    pub id: String,
    pub label: String,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    pub id: String,
    pub draft_id: EnrollmentDraftId,
    pub draft_revision: u32,
    pub course: CourseReference,
    pub route: String,
    pub entry_point: String,
    pub entry_label: String,
    pub explanation: String,
    pub earlier_topics: Vec<PathTopic>,
    pub refreshers: Vec<PathTopic>,
    pub criteria: Vec<CriterionResult>,
    pub required_outcome: String,
    pub assessment_attempt_id: Option<AttemptId>,
    /// Topics the learner chose to leave after acceptance: not assessed, no credit.
    #[serde(default)]
    pub bypassed: Vec<PathTopic>,
    /// Topics whose prior knowledge a unit challenge demonstrated; not completed here.
    #[serde(default)]
    pub checked: Vec<PathTopic>,
    /// Accepted bridge lessons, taken before the work that depends on them.
    #[serde(default)]
    pub bridges: Vec<PathTopic>,
    /// Declined bridge proposals; the same pair is not proposed again.
    #[serde(default)]
    pub declined_bridges: Vec<BridgeDecision>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeDecision {
    pub topic: String,
    pub before: String,
}
pub fn recommend(
    conn: &Connection,
    id: &EnrollmentDraftId,
    revision: u32,
) -> Result<Recommendation> {
    let tx = conn.unchecked_transaction()?;
    recommend_in_transaction(&tx, id, revision)
}

pub(crate) fn recommend_in_transaction(
    conn: &Connection,
    id: &EnrollmentDraftId,
    revision: u32,
) -> Result<Recommendation> {
    let draft = checked_draft(conn, id, revision)?;
    let options = enrollment::options(&draft.course.course_id)?;
    let (_, snapshot) = enrollment::course_snapshot(&draft.course.course_id)?;
    let mut start = 0;
    let mut results = Vec::new();
    let mut attempt_id = None;
    let (route, explanation) = match &draft.configuration.entry {
        EntryChoice::Foundations => ("foundations", "Begin with the introductory material. Familiar topics can be revisited or checked later."),
        EntryChoice::Manual { entry_point, .. } => {
            start = options.entry_points.iter().position(|e| &e.id == entry_point).unwrap();
            ("manual", "Your declared starting point sets this provisional route. Earlier material remains available; this choice does not award grades or mastery.")
        },
        EntryChoice::Diagnostic => {
            let attempt = latest(conn,id)?;
            let frozen = context(&attempt)?;
            if attempt.status != "completed" { return Err(invalid("finish the diagnostic or choose another entry route before reviewing the path")); }
            if !matches_draft(&draft,&frozen)? { return Err(invalid("the setup, goal or diagnostic bank changed; complete a new check or choose a manual starting point")); }
            results = criteria(&attempt)?;
            for (index, entry) in options.entry_points.iter().enumerate() {
                let rows: Vec<_> = results.iter().filter(|row| row.entry_point == entry.id).collect();
                let demonstrated = rows.len() == CRITERIA_PER_ENTRY_POINT
                    && rows.iter().all(|row| row.verdict == Verdict::Passed);
                if !demonstrated {
                    break;
                }
                start = (index + 1).min(options.entry_points.len() - 1);
            }
            let _ = &frozen;
            attempt_id = Some(attempt.id);
            ("diagnostic", "The check sampled every stage of this course. Your lessons begin at the first stage you did not fully demonstrate, and anything missed earlier becomes a refresher before the work that depends on it.")
        }
    };
    if let LearningGoal::LanguageLevel { target_level, .. } = &draft.configuration.goal {
        start = start.min(
            options
                .entry_points
                .iter()
                .position(|e| &e.id == target_level)
                .unwrap(),
        );
    }
    let mut earlier = Vec::new();
    let mut refreshers = Vec::new();
    if let Some(concepts) = snapshot["curriculum"].as_array() {
        let earlier_concepts: Vec<_> = concepts
            .iter()
            .filter(|c| {
                options
                    .entry_points
                    .iter()
                    .position(|e| Some(e.id.as_str()) == c["curriculum"]["phase"].as_str())
                    .is_some_and(|index| index < start)
            })
            .collect();
        let familiar = match &draft.configuration.entry {
            EntryChoice::Manual {
                familiar_competencies,
                ..
            } => familiar_competencies.clone(),
            _ => Vec::new(),
        };
        for concept in &earlier_concepts {
            let slug = concept["slug"].as_str().unwrap();
            earlier.push(PathTopic { id:slug.into(),label:concept["title"].as_str().unwrap().into(),reason:if familiar.iter().any(|id| id == slug) { "Declared familiar; not assessed." } else { "Earlier material remains available for optional review; no completion credit is awarded." }.into() });
        }
        let dependencies: HashSet<_> = concepts
            .iter()
            .filter(|c| !earlier_concepts.contains(c))
            .flat_map(|c| c["prereqs"].as_array().into_iter().flatten())
            .filter_map(|p| p.as_str())
            .collect();
        for topic in &earlier {
            let sampled = results
                .iter()
                .any(|r| r.competency == topic.id && r.verdict == Verdict::Passed);
            if dependencies.contains(topic.id.as_str()) && !sampled {
                refreshers.push(PathTopic {id:topic.id.clone(),label:topic.label.clone(),reason:"An upcoming topic depends on this earlier material; check it before dependent work.".into()});
            }
        }
    } else {
        for entry in &options.entry_points[..start] {
            earlier.push(PathTopic {id:entry.id.clone(),label:format!("{} introductory material",entry.label),reason:"A provisional band preference; listening, speaking and writing remain separately assessed.".into()});
        }
    }
    for row in &results {
        if row.verdict != Verdict::Passed
            && options
                .entry_points
                .iter()
                .position(|e| e.id == row.entry_point)
                .is_some_and(|index| index < start)
            && !refreshers.iter().any(|r| r.id == row.competency)
        {
            refreshers.push(PathTopic {
                id: row.competency.clone(),
                label: row.label.clone(),
                reason: if row.verdict == Verdict::Unknown {
                    "This prerequisite sample was skipped and remains unknown."
                } else {
                    "This sample showed a gap; revisit it before dependent work."
                }
                .into(),
            });
        }
    }
    let entry = &options.entry_points[start];
    let mut result = Recommendation {
        id: String::new(),
        draft_id: id.clone(),
        draft_revision: revision,
        course: draft.course,
        route: route.into(),
        entry_point: entry.id.clone(),
        entry_label: entry.label.clone(),
        explanation: explanation.into(),
        earlier_topics: earlier,
        refreshers,
        criteria: results,
        required_outcome: snapshot["course"]["outcome"].as_str().unwrap().into(),
        assessment_attempt_id: attempt_id,
        bypassed: Vec::new(),
        checked: Vec::new(),
        bridges: Vec::new(),
        declined_bridges: Vec::new(),
    };
    result.id = format!("recommendation-{}", digest(&result)?);
    Ok(result)
}
