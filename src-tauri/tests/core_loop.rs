//! Simulates the multi-day data loop without the GUI: focus pools, carryover
//! isolation, session focus persistence, and focus-aware fallbacks.

use principia_desk_lib::db::{self, Attempt, Session};
use principia_desk_lib::focus;

const SEED: &str = include_str!("../seed/concepts.json");

static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn test_db() -> rusqlite::Connection {
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("sdr-test-{}-{}", std::process::id(), n));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let conn = db::open(&dir.join("test.db")).unwrap();
    db::seed_concepts(&conn, SEED).unwrap();
    conn
}

fn session_with_focus(date: &str, focus: &str) -> Session {
    Session {
        date: date.into(),
        concept_id: None,
        status: "in_progress".into(),
        current_step: "quiz".into(),
        quiz_score: None,
        started_at: None,
        completed_at: None,
        reading_seconds: 0,
        session_type: "lesson".into(),
        plan_reason: String::new(),
        focus: focus.into(),
    }
}

#[test]
fn reading_does_not_replace_the_previous_assessment_date_or_inflate_retention() {
    use principia_desk_lib::mastery;
    let conn = test_db();
    let id = db::all_concepts(&conn, "javascript").unwrap()[0].id;
    mastery::record_quiz_outcome(&conn, id, "2026-07-01", 1.0).unwrap();
    mastery::record_course_read(&conn, id, "2026-07-10").unwrap();
    let after_reading = mastery::get(&conn, id).unwrap();
    assert_eq!(
        after_reading.last_assessed_date.as_deref(),
        Some("2026-07-01")
    );
    let assessed = mastery::record_quiz_outcome(&conn, id, "2026-07-10", 1.0).unwrap();
    assert_eq!(assessed.state, "mastered");
    let repeated = mastery::record_quiz_outcome(&conn, id, "2026-07-10", 1.0).unwrap();
    assert_eq!(repeated.review_interval_days, assessed.review_interval_days);
    assert_eq!(repeated.next_review_date, assessed.next_review_date);
}

#[test]
fn migration_adds_focus_columns() {
    let conn = test_db();
    let concept_focus: String = conn
        .query_row(
            "SELECT focus FROM concepts WHERE slug = 'cap-theorem'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(concept_focus, "system-design");

    db::upsert_session(&conn, &session_with_focus("2026-07-01", "javascript")).unwrap();
    let session_focus: String = conn
        .query_row(
            "SELECT focus FROM sessions WHERE date = '2026-07-01'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(session_focus, "javascript");
}

#[test]
fn migration_adds_exit_question_rounds() {
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("sdr-exit-test-{}-{}", std::process::id(), n));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("test.db");
    {
        let old = rusqlite::Connection::open(&path).unwrap();
        old.execute_batch(
            "CREATE TABLE exit_questions (
                id INTEGER PRIMARY KEY,
                course_id INTEGER NOT NULL,
                prompt TEXT NOT NULL,
                choices_json TEXT NOT NULL,
                correct_answer TEXT NOT NULL,
                explanation TEXT NOT NULL
            );",
        )
        .unwrap();
    }

    let conn = db::open(&path).unwrap();
    let round_column_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('exit_questions') WHERE name = 'round'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(round_column_count, 1);
}

#[test]
fn exit_questions_are_isolated_by_round() {
    let conn = test_db();
    let concept = db::all_concepts(&conn, "javascript").unwrap()[0].clone();
    let course = db::insert_course(
        &conn,
        "2026-07-14",
        concept.id,
        "# Course",
        "[]",
        "fallback",
    )
    .unwrap();
    db::insert_exit_question(
        &conn,
        course,
        1,
        "Round one",
        r#"["a","b","c","d"]"#,
        "a",
        "Why",
        "Section A",
        "objective a",
    )
    .unwrap();
    db::insert_exit_question(
        &conn,
        course,
        2,
        "Round two",
        r#"["a","b","c","d"]"#,
        "b",
        "Why",
        "Section B",
        "objective b",
    )
    .unwrap();

    let first = db::exit_questions_for_course(&conn, course, 1).unwrap();
    let second = db::exit_questions_for_course(&conn, course, 2).unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].prompt, "Round one");
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].prompt, "Round two");
}

