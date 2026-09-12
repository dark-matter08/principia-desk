use principia_desk_lib::{
    db,
    domain::{
        assessments::{self, Item, Owner, Purpose, Response, ResponseStatus},
        enrollment, primary_quiz,
    },
};
use rusqlite::{params, Connection};
use serde_json::json;
use std::path::PathBuf;

fn path() -> PathBuf {
    std::env::temp_dir().join(format!("principia-assessment-{}.db", rand::random::<u64>()))
}
fn fixture() -> (PathBuf, Connection, Owner) {
    let path = path();
    let conn = db::open(&path).unwrap();
    let options = enrollment::options("linux-bash").unwrap();
    let draft = enrollment::save_draft(
        &conn,
        &enrollment::SaveEnrollmentDraft {
            id: None,
            expected_revision: None,
            course: options.course,
            configuration: options.default_configuration,
        },
    )
    .unwrap();
    (path, conn, Owner::EnrollmentDraft(draft.id.0))
}
fn items() -> Vec<Item> {
    vec![
        Item {
            id: "paths".into(),
            body: json!({"prompt":"Identify the absolute path","choices":["/tmp/lab","tmp/lab"],"answer":"/tmp/lab","coverage":["paths"]}),
        },
        Item {
            id: "streams".into(),
            body: json!({"prompt":"Explain stderr redirection","rubric":{"separate_streams":"Identifies fd 2"},"coverage":["redirection"]}),
        },
    ]
}
fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}
fn response(answer: &str, status: ResponseStatus) -> Response {
    Response {
        answer: answer.into(),
        status,
    }
}

#[test]
fn diagnostic_rounds_resume_unchanged_without_classes_schedules_or_credit() {
    let (path, conn, owner) = fixture();
    db::set_config(&conn, "kiosk_level", "hard").unwrap();
    let original = assessments::start(
        &conn,
        &owner,
        Purpose::Diagnostic,
        &json!({"curriculum":"frozen-v1"}),
        "shell-v1",
        &items(),
    )
    .unwrap();
    let saved = assessments::save_response(
        &conn,
        &owner,
        &original.id,
        0,
        "streams",
        response("stderr is fd 2; still drafting", ResponseStatus::Draft),
    )
    .unwrap();
    drop(conn);
    let conn = db::open(&path).unwrap();
    let restored = assessments::start(
        &conn,
        &owner,
        Purpose::Diagnostic,
        &json!({"curriculum":"frozen-v1"}),
        "shell-v1",
        &items(),
    )
    .unwrap();
    assert_eq!(restored, saved);
    let mut changed = items();
    changed[0].body["answer"] = json!("rewritten answer");
    assert!(assessments::start(
        &conn,
        &owner,
        Purpose::Diagnostic,
        &json!({"curriculum":"v2"}),
        "shell-v2",
        &changed
    )
    .is_err());
    for table in [
        "classes",
        "sessions",
        "classroom_sessions",
        "classroom_schedule_slots",
        "attempts",
        "mastery",
    ] {
        assert_eq!(count(&conn, table), 0, "diagnostic changed {table}");
    }
    assert_eq!(
        db::get_config(&conn, "kiosk_level").unwrap().as_deref(),
        Some("hard")
    );
}

#[test]
fn answers_reject_other_owners_unknown_items_and_stale_revisions_but_allow_lost_response_retries() {
    let (_, conn, owner) = fixture();
    let round = assessments::start(
        &conn,
        &owner,
        Purpose::Diagnostic,
        &json!({}),
        "v1",
        &items(),
    )
    .unwrap();
    let answer = response("/tmp/lab", ResponseStatus::Answered);
    let saved =
        assessments::save_response(&conn, &owner, &round.id, 0, "paths", answer.clone()).unwrap();
    assert_eq!(
        assessments::save_response(&conn, &owner, &round.id, 0, "paths", answer).unwrap(),
        saved
    );
    assert!(assessments::save_response(
        &conn,
        &owner,
        &round.id,
        0,
        "streams",
        response("fd 2", ResponseStatus::Draft)
    )
    .is_err());
    assert!(assessments::save_response(
        &conn,
        &Owner::Class("another".into()),
        &round.id,
        1,
        "streams",
        response("fd 2", ResponseStatus::Draft)
    )
    .is_err());
    assert!(assessments::save_response(
        &conn,
        &owner,
        &round.id,
        1,
        "unknown",
        response("text", ResponseStatus::Answered)
    )
    .is_err());
    assert_eq!(assessments::round(&conn, &round.id).unwrap(), saved);
}

