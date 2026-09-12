//! CEFR language classes on the shared study runtime. The curated curriculum,
//! unit rotation and evidence rules stay in `language`; this adapter owns
//! planning, optional enrichment and publication, the frozen knowledge check,
//! saved production work and the transactional completion projection.
pub use super::engineering::{activate, patch_work, pause, resumable, save_answer, skip};
use super::engineering::{check_view, get, legacy_status, lifecycle_name};
use crate::{
    classroom::{self, ActiveClassroomSessionView, ProgramRow},
    db::DbError,
    domain::{
        assessments::{self, Item, Owner, Purpose, ResponseStatus, Round, RoundId},
        classes,
        sessions::{
            self, Disposition, PlanOwner, PlanSession, PreparedLesson, Session, SessionId,
            SessionKind, Stage, Status,
        },
    },
    generator::GenerationProfile,
    language::{self, CorrectionView, LanguageLessonView, StoredLesson},
    state::AppState,
};
use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

type Result<T> = std::result::Result<T, String>;

pub const CHECK_RUBRIC: &str = "language-check-v1";
const LEASE_SECONDS: u32 = 3600;
const STAGES: [Stage; 4] = [Stage::Learn, Stage::Practice, Stage::Check, Stage::Feedback];

/// Frozen at planning: the band, unit, pass and curated seed content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Selection {
    pub adapter: String,
    pub language: String,
    pub level: String,
    pub unit_slug: String,
    pub title: String,
    pub phase: i64,
    pub slot_id: Option<i64>,
    #[serde(default)]
    pub occurrence_id: Option<String>,
    pub service_date: String,
    pub revisit: bool,
    pub reason: String,
    pub(crate) seed: StoredLesson,
}

fn e<E: std::fmt::Display>(error: E) -> String {
    error.to_string()
}

pub fn selection(session: &Session) -> Result<Selection> {
    serde_json::from_value(session.context.selection.clone()).map_err(e)
}

/// Choose the next curated unit and pass on the accepted path and record a
/// planned session. Retries resume the same session.
pub fn plan(
    conn: &Connection,
    program: &ProgramRow,
    slot_id: Option<i64>,
    occurrence_id: Option<String>,
    today: &str,
    revisit: bool,
) -> Result<Session> {
    let language = program.subject_id.as_str();
    if !program.enabled {
        return Err(format!("{} is not enabled", program.label));
    }
    if let Some(id) = slot_id {
        classroom::validate_slot_start(conn, language, id, today)?;
    }
    let path = match classes::current_path(conn, language).map_err(e)? {
        Some(path) => path,
        None => classes::ensure_default_path(conn, language, today).map_err(e)?,
    };
    let owner = PlanOwner::Class {
        path: path.reference.clone(),
    };
    if let Some(existing) = sessions::resumable(conn, &owner).map_err(e)? {
        return Ok(existing);
    }
    let planned = language::plan_lesson(conn, language, revisit)?;
    let selection = Selection {
        adapter: "language".into(),
        language: language.into(),
        level: planned.level.clone(),
        unit_slug: planned.unit_slug.clone(),
        title: planned.seed.title.clone(),
        phase: planned.phase,
        slot_id,
        occurrence_id,
        service_date: today.into(),
        revisit,
        reason: format!(
            "{} · pass {} of {} on path revision {}",
            planned.unit_title, planned.phase, planned.level, path.revision
        ),
        seed: planned.seed,
    };
    sessions::plan(
        conn,
        &PlanSession {
            request_key: format!(
                "class:{}:{:032x}",
                path.reference.class_id,
                rand::random::<u128>()
            ),
            owner,
            kind: SessionKind::Lesson,
            stages: STAGES.to_vec(),
            selection: serde_json::to_value(&selection).map_err(e)?,
        },
        Utc::now(),
    )
    .map_err(e)
}

fn publish(
    conn: &Connection,
    lease: &sessions::PreparationLease,
    stored: &StoredLesson,
    provenance: Value,
) -> Result<Session> {
    sessions::publish_preparation(
        conn,
        lease,
        &PreparedLesson {
            title: stored.title.clone(),
            body: serde_json::to_value(stored).map_err(e)?,
            provenance,
        },
        Utc::now(),
    )
    .map_err(e)?;
    let session = sessions::get(conn, &lease.session_id).map_err(e)?;
    ensure_check_round(conn, &session)?;
    Ok(session)
}

