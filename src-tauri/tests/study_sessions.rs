use chrono::{DateTime, Duration, Utc};
use principia_desk_lib::{
    classroom, db,
    domain::{
        classes::{self, AcceptPath},
        enrollment::{self, EntryChoice, SaveEnrollmentDraft},
        placement,
        sessions::{
            self, CheckpointBody, Disposition, PlanOwner, PlanSession, PreparedLesson, SessionId,
            SessionKind, Stage, Status,
        },
    },
    language,
};
use rusqlite::{params, Connection};
use serde_json::json;
use std::sync::{Arc, Barrier};

fn now() -> DateTime<Utc> {
    "2026-09-09T23:59:50Z".parse().unwrap()
}
fn fixture() -> (std::path::PathBuf, Connection) {
    let path = std::env::temp_dir().join(format!(
        "principia-study-{:032x}/learner.db",
        rand::random::<u128>()
    ));
    let conn = db::open(&path).unwrap();
    db::seed_concepts(&conn, include_str!("../seed/concepts.json")).unwrap();
    language::initialize(&conn, "2026-09-09").unwrap();
    classroom::initialize(&conn).unwrap();
    (path, conn)
}
fn request(conn: &Connection, course: &str, entry: &str) -> PlanSession {
    if !classroom::has_enabled_schedule(conn, course).unwrap() {
        // Active classes may not overlap, so each course keeps its own hour.
        let hour = 6 + principia_desk_lib::catalog::COURSES
            .iter()
            .position(|c| c.id == course)
            .unwrap_or(0) as u32;
        classroom::upsert_slot(
            conn,
            &classroom::UpsertClassroomSlotInput {
                id: None,
                subject_id: course.into(),
                hour,
                minute: 0,
                weekdays: vec![1, 3, 5],
                enabled: true,
                durations: Default::default(),
                starts: Default::default(),
            },
        )
        .unwrap();
    }
    let options = enrollment::options(course).unwrap();
    let mut configuration = options.default_configuration;
    configuration.entry = EntryChoice::Manual {
        entry_point: entry.into(),
        familiar_competencies: vec![],
    };
    let draft = enrollment::save_draft(
        conn,
        &SaveEnrollmentDraft {
            id: None,
            expected_revision: None,
            course: options.course,
            configuration,
        },
    )
    .unwrap();
    let recommendation = placement::recommend(conn, &draft.id, draft.revision).unwrap();
    let path = classes::accept(
        conn,
        &AcceptPath {
            draft_id: draft.id,
            expected_revision: draft.revision,
            recommendation_id: recommendation.id,
        },
        "2026-09-09",
    )
    .unwrap();
    PlanSession {
        request_key: format!("request-{:032x}", rand::random::<u128>()),
        owner: PlanOwner::Class {
            path: path.reference,
        },
        kind: SessionKind::Lesson,
        stages: vec![Stage::Learn, Stage::Practice, Stage::Check, Stage::Feedback],
        selection: json!({"unit":entry,"reason":"accepted personal path","prompt_version":"test-v1"}),
    }
}
fn content() -> PreparedLesson {
    PreparedLesson {
        title: "Reliable shell arguments".into(),
        body: json!({"markdown":"Keep quoted arguments together.","exercise":{"starter":"printf '%s\\n' \"$@\""},"questions":[{"id":"quote-1","prompt":"How many arguments?","answer":"one"}]}),
        provenance: json!({"kind":"bundled_fixture","prompt_version":"test-v1"}),
    }
}
fn ready(conn: &Connection, input: &PlanSession) -> sessions::Session {
    let session = sessions::plan(conn, input, now()).unwrap();
    let lease = sessions::claim_preparation(conn, &session.id, now(), 60)
        .unwrap()
        .unwrap();
    sessions::publish_preparation(conn, &lease, &content(), now()).unwrap();
    sessions::get(conn, &session.id).unwrap()
}
fn feedback(conn: &Connection, session: &sessions::Session) -> sessions::Session {
    let mut body = session.checkpoint.body.clone();
    body.stage = Stage::Feedback;
    body.work
        .insert("script".into(), json!("printf '%s\\n' \"$@\""));
    sessions::save_checkpoint(conn, &session.id, session.checkpoint.revision, &body, now())
        .unwrap();
    sessions::get(conn, &session.id).unwrap()
}
fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

