//! Focus ownership rules for class sessions, independent of any OS lock.
use principia_desk_lib::{
    classroom::{
        self, ConfigureClassroomInput, StoredEngineeringLesson, StoredQuestion,
        UpsertClassroomSlotInput,
    },
    db,
    domain::{
        classes::{self, AcceptPath},
        enrollment::{self, FocusPolicy, SaveEnrollmentDraft},
        placement,
        sessions::{self, PreparedLesson, Status},
    },
    enforcement::{level_for, Coordinator},
    kiosk::KioskLevel,
    language,
    subjects::engineering,
};
use rusqlite::Connection;

const TODAY: &str = "2026-09-14";

fn fixture() -> Connection {
    let dir = std::env::temp_dir().join(format!(
        "principia-enforcement-{}-{:016x}",
        std::process::id(),
        rand::random::<u64>()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let conn = db::open(&dir.join("learner.db")).unwrap();
    db::seed_concepts(&conn, include_str!("../seed/concepts.json")).unwrap();
    language::initialize(&conn, TODAY).unwrap();
    classroom::initialize(&conn).unwrap();
    conn
}
fn activate_class(conn: &Connection, subject: &str, hour: u32) {
    classroom::upsert_slot(
        conn,
        &UpsertClassroomSlotInput {
            id: None,
            subject_id: subject.into(),
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
            subject_id: subject.into(),
            enabled: true,
            agent: "claude".into(),
            model: "sonnet".into(),
            custom_agent_bin: String::new(),
            session_minutes: 30,
            start_level: None,
            target_level: None,
            weekly_minutes: None,
        },
        TODAY,
    )
    .unwrap();
}
fn published(conn: &Connection, subject: &str) -> sessions::Session {
    let program = classroom::program_row(conn, subject).unwrap();
    let planned = engineering::plan(conn, &program, None, None, TODAY, false).unwrap();
    let chosen = engineering::selection(&planned).unwrap();
    let lease = sessions::claim_preparation(conn, &planned.id, chrono::Utc::now(), 60)
        .unwrap()
        .unwrap();
    let stored = StoredEngineeringLesson {
        concept_id: chosen.concept_id,
        concept_title: chosen.title.clone(),
        category: chosen.category.clone(),
        title: "Lesson".into(),
        markdown: "## Body".into(),
        resources: vec![],
        review_notes: vec![],
        research_note: None,
        level: "standard".into(),
        questions: vec![StoredQuestion {
            id: 1,
            prompt: "Q".into(),
            choices: vec!["a".into(), "b".into()],
            correct_index: 0,
            explanation: "e".into(),
            section: String::new(),
            learning_objective: "o".into(),
        }],
        exercise: None,
        source: "fixture".into(),
        path: None,
    };
    sessions::publish_preparation(
        conn,
        &lease,
        &PreparedLesson {
            title: "Lesson".into(),
            body: serde_json::to_value(&stored).unwrap(),
            provenance: serde_json::json!({"kind":"fixture"}),
        },
        chrono::Utc::now(),
    )
    .unwrap();
    sessions::get(conn, &planned.id).unwrap()
}

#[test]
fn a_class_focus_policy_is_snapshotted_into_future_sessions_only() {
    let conn = fixture();
    activate_class(&conn, "typescript", 9);
    let advisory = published(&conn, "typescript");
    assert_eq!(advisory.context.focus_policy, FocusPolicy::Advisory);
    let config =
        classes::set_focus_policy(&conn, "typescript", FocusPolicy::Strict, TODAY).unwrap();
    assert_eq!(config.focus_policy, FocusPolicy::Strict);
    assert_eq!(
        classroom::program_view(&conn, "typescript", TODAY)
            .unwrap()
            .focus_policy,
        "strict"
    );
    // The already planned session keeps its advisory snapshot and activates plainly.
    assert_eq!(
        sessions::get(&conn, &advisory.id)
            .unwrap()
            .context
            .focus_policy,
        FocusPolicy::Advisory
    );
    engineering::activate(&conn, &advisory.id).unwrap();
    engineering::skip(&conn, &advisory.id).unwrap();
    let strict = published(&conn, "typescript");
    assert_eq!(strict.context.focus_policy, FocusPolicy::Strict);
    assert!(engineering::activate(&conn, &strict.id)
        .unwrap_err()
        .contains("focus coordinator"));
}

#[test]
fn the_coordinator_admits_one_focused_session_and_blocks_competitors_until_release() {
    let conn = fixture();
    activate_class(&conn, "typescript", 9);
    activate_class(&conn, "javascript", 11);
    classes::set_focus_policy(&conn, "typescript", FocusPolicy::Focused, TODAY).unwrap();
    classes::set_focus_policy(&conn, "javascript", FocusPolicy::Strict, TODAY).unwrap();
    let ts = published(&conn, "typescript");
    let js = published(&conn, "javascript");
    let coordinator = Coordinator::default();
    let grant = coordinator
        .admit(&ts)
        .unwrap()
        .expect("focused sessions need a grant");
    let active =
        sessions::activate_focused(&conn, &ts.id, ts.revision, chrono::Utc::now(), grant).unwrap();
    assert_eq!(active.status, Status::Active);
    assert_eq!(
        level_for(&active.context.focus_policy),
        Some(KioskLevel::Firm)
    );
    assert_eq!(level_for(&js.context.focus_policy), Some(KioskLevel::Hard));
    assert_eq!(level_for(&FocusPolicy::Advisory), None);
    // Ownership is recorded by the app-level activation; simulate it here.
    coordinator.own(&active).unwrap();
    let blocked = coordinator.admit(&js).unwrap_err();
    assert!(blocked.contains("typescript"), "{blocked}");
    assert!(
        coordinator.admit(&active).unwrap().is_some(),
        "the holder may re-admit itself"
    );
    assert!(coordinator.release_owner(&active.id.0));
    assert!(
        !coordinator.release_owner(&active.id.0),
        "release is idempotent"
    );
    assert!(coordinator.admit(&js).unwrap().is_some());
}

#[test]
fn unprepared_focused_sessions_cannot_take_focus_and_accepting_focused_paths_is_allowed() {
    let conn = fixture();
    activate_class(&conn, "linux-bash", 8);
    let options = enrollment::options("linux-bash").unwrap();
    let mut configuration = options.default_configuration.clone();
    configuration.focus_policy = FocusPolicy::Focused;
    let draft = enrollment::save_draft(
        &conn,
        &SaveEnrollmentDraft {
            id: None,
            expected_revision: None,
            course: options.course,
            configuration,
        },
    )
    .unwrap();
    let recommendation = placement::recommend(&conn, &draft.id, draft.revision).unwrap();
    let path = classes::accept(
        &conn,
        &AcceptPath {
            draft_id: draft.id,
            expected_revision: draft.revision,
            recommendation_id: recommendation.id,
        },
        TODAY,
    )
    .unwrap();
    assert_eq!(path.configuration.focus_policy, FocusPolicy::Focused);
    let program = classroom::program_row(&conn, "linux-bash").unwrap();
    let planned = engineering::plan(&conn, &program, None, None, TODAY, false).unwrap();
    let coordinator = Coordinator::default();
    let error = coordinator.admit(&planned).unwrap_err();
    assert!(error.contains("not prepared"), "{error}");
}
