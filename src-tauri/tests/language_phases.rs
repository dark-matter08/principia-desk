//! Language passes vary their tasks and checks by phase and gate passing on
//! phase-specific practice evidence, without presenting it as proficiency.
use principia_desk_lib::{classroom, db, language};
use rusqlite::Connection;

fn fixture() -> Connection {
    let path = std::env::temp_dir().join(format!(
        "principia-phases-{:032x}.db",
        rand::random::<u128>()
    ));
    let conn = db::open(&path).unwrap();
    db::seed_concepts(&conn, include_str!("../seed/concepts.json")).unwrap();
    language::initialize(&conn, "2026-09-09").unwrap();
    classroom::initialize(&conn).unwrap();
    conn
}

#[test]
fn passes_vary_task_families_and_rotate_curated_checks() {
    let unit = "de-a1-greetings-introductions";
    let mut previous: Vec<String> = Vec::new();
    for phase in 1..=7 {
        let check = language::preview_check("german", "A1", unit, phase).unwrap();
        assert_eq!(check.len(), 5, "phase {phase}");
        for question in &check {
            assert_eq!(
                question.choices.len(),
                4,
                "phase {phase}: {}",
                question.prompt
            );
            let mut unique = question.choices.clone();
            unique.sort();
            unique.dedup();
            assert_eq!(unique.len(), 4, "phase {phase} has duplicate choices");
            assert!(
                language::STRANDS.contains(&question.strand.as_str()),
                "{}",
                question.strand
            );
        }
        let prompts: Vec<String> = check.iter().map(|q| q.prompt.clone()).collect();
        let shared = prompts.iter().filter(|p| previous.contains(p)).count();
        assert!(
            shared <= 2,
            "phase {phase} repeats {shared} prompts of the previous pass"
        );
        previous = prompts;
    }
    let recognition = language::preview_check("german", "A1", unit, 1).unwrap();
    let interaction = language::preview_check("german", "A1", unit, 3).unwrap();
    let listening = language::preview_check("german", "A1", unit, 4).unwrap();
    let written = language::preview_check("german", "A1", unit, 5).unwrap();
    let retrieval = language::preview_check("german", "A1", unit, 7).unwrap();
    assert!(
        recognition
            .iter()
            .filter(|q| q.prompt.starts_with("In this scenario, what does"))
            .count()
            >= 2
    );
    assert!(
        interaction
            .iter()
            .filter(|q| q.prompt.contains("Which line answers it"))
            .count()
            >= 2
    );
    assert!(listening.iter().filter(|q| q.strand == "listening").count() >= 2);
    assert!(written.iter().filter(|q| q.strand == "writing").count() >= 3);
    assert!(
        retrieval
            .iter()
            .filter(|q| q.prompt.starts_with("Which target-language expression")
                || q.prompt.contains("What does that mean"))
            .count()
            >= 2
    );
    // Italian units have the same shape.
    let italian = language::preview_check("italian", "A1", "it-a1-greetings", 3).unwrap();
    assert_eq!(italian.len(), 5);
    assert!(language::preview_check("german", "A1", "no-such-unit", 1).is_err());
}

#[test]
fn phase_requirements_gate_passing_and_are_explained() {
    let conn = fixture();
    let unit = "de-a1-greetings-introductions";
    let all_right: Vec<(String, bool)> = [
        "listening",
        "reading",
        "grammar",
        "writing",
        "vocabulary_pragmatics",
    ]
    .iter()
    .map(|s| (s.to_string(), true))
    .collect();
    let evidence =
        |phase: i64,
         writing: &'static str,
         speaking: bool,
         listened: bool,
         results: &'static [(String, bool)]| language::CompletionEvidence {
            language: "german",
            level: "A1",
            unit_slug: unit,
            phase,
            strand_results: results,
            writing_response: writing,
            speaking_completed: speaking,
            listened,
            confidence: 3,
        };
    let results: &'static [(String, bool)] = Box::leak(all_right.clone().into_boxed_slice());
    // Recognition pass: knowledge alone counts.
    assert!(
        language::project_completion(&conn, evidence(1, "", false, false, results), "2026-09-10")
            .unwrap()
            .passed
    );
    // Listening transfer needs the exchange played.
    let blocked =
        language::project_completion(&conn, evidence(4, "", false, false, results), "2026-09-11")
            .unwrap();
    assert!(!blocked.passed);
    assert!(blocked
        .requirement
        .as_deref()
        .unwrap_or_default()
        .contains("listening"));
    assert!(
        language::project_completion(&conn, evidence(4, "", false, true, results), "2026-09-12")
            .unwrap()
            .passed
    );
    // Written production needs twelve words; spoken production the speaking task.
    assert!(
        !language::project_completion(
            &conn,
            evidence(5, "Hallo, ich bin Tom.", false, true, results),
            "2026-09-13"
        )
        .unwrap()
        .passed
    );
    assert!(language::project_completion(&conn, evidence(5, "Hallo, ich heisse Tom und ich komme aus Berlin. Ich lerne Deutsch. Auf Wiedersehen!", false, true, results), "2026-09-14").unwrap().passed);
    assert!(
        !language::project_completion(&conn, evidence(6, "", false, true, results), "2026-09-15")
            .unwrap()
            .passed
    );
    assert!(
        language::project_completion(&conn, evidence(6, "", true, true, results), "2026-09-16")
            .unwrap()
            .passed
    );
    // Integrated retrieval needs four of five and one production sample.
    let mostly: Vec<(String, bool)> = all_right
        .iter()
        .enumerate()
        .map(|(i, (s, _))| (s.clone(), i < 3))
        .collect();
    let mostly: &'static [(String, bool)] = Box::leak(mostly.into_boxed_slice());
    let weak =
        language::project_completion(&conn, evidence(7, "", true, true, mostly), "2026-09-17")
            .unwrap();
    assert!(
        !weak.passed
            && weak
                .requirement
                .as_deref()
                .unwrap_or_default()
                .contains("four correct")
    );
    assert!(
        !language::project_completion(&conn, evidence(7, "", false, true, results), "2026-09-18")
            .unwrap()
            .passed
    );
    assert!(
        language::project_completion(&conn, evidence(7, "", true, true, results), "2026-09-19")
            .unwrap()
            .passed
    );
}