#[test]
fn a_saved_draft_is_not_a_submitted_answer_and_grading_cannot_overwrite_newer_work() {
    let (_, conn, owner) = fixture();
    let first = assessments::start(
        &conn,
        &owner,
        Purpose::Diagnostic,
        &json!({}),
        "v1",
        &items(),
    )
    .unwrap();
    let first = assessments::save_response(
        &conn,
        &owner,
        &first.id,
        0,
        "paths",
        response("/tmp/lab", ResponseStatus::Draft),
    )
    .unwrap();
    {
        let tx = conn.unchecked_transaction().unwrap();
        assert!(
            assessments::submit_in_transaction(&tx, &owner, &first, &json!({"score":1}), true)
                .is_err()
        );
    }
    let changed = assessments::save_response(
        &conn,
        &owner,
        &first.id,
        first.revision,
        "paths",
        response("tmp/lab", ResponseStatus::Answered),
    )
    .unwrap();
    {
        let tx = conn.unchecked_transaction().unwrap();
        assert!(
            assessments::submit_in_transaction(&tx, &owner, &first, &json!({"score":1}), true)
                .unwrap_err()
                .to_string()
                .contains("changed")
        );
    }
    assert_eq!(assessments::round(&conn, &first.id).unwrap(), changed);
    assert_eq!(count(&conn, "assessment_submissions"), 0);
}

#[test]
fn diagnostic_followups_preserve_skipped_unknowns_and_terminal_rounds_are_immutable() {
    let (_, conn, owner) = fixture();
    let first = assessments::start(
        &conn,
        &owner,
        Purpose::Diagnostic,
        &json!({}),
        "v1",
        &items(),
    )
    .unwrap();
    let first = assessments::save_response(
        &conn,
        &owner,
        &first.id,
        0,
        "paths",
        response("/tmp/lab", ResponseStatus::Answered),
    )
    .unwrap();
    let first = assessments::save_response(
        &conn,
        &owner,
        &first.id,
        first.revision,
        "streams",
        response("", ResponseStatus::Skipped),
    )
    .unwrap();
    let result = json!({"covered":["paths"],"unknown":["redirection"]});
    assert!(assessments::submit_in_transaction(&conn, &owner, &first, &result, false).is_err());
    assert!(assessments::append_round(&conn, &first.attempt_id, "v1", &items()).is_err());
    let tx = conn.unchecked_transaction().unwrap();
    let submitted =
        assessments::submit_in_transaction(&tx, &owner, &first, &result, false).unwrap();
    tx.commit().unwrap();
    let tx = conn.unchecked_transaction().unwrap();
    assert_eq!(
        assessments::submit_in_transaction(&tx, &owner, &first, &json!({"different":true}), false)
            .unwrap(),
        submitted
    );
    tx.commit().unwrap();
    let next = assessments::append_round(
        &conn,
        &first.attempt_id,
        "v1-followup",
        &[items()[1].clone()],
    )
    .unwrap();
    assert_eq!(next.ordinal, 2);
    assert_ne!(next.id, first.id);
    assert!(assessments::save_response(
        &conn,
        &owner,
        &first.id,
        first.revision,
        "streams",
        response("late", ResponseStatus::Answered)
    )
    .is_err());
    assert!(conn
        .execute(
            "UPDATE assessment_submissions SET result_json = '{}' WHERE round_id = ?1",
            [&first.id.0]
        )
        .is_err());
    assert!(conn
        .execute(
            "UPDATE assessment_rounds SET items_json = '[]' WHERE id = ?1",
            [&first.id.0]
        )
        .is_err());
    assert_eq!(count(&conn, "mastery"), 0);
}

