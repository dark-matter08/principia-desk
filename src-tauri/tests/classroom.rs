use principia_desk_lib::{
    classroom::{
        self, AvailabilityWindowInput, ConfigureClassroomInput, PlanClassroomScheduleInput,
        UpsertClassroomSlotInput,
    },
    db::{self, Session},
    language, mastery, selection,
};
use rusqlite::params;

static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn test_db() -> rusqlite::Connection {
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("sdr-classroom-{}-{}", std::process::id(), n));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let conn = db::open(&dir.join("test.db")).unwrap();
    db::seed_concepts(&conn, include_str!("../seed/concepts.json")).unwrap();
    language::initialize(&conn, "2026-07-21").unwrap();
    classroom::initialize(&conn).unwrap();
    db::set_config(&conn, "schedule_hour", "19").unwrap();
    db::set_config(&conn, "schedule_minute", "0").unwrap();
    conn
}

fn configure(conn: &rusqlite::Connection, subject_id: &str, agent: &str) {
    let is_language = subject_id == "german" || subject_id == "italian";
    classroom::configure_program(
        conn,
        &ConfigureClassroomInput {
            subject_id: subject_id.into(),
            enabled: false,
            agent: agent.into(),
            model: "sonnet".into(),
            custom_agent_bin: String::new(),
            session_minutes: 30,
            start_level: is_language.then(|| "A1".into()),
            target_level: is_language.then(|| "A2".into()),
            weekly_minutes: is_language.then_some(210),
        },
        "2026-07-21",
    )
    .unwrap();
    // Legacy enabled-program fixtures can predate schedules. Activation itself
    // is exercised separately below with the new schedule prerequisite.
    conn.execute(
        "UPDATE classroom_programs SET enabled=1 WHERE subject_id=?1",
        [subject_id],
    )
    .unwrap();
    if is_language {
        conn.execute(
            "UPDATE language_programs SET enabled=1 WHERE language=?1",
            [subject_id],
        )
        .unwrap();
    }
}

#[test]
fn planned_time_collision_never_changes_a_manual_slot_or_program() {
    let conn = test_db();
    configure(&conn, "german", "claude");
    let manual_id = classroom::upsert_slot(
        &conn,
        &UpsertClassroomSlotInput {
            id: None,
            subject_id: "german".into(),
            hour: 7,
            minute: 0,
            weekdays: vec![7],
            enabled: true,
            durations: Default::default(),
            starts: Default::default(),
        },
    )
    .unwrap();
    for commit in [false, true] {
        let result = classroom::plan_schedule(
            &conn,
            &PlanClassroomScheduleInput {
                subject_id: "german".into(),
                learning_goal: "must not replace the old goal".into(),
                target_weekly_minutes: 90,
                commit,
                windows: vec![AvailabilityWindowInput {
                    weekdays: vec![1, 3, 5],
                    start_hour: 7,
                    start_minute: 0,
                    end_hour: 8,
                    end_minute: 0,
                }],
            },
            "2026-07-21",
        );
        assert!(result.unwrap_err().contains("manual class"));
        let slot = classroom::slot_views(&conn, "2026-07-21", false)
            .unwrap()
            .into_iter()
            .find(|s| s.id == manual_id)
            .unwrap();
        assert_eq!(slot.source, "manual");
        assert_eq!(slot.weekdays, vec![7]);
        let goal: String = conn
            .query_row(
                "SELECT learning_goal FROM classroom_programs WHERE subject_id = 'german'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(goal.is_empty());
    }
}

#[test]
fn restarting_does_not_recreate_a_deleted_imported_language_schedule() {
    let conn = test_db();
    // Simulate an installation that has not performed the legacy import yet.
    conn.execute(
        "DELETE FROM config WHERE key = 'migration:classroom_language_slots:v1'",
        [],
    )
    .unwrap();
    conn.execute("INSERT INTO language_schedule_slots (language, hour, minute, weekdays_json, enabled, created_at) VALUES ('german', 7, 0, '[1,2,3,4,5]', 1, 'now')", []).unwrap();
    classroom::initialize(&conn).unwrap();
    let slot = classroom::slot_views(&conn, "2026-07-21", false)
        .unwrap()
        .into_iter()
        .find(|s| s.subject_id == "german")
        .unwrap();
    classroom::delete_slot(&conn, slot.id).unwrap();
    classroom::initialize(&conn).unwrap();
    assert!(classroom::slot_views(&conn, "2026-07-21", false)
        .unwrap()
        .iter()
        .all(|s| s.id != slot.id));
}

#[test]
fn classroom_seeds_every_subject_with_an_isolated_prompt_contract() {
    let conn = test_db();
    let programs = classroom::program_views(&conn, "2026-07-21").unwrap();
    assert_eq!(programs.len(), 9);
    assert_eq!(programs.len(), classroom::subjects().len());
    classroom::prompt_contracts_are_isolated().unwrap();
    for program in programs {
        assert_eq!(
            program.prompt_profile,
            format!("classroom.{}", program.subject_id)
        );
        assert_eq!(program.prompt_version, "v1");
    }
}

#[test]
fn classroom_reader_persists_exercise_work_and_exposes_chat_context() {
    let conn = test_db();
    let concept_id: i64 = conn
        .query_row(
            "SELECT id FROM concepts WHERE focus = 'javascript' ORDER BY id LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let payload = serde_json::json!({
        "concept_id": concept_id,
        "concept_title": "Runtime boundaries",
        "category": "architecture",
        "title": "A reader fixture",
        "markdown": "## The simple version\n\nA grounded classroom lesson.",
        "resources": [],
        "questions": [],
        "exercise": {
            "title": "Build a boundary probe",
            "instructions": "Implement the probe and record the evidence that demonstrates the boundary.",
            "starter_code": "export const probe = () => true;",
            "deliverable": "A working probe with evidence.",
            "hints": ["Start at the public contract."]
        },
        "source": "fixture"
    })
    .to_string();
    conn.execute(
        "INSERT INTO classroom_sessions
            (subject_id, session_date, status, title, payload_json, agent_used,
             prompt_version, started_at)
         VALUES ('javascript', '2026-07-21', 'in_progress', 'fixture', ?1,
                 'fixture', 'classroom.javascript.v1', 'now')",
        [payload],
    )
    .unwrap();
    let session_id = conn.last_insert_rowid();

    let chat = classroom::engineering_chat_context(&conn, session_id).unwrap();
    assert_eq!(chat.focus, "javascript");
    assert!(chat.markdown.contains("grounded classroom lesson"));
    assert_eq!(chat.title, "A reader fixture");
    assert!(chat.exercise.contains("Build a boundary probe"));
    assert!(!chat.learner_outcome.is_empty());

    let initial = classroom::classroom_exercise(&conn, session_id)
        .unwrap()
        .unwrap();
    assert_eq!(initial.title, "Build a boundary probe");
    assert!(initial.draft.is_none());

    db::save_exercise_draft(&conn, None, Some(session_id), "draft evidence").unwrap();
    db::save_exercise_completion(
        &conn,
        None,
        Some(session_id),
        true,
        "The boundary test proves the chosen contract remains isolated.",
    )
    .unwrap();
    let saved = classroom::classroom_exercise(&conn, session_id)
        .unwrap()
        .unwrap();
    assert_eq!(saved.draft.as_deref(), Some("draft evidence"));
    assert!(saved.completed);
    assert!(saved.reflection.contains("boundary test"));
}