#[test]
fn migration_allows_deepseek_course_sources() {
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("sdr-source-test-{}-{}", std::process::id(), n));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("test.db");
    {
        let old = rusqlite::Connection::open(&path).unwrap();
        old.execute_batch(
            "CREATE TABLE courses (
                id INTEGER PRIMARY KEY,
                session_date TEXT NOT NULL,
                concept_id INTEGER NOT NULL,
                markdown TEXT NOT NULL,
                resources_json TEXT NOT NULL DEFAULT '[]',
                source TEXT NOT NULL CHECK(source IN ('claude','codex','fallback')),
                generated_at TEXT NOT NULL
            );",
        )
        .unwrap();
    }

    let conn = db::open(&path).unwrap();
    db::seed_concepts(&conn, SEED).unwrap();
    let concept = db::all_concepts(&conn, "javascript").unwrap()[0].clone();
    let course = db::insert_course(
        &conn,
        "2026-07-14",
        concept.id,
        "# DeepSeek course",
        "[]",
        "deepseek",
    )
    .unwrap();
    assert!(course > 0);
}

#[test]
fn seed_focus_pools_are_isolated() {
    let conn = test_db();
    for track in focus::selectable() {
        let pool = db::roulette_pool(&conn, track).unwrap();
        // Lower bound only: tracks grow independently as curriculum content
        // is added (javascript in particular covers far more browser-facing
        // ground than typescript or developer-tooling), so there is no
        // reason to cap how large any one pool gets.
        assert!(
            pool.len() >= 15,
            "track {track} pool too small: {}",
            pool.len()
        );
        assert!(pool.iter().all(|c| c.focus == *track));
    }
    let legacy = db::roulette_pool(&conn, focus::LEGACY_FOCUS).unwrap();
    assert!(legacy.len() >= 70, "legacy pool {}", legacy.len());
    assert!(legacy.iter().all(|c| c.focus == focus::LEGACY_FOCUS));
}

#[test]
fn curriculum_seed_has_valid_focus_local_prerequisite_graphs() {
    let entries: Vec<serde_json::Value> = serde_json::from_str(SEED).unwrap();
    let mut by_slug = std::collections::HashMap::new();

    for entry in &entries {
        let slug = entry["slug"].as_str().expect("concept slug");
        let concept_focus = entry["focus"].as_str().unwrap_or(focus::LEGACY_FOCUS);
        let tier = entry["tier"].as_i64().expect("concept tier");
        assert!(
            by_slug.insert(slug, (concept_focus, tier)).is_none(),
            "duplicate concept slug: {slug}"
        );
    }

    for entry in &entries {
        let slug = entry["slug"].as_str().unwrap();
        let concept_focus = entry["focus"].as_str().unwrap_or(focus::LEGACY_FOCUS);
        let concept_tier = entry["tier"].as_i64().unwrap();
        for prereq in entry["prereqs"].as_array().expect("prereqs array") {
            let prereq = prereq.as_str().expect("prereq slug");
            let (prereq_focus, prereq_tier) = by_slug
                .get(prereq)
                .unwrap_or_else(|| panic!("{slug} has dangling prerequisite {prereq}"));
            assert_eq!(
                *prereq_focus, concept_focus,
                "{slug} ({concept_focus}) has cross-focus prerequisite {prereq} ({prereq_focus})"
            );
            assert!(
                *prereq_tier <= concept_tier,
                "{slug} (tier {concept_tier}) depends on later {prereq} (tier {prereq_tier})"
            );
        }
    }

    for track in focus::selectable() {
        let track_entries: Vec<&serde_json::Value> = entries
            .iter()
            .filter(|entry| entry["focus"].as_str() == Some(track))
            .collect();
        let entry_points = entries
            .iter()
            .filter(|entry| entry["focus"].as_str() == Some(track))
            .filter(|entry| entry["tier"].as_i64() == Some(0))
            .count();
        assert!(
            entry_points >= 4,
            "track {track} needs at least four tier-0 entry points, found {entry_points}"
        );
        let core_count = track_entries
            .iter()
            .filter(|entry| entry["curriculum"]["core"].as_bool() == Some(true))
            .count();
        assert!(
            core_count >= entry_points,
            "track {track} must retain its foundations in the core path"
        );
        // Course length follows its authored outcome; a shell course is not
        // padded to fit the historical 30-session frontend schedule.
        for phase in ["foundations", "mechanisms", "production", "synthesis"] {
            assert!(
                track_entries
                    .iter()
                    .any(|entry| entry["curriculum"]["phase"] == phase
                        && entry["curriculum"]["core"] == true),
                "{track} has no core work in entry phase {phase}"
            );
        }
        for entry in track_entries {
            let slug = entry["slug"].as_str().unwrap();
            let brief: db::CurriculumBrief =
                serde_json::from_value(entry["curriculum"].clone()).unwrap();
            brief
                .validate()
                .unwrap_or_else(|reason| panic!("{slug} curriculum brief: {reason}"));
            for related in &brief.related_concepts {
                let (related_focus, _) = by_slug
                    .get(related.as_str())
                    .unwrap_or_else(|| panic!("{slug} has unknown related concept {related}"));
                assert_ne!(
                    *related_focus, track,
                    "{slug} related concept {related} must be cross-track"
                );
            }
        }
    }
}

