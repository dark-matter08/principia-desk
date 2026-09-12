use principia_desk_lib::{
    catalog, classroom, db,
    domain::{
        classes::{self, AcceptPath, PathChange, RevisePath},
        enrollment::{self, EntryChoice, LearningGoal, SaveEnrollmentDraft},
        placement,
        sessions::{self, Disposition, PlanOwner, PlanSession, PreparedLesson, SessionKind, Stage},
    },
    language,
};
use rusqlite::{params, Connection};

fn fixture() -> (std::path::PathBuf, Connection) {
    let path = std::env::temp_dir().join(format!(
        "principia-class-{:032x}.db",
        rand::random::<u128>()
    ));
    let conn = db::open(&path).unwrap();
    db::seed_concepts(&conn, include_str!("../seed/concepts.json")).unwrap();
    language::initialize(&conn, "2026-09-09").unwrap();
    classroom::initialize(&conn).unwrap();
    (path, conn)
}
fn setup(conn: &Connection, course: &str, entry: &str) -> AcceptPath {
    let options = enrollment::options(course).unwrap();
    let mut configuration = options.default_configuration;
    configuration.entry = EntryChoice::Manual {
        entry_point: entry.into(),
        familiar_competencies: vec![],
    };
    if let LearningGoal::LanguageLevel { target_level, .. } = &mut configuration.goal {
        *target_level = "B2".into();
    }
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
    AcceptPath {
        draft_id: draft.id,
        expected_revision: draft.revision,
        recommendation_id: recommendation.id,
    }
}
fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

#[test]
fn all_nine_paths_save_without_activating_unscheduled_classes_or_awarding_credit() {
    let (file, conn) = fixture();
    let tables = [
        "sessions",
        "classroom_sessions",
        "language_sessions",
        "mastery",
        "attempts",
        "assessment_attempts",
        "classroom_schedule_slots",
    ];
    let before: Vec<_> = tables.iter().map(|t| count(&conn, t)).collect();
    let mut accepted = vec![];
    for course in catalog::COURSES {
        let request = setup(&conn, course.course_id, course.entry_points[1].id);
        let path = classes::accept(&conn, &request, "2026-09-09").unwrap();
        let repeated = classes::accept(&conn, &request, "2026-09-10").unwrap();
        assert_eq!(
            serde_json::to_value(&path).unwrap(),
            serde_json::to_value(repeated).unwrap()
        );
        assert_eq!(path.recommendation.required_outcome, course.outcome);
        if course.kind == catalog::SubjectKind::Language {
            assert!(language::program_view(&conn, course.id, "2026-09-09")
                .unwrap()
                .milestones
                .iter()
                .all(|m| !m.reached));
        }
        assert!(!classroom::program_row(&conn, course.id).unwrap().enabled);
        assert_eq!(
            classroom::program_view(&conn, course.id, "2026-09-09")
                .unwrap()
                .accepted_path
                .unwrap()
                .entry_point,
            course.entry_points[1].id
        );
        accepted.push(path);
    }
    assert_eq!(count(&conn, "classes"), 9);
    assert_eq!(count(&conn, "path_revisions"), 9);
    for (table, before) in tables.iter().zip(before) {
        assert_eq!(count(&conn, table), before, "acceptance changed {table}");
    }
    assert_eq!(
        conn.query_row("PRAGMA foreign_key_check", [], |_| Ok(()))
            .optional()
            .unwrap(),
        None
    );
    drop(conn);
    let conn = db::open(&file).unwrap();
    for expected in accepted {
        assert_eq!(
            classes::current_path(&conn, &expected.recommendation.course.course_id)
                .unwrap()
                .unwrap()
                .reference,
            expected.reference
        );
    }
}

