use principia_desk_lib::{
    catalog, db,
    domain::enrollment::{self, EntryChoice, LearningGoal, SaveEnrollmentDraft},
};
use rusqlite::{params, Connection};

fn database() -> (std::path::PathBuf, Connection) {
    let path = std::env::temp_dir().join(format!(
        "principia-enrollment-{}-{:032x}.db",
        std::process::id(),
        rand::random::<u128>()
    ));
    let conn = db::open(&path).unwrap();
    (path, conn)
}
fn input(course_id: &str) -> SaveEnrollmentDraft {
    let options = enrollment::options(course_id).unwrap();
    SaveEnrollmentDraft {
        id: None,
        expected_revision: None,
        course: options.course,
        configuration: options.default_configuration,
    }
}
fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
        row.get(0)
    })
    .unwrap()
}

#[test]
fn native_curriculum_fingerprints_match_every_generated_browser_reference() {
    let generated = include_str!("../../src/lib/catalog.generated.ts");
    for course in catalog::COURSES {
        let (reference, snapshot) = enrollment::course_snapshot(course.course_id).unwrap();
        assert!(
            generated.contains(&format!(
                "\"{}\": \"{}\"",
                reference.course_id, reference.fingerprint
            )),
            "{} fingerprint differs across native and browser",
            course.id
        );
        assert_eq!(snapshot["course"]["course_id"], course.course_id);
        assert_eq!(reference.fingerprint.len(), 64);
    }
}

#[test]
fn all_nine_courses_persist_entry_preferences_without_creating_learning_credit() {
    let (path, conn) = database();
    db::set_config(&conn, "kiosk_level", "hard").unwrap();
    let mut saved = Vec::new();
    for course in catalog::COURSES {
        let mut request = input(course.course_id);
        let options = enrollment::options(course.course_id).unwrap();
        request.configuration.entry = EntryChoice::Manual {
            entry_point: options.entry_points[1].id.clone(),
            familiar_competencies: vec![options.familiarity_options[0].id.clone()],
        };
        request.configuration.tutor.provider = "deepseek".into();
        request.configuration.tutor.model = "provider-specific-model".into();
        saved.push(enrollment::save_draft(&conn, &request).unwrap());
    }
    for table in [
        "classes",
        "path_revisions",
        "sessions",
        "classroom_sessions",
        "language_sessions",
        "mastery",
        "attempts",
        "classroom_schedule_slots",
        "language_schedule_slots",
    ] {
        assert_eq!(count(&conn, table), 0, "drafts changed {table}");
    }
    assert_eq!(
        db::get_config(&conn, "kiosk_level").unwrap().as_deref(),
        Some("hard")
    );
    drop(conn);
    let conn = db::open(&path).unwrap();
    for draft in saved {
        assert_eq!(
            enrollment::draft_for_course(&conn, &draft.course.course_id).unwrap(),
            Some(draft)
        );
    }
}

#[test]
fn foundations_diagnostic_and_manual_routes_are_choices_not_assessment_results() {
    let (_, conn) = database();
    for (course, route) in [
        ("system-design", EntryChoice::Foundations),
        ("linux-bash", EntryChoice::Diagnostic),
        (
            "german",
            EntryChoice::Manual {
                entry_point: "B1".into(),
                familiar_competencies: vec!["reading".into()],
            },
        ),
    ] {
        let mut request = input(course);
        request.configuration.entry = route.clone();
        if course == "german" {
            request.configuration.goal = LearningGoal::LanguageLevel {
                target_level: "B2".into(),
                note: "Read comfortably; assess listening separately".into(),
            };
        }
        let draft = enrollment::save_draft(&conn, &request).unwrap();
        assert_eq!(draft.configuration.entry, route);
        assert_eq!(draft.accepted_class_id, None);
    }
    for table in ["classes", "mastery", "language_skill_scores", "attempts"] {
        assert_eq!(count(&conn, table), 0);
    }
}