#[test]
fn session_focus_persists_and_rejects_invalid_values() {
    let conn = test_db();
    let mut pending = Session {
        date: "2026-08-01".into(),
        concept_id: None,
        status: "pending".into(),
        current_step: "quiz".into(),
        quiz_score: None,
        started_at: None,
        completed_at: None,
        reading_seconds: 0,
        session_type: "lesson".into(),
        plan_reason: String::new(),
        focus: String::new(),
    };
    db::upsert_session(&conn, &pending).unwrap();
    focus::validate_selectable("javascript").unwrap();
    focus::validate_selectable("system-design").unwrap();
    assert!(focus::validate_selectable("unknown-subject").is_err());

    pending.focus = "typescript".into();
    pending.status = "in_progress".into();
    db::upsert_session(&conn, &pending).unwrap();

    pending.focus = "javascript".into();
    db::upsert_session(&conn, &pending).unwrap();
    let stored = db::get_session(&conn, "2026-08-01").unwrap().unwrap();
    assert_eq!(stored.focus, "typescript");

    db::set_session_focus(&conn, "2026-08-01", "developer-tooling").unwrap();
    let changed = db::get_session(&conn, "2026-08-01").unwrap().unwrap();
    assert_eq!(changed.focus, "developer-tooling");
}

#[test]
fn streak_counts_consecutive_completed_days() {
    let conn = test_db();
    for date in ["2026-06-07", "2026-06-08", "2026-06-09"] {
        db::upsert_session(
            &conn,
            &Session {
                date: date.into(),
                concept_id: None,
                status: "completed".into(),
                current_step: "done".into(),
                quiz_score: Some(1.0),
                started_at: None,
                completed_at: None,
                reading_seconds: 1800,
                session_type: "lesson".into(),
                plan_reason: String::new(),
                focus: "javascript".into(),
            },
        )
        .unwrap();
    }
    assert_eq!(db::streak(&conn, "2026-06-09").unwrap(), 3);
    assert_eq!(db::streak(&conn, "2026-06-11").unwrap(), 0);
}

#[test]
fn focus_aware_fallback_selection() {
    let js = principia_desk_lib::generator::pick_fallback("javascript", "js-event-loop scheduling");
    assert_eq!(js.slug, "js-event-loop");
    let js_from_catalog_title = principia_desk_lib::generator::pick_fallback(
        "javascript",
        "Event loop phases, tasks, microtasks, and rendering opportunities",
    );
    assert_eq!(js_from_catalog_title.slug, "js-event-loop");
    let ts =
        principia_desk_lib::generator::pick_fallback("typescript", "ts-structural-typing rules");
    assert_eq!(ts.slug, "ts-structural-typing");
    let architecture = principia_desk_lib::generator::pick_fallback(
        "frontend-architecture",
        "fa-domain-boundaries",
    );
    assert_eq!(architecture.slug, "fa-domain-boundaries");
    let dt = principia_desk_lib::generator::pick_fallback(
        "developer-tooling",
        "dt-ast-parsing pipeline",
    );
    assert_eq!(dt.slug, "dt-ast-parsing");
    let legacy =
        principia_desk_lib::generator::pick_fallback("system-design", "Rate limiting algorithms");
    assert_eq!(legacy.slug, "rate-limiting");
    assert!(!js.markdown.to_lowercase().contains("cap theorem"));
}

#[test]
fn json_payload_parser_handles_fenced_and_prose() {
    #[derive(serde::Deserialize)]
    struct T {
        x: i64,
    }
    let fenced = "Here you go:\n```json\n{\"x\": 5}\n```\nthanks";
    assert_eq!(
        principia_desk_lib::generator::parse_json_payload::<T>(fenced)
            .unwrap()
            .x,
        5
    );
    let bare = "prefix {\"x\": 7} suffix";
    assert_eq!(
        principia_desk_lib::generator::parse_json_payload::<T>(bare)
            .unwrap()
            .x,
        7
    );
}

