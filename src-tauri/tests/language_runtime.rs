//! CEFR language classes on the shared study runtime.
use principia_desk_lib::{
    classroom::{self, ConfigureClassroomInput, UpsertClassroomSlotInput},
    db,
    domain::{
        assessments::{self, Owner, Purpose},
        classes,
        sessions::{self, Stage, Status},
    },
    language,
    progress::{self, ProgressQuery},
    subjects::language as adapter,
};
use rusqlite::Connection;

const TODAY: &str = "2026-07-21";

fn fixture() -> (std::path::PathBuf, Connection) {
    let dir = std::env::temp_dir().join(format!(
        "principia-language-runtime-{}-{:016x}",
        std::process::id(),
        rand::random::<u64>()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("learner.db");
    let conn = db::open(&path).unwrap();
    db::seed_concepts(&conn, include_str!("../seed/concepts.json")).unwrap();
    language::initialize(&conn, TODAY).unwrap();
    classroom::initialize(&conn).unwrap();
    (path, conn)
}

fn activate(conn: &Connection, language: &str, hour: u32) -> i64 {
    let slot = classroom::upsert_slot(
        conn,
        &UpsertClassroomSlotInput {
            id: None,
            subject_id: language.into(),
            hour,
            minute: 0,
            weekdays: vec![1, 2, 3, 4, 5, 6, 7],
            enabled: true,
            durations: Default::default(),
            starts: Default::default(),
        },
    )
    .unwrap();
    classroom::configure_program(
        conn,
        &ConfigureClassroomInput {
            subject_id: language.into(),
            enabled: true,
            agent: "claude".into(),
            model: "sonnet".into(),
            custom_agent_bin: String::new(),
            session_minutes: 30,
            start_level: Some("A1".into()),
            target_level: Some("A2".into()),
            weekly_minutes: Some(210),
        },
        TODAY,
    )
    .unwrap();
    slot
}

fn plan(conn: &Connection, language: &str, slot: Option<i64>) -> sessions::Session {
    let program = classroom::program_row(conn, language).unwrap();
    adapter::plan(conn, &program, slot, None, TODAY, false).unwrap()
}

fn correct_index(conn: &Connection, session: &sessions::Session, index: usize) -> usize {
    conn.query_row(
        "SELECT json_extract(v.content_json, ?2) FROM lesson_versions v JOIN study_sessions s ON s.lesson_version_id = v.id WHERE s.id = ?1",
        rusqlite::params![session.id.0, format!("$.body.questions[{index}].correct_index")],
        |r| r.get::<_, i64>(0),
    )
    .unwrap() as usize
}

#[test]
fn a_language_lesson_is_planned_from_the_curated_unit_and_published_without_a_tutor() {
    let (path, conn) = fixture();
    let slot = activate(&conn, "german", 7);
    let planned = plan(&conn, "german", Some(slot));
    let chosen = adapter::selection(&planned).unwrap();
    assert_eq!(
        (chosen.adapter.as_str(), chosen.level.as_str(), chosen.phase),
        ("language", "A1", 1)
    );
    assert!(!chosen.title.is_empty());
    let path_ref = classes::current_path(&conn, "german").unwrap().unwrap();
    assert_eq!(path_ref.recommendation.route, "foundations");
    assert_eq!(plan(&conn, "german", None).id, planned.id);
    assert!(adapter::view(&conn, &planned.id).unwrap().is_none());
    let ready = adapter::publish_curated(&conn, &planned.id).unwrap();
    assert_eq!(ready.status, Status::Ready);
    let view = adapter::view(&conn, &ready.id).unwrap().unwrap();
    assert_eq!(
        (
            view.runtime.as_str(),
            view.lifecycle.as_str(),
            view.status.as_str()
        ),
        ("study", "ready", "in_progress")
    );
    assert_eq!(view.language, "german");
    assert!(!view.questions.is_empty());
    assert!(view.markdown.split_whitespace().count() > 50);
    let check = view.check.clone().unwrap();
    assert!(!check.submitted);
    // Publishing again is idempotent and the frozen round is reused.
    assert_eq!(
        adapter::publish_curated(&conn, &ready.id).unwrap().status,
        Status::Ready
    );
    assert_eq!(
        adapter::view(&conn, &ready.id)
            .unwrap()
            .unwrap()
            .check
            .unwrap()
            .round_id,
        check.round_id
    );
    assert!(classroom::validate_slot_start(&conn, "german", slot, TODAY).is_err());
    let summaries = classroom::active_sessions(&conn).unwrap();
    assert!(summaries
        .iter()
        .any(|s| s.session_id == ready.id.0 && s.kind == "language" && s.runtime == "study"));
    drop(conn);
    let conn = db::open(&path).unwrap();
    let activated = adapter::activate(&conn, &ready.id).unwrap();
    assert_eq!(activated.status, Status::Active);
    let saved = adapter::save_answer(&conn, &ready.id, &check.round_id, 0, 1, Some(0)).unwrap();
    assert_eq!(saved.revision, 1);
    assert_eq!(saved.responses["1"].answer, "0");
}

#[test]
fn completing_a_language_lesson_records_strand_evidence_unit_progress_and_the_result() {
    let (_, conn) = fixture();
    activate(&conn, "italian", 8);
    let session = adapter::publish_curated(&conn, &plan(&conn, "italian", None).id).unwrap();
    adapter::activate(&conn, &session.id).unwrap();
    let view = adapter::view(&conn, &session.id).unwrap().unwrap();
    let mut check = view.check.unwrap();
    let total = view.questions.len();
    for (index, question) in view.questions.iter().enumerate() {
        let choice = if index == 0 {
            (correct_index(&conn, &session, 0) + 1) % question.choices.len()
        } else {
            correct_index(&conn, &session, index)
        };
        check = adapter::save_answer(
            &conn,
            &session.id,
            &check.round_id,
            check.revision,
            question.id,
            Some(choice),
        )
        .unwrap();
    }
    let input = adapter::LanguageCheckInput {
        writing_response: "Ciao, mi chiamo Lucia e abito a Milano con la mia famiglia da tre anni."
            .into(),
        speaking_completed: true,
        listened: true,
        confidence: 4,
    };
    let result = adapter::submit(
        &conn,
        &session.id,
        &check.round_id,
        check.revision,
        &input,
        TODAY,
    )
    .unwrap();
    assert_eq!(result["session_id"], session.id.0);
    let expected_knowledge = (total - 1) as f64 / total as f64;
    assert!(result["passed"].as_bool().unwrap() == (expected_knowledge >= 0.6));
    assert_eq!(result["corrections"].as_array().unwrap().len(), total);
    assert!(!result["corrections"][0]["correct"].as_bool().unwrap());
    assert_eq!(result["current_level"], "A1");
    let finished = sessions::get(&conn, &session.id).unwrap();
    assert_eq!(finished.status, Status::Completed);
    assert_eq!(finished.checkpoint.body.stage, Stage::Feedback);
    assert_eq!(
        finished.checkpoint.body.work["confidence"],
        serde_json::json!(4)
    );
    let attempt = assessments::latest(
        &conn,
        &Owner::StudySession(session.id.0.clone()),
        Purpose::ExitCheck,
    )
    .unwrap()
    .unwrap();
    assert_eq!(attempt.status, "completed");
    let chosen = adapter::selection(&session).unwrap();
    let (phase_completed, encounters): (i64, i64) = conn
        .query_row(
            "SELECT phase_completed, encounters FROM language_unit_progress WHERE language='italian' AND unit_slug=?1",
            [&chosen.unit_slug],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(encounters, 1);
    assert_eq!(
        phase_completed,
        i64::from(result["passed"].as_bool().unwrap())
    );
    let strands: i64 = conn
        .query_row("SELECT COUNT(*) FROM language_skill_scores WHERE language='italian' AND encounters > 0", [], |r| r.get(0))
        .unwrap();
    assert!(
        strands >= 3,
        "writing, listening and spoken evidence were recorded"
    );
    let progress = language::program_view(&conn, "italian", TODAY).unwrap();
    assert_eq!(
        progress.completed_steps,
        i64::from(result["passed"].as_bool().unwrap())
    );
    // Repeating the submission returns the original result without new evidence.
    let again = adapter::submit(
        &conn,
        &session.id,
        &check.round_id,
        check.revision,
        &input,
        TODAY,
    )
    .unwrap();
    assert_eq!(again["score"], result["score"]);
    let encounters_again: i64 = conn
        .query_row("SELECT encounters FROM language_unit_progress WHERE language='italian' AND unit_slug=?1", [&chosen.unit_slug], |r| r.get(0))
        .unwrap();
    assert_eq!(encounters_again, 1);
    let dashboard = progress::read(&conn, TODAY, &ProgressQuery::default()).unwrap();
    let entry = dashboard
        .history
        .iter()
        .find(|e| e.source == "study")
        .unwrap();
    assert_eq!(
        (
            entry.subject_id.as_str(),
            entry.status.as_str(),
            entry.can_read
        ),
        ("italian", "completed", true)
    );
    assert!(progress::lesson(&conn, "study", &session.id.0)
        .unwrap()
        .unwrap()
        .markdown
        .contains(&view.markdown[..40]));
    let reopened = adapter::view(&conn, &session.id).unwrap().unwrap();
    assert_eq!(reopened.status, "completed");
    assert_eq!(reopened.outcome.unwrap()["current_level"], "A1");
    // The next lesson advances the rotation instead of repeating the same pass.
    let next = plan(&conn, "italian", None);
    let next_chosen = adapter::selection(&next).unwrap();
    assert!(next_chosen.unit_slug != chosen.unit_slug || next_chosen.phase != chosen.phase);
}

#[test]
fn an_unanswered_language_check_cannot_be_submitted_and_skipping_keeps_the_lesson_readable() {
    let (_, conn) = fixture();
    activate(&conn, "german", 9);
    let session = adapter::publish_curated(&conn, &plan(&conn, "german", None).id).unwrap();
    adapter::activate(&conn, &session.id).unwrap();
    let check = adapter::view(&conn, &session.id)
        .unwrap()
        .unwrap()
        .check
        .unwrap();
    let input = adapter::LanguageCheckInput {
        writing_response: String::new(),
        speaking_completed: false,
        listened: false,
        confidence: 3,
    };
    let error = adapter::submit(
        &conn,
        &session.id,
        &check.round_id,
        check.revision,
        &input,
        TODAY,
    )
    .unwrap_err();
    assert!(error.contains("Answer every"), "{error}");
    let skipped = adapter::skip(&conn, &session.id).unwrap();
    assert_eq!(skipped.status, Status::Skipped);
    let view = adapter::view(&conn, &session.id).unwrap().unwrap();
    assert_eq!(view.status, "skipped");
    assert!(view.outcome.is_none());
    let progress = language::program_view(&conn, "german", TODAY).unwrap();
    assert_eq!(progress.completed_steps, 0);
    assert!(classroom::active_sessions(&conn).unwrap().is_empty());
    assert_ne!(plan(&conn, "german", None).id, session.id);
}
