use principia_desk_lib::{
    db,
    generator::{self, CourseRequest},
};
use serde::Deserialize;
use std::sync::{Arc, Mutex};
use std::time::Instant;

const SEED: &str = include_str!("../seed/concepts.json");
const GOLDEN_CASES: &str = include_str!("../evals/golden-cases.json");

#[derive(Debug, Deserialize)]
struct GoldenCase {
    slug: String,
    dossier: String,
}

fn seeded_db() -> rusqlite::Connection {
    let path = std::env::temp_dir().join(format!(
        "sdr-quality-eval-{}-{}.db",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    let _ = std::fs::remove_file(&path);
    let conn = db::open(&path).unwrap();
    db::seed_concepts(&conn, SEED).unwrap();
    conn
}

#[test]
fn golden_eval_covers_every_track_with_complete_curriculum_contracts() {
    let conn = seeded_db();
    let cases: Vec<GoldenCase> = serde_json::from_str(GOLDEN_CASES).unwrap();
    for track in principia_desk_lib::focus::selectable() {
        let covered = cases
            .iter()
            .filter_map(|case| {
                conn.query_row(
                    "SELECT id FROM concepts WHERE slug = ?1 AND focus = ?2",
                    rusqlite::params![case.slug, track],
                    |row| row.get::<_, i64>(0),
                )
                .ok()
            })
            .count();
        assert!(
            covered >= 2,
            "golden eval needs at least two representative cases for {track}"
        );
    }
    for case in cases {
        assert!(case.dossier.split_whitespace().count() >= 12);
        let concept_id: i64 = conn
            .query_row(
                "SELECT id FROM concepts WHERE slug = ?1",
                [&case.slug],
                |row| row.get(0),
            )
            .unwrap_or_else(|_| panic!("golden case has unknown slug {}", case.slug));
        let concept = db::get_concept(&conn, concept_id).unwrap().unwrap();
        concept
            .curriculum
            .validate()
            .unwrap_or_else(|reason| panic!("{}: {reason}", case.slug));
    }
}

#[tokio::test]
#[ignore = "explicit live-provider quality and telemetry evaluation"]
async fn live_golden_courses_pass_the_editor_and_deterministic_gate() {
    assert_eq!(
        std::env::var("PRINCIPIA_LIVE_EVAL").as_deref(),
        Ok("1"),
        "set PRINCIPIA_LIVE_EVAL=1 to acknowledge provider cost"
    );
    let conn = seeded_db();
    let cases: Vec<GoldenCase> = serde_json::from_str(GOLDEN_CASES).unwrap();
    let agent = std::env::var("PRINCIPIA_EVAL_AGENT").unwrap_or_else(|_| "deepseek".into());
    let model = std::env::var("PRINCIPIA_EVAL_MODEL").unwrap_or_else(|_| "deepseek-chat".into());
    let custom_bin = std::env::var("PRINCIPIA_EVAL_CUSTOM_BIN").unwrap_or_default();
    let generator = generator::Generator::new(
        "claude".into(),
        Some("codex".into()),
        std::env::temp_dir().join("sdr-live-quality-eval"),
        Arc::new(Mutex::new(model)),
        Arc::new(Mutex::new(agent)),
        Arc::new(Mutex::new(custom_bin)),
        Default::default(),
    );

    for case in cases {
        let concept_id: i64 = conn
            .query_row(
                "SELECT id FROM concepts WHERE slug = ?1",
                [&case.slug],
                |row| row.get(0),
            )
            .unwrap();
        let concept = db::get_concept(&conn, concept_id).unwrap().unwrap();
        let started = Instant::now();
        let (course, source) = generator
            .generate_course(CourseRequest {
                title: &concept.title,
                category: &concept.category,
                dossier: &case.dossier,
                focus: &concept.focus,
                curriculum: &concept.curriculum,
                budget: generator::LessonBudget::default(),
            })
            .await
            .unwrap_or_else(|error| panic!("{} live generation: {error}", case.slug));
        generator::validate_generated_course(&course)
            .unwrap_or_else(|reason| panic!("{} deterministic gate: {reason}", case.slug));
        let words = course.markdown.split_whitespace().count();
        println!(
            "{{\"slug\":\"{}\",\"provider\":\"{}\",\"latency_ms\":{},\"words\":{},\"estimated_tokens\":{}}}",
            case.slug,
            source,
            started.elapsed().as_millis(),
            words,
            words * 4 / 3
        );
    }
}
