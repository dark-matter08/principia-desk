//! A learner's own class: from a brief to a published course that the rest
//! of the desk treats like a bundled one.

use principia_desk_lib::{
    catalog, classroom,
    classroom::{ConfigureClassroomInput, UpsertClassroomSlotInput},
    db,
    db::CurriculumBrief,
    domain::{
        classes::{self, AcceptPath},
        custom::{self, CourseBrief, CourseDraft, DraftTopic},
        enrollment::{self, EntryChoice, SaveEnrollmentDraft},
        placement,
    },
    language, mastery,
    subjects::engineering,
};
use rusqlite::Connection;

fn fixture() -> (std::path::PathBuf, Connection) {
    let path = std::env::temp_dir().join(format!(
        "principia-custom-{:032x}.db",
        rand::random::<u128>()
    ));
    let conn = db::open(&path).unwrap();
    db::seed_concepts(&conn, include_str!("../seed/concepts.json")).unwrap();
    language::initialize(&conn, "2026-09-12").unwrap();
    classroom::initialize(&conn).unwrap();
    (path, conn)
}

/// The catalog registry is one per process, so each test names its class
/// differently.
fn brief_titled(title: &str) -> CourseBrief {
    CourseBrief {
        title: title.into(),
        outcome: "Build and ship a small command-line tool with tests and a release binary".into(),
        background: "Comfortable in Python".into(),
        trusted_hosts: vec!["doc.rust-lang.org".into(), "docs.rs".into()],
        agent: "claude".into(),
        model: "opus".into(),
        custom_agent_bin: String::new(),
    }
}

fn topic(slug: &str, phase: &str, core: bool, prereqs: &[&str]) -> DraftTopic {
    DraftTopic {
        slug: slug.into(),
        title: format!("Topic {slug}"),
        category: "fundamentals".into(),
        prereqs: prereqs.iter().map(|p| p.to_string()).collect(),
        curriculum: CurriculumBrief {
            phase: phase.into(),
            core,
            learner_outcome: "Explain the mechanism with a worked example and a measured result of your own making".into(),
            mechanisms: vec!["one mechanism".into(), "another mechanism".into()],
            production_scenario: "A tool in daily use meets this problem on a busy day and someone has to fix it before lunch".into(),
            misconceptions: vec!["it is not magic".into()],
            evidence: "A trace showing the mechanism at work in the tool".into(),
            artifact: "A short note with the trace and the fix applied".into(),
            primary_sources: vec![
                "https://doc.rust-lang.org/book/".into(),
                "https://docs.rs/clap/latest/clap/".into(),
            ],
            related_concepts: vec![],
        },
    }
}

fn complete(mut draft: CourseDraft) -> CourseDraft {
    draft.summary = "Build small, fast command-line tools in Rust with care.".into();
    draft.context = "Teach ownership and error handling through small tools that are run and measured, comparing each choice with what the compiler and the operating system actually do.".into();
    draft.environment = "A terminal with a Rust toolchain and a text editor.".into();
    draft.outcome = "Build and ship a small command-line tool with argument parsing, clean error handling, tests and a release binary, and explain each design choice with evidence.".into();
    draft.topics = vec![
        topic("ownership", "foundations", true, &[]),
        topic("errors", "foundations", true, &["ownership"]),
        topic("clap", "mechanisms", true, &["errors"]),
        topic("io", "mechanisms", false, &["ownership"]),
        topic("testing", "production", true, &["clap"]),
        topic("release", "synthesis", true, &["testing", "io"]),
    ];
    draft
}

/// Walk the class through what publishing needs: the tutor has read it
/// back with nothing open, every source fetched and reachable, and the
/// learner's own read-through confirmed on the draft as it stands.
fn checked(conn: &Connection, id: &str) -> custom::CustomCourseView {
    let view = custom::get(conn, id).unwrap();
    custom::save_review(conn, id, &[]).unwrap();
    let sources: Vec<custom::SourceCheck> = view
        .draft
        .topics
        .iter()
        .flat_map(|topic| {
            topic
                .curriculum
                .primary_sources
                .iter()
                .map(move |url| custom::SourceCheck {
                    topic: topic.slug.clone(),
                    url: url.clone(),
                    state: "reachable".into(),
                    accepted: false,
                })
        })
        .collect();
    custom::save_sources(conn, id, &sources).unwrap();
    custom::mark_read(conn, id, true).unwrap()
}