#[test]
fn json_parser_survives_embedded_code_fences_in_markdown() {
    #[derive(serde::Deserialize)]
    struct Course {
        markdown: String,
    }
    let raw = "```json\n{\"markdown\": \"intro\\n```\\ncode here\\n```\\noutro\"}\n```";
    let c = principia_desk_lib::generator::parse_json_payload::<Course>(raw).unwrap();
    assert!(c.markdown.contains("code here"));
}

#[test]
fn mastery_lifecycle_transitions() {
    use principia_desk_lib::mastery;
    let conn = test_db();
    let concept = db::all_concepts(&conn, "javascript").unwrap()[0].clone();
    let id = concept.id;

    mastery::record_course_read(&conn, id, "2026-06-01").unwrap();
    assert_eq!(mastery::get(&conn, id).unwrap().state, "introduced");

    let m = mastery::record_quiz_outcome(&conn, id, "2026-06-02", 1.0).unwrap();
    assert_eq!(m.state, "practicing");

    let m = mastery::record_quiz_outcome(&conn, id, "2026-06-03", 1.0).unwrap();
    assert_eq!(m.state, "practicing");

    let m = mastery::record_quiz_outcome(&conn, id, "2026-06-11", 1.0).unwrap();
    assert_eq!(m.state, "mastered");

    let m = mastery::record_quiz_outcome(&conn, id, "2026-06-18", 1.0).unwrap();
    assert_eq!(m.state, "maintenance");

    let m = mastery::record_quiz_outcome(&conn, id, "2026-07-09", 0.0).unwrap();
    assert_eq!(m.state, "decayed");

    let other = db::all_concepts(&conn, "javascript").unwrap()[1].clone();
    mastery::record_course_read(&conn, other.id, "2026-06-01").unwrap();
    let m = mastery::record_quiz_outcome(&conn, other.id, "2026-06-02", 0.2).unwrap();
    assert_eq!(m.state, "struggling");
}

#[test]
fn dossier_reflects_ledger_and_notes_within_focus() {
    use principia_desk_lib::mastery;
    let conn = test_db();
    let track = "developer-tooling";
    let concepts = db::all_concepts(&conn, track).unwrap();
    let (a, b) = (concepts[0].clone(), concepts[1].clone());

    mastery::record_course_read(&conn, a.id, "2026-06-01").unwrap();
    mastery::record_quiz_outcome(&conn, a.id, "2026-06-02", 0.3).unwrap();
    mastery::set_teacher_note(&conn, a.id, "confuses token with node kind").unwrap();
    mastery::record_course_read(&conn, b.id, "2026-06-02").unwrap();
    let course = db::insert_course(&conn, "2026-06-02", b.id, "# C", "[]", "fallback").unwrap();
    let exit_question = db::insert_exit_question(
        &conn,
        course,
        1,
        "Where does invalidation belong?",
        r#"["cache","component","router","nowhere"]"#,
        "cache",
        "The cache owns freshness.",
        "Data boundaries",
        "cache ownership controls invalidation",
    )
    .unwrap();
    db::insert_exit_attempt(
        &conn,
        course,
        exit_question,
        1,
        false,
        "component",
        "Data boundaries",
        "cache ownership controls invalidation",
        "put invalidation in a leaf component",
    )
    .unwrap();
    db::save_exercise_completion(
        &conn,
        Some(course),
        None,
        true,
        "Boundary test fails forbidden imports; public facade reduces fan-out.",
    )
    .unwrap();

    let d = mastery::build_dossier(&conn, "2026-06-03", track).unwrap();
    assert!(
        d.contains("STRUGGLING (1)"),
        "dossier missing struggling section: {d}"
    );
    assert!(d.contains("confuses token with node kind"));
    assert!(d.contains(focus::label("developer-tooling")));
    assert!(d.contains("RECENT COURSES"));
    assert!(d.contains("RECENT EXIT-CHECK MISCONCEPTIONS"));
    assert!(d.contains("cache ownership controls invalidation"));
    assert!(d.contains("put invalidation in a leaf component"));
    assert!(d.contains("PRACTICAL WORK: 1 exercise(s) completed"));
    assert!(d.contains("public facade reduces fan-out"));

    let fresh = test_db();
    let d0 = mastery::build_dossier(&fresh, "2026-06-01", track).unwrap();
    assert!(d0.contains("Day 1 of teaching"));
}