#[test]
fn classroom_completion_becomes_teacher_memory_for_the_next_course() {
    let conn = test_db();
    let (concept_id, concept_slug): (i64, String) = conn
        .query_row(
            "SELECT id, slug FROM concepts WHERE focus = 'javascript' ORDER BY id LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    let payload = serde_json::json!({
        "concept_id": concept_id,
        "concept_title": "Runtime boundaries",
        "category": "architecture",
        "title": "Classroom runtime evidence",
        "markdown": "lesson",
        "resources": [],
        "questions": [],
        "exercise": null,
        "source": "fixture"
    })
    .to_string();
    conn.execute(
        "INSERT INTO classroom_sessions
            (subject_id, session_date, status, title, payload_json, score,
             agent_used, prompt_version, started_at, completed_at)
         VALUES ('javascript', '2026-07-21', 'completed', 'Classroom runtime evidence',
                 ?1, 0.6, 'fixture', 'classroom.javascript.v1', '2026-07-21T09:00:00',
                 '2026-07-21T10:00:00')",
        [payload],
    )
    .unwrap();
    let session_id = conn.last_insert_rowid();
    db::save_exercise_completion(
        &conn,
        None,
        Some(session_id),
        true,
        "Measured the runtime boundary in DevTools.",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO classroom_exit_attempts
            (session_id, concept_id, question_id, section, learning_objective,
             misconception, correct, attempted_at)
         VALUES (?1, ?2, 1, 'Core mechanics', 'Trace the runtime boundary',
                 'Confused the API surface with the runtime mechanism.', 0,
                 '2026-07-21T10:00:00')",
        params![session_id, concept_id],
    )
    .unwrap();
    mastery::record_course_read(&conn, concept_id, "2026-07-21").unwrap();
    mastery::record_quiz_outcome(&conn, concept_id, "2026-07-21", 0.6).unwrap();

    let dossier = mastery::build_dossier(&conn, "2026-07-22", "javascript").unwrap();
    assert!(dossier.contains("Day 2 of teaching"));
    assert!(dossier.contains("Classroom runtime evidence"));
    assert!(dossier.contains(&concept_slug));
    assert!(dossier.contains("Trace the runtime boundary"));
    assert!(dossier.contains("Confused the API surface"));
    assert!(dossier.contains("1 exercise(s) completed"));
    assert!(dossier.contains("Measured the runtime boundary in DevTools"));
}

#[test]
fn class_agent_and_model_settings_do_not_leak_between_subjects() {
    let conn = test_db();
    configure(&conn, "javascript", "deepseek");
    configure(&conn, "frontend-architecture", "claude");
    let javascript = classroom::program_row(&conn, "javascript").unwrap();
    let architecture = classroom::program_row(&conn, "frontend-architecture").unwrap();
    assert_eq!(javascript.agent, "deepseek");
    assert_eq!(javascript.model, "sonnet");
    assert_eq!(architecture.agent, "claude");
    assert_eq!(architecture.model, "sonnet");
    assert_ne!(javascript.prompt_profile, architecture.prompt_profile);
}

#[test]
fn only_enabled_class_slots_supply_os_wakeup_times() {
    let conn = test_db();
    configure(&conn, "german", "claude");
    configure(&conn, "frontend-architecture", "deepseek");
    classroom::upsert_slot(
        &conn,
        &UpsertClassroomSlotInput {
            id: None,
            subject_id: "german".into(),
            hour: 7,
            minute: 30,
            weekdays: vec![1, 2, 3, 4, 5],
            enabled: true,
            durations: Default::default(),
            starts: Default::default(),
        },
    )
    .unwrap();
    classroom::upsert_slot(
        &conn,
        &UpsertClassroomSlotInput {
            id: None,
            subject_id: "frontend-architecture".into(),
            hour: 12,
            minute: 15,
            weekdays: vec![2, 4],
            enabled: true,
            durations: Default::default(),
            starts: Default::default(),
        },
    )
    .unwrap();
    assert_eq!(
        classroom::all_schedule_times(&conn).unwrap(),
        vec![(7, 30), (12, 15)]
    );
}