#[test]
fn reading_again_keeps_what_was_settled() {
    let finding = |topic: &str, message: &str, status: &str, note: &str| custom::ReviewFinding {
        severity: "medium".into(),
        topic: topic.into(),
        message: message.into(),
        fix: String::new(),
        status: status.into(),
        note: note.into(),
        carried: false,
    };
    let previous = vec![
        finding(
            "clap",
            "The clap topic is too broad for one session and should be split",
            "fixed",
            "by the tutor: split it",
        ),
        finding(
            "",
            "The capstone stage has no elective for a learner who finishes early",
            "dismissed",
            "electives come later",
        ),
        finding(
            "io",
            "The io topic lacks a source on buffered reads",
            "open",
            "",
        ),
        finding(
            "errors",
            "Errors is taught before ownership is settled",
            "fixed",
            "by hand",
        ),
    ];
    let fresh = vec![
        finding(
            "clap",
            "The clap topic is too broad for a single session; split it",
            "open",
            "",
        ),
        finding(
            "",
            "The capstone stage still has no elective for someone who finishes early",
            "open",
            "",
        ),
        finding("testing", "Testing has no prerequisite on clap", "open", ""),
    ];
    let merged = custom::merge_reviews(&previous, fresh);
    let brief: Vec<(&str, &str, bool)> = merged
        .iter()
        .map(|f| (f.topic.as_str(), f.status.as_str(), f.carried))
        .collect();
    assert_eq!(
        brief,
        vec![
            ("clap", "fixed", false),
            ("", "dismissed", false),
            ("testing", "open", false),
            ("io", "open", true),
            ("errors", "fixed", true),
        ],
        "repeated objections keep their settlement; earlier ones not raised again are carried, open ones still open"
    );
    assert_eq!(merged[1].note, "electives come later");
    // Different topics never match, however alike the words.
    let other = custom::merge_reviews(
        &[finding(
            "io",
            "The topic is too broad for one session",
            "dismissed",
            "no",
        )],
        vec![finding(
            "clap",
            "The topic is too broad for one session",
            "open",
            "",
        )],
    );
    assert_eq!(other[0].status, "open");
    assert!(other[1].carried);
}