#[test]
fn retries_do_not_duplicate_drafts_and_stale_writes_cannot_replace_newer_choices() {
    let (_, conn) = database();
    let request = input("bash-scripting");
    let first = enrollment::save_draft(&conn, &request).unwrap();
    assert_eq!(enrollment::save_draft(&conn, &request).unwrap(), first);
    let mut update = request.clone();
    update.id = Some(first.id.clone());
    update.expected_revision = Some(first.revision);
    update.configuration.entry = EntryChoice::Manual {
        entry_point: "production".into(),
        familiar_competencies: vec!["bs-functions".into(), "bs-arrays".into()],
    };
    let second = enrollment::save_draft(&conn, &update).unwrap();
    assert_eq!(second.revision, 2);
    assert_eq!(enrollment::save_draft(&conn, &update).unwrap(), second);
    update.configuration.pace.session_minutes = 45;
    assert!(enrollment::save_draft(&conn, &update).is_err());
    assert_eq!(
        enrollment::draft_for_course(&conn, "bash-scripting").unwrap(),
        Some(second)
    );
    assert_eq!(count(&conn, "enrollment_drafts"), 1);
}

#[test]
fn wrong_versions_foreign_competencies_and_inverted_goals_are_rejected_before_writes() {
    let (_, conn) = database();
    let mut wrong_version = input("linux-bash");
    wrong_version.course.fingerprint = "0".repeat(64);
    let mut foreign = input("bash-scripting");
    foreign.configuration.entry = EntryChoice::Manual {
        entry_point: "production".into(),
        familiar_competencies: vec!["lb-quoting".into()],
    };
    let mut inverted = input("german");
    inverted.configuration.entry = EntryChoice::Manual {
        entry_point: "B2".into(),
        familiar_competencies: vec!["reading".into()],
    };
    let mut wrong_goal = input("typescript");
    wrong_goal.configuration.goal = LearningGoal::LanguageLevel {
        target_level: "A2".into(),
        note: String::new(),
    };
    for request in [wrong_version, foreign, inverted, wrong_goal] {
        assert!(enrollment::save_draft(&conn, &request).is_err());
    }
    assert_eq!(count(&conn, "enrollment_drafts"), 0);
    assert_eq!(count(&conn, "course_snapshots"), 0);
}

#[test]
fn failed_draft_storage_rolls_back_its_new_curriculum_snapshot() {
    let (_, conn) = database();
    conn.execute_batch("CREATE TRIGGER fail_draft BEFORE INSERT ON enrollment_drafts BEGIN SELECT RAISE(ABORT, 'injected failure'); END;").unwrap();
    assert!(enrollment::save_draft(&conn, &input("linux-bash")).is_err());
    assert_eq!(count(&conn, "course_snapshots"), 0);
    assert_eq!(count(&conn, "enrollment_drafts"), 0);
}

#[test]
fn rebasing_a_draft_keeps_the_previous_curriculum_snapshot() {
    let (_, conn) = database();
    let request = input("linux-bash");
    let first = enrollment::save_draft(&conn, &request).unwrap();
    let old_fingerprint = "a".repeat(64);
    conn.execute("INSERT INTO course_snapshots VALUES (?1, 'linux-bash', 'previous', '{\"retained\":true}', 'earlier')", [&old_fingerprint]).unwrap();
    conn.execute(
        "UPDATE enrollment_drafts SET course_snapshot_fingerprint = ?1 WHERE id = ?2",
        params![old_fingerprint, first.id.0],
    )
    .unwrap();
    let old = enrollment::draft_for_course(&conn, "linux-bash")
        .unwrap()
        .unwrap();
    assert_eq!(old.course.version, "previous");
    let mut update = request;
    update.id = Some(old.id);
    update.expected_revision = Some(old.revision);
    let refreshed = enrollment::save_draft(&conn, &update).unwrap();
    assert_eq!(refreshed.course.version, "v1");
    assert_eq!(count(&conn, "course_snapshots"), 2);
    assert!(conn
        .execute(
            "UPDATE course_snapshots SET body_json = '{}' WHERE fingerprint = ?1",
            [&old_fingerprint]
        )
        .is_err());
    assert!(conn
        .execute(
            "DELETE FROM course_snapshots WHERE fingerprint = ?1",
            [&old_fingerprint]
        )
        .is_err());
}
