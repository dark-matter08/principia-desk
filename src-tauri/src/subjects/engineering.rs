//! Engineering classes on the shared study runtime. Topic selection, teaching
//! prompts and quality gates stay in `classroom`/`generator`; this adapter owns
//! planning, preparation publication, the frozen knowledge check, saved work
//! and the completion projection for class-owned sessions.
use crate::{
    classroom::{
        self, ActiveClassroomSessionView, CheckView, ClassroomCorrectionView,
        ClassroomQuestionView, EngineeringLessonView, EngineeringSessionResult, ProgramRow,
        StoredEngineeringLesson, StoredQuestion,
    },
    db,
    domain::{
        assessments::{self, Item, Owner, Purpose, Response, ResponseStatus, Round, RoundId},
        classes,
        sessions::{
            self, Checkpoint, CheckpointBody, Disposition, PlanOwner, PlanSession, PreparedLesson,
            ReadingPosition, Session, SessionId, SessionKind, Stage, Status,
        },
    },
    generator::{CourseRequest, GenerationProfile},
    mastery, selection,
    state::AppState,
};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

type Result<T> = std::result::Result<T, String>;

pub const CHECK_RUBRIC: &str = "engineering-check-v1";
/// One app process prepares a lesson at a time; an expired lease after a crash
/// can be reclaimed on the next start.
const LEASE_SECONDS: u32 = 3600;
const STAGES: [Stage; 4] = [Stage::Learn, Stage::Practice, Stage::Check, Stage::Feedback];

/// Frozen at planning: which topic, why, and which appointment it serves.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Selection {
    pub adapter: String,
    pub concept_id: i64,
    pub slug: String,
    pub title: String,
    pub category: String,
    pub slot_id: Option<i64>,
    /// Durable appointment this lesson serves, when started from one.
    #[serde(default)]
    pub occurrence_id: Option<String>,
    /// Minutes this lesson was given inside a block, fixed at planning so the
    /// estimate does not drift as the block runs down.
    #[serde(default)]
    pub minutes: Option<i64>,
    pub service_date: String,
    pub revisit: bool,
    pub reason: String,
}

fn e<E: std::fmt::Display>(error: E) -> String {
    error.to_string()
}

pub fn selection(session: &Session) -> Result<Selection> {
    serde_json::from_value(session.context.selection.clone()).map_err(e)
}

/// Snake-case lifecycle name for views.
pub fn lifecycle_name(status: Status) -> String {
    serde_json::to_value(status)
        .ok()
        .and_then(|value| value.as_str().map(String::from))
        .unwrap_or_default()
}

fn owner_for(conn: &Connection, course_id: &str) -> Result<Option<PlanOwner>> {
    Ok(classes::current_path(conn, course_id)
        .map_err(e)?
        .map(|path| PlanOwner::Class {
            path: path.reference,
        }))
}

pub fn get(conn: &Connection, id: &SessionId) -> Result<Option<Session>> {
    let exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM study_sessions WHERE id = ?1)",
            [&id.0],
            |row| row.get(0),
        )
        .map_err(e)?;
    if !exists {
        return Ok(None);
    }
    sessions::get(conn, id).map(Some).map_err(e)
}

/// The class's resumable shared-runtime session, if any.
pub fn resumable(conn: &Connection, course_id: &str) -> Result<Option<Session>> {
    let Some(owner) = owner_for(conn, course_id)? else {
        return Ok(None);
    };
    sessions::resumable(conn, &owner).map_err(e)
}