fn claim(
    conn: &Connection,
    id: &SessionId,
) -> Result<std::result::Result<sessions::PreparationLease, Session>> {
    let session = sessions::get(conn, id).map_err(e)?;
    if matches!(
        session.status,
        Status::Ready | Status::Active | Status::Paused
    ) {
        return Ok(Err(session));
    }
    if session.status.terminal() {
        return Err("This lesson is finished. Start a new one.".into());
    }
    if sessions::preparation(conn, id).map_err(e)?.status == "failed" {
        sessions::retry_preparation(conn, id, Utc::now()).map_err(e)?;
    }
    sessions::claim_preparation(conn, id, Utc::now(), LEASE_SECONDS)
        .map_err(e)?
        .map(Ok)
        .ok_or_else(|| "This lesson is already being prepared. Wait for it to finish.".to_string())
}

/// Enrich the curated seed with the class tutor when available, then publish
/// one immutable lesson version. The curated objective and checks never move.
pub async fn prepare(state: &AppState, id: &SessionId) -> Result<Session> {
    let (lease, session, chosen, program, contract, profile) = {
        let conn = state.db.0.lock().unwrap();
        let lease = match claim(&conn, id)? {
            Ok(lease) => lease,
            Err(ready) => return Ok(ready),
        };
        let session = sessions::get(&conn, id).map_err(e)?;
        let chosen = selection(&session)?;
        let program = classroom::program_row(&conn, &chosen.language)?;
        let contract =
            classroom::contract_with_goal(&program, classroom::subject(&chosen.language)?.prompt);
        let tutor = &session.context.tutor;
        let profile = GenerationProfile {
            subject_id: chosen.language.clone(),
            agent: tutor.provider.clone(),
            model: tutor.model.clone(),
            custom_bin: tutor.custom_agent_bin.clone().unwrap_or_default(),
            prompt_version: format!("{}.{}", program.prompt_profile, program.prompt_version),
        };
        (lease, session, chosen, program, contract, profile)
    };
    let (mut stored, source) = state
        .generator
        .enrich_classroom_language_lesson(&contract, &profile, &chosen.seed)
        .await;
    if let PlanOwner::Class { path } = &session.context.owner {
        stored.path = Some(path.clone());
    }
    let conn = state.db.0.lock().unwrap();
    publish(
        &conn,
        &lease,
        &stored,
        json!({
            "kind": "language",
            "source": source,
            "runner": profile.agent,
            "model": profile.model,
            "prompt_version": profile.prompt_version,
            "prompt_profile": program.prompt_profile,
            "level": chosen.level,
            "unit_slug": chosen.unit_slug,
            "phase": chosen.phase,
            "prepared_at": Utc::now().to_rfc3339(),
        }),
    )
}

/// Publish the curated seed without a tutor call: the offline path used by
/// tests and desktop fixtures, and the content a failed enrichment falls back to.
pub fn publish_curated(conn: &Connection, id: &SessionId) -> Result<Session> {
    let lease = match claim(conn, id)? {
        Ok(lease) => lease,
        Err(ready) => return Ok(ready),
    };
    let session = sessions::get(conn, id).map_err(e)?;
    let mut stored = selection(&session)?.seed;
    if let PlanOwner::Class { path } = &session.context.owner {
        stored.path = Some(path.clone());
    }
    publish(
        conn,
        &lease,
        &stored,
        json!({"kind": "language", "source": "curated", "prepared_at": Utc::now().to_rfc3339()}),
    )
}

fn stored_lesson(
    conn: &Connection,
    id: &SessionId,
) -> Result<Option<(sessions::LessonVersion, StoredLesson)>> {
    let Some(lesson) = sessions::lesson(conn, id).map_err(e)? else {
        return Ok(None);
    };
    let stored = serde_json::from_value(lesson.content.body.clone()).map_err(e)?;
    Ok(Some((lesson, stored)))
}