#[test]
fn multiple_subject_slots_can_complete_on_the_same_day_without_touching_primary() {
    let conn = test_db();
    configure(&conn, "javascript", "claude");
    configure(&conn, "frontend-architecture", "deepseek");
    let js_slot = classroom::upsert_slot(
        &conn,
        &UpsertClassroomSlotInput {
            id: None,
            subject_id: "javascript".into(),
            hour: 8,
            minute: 0,
            weekdays: vec![2],
            enabled: true,
            durations: Default::default(),
            starts: Default::default(),
        },
    )
    .unwrap();
    let architecture_slot = classroom::upsert_slot(
        &conn,
        &UpsertClassroomSlotInput {
            id: None,
            subject_id: "frontend-architecture".into(),
            hour: 13,
            minute: 0,
            weekdays: vec![2],
            enabled: true,
            durations: Default::default(),
            starts: Default::default(),
        },
    )
    .unwrap();
    let primary = Session {
        date: "2026-07-21".into(),
        concept_id: None,
        status: "pending".into(),
        current_step: "quiz".into(),
        quiz_score: None,
        started_at: None,
        completed_at: None,
        reading_seconds: 0,
        session_type: "lesson".into(),
        plan_reason: String::new(),
        focus: "typescript".into(),
    };
    db::upsert_session(&conn, &primary).unwrap();
    for (slot, subject) in [
        (js_slot, "javascript"),
        (architecture_slot, "frontend-architecture"),
    ] {
        conn.execute(
            "INSERT INTO classroom_sessions
                (subject_id, slot_id, session_date, status, title, payload_json,
                 agent_used, prompt_version, started_at, completed_at)
             VALUES (?1, ?2, '2026-07-21', 'completed', 'fixture', '{}',
                     'fixture', ?3, 'now', 'now')",
            params![subject, slot, format!("classroom.{subject}.v1")],
        )
        .unwrap();
    }
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM classroom_sessions WHERE session_date = '2026-07-21'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 2);
    let still_primary = db::get_session(&conn, "2026-07-21").unwrap().unwrap();
    assert_eq!(still_primary.status, primary.status);
    assert_eq!(still_primary.current_step, primary.current_step);
    assert_eq!(still_primary.focus, primary.focus);
}