#[test]
fn restart_keeps_identity_content_tutor_path_and_checkpoint_across_midnight() {
    let (file, conn) = fixture();
    let input = request(&conn, "linux-bash", "mechanisms");
    let ready = ready(&conn, &input);
    let session = sessions::activate(&conn, &ready.id, ready.revision, now()).unwrap();
    let mut work = session.checkpoint.body.clone();
    work.stage = Stage::Practice;
    work.reading.anchor = Some("#quoted-arguments".into());
    work.reading.offset = 137;
    work.work.insert(
        "transcript".into(),
        json!("$ printf '%s\\n' \"two words\"\ntwo words"),
    );
    let saved = sessions::save_checkpoint(&conn, &session.id, 0, &work, now()).unwrap();
    let old_content = sessions::lesson(&conn, &session.id).unwrap().unwrap();
    let old_context = session.context.clone();
    // A new path and different tutor govern later selections only.
    let revised = request(&conn, "linux-bash", "production");
    let mut config = classes::current_configuration(&conn, "linux-bash")
        .unwrap()
        .unwrap();
    config.tutor.provider = "openrouter".into();
    config.tutor.model = "qwen/qwen3-14b".into();
    conn.execute(
        "UPDATE classes SET configuration_json=?1 WHERE course_id='linux-bash'",
        [serde_json::to_string(&config).unwrap()],
    )
    .unwrap();
    drop(conn);
    let reopened = db::open(&file).unwrap();
    let resumed = sessions::resumable(&reopened, &revised.owner)
        .unwrap()
        .unwrap();
    assert_eq!(resumed.id, session.id);
    assert_eq!(resumed.context, old_context);
    assert_eq!(resumed.checkpoint, saved);
    assert_eq!(
        sessions::lesson(&reopened, &resumed.id).unwrap().unwrap(),
        old_content
    );
    assert_eq!(
        sessions::plan(&reopened, &input, now() + Duration::days(1))
            .unwrap()
            .id,
        session.id
    );
    assert!(sessions::plan(&reopened, &revised, now() + Duration::days(1)).is_err());
    for table in [
        "sessions",
        "classroom_sessions",
        "language_sessions",
        "attempts",
        "mastery",
        "study_results",
    ] {
        assert_eq!(
            count(&reopened, table),
            0,
            "unexpected learning credit in {table}"
        );
    }
}

#[test]
fn independent_connections_deduplicate_planning_and_preparation() {
    let (file, conn) = fixture();
    let input = request(&conn, "bash-scripting", "foundations");
    let barrier = Arc::new(Barrier::new(2));
    let threads: Vec<_> = (0..2)
        .map(|_| {
            let file = file.clone();
            let input = input.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let conn = db::open(&file).unwrap();
                barrier.wait();
                sessions::plan(&conn, &input, now()).unwrap().id
            })
        })
        .collect();
    let ids: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    assert_eq!(ids[0], ids[1]);
    assert_eq!(count(&conn, "study_sessions"), 1);
    let threads: Vec<_> = (0..2)
        .map(|_| {
            let file = file.clone();
            let id = ids[0].clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let conn = db::open(&file).unwrap();
                barrier.wait();
                sessions::claim_preparation(&conn, &id, now(), 60).unwrap()
            })
        })
        .collect();
    let leases: Vec<_> = threads
        .into_iter()
        .filter_map(|t| t.join().unwrap())
        .collect();
    assert_eq!(leases.len(), 1);
    let lease = &leases[0];
    let published = sessions::publish_preparation(&conn, lease, &content(), now()).unwrap();
    assert_eq!(
        sessions::publish_preparation(&conn, lease, &content(), now() + Duration::minutes(5))
            .unwrap(),
        published
    );
    let mut different = content();
    different.body["markdown"] = json!("Replaced after publication");
    assert!(sessions::publish_preparation(&conn, lease, &different, now()).is_err());
    assert_eq!(count(&conn, "lesson_versions"), 1);
    let mut different_request = input.clone();
    different_request.selection["unit"] = json!("another unit");
    assert!(sessions::plan(&conn, &different_request, now()).is_err());
}

