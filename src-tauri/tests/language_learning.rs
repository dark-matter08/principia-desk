use principia_desk_lib::classroom;
use principia_desk_lib::db::{self, Session};
use principia_desk_lib::language::{self, ConfigureProgramInput, SubmitSessionInput};

static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn test_db() -> rusqlite::Connection {
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("sdr-language-test-{}-{}", std::process::id(), n));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let conn = db::open(&dir.join("test.db")).unwrap();
    language::initialize(&conn, "2026-07-21").unwrap();
    db::set_config(&conn, "schedule_hour", "19").unwrap();
    db::set_config(&conn, "schedule_minute", "0").unwrap();
    conn
}

fn correct_answers(conn: &rusqlite::Connection, session_id: i64) -> Vec<usize> {
    let lesson_json: String = conn
        .query_row(
            "SELECT lesson_json FROM language_sessions WHERE id = ?1",
            [session_id],
            |row| row.get(0),
        )
        .unwrap();
    let payload: serde_json::Value = serde_json::from_str(&lesson_json).unwrap();
    payload["questions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|question| question["correct_index"].as_u64().unwrap() as usize)
        .collect()
}

fn enable_german(conn: &rusqlite::Connection) {
    language::configure_program(
        conn,
        &ConfigureProgramInput {
            language: "german".into(),
            enabled: true,
            start_level: "A1".into(),
            target_level: "A2".into(),
            weekly_minutes: 210,
            session_minutes: 30,
        },
        "2026-07-21",
    )
    .unwrap();
}

fn pass_session(conn: &rusqlite::Connection, session_id: i64, today: &str) {
    let result = language::submit_session(
        conn,
        &SubmitSessionInput {
            session_id,
            answers: correct_answers(conn, session_id),
            writing_response:
                "I completed the requested writing practice with enough detail to show my work."
                    .into(),
            speaking_completed: true,
            listened: true,
            confidence: 4,
        },
        today,
    )
    .unwrap();
    assert!(result.passed);
}

#[test]
fn bundled_language_curricula_meet_the_quality_contract() {
    language::validate_curriculum("german").unwrap();
    language::validate_curriculum("italian").unwrap();
    assert!(language::validate_curriculum("french").is_err());
    assert!(language::pedagogical_summary("german")
        .unwrap()
        .contains("40 action-oriented CEFR units"));
    assert!(language::pedagogical_summary("italian")
        .unwrap()
        .contains("starts from first principles"));
}

#[test]
fn a1_completes_alphabet_and_counting_foundations_before_later_scenarios() {
    let conn = test_db();
    enable_german(&conn);

    for day in 21..=23 {
        let today = format!("2026-07-{day}");
        let lesson = language::start_session(&conn, "german", &today, false).unwrap();
        assert_eq!(lesson.unit_slug, "de-a1-greetings-introductions");
        assert!(lesson.markdown.contains("## First principles foundation"));
        assert!(lesson
            .markdown
            .contains("The German alphabet and letter names"));
        assert!(lesson.markdown.contains("A ah, B beh, C tseh"));
        // First-principles contract: outcome, building blocks, one analogy
        // with its breaking point, and a reconstruction check.
        assert!(lesson.markdown.contains("**Outcome in plain terms:**"));
        assert!(lesson.markdown.contains("**Smallest reliable pieces:**"));
        assert!(lesson
            .markdown
            .contains("### One analogy, and where it breaks"));
        assert!(lesson.markdown.contains("**Where it breaks:**"));
        assert!(lesson.markdown.contains("**Derive it:**"));
        pass_session(&conn, lesson.session_id.parse::<i64>().unwrap(), &today);
    }

    let numbers = language::start_session(&conn, "german", "2026-07-24", false).unwrap();
    assert_eq!(numbers.unit_slug, "de-a1-numbers-personal-info");
    assert!(numbers
        .markdown
        .contains("Count from zero before using numbers"));
    assert!(numbers
        .markdown
        .contains("null, eins, zwei, drei, vier, fünf"));
}

#[test]
fn italian_a1_starts_with_alphabet_and_sound_spelling() {
    let conn = test_db();
    language::configure_program(
        &conn,
        &ConfigureProgramInput {
            language: "italian".into(),
            enabled: true,
            start_level: "A1".into(),
            target_level: "A2".into(),
            weekly_minutes: 210,
            session_minutes: 30,
        },
        "2026-07-21",
    )
    .unwrap();

    let lesson = language::start_session(&conn, "italian", "2026-07-21", false).unwrap();
    assert_eq!(lesson.unit_slug, "it-a1-greetings");
    assert!(lesson
        .markdown
        .contains("The Italian alphabet and letter names"));
    assert!(lesson.markdown.contains("Core Italian uses 21 letters"));
    assert!(lesson.markdown.contains("Vowels are the sound anchors"));
}

#[test]
fn opening_an_existing_database_adds_language_tables_without_touching_old_data() {
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "sdr-language-migration-{}-{}",
        std::process::id(),
        n
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("test.db");
    {
        let old = rusqlite::Connection::open(&path).unwrap();
        old.execute_batch(
            "CREATE TABLE config (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO config (key, value) VALUES ('legacy-marker', 'preserved');",
        )
        .unwrap();
    }
    let conn = db::open(&path).unwrap();
    language::initialize(&conn, "2026-07-21").unwrap();
    let marker = db::get_config(&conn, "legacy-marker").unwrap();
    assert_eq!(marker.as_deref(), Some("preserved"));
    let language_tables: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type = 'table' AND name LIKE 'language_%'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(language_tables, 5);
}

#[test]
fn legacy_language_slots_migrate_into_the_classroom_schedule() {
    let conn = test_db();
    enable_german(&conn);
    // Simulate a pre-classroom install that owns legacy language slots.
    for (hour, minute) in [(7, 30), (12, 15)] {
        conn.execute(
            "INSERT INTO language_schedule_slots
                (language, hour, minute, weekdays_json, enabled, created_at)
             VALUES ('german', ?1, ?2, '[1,2,3,4,5,6]', 1, 'now')",
            rusqlite::params![hour, minute],
        )
        .unwrap();
    }
    classroom::initialize(&conn).unwrap();
    assert_eq!(
        classroom::all_schedule_times(&conn).unwrap(),
        vec![(7, 30), (12, 15)]
    );
}

#[test]
fn frontend_completion_does_not_consume_or_mutate_language_practice() {
    let conn = test_db();
    enable_german(&conn);
    classroom::initialize(&conn).unwrap();
    let slot_id = classroom::upsert_slot(
        &conn,
        &classroom::UpsertClassroomSlotInput {
            id: None,
            subject_id: "german".into(),
            hour: 7,
            minute: 30,
            weekdays: vec![1, 2, 3, 4, 5, 6, 7],
            enabled: true,
            durations: Default::default(),
            starts: Default::default(),
        },
    )
    .unwrap();
    let frontend = Session {
        date: "2026-07-21".into(),
        concept_id: None,
        status: "completed".into(),
        current_step: "done".into(),
        quiz_score: Some(1.0),
        started_at: Some("2026-07-21T19:00:00".into()),
        completed_at: Some("2026-07-21T19:30:00".into()),
        reading_seconds: 0,
        session_type: "lesson".into(),
        plan_reason: String::new(),
        focus: "javascript".into(),
    };
    db::upsert_session(&conn, &frontend).unwrap();

    let slots = classroom::slot_views(&conn, "2026-07-21", true).unwrap();
    assert!(slots.iter().any(|slot| slot.id == slot_id && slot.owed));

    let lesson =
        language::start_classroom_session(&conn, "german", Some(slot_id), "2026-07-21", false)
            .unwrap();
    assert_eq!(lesson.language, "german");
    assert_eq!(lesson.level, "A1");
    assert_eq!(lesson.unit_slug, "de-a1-greetings-introductions");
    assert_eq!(lesson.questions.len(), 5);
    assert!(lesson.markdown.contains("## Model dialogue"));

    let unchanged = db::get_session(&conn, "2026-07-21").unwrap().unwrap();
    assert_eq!(unchanged.status, "completed");
    assert_eq!(unchanged.focus, "javascript");
}

#[test]
fn passed_language_session_records_skill_and_unit_evidence() {
    let conn = test_db();
    enable_german(&conn);
    let lesson = language::start_session(&conn, "german", "2026-07-21", false).unwrap();
    let answers = correct_answers(&conn, lesson.session_id.parse::<i64>().unwrap());
    let result = language::submit_session(
        &conn,
        &SubmitSessionInput {
            session_id: lesson.session_id.parse::<i64>().unwrap(),
            answers,
            writing_response:
                "Guten Tag. Ich heiße Armand und ich komme aus England. Freut mich sehr.".into(),
            speaking_completed: true,
            listened: true,
            confidence: 4,
        },
        "2026-07-21",
    )
    .unwrap();
    assert!(result.passed);
    assert!(result.score >= 0.8);
    assert_eq!(result.progress.completed_steps, 1);
    assert!(result
        .progress
        .skills
        .iter()
        .any(|skill| skill.id == "listening" && skill.encounters > 0));
    assert!(language::active_session_for(&conn, "german")
        .unwrap()
        .is_none());
}

#[test]
fn cefr_level_advances_only_after_unit_and_skill_gate_evidence() {
    let conn = test_db();
    enable_german(&conn);
    let seed: serde_json::Value =
        serde_json::from_str(include_str!("../seed/languages/german.json")).unwrap();
    for unit in seed["levels"][0]["units"].as_array().unwrap() {
        let slug = unit["slug"].as_str().unwrap();
        conn.execute(
            "INSERT INTO language_unit_progress
                (language, unit_slug, phase_completed, score_ema, encounters)
             VALUES ('german', ?1, 3, 0.9, 3)",
            [slug],
        )
        .unwrap();
    }
    for strand in language::STRANDS {
        conn.execute(
            "INSERT INTO language_skill_scores
                (language, strand, score_ema, encounters)
             VALUES ('german', ?1, 0.8, 4)",
            [strand],
        )
        .unwrap();
    }
    let lesson = language::start_session(&conn, "german", "2026-07-21", false).unwrap();
    let result = language::submit_session(
        &conn,
        &SubmitSessionInput {
            session_id: lesson.session_id.parse::<i64>().unwrap(),
            answers: correct_answers(&conn, lesson.session_id.parse::<i64>().unwrap()),
            writing_response:
                "Guten Tag. Ich heiße Armand und ich komme aus England. Freut mich sehr.".into(),
            speaking_completed: true,
            listened: true,
            confidence: 4,
        },
        "2026-07-21",
    )
    .unwrap();
    assert_eq!(result.level_advanced_to.as_deref(), Some("A2"));
    assert_eq!(result.current_level, "A2");
}

#[test]
fn due_calculation_honors_weekdays_time_and_completion() {
    let monday = chrono::NaiveDate::from_ymd_opt(2026, 7, 20)
        .unwrap()
        .and_hms_opt(8, 0, 0)
        .unwrap();
    assert!(classroom::slot_due_at(
        7,
        30,
        &[1, 2, 3, 4, 5],
        monday,
        false
    ));
    assert!(!classroom::slot_due_at(
        9,
        0,
        &[1, 2, 3, 4, 5],
        monday,
        false
    ));
    assert!(!classroom::slot_due_at(
        7,
        30,
        &[1, 2, 3, 4, 5],
        monday,
        true
    ));
}

fn complete_level(conn: &rusqlite::Connection, language: &str, level: &str) {
    let seed: serde_json::Value =
        serde_json::from_str(include_str!("../seed/languages/german.json")).unwrap();
    let spec = seed["levels"]
        .as_array()
        .unwrap()
        .iter()
        .find(|candidate| candidate["level"] == level)
        .unwrap();
    let sessions_per_unit = spec["sessions_per_unit"].as_i64().unwrap();
    for unit in spec["units"].as_array().unwrap() {
        conn.execute(
            "INSERT INTO language_unit_progress
                (language, unit_slug, phase_completed, score_ema, encounters)
             VALUES (?1, ?2, ?3, 0.4, 1)
             ON CONFLICT(language, unit_slug) DO UPDATE SET phase_completed = ?3",
            rusqlite::params![language, unit["slug"].as_str().unwrap(), sessions_per_unit],
        )
        .unwrap();
    }
}

#[test]
fn completing_each_target_stops_there_and_keeps_settings_editable() {
    for target in ["A1", "A2", "B1", "B2"] {
        let conn = test_db();
        let mut settings = ConfigureProgramInput {
            language: "german".into(),
            enabled: true,
            start_level: target.into(),
            target_level: target.into(),
            weekly_minutes: 210,
            session_minutes: 30,
        };
        language::configure_program(&conn, &settings, "2026-07-21").unwrap();
        complete_level(&conn, "german", target);
        for strand in language::STRANDS {
            conn.execute("INSERT INTO language_skill_scores (language, strand, score_ema, encounters) VALUES ('german', ?1, 0.9, 5)", [strand]).unwrap();
        }
        let lesson = language::start_session(&conn, "german", "2026-07-21", true).unwrap();
        pass_session(
            &conn,
            lesson.session_id.parse::<i64>().unwrap(),
            "2026-07-21",
        );
        let progress = language::program_view(&conn, "german", "2026-07-21").unwrap();
        assert_eq!(progress.current_level, target);
        settings.enabled = false;
        language::configure_program(&conn, &settings, "2026-07-21").unwrap();
        if target == "A1" {
            settings.enabled = true;
            settings.target_level = "A2".into();
            language::configure_program(&conn, &settings, "2026-07-21").unwrap();
            let next = language::start_session(&conn, "german", "2026-07-22", false).unwrap();
            assert_eq!(next.level, "A2");
        }
    }
}

#[test]
fn invalid_last_answer_does_not_leave_partial_language_evidence() {
    let conn = test_db();
    enable_german(&conn);
    let lesson = language::start_session(&conn, "german", "2026-07-21", false).unwrap();
    let mut answers = correct_answers(&conn, lesson.session_id.parse::<i64>().unwrap());
    *answers.last_mut().unwrap() = usize::MAX;
    let result = language::submit_session(
        &conn,
        &SubmitSessionInput {
            session_id: lesson.session_id.parse::<i64>().unwrap(),
            answers,
            writing_response: "draft".into(),
            speaking_completed: false,
            listened: false,
            confidence: 3,
        },
        "2026-07-21",
    );
    assert!(result.is_err());
    let evidence: i64 = conn
        .query_row("SELECT COUNT(*) FROM language_skill_scores", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(evidence, 0);
    pass_session(
        &conn,
        lesson.session_id.parse::<i64>().unwrap(),
        "2026-07-21",
    );
}

#[test]
fn changing_legacy_start_after_learning_cannot_rewrite_the_progress_baseline() {
    let conn = test_db();
    enable_german(&conn);
    let lesson = language::start_session(&conn, "german", "2026-07-21", false).unwrap();
    pass_session(
        &conn,
        lesson.session_id.parse::<i64>().unwrap(),
        "2026-07-21",
    );
    let result = language::configure_program(
        &conn,
        &ConfigureProgramInput {
            language: "german".into(),
            enabled: true,
            start_level: "A2".into(),
            target_level: "A2".into(),
            weekly_minutes: 210,
            session_minutes: 30,
        },
        "2026-07-21",
    );
    assert!(result.is_err());
    assert_eq!(
        language::program_view(&conn, "german", "2026-07-21")
            .unwrap()
            .start_level,
        "A1"
    );
}

fn wrong_answers(conn: &rusqlite::Connection, session_id: i64) -> Vec<usize> {
    let lesson_json: String = conn
        .query_row(
            "SELECT lesson_json FROM language_sessions WHERE id = ?1",
            [session_id],
            |row| row.get(0),
        )
        .unwrap();
    let payload: serde_json::Value = serde_json::from_str(&lesson_json).unwrap();
    payload["questions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|question| {
            let correct = question["correct_index"].as_u64().unwrap() as usize;
            let count = question["choices"].as_array().unwrap().len();
            (correct + 1) % count
        })
        .collect()
}

#[test]
fn completed_level_units_are_never_reserved_automatically() {
    let conn = test_db();
    enable_german(&conn);
    complete_level(&conn, "german", "A1");
    // Every unit complete + skills below the gate => targeted remediation on
    // the weakest unit, never a fresh rotation through completed units.
    let remediation = language::start_session(&conn, "german", "2026-07-22", false).unwrap();
    assert_eq!(remediation.level, "A1");
    assert!(
        remediation.phase > 3,
        "remediation runs past the completed phases"
    );
    // Failing keeps the skill gate unmet, so the next start is remediation
    // again rather than a phase-1 re-serve of a completed unit.
    language::submit_session(
        &conn,
        &SubmitSessionInput {
            session_id: remediation.session_id.parse::<i64>().unwrap(),
            answers: wrong_answers(&conn, remediation.session_id.parse::<i64>().unwrap()),
            writing_response: "short".into(),
            speaking_completed: false,
            listened: false,
            confidence: 1,
        },
        "2026-07-22",
    )
    .unwrap();
    let again = language::start_session(&conn, "german", "2026-07-23", false).unwrap();
    assert!(
        again.phase > 3,
        "completed units are never started from phase 1 again"
    );
}

#[test]
fn a_finished_top_level_rejects_new_sessions_but_allows_revisit() {
    let conn = test_db();
    enable_german(&conn);
    conn.execute(
        "UPDATE language_programs SET current_level = 'B2' WHERE language = 'german'",
        [],
    )
    .unwrap();
    complete_level(&conn, "german", "B2");
    let error = language::start_session(&conn, "german", "2026-07-22", false).unwrap_err();
    assert!(error.contains("complete"), "{error}");
    let revisit = language::start_session(&conn, "german", "2026-07-22", true).unwrap();
    assert_eq!(revisit.level, "B2");
    assert!(revisit.phase > 27);
}
