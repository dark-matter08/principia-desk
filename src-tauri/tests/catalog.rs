use principia_desk_lib::{catalog, classroom, db, language, research};
use rusqlite::{params, Connection};
use serde_json::Value;

const SEED: &str = include_str!("../seed/concepts.json");
static SERIAL: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
fn database() -> (std::path::PathBuf, Connection) {
    let serial = SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "principia-catalog-{}-{serial}.db",
        std::process::id()
    ));
    let conn = db::open(&path).unwrap();
    (path, conn)
}

#[test]
fn entry_maps_references_and_primary_sources_match_their_course() {
    let seed: Vec<Value> = serde_json::from_str(SEED).unwrap();
    for course in catalog::COURSES {
        if course.kind == catalog::SubjectKind::Language {
            continue;
        }
        for entry in course.entry_points {
            assert!(
                seed.iter().any(|concept| concept["focus"] == course.id
                    && concept["curriculum"]["phase"] == entry.id
                    && concept["curriculum"]["core"] == true),
                "{} has no core entry work for {}",
                course.id,
                entry.id
            );
        }
        assert!(!course.reference_lessons.is_empty());
        assert_eq!(course.reference_lessons.len(), course.bundled_lessons.len());
        for (slug, content) in course.reference_lessons.iter().zip(course.bundled_lessons) {
            let lesson: Value = serde_json::from_str(content).unwrap();
            assert_eq!(lesson["slug"], *slug);
            assert!(seed
                .iter()
                .any(|concept| concept["slug"] == *slug && concept["focus"] == course.id));
        }
        for concept in seed.iter().filter(|concept| concept["focus"] == course.id) {
            for url in concept["curriculum"]["primary_sources"].as_array().unwrap() {
                assert!(
                    research::is_source_for(course.id, url.as_str().unwrap()),
                    "{} has an out-of-policy source: {url}",
                    concept["slug"]
                );
            }
        }
    }
}

#[test]
fn catalog_refresh_and_reopen_preserve_topic_identity_work_and_preferences() {
    let (path, conn) = database();
    // Simulate a retained System Design record with an ID unrelated to seed order.
    conn.execute("INSERT INTO concepts (id, slug, title, category, times_picked, last_picked_date, active) VALUES (700, 'cap-theorem', 'Old title', 'fundamentals', 4, '2026-09-01', 0)", []).unwrap();
    conn.execute("INSERT INTO courses (id, session_date, concept_id, markdown, source, generated_at) VALUES (800, '2026-09-01', 700, '# Saved lesson', 'fallback', '2026-09-01T10:00:00Z')", []).unwrap();
    conn.execute("INSERT INTO exercise_drafts (course_id, draft, completed, reflection, updated_at) VALUES (800, 'saved design', 1, 'my reasoning', '2026-09-01T11:00:00Z')", []).unwrap();
    conn.execute("INSERT INTO mastery (concept_id, state, score_ema, encounters, last_seen_date, last_assessed_date) VALUES (700, 'practicing', 0.75, 4, '2026-09-01', '2026-08-28')", []).unwrap();
    db::set_config(&conn, "selected_focus", "system-design").unwrap();
    db::set_config(&conn, "kiosk_level", "firm").unwrap();
    assert_eq!(db::seed_concepts(&conn, SEED).unwrap(), 299);
    language::initialize(&conn, "2026-09-09").unwrap();
    classroom::initialize(&conn).unwrap();
    conn.execute("UPDATE classroom_programs SET enabled = 1, agent = 'deepseek', model = 'sonnet', session_minutes = 45, learning_goal = 'keep my goal', label = 'old label' WHERE subject_id = 'typescript'", []).unwrap();
    drop(conn);

    let conn = db::open(&path).unwrap();
    assert_eq!(db::seed_concepts(&conn, SEED).unwrap(), 0);
    classroom::initialize(&conn).unwrap();
    let retained: (i64, i64, i64, String) = conn.query_row("SELECT id, times_picked, active, last_picked_date FROM concepts WHERE slug = 'cap-theorem'", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).unwrap();
    assert_eq!(retained, (700, 4, 0, "2026-09-01".into()));
    assert_eq!(
        conn.query_row(
            "SELECT draft FROM exercise_drafts WHERE course_id = 800",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "saved design"
    );
    assert_eq!(
        conn.query_row(
            "SELECT last_assessed_date FROM mastery WHERE concept_id = 700",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "2026-08-28"
    );
    assert_eq!(
        conn.query_row("SELECT markdown FROM courses WHERE id = 800", [], |r| {
            r.get::<_, String>(0)
        })
        .unwrap(),
        "# Saved lesson"
    );
    let preferences: (i64, String, i64, String, String) = conn.query_row("SELECT enabled, agent, session_minutes, learning_goal, label FROM classroom_programs WHERE subject_id = 'typescript'", [], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).unwrap();
    assert_eq!(
        preferences,
        (
            1,
            "deepseek".into(),
            45,
            "keep my goal".into(),
            "TypeScript".into()
        )
    );
    assert_eq!(
        db::get_config(&conn, "kiosk_level").unwrap().as_deref(),
        Some("firm")
    );
    for (focus, count) in [
        ("system-design", 72),
        ("linux-bash", 18),
        ("bash-scripting", 18),
    ] {
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM concepts WHERE focus = ?1",
                [focus],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            count
        );
    }
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM classroom_programs", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        9
    );
    assert!(!conn
        .prepare("PRAGMA foreign_key_check")
        .unwrap()
        .exists([])
        .unwrap());
}

#[test]
fn invalid_graphs_fail_before_any_seed_changes_are_written() {
    let (_, conn) = database();
    let baseline: Vec<Value> = serde_json::from_str(SEED).unwrap();
    for failure in ["cycle", "missing", "duplicate", "cross-course"] {
        let mut seed = baseline.clone();
        match failure {
            "cycle" => {
                seed[0]["prereqs"] = serde_json::json!(["consistency-models"]);
                seed[1]["prereqs"] = serde_json::json!(["cap-theorem"]);
            }
            "missing" => seed[0]["prereqs"] = serde_json::json!(["missing-topic"]),
            "duplicate" => seed.push(seed[0].clone()),
            _ => seed[0]["prereqs"] = serde_json::json!(["lb-terminal-model"]),
        }
        assert!(
            db::seed_concepts(&conn, &serde_json::to_string(&seed).unwrap()).is_err(),
            "accepted {failure}"
        );
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM concepts", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}

#[test]
fn a_late_seed_write_failure_rolls_back_earlier_metadata_refreshes() {
    let (_, conn) = database();
    db::seed_concepts(&conn, SEED).unwrap();
    conn.execute(
        "UPDATE concepts SET title = ?1 WHERE slug = 'cap-theorem'",
        params!["retained title"],
    )
    .unwrap();
    conn.execute_batch("CREATE TRIGGER reject_catalog_update BEFORE UPDATE ON concepts WHEN OLD.slug = 'bs-capstone' BEGIN SELECT RAISE(ABORT, 'injected late write failure'); END;").unwrap();
    assert!(db::seed_concepts(&conn, SEED).is_err());
    assert_eq!(
        conn.query_row(
            "SELECT title FROM concepts WHERE slug = 'cap-theorem'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "retained title"
    );
}