#[test]
fn teacher_preamble_wraps_dossier() {
    let n = principia_desk_lib::generator::TEACHER_PROMPT
        .matches("{{DOSSIER}}")
        .count();
    assert_eq!(n, 1);
    assert!(principia_desk_lib::generator::COURSE_PROMPT.contains("{{FOCUS_LABEL}}"));
    assert!(principia_desk_lib::generator::TEACHER_PROMPT.contains("{{MONTH_OUTCOME}}"));
    assert!(principia_desk_lib::generator::COURSE_PROMPT.contains("{{MONTH_OUTCOME}}"));
    assert!(principia_desk_lib::generator::PLAN_PROMPT.contains("{{MONTH_OUTCOME}}"));
    assert!(principia_desk_lib::generator::AUDIO_PROMPT.contains("{{MONTH_OUTCOME}}"));
}

#[test]
fn generation_prompts_preserve_the_learning_quality_contract() {
    use principia_desk_lib::generator::{
        AUDIO_PROMPT, COURSE_PROMPT, EXIT_PROMPT, FIRST_PRINCIPLES_PROMPT, GRADE_PROMPT,
        QUIZ_PROMPT, TEACHER_PROMPT,
    };

    assert!(FIRST_PRINCIPLES_PROMPT.contains("first-principles.v1"));
    assert!(FIRST_PRINCIPLES_PROMPT.contains("smallest building"));
    assert!(FIRST_PRINCIPLES_PROMPT.contains("where the analogy breaks"));
    assert!(FIRST_PRINCIPLES_PROMPT.contains("identify the missing building block"));
    assert!(COURSE_PROMPT.contains("1500-2100 words"));
    assert!(COURSE_PROMPT.contains("reading hint"));
    assert!(COURSE_PROMPT.contains("> **Key idea:**"));
    assert!(COURSE_PROMPT.contains("3-5 observable acceptance criteria"));
    assert!(COURSE_PROMPT.contains("what would fail at 10x scale or team size"));
    assert!(COURSE_PROMPT.contains("irreducible building blocks and constraints"));
    assert!(COURSE_PROMPT.contains("where the analogy breaks down"));
    for heading in [
        "## Why this matters",
        "## The simple version",
        "## Core mechanics",
        "## Mental model",
        "## Runnable experiment",
        "## Production architecture lens",
        "## Trade-offs and failure modes",
        "## Migration and observability",
        "## Practical exercise",
        "## Key takeaways",
    ] {
        assert!(COURSE_PROMPT.contains(heading), "missing heading {heading}");
    }

    assert!(QUIZ_PROMPT.contains("exactly 5 questions: 3 multiple-choice and 2 free-text"));
    assert!(QUIZ_PROMPT.contains("At least 2 questions must use a realistic production constraint"));
    assert!(QUIZ_PROMPT.contains("accurate language tag and a blank line"));
    assert!(EXIT_PROMPT.contains("Do not repeat or lightly rephrase"));
    assert!(GRADE_PROMPT.contains("connect its decision to the stated constraint or evidence"));
    assert!(TEACHER_PROMPT.contains("Days 28-30"));
    assert!(TEACHER_PROMPT.contains("Teach every unfamiliar idea from first principles"));
    assert!(AUDIO_PROMPT.contains("verbalized runnable experiment"));
}