#[test]
fn publishing_needs_the_review_settled_the_sources_fetched_and_the_read_through_confirmed() {
    let (_file, conn) = fixture();
    let created = custom::create(&conn, &brief_titled("Rust checks"), "manual").unwrap();
    let saved = custom::save_draft(&conn, &created.id, complete(created.draft.clone())).unwrap();
    assert!(saved.issues.is_empty());
    assert_eq!(
        saved.checks.blockers,
        vec![
            "the tutor has not read the draft back",
            "the sources have not been fetched",
            "your own read-through of this version is not confirmed"
        ]
    );
    assert!(custom::publish(&conn, &created.id)
        .unwrap_err()
        .to_string()
        .contains("not read the draft back"));

    // A review with findings: every finding has to be settled.
    let finding = custom::ReviewFinding {
        severity: "high".into(),
        topic: "clap".into(),
        message: "too broad".into(),
        fix: "split it".into(),
        status: "fixed".into(),
        note: "stale".into(),
        carried: false,
    };
    let view = custom::save_review(&conn, &created.id, &[finding.clone(), finding]).unwrap();
    assert!(view.checks.reviewed && view.checks.review_current);
    assert_eq!(view.review[0].status, "open", "a saved review starts open");
    assert_eq!(view.checks.open_findings, 2);
    assert!(view.checks.blockers[0].contains("2 findings from the review still open"));
    let view = custom::resolve_finding(&conn, &created.id, 0, "fixed", "by hand").unwrap();
    let view2 =
        custom::resolve_finding(&conn, &created.id, 1, "dismissed", "not for this course").unwrap();
    assert_eq!(view.review[0].status, "fixed");
    assert_eq!(view2.review[1].note, "not for this course");
    assert_eq!(view2.checks.open_findings, 0);
    assert!(custom::resolve_finding(&conn, &created.id, 5, "fixed", "").is_err());
    assert!(custom::resolve_finding(&conn, &created.id, 0, "gone", "").is_err());

    // Sources: an unreachable one blocks until it is replaced or accepted;
    // a URL added after the fetch blocks until fetched.
    let mut sources: Vec<custom::SourceCheck> = view2
        .draft
        .topics
        .iter()
        .flat_map(|topic| {
            topic
                .curriculum
                .primary_sources
                .iter()
                .map(move |url| custom::SourceCheck {
                    topic: topic.slug.clone(),
                    url: url.clone(),
                    state: "reachable".into(),
                    accepted: false,
                })
        })
        .collect();
    sources[0].state = "unreachable".into();
    let view = custom::save_sources(&conn, &created.id, &sources).unwrap();
    assert_eq!(view.checks.pending_sources, 1);
    assert!(view
        .checks
        .blockers
        .iter()
        .any(|b| b.contains("1 unreachable source neither replaced nor accepted")));
    let url = sources[0].url.clone();
    let view = custom::accept_source(&conn, &created.id, &url, true).unwrap();
    assert_eq!(view.checks.pending_sources, 0);
    assert!(custom::accept_source(&conn, &created.id, "https://docs.rs/nothing", true).is_err());
    // Fetching again keeps the acceptance while the address is the same.
    let view = custom::save_sources(&conn, &created.id, &sources).unwrap();
    assert!(view.sources[0].accepted);
    let mut draft = view.draft.clone();
    draft.topics[0]
        .curriculum
        .primary_sources
        .push("https://docs.rs/anyhow/latest/anyhow/".into());
    let view = custom::save_draft(&conn, &created.id, draft).unwrap();
    assert_eq!(view.checks.unchecked_sources, 1);
    assert!(
        !view.checks.review_current,
        "the review was made on an earlier draft"
    );
    assert!(view
        .checks
        .blockers
        .iter()
        .any(|b| b.contains("1 source added since the last fetch")));
    let mut sources = sources.clone();
    sources.push(custom::SourceCheck {
        topic: "ownership".into(),
        url: "https://docs.rs/anyhow/latest/anyhow/".into(),
        state: "reachable".into(),
        accepted: false,
    });
    let view = custom::save_sources(&conn, &created.id, &sources).unwrap();
    assert_eq!(view.checks.unchecked_sources, 0);
    assert_eq!(
        view.checks.blockers,
        vec!["your own read-through of this version is not confirmed"]
    );

    // A replacement found for a dead source goes into every topic that cites it
    // and counts as reachable in the record of the fetch.
    let dead = view.sources[0].url.clone();
    let cited_before = view
        .draft
        .topics
        .iter()
        .flat_map(|t| t.curriculum.primary_sources.iter())
        .filter(|u| **u == dead)
        .count();
    assert!(cited_before >= 1);
    let view = custom::replace_source(
        &conn,
        &created.id,
        &dead,
        "https://docs.rs/clap/latest/clap/struct.Command.html",
    )
    .unwrap();
    assert!(!view
        .draft
        .topics
        .iter()
        .flat_map(|t| t.curriculum.primary_sources.iter())
        .any(|u| *u == dead));
    assert!(view.sources.iter().all(|s| s.url != dead));
    assert!(view.sources.iter().any(|s| s.url
        == "https://docs.rs/clap/latest/clap/struct.Command.html"
        && s.state == "reachable"));
    assert_eq!(
        view.checks.unchecked_sources, 0,
        "the replacement was fetched before it was chosen"
    );
    assert!(custom::replace_source(
        &conn,
        &created.id,
        "https://docs.rs/none",
        "https://docs.rs/other"
    )
    .is_err());

    // The read-through is on this exact draft: an edit after it needs another.
    let view = custom::mark_read(&conn, &created.id, true).unwrap();
    assert!(view.checks.read && view.checks.blockers.is_empty());
    let mut draft = view.draft.clone();
    draft.summary = "Build small, fast and careful command-line tools in Rust.".into();
    let view = custom::save_draft(&conn, &created.id, draft).unwrap();
    assert!(!view.checks.read);
    assert!(custom::publish(&conn, &created.id)
        .unwrap_err()
        .to_string()
        .contains("read-through"));
    let view = custom::mark_read(&conn, &created.id, true).unwrap();
    assert!(view.checks.blockers.is_empty());
    let published = custom::publish(&conn, &created.id).unwrap();
    assert_eq!(published.status, "published");
}