/// The frozen knowledge check of a prepared lesson; idempotent.
pub fn ensure_check_round(conn: &Connection, session: &Session) -> Result<Round> {
    let (lesson, stored) = stored_lesson(conn, &session.id)?.ok_or("prepared lesson is missing")?;
    let items = stored
        .questions
        .iter()
        .map(|question| {
            Ok(Item {
                id: question.id.to_string(),
                body: serde_json::to_value(question).map_err(e)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let chosen = selection(session)?;
    let context = json!({
        "lesson_version_id": lesson.id.0,
        "level": chosen.level,
        "unit_slug": chosen.unit_slug,
        "phase": chosen.phase,
        "content_provenance": "lesson_version",
    });
    assessments::start(
        conn,
        &Owner::StudySession(session.id.0.clone()),
        Purpose::ExitCheck,
        &context,
        CHECK_RUBRIC,
        &items,
    )
    .map_err(e)
}

fn check_round(conn: &Connection, session: &Session) -> Result<Option<Round>> {
    if session.status.terminal() || session.lesson_version_id.is_none() {
        return Ok(assessments::latest(
            conn,
            &Owner::StudySession(session.id.0.clone()),
            Purpose::ExitCheck,
        )
        .map_err(e)?
        .and_then(|attempt| attempt.rounds.last().cloned()));
    }
    ensure_check_round(conn, session).map(Some)
}

/// Render a prepared shared-runtime language lesson.
pub fn view(conn: &Connection, id: &SessionId) -> Result<Option<LanguageLessonView>> {
    let Some(session) = get(conn, id)? else {
        return Ok(None);
    };
    let Some((_, stored)) = stored_lesson(conn, id)? else {
        return Ok(None);
    };
    let chosen = selection(&session)?;
    let program = classroom::program_row(conn, &chosen.language)?;
    let round = check_round(conn, &session)?;
    let outcome = sessions::result(conn, id)
        .map_err(e)?
        .and_then(|result| result.outcome.get("result").cloned());
    let mut view = language::view_from_stored(
        id.0.clone(),
        &chosen.language,
        &chosen.level,
        &chosen.unit_slug,
        chosen.phase,
        legacy_status(session.status),
        program.session_minutes,
        stored,
    )?;
    view.runtime = "study".into();
    view.lifecycle = lifecycle_name(session.status);
    view.revision = session.revision;
    view.checkpoint = Some(session.checkpoint.clone());
    view.check = round.as_ref().map(check_view);
    view.outcome = outcome;
    Ok(Some(view))
}

/// Self-reported production work submitted with the knowledge check. It is
/// activity and confidence, never proficiency by itself.
#[derive(Debug, Clone, Deserialize)]
pub struct LanguageCheckInput {
    #[serde(default)]
    pub writing_response: String,
    #[serde(default)]
    pub speaking_completed: bool,
    #[serde(default)]
    pub listened: bool,
    #[serde(default = "default_confidence")]
    pub confidence: i64,
}
fn default_confidence() -> i64 {
    3
}

/// Grade exactly the displayed round, then complete the session: the check
/// submission, strand evidence, unit progress, band advancement and lifecycle
/// commit together. A repeated submission returns the original result.
pub fn submit(
    conn: &Connection,
    id: &SessionId,
    round_id: &RoundId,
    expected_revision: u32,
    input: &LanguageCheckInput,
    today: &str,
) -> Result<Value> {
    if let Some(previous) = sessions::result(conn, id).map_err(e)? {
        return previous
            .outcome
            .get("result")
            .cloned()
            .ok_or_else(|| "This lesson was skipped; its check was not graded.".into());
    }
    let session = sessions::get(conn, id).map_err(e)?;
    if session.status != Status::Active {
        return Err("Resume this lesson before submitting its knowledge check.".into());
    }
    let owner = Owner::StudySession(id.0.clone());
    let round = assessments::round(conn, round_id).map_err(e)?;
    let owns: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM assessment_attempts WHERE id=?1 AND owner_kind='study_session' AND owner_key=?2)",
            params![round.attempt_id.0, id.0],
            |r| r.get(0),
        )
        .map_err(e)?;
    if !owns {
        return Err("This knowledge check belongs to another lesson.".into());
    }
    if round.revision != expected_revision {
        return Err("Your saved answers changed. Review them, then submit again.".into());
    }
    let chosen = selection(&session)?;
    let mut corrections = Vec::new();
    let mut strand_results = Vec::new();
    for item in &round.items {
        let question: language::StoredQuestion =
            serde_json::from_value(item.body.clone()).map_err(e)?;
        let response = round
            .responses
            .get(&item.id)
            .filter(|response| response.status == ResponseStatus::Answered)
            .ok_or("Answer every knowledge check before submitting.")?;
        let selected: usize = response
            .answer
            .parse()
            .map_err(|_| "A saved answer is unreadable; choose it again.".to_string())?;
        if selected >= question.choices.len() {
            return Err(format!("answer {} is invalid", question.id));
        }
        let correct = selected == question.correct_index;
        strand_results.push((question.strand.clone(), correct));
        corrections.push(CorrectionView {
            question_id: question.id,
            prompt: question.prompt.clone(),
            selected_answer: question.choices[selected].clone(),
            correct_answer: question.choices[question.correct_index].clone(),
            correct,
            explanation: question.explanation.clone(),
        });
    }
    let mut body = session.checkpoint.body.clone();
    body.stage = Stage::Feedback;
    body.work.insert(
        "writing_response".into(),
        Value::String(input.writing_response.trim().into()),
    );
    body.work.insert(
        "speaking_completed".into(),
        Value::Bool(input.speaking_completed),
    );
    body.work
        .insert("listened".into(), Value::Bool(input.listened));
    body.work
        .insert("confidence".into(), json!(input.confidence.clamp(1, 5)));
    let checkpoint =
        sessions::save_checkpoint(conn, id, session.checkpoint.revision, &body, Utc::now())
            .map_err(e)?;
    let session = sessions::get(conn, id).map_err(e)?;
    let today = today.to_string();
    let outcome = sessions::finish(
        conn,
        id,
        session.revision,
        checkpoint.revision,
        Disposition::Completed,
        Utc::now(),
        |tx, _| {
            let evidence = language::project_completion(
                tx,
                language::CompletionEvidence {
                    language: &chosen.language,
                    level: &chosen.level,
                    unit_slug: &chosen.unit_slug,
                    phase: chosen.phase,
                    strand_results: &strand_results,
                    writing_response: &input.writing_response,
                    speaking_completed: input.speaking_completed,
                    listened: input.listened,
                    confidence: input.confidence,
                },
                &today,
            )
            .map_err(DbError::Invalid)?;
            let progress =
                language::program_view(tx, &chosen.language, &today).map_err(DbError::Invalid)?;
            let result = json!({
                "session_id": id.0,
                "passed": evidence.passed,
                "score": evidence.score,
                "corrections": corrections,
                "level_advanced_to": evidence.level_advanced_to,
                "current_level": evidence.current_level,
                "progress": progress,
            });
            assessments::submit_in_transaction(tx, &owner, &round, &result, true)?;
            crate::domain::schedule::finish_step(tx, &format!("study:{}", id.0), true, Utc::now())?;
            Ok(json!({
                "kind": "completed",
                "level": chosen.level,
                "unit_slug": chosen.unit_slug,
                "phase": chosen.phase,
                "result": result,
            }))
        },
    )
    .map_err(e)?;
    outcome
        .outcome
        .get("result")
        .cloned()
        .ok_or_else(|| "Session result is missing.".into())
}

/// Resumable language lessons for Today and the class workspace.
pub fn active_summaries(conn: &Connection) -> Result<Vec<ActiveClassroomSessionView>> {
    let mut statement = conn
        .prepare(
            "SELECT s.id, c.course_id, p.label, s.status,
                    COALESCE(json_extract(v.content_json, '$.title'),
                             json_extract(s.context_json, '$.selection.title'))
             FROM study_sessions s
             JOIN classes c ON c.id = s.class_id
             JOIN classroom_programs p ON p.subject_id = c.course_id
             LEFT JOIN lesson_versions v ON v.id = s.lesson_version_id
             WHERE s.owner_kind = 'class'
               AND s.status NOT IN ('completed','skipped')
               AND json_extract(s.context_json, '$.selection.adapter') = 'language'
             ORDER BY s.rowid",
        )
        .map_err(e)?;
    let rows = statement
        .query_map([], |row| {
            Ok(ActiveClassroomSessionView {
                session_id: row.get::<_, String>(0)?,
                subject_id: row.get(1)?,
                kind: "language".into(),
                label: row.get(2)?,
                title: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                runtime: "study".into(),
                lifecycle: row.get(3)?,
            })
        })
        .map_err(e)?;
    rows.collect::<std::result::Result<Vec<_>, _>>().map_err(e)
}