#[test]
fn late_submission_storage_failure_rolls_back_the_joined_transaction() {
    let (_, conn, owner) = fixture();
    let round =
        assessments::start(&conn, &owner, Purpose::Diagnostic, &json!({}), "v1", &[]).unwrap();
    conn.execute_batch("CREATE TRIGGER fail_final_assessment BEFORE UPDATE OF status ON assessment_attempts BEGIN SELECT RAISE(ABORT, 'injected final write failure'); END;").unwrap();
    {
        let tx = conn.unchecked_transaction().unwrap();
        assert!(assessments::submit_in_transaction(
            &tx,
            &owner,
            &round,
            &json!({"unknown":true}),
            true
        )
        .is_err());
    }
    assert_eq!(count(&conn, "assessment_submissions"), 0);
    assert_eq!(
        assessments::latest(&conn, &owner, Purpose::Diagnostic)
            .unwrap()
            .unwrap()
            .status,
        "active"
    );
}

#[test]
fn upgrade_preserves_and_imports_the_original_frozen_question_and_pending_answer_once() {
    let path = path();
    let old = Connection::open(&path).unwrap();
    old.execute_batch(include_str!("fixtures/upgrades/original-main.sql"))
        .unwrap();
    old.execute_batch(include_str!("fixtures/upgrades/common-records.sql"))
        .unwrap();
    let question = db::Question {
        id: 601,
        course_id: 501,
        prompt: "Frozen prompt — Grüße".into(),
        kind: "mcq".into(),
        choices_json: Some("[\"A\",\"B\"]".into()),
        correct_answer: "B".into(),
        explanation: "Frozen explanation".into(),
        origin: "carryover".into(),
    };
    let raw = serde_json::to_string(&vec![question]).unwrap();
    old.execute(
        "UPDATE config SET value = ?1 WHERE key = 'quiz_round:2026-09-08'",
        [&raw],
    )
    .unwrap();
    old.execute(
        "INSERT INTO config VALUES ('pending_answers:2026-09-08', ?1)",
        ["{\"601\":\"A\"}"],
    )
    .unwrap();
    drop(old);
    let conn = db::open(&path).unwrap();
    let frozen = primary_quiz::frozen_questions(&conn, "2026-09-08")
        .unwrap()
        .unwrap();
    assert_eq!(frozen[0].prompt, "Frozen prompt — Grüße");
    let round = primary_quiz::current(&conn, "2026-09-08").unwrap().unwrap();
    assert_eq!(round.responses["601"].answer, "A");
    assert_eq!(round.responses["601"].status, ResponseStatus::Answered);
    conn.execute(
        "UPDATE questions SET prompt = 'later edit' WHERE id = 601",
        [],
    )
    .unwrap();
    drop(conn);
    let conn = db::open(&path).unwrap();
    assert_eq!(
        primary_quiz::current(&conn, "2026-09-08").unwrap().unwrap(),
        round
    );
    assert_eq!(
        primary_quiz::frozen_questions(&conn, "2026-09-08")
            .unwrap()
            .unwrap()[0]
            .prompt,
        "Frozen prompt — Grüße"
    );
    assert_eq!(count(&conn, "assessment_attempts"), 1);
    assert_eq!(count(&conn, "attempts"), 1);
    assert!(db::set_config(&conn, "pending_answers:2026-09-08", "{}").is_err());
    assert!(db::set_config(&conn, "quiz_round:2026-09-09", "[]").is_err());
    assert!(conn
        .execute("DELETE FROM config WHERE key = 'quiz_round:2026-09-08'", [])
        .is_err());
    db::set_config(&conn, "unrelated_setting", "still writable").unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT value FROM legacy_assessment_config WHERE key = ?1",
            params!["quiz_round:2026-09-08"],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        raw
    );
}
