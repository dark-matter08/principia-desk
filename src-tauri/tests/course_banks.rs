//! Practice questions: extra questions the tutor writes for a class, stage
//! by stage, kept apart from the bank the checks sample.

use principia_desk_lib::{
    class_builder::{self, WrittenQuestion},
    classroom, db,
    domain::{banks, placement},
    language,
};

fn fixture() -> (std::path::PathBuf, rusqlite::Connection) {
    let path = std::env::temp_dir().join(format!(
        "principia-practice-{:032x}.db",
        rand::random::<u128>()
    ));
    let conn = db::open(&path).unwrap();
    db::seed_concepts(&conn, include_str!("../seed/concepts.json")).unwrap();
    language::initialize(&conn, "2026-09-12").unwrap();
    classroom::initialize(&conn).unwrap();
    (path, conn)
}

#[test]
fn practice_questions_add_up_by_stage_and_never_touch_the_checks() {
    let (_file, conn) = fixture();
    let empty = banks::view(&conn, "developer-tooling").unwrap();
    assert_eq!(empty.usable, 0);
    assert_eq!(empty.checks_bank.source, "bundled");
    assert_eq!(empty.checks_bank.questions, 14);
    assert_eq!(empty.per_stage.len(), 4);

    // A bundled course reads as a draft with bare slugs.
    let draft = class_builder::draft_of_bundled(&conn, "developer-tooling").unwrap();
    let foundations: Vec<_> = draft
        .topics
        .iter()
        .filter(|t| t.curriculum.phase == "foundations")
        .collect();
    let written: Vec<WrittenQuestion> = foundations
        .iter()
        .take(4)
        .enumerate()
        .map(|(n, topic)| WrittenQuestion {
            topic: topic.slug.clone(),
            label: format!("{} skill", topic.title),
            prompt: format!("Question {n} about {}?", topic.title),
            choices: vec!["one".into(), "two".into(), "three".into(), "four".into()],
            answer: "two".into(),
            explanation: "Because the source says so.".into(),
            source: topic.curriculum.primary_sources[0].clone(),
        })
        .collect();
    let questions =
        class_builder::practice_from_written(&draft, "foundations", 4, "b1", written.clone())
            .unwrap();
    assert_eq!(questions.len(), 4);
    assert_eq!(questions[0].competency, foundations[0].slug);

    let view = banks::add(&conn, "developer-tooling", questions.clone()).unwrap();
    assert_eq!(view.usable, 4);
    assert_eq!(view.per_stage[0].count, 4, "all on foundations");
    assert!(view.written_at.is_some());
    // Adding the same prompts again adds nothing; a new batch adds.
    assert!(banks::add(&conn, "developer-tooling", questions).is_err());
    let more = class_builder::practice_from_written(
        &draft,
        "foundations",
        1,
        "b2",
        vec![WrittenQuestion {
            prompt: "A fresh question?".into(),
            ..written[0].clone()
        }],
    )
    .unwrap();
    let view = banks::add(&conn, "developer-tooling", more).unwrap();
    assert_eq!(view.usable, 5);

    // A disputed key stays listed, marked, and leaves the count.
    let id = view.questions[0].id.clone();
    let view = banks::void_question(&conn, "developer-tooling", &id, "wrong").unwrap();
    assert_eq!(view.usable, 4);
    assert!(view.questions[0].voided);
    assert_eq!(view.questions.len(), 5);

    // The checks still sample the authored bank, untouched.
    let sampled = placement::bank("developer-tooling").unwrap();
    assert_eq!(sampled.version, "entry-v1");
    assert_eq!(sampled.questions.len(), 14);

    let cleared = banks::clear(&conn, "developer-tooling").unwrap();
    assert_eq!(cleared.usable, 0);
    assert!(banks::add(&conn, "custom-nothing", Vec::new()).is_err());
}