#[test]
fn expired_workers_cannot_publish_and_failed_requests_retry_without_losing_selection() {
    let (file, conn) = fixture();
    let input = request(&conn, "linux-bash", "foundations");
    let planned = sessions::plan(&conn, &input, now()).unwrap();
    let first = sessions::claim_preparation(&conn, &planned.id, now(), 10)
        .unwrap()
        .unwrap();
    let renewed =
        sessions::renew_preparation(&conn, &first, now() + Duration::seconds(5), 20).unwrap();
    assert_eq!(
        renewed.expires_at_millis,
        (now() + Duration::seconds(25)).timestamp_millis()
    );
    assert!(
        sessions::claim_preparation(&conn, &planned.id, now() + Duration::seconds(11), 10)
            .unwrap()
            .is_none()
    );
    drop(conn);
    let conn = db::open(&file).unwrap();
    let later = now() + Duration::seconds(25);
    assert!(sessions::publish_preparation(&conn, &first, &content(), later).is_err());
    let replacement = sessions::claim_preparation(&conn, &planned.id, later, 60)
        .unwrap()
        .unwrap();
    assert_ne!(first.token, replacement.token);
    assert!(sessions::fail_preparation(&conn, &first, "old worker failed", later).is_err());
    sessions::fail_preparation(&conn, &replacement, "provider unavailable", later).unwrap();
    assert_eq!(
        sessions::preparation(&conn, &planned.id)
            .unwrap()
            .error
            .as_deref(),
        Some("provider unavailable")
    );
    assert!(sessions::claim_preparation(&conn, &planned.id, later, 60)
        .unwrap()
        .is_none());
    sessions::retry_preparation(&conn, &planned.id, later).unwrap();
    sessions::retry_preparation(&conn, &planned.id, later).unwrap();
    let retry = sessions::claim_preparation(&conn, &planned.id, later, 60)
        .unwrap()
        .unwrap();
    assert_eq!(retry.request_fingerprint, first.request_fingerprint);
    assert!(sessions::publish_preparation(&conn, &replacement, &content(), later).is_err());
    sessions::publish_preparation(&conn, &retry, &content(), later).unwrap();
    assert_eq!(
        sessions::preparation(&conn, &planned.id).unwrap().attempts,
        3
    );
    assert_eq!(
        sessions::get(&conn, &planned.id).unwrap().context.selection,
        input.selection
    );
}

#[test]
fn foreground_handoff_preserves_other_class_work_and_rejects_stale_autosaves() {
    let (_, conn) = fixture();
    let one = ready(&conn, &request(&conn, "linux-bash", "foundations"));
    let two = ready(&conn, &request(&conn, "german", "A1"));
    let first = sessions::activate(&conn, &one.id, one.revision, now()).unwrap();
    let second = sessions::activate(&conn, &two.id, two.revision, now()).unwrap();
    let paused = sessions::get(&conn, &first.id).unwrap();
    assert_eq!(paused.status, Status::Paused);
    assert_eq!(second.status, Status::Active);
    assert!(sessions::activate(&conn, &first.id, first.revision, now()).is_err());
    // A pending save captured its original owner before the foreground switch.
    let mut body = first.checkpoint.body.clone();
    body.work.insert("script".into(), json!("original work"));
    let saved = sessions::save_checkpoint(&conn, &first.id, 0, &body, now()).unwrap();
    assert_eq!(
        sessions::save_checkpoint(&conn, &first.id, 0, &body, now()).unwrap(),
        saved
    );
    body.work
        .insert("script".into(), json!("stale replacement"));
    assert!(sessions::save_checkpoint(&conn, &first.id, 0, &body, now()).is_err());
    assert!(sessions::get(&conn, &second.id)
        .unwrap()
        .checkpoint
        .body
        .work
        .is_empty());
    let resumed = sessions::activate(&conn, &first.id, paused.revision, now()).unwrap();
    assert_eq!(resumed.checkpoint, saved);
    assert_eq!(
        sessions::get(&conn, &second.id).unwrap().status,
        Status::Paused
    );
}