#[test]
fn pop_quiz_sample_prefers_struggling_within_focus() {
    use principia_desk_lib::mastery;
    let conn = test_db();
    let track = "typescript";
    let concepts = db::all_concepts(&conn, track).unwrap();
    let (good, bad) = (concepts[0].clone(), concepts[1].clone());

    let c1 = db::insert_course(&conn, "2026-06-01", good.id, "# A", "[]", "fallback").unwrap();
    let c2 = db::insert_course(&conn, "2026-06-02", bad.id, "# B", "[]", "fallback").unwrap();
    let mut ids = Vec::new();
    for (course, n) in [(c1, 3), (c2, 3)] {
        for i in 0..n {
            let q = db::insert_question(
                &conn,
                course,
                &format!("Q{course}-{i}"),
                "mcq",
                Some(r#"["a","b","c","d"]"#),
                "a",
                "x",
            )
            .unwrap();
            db::record_attempt(
                &conn,
                &Attempt {
                    question_id: q,
                    session_date: "2026-06-03".into(),
                    user_answer: "a".into(),
                    correct: true,
                    grader_feedback: String::new(),
                },
            )
            .unwrap();
            ids.push(q);
        }
    }
    mastery::record_quiz_outcome(&conn, good.id, "2026-06-03", 1.0).unwrap();
    mastery::record_quiz_outcome(&conn, bad.id, "2026-06-03", 0.0).unwrap();
    db::push_carryover(&conn, ids[0], "2026-06-03", "2026-06-04").unwrap();

    let sample = db::pop_quiz_sample(&conn, "2026-06-04", track, &[], 4).unwrap();
    assert!(!sample.is_empty());
    assert!(sample.iter().all(|q| q.id != ids[0]));
    assert_eq!(sample[0].course_id, c2);

    let other = db::pop_quiz_sample(&conn, "2026-06-04", "javascript", &[], 4).unwrap();
    assert!(other.is_empty());

    let daily_review = db::spaced_review_sample(&conn, "2026-06-04", track, &[], 2).unwrap();
    assert!(!daily_review.is_empty());
    assert!(daily_review.iter().all(|question| question.course_id == c2));
    assert!(daily_review.iter().all(|question| question.id != ids[0]));
}

#[test]
fn migration_adds_exercise_tables_to_pre_existing_db() {
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "sdr-exercise-mig-test-{}-{}",
        std::process::id(),
        n
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("test.db");
    {
        // Simulate a database created before the exercise workspace existed:
        // no course_exercises/exercise_drafts tables at all.
        let old = rusqlite::Connection::open(&path).unwrap();
        old.execute_batch(
            "CREATE TABLE courses (
                id INTEGER PRIMARY KEY,
                session_date TEXT NOT NULL,
                concept_id INTEGER NOT NULL,
                markdown TEXT NOT NULL,
                resources_json TEXT NOT NULL DEFAULT '[]',
                source TEXT NOT NULL CHECK(source IN ('claude','codex','cursor','gemini','deepseek','custom','fallback')),
                generated_at TEXT NOT NULL
            );",
        )
        .unwrap();
    }

    let conn = db::open(&path).unwrap();
    db::seed_concepts(&conn, SEED).unwrap();
    let concept = db::all_concepts(&conn, "javascript").unwrap()[0].clone();
    let course = db::insert_course(
        &conn,
        "2026-07-15",
        concept.id,
        "# Course",
        "[]",
        "fallback",
    )
    .unwrap();

    // The additive migration must have created both tables so read/save
    // helpers work immediately on an upgraded, pre-existing database.
    db::upsert_course_exercise(
        &conn,
        course,
        "Build a tiny tracer",
        "Trace the log ordering.",
        Some("console.log(1);"),
        Some("A log with annotations."),
        &["look at the sync lines first".to_string()],
    )
    .unwrap();
    let saved = db::get_course_exercise(&conn, course).unwrap().unwrap();
    assert_eq!(saved.title, "Build a tiny tracer");

    db::save_exercise_draft(&conn, Some(course), None, "my draft text").unwrap();
    let draft = db::get_exercise_draft(&conn, Some(course), None)
        .unwrap()
        .unwrap();
    assert_eq!(draft, "my draft text");
    db::save_exercise_completion(
        &conn,
        Some(course),
        None,
        true,
        "Trace proves ordering; yielding trades throughput for responsiveness.",
    )
    .unwrap();
    let completion = db::get_exercise_completion(&conn, Some(course), None).unwrap();
    assert!(completion.0);
    assert!(completion.1.contains("yielding trades throughput"));
}

#[test]
fn course_exercise_upsert_overwrites_prior_version() {
    let conn = test_db();
    let concept = db::all_concepts(&conn, "javascript").unwrap()[0].clone();
    let course =
        db::insert_course(&conn, "2026-07-16", concept.id, "# C", "[]", "fallback").unwrap();

    db::upsert_course_exercise(
        &conn,
        course,
        "First title",
        "First instructions",
        None,
        None,
        &["hint one".to_string()],
    )
    .unwrap();
    db::upsert_course_exercise(
        &conn,
        course,
        "Second title",
        "Second instructions",
        Some("let x = 1;"),
        Some("A working snippet."),
        &["hint one".to_string(), "hint two".to_string()],
    )
    .unwrap();

    let saved = db::get_course_exercise(&conn, course).unwrap().unwrap();
    assert_eq!(saved.title, "Second title");
    assert_eq!(saved.instructions, "Second instructions");
    assert_eq!(saved.starter_code, Some("let x = 1;".to_string()));
    assert_eq!(saved.deliverable, Some("A working snippet.".to_string()));
    assert_eq!(
        saved.hints,
        vec!["hint one".to_string(), "hint two".to_string()]
    );
}