#[test]
fn a_brief_becomes_a_draft_that_publishes_into_the_catalog_and_teaches_like_any_course() {
    let (_file, conn) = fixture();
    let created = custom::create(&conn, &brief_titled("Rust for CLI tools"), "manual").unwrap();
    assert_eq!(created.id, "custom-rust-for-cli-tools");
    assert_eq!(created.status, "draft");
    assert_eq!(created.draft.entry_points.len(), 4);
    assert!(
        !created.issues.is_empty(),
        "a blank draft is not ready to publish"
    );
    assert!(
        catalog::course(&created.id).is_none(),
        "a draft is not a course yet"
    );
    assert!(custom::publish(&conn, &created.id)
        .unwrap_err()
        .to_string()
        .contains("not ready"));

    let saved = custom::save_draft(&conn, &created.id, complete(created.draft.clone())).unwrap();
    assert!(saved.issues.is_empty(), "{:?}", saved.issues);
    checked(&conn, &created.id);
    let published = custom::publish(&conn, &created.id).unwrap();
    assert_eq!(
        (published.status.as_str(), published.version),
        ("published", 1)
    );

    // The course is in the catalog, engineering-kind, with its own prompt.
    let course = catalog::course(&created.id).expect("registered");
    assert_eq!(course.label, "Rust for CLI tools");
    assert_eq!(course.kind, catalog::SubjectKind::Engineering);
    assert!(course
        .prompt
        .contains("PROMPT PROFILE: custom.custom-rust-for-cli-tools.v1"));
    assert!(catalog::engineering_ids().contains(&course.id));
    assert!(catalog::all().iter().any(|c| c.id == course.id));
    assert!(classroom::prompt_contracts_are_isolated().is_ok());

    // Its topics are concepts of the course, with prefixed slugs.
    let concepts = db::all_concepts(&conn, &created.id).unwrap();
    assert_eq!(concepts.len(), 6);
    assert!(concepts
        .iter()
        .any(|c| c.slug == "custom-rust-for-cli-tools-ownership"));
    assert_eq!(mastery::overview(&conn, &created.id).unwrap().len(), 6);

    // It has a program with the brief's tutor, and the desk lists it.
    let program = classroom::program_row(&conn, &created.id).unwrap();
    assert_eq!(
        (program.agent.as_str(), program.model.as_str()),
        ("claude", "opus")
    );
    assert!(classroom::program_views(&conn, "2026-09-12")
        .unwrap()
        .iter()
        .any(|view| view.subject_id == created.id));

    // Enrollment works from the registry: options, a manual entry, an
    // accepted path, and a planned lesson on the first topic.
    let options = enrollment::options(&created.id).unwrap();
    assert_eq!(options.familiarity_options.len(), 6);
    assert_eq!(options.entry_points.len(), 4);
    let mut configuration = options.default_configuration;
    configuration.entry = EntryChoice::Manual {
        entry_point: "foundations".into(),
        familiar_competencies: vec![],
    };
    let draft = enrollment::save_draft(
        &conn,
        &SaveEnrollmentDraft {
            id: None,
            expected_revision: None,
            course: options.course.clone(),
            configuration,
        },
    )
    .unwrap();
    let recommendation = placement::recommend(&conn, &draft.id, draft.revision).unwrap();
    let path = classes::accept(
        &conn,
        &AcceptPath {
            draft_id: draft.id,
            expected_revision: draft.revision,
            recommendation_id: recommendation.id,
        },
        "2026-09-12",
    )
    .unwrap();
    assert_eq!(path.recommendation.course.course_id, created.id);
    // With a study time the class activates like any other, and a lesson
    // can be planned on the first topic.
    classroom::upsert_slot(
        &conn,
        &UpsertClassroomSlotInput {
            id: None,
            subject_id: created.id.clone(),
            hour: 7,
            minute: 30,
            weekdays: vec![1, 2, 3, 4, 5],
            enabled: true,
            durations: Default::default(),
            starts: Default::default(),
        },
    )
    .unwrap();
    classroom::configure_program(
        &conn,
        &ConfigureClassroomInput {
            subject_id: created.id.clone(),
            enabled: true,
            agent: "claude".into(),
            model: "opus".into(),
            custom_agent_bin: String::new(),
            session_minutes: 30,
            start_level: None,
            target_level: None,
            weekly_minutes: None,
        },
        "2026-09-12",
    )
    .unwrap();
    let program = classroom::program_row(&conn, &created.id).unwrap();
    assert!(program.enabled);
    let session = engineering::plan(&conn, &program, None, None, "2026-09-12", false).unwrap();
    let chosen = engineering::selection(&session).unwrap();
    assert!(
        chosen.slug.starts_with("custom-rust-for-cli-tools-"),
        "{}",
        chosen.slug
    );

    // The placement check is not available until a bank exists; the other
    // starting points are.
    assert!(placement::bank(&created.id).is_err());

    // A published course survives a restart: loading registers it again.
    catalog::unregister_custom(&created.id);
    assert!(catalog::course(&created.id).is_none());
    assert_eq!(custom::load_published(&conn).unwrap(), 1);
    assert!(catalog::course(&created.id).is_some());
    assert_eq!(
        enrollment::course_snapshot(&created.id).unwrap().0,
        options.course
    );
}