#[test]
fn generic_language_slot_routes_to_cefr_engine_without_consuming_primary() {
    let conn = test_db();
    configure(&conn, "german", "claude");
    let slot = classroom::upsert_slot(
        &conn,
        &UpsertClassroomSlotInput {
            id: None,
            subject_id: "german".into(),
            hour: 7,
            minute: 0,
            weekdays: vec![2],
            enabled: true,
            durations: Default::default(),
            starts: Default::default(),
        },
    )
    .unwrap();
    let lesson =
        language::start_classroom_session(&conn, "german", Some(slot), "2026-07-21", false)
            .unwrap();
    let linked: i64 = conn
        .query_row(
            "SELECT classroom_slot_id FROM language_sessions WHERE id = ?1",
            [lesson.session_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(linked, slot);
    assert!(db::get_session(&conn, "2026-07-21").unwrap().is_none());
    let view = classroom::slot_views(&conn, "2026-07-21", true)
        .unwrap()
        .into_iter()
        .find(|candidate| candidate.id == slot)
        .unwrap();
    assert!(view.in_progress);
    assert!(!view.owed);
}

#[test]
fn deleting_a_slot_cuts_the_link_on_history_without_losing_it() {
    let conn = test_db();
    configure(&conn, "german", "claude");
    let slot = classroom::upsert_slot(
        &conn,
        &UpsertClassroomSlotInput {
            id: None,
            subject_id: "german".into(),
            hour: 8,
            minute: 0,
            weekdays: vec![3],
            enabled: true,
            durations: Default::default(),
            starts: Default::default(),
        },
    )
    .unwrap();
    conn.execute(
        "INSERT INTO language_sessions
            (slot_id, classroom_slot_id, language, session_date, level, unit_slug, phase,
             status, lesson_json, response_json, started_at, completed_at)
         VALUES (NULL, ?1, 'german', '2026-07-20', 'A1', 'alphabet', 1,
                 'completed', '{}', '{}', 'now', 'now')",
        [slot],
    )
    .unwrap();
    classroom::delete_slot(&conn, slot).unwrap();
    let linked: Option<i64> = conn
        .query_row(
            "SELECT classroom_slot_id FROM language_sessions
             WHERE language = 'german' AND session_date = '2026-07-20'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(linked, None);
    let kept: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM language_sessions
             WHERE language = 'german' AND session_date = '2026-07-20'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(kept, 1);
}

#[test]
fn language_classes_can_be_paused_independently_on_the_same_day() {
    let conn = test_db();
    configure(&conn, "german", "claude");
    configure(&conn, "italian", "deepseek");
    language::start_classroom_session(&conn, "german", None, "2026-07-21", false).unwrap();
    language::start_classroom_session(&conn, "italian", None, "2026-07-21", false).unwrap();
    let active = language::active_summaries(&conn).unwrap();
    assert_eq!(active.len(), 2);
    assert!(active.iter().any(|session| session.language == "german"));
    assert!(active.iter().any(|session| session.language == "italian"));
}

#[test]
fn editing_a_class_slot_changes_its_fire_time_in_place() {
    let conn = test_db();
    configure(&conn, "german", "claude");
    let slot_id = classroom::upsert_slot(
        &conn,
        &UpsertClassroomSlotInput {
            id: None,
            subject_id: "german".into(),
            hour: 7,
            minute: 30,
            weekdays: vec![1, 2, 3, 4, 5],
            enabled: true,
            durations: Default::default(),
            starts: Default::default(),
        },
    )
    .unwrap();
    // Re-saving with the same id and a new time is how the settings page
    // edits a class's schedule, rather than creating a duplicate slot.
    let updated_id = classroom::upsert_slot(
        &conn,
        &UpsertClassroomSlotInput {
            id: Some(slot_id),
            subject_id: "german".into(),
            hour: 18,
            minute: 45,
            weekdays: vec![6, 7],
            enabled: true,
            durations: Default::default(),
            starts: Default::default(),
        },
    )
    .unwrap();
    assert_eq!(updated_id, slot_id);
    let views = classroom::slot_views(&conn, "2026-07-21", false).unwrap();
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].id, slot_id);
    assert_eq!(views[0].hour, 18);
    assert_eq!(views[0].minute, 45);
    assert_eq!(views[0].weekdays, vec![6, 7]);
}

#[test]
fn editing_a_class_slot_onto_another_slots_time_is_rejected_clearly() {
    let conn = test_db();
    configure(&conn, "german", "claude");
    classroom::upsert_slot(
        &conn,
        &UpsertClassroomSlotInput {
            id: None,
            subject_id: "german".into(),
            hour: 7,
            minute: 0,
            weekdays: vec![1, 2, 3, 4, 5],
            enabled: true,
            durations: Default::default(),
            starts: Default::default(),
        },
    )
    .unwrap();
    let second_id = classroom::upsert_slot(
        &conn,
        &UpsertClassroomSlotInput {
            id: None,
            subject_id: "german".into(),
            hour: 18,
            minute: 0,
            weekdays: vec![6],
            enabled: true,
            durations: Default::default(),
            starts: Default::default(),
        },
    )
    .unwrap();
    let result = classroom::upsert_slot(
        &conn,
        &UpsertClassroomSlotInput {
            id: Some(second_id),
            subject_id: "german".into(),
            hour: 7,
            minute: 0,
            weekdays: vec![6],
            enabled: true,
            durations: Default::default(),
            starts: Default::default(),
        },
    );
    assert_eq!(
        result.unwrap_err(),
        "this class already has a slot at that time"
    );
}

fn window(weekdays: Vec<u8>, start: (u32, u32), end: (u32, u32)) -> AvailabilityWindowInput {
    AvailabilityWindowInput {
        weekdays,
        start_hour: start.0,
        start_minute: start.1,
        end_hour: end.0,
        end_minute: end.1,
    }
}

#[test]
fn planning_a_schedule_previews_without_writing_when_not_committed() {
    let conn = test_db();
    configure(&conn, "german", "claude");
    let plan = classroom::plan_schedule(
        &conn,
        &PlanClassroomScheduleInput {
            subject_id: "german".into(),
            learning_goal: "conversational travel German".into(),
            target_weekly_minutes: 90,

            windows: vec![window(vec![1, 3, 5], (7, 0), (8, 0))],
            commit: false,
        },
        "2026-07-21",
    )
    .unwrap();
    assert_eq!(plan.slots.len(), 1);
    assert_eq!(plan.slots[0].weekdays, vec![1, 3, 5]);
    assert_eq!(plan.total_weekly_minutes, 3 * 30);
    assert!(plan.meets_target);
    assert!(plan.program.is_none());
    assert!(plan.schedule.is_none());
    // Nothing was persisted.
    let slots = classroom::slot_views(&conn, "2026-07-21", false).unwrap();
    assert!(slots.is_empty());
    let program = classroom::program_row(&conn, "german").unwrap();
    assert_eq!(program.learning_goal, "");
}

#[test]
fn committing_a_plan_preserves_manual_slots_and_replaces_only_planned_ones() {
    let conn = test_db();
    configure(&conn, "german", "claude");
    // A slot the learner placed by hand.
    classroom::upsert_slot(
        &conn,
        &UpsertClassroomSlotInput {
            id: None,
            subject_id: "german".into(),
            hour: 19,
            minute: 0,
            weekdays: vec![7],
            enabled: true,
            durations: Default::default(),
            starts: Default::default(),
        },
    )
    .unwrap();

    let first_plan = classroom::plan_schedule(
        &conn,
        &PlanClassroomScheduleInput {
            subject_id: "german".into(),
            learning_goal: "conversational travel German".into(),
            target_weekly_minutes: 90,

            windows: vec![window(vec![1, 3, 5], (7, 0), (8, 0))],
            commit: true,
        },
        "2026-07-21",
    )
    .unwrap();
    assert!(first_plan.program.is_some());
    let mut slots = classroom::slot_views(&conn, "2026-07-21", false).unwrap();
    slots.sort_by_key(|slot| (slot.hour, slot.minute));
    assert_eq!(slots.len(), 2);
    assert_eq!(slots[0].hour, 7);
    assert_eq!(slots[0].source, "planned");
    assert_eq!(slots[1].hour, 19);
    assert_eq!(slots[1].source, "manual");

    // Re-planning with different availability swaps the previous planned
    // slot but never touches the hand-placed one.
    classroom::plan_schedule(
        &conn,
        &PlanClassroomScheduleInput {
            subject_id: "german".into(),
            learning_goal: "conversational travel German".into(),
            target_weekly_minutes: 60,

            windows: vec![window(vec![2, 4], (18, 0), (19, 0))],
            commit: true,
        },
        "2026-07-21",
    )
    .unwrap();
    let mut slots = classroom::slot_views(&conn, "2026-07-21", false).unwrap();
    slots.sort_by_key(|slot| (slot.hour, slot.minute));
    assert_eq!(slots.len(), 2);
    assert_eq!(slots[0].hour, 18);
    assert_eq!(slots[0].source, "planned");
    assert_eq!(slots[0].weekdays, vec![2, 4]);
    assert_eq!(slots[1].hour, 19);
    assert_eq!(slots[1].source, "manual");

    let program = classroom::program_row(&conn, "german").unwrap();
    assert_eq!(program.learning_goal, "conversational travel German");
    assert_eq!(program.target_weekly_minutes, 60);
    // The weekly commitment is what the study times add up to: the two
    // planned half hours and the hand-placed one, not the target typed in.
    let language_view = language::program_view(&conn, "german", "2026-07-21").unwrap();
    assert_eq!(language_view.weekly_minutes, 90);
}

#[test]
fn hand_editing_a_planned_slot_claims_it_so_replanning_leaves_it_alone() {
    let conn = test_db();
    configure(&conn, "javascript", "claude");
    classroom::plan_schedule(
        &conn,
        &PlanClassroomScheduleInput {
            subject_id: "javascript".into(),
            learning_goal: String::new(),
            target_weekly_minutes: 30,

            windows: vec![window(vec![1], (7, 0), (8, 0))],
            commit: true,
        },
        "2026-07-21",
    )
    .unwrap();
    let planned_id = classroom::slot_views(&conn, "2026-07-21", false)
        .unwrap()
        .into_iter()
        .find(|slot| slot.subject_id == "javascript")
        .unwrap()
        .id;
    // The learner nudges the planned slot's time by hand.
    classroom::upsert_slot(
        &conn,
        &UpsertClassroomSlotInput {
            id: Some(planned_id),
            subject_id: "javascript".into(),
            hour: 7,
            minute: 15,
            weekdays: vec![1],
            enabled: true,
            durations: Default::default(),
            starts: Default::default(),
        },
    )
    .unwrap();
    // Re-planning must not delete or overwrite the now-manual slot.
    classroom::plan_schedule(
        &conn,
        &PlanClassroomScheduleInput {
            subject_id: "javascript".into(),
            learning_goal: String::new(),
            target_weekly_minutes: 30,

            windows: vec![window(vec![3], (12, 0), (13, 0))],
            commit: true,
        },
        "2026-07-21",
    )
    .unwrap();
    let mut slots = classroom::slot_views(&conn, "2026-07-21", false).unwrap();
    slots.sort_by_key(|slot| (slot.hour, slot.minute));
    assert_eq!(slots.len(), 2);
    assert_eq!(slots[0].hour, 7);
    assert_eq!(slots[0].minute, 15);
    assert_eq!(slots[0].source, "manual");
    assert_eq!(slots[1].hour, 12);
    assert_eq!(slots[1].source, "planned");
}

#[test]
fn plan_schedule_rejects_empty_windows_and_inverted_times() {
    let conn = test_db();
    configure(&conn, "german", "claude");
    let no_windows = classroom::plan_schedule(
        &conn,
        &PlanClassroomScheduleInput {
            subject_id: "german".into(),
            learning_goal: String::new(),
            target_weekly_minutes: 60,

            windows: vec![],
            commit: false,
        },
        "2026-07-21",
    );
    assert_eq!(
        no_windows.unwrap_err(),
        "add at least one availability window"
    );

    let inverted = classroom::plan_schedule(
        &conn,
        &PlanClassroomScheduleInput {
            subject_id: "german".into(),
            learning_goal: String::new(),
            target_weekly_minutes: 60,

            windows: vec![window(vec![1], (9, 0), (8, 0))],
            commit: false,
        },
        "2026-07-21",
    );
    assert_eq!(
        inverted.unwrap_err(),
        "availability window end time must be after its start time"
    );
}

#[test]
fn streak_counts_a_day_covered_only_by_a_classroom_or_language_session() {
    let conn = test_db();
    configure(&conn, "javascript", "claude");
    configure(&conn, "german", "claude");
    // No primary `sessions` row is ever inserted for either date — the
    // streak must still see these days as "shown up" because the cosmetic
    // uptime badge is meant to reflect any subject, not just the primary one.
    conn.execute(
        "INSERT INTO classroom_sessions
            (subject_id, slot_id, session_date, status, title, payload_json,
             agent_used, prompt_version, started_at, completed_at)
         VALUES ('javascript', NULL, '2026-07-19', 'completed', 'fixture', '{}',
                 'fixture', 'classroom.javascript.v1', 'now', 'now')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO language_sessions
            (slot_id, classroom_slot_id, language, session_date, level, unit_slug, phase,
             status, lesson_json, score, response_json, started_at, completed_at)
         VALUES (NULL, NULL, 'german', '2026-07-20', 'A1', 'fixture-unit', 1,
                 'completed', '{}', 1.0, '{}', 'now', 'now')",
        [],
    )
    .unwrap();
    assert_eq!(db::streak(&conn, "2026-07-20").unwrap(), 2);
    assert_eq!(db::streak(&conn, "2026-07-21").unwrap(), 2);
}

#[test]
fn additive_classroom_migration_preserves_existing_config() {
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "sdr-classroom-migration-{}-{}",
        std::process::id(),
        n
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("test.db");
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE config (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO config VALUES ('legacy-marker', 'preserved');",
        )
        .unwrap();
    }
    let conn = db::open(&path).unwrap();
    language::initialize(&conn, "2026-07-21").unwrap();
    classroom::initialize(&conn).unwrap();
    assert_eq!(
        db::get_config(&conn, "legacy-marker").unwrap().as_deref(),
        Some("preserved")
    );
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM classroom_programs", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(count, 9);
}

