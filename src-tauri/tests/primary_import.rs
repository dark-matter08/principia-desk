use principia_desk_lib::{
    db,
    domain::assessments::{self, Item, Owner, Purpose, Response, ResponseStatus},
    storage::primary_import::{self, Cell, ContentSelection, RecoveryReason, SubjectResolution},
};
use rusqlite::{params, Connection, OpenFlags};
use serde_json::json;
use std::path::PathBuf;

fn fixture() -> (PathBuf, Connection) {
    let directory = std::env::temp_dir().join(format!(
        "principia-primary-import-{:032x}",
        rand::random::<u128>()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("learner.db");
    let conn = db::open(&path).unwrap();
    conn.execute_batch("INSERT INTO concepts(id,slug,title,category,focus,brief_json) VALUES
        (1,'old-js','Original JS topic','fundamentals','javascript','{ \"old\": true }'),
        (2,'old-design','Pre-drawn topic','fundamentals','system-design','{}');
        INSERT INTO sessions(date,concept_id,status,current_step,reading_seconds,focus) VALUES
        ('2026-09-01',1,'in_progress','course',17,'javascript'),
        ('2026-09-02',1,'in_progress','review',0,'javascript'),
        ('2026-09-03',2,'pending','quiz',0,''),
        ('2026-09-04',1,'in_progress','course',9,'typescript');
        INSERT INTO sessions(date,concept_id,status,current_step,reading_seconds,focus,completed_at) VALUES
        ('2026-08-31',1,'completed','done',1800,'javascript','2026-08-31T10:00:00');
        INSERT INTO courses(id,session_date,concept_id,markdown,resources_json,source,generated_at) VALUES
        (11,'2026-09-01',1,'# Original\n\nExact lesson body.','[  ]','fallback','original'),
        (12,'2026-09-02',1,'# Candidate one','[]','claude','first'),
        (13,'2026-09-02',1,'# Candidate two','[]','codex','second'),
        (14,'2026-09-03',2,'# A pre-drawn document is not a subject choice','[]','fallback','old'),
        (15,'2026-09-04',1,'# Subject mismatch','[]','fallback','old'),
        (16,'2026-08-31',1,'# Archived completed lesson','[]','claude','original'),
        (17,'2026-08-30',1,'# Orphan archive document','[]','fallback','original');
        INSERT INTO course_exercises VALUES(11,'Saved practical work','Original instructions',NULL,NULL,'[ \"first\", \"second\" ]');
        INSERT INTO exercise_drafts(course_id,draft,completed,reflection,updated_at) VALUES(11,'const unfinished = ',0,'  Original reflection  ','original');
        INSERT INTO config VALUES('provider_api_key','PRIVATE_TOKEN_MUST_NOT_LEAK');
        INSERT INTO config VALUES('planned:2026-09-01','1');
        INSERT INTO config VALUES('exit_quiz_round:11','3');
        INSERT INTO config VALUES('exit_quiz_count:11','9');
        INSERT INTO profile VALUES('preferred_focus','typescript');
        INSERT INTO legacy_assessment_config VALUES('quiz_round:2026-09-03','{malformed original');
        INSERT INTO legacy_assessment_config VALUES('pending_answers:2026-09-03','{ \"41\": \"original draft\" }');
        INSERT INTO generation_jobs(kind,target_date,status,attempts,created_at) VALUES('course','2026-09-03','running',2,'original');").unwrap();
    let question = db::insert_question(
        &conn,
        11,
        "Original frozen question",
        "free",
        None,
        "original rubric",
        "original explanation",
    )
    .unwrap();
    let stored_question = db::questions_for_course(&conn, 11).unwrap().remove(0);
    let owner = Owner::LegacyPrimary("2026-09-02".into());
    let round = assessments::start(
        &conn,
        &owner,
        Purpose::Retrieval,
        &json!({"stored_context":true}),
        "original-rubric",
        &[Item {
            id: question.to_string(),
            body: serde_json::to_value(stored_question).unwrap(),
        }],
    )
    .unwrap();
    assessments::save_response(
        &conn,
        &owner,
        &round.id,
        0,
        &question.to_string(),
        Response {
            answer: "The learner's unfinished answer…".into(),
            status: ResponseStatus::Draft,
        },
    )
    .unwrap();
    (path, conn)
}

#[test]
fn import_preserves_every_legacy_owner_without_inventing_assignment_or_credit() {
    let (_path, conn) = fixture();
    let before_changes = conn.total_changes();
    let plan = primary_import::inspect(&conn).unwrap();
    assert_eq!(
        conn.total_changes(),
        before_changes,
        "inspection must not write"
    );
    assert_eq!(plan.sessions.len(), 5);
    assert_eq!(
        plan.sessions
            .iter()
            .filter(|s| s.original_status == "in_progress")
            .count(),
        3
    );
    let reading = plan
        .sessions
        .iter()
        .find(|s| s.service_date == "2026-09-01")
        .unwrap();
    assert_eq!(
        reading.session_id,
        db::primary_session_id(&conn, &reading.service_date)
            .unwrap()
            .unwrap()
    );
    assert_eq!(reading.content, ContentSelection::Stored { course_id: 11 });
    assert!(reading.recovery.is_empty());
    let pending = plan
        .sessions
        .iter()
        .find(|s| s.service_date == "2026-09-03")
        .unwrap();
    assert_eq!(
        pending.subject,
        SubjectResolution::NotSelected,
        "neither a pre-drawn concept nor preferred focus chooses the session subject"
    );
    assert_eq!(pending.original_status, "pending");
    assert_eq!(
        pending.content,
        ContentSelection::Ambiguous {
            course_ids: vec![14]
        }
    );
    assert!(pending
        .recovery
        .contains(&RecoveryReason::SubjectNotRecorded));
    assert!(pending
        .recovery
        .contains(&RecoveryReason::AssessmentFormatUnknown));
    let conflicting = plan
        .sessions
        .iter()
        .find(|s| s.service_date == "2026-09-04")
        .unwrap();
    assert_eq!(
        conflicting.subject,
        SubjectResolution::Recorded {
            course_id: "typescript".into()
        }
    );
    assert!(conflicting
        .recovery
        .contains(&RecoveryReason::SubjectConceptMismatch));
    assert_eq!(plan.unattached_course_ids, vec![17]);
    for table in ["study_sessions", "classes", "path_revisions", "mastery"] {
        let count: u32 = conn
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0, "inspection created records in {table}");
    }
}

#[test]
fn duplicate_documents_and_unknown_payloads_are_retained_byte_for_byte() {
    let (path, conn) = fixture();
    // SQLite TEXT can contain non-UTF8 bytes even though Rust Strings cannot.
    conn.execute(
        "UPDATE courses SET resources_json=CAST(?1 AS TEXT) WHERE id=12",
        [vec![0xffu8, 0, 0xc0]],
    )
    .unwrap();
    let original_draft: String = conn
        .query_row(
            "SELECT draft FROM exercise_drafts WHERE course_id=11",
            [],
            |r| r.get(0),
        )
        .unwrap();
    drop(conn);
    let reader = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let plan = primary_import::inspect(&reader).unwrap();
    let round_owner = plan
        .sessions
        .iter()
        .find(|s| s.service_date == "2026-09-02")
        .unwrap();
    assert_eq!(
        round_owner.content,
        ContentSelection::Ambiguous {
            course_ids: vec![12, 13]
        }
    );
    assert_eq!(round_owner.assessment_attempt_ids.len(), 1);
    let invalid = plan.tables["courses"]
        .iter()
        .find(|r| r["id"] == Cell::Integer(12))
        .unwrap();
    assert_eq!(invalid["resources_json"], Cell::Text(vec![0xff, 0, 0xc0]));
    assert_eq!(
        plan.tables["exercise_drafts"][0]["draft"],
        Cell::Text(original_draft.into_bytes())
    );
    assert_eq!(
        plan.tables["legacy_assessment_config"][1]["value"],
        Cell::Text(b"{malformed original".to_vec())
    );
    assert_eq!(plan.tables["assessment_work"].len(), 1);
    assert_eq!(plan.tables["primary_config"].len(), 3);
    let serialized = serde_json::to_string(&plan).unwrap();
    assert!(!serialized.contains("PRIVATE_TOKEN_MUST_NOT_LEAK"));
    assert!(!plan.tables.contains_key("profile"));
    assert_eq!(
        serde_json::from_str::<primary_import::PrimaryImport>(&serialized).unwrap(),
        plan
    );
    primary_import::verify_unchanged(&reader, &plan).unwrap();
}

#[test]
fn a_primary_write_invalidates_the_reviewed_import_but_other_class_work_does_not() {
    let (_path, conn) = fixture();
    let original = primary_import::inspect(&conn).unwrap();
    // Configuration changes must not rewrite historical tutor provenance.
    db::set_config(&conn, "agent", "ollama").unwrap();
    db::set_config(&conn, "model", "a-new-default").unwrap();
    conn.execute_batch("INSERT INTO classroom_programs(subject_id,kind,label,short_code,enabled,agent,model,prompt_profile,updated_at) VALUES('javascript','engineering','A separate class','JS',1,'claude','sonnet','javascript','original');
        INSERT INTO classroom_sessions(id,subject_id,session_date,status,title,payload_json,agent_used,prompt_version,started_at) VALUES(101,'javascript','2026-09-01','in_progress','Another class','{}','fallback','legacy','original');
        INSERT INTO exercise_drafts(classroom_session_id,draft,completed,reflection,updated_at) VALUES(101,'Another class draft',0,'','original');").unwrap();
    db::save_exercise_draft(&conn, None, Some(101), "A later class draft").unwrap();
    primary_import::verify_unchanged(&conn, &original).unwrap();
    let id = db::primary_session_id(&conn, "2026-09-01")
        .unwrap()
        .unwrap();
    db::save_primary_reading(&conn, &id, 18).unwrap();
    assert!(primary_import::verify_unchanged(&conn, &original).is_err());
    let tick = primary_import::inspect(&conn).unwrap();
    assert_ne!(tick.fingerprint, original.fingerprint);
    db::save_exercise_draft(&conn, Some(11), None, "A later durable draft").unwrap();
    assert!(primary_import::verify_unchanged(&conn, &tick).is_err());
    let draft = primary_import::inspect(&conn).unwrap();
    db::insert_course(&conn, "2026-09-01", 1, "# Late worker", "[]", "claude").unwrap();
    assert!(primary_import::verify_unchanged(&conn, &draft).is_err());
}

#[test]
fn inspection_uses_one_sqlite_snapshot_and_detects_drift_after_it_closes() {
    let (path, writer) = fixture();
    let reader = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let snapshot = reader.unchecked_transaction().unwrap();
    let original = primary_import::inspect(&snapshot).unwrap();
    writer
        .execute(
            "UPDATE courses SET markdown='new worker publication' WHERE id=11",
            [],
        )
        .unwrap();
    primary_import::verify_unchanged(&snapshot, &original).unwrap();
    snapshot.commit().unwrap();
    assert!(primary_import::verify_unchanged(&reader, &original).is_err());
}

#[test]
fn inconsistent_crosswalks_and_unmapped_identities_stop_import() {
    let (_path, conn) = fixture();
    let expected = db::primary_session_id(&conn, "2026-09-01")
        .unwrap()
        .unwrap();
    conn.execute("UPDATE legacy_crosswalk SET entity_id='wrong-owner' WHERE legacy_table='sessions' AND legacy_key='2026-09-01'", []).unwrap();
    assert!(primary_import::inspect(&conn)
        .unwrap_err()
        .to_string()
        .contains("crosswalk"));
    conn.execute("UPDATE legacy_crosswalk SET entity_id=?1 WHERE legacy_table='sessions' AND legacy_key='2026-09-01'", [&expected]).unwrap();
    conn.execute("INSERT INTO legacy_crosswalk VALUES('sessions','missing-row','primary_session','unmapped','old')", []).unwrap();
    assert!(primary_import::inspect(&conn).is_err());
}

#[test]
fn unknown_stages_missing_content_and_missing_terminal_times_need_recovery() {
    let (_path, conn) = fixture();
    conn.execute("INSERT INTO sessions(date,status,current_step,reading_seconds,focus,session_type) VALUES('not-a-date','in_progress','unknown-stage',-4,'javascript','unknown-kind')", []).unwrap();
    conn.execute("INSERT INTO sessions(date,status,current_step,focus) VALUES('2026-09-05','in_progress','course','javascript')", []).unwrap();
    conn.execute(
        "UPDATE sessions SET completed_at=NULL WHERE date=?1",
        params!["2026-08-31"],
    )
    .unwrap();
    let plan = primary_import::inspect(&conn).unwrap();
    let unusual = plan
        .sessions
        .iter()
        .find(|s| s.service_date == "not-a-date")
        .unwrap();
    for reason in [
        RecoveryReason::InvalidServiceDate,
        RecoveryReason::UnknownStage,
        RecoveryReason::UnknownSessionKind,
        RecoveryReason::InvalidReadingTime,
    ] {
        assert!(unusual.recovery.contains(&reason));
    }
    assert!(plan
        .sessions
        .iter()
        .find(|s| s.service_date == "2026-09-05")
        .unwrap()
        .recovery
        .contains(&RecoveryReason::ReadingWithoutLesson));
    let finished = plan
        .sessions
        .iter()
        .find(|s| s.service_date == "2026-08-31")
        .unwrap();
    assert_eq!(finished.original_status, "completed");
    assert!(finished
        .recovery
        .contains(&RecoveryReason::TerminalTimeNotRecorded));
}

#[test]
fn inspection_rejects_an_unknown_schema_or_modified_migration_ledger() {
    let (_path, conn) = fixture();
    conn.pragma_update(None, "user_version", 99).unwrap();
    assert!(primary_import::inspect(&conn)
        .unwrap_err()
        .to_string()
        .contains("schema v7"));
    conn.pragma_update(None, "user_version", 7).unwrap();
    conn.execute(
        "UPDATE schema_migrations SET checksum='tampered' WHERE version=6",
        [],
    )
    .unwrap();
    assert!(primary_import::inspect(&conn)
        .unwrap_err()
        .to_string()
        .contains("migration history mismatch"));
}

#[test]
fn unfamiliar_frozen_rounds_and_lesson_metadata_require_recovery_without_resampling() {
    let (_path, conn) = fixture();
    let owner = Owner::LegacyPrimary("2026-09-01".into());
    assessments::start(
        &conn,
        &owner,
        Purpose::Retrieval,
        &json!({"old":true}),
        "unknown-old-format",
        &[Item {
            id: "old-question".into(),
            body: json!({"legacy_prompt":"must remain intact"}),
        }],
    )
    .unwrap();
    conn.execute(
        "UPDATE courses SET resources_json='{ malformed stored sources' WHERE id=11",
        [],
    )
    .unwrap();
    let original_questions: i64 = conn
        .query_row("SELECT COUNT(*) FROM questions", [], |r| r.get(0))
        .unwrap();
    let plan = primary_import::inspect(&conn).unwrap();
    let row = plan
        .sessions
        .iter()
        .find(|s| s.service_date == "2026-09-01")
        .unwrap();
    assert!(row
        .recovery
        .contains(&RecoveryReason::AssessmentFormatUnknown));
    assert!(row
        .recovery
        .contains(&RecoveryReason::LessonMetadataUnknown));
    assert_eq!(row.content, ContentSelection::Stored { course_id: 11 });
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM questions", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        original_questions
    );
}

#[test]
fn original_main_and_pr_upgrade_records_keep_system_design_and_all_archived_documents() {
    for schema in [
        include_str!("fixtures/upgrades/original-main.sql"),
        include_str!("fixtures/upgrades/pr-head.sql"),
        include_str!("fixtures/upgrades/classroom-exercises.sql"),
    ] {
        let directory = std::env::temp_dir().join(format!(
            "principia-primary-old-{:032x}",
            rand::random::<u128>()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("learner.db");
        let old = Connection::open(&path).unwrap();
        old.execute_batch(schema).unwrap();
        old.execute_batch(include_str!("fixtures/upgrades/common-records.sql"))
            .unwrap();
        drop(old);
        let conn = db::open(&path).unwrap();
        let plan = primary_import::inspect(&conn).unwrap();
        assert_eq!(plan.sessions.len(), 1);
        assert_eq!(
            plan.sessions[0].subject,
            SubjectResolution::LinkedConcept {
                course_id: "system-design".into(),
                concept_id: 50
            }
        );
        assert_eq!(plan.sessions[0].original_status, "in_progress");
        assert_eq!(
            plan.sessions[0].content,
            ContentSelection::Ambiguous {
                course_ids: vec![501, 502]
            }
        );
        assert_eq!(
            plan.tables["audio_scripts"][0]["audio_dir"],
            Cell::Text(b"/retained/audio/501".to_vec())
        );
        assert_eq!(
            plan.tables["attempts"][0]["user_answer"],
            Cell::Text(b"A".to_vec())
        );
        let progress: (String, f64, i64) = conn
            .query_row(
                "SELECT state,score_ema,encounters FROM mastery WHERE concept_id=50",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(progress, ("practicing".into(), 0.725, 4));
        assert_eq!(
            plan.tables["sessions"][0]["focus"],
            Cell::Text(Vec::new()),
            "subject recovery must not rewrite the legacy source"
        );
    }
}

// ── One-time import into the shared study runtime ──────────────────────────

#[test]
fn apply_imports_finished_daily_sessions_and_leaves_open_work_and_recovery_rows_alone() {
    use principia_desk_lib::{
        classroom,
        domain::sessions::{self, SessionId, Status},
        language,
        progress::{self, ProgressQuery},
    };
    let (path, conn) = fixture();
    // Progress lists every class, so the class registry must exist.
    language::initialize(&conn, "2026-09-10").unwrap();
    classroom::initialize(&conn).unwrap();
    conn.execute_batch("INSERT INTO sessions(date,concept_id,status,current_step,reading_seconds,focus,session_type,quiz_score,completed_at) VALUES
        ('2026-08-29',1,'skipped','quiz',0,'javascript','lesson',NULL,'2026-08-29T09:00:00'),
        ('2026-08-28',NULL,'completed','done',0,'javascript','pop_quiz',0.75,'2026-08-28T09:00:00'),
        ('2026-08-27',1,'completed','done',10,'javascript','lesson',NULL,NULL),
        ('2026-08-26',NULL,'completed','done',0,'not-a-course','pop_quiz',0.5,'2026-08-26T09:00:00');
        INSERT INTO exit_questions(course_id,round,prompt,choices_json,correct_answer,explanation,section,learning_objective)
        VALUES(16,1,'Archived check','[\"a\",\"b\"]','a','because','Section','Objective');").unwrap();
    let legacy_rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
        .unwrap();
    let summary = primary_import::apply(&conn).unwrap();
    assert_eq!(summary.imported.len(), 3, "{summary:?}");
    assert_eq!(
        summary.open_work, 4,
        "three in-progress days and one pending day stay legacy"
    );
    assert_eq!(summary.retained.len(), 2);
    let retained_dates: Vec<_> = summary
        .retained
        .iter()
        .map(|r| r.service_date.as_str())
        .collect();
    assert_eq!(retained_dates, ["2026-08-26", "2026-08-27"]);
    assert!(summary.retained[0]
        .recovery
        .contains(&RecoveryReason::CourseNotInCatalog));
    assert!(summary.retained[1]
        .recovery
        .contains(&RecoveryReason::TerminalTimeNotRecorded));

    let finished = summary
        .imported
        .iter()
        .find(|s| s.service_date == "2026-08-31")
        .unwrap();
    assert_eq!(
        finished.session_id,
        db::primary_session_id(&conn, "2026-08-31")
            .unwrap()
            .unwrap()
    );
    let id = SessionId(finished.session_id.clone());
    let session = sessions::get(&conn, &id).unwrap();
    assert_eq!(session.status, Status::Completed);
    assert_eq!(session.finished_at.as_deref(), Some("2026-08-31T10:00:00"));
    assert_eq!(session.context.course.course_id, "javascript");
    assert_eq!(session.context.tutor.provider, "claude");
    assert_eq!(session.context.tutor.model, "unknown");
    assert_eq!(session.context.selection["adapter"], "legacy_primary");
    let lesson = sessions::lesson(&conn, &id).unwrap().unwrap();
    assert_eq!(lesson.content.title, "Original JS topic");
    assert_eq!(
        lesson.content.body["markdown"],
        "# Archived completed lesson"
    );
    assert_eq!(
        lesson.content.body["exit_checks"].as_array().unwrap().len(),
        1
    );
    assert_eq!(lesson.content.provenance["kind"], "legacy_primary");
    let result = sessions::result(&conn, &id).unwrap().unwrap();
    assert_eq!(result.outcome["legacy"]["reading_seconds"], 1800);
    assert_eq!(result.checkpoint.work["reading_seconds"], 1800);

    let skipped = summary
        .imported
        .iter()
        .find(|s| s.service_date == "2026-08-29")
        .unwrap();
    assert_eq!(skipped.disposition, "skipped");
    assert!(skipped.lesson_version_id.is_none());
    assert_eq!(
        sessions::get(&conn, &SessionId(skipped.session_id.clone()))
            .unwrap()
            .status,
        Status::Skipped
    );

    let retrieval = summary
        .imported
        .iter()
        .find(|s| s.service_date == "2026-08-28")
        .unwrap();
    let retrieval_lesson = sessions::lesson(&conn, &SessionId(retrieval.session_id.clone()))
        .unwrap()
        .unwrap();
    assert_eq!(retrieval_lesson.content.body["kind"], "legacy_retrieval");
    assert_eq!(retrieval_lesson.content.body["quiz_score"], 0.75);

    // Idempotent, and the legacy source rows are untouched.
    let again = primary_import::apply(&conn).unwrap();
    assert!(again.imported.is_empty());
    assert_eq!(again.already_imported, 3);
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        legacy_rows
    );
    assert_eq!(
        conn.query_row(
            "SELECT status FROM sessions WHERE date='2026-08-31'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "completed"
    );
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM study_sessions", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        3
    );

    // History reads imported days from the runtime once, and the rest from legacy.
    let dashboard = progress::read(&conn, "2026-09-10", &ProgressQuery::default()).unwrap();
    let study: Vec<_> = dashboard
        .history
        .iter()
        .filter(|e| e.source == "study")
        .collect();
    assert_eq!(study.len(), 3);
    for entry in &dashboard.history {
        assert!(
            !(entry.source == "primary"
                && ["2026-08-31", "2026-08-29", "2026-08-28"].contains(&entry.date.as_str())),
            "{entry:?}"
        );
    }
    let archived = study.iter().find(|e| e.date == "2026-08-31").unwrap();
    assert!(
        archived.can_read && archived.subject_id == "javascript" && archived.status == "completed"
    );
    let quiz_day = study.iter().find(|e| e.date == "2026-08-28").unwrap();
    assert!(!quiz_day.can_read);
    assert!((quiz_day.score.unwrap() - 0.75).abs() < 1e-9);
    assert!(dashboard
        .history
        .iter()
        .any(|e| e.source == "primary" && e.date == "2026-09-01"));
    assert!(dashboard
        .history
        .iter()
        .any(|e| e.source == "primary" && e.date == "2026-08-27"));
    assert!(progress::lesson(&conn, "study", &finished.session_id)
        .unwrap()
        .unwrap()
        .markdown
        .contains("Archived completed lesson"));
    drop(conn);
    let reopened = db::open(&path).unwrap();
    assert_eq!(
        reopened
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    assert!(reopened
        .prepare("PRAGMA foreign_key_check")
        .unwrap()
        .query([])
        .unwrap()
        .next()
        .unwrap()
        .is_none());
}

#[test]
fn apply_keeps_the_active_class_session_in_the_foreground() {
    use principia_desk_lib::{
        classroom::{
            self, ConfigureClassroomInput, StoredEngineeringLesson, StoredQuestion,
            UpsertClassroomSlotInput,
        },
        domain::sessions::{self, PreparedLesson, Status},
        language,
        subjects::engineering,
    };
    let directory = std::env::temp_dir().join(format!(
        "principia-import-foreground-{:032x}",
        rand::random::<u128>()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let conn = db::open(&directory.join("learner.db")).unwrap();
    db::seed_concepts(&conn, include_str!("../seed/concepts.json")).unwrap();
    language::initialize(&conn, "2026-09-10").unwrap();
    classroom::initialize(&conn).unwrap();
    classroom::upsert_slot(
        &conn,
        &UpsertClassroomSlotInput {
            id: None,
            subject_id: "javascript".into(),
            hour: 9,
            minute: 0,
            weekdays: vec![1, 2, 3, 4, 5, 6, 7],
            enabled: true,
            durations: Default::default(),
            starts: Default::default(),
        },
    )
    .unwrap();
    classroom::configure_program(
        &conn,
        &ConfigureClassroomInput {
            subject_id: "javascript".into(),
            enabled: true,
            agent: "claude".into(),
            model: "sonnet".into(),
            custom_agent_bin: String::new(),
            session_minutes: 30,
            start_level: None,
            target_level: None,
            weekly_minutes: None,
        },
        "2026-09-10",
    )
    .unwrap();
    let program = classroom::program_row(&conn, "javascript").unwrap();
    let planned = engineering::plan(&conn, &program, None, None, "2026-09-10", false).unwrap();
    let chosen = engineering::selection(&planned).unwrap();
    let lease = sessions::claim_preparation(&conn, &planned.id, chrono::Utc::now(), 60)
        .unwrap()
        .unwrap();
    let stored = StoredEngineeringLesson {
        concept_id: chosen.concept_id,
        concept_title: chosen.title.clone(),
        category: chosen.category.clone(),
        title: "Class lesson".into(),
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
        &conn,
        &lease,
        &PreparedLesson {
            title: "Class lesson".into(),
            body: serde_json::to_value(&stored).unwrap(),
            provenance: json!({"kind":"fixture"}),
        },
        chrono::Utc::now(),
    )
    .unwrap();
    let active = engineering::activate(&conn, &planned.id).unwrap();
    assert_eq!(active.status, Status::Active);
    let concept_id: i64 = conn
        .query_row(
            "SELECT id FROM concepts WHERE focus='javascript' ORDER BY id LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    conn.execute("INSERT INTO sessions(date,concept_id,status,current_step,reading_seconds,focus,completed_at) VALUES('2026-08-31',?1,'completed','done',1800,'javascript','2026-08-31T10:00:00')", params![concept_id]).unwrap();
    conn.execute("INSERT INTO courses(session_date,concept_id,markdown,resources_json,source,generated_at) VALUES('2026-08-31',?1,'# Archived','[]','codex','original')", params![concept_id]).unwrap();
    let summary = primary_import::apply(&conn).unwrap();
    assert_eq!(summary.imported.len(), 1);
    let after = sessions::get(&conn, &planned.id).unwrap();
    assert_eq!(
        after.status,
        Status::Active,
        "the class lesson keeps the foreground"
    );
    assert_eq!(after.revision, active.revision + 2);
    assert_eq!(
        classroom::completed_lesson_count(&conn, "javascript").unwrap(),
        1,
        "legacy completion still counts once"
    );
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM study_sessions WHERE status='active'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
}