use rusqlite::OptionalExtension;
#[test]
fn stale_recommendation_and_failed_adapter_write_leave_no_partial_enrollment() {
    let (_, conn) = fixture();
    let mut input = setup(&conn, "linux-bash", "mechanisms");
    let original = input.recommendation_id.clone();
    input.recommendation_id = "stale".into();
    assert!(classes::accept(&conn, &input, "2026-09-09").is_err());
    assert_eq!(count(&conn, "classes"), 0);
    assert!(!classroom::program_row(&conn, "linux-bash").unwrap().enabled);
    input.recommendation_id = original;
    conn.execute_batch("CREATE TRIGGER fail_accept BEFORE INSERT ON path_revisions BEGIN SELECT RAISE(ABORT,'injected write failure'); END;").unwrap();
    assert!(classes::accept(&conn, &input, "2026-09-09").is_err());
    assert_eq!(count(&conn, "classes"), 0);
    assert!(!classroom::program_row(&conn, "linux-bash").unwrap().enabled);
    assert_eq!(
        enrollment::draft(&conn, &input.draft_id).unwrap().status,
        enrollment::DraftStatus::Draft
    );
}

#[test]
fn engineering_selection_starts_in_accepted_phase_without_awarding_prerequisite_mastery() {
    let (_, conn) = fixture();
    for course in catalog::COURSES
        .iter()
        .filter(|c| c.kind == catalog::SubjectKind::Engineering)
    {
        let input = setup(&conn, course.course_id, "mechanisms");
        let accepted = classes::accept(&conn, &input, "2026-09-09").unwrap();
        let concept = classes::next_concept(&conn, course.course_id, "2026-09-09")
            .unwrap()
            .unwrap();
        assert_eq!(
            concept.curriculum.phase, "mechanisms",
            "{} returned {}",
            course.id, concept.slug
        );
        assert!(!accepted
            .recommendation
            .earlier_topics
            .iter()
            .any(|t| t.id == concept.slug));
        assert!(!accepted.recommendation.refreshers.is_empty());
    }
    assert_eq!(count(&conn, "mastery"), 0);
    assert_eq!(count(&conn, "classroom_sessions"), 0);
}

#[test]
fn reassessment_keeps_the_class_and_prior_path_and_the_original_retry_result() {
    let (_, conn) = fixture();
    let first_input = setup(&conn, "bash-scripting", "foundations");
    let first = classes::accept(&conn, &first_input, "2026-09-09").unwrap();
    let second_input = setup(&conn, "bash-scripting", "production");
    let second = classes::accept(&conn, &second_input, "2026-09-10").unwrap();
    assert_eq!(first.reference.class_id, second.reference.class_id);
    assert_ne!(
        first.reference.path_revision_id,
        second.reference.path_revision_id
    );
    assert_eq!(second.revision, 2);
    assert_eq!(
        classes::accept(&conn, &first_input, "2026-09-11")
            .unwrap()
            .reference,
        first.reference
    );
    assert_eq!(
        classes::current_path(&conn, "bash-scripting")
            .unwrap()
            .unwrap()
            .reference,
        second.reference
    );
    assert_eq!(count(&conn, "classes"), 1);
    assert_eq!(count(&conn, "path_revisions"), 2);
    assert_eq!(
        classes::next_concept(&conn, "bash-scripting", "2026-09-11")
            .unwrap()
            .unwrap()
            .curriculum
            .phase,
        "production"
    );
}