#[test]
fn republishing_bumps_the_version_keeps_taught_topics_and_drops_untaught_ones() {
    let (_file, conn) = fixture();
    let created = custom::create(&conn, &brief_titled("Rust again"), "tutor").unwrap();
    assert_eq!(created.id, "custom-rust-again");
    custom::save_draft(&conn, &created.id, complete(created.draft.clone())).unwrap();
    checked(&conn, &created.id);
    custom::publish(&conn, &created.id).unwrap();
    let before = enrollment::course_snapshot(&created.id).unwrap().0;
    // One topic was taught; another is removed in the second version.
    conn.execute(
        "UPDATE concepts SET times_picked=1 WHERE slug='custom-rust-again-io'",
        [],
    )
    .unwrap();
    let mut second = custom::get(&conn, &created.id).unwrap().draft;
    second
        .topics
        .retain(|t| t.slug != "io" && t.slug != "release");
    second
        .topics
        .push(topic("ship", "synthesis", true, &["testing"]));
    second
        .topics
        .push(topic("profiling", "production", false, &["clap"]));
    second.entry_points[0].label = "Getting started".into();
    custom::save_draft(&conn, &created.id, second).unwrap();
    checked(&conn, &created.id);
    let published = custom::publish(&conn, &created.id).unwrap();
    assert_eq!(published.version, 2);
    let course = catalog::course(&created.id).unwrap();
    assert_eq!(course.version, "v2");
    assert_eq!(course.entry_points[0].label, "Getting started");
    let slugs: Vec<String> = db::all_concepts(&conn, &created.id)
        .unwrap()
        .into_iter()
        .map(|c| c.slug)
        .collect();
    assert!(
        slugs.contains(&"custom-rust-again-io".to_string()),
        "a taught topic stays"
    );
    assert!(
        !slugs.contains(&"custom-rust-again-release".to_string()),
        "an untaught one leaves"
    );
    assert!(slugs.contains(&"custom-rust-again-ship".to_string()));
    assert_ne!(
        enrollment::course_snapshot(&created.id)
            .unwrap()
            .0
            .fingerprint,
        before.fingerprint,
        "a new version is a new snapshot"
    );
}

#[test]
fn a_class_file_round_trips_and_a_draft_can_be_deleted_but_a_published_class_cannot() {
    let (_file, conn) = fixture();
    let created = custom::create(&conn, &brief_titled("Rust to share"), "manual").unwrap();
    custom::save_draft(&conn, &created.id, complete(created.draft.clone())).unwrap();
    let file = custom::export(&conn, &created.id).unwrap();
    assert_eq!(file.format, custom::CLASS_FILE_FORMAT);
    let imported = custom::import(&conn, file).unwrap();
    assert_ne!(imported.id, created.id, "an import is a new class");
    assert_eq!(imported.id, "custom-rust-to-share-2");
    assert_eq!(imported.origin, "import");
    assert_eq!(imported.draft.topics.len(), 6);
    assert!(imported.issues.is_empty());
    assert_eq!(custom::list(&conn).unwrap().len(), 2);

    custom::delete_draft(&conn, &imported.id).unwrap();
    assert_eq!(custom::list(&conn).unwrap().len(), 1);
    checked(&conn, &created.id);
    custom::publish(&conn, &created.id).unwrap();
    assert!(custom::delete_draft(&conn, &created.id).is_err());
}