/// Select the next topic on the accepted path and record a planned session.
/// Provider work has not started; retries resume the same session.
pub fn plan(
    conn: &Connection,
    program: &ProgramRow,
    slot_id: Option<i64>,
    occurrence_id: Option<String>,
    today: &str,
    revisit: bool,
) -> Result<Session> {
    let subject_id = program.subject_id.as_str();
    if !program.enabled {
        return Err(format!("{} is not enabled", program.label));
    }
    if let Some(id) = slot_id {
        classroom::validate_slot_start(conn, subject_id, id, today)?;
    }
    let path = match classes::current_path(conn, subject_id).map_err(e)? {
        Some(path) => path,
        None => classes::ensure_default_path(conn, subject_id, today).map_err(e)?,
    };
    let owner = PlanOwner::Class {
        path: path.reference.clone(),
    };
    if let Some(existing) = sessions::resumable(conn, &owner).map_err(e)? {
        return Ok(existing);
    }
    let concept = if revisit {
        selection::draw_completed(conn, today, subject_id)
    } else {
        classes::next_concept(conn, subject_id, today)
    }
    .map_err(e)?
    .ok_or_else(|| {
        if revisit {
            format!("no completed modules to revisit in {}", program.label)
        } else {
            format!(
                "every module in {} is completed; use revisit or pick another subject",
                program.label
            )
        }
    })?;
    let minutes = occurrence_id.as_deref().and_then(|occurrence| {
        crate::domain::schedule::block_progress(conn, occurrence, Utc::now())
            .ok()
            .flatten()
            .map(|block| block.next_minutes)
    });
    let selection = Selection {
        adapter: "engineering".into(),
        concept_id: concept.id,
        slug: concept.slug.clone(),
        title: concept.title.clone(),
        category: concept.category.clone(),
        slot_id,
        occurrence_id,
        minutes,
        service_date: today.into(),
        revisit,
        reason: if revisit {
            "explicit revisit of a completed topic".into()
        } else {
            format!(
                "next topic on the accepted personal path (revision {})",
                path.revision
            )
        },
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

const REVIEW_STAGES: [Stage; 3] = [Stage::Recall, Stage::Check, Stage::Feedback];

/// Topics of a class whose spaced review is due on `today`, most overdue first.
pub fn review_due(
    conn: &Connection,
    course_id: &str,
    today: &str,
) -> Result<Vec<(db::Concept, String)>> {
    let mut statement = conn
        .prepare(
            "SELECT c.id, m.next_review_date FROM mastery m JOIN concepts c ON c.id = m.concept_id
             WHERE c.active = 1 AND c.focus = ?1 AND m.next_review_date IS NOT NULL AND m.next_review_date <= ?2
             ORDER BY m.next_review_date, c.id",
        )
        .map_err(e)?;
    let rows = statement
        .query_map(params![course_id, today], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(e)?;
    let mut out = Vec::new();
    for row in rows {
        let (id, due) = row.map_err(e)?;
        if let Some(concept) = db::get_concept(conn, id).map_err(e)? {
            out.push((concept, due));
        }
    }
    Ok(out)
}

/// Plan a delayed-retrieval session for the most overdue topic. It has no new
/// lesson: recall, check, feedback. A saved review resumes; a saved lesson must
/// finish first because one resumable session exists per class.
pub fn plan_review(conn: &Connection, program: &ProgramRow, today: &str) -> Result<Session> {
    let subject_id = program.subject_id.as_str();
    if !program.enabled {
        return Err(format!("{} is not enabled", program.label));
    }
    let path = match classes::current_path(conn, subject_id).map_err(e)? {
        Some(path) => path,
        None => classes::ensure_default_path(conn, subject_id, today).map_err(e)?,
    };
    let owner = PlanOwner::Class {
        path: path.reference.clone(),
    };
    if let Some(existing) = sessions::resumable(conn, &owner).map_err(e)? {
        if selection(&existing)?.reason.starts_with("spaced review") {
            return Ok(existing);
        }
        return Err("Finish or discard the saved lesson before starting a review.".into());
    }
    let (concept, due) = review_due(conn, subject_id, today)?
        .into_iter()
        .next()
        .ok_or_else(|| format!("No review is due in {} today.", program.label))?;
    let selection = Selection {
        adapter: "engineering".into(),
        concept_id: concept.id,
        slug: concept.slug.clone(),
        title: concept.title.clone(),
        category: concept.category.clone(),
        slot_id: None,
        occurrence_id: None,
        minutes: None,
        service_date: today.into(),
        revisit: false,
        reason: format!("spaced review due {due}"),
    };
    sessions::plan(
        conn,
        &PlanSession {
            request_key: format!(
                "review:{}:{:032x}",
                path.reference.class_id,
                rand::random::<u128>()
            ),
            owner,
            kind: SessionKind::Retrieval,
            stages: REVIEW_STAGES.to_vec(),
            selection: serde_json::to_value(&selection).map_err(e)?,
        },
        Utc::now(),
    )
    .map_err(e)
}

/// Questions of the most recent published lesson on `slug` for this class.
fn previous_lesson_questions(
    conn: &Connection,
    class_id: &str,
    slug: &str,
) -> Result<Option<Vec<StoredQuestion>>> {
    let content: Option<String> = conn
        .query_row(
            "SELECT lv.content_json FROM lesson_versions lv JOIN study_sessions s ON s.id = lv.session_id
             WHERE s.class_id = ?1 AND json_extract(s.context_json,'$.selection.slug') = ?2
               AND json_extract(lv.content_json,'$.body.source') NOT LIKE 'retrieval:%'
             ORDER BY lv.created_at DESC LIMIT 1",
            params![class_id, slug],
            |r| r.get(0),
        )
        .optional()
        .map_err(e)?;
    let Some(content) = content else {
        return Ok(None);
    };
    let value: Value = serde_json::from_str(&content).map_err(e)?;
    let stored: StoredEngineeringLesson =
        serde_json::from_value(value["body"].clone()).map_err(e)?;
    Ok(Some(stored.questions))
}

fn bundled_questions(course: &crate::generator::FallbackCourse) -> Vec<StoredQuestion> {
    course
        .questions
        .iter()
        .filter(|question| question.kind == "mcq")
        .take(5)
        .enumerate()
        .filter_map(|(index, question)| {
            let choices = question.choices.clone().unwrap_or_default();
            let correct_index = choices
                .iter()
                .position(|choice| choice.trim() == question.correct_answer.trim())?;
            Some(StoredQuestion {
                id: index + 1,
                prompt: question.prompt.clone(),
                choices,
                correct_index,
                explanation: question.explanation.clone(),
                section: "Retrieval".into(),
                learning_objective: course
                    .key_takeaways
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| question.prompt.clone()),
            })
        })
        .collect()
}

/// Publish retrieval material without a provider: the topic's bundled
/// reference questions, preferring ones the last lesson did not show, or the
/// last lesson's own questions when nothing else exists. A repeated sample is
/// recorded as such and never presented as fresh transfer.
pub fn prepare_review(conn: &Connection, id: &SessionId) -> Result<Session> {
    let session = sessions::get(conn, id).map_err(e)?;
    if matches!(
        session.status,
        Status::Ready | Status::Active | Status::Paused
    ) {
        return Ok(session);
    }
    if session.status.terminal() {
        return Err("This review is finished. Start a new one.".into());
    }
    if sessions::preparation(conn, id).map_err(e)?.status == "failed" {
        sessions::retry_preparation(conn, id, Utc::now()).map_err(e)?;
    }
    let Some(lease) =
        sessions::claim_preparation(conn, id, Utc::now(), LEASE_SECONDS).map_err(e)?
    else {
        return Err("This review is already being prepared.".into());
    };
    let chosen = selection(&session)?;
    let course_id = session.context.course.course_id.clone();
    let PlanOwner::Class { path } = &session.context.owner else {
        return Err("This session does not belong to a class.".into());
    };
    let concept = db::get_concept(conn, chosen.concept_id)
        .map_err(e)?
        .ok_or("the reviewed topic no longer exists")?;
    let previous = previous_lesson_questions(conn, &path.class_id, &concept.slug)?;
    let bundled = crate::generator::fallback_for_slug(&course_id, &concept.slug);
    let mut pool = bundled.as_ref().map(bundled_questions).unwrap_or_default();
    let mut material = "bundled";
    if let Some(previous) = &previous {
        let unseen: Vec<StoredQuestion> = pool
            .iter()
            .filter(|q| !previous.iter().any(|p| p.prompt == q.prompt))
            .cloned()
            .collect();
        if unseen.len() >= 3 {
            pool = unseen;
        } else if pool.is_empty() {
            pool = previous.clone();
            material = "previous_lesson";
        }
    }
    if pool.is_empty() {
        let _ = sessions::fail_preparation(
            conn,
            &lease,
            "No retrieval material exists for this topic yet; complete a lesson on it first.",
            Utc::now(),
        );
        return Err(format!(
            "No retrieval material exists for {} yet; complete a lesson on it first.",
            concept.title
        ));
    }
    let fresh = previous.as_ref().is_none_or(|previous| {
        pool.iter()
            .all(|q| !previous.iter().any(|p| p.prompt == q.prompt))
    });
    for (index, question) in pool.iter_mut().enumerate() {
        question.id = index + 1;
    }
    let takeaways = match &bundled {
        Some(course) if !course.key_takeaways.is_empty() => course.key_takeaways.clone(),
        _ => concept.curriculum.mechanisms.clone(),
    };
    let markdown = format!(
        "## Recall before you check

{}

Say each of these in your own words before opening the check:

{}
",
        concept.curriculum.learner_outcome,
        takeaways
            .iter()
            .map(|line| format!("- {line}"))
            .collect::<Vec<_>>()
            .join(
                "
"
            )
    );
    let stored = StoredEngineeringLesson {
        concept_id: concept.id,
        concept_title: concept.title.clone(),
        category: concept.category.clone(),
        title: format!("Review: {}", concept.title),
        markdown,
        resources: vec![],
        review_notes: vec![],
        research_note: None,
        level: classroom::standard_level(),
        questions: pool,
        exercise: None,
        source: if fresh {
            "retrieval:fresh".into()
        } else {
            "retrieval:repeat".into()
        },
        path: Some(path.clone()),
    };
    let content = PreparedLesson {
        title: stored.title.clone(),
        body: serde_json::to_value(&stored).map_err(e)?,
        provenance: json!({"kind": "retrieval", "material": material, "fresh_sample": fresh, "prepared_at": Utc::now().to_rfc3339()}),
    };
    sessions::publish_preparation(conn, &lease, &content, Utc::now()).map_err(e)?;
    let session = sessions::get(conn, id).map_err(e)?;
    ensure_check_round(conn, &session)?;
    Ok(session)
}

/// Whether a lesson is written for someone starting from scratch: the
/// learner chose to begin at the foundations (or the first stage with nothing
/// familiar) and the topic belongs to the foundations phase. Later phases are
/// taught at the standard level even on that path, because by then the
/// foundations lessons have been read.
pub fn lesson_level(
    path: &crate::domain::classes::AcceptedPath,
    phase: &str,
    first_entry_point: &str,
) -> &'static str {
    use crate::domain::enrollment::EntryChoice;
    let from_scratch = match &path.configuration.entry {
        EntryChoice::Foundations => true,
        EntryChoice::Manual {
            entry_point,
            familiar_competencies,
        } => entry_point == first_entry_point && familiar_competencies.is_empty(),
        EntryChoice::Diagnostic => {
            path.recommendation.route == "foundations"
                || (path.recommendation.entry_point == first_entry_point
                    && path.recommendation.criteria.is_empty())
        }
    };
    if from_scratch && phase == "foundations" {
        "beginner"
    } else {
        "standard"
    }
}

/// What the tutor is told when the learner is an absolute beginner. The
/// section headings stay fixed for the gate; their meaning is restated for
/// someone who has never done any of this.
pub fn beginner_contract(practice_minutes: i64) -> String {
    format!(
        "\nLEARNER LEVEL: ABSOLUTE BEGINNER. This learner chose to start from scratch and this is a foundations topic. Assume they have never done any of this before and know none of the words. Write the whole lesson for them:\n\
- Explain every term the first time it appears, in one plain sentence, before using it. No jargon left undefined, no acronym without its expansion, no \"as you know\".\n\
- One new idea at a time. Every step is something they can do right now on their own machine: the exact thing to type or click, what they will see, and what it means. Show the expected output.\n\
- Use everyday analogies and keep them; skip internals (processes, system calls, memory, source code, standards) unless the brief's own mechanism needs one, and then give it as a one-line picture.\n\
- The section headings are fixed, but write each for a beginner: `Core mechanics` is how it works, one small step at a time; `Mental model` is the picture to keep in their head; `Runnable experiment` is the first thing to try, guaranteed to work on a fresh machine; `Production architecture lens` is where they will meet this in real work, told as one concrete everyday situation, not a system design; `Trade-offs and failure modes` is the mistakes beginners make and how to notice them; `Migration and observability` is how to check what happened and how to undo it safely; `Key takeaways` is what to remember.\n\
- The practical exercise is a first success inside {practice_minutes} minutes: three to five tiny steps with the exact command and the result to expect. Nothing to install, nothing to configure, no design documents.\n\
- The check asks only about things the lesson showed, in the lesson's own words, with choices a beginner can tell apart.\n\
- Reading hints say plainly what to do in each section (read, try, remember); never tell a beginner to skim.\n"
    )
}

/// Run the teaching pipeline for a planned session and publish one immutable
/// lesson version. Failures keep the session and its error for an explicit retry.
pub async fn prepare(state: &AppState, id: &SessionId) -> Result<Session> {
    let (lease, session, program, concept, dossier, contract, profile, path, budget, level) = {
        let conn = state.db.0.lock().unwrap();
        let session = sessions::get(&conn, id).map_err(e)?;
        if matches!(
            session.status,
            Status::Ready | Status::Active | Status::Paused
        ) {
            return Ok(session);
        }
        if session.status.terminal() {
            return Err("This lesson is finished. Start a new one.".into());
        }
        if sessions::preparation(&conn, id).map_err(e)?.status == "failed" {
            sessions::retry_preparation(&conn, id, Utc::now()).map_err(e)?;
        }
        let Some(lease) =
            sessions::claim_preparation(&conn, id, Utc::now(), LEASE_SECONDS).map_err(e)?
        else {
            return Err("This lesson is already being prepared. Wait for it to finish.".into());
        };
        let chosen = selection(&session)?;
        let course_id = session.context.course.course_id.clone();
        let program = classroom::program_row(&conn, &course_id)?;
        let concept = db::get_concept(&conn, chosen.concept_id)
            .map_err(e)?
            .ok_or("the selected topic no longer exists")?;
        let dossier = mastery::build_dossier(&conn, &chosen.service_date, &course_id).map_err(e)?;
        let PlanOwner::Class { path } = &session.context.owner else {
            return Err("This session does not belong to a class.".into());
        };
        let path = classes::path_by_id(&conn, &path.path_revision_id).map_err(e)?;
        let mut contract =
            classroom::contract_with_goal(&program, classroom::subject(&course_id)?.prompt);
        contract.push_str(&format!("\nACCEPTED PERSONAL PATH: {} ({}) · revision {}. {}\nEarlier material is optional, with no completion or mastery credit. Check relevant prerequisites inside this lesson and offer concise refreshers instead of restarting the course. The required outcome remains: {}.\nPrerequisite advice: {}",
            path.recommendation.entry_label, path.recommendation.route, path.revision, path.recommendation.explanation, path.recommendation.required_outcome,
            serde_json::to_string(&path.recommendation.refreshers).map_err(e)?));
        let first_entry = crate::domain::enrollment::options(&course_id)
            .map_err(e)?
            .entry_points
            .first()
            .map(|entry| entry.id.clone())
            .unwrap_or_default();
        let level = lesson_level(&path, &concept.curriculum.phase, &first_entry);
        let minutes = session_minutes(&conn, id, program.session_minutes);
        if level == "beginner" {
            contract.push_str(&beginner_contract(
                crate::generator::LessonBudget::for_minutes(minutes)
                    .plan()
                    .practice_minutes,
            ));
        }
        let tutor = &session.context.tutor;
        let profile = GenerationProfile {
            subject_id: course_id.clone(),
            agent: tutor.provider.clone(),
            model: tutor.model.clone(),
            custom_bin: tutor.custom_agent_bin.clone().unwrap_or_default(),
            prompt_version: format!("{}.{}", program.prompt_profile, program.prompt_version),
        };
        let budget = crate::generator::LessonBudget::for_minutes(minutes);
        (
            lease, session, program, concept, dossier, contract, profile, path, budget, level,
        )
    };
    let course_id = session.context.course.course_id.clone();
    let generated = state
        .generator
        .generate_classroom_course(
            CourseRequest {
                title: &concept.title,
                category: &concept.category,
                dossier: &dossier,
                focus: &course_id,
                curriculum: &concept.curriculum,
                budget,
            },
            &contract,
            &profile,
        )
        .await;
    let conn = state.db.0.lock().unwrap();
    let published = generated.map_err(e).and_then(|(course, source)| {
        let questions = classroom::stored_questions(&course)?;
        let stored = StoredEngineeringLesson {
            concept_id: concept.id,
            concept_title: concept.title.clone(),
            category: concept.category.clone(),
            title: course.title.clone(),
            markdown: course.markdown,
            resources: course.resources,
            review_notes: course.review_notes,
            research_note: course.research_note,
            level: level.to_string(),
            questions,
            exercise: course.exercise,
            source: source.clone(),
            path: Some(path.reference.clone()),
        };
        Ok(PreparedLesson {
            title: course.title,
            body: serde_json::to_value(&stored).map_err(e)?,
            provenance: json!({
                "kind": "generated",
                "runner": profile.agent,
                "model": profile.model,
                "source": source,
                "prompt_version": profile.prompt_version,
                "prompt_profile": program.prompt_profile,
                "generated_at": Utc::now().to_rfc3339(),
            }),
        })
    });
    match published {
        Ok(content) => {
            sessions::publish_preparation(&conn, &lease, &content, Utc::now()).map_err(e)?;
            let session = sessions::get(&conn, id).map_err(e)?;
            ensure_check_round(&conn, &session)?;
            Ok(session)
        }
        Err(message) => {
            let bounded: String = message.chars().take(8000).collect();
            let _ = sessions::fail_preparation(&conn, &lease, &bounded, Utc::now());
            Err(message)
        }
    }
}

fn stored_lesson(
    conn: &Connection,
    id: &SessionId,
) -> Result<Option<(sessions::LessonVersion, StoredEngineeringLesson)>> {
    let Some(lesson) = sessions::lesson(conn, id).map_err(e)? else {
        return Ok(None);
    };
    let stored = serde_json::from_value(lesson.content.body.clone()).map_err(e)?;
    Ok(Some((lesson, stored)))
}

/// The frozen five-question check for a prepared lesson. Idempotent: a second
/// call resumes the same round and never resamples the displayed questions.
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
    let context = json!({
        "lesson_version_id": lesson.id.0,
        "concept_id": stored.concept_id,
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

pub fn check_view(round: &Round) -> CheckView {
    CheckView {
        round_id: round.id.clone(),
        revision: round.revision,
        responses: round.responses.clone(),
        submitted: round.submission.is_some(),
    }
}

fn outcome_result(outcome: &Value) -> Result<Option<EngineeringSessionResult>> {
    if outcome.get("result").is_none() {
        return Ok(None);
    }
    serde_json::from_value(outcome["result"].clone())
        .map(Some)
        .map_err(e)
}

pub fn legacy_status(status: Status) -> &'static str {
    lifecycle_legacy(status)
}

fn lifecycle_legacy(status: Status) -> &'static str {
    match status {
        Status::Completed => "completed",
        Status::Skipped => "skipped",
        _ => "in_progress",
    }
}

/// Render a prepared shared-runtime lesson. Planned/preparing sessions have no
/// content yet and return `None`.
/// The minutes this session was given. A session started from an appointment
/// inherits that day's length; one started by hand takes the class default.
pub fn session_minutes(conn: &Connection, id: &SessionId, default: i64) -> i64 {
    // A block lesson carries the minutes it was given at planning.
    if let Ok(Some(minutes)) = conn.query_row(
        "SELECT json_extract(context_json,'$.selection.minutes') FROM study_sessions WHERE id = ?1",
        [&id.0],
        |r| r.get::<_, Option<i64>>(0),
    ) {
        if minutes > 0 {
            return minutes;
        }
    }
    // The appointment is known before the claim through the session's own
    // context, and after it through the reference the claim records.
    conn.query_row(
        "SELECT o.duration_minutes FROM schedule_occurrences o
         WHERE o.session_ref = ?1
            OR o.id = (SELECT json_extract(context_json,'$.selection.occurrence_id') FROM study_sessions WHERE id = ?2)
         LIMIT 1",
        params![format!("study:{}", id.0), id.0],
        |r| r.get::<_, i64>(0),
    )
    .ok()
    .filter(|m| *m > 0)
    .unwrap_or(default)
}

pub fn view(conn: &Connection, id: &SessionId) -> Result<Option<EngineeringLessonView>> {
    let Some(session) = get(conn, id)? else {
        return Ok(None);
    };
    let Some((_, stored)) = stored_lesson(conn, id)? else {
        return Ok(None);
    };
    let course_id = session.context.course.course_id.clone();
    let program = classroom::program_row(conn, &course_id)?;
    let concept = db::get_concept(conn, stored.concept_id)
        .map_err(e)?
        .ok_or_else(|| "lesson concept no longer exists".to_string())?;
    let prerequisites_json: String = conn
        .query_row(
            "SELECT prereqs_json FROM concepts WHERE id = ?1",
            [concept.id],
            |row| row.get(0),
        )
        .map_err(e)?;
    let session_index = classroom::completed_lesson_count(conn, &course_id)?
        + i64::from(!session.status.terminal());
    let why_now = format!(
        "Session {session_index} advances the {} phase: {}",
        concept.curriculum.phase, concept.curriculum.learner_outcome
    );
    let round = check_round(conn, &session)?;
    let outcome = sessions::result(conn, id)
        .map_err(e)?
        .map(|result| outcome_result(&result.outcome))
        .transpose()?
        .flatten();
    let minutes = session_minutes(conn, id, program.session_minutes);
    Ok(Some(EngineeringLessonView {
        session_id: id.0.clone(),
        runtime: "study".into(),
        lifecycle: serde_json::to_value(session.status)
            .ok()
            .and_then(|value| value.as_str().map(String::from))
            .unwrap_or_default(),
        revision: session.revision,
        checkpoint: Some(session.checkpoint.clone()),
        check: round.as_ref().map(check_view),
        outcome,
        subject_id: course_id,
        label: program.label,
        short_code: program.short_code,
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
        kind: if stored.source.starts_with("retrieval:") {
            "retrieval".into()
        } else {
            "lesson".into()
        },
        fresh_sample: stored.source != "retrieval:repeat",
        agent_used: stored.source,
        prompt_profile: program.prompt_profile,
        prompt_version: program.prompt_version,
        estimated_minutes: minutes,
        plan: crate::generator::LessonBudget::for_minutes(minutes).plan(),
        status: legacy_status(session.status).into(),
    }))
}

/// Foreground the lesson (advisory). Ready and paused sessions become active.
pub fn activate(conn: &Connection, id: &SessionId) -> Result<Session> {
    let session = sessions::get(conn, id).map_err(e)?;
    match session.status {
        Status::Active => Ok(session),
        Status::Ready | Status::Paused => {
            sessions::activate(conn, id, session.revision, Utc::now()).map_err(e)
        }
        Status::Completed | Status::Skipped => Err("This lesson is finished.".into()),
        Status::Planned | Status::Preparing => Err("This lesson is not prepared yet.".into()),
    }
}

pub fn pause(conn: &Connection, id: &SessionId) -> Result<Session> {
    let session = sessions::get(conn, id).map_err(e)?;
    if session.status != Status::Active {
        return Ok(session);
    }
    sessions::pause(conn, id, session.revision, Utc::now()).map_err(e)
}

/// Merge saved work into the session checkpoint. Reading position, stage and
/// editor fields are learner work, never credit.
pub fn patch_work(
    conn: &Connection,
    id: &SessionId,
    expected_revision: u32,
    stage: Option<Stage>,
    reading: Option<ReadingPosition>,
    work: BTreeMap<String, Value>,
) -> Result<Checkpoint> {
    let session = sessions::get(conn, id).map_err(e)?;
    let mut body = session.checkpoint.body.clone();
    if let Some(stage) = stage {
        if stage == Stage::Feedback {
            return Err("Feedback is reached by submitting the knowledge check.".into());
        }
        body.stage = stage;
    }
    if let Some(reading) = reading {
        body.reading = reading;
    }
    for (key, value) in work {
        if value.is_null() {
            body.work.remove(&key);
        } else {
            body.work.insert(key, value);
        }
    }
    sessions::save_checkpoint(conn, id, expected_revision, &body, Utc::now()).map_err(e)
}

/// Convenience for the exercise workspace: draft, completion and reflection.
pub fn exercise_view(conn: &Connection, id: &SessionId) -> Result<Option<db::ExerciseView>> {
    let Some(session) = get(conn, id)? else {
        return Ok(None);
    };
    let Some((_, stored)) = stored_lesson(conn, id)? else {
        return Ok(None);
    };
    let work = &session.checkpoint.body.work;
    Ok(stored.exercise.map(|exercise| db::ExerciseView {
        course_id: None,
        classroom_session_id: None,
        study_session_id: Some(id.0.clone()),
        title: exercise.title,
        instructions: exercise.instructions,
        starter_code: exercise.starter_code,
        deliverable: exercise.deliverable,
        hints: exercise.hints,
        draft: work
            .get("exercise_draft")
            .and_then(Value::as_str)
            .filter(|draft| !draft.is_empty())
            .map(String::from),
        completed: work
            .get("exercise_completed")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        reflection: work
            .get("exercise_reflection")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    }))
}

pub fn checkpoint_revision(conn: &Connection, id: &SessionId) -> Result<u32> {
    Ok(sessions::get(conn, id).map_err(e)?.checkpoint.revision)
}

/// Exercise draft/completion saves are single-editor and debounced, so they
/// merge into the latest checkpoint revision under the command lock.
pub fn save_exercise_work(
    conn: &Connection,
    id: &SessionId,
    draft: Option<String>,
    completion: Option<(bool, String)>,
) -> Result<()> {
    let session = sessions::get(conn, id).map_err(e)?;
    let mut work = BTreeMap::new();
    if let Some(draft) = draft {
        work.insert("exercise_draft".to_string(), Value::String(draft));
    }
    if let Some((completed, reflection)) = completion {
        work.insert("exercise_completed".to_string(), Value::Bool(completed));
        work.insert(
            "exercise_reflection".to_string(),
            Value::String(reflection.trim().to_string()),
        );
    }
    patch_work(conn, id, session.checkpoint.revision, None, None, work).map(|_| ())
}

/// The tutor snapshotted at planning, for auxiliary calls about this lesson.
pub fn generation_profile(conn: &Connection, id: &SessionId) -> Result<GenerationProfile> {
    let session = sessions::get(conn, id).map_err(e)?;
    let course_id = session.context.course.course_id.clone();
    let program = classroom::program_row(conn, &course_id)?;
    let tutor = &session.context.tutor;
    Ok(GenerationProfile {
        subject_id: course_id,
        agent: tutor.provider.clone(),
        model: tutor.model.clone(),
        custom_bin: tutor.custom_agent_bin.clone().unwrap_or_default(),
        prompt_version: format!("{}.{}", program.prompt_profile, program.prompt_version),
    })
}

pub fn chat_context(
    conn: &Connection,
    id: &SessionId,
) -> Result<crate::generator::CourseChatContext> {
    let (_, stored) = stored_lesson(conn, id)?.ok_or("lesson is not prepared yet")?;
    let concept = db::get_concept(conn, stored.concept_id)
        .map_err(e)?
        .ok_or_else(|| "lesson concept not found".to_string())?;
    let session = sessions::get(conn, id).map_err(e)?;
    let exercise = stored
        .exercise
        .as_ref()
        .map(serde_json::to_string_pretty)
        .transpose()
        .map_err(e)?
        .unwrap_or_else(|| "(this course has no separate exercise)".into());
    Ok(crate::generator::CourseChatContext {
        title: stored.title,
        focus: session.context.course.course_id,
        markdown: stored.markdown,
        learner_outcome: concept.curriculum.learner_outcome,
        cumulative_artifact: concept.curriculum.artifact,
        exercise,
    })
}

fn stored_question(item: &Item) -> Result<StoredQuestion> {
    serde_json::from_value(item.body.clone()).map_err(e)
}

/// Save one knowledge-check answer against the frozen round. `None` clears it.
pub fn save_answer(
    conn: &Connection,
    id: &SessionId,
    round_id: &RoundId,
    expected_revision: u32,
    question_id: usize,
    choice: Option<usize>,
) -> Result<CheckView> {
    let round = assessments::round(conn, round_id).map_err(e)?;
    let item = round
        .items
        .iter()
        .find(|item| item.id == question_id.to_string())
        .ok_or("Question does not belong to the displayed knowledge check.")?;
    // Any adapter's frozen item exposes its displayed choices the same way.
    let choice_count = item.body["choices"].as_array().map_or(0, Vec::len);
    let response = match choice {
        Some(index) if index < choice_count => Response {
            answer: index.to_string(),
            status: ResponseStatus::Answered,
        },
        Some(_) => return Err("Choose one of the displayed answers.".into()),
        None => Response {
            answer: String::new(),
            status: ResponseStatus::Draft,
        },
    };
    let saved = assessments::save_response(
        conn,
        &Owner::StudySession(id.0.clone()),
        round_id,
        expected_revision,
        &item.id,
        response,
    )
    .map_err(e)?;
    Ok(check_view(&saved))
}

fn grade(round: &Round) -> Result<(f64, Vec<ClassroomCorrectionView>, Vec<StoredQuestion>)> {
    let mut correct_count = 0usize;
    let mut corrections = Vec::new();
    let mut questions = Vec::new();
    for item in &round.items {
        let question = stored_question(item)?;
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
        correct_count += usize::from(correct);
        corrections.push(ClassroomCorrectionView {
            question_id: question.id,
            prompt: question.prompt.clone(),
            selected_answer: question.choices[selected].clone(),
            correct_answer: question.choices[question.correct_index].clone(),
            correct,
            explanation: question.explanation.clone(),
        });
        questions.push(question);
    }
    let score = if round.items.is_empty() {
        1.0
    } else {
        correct_count as f64 / round.items.len() as f64
    };
    Ok((score, corrections, questions))
}

/// Grade exactly the displayed round, then complete the session: result,
/// check evidence, mastery and the finished lifecycle commit together. A
/// repeated submission returns the original result.
pub fn submit(
    conn: &Connection,
    id: &SessionId,
    round_id: &RoundId,
    expected_revision: u32,
    reflection: &str,
    today: &str,
) -> Result<EngineeringSessionResult> {
    if let Some(previous) = sessions::result(conn, id).map_err(e)? {
        return outcome_result(&previous.outcome)?
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
    let (score, corrections, questions) = grade(&round)?;
    let course_id = session.context.course.course_id.clone();
    let concept_id = selection(&session)?.concept_id;
    let retrieval =
        stored_lesson(conn, id)?.is_some_and(|(_, stored)| stored.source.starts_with("retrieval:"));
    let fresh_sample =
        stored_lesson(conn, id)?.is_none_or(|(_, stored)| stored.source != "retrieval:repeat");
    let result = EngineeringSessionResult {
        session_id: id.0.clone(),
        subject_id: course_id,
        passed: score >= 0.8,
        score,
        corrections: corrections.clone(),
        kind: if retrieval {
            "retrieval".into()
        } else {
            "lesson".into()
        },
        fresh_sample,
    };
    let result_value = serde_json::to_value(&result).map_err(e)?;
    let mut body = session.checkpoint.body.clone();
    body.stage = Stage::Feedback;
    body.work
        .insert("reflection".into(), Value::String(reflection.trim().into()));
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
            assessments::submit_in_transaction(tx, &owner, &round, &result_value, true)?;
            let now = crate::language::now_iso();
            for (question, correction) in questions.iter().zip(&corrections) {
                let misconception = if correction.correct {
                    String::new()
                } else {
                    format!(
                        "Selected “{}” instead of “{}”. {}",
                        correction.selected_answer,
                        correction.correct_answer,
                        correction.explanation
                    )
                };
                tx.execute(
                    "INSERT INTO classroom_exit_attempts
                        (study_session_id, concept_id, question_id, section, learning_objective,
                         misconception, correct, attempted_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        id.0,
                        concept_id,
                        question.id as i64,
                        question.section,
                        question.learning_objective,
                        misconception,
                        correction.correct as i64,
                        now,
                    ],
                )?;
            }
            if !retrieval {
                mastery::record_course_read(tx, concept_id, &today)?;
            }
            mastery::record_quiz_outcome(tx, concept_id, &today, score)?;
            crate::domain::schedule::finish_step(tx, &format!("study:{}", id.0), true, Utc::now())?;
            Ok(json!({"kind": "completed", "concept_id": concept_id, "result": result_value}))
        },
    )
    .map_err(e)?;
    outcome_result(&outcome.outcome)?.ok_or_else(|| "Session result is missing.".into())
}