#[test]
fn completion_is_atomic_idempotent_and_keeps_submitted_work_immutable() {
    let (_, conn) = fixture();
    let input = request(&conn, "linux-bash", "foundations");
    let ready = ready(&conn, &input);
    let active = sessions::activate(&conn, &ready.id, ready.revision, now()).unwrap();
    assert!(sessions::finish(
        &conn,
        &active.id,
        active.revision,
        0,
        Disposition::Completed,
        now(),
        |_, _| panic!("adapter must not run before feedback")
    )
    .is_err());
    let active = feedback(&conn, &active);
    let failed = sessions::finish(
        &conn,
        &active.id,
        active.revision,
        active.checkpoint.revision,
        Disposition::Completed,
        now(),
        |tx, _| {
            db::set_config(tx, "test_projection", "partial credit")?;
            Err(db::DbError::Invalid(
                "injected occurrence projection failure".into(),
            ))
        },
    );
    assert!(failed.is_err());
    assert_eq!(db::get_config(&conn, "test_projection").unwrap(), None);
    assert_eq!(
        sessions::get(&conn, &active.id).unwrap().status,
        Status::Active
    );
    assert!(sessions::result(&conn, &active.id).unwrap().is_none());
    let complete = sessions::finish(
        &conn,
        &active.id,
        active.revision,
        active.checkpoint.revision,
        Disposition::Completed,
        now(),
        |tx, s| {
            db::set_config(tx, "test_projection", "one committed projection")?;
            Ok(json!({"completed_work":s.checkpoint.body.work}))
        },
    )
    .unwrap();
    let retry = sessions::finish(
        &conn,
        &active.id,
        active.revision,
        active.checkpoint.revision,
        Disposition::Completed,
        now() + Duration::days(1),
        |_, _| panic!("duplicate progress write"),
    )
    .unwrap();
    assert_eq!(complete, retry);
    assert_eq!(complete.checkpoint, active.checkpoint.body);
    assert!(sessions::save_checkpoint(
        &conn,
        &active.id,
        active.checkpoint.revision,
        &active.checkpoint.body,
        now()
    )
    .is_err());
    for sql in [
        "UPDATE study_sessions SET status='active' WHERE id=?1",
        "UPDATE study_sessions SET context_json='{}' WHERE id=?1",
        "UPDATE study_checkpoints SET body_json='{}' WHERE session_id=?1",
        "UPDATE lesson_versions SET content_json='{}' WHERE session_id=?1",
        "UPDATE study_results SET outcome_json='{}' WHERE session_id=?1",
        "DELETE FROM study_sessions WHERE id=?1",
    ] {
        assert!(conn.execute(sql, [&active.id.0]).is_err(), "allowed {sql}");
    }
    assert_eq!(sessions::plan(&conn, &input, now()).unwrap().id, active.id);
    let mut next = input.clone();
    next.request_key = "explicit-next-lesson".into();
    let next = sessions::plan(&conn, &next, now()).unwrap();
    assert_ne!(next.id, active.id);
    let PlanOwner::Class { path } = input.owner else {
        unreachable!()
    };
    assert_eq!(sessions::history(&conn, &path.class_id).unwrap().len(), 2);
}

#[test]
fn skipping_interrupted_preparation_invalidates_worker_and_preserves_history() {
    let (_, conn) = fixture();
    let input = request(&conn, "bash-scripting", "foundations");
    let planned = sessions::plan(&conn, &input, now()).unwrap();
    let lease = sessions::claim_preparation(&conn, &planned.id, now(), 60)
        .unwrap()
        .unwrap();
    let preparing = sessions::get(&conn, &planned.id).unwrap();
    let skipped = sessions::finish(
        &conn,
        &planned.id,
        preparing.revision,
        0,
        Disposition::Skipped,
        now(),
        |_, _| Ok(json!({"reason":"learner requested a different lesson"})),
    )
    .unwrap();
    assert_eq!(skipped.disposition, Disposition::Skipped);
    assert_eq!(
        sessions::preparation(&conn, &planned.id).unwrap().status,
        "cancelled"
    );
    assert!(sessions::publish_preparation(&conn, &lease, &content(), now()).is_err());
    assert!(sessions::retry_preparation(&conn, &planned.id, now()).is_err());
    assert!(sessions::resumable(&conn, &input.owner).unwrap().is_none());
    assert_eq!(count(&conn, "lesson_versions"), 0);
    assert_eq!(count(&conn, "mastery"), 0);
}

