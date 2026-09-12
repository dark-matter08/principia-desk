//! Unit challenges check a learner out of demonstrated topics only.
use principia_desk_lib::{
    classroom, db,
    domain::{
        assessments::{Response, ResponseStatus},
        challenges,
        classes::{self, AcceptPath},
        enrollment::{self, SaveEnrollmentDraft},
        placement,
    },
    language,
};
use rusqlite::Connection;

fn fixture() -> Connection {
    let path = std::env::temp_dir().join(format!(
        "principia-challenge-{:032x}.db",
        rand::random::<u128>()
    ));
    let conn = db::open(&path).unwrap();
    db::seed_concepts(&conn, include_str!("../seed/concepts.json")).unwrap();
    language::initialize(&conn, "2026-09-09").unwrap();
    classroom::initialize(&conn).unwrap();
    conn
}
fn accept(conn: &Connection, course: &str) -> classes::AcceptedPath {
    let options = enrollment::options(course).unwrap();
    let draft = enrollment::save_draft(
        conn,
        &SaveEnrollmentDraft {
            id: None,
            expected_revision: None,
            course: options.course,
            configuration: options.default_configuration,
        },
    )
    .unwrap();
    let recommendation = placement::recommend(conn, &draft.id, draft.revision).unwrap();
    classes::accept(
        conn,
        &AcceptPath {
            draft_id: draft.id,
            expected_revision: draft.revision,
            recommendation_id: recommendation.id,
        },
        "2026-09-09",
    )
    .unwrap()
}
fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}
fn answered(answer: &str) -> Response {
    Response {
        answer: answer.into(),
        status: ResponseStatus::Answered,
    }
}

#[test]
fn a_challenge_checks_out_demonstrated_samples_only_without_credit_or_sessions() {
    let conn = fixture();
    let accepted = accept(&conn, "typescript");
    let before = classroom::curriculum_map(&conn, "typescript").unwrap();
    let challenge = challenges::start(&conn, "typescript", "foundations", false).unwrap();
    assert_eq!(
        (
            challenge.unit.as_str(),
            challenge.path_revision,
            challenge.questions.len(),
            challenge.submitted
        ),
        ("foundations", 1, 3, false)
    );
    assert_eq!(
        challenges::get(&conn, "typescript")
            .unwrap()
            .unwrap()
            .attempt_id,
        challenge.attempt_id
    );
    let bank = placement::bank("typescript").unwrap();
    let first = bank
        .questions
        .iter()
        .find(|q| q.id == challenge.questions[0].id)
        .unwrap();
    let revision = challenges::save_response(
        &conn,
        "typescript",
        &challenge.round_id,
        challenge.revision,
        &first.id,
        answered(&first.answer),
    )
    .unwrap();
    assert!(challenges::save_response(
        &conn,
        "typescript",
        &challenge.round_id,
        revision,
        &challenge.questions[1].id,
        answered("no-such-choice")
    )
    .is_err());
    let revision = challenges::save_response(
        &conn,
        "typescript",
        &challenge.round_id,
        revision,
        &challenge.questions[1].id,
        Response {
            answer: String::new(),
            status: ResponseStatus::Skipped,
        },
    )
    .unwrap();
    let revision = challenges::save_response(
        &conn,
        "typescript",
        &challenge.round_id,
        revision,
        &challenge.questions[2].id,
        Response {
            answer: String::new(),
            status: ResponseStatus::Skipped,
        },
    )
    .unwrap();
    assert!(challenges::apply(
        &conn,
        "typescript",
        &challenge.attempt_id.0,
        1,
        "2026-09-10"
    )
    .unwrap_err()
    .to_string()
    .contains("Submit the challenge"));
    assert!(challenges::submit(&conn, "typescript", &challenge.round_id, revision - 1).is_err());
    let submitted = challenges::submit(&conn, "typescript", &challenge.round_id, revision).unwrap();
    assert!(submitted.submitted);
    assert_eq!(
        submitted
            .demonstrated
            .iter()
            .map(|t| t.id.as_str())
            .collect::<Vec<_>>(),
        vec![first.competency.as_str()]
    );
    assert_eq!(submitted.needs_practice.len(), 2);
    assert_eq!(submitted.applied_revision, None);
    // Submission is immutable and re-submission returns the same view.
    assert_eq!(
        challenges::submit(&conn, "typescript", &challenge.round_id, revision)
            .unwrap()
            .criteria
            .len(),
        3
    );
    let path = challenges::apply(
        &conn,
        "typescript",
        &challenge.attempt_id.0,
        accepted.revision,
        "2026-09-10",
    )
    .unwrap();
    assert_eq!(path.revision, 2);
    assert_eq!(
        path.recommendation
            .checked
            .iter()
            .map(|t| t.id.as_str())
            .collect::<Vec<_>>(),
        vec![first.competency.as_str()]
    );
    assert!(path.recommendation.checked[0]
        .reason
        .contains(&challenge.attempt_id.0));
    let after = classroom::curriculum_map(&conn, "typescript").unwrap();
    let entry = after
        .concepts
        .iter()
        .find(|c| c.slug == first.competency)
        .unwrap();
    assert_eq!(
        (entry.path_status.as_str(), entry.required),
        ("prior_knowledge_checked", false)
    );
    assert_eq!(after.path.as_ref().unwrap().checked, 1);
    assert_eq!(
        after.path.as_ref().unwrap().required_total + usize::from(entry.core),
        before.path.as_ref().unwrap().required_total
    );
    assert_eq!(after.path.as_ref().unwrap().coverage_done, 0);
    assert_eq!(
        challenges::get(&conn, "typescript")
            .unwrap()
            .unwrap()
            .applied_revision,
        Some(2)
    );
    // Replays keep the same revision; the failed sample stays on the route.
    assert_eq!(
        challenges::apply(
            &conn,
            "typescript",
            &challenge.attempt_id.0,
            accepted.revision,
            "2026-09-10"
        )
        .unwrap()
        .revision,
        2
    );
    assert_eq!(count(&conn, "path_revisions"), 2);
    assert_eq!(
        after
            .concepts
            .iter()
            .find(|c| c.slug == submitted.needs_practice[0].id)
            .unwrap()
            .path_status,
        "upcoming"
    );
    assert_eq!(count(&conn, "mastery"), 0);
    assert_eq!(count(&conn, "study_sessions"), 0);
    assert_eq!(count(&conn, "schedule_occurrences"), 0);
    assert_ne!(
        classes::next_concept(&conn, "typescript", "2026-09-10")
            .unwrap()
            .unwrap()
            .slug,
        first.competency
    );
}