#[test]
fn completed_modules_are_never_redrawn_automatically() {
    let conn = test_db();
    configure(&conn, "javascript", "claude");
    conn.execute(
        "INSERT INTO mastery (concept_id, state, score_ema, encounters)
         SELECT id, 'mastered', 1.0, 2 FROM concepts WHERE active = 1 AND focus = 'javascript'",
        [],
    )
    .unwrap();
    assert!(!selection::drawable_exists(&conn, "javascript").unwrap());
    assert!(selection::draw(&conn, "2026-07-22", "javascript")
        .unwrap()
        .is_none());
    let revisited = selection::draw_completed(&conn, "2026-07-22", "javascript")
        .unwrap()
        .expect("explicit revisit serves a completed module");
    assert_eq!(revisited.focus, "javascript");
}

#[test]
fn decayed_modules_return_but_mastered_ones_do_not() {
    let conn = test_db();
    configure(&conn, "javascript", "claude");
    conn.execute(
        "INSERT INTO mastery (concept_id, state, score_ema, encounters)
         SELECT id, 'mastered', 1.0, 2 FROM concepts WHERE active = 1 AND focus = 'javascript'",
        [],
    )
    .unwrap();
    let decayed_id: i64 = conn
        .query_row(
            "SELECT id FROM concepts WHERE focus = 'javascript' AND active = 1 LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    conn.execute(
        "UPDATE mastery SET state = 'decayed' WHERE concept_id = ?1",
        [decayed_id],
    )
    .unwrap();
    assert!(selection::drawable_exists(&conn, "javascript").unwrap());
    for _ in 0..10 {
        let drawn = selection::draw(&conn, "2026-07-22", "javascript")
            .unwrap()
            .expect("the decayed module is drawable");
        assert_eq!(
            drawn.id, decayed_id,
            "only the decayed module may be re-served"
        );
    }
}