#[test]
fn daily_routine_has_stable_identity_without_inventing_a_class_or_allowing_unready_focus() {
    let (_, conn) = fixture();
    let options = enrollment::options("system-design").unwrap();
    let input = PlanSession {
        request_key: "daily-start".into(),
        owner: PlanOwner::DailyRoutine {
            service_date: "2026-09-09".into(),
            course: options.course,
            tutor: options.default_configuration.tutor,
            focus_policy: enrollment::FocusPolicy::Strict,
            goal: options.default_configuration.goal,
            pace: options.default_configuration.pace,
        },
        kind: SessionKind::Retrieval,
        stages: vec![Stage::Recall, Stage::Check, Stage::Feedback],
        selection: json!({"review_concepts":["caching"],"reason":"retrieval due"}),
    };
    let planned = sessions::plan(&conn, &input, now()).unwrap();
    assert!(sessions::activate(&conn, &planned.id, planned.revision, now()).is_err());
    let prepared = ready(&conn, &input);
    assert!(
        sessions::activate(&conn, &prepared.id, prepared.revision, now())
            .unwrap_err()
            .to_string()
            .contains("focus coordinator")
    );
    assert_eq!(count(&conn, "classes"), 0);
    assert_eq!(count(&conn, "path_revisions"), 0);
    let mut tomorrow = input.clone();
    tomorrow.request_key = "tomorrow".into();
    if let PlanOwner::DailyRoutine { service_date, .. } = &mut tomorrow.owner {
        *service_date = "2026-09-10".into();
    }
    assert_eq!(
        sessions::resumable(&conn, &tomorrow.owner)
            .unwrap()
            .unwrap()
            .id,
        planned.id
    );
    assert!(sessions::plan(&conn, &tomorrow, now() + Duration::days(1)).is_err());
    let mut invalid = input.clone();
    invalid.request_key = "invalid-retrieval".into();
    invalid.stages = vec![Stage::Learn, Stage::Feedback];
    assert!(sessions::plan(&conn, &invalid, now()).is_err());
    assert!(sessions::get(&conn, &SessionId("missing".into())).is_err());
}

#[test]
fn competing_checkpoint_writes_do_not_silently_replace_each_other() {
    let (file, conn) = fixture();
    let input = request(&conn, "linux-bash", "foundations");
    let prepared = ready(&conn, &input);
    let barrier = Arc::new(Barrier::new(2));
    let threads: Vec<_> = (0..2)
        .map(|index| {
            let file = file.clone();
            let id = prepared.id.clone();
            let barrier = barrier.clone();
            let mut body: CheckpointBody = prepared.checkpoint.body.clone();
            body.work
                .insert("draft".into(), json!(format!("writer {index}")));
            std::thread::spawn(move || {
                let conn = db::open(&file).unwrap();
                barrier.wait();
                sessions::save_checkpoint(&conn, &id, 0, &body, now()).is_ok()
            })
        })
        .collect();
    assert_eq!(
        threads
            .into_iter()
            .map(|t| usize::from(t.join().unwrap()))
            .sum::<usize>(),
        1
    );
    let checkpoint = sessions::get(&conn, &prepared.id).unwrap().checkpoint;
    assert_eq!(checkpoint.revision, 1);
    assert!(matches!(
        checkpoint.body.work["draft"].as_str(),
        Some("writer 0" | "writer 1")
    ));
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM study_checkpoints WHERE session_id=?1",
            params![prepared.id.0],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
}