/// Discard a lesson without credit. Saved work and any prepared content remain
/// readable as history; an active class placement is untouched.
pub fn skip(conn: &Connection, id: &SessionId) -> Result<Session> {
    let session = sessions::get(conn, id).map_err(e)?;
    if session.status.terminal() {
        return Ok(session);
    }
    sessions::finish(
        conn,
        id,
        session.revision,
        session.checkpoint.revision,
        Disposition::Skipped,
        Utc::now(),
        |tx, _| {
            crate::domain::schedule::finish_step(
                tx,
                &format!("study:{}", id.0),
                false,
                Utc::now(),
            )?;
            Ok(json!({"kind": "skipped"}))
        },
    )
    .map_err(e)?;
    sessions::get(conn, id).map_err(e)
}

// ── Readers shared with the compatibility engine ───────────────────────────

pub fn completed_count(conn: &Connection, course_id: &str) -> Result<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM study_sessions s JOIN classes c ON c.id = s.class_id
         WHERE c.course_id = ?1 AND s.status = 'completed'",
        [course_id],
        |row| row.get(0),
    )
    .map_err(e)
}

/// A rule is consumed for a service date once a shared-runtime session was
/// planned for it, whatever that session's later disposition.
pub fn slot_consumed(conn: &Connection, slot_id: i64, service_date: &str) -> Result<bool> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM study_sessions
         WHERE owner_kind = 'class'
           AND json_extract(context_json, '$.selection.slot_id') = ?1
           AND json_extract(context_json, '$.selection.service_date') = ?2)",
        params![slot_id, service_date],
        |row| row.get(0),
    )
    .map_err(e)
}