#[test]
fn activation_requires_a_saved_enabled_rule_and_keeps_configuration_atomic() {
    let conn = test_db();
    let mut input = ConfigureClassroomInput {
        subject_id: "linux-bash".into(),
        enabled: true,
        agent: "codex".into(),
        model: "default".into(),
        custom_agent_bin: String::new(),
        session_minutes: 45,
        start_level: None,
        target_level: None,
        weekly_minutes: None,
    };
    assert!(classroom::configure_program(&conn, &input, "2026-09-10")
        .unwrap_err()
        .contains("study time"));
    assert_eq!(
        classroom::program_row(&conn, "linux-bash").unwrap().agent,
        "claude"
    );
    assert!(classroom::all_schedule_times(&conn).unwrap().is_empty());
    let id = classroom::upsert_slot(
        &conn,
        &UpsertClassroomSlotInput {
            id: None,
            subject_id: "linux-bash".into(),
            hour: 8,
            minute: 30,
            weekdays: vec![1, 3, 5],
            enabled: false,
            durations: Default::default(),
            starts: Default::default(),
        },
    )
    .unwrap();
    assert!(classroom::configure_program(&conn, &input, "2026-09-10").is_err());
    classroom::upsert_slot(
        &conn,
        &UpsertClassroomSlotInput {
            id: Some(id),
            subject_id: "linux-bash".into(),
            hour: 8,
            minute: 30,
            weekdays: vec![1, 3, 5],
            enabled: true,
            durations: Default::default(),
            starts: Default::default(),
        },
    )
    .unwrap();
    classroom::configure_program(&conn, &input, "2026-09-10").unwrap();
    assert_eq!(classroom::all_schedule_times(&conn).unwrap(), vec![(8, 30)]);
    input.enabled = false;
    classroom::configure_program(&conn, &input, "2026-09-10").unwrap();
    assert!(classroom::all_schedule_times(&conn).unwrap().is_empty());
    assert_eq!(
        classroom::slot_views(&conn, "2026-09-10", false)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        db::get_config(&conn, "schedule_hour").unwrap().as_deref(),
        Some("19")
    );
    input.enabled = true;
    classroom::configure_program(&conn, &input, "2026-09-10").unwrap();
    classroom::delete_slot(&conn, id).unwrap();
    assert!(!classroom::program_row(&conn, "linux-bash").unwrap().enabled);
    classroom::upsert_slot(
        &conn,
        &UpsertClassroomSlotInput {
            id: None,
            subject_id: "linux-bash".into(),
            hour: 8,
            minute: 30,
            weekdays: vec![1, 3, 5],
            enabled: true,
            durations: Default::default(),
            starts: Default::default(),
        },
    )
    .unwrap();
    assert!(!classroom::program_row(&conn, "linux-bash").unwrap().enabled);
    assert!(classroom::all_schedule_times(&conn).unwrap().is_empty());
}

#[test]
fn editing_a_schedule_cannot_transfer_it_to_another_class() {
    let conn = test_db();
    let mut input = UpsertClassroomSlotInput {
        id: None,
        subject_id: "linux-bash".into(),
        hour: 8,
        minute: 30,
        weekdays: vec![1, 3, 5],
        enabled: true,
        durations: Default::default(),
        starts: Default::default(),
    };
    let id = classroom::upsert_slot(&conn, &input).unwrap();
    input.id = Some(id);
    input.subject_id = "german".into();
    input.hour = 10;
    assert!(classroom::upsert_slot(&conn, &input)
        .unwrap_err()
        .contains("not found"));
    let slots = classroom::slot_views(&conn, "2026-09-10", false).unwrap();
    assert_eq!(slots.len(), 1);
    assert_eq!(slots[0].subject_id, "linux-bash");
    assert_eq!(slots[0].hour, 8);
}

#[test]
fn dormant_daily_generation_jobs_remain_archived_while_saved_work_can_resume() {
    let conn = test_db();
    for (date, status) in [("2026-09-09", "in_progress"), ("2026-09-10", "pending")] {
        db::upsert_session(
            &conn,
            &Session {
                date: date.into(),
                status: status.into(),
                current_step: "course".into(),
                concept_id: None,
                quiz_score: None,
                started_at: Some("2026-09-09T09:00:00".into()),
                completed_at: None,
                reading_seconds: 17,
                session_type: "lesson".into(),
                plan_reason: String::new(),
                focus: "system-design".into(),
            },
        )
        .unwrap();
    }
    db::jobs::enqueue(&conn, "course", "2026-09-10").unwrap();
    db::jobs::enqueue(&conn, "quiz", "2026-09-11").unwrap();
    db::jobs::enqueue(&conn, "course", "2026-09-09").unwrap();
    let job = db::jobs::next_for_active_legacy_session(&conn)
        .unwrap()
        .unwrap();
    assert_eq!(job.2, "2026-09-09");
    db::jobs::mark(&conn, job.0, "done", None).unwrap();
    assert!(db::jobs::next_for_active_legacy_session(&conn)
        .unwrap()
        .is_none());
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM generation_jobs WHERE status='queued'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        2
    );
    assert_eq!(
        db::get_session(&conn, "2026-09-09")
            .unwrap()
            .unwrap()
            .reading_seconds,
        17
    );
    assert_eq!(
        db::get_session(&conn, "2026-09-10")
            .unwrap()
            .unwrap()
            .status,
        "pending"
    );
}

// ── Cross-class schedule overlap validation ────────────────────────────────

fn study_time(
    conn: &rusqlite::Connection,
    subject_id: &str,
    hour: u32,
    minute: u32,
    weekdays: Vec<u8>,
) -> Result<i64, String> {
    classroom::upsert_slot(
        conn,
        &UpsertClassroomSlotInput {
            id: None,
            subject_id: subject_id.into(),
            hour,
            minute,
            weekdays,
            enabled: true,
            durations: Default::default(),
            starts: Default::default(),
        },
    )
}

fn set_class(
    conn: &rusqlite::Connection,
    subject_id: &str,
    enabled: bool,
    session_minutes: i64,
) -> Result<(), String> {
    classroom::configure_program(
        conn,
        &ConfigureClassroomInput {
            subject_id: subject_id.into(),
            enabled,
            agent: "claude".into(),
            model: "sonnet".into(),
            custom_agent_bin: String::new(),
            session_minutes,
            start_level: None,
            target_level: None,
            weekly_minutes: None,
        },
        "2026-07-21",
    )
}

fn program_enabled(conn: &rusqlite::Connection, subject_id: &str) -> bool {
    conn.query_row(
        "SELECT enabled FROM classroom_programs WHERE subject_id=?1",
        [subject_id],
        |r| r.get::<_, i64>(0),
    )
    .unwrap()
        == 1
}