#[test]
fn exercise_draft_autosave_overwrites_and_is_isolated_per_course() {
    let conn = test_db();
    let concepts = db::all_concepts(&conn, "javascript").unwrap();
    let c1 =
        db::insert_course(&conn, "2026-07-17", concepts[0].id, "# A", "[]", "fallback").unwrap();
    let c2 =
        db::insert_course(&conn, "2026-07-18", concepts[1].id, "# B", "[]", "fallback").unwrap();

    assert!(db::get_exercise_draft(&conn, Some(c1), None)
        .unwrap()
        .is_none());

    db::save_exercise_draft(&conn, Some(c1), None, "draft v1").unwrap();
    db::save_exercise_draft(&conn, Some(c1), None, "draft v2").unwrap();
    db::save_exercise_draft(&conn, Some(c2), None, "other course draft").unwrap();

    assert_eq!(
        db::get_exercise_draft(&conn, Some(c1), None).unwrap(),
        Some("draft v2".to_string())
    );
    assert_eq!(
        db::get_exercise_draft(&conn, Some(c2), None).unwrap(),
        Some("other course draft".to_string())
    );
}

#[test]
fn get_course_exercise_is_none_when_no_exercise_saved() {
    let conn = test_db();
    let concept = db::all_concepts(&conn, "javascript").unwrap()[0].clone();
    let course =
        db::insert_course(&conn, "2026-07-19", concept.id, "# C", "[]", "fallback").unwrap();
    assert!(db::get_course_exercise(&conn, course).unwrap().is_none());
}

#[test]
fn migration_adds_exit_attempts_table_to_pre_existing_db() {
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "sdr-exit-attempts-mig-test-{}-{}",
        std::process::id(),
        n
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("test.db");
    {
        // Pre-dates both `round`/`section`/`learning_objective` on
        // exit_questions and the exit_attempts table entirely.
        let old = rusqlite::Connection::open(&path).unwrap();
        old.execute_batch(
            "CREATE TABLE courses (
                id INTEGER PRIMARY KEY,
                session_date TEXT NOT NULL,
                concept_id INTEGER NOT NULL,
                markdown TEXT NOT NULL,
                resources_json TEXT NOT NULL DEFAULT '[]',
                source TEXT NOT NULL CHECK(source IN ('claude','codex','cursor','gemini','deepseek','custom','fallback')),
                generated_at TEXT NOT NULL
            );
            CREATE TABLE exit_questions (
                id INTEGER PRIMARY KEY,
                course_id INTEGER NOT NULL,
                prompt TEXT NOT NULL,
                choices_json TEXT NOT NULL,
                correct_answer TEXT NOT NULL,
                explanation TEXT NOT NULL
            );",
        )
        .unwrap();
    }

    let conn = db::open(&path).unwrap();
    db::seed_concepts(&conn, SEED).unwrap();
    let concept = db::all_concepts(&conn, "javascript").unwrap()[0].clone();
    let course = db::insert_course(
        &conn,
        "2026-07-20",
        concept.id,
        "# Course",
        "[]",
        "fallback",
    )
    .unwrap();
    let question_id = db::insert_exit_question(
        &conn,
        course,
        1,
        "Q1",
        r#"["a","b","c","d"]"#,
        "a",
        "explain",
        "Core mechanics",
        "objective one",
    )
    .unwrap();

    // The migration must have created exit_attempts so this succeeds on an
    // upgraded, pre-existing database without a fresh install.
    let attempt_id = db::insert_exit_attempt(
        &conn,
        course,
        question_id,
        1,
        false,
        "b",
        "Core mechanics",
        "objective one",
        "picked the distractor instead of the mechanism",
    )
    .unwrap();
    assert!(attempt_id > 0);
}