#[test]
fn a_written_bank_turns_on_the_placement_check_and_a_disputed_key_is_set_aside() {
    use principia_desk_lib::class_builder::{bank_from_written, WrittenQuestion};
    let (_file, conn) = fixture();
    let created = custom::create(&conn, &brief_titled("Rust with questions"), "manual").unwrap();
    let draft = complete(created.draft.clone());
    custom::save_draft(&conn, &created.id, draft.clone()).unwrap();
    // No bank: the check is unavailable, the two other routes work.
    checked(&conn, &created.id);
    custom::publish(&conn, &created.id).unwrap();
    assert!(!placement::has_bank(&created.id));
    assert!(
        !enrollment::options(&created.id)
            .unwrap()
            .diagnostic_available
    );

    // Three questions per stage on core topics of that stage.
    let written = |topic: &str| WrittenQuestion {
        topic: topic.into(),
        label: "A skill".into(),
        prompt: "Which one?".into(),
        choices: vec!["one".into(), "two".into(), "three".into(), "four".into()],
        answer: "three".into(),
        explanation: "Because the third is right.".into(),
        source: "https://doc.rust-lang.org/book/".into(),
    };
    let mut questions = Vec::new();
    for topic in [
        "ownership",
        "errors",
        "ownership",
        "clap",
        "clap",
        "clap",
        "testing",
        "testing",
        "testing",
        "release",
        "release",
        "release",
    ] {
        questions.push(written(topic));
    }
    let bank =
        bank_from_written(&custom::get(&conn, &created.id).unwrap().draft, questions).unwrap();
    assert_eq!(bank.questions.len(), 12);
    let view = custom::save_bank(&conn, &created.id, &bank).unwrap();
    assert!(view.bank.is_some());
    assert!(placement::has_bank(&created.id));
    assert!(
        enrollment::options(&created.id)
            .unwrap()
            .diagnostic_available
    );
    let usable = placement::bank(&created.id).unwrap();
    assert_eq!(usable.questions.len(), 12);
    assert_eq!(
        usable.questions[0].competency,
        format!("{}-ownership", created.id)
    );

    // The placement check runs on it: a diagnostic entry, a started round.
    let options = enrollment::options(&created.id).unwrap();
    let mut configuration = options.default_configuration;
    configuration.entry = EntryChoice::Diagnostic;
    let enrollment_draft = enrollment::save_draft(
        &conn,
        &SaveEnrollmentDraft {
            id: None,
            expected_revision: None,
            course: options.course.clone(),
            configuration,
        },
    )
    .unwrap();
    let round = placement::start(
        &conn,
        &enrollment_draft.id,
        enrollment_draft.revision,
        false,
    )
    .unwrap();
    assert_eq!(
        round.questions.len(),
        12,
        "every stage is sampled three times"
    );

    // A disputed key is set aside from the next sample and the bank shows why.
    let disputed = usable.questions[0].id.clone();
    let view = custom::void_question(
        &conn,
        &created.id,
        &disputed,
        "the key contradicts the book",
    )
    .unwrap();
    let stored = view.bank.unwrap();
    let voided = stored.questions.iter().find(|q| q.id == disputed).unwrap();
    assert!(voided.voided);
    assert_eq!(voided.void_reason, "the key contradicts the book");
    assert!(
        placement::bank(&created.id).is_err(),
        "with one stage short of three, no new check can start until the bank is corrected"
    );
    assert!(!placement::has_bank(&created.id));

    // Loading after a restart registers the bank again, still without the voided question.
    catalog::unregister_custom(&created.id);
    custom::load_published(&conn).unwrap();
    assert!(catalog::custom_bank(&created.id).is_some());
}