#[test]
fn one_open_challenge_per_class_and_only_authored_units() {
    let conn = fixture();
    accept(&conn, "typescript");
    assert!(challenges::start(&conn, "typescript", "elective", false)
        .unwrap_err()
        .to_string()
        .contains("not part of this course"));
    assert!(challenges::start(&conn, "javascript", "foundations", false)
        .unwrap_err()
        .to_string()
        .contains("Accept a learning path"));
    let open = challenges::start(&conn, "typescript", "foundations", false).unwrap();
    assert!(challenges::start(&conn, "typescript", "mechanisms", false)
        .unwrap_err()
        .to_string()
        .contains("Finish the open"));
    assert_eq!(
        challenges::start(&conn, "typescript", "foundations", true)
            .unwrap()
            .attempt_id,
        open.attempt_id
    );
    let bank = placement::bank("typescript").unwrap();
    let mut revision = open.revision;
    for question in &open.questions {
        let wrong = bank
            .questions
            .iter()
            .find(|q| q.id == question.id)
            .unwrap()
            .choices
            .iter()
            .find(|c| {
                c.id != bank
                    .questions
                    .iter()
                    .find(|q| q.id == question.id)
                    .unwrap()
                    .answer
            })
            .unwrap()
            .id
            .clone();
        revision = challenges::save_response(
            &conn,
            "typescript",
            &open.round_id,
            revision,
            &question.id,
            answered(&wrong),
        )
        .unwrap();
    }
    let failed = challenges::submit(&conn, "typescript", &open.round_id, revision).unwrap();
    assert!(failed.demonstrated.is_empty() && failed.needs_practice.len() == 3);
    assert!(
        challenges::apply(&conn, "typescript", &failed.attempt_id.0, 1, "2026-09-10")
            .unwrap_err()
            .to_string()
            .contains("No sample was demonstrated")
    );
    let next = challenges::start(&conn, "typescript", "mechanisms", false).unwrap();
    assert_ne!(next.attempt_id, open.attempt_id);
    assert_eq!(next.unit, "mechanisms");
    assert_eq!(count(&conn, "path_revisions"), 1);
    assert!(challenges::start(&conn, "german", "A1", false)
        .unwrap_err()
        .to_string()
        .contains("engineering"));
}