/// Status of the shared-runtime session planned for a rule on a service date.
pub fn slot_session_status(
    conn: &Connection,
    slot_id: i64,
    service_date: &str,
) -> Result<Option<String>> {
    conn.query_row(
        "SELECT status FROM study_sessions
         WHERE owner_kind = 'class'
           AND json_extract(context_json, '$.selection.slot_id') = ?1
           AND json_extract(context_json, '$.selection.service_date') = ?2
         ORDER BY rowid DESC LIMIT 1",
        params![slot_id, service_date],
        |row| row.get(0),
    )
    .optional()
    .map_err(e)
}

pub fn has_open_session(conn: &Connection, course_id: &str) -> Result<bool> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM study_sessions s JOIN classes c ON c.id = s.class_id
         WHERE c.course_id = ?1 AND s.status NOT IN ('completed','skipped'))",
        [course_id],
        |row| row.get(0),
    )
    .map_err(e)
}

/// Resumable engineering lessons for Today and the class workspace.
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
               AND json_extract(s.context_json, '$.selection.adapter') = 'engineering'
             ORDER BY s.rowid",
        )
        .map_err(e)?;
    let rows = statement
        .query_map([], |row| {
            Ok(ActiveClassroomSessionView {
                session_id: row.get::<_, String>(0)?,
                subject_id: row.get(1)?,
                kind: "engineering".into(),
                label: row.get(2)?,
                title: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                runtime: "study".into(),
                lifecycle: row.get(3)?,
            })
        })
        .map_err(e)?;
    rows.collect::<std::result::Result<Vec<_>, _>>().map_err(e)
}

/// Legacy-shaped status for readers that still switch on `in_progress`.
pub fn legacy_status_of(status: &str) -> &'static str {
    match status {
        "completed" => "completed",
        "skipped" => "skipped",
        _ => "in_progress",
    }
}

#[allow(dead_code)]
fn _assert_stage_order() {
    debug_assert!(STAGES.windows(2).all(|pair| pair[0] < pair[1]));
    let _ = CheckpointBody {
        stage: Stage::Learn,
        reading: ReadingPosition::default(),
        work: BTreeMap::new(),
    };
}