#[test]
fn shared_assessments_commit_with_session_results_and_pending_rounds_block_completion() {
    use principia_desk_lib::domain::assessments::{
        self, Item, Owner, Purpose, Response, ResponseStatus,
    };
    let (_, conn) = fixture();
    let input = request(&conn, "linux-bash", "foundations");
    let prepared = ready(&conn, &input);
    let active = sessions::activate(&conn, &prepared.id, prepared.revision, now()).unwrap();
    let active = feedback(&conn, &active);
    let owner = Owner::StudySession(active.id.0.clone());
    let round = assessments::start(
        &conn,
        &owner,
        Purpose::ExitCheck,
        &json!({"lesson_version_id":active.lesson_version_id}),
        "quoting-v1",
        &[Item {
            id: "quote-1".into(),
            body: json!({"prompt":"How many arguments?","answer":"one"}),
        }],
    )
    .unwrap();
    let pending = sessions::finish(
        &conn,
        &active.id,
        active.revision,
        active.checkpoint.revision,
        Disposition::Completed,
        now(),
        |tx, _| {
            db::set_config(tx, "premature_credit", "1")?;
            Ok(json!({"done":true}))
        },
    );
    assert!(pending
        .unwrap_err()
        .to_string()
        .contains("current assessment"));
    assert_eq!(db::get_config(&conn, "premature_credit").unwrap(), None);
    let round = assessments::save_response(
        &conn,
        &owner,
        &round.id,
        0,
        "quote-1",
        Response {
            answer: "one".into(),
            status: ResponseStatus::Answered,
        },
    )
    .unwrap();
    let failed = sessions::finish(
        &conn,
        &active.id,
        active.revision,
        active.checkpoint.revision,
        Disposition::Completed,
        now(),
        |tx, _| {
            assessments::submit_in_transaction(tx, &owner, &round, &json!({"correct":true}), true)?;
            Err(db::DbError::Invalid(
                "progress projection failed after grading".into(),
            ))
        },
    );
    assert!(failed.is_err());
    assert!(assessments::round(&conn, &round.id)
        .unwrap()
        .submission
        .is_none());
    let completed = sessions::finish(
        &conn,
        &active.id,
        active.revision,
        active.checkpoint.revision,
        Disposition::Completed,
        now(),
        |tx, _| {
            assessments::submit_in_transaction(tx, &owner, &round, &json!({"correct":true}), true)?;
            Ok(json!({"round_id":round.id,"qualifying_work":true}))
        },
    )
    .unwrap();
    assert_eq!(completed.disposition, Disposition::Completed);
    assert!(assessments::round(&conn, &round.id)
        .unwrap()
        .submission
        .is_some());
    assert_eq!(
        assessments::latest(&conn, &owner, Purpose::ExitCheck)
            .unwrap()
            .unwrap()
            .status,
        "completed"
    );
    assert!(assessments::start(&conn, &owner, Purpose::Lesson, &json!({}), "v1", &[]).is_err());
}

#[test]
fn skipping_a_lesson_retains_its_unsubmitted_answers_and_does_not_cancel_class_placement() {
    use principia_desk_lib::domain::assessments::{
        self, Item, Owner, Purpose, Response, ResponseStatus,
    };
    let (_, conn) = fixture();
    let input = request(&conn, "linux-bash", "foundations");
    let prepared = ready(&conn, &input);
    let active = sessions::activate(&conn, &prepared.id, prepared.revision, now()).unwrap();
    let owner = Owner::StudySession(active.id.0.clone());
    let PlanOwner::Class { path } = &input.owner else {
        unreachable!()
    };
    let diagnostic_owner = Owner::Class(path.class_id.clone());
    let item = Item {
        id: "quote-1".into(),
        body: json!({"prompt":"How many arguments?","answer":"one"}),
    };
    let round = assessments::start(
        &conn,
        &owner,
        Purpose::ExitCheck,
        &json!({}),
        "v1",
        std::slice::from_ref(&item),
    )
    .unwrap();
    let answer = Response {
        answer: "thinking about quoting".into(),
        status: ResponseStatus::Draft,
    };
    assessments::save_response(&conn, &owner, &round.id, 0, "quote-1", answer.clone()).unwrap();
    assessments::start(
        &conn,
        &diagnostic_owner,
        Purpose::Diagnostic,
        &json!({}),
        "v1",
        &[item],
    )
    .unwrap();
    sessions::finish(
        &conn,
        &active.id,
        active.revision,
        0,
        Disposition::Skipped,
        now(),
        |_, _| Ok(json!({"reason":"pause this topic"})),
    )
    .unwrap();
    let preserved = assessments::round(&conn, &round.id).unwrap();
    assert_eq!(preserved.responses["quote-1"], answer);
    assert!(preserved.submission.is_none());
    assert!(assessments::save_response(
        &conn,
        &owner,
        &round.id,
        preserved.revision,
        "quote-1",
        Response {
            answer: "one".into(),
            status: ResponseStatus::Answered
        }
    )
    .is_err());
    assert_eq!(
        assessments::latest(&conn, &owner, Purpose::ExitCheck)
            .unwrap()
            .unwrap()
            .status,
        "abandoned"
    );
    assert_eq!(
        assessments::latest(&conn, &diagnostic_owner, Purpose::Diagnostic)
            .unwrap()
            .unwrap()
            .status,
        "active"
    );
}