#[test]
fn overlapping_study_times_across_active_classes_are_rejected_and_name_the_conflict() {
    let conn = test_db();
    study_time(&conn, "linux-bash", 9, 0, vec![1, 2, 3, 4, 5]).unwrap();
    set_class(&conn, "linux-bash", true, 30).unwrap();
    // 09:15 on Wednesday falls inside the 09:00–09:30 Linux Bash session.
    let error = study_time(&conn, "typescript", 9, 15, vec![3]).unwrap_err();
    assert!(error.contains("TypeScript"), "{error}");
    assert!(error.contains("Wednesday"), "{error}");
    assert!(error.contains("Linux Bash"), "{error}");
    assert!(error.contains("09:00"), "{error}");
    // Adjacent times do not overlap; other weekdays are free.
    study_time(&conn, "typescript", 9, 30, vec![3]).unwrap();
    study_time(&conn, "typescript", 9, 15, vec![6]).unwrap();
    // A class also cannot overlap its own other study time.
    let own = study_time(&conn, "typescript", 9, 45, vec![3]).unwrap_err();
    assert!(own.contains("TypeScript") && own.contains("09:30"), "{own}");
    assert_eq!(
        classroom::slot_views(&conn, "2026-07-21", false)
            .unwrap()
            .iter()
            .filter(|slot| slot.subject_id == "typescript")
            .count(),
        2
    );
}

#[test]
fn schedule_overlap_detection_wraps_around_the_end_of_the_week() {
    let conn = test_db();
    study_time(&conn, "linux-bash", 0, 10, vec![1]).unwrap();
    set_class(&conn, "linux-bash", true, 30).unwrap();
    // Sunday 23:45 + 30 minutes runs into Monday 00:10.
    let error = study_time(&conn, "typescript", 23, 45, vec![7]).unwrap_err();
    assert!(
        error.contains("Sunday") && error.contains("Monday") && error.contains("00:10"),
        "{error}"
    );
    // Saturday 23:45 ends at Sunday 00:15, which is free.
    study_time(&conn, "typescript", 23, 45, vec![6]).unwrap();
}

