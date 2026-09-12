use principia_desk_lib::{
    catalog, db,
    domain::{
        assessments::{Response, ResponseStatus},
        enrollment::{self, EnrollmentDraft, EntryChoice, SaveEnrollmentDraft},
        placement::{self, DiagnosticView, Verdict},
    },
};
use rusqlite::Connection;
fn fixture(course: &str) -> (std::path::PathBuf, Connection, EnrollmentDraft) {
    let path =
        std::env::temp_dir().join(format!("principia-placement-{}.db", rand::random::<u64>()));
    let conn = db::open(&path).unwrap();
    let options = enrollment::options(course).unwrap();
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
    (path, conn, draft)
}
fn answer(
    conn: &Connection,
    draft: &EnrollmentDraft,
    mut view: DiagnosticView,
    wrong: &[usize],
    skip: bool,
) -> DiagnosticView {
    let bank = placement::bank(&draft.course.course_id).unwrap();
    for (index, q) in view.questions.iter().enumerate() {
        let key = bank.questions.iter().find(|x| x.id == q.id).unwrap();
        let response = if skip {
            Response {
                answer: String::new(),
                status: ResponseStatus::Skipped,
            }
        } else {
            Response {
                answer: if wrong.contains(&index) {
                    key.choices
                        .iter()
                        .find(|c| c.id != key.answer)
                        .unwrap()
                        .id
                        .clone()
                } else {
                    key.answer.clone()
                },
                status: ResponseStatus::Answered,
            }
        };
        view.revision = placement::save_response(
            conn,
            &draft.id,
            &view.round_id,
            view.revision,
            &q.id,
            response,
        )
        .unwrap();
    }
    placement::submit(conn, &draft.id, &view.round_id, view.revision).unwrap()
}
fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}
#[test]
fn all_nine_banks_and_command_flows_work_without_lessons_focus_or_mastery() {
    for course in catalog::COURSES {
        let (_, conn, draft) = fixture(course.course_id);
        db::set_config(&conn, "kiosk_level", "advisory").unwrap();
        let view = placement::start(&conn, &draft.id, draft.revision, false).unwrap();
        let wire = serde_json::to_value(&view).unwrap();
        assert!(wire["questions"][0].get("answer").is_none());
        assert!(view.matches_draft);
        let sampled_stages = if course.kind == catalog::SubjectKind::Language {
            2 // the fixture's default goal is A2, so B1 and B2 are not sampled
        } else {
            4
        };
        assert_eq!(
            view.questions.len(),
            sampled_stages * placement::CRITERIA_PER_ENTRY_POINT
        );
        let done = answer(&conn, &draft, view, &[], false);
        assert!(done.criteria.iter().all(|r| r.verdict == Verdict::Passed));
        assert!(
            placement::recommend(&conn, &draft.id, draft.revision).is_err(),
            "must finish optional follow-up decision"
        );
        placement::finish(&conn, &draft.id, &done.round_id).unwrap();
        let recommendation = placement::recommend(&conn, &draft.id, draft.revision).unwrap();
        assert!(!recommendation.required_outcome.is_empty());
        // Every stage was sampled, so a full pass reaches the last one and
        // nothing is left with the excuse of never having been measured.
        assert_eq!(
            recommendation.criteria.len(),
            sampled_stages * placement::CRITERIA_PER_ENTRY_POINT,
            "the check samples every stage it can place you in"
        );
        assert_eq!(
            recommendation.entry_point,
            if course.kind == catalog::SubjectKind::Language {
                "A2"
            } else {
                "synthesis"
            }
        );
        for table in [
            "classes",
            "path_revisions",
            "sessions",
            "classroom_sessions",
            "classroom_schedule_slots",
            "attempts",
            "mastery",
        ] {
            assert_eq!(count(&conn, table), 0, "{} wrote {table}", course.id);
        }
        assert_eq!(
            db::get_config(&conn, "kiosk_level").unwrap().as_deref(),
            Some("advisory")
        );
    }
}
#[test]
fn interrupted_check_keeps_answers_and_frozen_definition_and_rejects_wrong_owner() {
    let (path, conn, draft) = fixture("linux-bash");
    let view = placement::start(&conn, &draft.id, draft.revision, false).unwrap();
    let question = &view.questions[0];
    let response = Response {
        answer: question.choices[1].id.clone(),
        status: ResponseStatus::Draft,
    };
    assert_eq!(
        placement::save_response(
            &conn,
            &draft.id,
            &view.round_id,
            0,
            &question.id,
            response.clone()
        )
        .unwrap(),
        1
    );
    assert_eq!(
        placement::save_response(
            &conn,
            &draft.id,
            &view.round_id,
            0,
            &question.id,
            response.clone()
        )
        .unwrap(),
        1
    );
    assert!(placement::save_response(
        &conn,
        &draft.id,
        &view.round_id,
        0,
        &view.questions[1].id,
        response.clone()
    )
    .is_err());
    assert!(placement::submit(&conn, &draft.id, &view.round_id, 1).is_err());
    let other_options = enrollment::options("bash-scripting").unwrap();
    let other = enrollment::save_draft(
        &conn,
        &SaveEnrollmentDraft {
            id: None,
            expected_revision: None,
            course: other_options.course,
            configuration: other_options.default_configuration,
        },
    )
    .unwrap();
    assert!(placement::save_response(
        &conn,
        &other.id,
        &view.round_id,
        1,
        &question.id,
        response.clone()
    )
    .is_err());
    drop(conn);
    let conn = db::open(&path).unwrap();
    let resumed = placement::start(&conn, &draft.id, draft.revision, false).unwrap();
    assert_eq!(resumed.round_id, view.round_id);
    assert_eq!(resumed.responses[&question.id], response);
    assert_eq!(
        serde_json::to_value(resumed.questions).unwrap(),
        serde_json::to_value(view.questions).unwrap()
    );
    assert_eq!(count(&conn, "assessment_attempts"), 1);
}
#[test]
fn skipped_checks_remain_unknown_and_uneven_profiles_get_specific_bridges() {
    let (_, conn, draft) = fixture("bash-scripting");
    let first = placement::start(&conn, &draft.id, draft.revision, false).unwrap();
    let skipped = answer(&conn, &draft, first, &[], true);
    assert!(skipped
        .criteria
        .iter()
        .all(|r| r.verdict == Verdict::Unknown));
    placement::finish(&conn, &draft.id, &skipped.round_id).unwrap();
    assert_eq!(
        placement::recommend(&conn, &draft.id, draft.revision)
            .unwrap()
            .entry_point,
        "foundations"
    );
    // An uneven profile: foundations demonstrated, one mechanisms sample
    // missed. The route stops there rather than reading the later stage as
    // permission to skip the gap.
    let next = placement::start(&conn, &draft.id, draft.revision, true).unwrap();
    assert_ne!(next.attempt_id, skipped.attempt_id);
    let done = answer(&conn, &draft, next, &[4], false);
    placement::finish(&conn, &draft.id, &done.round_id).unwrap();
    let path = placement::recommend(&conn, &draft.id, draft.revision).unwrap();
    assert_eq!(path.entry_point, "mechanisms");
    assert!(
        path.refreshers.iter().any(|r| r.id == "bs-loops"),
        "an earlier prerequisite that upcoming work needs and the check did not sample is queued as a refresher"
    );
    assert!(
        !path.refreshers.iter().any(|r| r.id == "bs-arguments"),
        "a demonstrated sample is not queued again"
    );
    // A stage the learner never reached is not silently skipped.
    let third = placement::start(&conn, &draft.id, draft.revision, true).unwrap();
    let all_right = answer(&conn, &draft, third, &[], false);
    placement::finish(&conn, &draft.id, &all_right.round_id).unwrap();
    assert_eq!(
        placement::recommend(&conn, &draft.id, draft.revision)
            .unwrap()
            .entry_point,
        "synthesis",
        "every stage demonstrated reaches the last one"
    );
    assert_eq!(count(&conn, "assessment_attempts"), 3);
    assert_eq!(count(&conn, "assessment_submissions"), 3);
}
#[test]
fn followups_are_bounded_idempotent_and_retain_original_criterion_evidence() {
    let (_, conn, draft) = fixture("linux-bash");
    let first = placement::start(&conn, &draft.id, draft.revision, false).unwrap();
    let done = answer(&conn, &draft, first, &[0, 1], false);
    assert!(done.can_follow_up);
    let followup = placement::follow_up(&conn, &draft.id, &done.round_id).unwrap();
    assert_eq!(followup.questions.len(), 2);
    assert_eq!(
        placement::follow_up(&conn, &draft.id, &done.round_id)
            .unwrap()
            .round_id,
        followup.round_id
    );
    assert!(placement::finish(&conn, &draft.id, &followup.round_id).is_err());
    let final_view = answer(&conn, &draft, followup, &[], false);
    assert!(final_view.completed);
    assert!(!final_view.can_follow_up);
    assert_eq!(final_view.criteria[0].evidence.len(), 2);
    assert_eq!(
        final_view.criteria[0].evidence[0].verdict,
        Verdict::NeedsPractice
    );
    assert_eq!(final_view.criteria[0].verdict, Verdict::Passed);
    placement::submit(&conn, &draft.id, &final_view.round_id, 0).unwrap();
    assert_eq!(count(&conn, "assessment_rounds"), 2);
    assert_eq!(count(&conn, "assessment_submissions"), 2);
}
#[test]
fn changed_goal_invalidates_recommendation_without_rewriting_the_old_check() {
    let (_, conn, draft) = fixture("german");
    let first = placement::start(&conn, &draft.id, draft.revision, false).unwrap();
    assert_eq!(
        first.questions.len(),
        2 * placement::CRITERIA_PER_ENTRY_POINT,
        "an A2 goal samples A1 and A2 and leaves B1 and B2 alone"
    );
    let done = answer(&conn, &draft, first, &[], false);
    placement::finish(&conn, &draft.id, &done.round_id).unwrap();
    let original = placement::recommend(&conn, &draft.id, draft.revision).unwrap();
    let mut configuration = draft.configuration.clone();
    configuration.goal = enrollment::LearningGoal::LanguageLevel {
        target_level: "B1".into(),
        note: String::new(),
    };
    let edited = enrollment::save_draft(
        &conn,
        &SaveEnrollmentDraft {
            id: Some(draft.id.clone()),
            expected_revision: Some(draft.revision),
            course: draft.course.clone(),
            configuration,
        },
    )
    .unwrap();
    assert!(placement::recommend(&conn, &draft.id, draft.revision).is_err());
    assert!(placement::recommend(&conn, &edited.id, edited.revision).is_err());
    assert!(
        !placement::get(&conn, &edited.id)
            .unwrap()
            .unwrap()
            .matches_draft
    );
    let fresh = placement::start(&conn, &edited.id, edited.revision, true).unwrap();
    assert_eq!(
        fresh.questions.len(),
        3 * placement::CRITERIA_PER_ENTRY_POINT
    );
    assert_ne!(Some(fresh.attempt_id), original.assessment_attempt_id);
    assert_eq!(count(&conn, "assessment_submissions"), 1);
}
#[test]
fn manual_and_foundations_previews_are_stable_and_have_no_assessed_credit() {
    let (_, conn, draft) = fixture("bash-scripting");
    let mut config = draft.configuration.clone();
    config.entry = EntryChoice::Manual {
        entry_point: "mechanisms".into(),
        familiar_competencies: vec!["bs-arguments".into()],
    };
    let manual = enrollment::save_draft(
        &conn,
        &SaveEnrollmentDraft {
            id: Some(draft.id.clone()),
            expected_revision: Some(draft.revision),
            course: draft.course.clone(),
            configuration: config,
        },
    )
    .unwrap();
    let first = placement::recommend(&conn, &manual.id, manual.revision).unwrap();
    assert_eq!(first.entry_point, "mechanisms");
    assert!(first.criteria.is_empty() && first.assessment_attempt_id.is_none());
    assert!(first
        .earlier_topics
        .iter()
        .any(|r| r.id == "bs-arguments" && r.reason.starts_with("Declared")));
    assert_eq!(
        first.id,
        placement::recommend(&conn, &manual.id, manual.revision)
            .unwrap()
            .id
    );
    assert_eq!(count(&conn, "assessment_attempts"), 0);
    assert_eq!(count(&conn, "classes"), 0);
    let mut config = manual.configuration.clone();
    config.entry = EntryChoice::Foundations;
    let foundation = enrollment::save_draft(
        &conn,
        &SaveEnrollmentDraft {
            id: Some(manual.id),
            expected_revision: Some(manual.revision),
            course: manual.course,
            configuration: config,
        },
    )
    .unwrap();
    let view = placement::recommend(&conn, &foundation.id, foundation.revision).unwrap();
    assert_eq!(view.entry_point, "foundations");
    assert!(view.earlier_topics.is_empty());
}