#[test]
fn insert_exit_attempt_persists_every_field_and_supports_multiple_rounds() {
    let conn = test_db();
    let concept = db::all_concepts(&conn, "javascript").unwrap()[0].clone();
    let course =
        db::insert_course(&conn, "2026-07-21", concept.id, "# C", "[]", "fallback").unwrap();
    let q1 = db::insert_exit_question(
        &conn,
        course,
        1,
        "Q1",
        r#"["a","b"]"#,
        "a",
        "why a",
        "Section A",
        "objective a",
    )
    .unwrap();
    let q2 = db::insert_exit_question(
        &conn,
        course,
        2,
        "Q2",
        r#"["a","b"]"#,
        "b",
        "why b",
        "Section B",
        "objective b",
    )
    .unwrap();

    db::insert_exit_attempt(
        &conn,
        course,
        q1,
        1,
        false,
        "b",
        "Section A",
        "objective a",
        "confused the two mechanisms",
    )
    .unwrap();
    db::insert_exit_attempt(
        &conn,
        course,
        q2,
        2,
        true,
        "b",
        "Section B",
        "objective b",
        "",
    )
    .unwrap();

    let (round, correct, user_answer, section, objective, misconception): (
        i64,
        i64,
        String,
        String,
        String,
        String,
    ) = conn
        .query_row(
            "SELECT round, correct, user_answer, section, learning_objective, misconception
             FROM exit_attempts WHERE question_id = ?1",
            [q1],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(round, 1);
    assert_eq!(correct, 0);
    assert_eq!(user_answer, "b");
    assert_eq!(section, "Section A");
    assert_eq!(objective, "objective a");
    assert_eq!(misconception, "confused the two mechanisms");

    let total: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM exit_attempts WHERE course_id = ?1",
            [course],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(total, 2);
}

#[test]
fn migration_moves_classroom_exercise_columns_into_the_unified_table() {
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "sdr-exercise-migration-{}-{}",
        std::process::id(),
        n
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("test.db");
    {
        // An install from before the unified exercise table: drafts keyed by
        // course only, classroom work stored as columns on classroom_sessions.
        let conn = rusqlite::Connection::open(&path).unwrap();
        // The draft has a real parent document; orphan detection is covered
        // separately by the migration integrity tests.
        conn.execute_batch(include_str!("fixtures/upgrades/original-main.sql"))
            .unwrap();
        conn.execute_batch("INSERT INTO concepts (id, slug, title, category) VALUES (7, 'retained', 'Retained', 'fundamentals');
            INSERT INTO courses (id, session_date, concept_id, markdown, source, generated_at) VALUES (7, '2026-07-21', 7, '# Retained', 'fallback', 'now');").unwrap();
        conn.execute_batch(
            "CREATE TABLE exercise_drafts (
                 course_id INTEGER PRIMARY KEY,
                 draft TEXT NOT NULL DEFAULT '',
                 completed INTEGER NOT NULL DEFAULT 0,
                 reflection TEXT NOT NULL DEFAULT '',
                 updated_at TEXT NOT NULL
             );
             INSERT INTO exercise_drafts VALUES (7, 'course draft', 1, 'course evidence', 'now');
             CREATE TABLE classroom_sessions (
                 id INTEGER PRIMARY KEY,
                 subject_id TEXT NOT NULL,
                 session_date TEXT NOT NULL,
                 status TEXT NOT NULL,
                 title TEXT NOT NULL,
                 payload_json TEXT NOT NULL,
                 score REAL,
                 agent_used TEXT NOT NULL,
                 prompt_version TEXT NOT NULL,
                 started_at TEXT NOT NULL,
                 completed_at TEXT,
                 exercise_draft TEXT NOT NULL DEFAULT '',
                 exercise_completed INTEGER NOT NULL DEFAULT 0,
                 exercise_reflection TEXT NOT NULL DEFAULT ''
             );
             INSERT INTO classroom_sessions
                 (subject_id, session_date, status, title, payload_json, score,
                  agent_used, prompt_version, started_at, completed_at,
                  exercise_draft, exercise_completed, exercise_reflection)
             VALUES ('javascript', '2026-07-21', 'completed', 'legacy', '{}', 0.6,
                     'fallback', 'classroom.javascript.v1', '2026-07-21T09:00:00',
                     '2026-07-21T10:00:00', 'legacy draft', 1, 'legacy evidence');",
        )
        .unwrap();
    }
    let conn = db::open(&path).unwrap();
    let classroom_id: i64 = conn
        .query_row("SELECT id FROM classroom_sessions", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        db::get_exercise_draft(&conn, None, Some(classroom_id)).unwrap(),
        Some("legacy draft".to_string())
    );
    let (completed, reflection) =
        db::get_exercise_completion(&conn, None, Some(classroom_id)).unwrap();
    assert!(completed);
    assert_eq!(reflection, "legacy evidence");
    assert_eq!(
        db::get_exercise_draft(&conn, Some(7), None).unwrap(),
        Some("course draft".to_string())
    );
}