#[test]
fn paused_classes_do_not_block_others_but_cannot_activate_into_an_overlap() {
    let conn = test_db();
    study_time(&conn, "typescript", 9, 0, vec![1, 3, 5]).unwrap();
    // TypeScript is still paused, so Linux Bash may take the same time.
    study_time(&conn, "linux-bash", 9, 0, vec![1, 3, 5]).unwrap();
    set_class(&conn, "linux-bash", true, 30).unwrap();
    let error = set_class(&conn, "typescript", true, 30).unwrap_err();
    assert!(
        error.contains("Linux Bash") && error.contains("Monday"),
        "{error}"
    );
    assert!(!program_enabled(&conn, "typescript"));
    // Moving the time resolves the overlap and activation proceeds.
    let id = classroom::slot_views(&conn, "2026-07-21", false)
        .unwrap()
        .into_iter()
        .find(|slot| slot.subject_id == "typescript")
        .unwrap()
        .id;
    classroom::upsert_slot(
        &conn,
        &UpsertClassroomSlotInput {
            id: Some(id),
            subject_id: "typescript".into(),
            hour: 10,
            minute: 0,
            weekdays: vec![1, 3, 5],
            enabled: true,
            durations: Default::default(),
            starts: Default::default(),
        },
    )
    .unwrap();
    set_class(&conn, "typescript", true, 30).unwrap();
    assert!(program_enabled(&conn, "typescript"));
    let status: String = conn
        .query_row(
            "SELECT COALESCE((SELECT status FROM classes WHERE course_id='typescript'),'none')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(status, "none", "settings alone never create a class record");
}

#[test]
fn lengthening_a_day_into_another_active_class_is_rejected() {
    let conn = test_db();
    let id = study_time(&conn, "linux-bash", 9, 0, vec![1]).unwrap();
    set_class(&conn, "linux-bash", true, 30).unwrap();
    study_time(&conn, "typescript", 9, 30, vec![1]).unwrap();
    set_class(&conn, "typescript", true, 30).unwrap();
    // Monday's own minutes are written down with the rule, so stretching
    // them runs into TypeScript and is refused.
    let error = classroom::upsert_slot(
        &conn,
        &UpsertClassroomSlotInput {
            id: Some(id),
            subject_id: "linux-bash".into(),
            hour: 9,
            minute: 0,
            weekdays: vec![1],
            enabled: true,
            durations: [(1u8, 45i64)].into_iter().collect(),
            starts: Default::default(),
        },
    )
    .unwrap_err();
    assert!(
        error.contains("TypeScript") && error.contains("09:30"),
        "{error}"
    );
    let view = classroom::slot_views(&conn, "2026-07-21", false)
        .unwrap()
        .into_iter()
        .find(|slot| slot.id == id)
        .unwrap();
    assert_eq!(
        view.durations.get(&1),
        Some(&30),
        "a rejected change writes nothing"
    );
    // The class's own session length follows the schedule and never
    // stretches a day that is already written down.
    set_class(&conn, "linux-bash", true, 45).unwrap();
    let view = classroom::slot_views(&conn, "2026-07-21", false)
        .unwrap()
        .into_iter()
        .find(|slot| slot.id == id)
        .unwrap();
    assert_eq!(view.durations.get(&1), Some(&30));
}

#[test]
fn the_session_length_follows_the_days_the_schedule_is_made_of() {
    let conn = test_db();
    let id = classroom::upsert_slot(
        &conn,
        &UpsertClassroomSlotInput {
            id: None,
            subject_id: "typescript".into(),
            hour: 9,
            minute: 0,
            weekdays: vec![1, 2, 3],
            enabled: true,
            durations: [(1u8, 60i64), (2, 60), (3, 30)].into_iter().collect(),
            starts: Default::default(),
        },
    )
    .unwrap();
    let minutes = |conn: &rusqlite::Connection| -> i64 {
        conn.query_row(
            "SELECT session_minutes FROM classroom_programs WHERE subject_id='typescript'",
            [],
            |r| r.get(0),
        )
        .unwrap()
    };
    assert_eq!(
        minutes(&conn),
        60,
        "two days of an hour outweigh one of half"
    );
    assert_eq!(
        classroom::weekly_minutes_scheduled(&conn, "typescript").unwrap(),
        150
    );
    // A tie goes to the shorter day; no schedule leaves the length alone.
    classroom::upsert_slot(
        &conn,
        &UpsertClassroomSlotInput {
            id: Some(id),
            subject_id: "typescript".into(),
            hour: 9,
            minute: 0,
            weekdays: vec![1, 2],
            enabled: true,
            durations: [(1u8, 60i64), (2, 30)].into_iter().collect(),
            starts: Default::default(),
        },
    )
    .unwrap();
    assert_eq!(minutes(&conn), 30);
    classroom::delete_slot(&conn, id).unwrap();
    assert_eq!(minutes(&conn), 30);
    assert_eq!(
        classroom::weekly_minutes_scheduled(&conn, "typescript").unwrap(),
        0
    );
}

#[test]
fn a_rule_can_start_at_a_different_time_on_each_day() {
    let conn = test_db();
    study_time(&conn, "linux-bash", 8, 0, vec![2]).unwrap();
    set_class(&conn, "linux-bash", true, 30).unwrap();
    // Monday and Wednesday at 07:30, Tuesday at 08:00: Tuesday runs into
    // Linux Bash, and the refusal names Tuesday's own time.
    let tuesday_late = UpsertClassroomSlotInput {
        id: None,
        subject_id: "typescript".into(),
        hour: 7,
        minute: 30,
        weekdays: vec![1, 2, 3],
        enabled: true,
        durations: Default::default(),
        starts: [(2u8, "08:00".to_string())].into_iter().collect(),
    };
    let error = classroom::upsert_slot(&conn, &tuesday_late).unwrap_err();
    assert!(
        error.contains("Tuesday") && error.contains("08:00") && error.contains("Linux Bash"),
        "{error}"
    );
    let bad_clock = UpsertClassroomSlotInput {
        starts: [(2u8, "25:00".to_string())].into_iter().collect(),
        ..tuesday_late.clone()
    };
    assert!(classroom::upsert_slot(&conn, &bad_clock)
        .unwrap_err()
        .contains("HH:MM"));
    // Tuesday at 06:45 is free. A day at the rule's own time is not kept
    // as its own entry.
    let id = classroom::upsert_slot(
        &conn,
        &UpsertClassroomSlotInput {
            starts: [(2u8, "06:45".to_string()), (3, "07:30".to_string())]
                .into_iter()
                .collect(),
            ..tuesday_late
        },
    )
    .unwrap();
    let view = classroom::slot_views(&conn, "2026-07-21", false)
        .unwrap()
        .into_iter()
        .find(|slot| slot.id == id)
        .unwrap();
    assert_eq!(view.starts.get(&2).map(String::as_str), Some("06:45"));
    assert_eq!(view.starts.get(&3), None);
    assert_eq!((view.hour, view.minute), (7, 30));
    // 2026-07-21 is a Tuesday: the next firing is Tuesday's own time.
    assert!(
        view.next_fire_at.ends_with("06:45:00") || view.next_fire_at.ends_with("07:30:00"),
        "{}",
        view.next_fire_at
    );
    set_class(&conn, "typescript", true, 30).unwrap();
    let mut times = classroom::all_schedule_times(&conn).unwrap();
    times.sort_unstable();
    assert_eq!(
        times,
        vec![(6, 45), (7, 30), (8, 0)],
        "the app wakes for every day's own time"
    );
}

#[test]
fn planner_previews_conflicts_without_writing_and_refuses_to_commit_them() {
    let conn = test_db();
    study_time(&conn, "linux-bash", 9, 0, vec![1, 3, 5]).unwrap();
    set_class(&conn, "linux-bash", true, 30).unwrap();
    let plan = |commit: bool, start_hour: u32| {
        classroom::plan_schedule(
            &conn,
            &PlanClassroomScheduleInput {
                subject_id: "typescript".into(),
                learning_goal: String::new(),
                target_weekly_minutes: 60,
                commit,
                windows: vec![window(vec![1, 2], (start_hour, 15), (start_hour + 1, 0))],
            },
            "2026-07-21",
        )
    };
    let preview = plan(false, 9).unwrap();
    assert_eq!(preview.conflicts.len(), 1);
    assert_eq!(
        (
            preview.conflicts[0].weekday,
            preview.conflicts[0].with_label.as_str(),
            preview.conflicts[0].with_hour
        ),
        (1, "Linux Bash", 9)
    );
    let before = classroom::slot_views(&conn, "2026-07-21", false)
        .unwrap()
        .len();
    let error = plan(true, 9).unwrap_err();
    assert!(error.contains("Linux Bash"), "{error}");
    assert_eq!(
        classroom::slot_views(&conn, "2026-07-21", false)
            .unwrap()
            .len(),
        before
    );
    let clear = plan(false, 10).unwrap();
    assert!(clear.conflicts.is_empty());
    let committed = plan(true, 10).unwrap();
    assert!(committed.conflicts.is_empty());
    assert_eq!(committed.schedule.unwrap().len(), 1);
    // Replanning ignores this class's own planned times but still respects
    // its manual ones and other active classes.
    let replanned = plan(false, 10).unwrap();
    assert!(replanned.conflicts.is_empty());
}

#[test]
fn accepting_a_path_saves_it_without_activating_an_overlapping_class() {
    use principia_desk_lib::domain::{
        classes::{self, AcceptPath},
        enrollment::{self, SaveEnrollmentDraft},
        placement,
    };
    let conn = test_db();
    // The paused TypeScript time was saved first; Linux Bash then took the
    // same time and activated, which a paused class cannot block.
    study_time(&conn, "typescript", 9, 0, vec![2]).unwrap();
    study_time(&conn, "linux-bash", 9, 0, vec![1, 2, 3, 4, 5]).unwrap();
    set_class(&conn, "linux-bash", true, 30).unwrap();
    let options = enrollment::options("typescript").unwrap();
    let draft = enrollment::save_draft(
        &conn,
        &SaveEnrollmentDraft {
            id: None,
            expected_revision: None,
            course: options.course,
            configuration: options.default_configuration,
        },
    )
    .unwrap();
    let recommendation = placement::recommend(&conn, &draft.id, draft.revision).unwrap();
    let accepted = classes::accept(
        &conn,
        &AcceptPath {
            draft_id: draft.id,
            expected_revision: draft.revision,
            recommendation_id: recommendation.id,
        },
        "2026-07-21",
    )
    .unwrap();
    assert_eq!(accepted.revision, 1);
    let status: String = conn
        .query_row(
            "SELECT status FROM classes WHERE course_id='typescript'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(status, "paused");
    assert!(!program_enabled(&conn, "typescript"));
    let error = set_class(&conn, "typescript", true, 30).unwrap_err();
    assert!(
        error.contains("Linux Bash") && error.contains("Tuesday"),
        "{error}"
    );
}