#[test]
fn a_new_language_path_changes_future_entry_without_rewriting_an_active_lesson() {
    let (_, conn) = fixture();
    classroom::upsert_slot(
        &conn,
        &classroom::UpsertClassroomSlotInput {
            id: None,
            subject_id: "german".into(),
            hour: 7,
            minute: 30,
            weekdays: vec![1, 3, 5],
            enabled: true,
            durations: Default::default(),
            starts: Default::default(),
        },
    )
    .unwrap();
    let first_input = setup(&conn, "german", "A1");
    let first = classes::accept(&conn, &first_input, "2026-09-09").unwrap();
    let lesson = language::start_session(&conn, "german", "2026-09-09", false).unwrap();
    let before: String = conn
        .query_row(
            "SELECT lesson_json FROM language_sessions WHERE id=?1",
            [lesson.session_id.parse::<i64>().unwrap()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&before).unwrap()["path"]["path_revision_id"],
        first.reference.path_revision_id
    );
    let second_input = setup(&conn, "german", "B1");
    classes::accept(&conn, &second_input, "2026-09-10").unwrap();
    let resumed = language::start_session(&conn, "german", "2026-09-10", false).unwrap();
    assert_eq!(resumed.session_id, lesson.session_id);
    assert_eq!(resumed.level, "A1");
    let after: String = conn
        .query_row(
            "SELECT lesson_json FROM language_sessions WHERE id=?1",
            [lesson.session_id.parse::<i64>().unwrap()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(before, after);
    let (baseline, cursor): (String, String) = conn
        .query_row(
            "SELECT start_level,current_level FROM language_programs WHERE language='german'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(baseline, "A1");
    assert_eq!(cursor, "B1");
    // A fixture terminal state leaves the next start free to use the accepted cursor.
    conn.execute(
        "UPDATE language_sessions SET status='completed' WHERE id=?1",
        params![lesson.session_id.parse::<i64>().unwrap()],
    )
    .unwrap();
    let next = language::start_session(&conn, "german", "2026-09-10", false).unwrap();
    assert_eq!(next.level, "B1");
}

#[test]
fn class_settings_stay_atomic_and_mutable_without_rewriting_the_accepted_path() {
    let (_, conn) = fixture();
    let request = setup(&conn, "german", "B1");
    let accepted = classes::accept(&conn, &request, "2026-09-09").unwrap();
    let mut input = classroom::ConfigureClassroomInput {
        subject_id: "german".into(),
        enabled: false,
        agent: "openrouter".into(),
        model: "openrouter/free".into(),
        custom_agent_bin: "".into(),
        session_minutes: 45,
        start_level: Some("B1".into()),
        target_level: Some("B2".into()),
        weekly_minutes: Some(315),
    };
    classroom::configure_program(&conn, &input, "2026-09-10").unwrap();
    let current = classes::current_configuration(&conn, "german")
        .unwrap()
        .unwrap();
    assert_eq!(current.tutor.provider, "openrouter");
    assert_eq!(current.pace.session_minutes, 45);
    assert_eq!(current.pace.weekly_minutes, Some(315));
    assert_eq!(
        conn.query_row("SELECT status FROM classes", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "paused"
    );
    assert_eq!(
        classes::current_path(&conn, "german")
            .unwrap()
            .unwrap()
            .configuration,
        accepted.configuration
    );
    input.agent = "codex".into();
    input.model = "default".into();
    input.weekly_minutes = Some(1);
    assert!(classroom::configure_program(&conn, &input, "2026-09-10").is_err());
    assert_eq!(
        classroom::program_row(&conn, "german").unwrap().agent,
        "openrouter"
    );
    assert_eq!(
        classes::current_configuration(&conn, "german")
            .unwrap()
            .unwrap(),
        current
    );
    // An accepted entry cannot be silently replaced through the legacy settings form.
    input.weekly_minutes = Some(315);
    input.start_level = Some("A1".into());
    assert!(classroom::configure_program(&conn, &input, "2026-09-10").is_err());
    assert_eq!(
        language::program_view(&conn, "german", "2026-09-10")
            .unwrap()
            .current_level,
        "B1"
    );
}

#[test]
fn diagnostic_acceptance_keeps_its_evidence_and_rejects_changed_setup() {
    use principia_desk_lib::domain::assessments::{Response, ResponseStatus};
    let (_, conn) = fixture();
    let options = enrollment::options("linux-bash").unwrap();
    let mut configuration = options.default_configuration;
    configuration.entry = EntryChoice::Diagnostic;
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
    let mut check = placement::start(&conn, &draft.id, draft.revision, false).unwrap();
    for question in &check.questions {
        check.revision = placement::save_response(
            &conn,
            &draft.id,
            &check.round_id,
            check.revision,
            &question.id,
            Response {
                answer: String::new(),
                status: ResponseStatus::Skipped,
            },
        )
        .unwrap();
    }
    placement::submit(&conn, &draft.id, &check.round_id, check.revision).unwrap();
    placement::finish(&conn, &draft.id, &check.round_id).unwrap();
    let recommendation = placement::recommend(&conn, &draft.id, draft.revision).unwrap();
    let input = AcceptPath {
        draft_id: draft.id.clone(),
        expected_revision: draft.revision,
        recommendation_id: recommendation.id,
    };
    let accepted = classes::accept(&conn, &input, "2026-09-09").unwrap();
    assert_eq!(
        accepted.recommendation.assessment_attempt_id,
        Some(check.attempt_id)
    );
    assert_eq!(accepted.recommendation.entry_point, "foundations");
    assert_eq!(count(&conn, "assessment_attempts"), 1);
    assert_eq!(count(&conn, "mastery"), 0);
    let next = setup(&conn, "linux-bash", "production");
    let mut draft = enrollment::draft(&conn, &next.draft_id).unwrap();
    draft.configuration.goal = LearningGoal::CourseOutcome {
        note: "A changed goal".into(),
    };
    enrollment::save_draft(
        &conn,
        &SaveEnrollmentDraft {
            id: Some(draft.id),
            expected_revision: Some(draft.revision),
            course: draft.course,
            configuration: draft.configuration,
        },
    )
    .unwrap();
    assert!(classes::accept(&conn, &next, "2026-09-10").is_err());
    assert_eq!(count(&conn, "path_revisions"), 1);
}

fn revise(
    conn: &Connection,
    course: &str,
    expected_revision: u32,
    change: PathChange,
) -> Result<classes::AcceptedPath, String> {
    classes::revise(
        conn,
        &RevisePath {
            course_id: course.into(),
            expected_revision,
            change,
        },
        "2026-09-10",
    )
    .map_err(|e| e.to_string())
}

#[test]
fn bypassing_and_including_topics_revise_the_route_without_credit() {
    let (_, conn) = fixture();
    let input = setup(&conn, "typescript", "foundations");
    let accepted = classes::accept(&conn, &input, "2026-09-09").unwrap();
    let first = classes::next_concept(&conn, "typescript", "2026-09-09")
        .unwrap()
        .unwrap();
    let core_total = classroom::curriculum_map(&conn, "typescript")
        .unwrap()
        .concepts
        .iter()
        .filter(|c| c.core)
        .count();
    let revised = revise(
        &conn,
        "typescript",
        accepted.revision,
        PathChange::Bypass {
            topics: vec![first.slug.clone()],
        },
    )
    .unwrap();
    assert_eq!(revised.revision, 2);
    assert_eq!(revised.reference.class_id, accepted.reference.class_id);
    assert!(revised
        .recommendation
        .bypassed
        .iter()
        .any(|t| t.id == first.slug));
    assert!(revised
        .recommendation
        .earlier_topics
        .iter()
        .any(|t| t.id == first.slug));
    assert_ne!(
        classes::next_concept(&conn, "typescript", "2026-09-10")
            .unwrap()
            .unwrap()
            .slug,
        first.slug
    );
    let map = classroom::curriculum_map(&conn, "typescript").unwrap();
    let entry = map.concepts.iter().find(|c| c.slug == first.slug).unwrap();
    assert_eq!(entry.path_status, "bypassed_by_choice");
    assert!(!entry.required);
    let coverage = map.path.as_ref().unwrap();
    assert_eq!(
        (
            coverage.revision,
            coverage.bypassed,
            coverage.required_done,
            coverage.coverage_done
        ),
        (2, 1, 0, 0)
    );
    assert_eq!(coverage.required_total + 1, core_total);
    assert_eq!(coverage.coverage_total, core_total);
    assert_eq!(count(&conn, "mastery"), 0);
    let included = revise(
        &conn,
        "typescript",
        2,
        PathChange::Include {
            topics: vec![first.slug.clone()],
        },
    )
    .unwrap();
    assert_eq!(included.revision, 3);
    assert!(
        included.recommendation.bypassed.is_empty()
            && included.recommendation.earlier_topics.is_empty()
    );
    let map = classroom::curriculum_map(&conn, "typescript").unwrap();
    let entry = map.concepts.iter().find(|c| c.slug == first.slug).unwrap();
    assert_eq!(
        (entry.path_status.as_str(), entry.required),
        ("upcoming", true)
    );
    assert_eq!(map.path.as_ref().unwrap().required_total, core_total);
    assert_eq!(count(&conn, "path_revisions"), 3);
    // Stale base, replayed change, and a change that would alter nothing.
    assert!(revise(
        &conn,
        "typescript",
        1,
        PathChange::Bypass {
            topics: vec![first.slug.clone()]
        }
    )
    .unwrap_err()
    .contains("path changed"));
    let replay = revise(
        &conn,
        "typescript",
        2,
        PathChange::Include {
            topics: vec![first.slug.clone()],
        },
    )
    .unwrap();
    assert_eq!(replay.reference, included.reference);
    assert!(revise(
        &conn,
        "typescript",
        3,
        PathChange::Include {
            topics: vec![first.slug.clone()]
        }
    )
    .unwrap_err()
    .contains("as it is"));
    assert!(revise(
        &conn,
        "typescript",
        3,
        PathChange::Bypass {
            topics: vec!["not-a-topic".into()]
        }
    )
    .unwrap_err()
    .contains("not a topic"));
    assert_eq!(count(&conn, "path_revisions"), 3);
    assert_eq!(
        classes::current_path(&conn, "typescript")
            .unwrap()
            .unwrap()
            .reference,
        included.reference
    );
}

#[test]
fn completed_topics_cannot_be_bypassed_and_language_paths_are_not_revised_this_way() {
    let (_, conn) = fixture();
    let input = setup(&conn, "javascript", "foundations");
    let accepted = classes::accept(&conn, &input, "2026-09-09").unwrap();
    let concept = classes::next_concept(&conn, "javascript", "2026-09-09")
        .unwrap()
        .unwrap();
    conn.execute(
        "INSERT INTO mastery (concept_id, state, score_ema, encounters, last_seen_date, next_review_date, review_interval_days, teacher_notes, last_assessed_date) VALUES (?1,'mastered',0.9,3,'2026-09-09','2026-09-20',11,'','2026-09-09')",
        params![concept.id],
    )
    .unwrap();
    assert!(revise(
        &conn,
        "javascript",
        accepted.revision,
        PathChange::Bypass {
            topics: vec![concept.slug.clone()]
        }
    )
    .unwrap_err()
    .contains("already completed"));
    let map = classroom::curriculum_map(&conn, "javascript").unwrap();
    assert_eq!(
        map.concepts
            .iter()
            .find(|c| c.slug == concept.slug)
            .unwrap()
            .path_status,
        "completed_here"
    );
    assert_eq!(map.path.as_ref().unwrap().required_done, 1);
    assert!(revise(
        &conn,
        "typescript",
        1,
        PathChange::Bypass {
            topics: vec![concept.slug.clone()]
        }
    )
    .unwrap_err()
    .contains("Accept a learning path"));
    let german = setup(&conn, "german", "A1");
    let path = classes::accept(&conn, &german, "2026-09-09").unwrap();
    assert!(revise(
        &conn,
        "german",
        path.revision,
        PathChange::Bypass {
            topics: vec!["A1".into()]
        }
    )
    .unwrap_err()
    .contains("starting band"));
}

/// Walk a runtime session for `slug` to a completed result with `passed`.
fn completed_session(
    conn: &Connection,
    path: &classes::AcceptedPath,
    slug: &str,
    passed: bool,
) -> String {
    let now = chrono::Utc::now();
    let concept = db::all_concepts(conn, &path.recommendation.course.course_id)
        .unwrap()
        .into_iter()
        .find(|c| c.slug == slug)
        .unwrap();
    let session = sessions::plan(conn, &PlanSession {
        request_key: format!("request-{:032x}", rand::random::<u128>()),
        owner: PlanOwner::Class { path: path.reference.clone() },
        kind: SessionKind::Lesson,
        stages: vec![Stage::Learn, Stage::Check, Stage::Feedback],
        selection: serde_json::json!({"adapter":"engineering","concept_id":concept.id,"slug":slug,"title":concept.title,"category":concept.category,"slot_id":null,"service_date":"2026-09-10","revisit":false,"reason":"test"}),
    }, now).unwrap();
    let lease = sessions::claim_preparation(conn, &session.id, now, 60)
        .unwrap()
        .unwrap();
    sessions::publish_preparation(
        conn,
        &lease,
        &PreparedLesson {
            title: "Lesson".into(),
            body: serde_json::json!({"markdown":"body"}),
            provenance: serde_json::json!({"kind":"fixture"}),
        },
        now,
    )
    .unwrap();
    let session = sessions::get(conn, &session.id).unwrap();
    let session = sessions::activate(conn, &session.id, session.revision, now).unwrap();
    let current = sessions::get(conn, &session.id).unwrap().checkpoint;
    let checkpoint = sessions::save_checkpoint(
        conn,
        &session.id,
        current.revision,
        &sessions::CheckpointBody {
            stage: Stage::Feedback,
            reading: current.body.reading.clone(),
            work: current.body.work.clone(),
        },
        now,
    )
    .unwrap()
    .revision;
    sessions::finish(
        conn,
        &session.id,
        session.revision,
        checkpoint,
        Disposition::Completed,
        now,
        |_, _| Ok(serde_json::json!({"passed": passed, "score": if passed { 1.0 } else { 0.25 }})),
    )
    .unwrap();
    session.id.0
}

#[test]
fn a_failed_check_with_a_set_aside_prerequisite_proposes_a_bridge_served_first() {
    let (_, conn) = fixture();
    let input = setup(&conn, "typescript", "foundations");
    let accepted = classes::accept(&conn, &input, "2026-09-09").unwrap();
    // Planning needs an active class: give it a study time and enable it.
    classroom::upsert_slot(
        &conn,
        &classroom::UpsertClassroomSlotInput {
            id: None,
            subject_id: "typescript".into(),
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
        &classroom::ConfigureClassroomInput {
            subject_id: "typescript".into(),
            enabled: true,
            agent: "claude".into(),
            model: "sonnet".into(),
            custom_agent_bin: String::new(),
            session_minutes: 30,
            start_level: None,
            target_level: None,
            weekly_minutes: None,
        },
        "2026-09-09",
    )
    .unwrap();
    let map = classroom::curriculum_map(&conn, "typescript").unwrap();
    let dependent = map
        .concepts
        .iter()
        .find(|c| !c.prerequisites.is_empty())
        .unwrap()
        .clone();
    let prerequisite = dependent.prerequisites[0].clone();
    // Set the prerequisite aside, then fail the dependent topic's check.
    let revised = revise(
        &conn,
        "typescript",
        accepted.revision,
        PathChange::Bypass {
            topics: vec![prerequisite.clone()],
        },
    )
    .unwrap();
    assert!(classes::bridge_proposals(&conn, "typescript")
        .unwrap()
        .is_empty());
    let failed = completed_session(&conn, &revised, &dependent.slug, false);
    let proposals = classes::bridge_proposals(&conn, "typescript").unwrap();
    assert_eq!(proposals.len(), 1);
    assert_eq!(
        (
            proposals[0].topic.id.as_str(),
            proposals[0].before.id.as_str(),
            proposals[0].session_id.as_str()
        ),
        (
            prerequisite.as_str(),
            dependent.slug.as_str(),
            failed.as_str()
        )
    );
    assert_eq!(
        classroom::curriculum_map(&conn, "typescript")
            .unwrap()
            .bridge_proposals
            .len(),
        1
    );
    // Declining silences the pair; accepting records the bridge and serves it next.
    let declined = revise(
        &conn,
        "typescript",
        revised.revision,
        PathChange::DeclineBridge {
            topic: prerequisite.clone(),
            before: dependent.slug.clone(),
        },
    )
    .unwrap();
    assert!(classes::bridge_proposals(&conn, "typescript")
        .unwrap()
        .is_empty());
    let bridged = revise(
        &conn,
        "typescript",
        declined.revision,
        PathChange::AcceptBridge {
            topic: prerequisite.clone(),
            before: dependent.slug.clone(),
        },
    )
    .unwrap();
    assert!(bridged.recommendation.declined_bridges.is_empty());
    assert_eq!(bridged.recommendation.bridges[0].id, prerequisite);
    let map = classroom::curriculum_map(&conn, "typescript").unwrap();
    let entry = map
        .concepts
        .iter()
        .find(|c| c.slug == prerequisite)
        .unwrap();
    assert_eq!(
        (
            entry.path_status.as_str(),
            entry.required,
            map.path.as_ref().unwrap().bridges
        ),
        ("bridge", true, 1)
    );
    assert_eq!(
        classes::next_concept(&conn, "typescript", "2026-09-11")
            .unwrap()
            .unwrap()
            .slug,
        prerequisite
    );
    // Once the bridge lesson completes, regular selection resumes and the bridge is not proposed again.
    completed_session(&conn, &bridged, &prerequisite, true);
    assert_ne!(
        classes::next_concept(&conn, "typescript", "2026-09-12")
            .unwrap()
            .unwrap()
            .slug,
        prerequisite
    );
    assert!(classes::bridge_proposals(&conn, "typescript")
        .unwrap()
        .is_empty());
    assert_eq!(count(&conn, "mastery"), 0);
    // A passed check proposes nothing; a completed prerequisite is never proposed.
    let other = map
        .concepts
        .iter()
        .find(|c| !c.prerequisites.is_empty() && c.slug != dependent.slug)
        .unwrap();
    completed_session(&conn, &bridged, &other.slug, true);
    assert!(classes::bridge_proposals(&conn, "typescript")
        .unwrap()
        .is_empty());
}

#[test]
fn the_overview_route_answers_completed_demonstrated_review_and_next_without_picking() {
    let (_, conn) = fixture();
    assert!(classroom::program_view(&conn, "typescript", "2026-09-09")
        .unwrap()
        .route
        .is_none());
    let input = setup(&conn, "typescript", "mechanisms");
    let accepted = classes::accept(&conn, &input, "2026-09-09").unwrap();
    let view = classroom::program_view(&conn, "typescript", "2026-09-09").unwrap();
    let route = view
        .route
        .clone()
        .expect("engineering class with a path has a route");
    assert_eq!(
        (route.revision, route.required_done, route.coverage_done),
        (1, 0, 0)
    );
    assert!(
        route.required_total < route.coverage_total,
        "earlier material leaves the route"
    );
    assert_eq!(route.needs_review, accepted.recommendation.refreshers.len());
    let next = route.next.clone().expect("something is next");
    assert_eq!(next.reason, "Next required topic on your accepted route.");
    // Peeking never records a pick; the real pick serves the same topic.
    let before: i64 = conn
        .query_row(
            "SELECT times_picked FROM concepts WHERE slug = ?1",
            [&next.slug],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(before, 0);
    assert_eq!(
        classroom::program_view(&conn, "typescript", "2026-09-09")
            .unwrap()
            .route
            .unwrap()
            .next
            .unwrap()
            .slug,
        next.slug
    );
    assert_eq!(
        classes::next_concept(&conn, "typescript", "2026-09-09")
            .unwrap()
            .unwrap()
            .slug,
        next.slug
    );
    let after: i64 = conn
        .query_row(
            "SELECT times_picked FROM concepts WHERE slug = ?1",
            [&next.slug],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(after, 1);
    // A bypass shrinks the required work; an accepted bridge is announced as next.
    let bypassed = revise(
        &conn,
        "typescript",
        accepted.revision,
        PathChange::Bypass {
            topics: vec![next.slug.clone()],
        },
    )
    .unwrap();
    let route = classroom::program_view(&conn, "typescript", "2026-09-10")
        .unwrap()
        .route
        .unwrap();
    assert_eq!(
        route.required_total + 1,
        view.route.as_ref().unwrap().required_total
    );
    assert_ne!(route.next.as_ref().unwrap().slug, next.slug);
    let map = classroom::curriculum_map(&conn, "typescript").unwrap();
    let dependent = map
        .concepts
        .iter()
        .find(|c| c.prerequisites.contains(&next.slug))
        .map(|c| c.slug.clone())
        .unwrap_or_else(|| map.concepts[0].slug.clone());
    let bridged = revise(
        &conn,
        "typescript",
        bypassed.revision,
        PathChange::AcceptBridge {
            topic: next.slug.clone(),
            before: dependent,
        },
    )
    .unwrap();
    let route = classroom::program_view(&conn, "typescript", "2026-09-10")
        .unwrap()
        .route
        .unwrap();
    assert_eq!(route.revision, bridged.revision);
    assert_eq!(route.next.as_ref().unwrap().slug, next.slug);
    assert!(route
        .next
        .as_ref()
        .unwrap()
        .reason
        .starts_with("Bridge lesson"));
    assert_eq!(
        route.needs_review,
        accepted.recommendation.refreshers.len() + 1
    );
    assert!(classroom::program_view(&conn, "german", "2026-09-10")
        .unwrap()
        .route
        .is_none());
}

#[test]
fn a_changed_curriculum_marks_the_route_stale_instead_of_failing_the_program_view() {
    let (_, conn) = fixture();
    let input = setup(&conn, "typescript", "foundations");
    let accepted = classes::accept(&conn, &input, "2026-09-09").unwrap();
    // Simulate a curriculum update: a later revision (rows are immutable) carries
    // an older snapshot fingerprint than the bundled curriculum now has.
    conn.execute(
        "INSERT INTO course_snapshots(fingerprint, course_id, version, body_json, created_at) VALUES ('0000000000000000000000000000000000000000000000000000000000000000', 'typescript', 'v0', '{}', '2026-09-01T00:00:00Z')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO path_revisions(id, class_id, revision, course_snapshot_fingerprint, entry_profile_json, plan_json, accepted_at) SELECT 'path-stale', class_id, revision + 1, '0000000000000000000000000000000000000000000000000000000000000000', entry_profile_json, plan_json, accepted_at FROM path_revisions WHERE id = ?1",
        [&accepted.reference.path_revision_id],
    )
    .unwrap();
    conn.execute(
        "UPDATE classes SET active_path_revision_id = 'path-stale' WHERE id = ?1",
        [&accepted.reference.class_id],
    )
    .unwrap();
    assert!(classes::next_concept(&conn, "typescript", "2026-09-10")
        .unwrap_err()
        .to_string()
        .contains("curriculum changed"));
    let view = classroom::program_view(&conn, "typescript", "2026-09-10").unwrap();
    let route = view
        .route
        .expect("route summary survives a changed curriculum");
    assert!(route.stale);
    assert!(route.next.is_none());
    assert!(classroom::program_views(&conn, "2026-09-10").is_ok());
}
