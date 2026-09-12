//! Offline validation of the bundled reference lessons: schema, concept link,
//! objective-linked questions, topic specificity and representative kinds.
use principia_desk_lib::{catalog, db, generator};
use std::collections::{BTreeMap, BTreeSet};

const KINDS: [&str; 5] = [
    "beginner",
    "advanced",
    "remediation",
    "retrieval",
    "capstone",
];
/// Courses whose bundled set must already cover every representative kind.
const COMPLETE: [&str; 7] = [
    "linux-bash",
    "bash-scripting",
    "typescript",
    "javascript",
    "frontend-architecture",
    "developer-tooling",
    "system-design",
];

fn concepts() -> Vec<db::Concept> {
    let path = std::env::temp_dir().join(format!(
        "principia-fixtures-{:032x}.db",
        rand::random::<u128>()
    ));
    let conn = db::open(&path).unwrap();
    db::seed_concepts(&conn, include_str!("../seed/concepts.json")).unwrap();
    catalog::COURSES
        .iter()
        .filter(|course| course.kind == catalog::SubjectKind::Engineering)
        .flat_map(|course| db::all_concepts(&conn, course.id).unwrap())
        .collect()
}

fn specific(markdown: &str, concept: &db::Concept) -> bool {
    let text = markdown.to_lowercase();
    let mut tokens: Vec<String> = concept
        .title
        .split(|c: char| !c.is_alphanumeric())
        .chain(
            concept
                .curriculum
                .mechanisms
                .iter()
                .flat_map(|m| m.split(|c: char| !c.is_alphanumeric())),
        )
        .map(|t| t.to_lowercase())
        .filter(|t| t.len() >= 5)
        .collect();
    tokens.sort();
    tokens.dedup();
    tokens.iter().filter(|t| text.contains(t.as_str())).count() >= 2
}

#[test]
fn every_bundled_lesson_is_a_valid_specific_reference_for_its_topic() {
    let concepts = concepts();
    let mut kinds_by_course: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    let mut total = 0;
    for course in catalog::COURSES
        .iter()
        .filter(|c| c.kind == catalog::SubjectKind::Engineering)
    {
        for source in course.bundled_lessons {
            let lesson: generator::FallbackCourse =
                serde_json::from_str(source).unwrap_or_else(|e| panic!("{}: {e}", course.id));
            total += 1;
            let concept = concepts
                .iter()
                .find(|c| c.slug == lesson.slug && c.focus == course.id)
                .unwrap_or_else(|| panic!("{} is not a concept of {}", lesson.slug, course.id));
            assert!(
                KINDS.contains(&lesson.kind.as_str()) || lesson.kind.is_empty(),
                "{}: unknown kind {}",
                lesson.slug,
                lesson.kind
            );
            let words = lesson.markdown.split_whitespace().count();
            let minimum = if lesson.kind == "retrieval" { 120 } else { 400 };
            assert!(words >= minimum, "{}: {words} words", lesson.slug);
            // Kind-tagged fixtures carry three takeaways; older references keep at least one.
            let takeaways = if lesson.kind.is_empty() { 1 } else { 3 };
            assert!(
                lesson.key_takeaways.len() >= takeaways,
                "{}: takeaways",
                lesson.slug
            );
            assert!(
                !lesson.resources.is_empty()
                    && lesson
                        .resources
                        .iter()
                        .all(|r| r.url.starts_with("https://")),
                "{}: resources",
                lesson.slug
            );
            assert!(
                specific(&lesson.markdown, concept),
                "{}: markdown does not name its topic",
                lesson.slug
            );
            let mcq: Vec<_> = lesson
                .questions
                .iter()
                .filter(|q| q.kind == "mcq")
                .collect();
            let free = lesson.questions.iter().filter(|q| q.kind == "free").count();
            if lesson.kind == "retrieval" {
                assert!(
                    mcq.len() >= 5,
                    "{}: a retrieval set needs five multiple-choice questions",
                    lesson.slug
                );
            } else {
                assert!(
                    mcq.len() >= 3 && free >= 1,
                    "{}: needs three multiple-choice and one free question",
                    lesson.slug
                );
            }
            for question in &lesson.questions {
                assert!(
                    !question.prompt.trim().is_empty() && !question.explanation.trim().is_empty(),
                    "{}",
                    lesson.slug
                );
                assert!(
                    !question.learning_objective.trim().is_empty(),
                    "{}: objective missing on {}",
                    lesson.slug,
                    question.prompt
                );
                if question.kind == "mcq" {
                    let choices = question.choices.as_ref().expect("mcq choices");
                    let unique: BTreeSet<_> = choices.iter().collect();
                    assert_eq!(
                        (choices.len(), unique.len()),
                        (4, 4),
                        "{}: {}",
                        lesson.slug,
                        question.prompt
                    );
                    assert!(
                        choices.contains(&question.correct_answer),
                        "{}: answer not among choices: {}",
                        lesson.slug,
                        question.prompt
                    );
                } else {
                    assert_eq!(question.kind, "free", "{}", lesson.slug);
                    assert!(
                        question.correct_answer.split_whitespace().count() >= 8,
                        "{}: free answer too thin",
                        lesson.slug
                    );
                }
            }
            if lesson.kind != "retrieval" {
                let exercise = lesson
                    .exercise
                    .as_ref()
                    .unwrap_or_else(|| panic!("{}: exercise", lesson.slug));
                assert!(
                    exercise
                        .deliverable
                        .as_deref()
                        .is_some_and(|d| !d.trim().is_empty()),
                    "{}: deliverable",
                    lesson.slug
                );
                assert!(
                    !exercise.instructions.trim().is_empty() && !exercise.hints.is_empty(),
                    "{}: exercise body",
                    lesson.slug
                );
            }
            if lesson.kind == "capstone" {
                assert_eq!(
                    concept.curriculum.phase, "synthesis",
                    "{}: capstone must be a synthesis topic",
                    lesson.slug
                );
                assert!(
                    lesson.markdown.contains("mktemp -d"),
                    "{}: capstone lab needs an isolated setup",
                    lesson.slug
                );
            }
            kinds_by_course
                .entry(course.id)
                .or_default()
                .insert(lesson.kind.clone());
        }
    }
    assert!(total >= 21, "{total} bundled lessons");
    for course in COMPLETE {
        let kinds = kinds_by_course.get(course).cloned().unwrap_or_default();
        for kind in KINDS {
            assert!(kinds.contains(kind), "{course} lacks a {kind} fixture");
        }
    }
    // Lookups by slug never fall back to a title-scored neighbour.
    assert_eq!(
        generator::fallback_for_slug("linux-bash", "lb-capstone").map(|c| c.kind),
        Some("capstone".into())
    );
    assert!(generator::fallback_for_slug("linux-bash", "lb-navigation").is_none());
}

/// Every engineering brief is specific: no templated placeholders remain.
#[test]
fn no_engineering_brief_is_templated() {
    for concept in concepts() {
        let brief = &concept.curriculum;
        assert!(
            !brief
                .mechanisms
                .iter()
                .any(|m| m.starts_with("The first-principles mechanism behind")),
            "{} still carries a templated mechanism",
            concept.slug
        );
        assert!(
            !brief
                .learner_outcome
                .starts_with("Diagnose, explain, and apply"),
            "{} still carries a templated outcome",
            concept.slug
        );
        brief
            .validate()
            .unwrap_or_else(|reason| panic!("{}: {reason}", concept.slug));
    }
}
