//! The class builder's tutor calls against a real runner. Ignored by
//! default: it needs a model on the machine. Run with
//! `cargo test --test custom_tutor_live -- --ignored --nocapture`,
//! optionally with `PRINCIPIA_LIVE_AGENT=ollama PRINCIPIA_LIVE_MODEL=qwen2.5:7b`.

use principia_desk_lib::{
    domain::custom::{self, CourseBrief},
    execution_log::Feed,
    generator::Generator,
};
use std::sync::{Arc, Mutex};

fn generator(agent: &str, model: &str) -> Generator {
    let scratch =
        std::env::temp_dir().join(format!("principia-live-{:016x}", rand::random::<u64>()));
    let (tx, _rx) = tokio::sync::broadcast::channel(64);
    Generator::new(
        "claude".into(),
        None,
        scratch,
        Arc::new(Mutex::new(model.into())),
        Arc::new(Mutex::new(agent.into())),
        Arc::new(Mutex::new(String::new())),
        Feed::new(tx),
    )
}

#[tokio::test]
#[ignore]
async fn the_tutor_drafts_reviews_and_writes_questions_for_a_brief() {
    let agent = std::env::var("PRINCIPIA_LIVE_AGENT").unwrap_or_else(|_| "ollama".into());
    let model = std::env::var("PRINCIPIA_LIVE_MODEL").unwrap_or_else(|_| "qwen2.5:7b".into());
    let generator = generator(&agent, &model);
    let brief = CourseBrief {
        title: "Rust for CLI tools".into(),
        outcome: "Build and ship a small command-line tool with argument parsing, clean error handling, tests and a release binary".into(),
        background: "Comfortable in Python; never used a systems language".into(),
        trusted_hosts: vec!["doc.rust-lang.org".into(), "docs.rs".into()],
        agent: agent.clone(),
        model: model.clone(),
        custom_agent_bin: String::new(),
    };
    let started = std::time::Instant::now();
    let (draft, issues, source) = generator
        .draft_custom_course("custom-rust-for-cli-tools", &brief)
        .await
        .expect("a draft");
    eprintln!(
        "draft from {source} in {:?}: {} topics, {} issues",
        started.elapsed(),
        draft.topics.len(),
        issues.len()
    );
    for issue in &issues {
        eprintln!("  issue {}: {}", issue.at, issue.message);
    }
    for topic in &draft.topics {
        eprintln!(
            "  [{}] {} ({}) <- {:?} :: {}",
            topic.curriculum.phase,
            topic.title,
            topic.slug,
            topic.prereqs,
            topic.curriculum.primary_sources.join(" ")
        );
    }
    assert!(!draft.topics.is_empty(), "the tutor drafted no topics");
    assert_eq!(draft.id, "custom-rust-for-cli-tools");
    assert_eq!(draft.entry_points.len(), 4);
    eprintln!("validator: {:?}", custom::validate(&draft).len());

    let (findings, _) = generator
        .review_custom_course(&brief, &draft, &[])
        .await
        .expect("a review");
    eprintln!("review: {} finding(s)", findings.len());
    for finding in &findings {
        eprintln!(
            "  [{}] {} :: {} => {}",
            finding.severity, finding.topic, finding.message, finding.fix
        );
    }

    // The bank is held to its own shape, not to the draft's remaining issues.
    let started = std::time::Instant::now();
    match generator.write_custom_course_bank(&brief, &draft).await {
        Ok((bank, _)) => {
            eprintln!(
                "bank in {:?}: {} questions",
                started.elapsed(),
                bank.questions.len()
            );
            for question in &bank.questions {
                eprintln!(
                    "  [{}] {} -> {} ({}) {}",
                    question.entry_point,
                    question.prompt,
                    question.answer,
                    question.competency,
                    question.source
                );
            }
            assert_eq!(bank.questions.len(), 12);
        }
        Err(error) => eprintln!("bank not written after one correction: {error}"),
    }
}

/// The read has to close: a full first read, then a confirmation read
/// that is given the earlier findings as settled and must not start over.
/// Prints both lists; asserts the confirmation read is not a second batch.
#[tokio::test]
#[ignore]
async fn a_confirmation_read_does_not_bring_a_second_batch() {
    use principia_desk_lib::{
        db::CurriculumBrief,
        domain::custom::{CourseDraft, DraftTopic},
    };
    let agent = std::env::var("PRINCIPIA_LIVE_AGENT").unwrap_or_else(|_| "ollama".into());
    let model = std::env::var("PRINCIPIA_LIVE_MODEL").unwrap_or_else(|_| "qwen2.5:7b".into());
    let generator = generator(&agent, &model);
    let brief = CourseBrief {
        title: "Rust for CLI tools".into(),
        outcome: "Build and ship a small command-line tool with argument parsing, clean error handling, tests and a release binary".into(),
        background: "Comfortable in Python; never used a systems language".into(),
        trusted_hosts: vec!["doc.rust-lang.org".into(), "docs.rs".into()],
        agent: agent.clone(),
        model: model.clone(),
        custom_agent_bin: String::new(),
    };
    let topic = |slug: &str, title: &str, phase: &str, core: bool, prereqs: &[&str]| {
        DraftTopic {
        slug: slug.into(),
        title: title.into(),
        category: "fundamentals".into(),
        prereqs: prereqs.iter().map(|p| p.to_string()).collect(),
        curriculum: CurriculumBrief {
            phase: phase.into(),
            core,
            learner_outcome: format!("Explain {title} with a worked example and a measured result of your own"),
            mechanisms: vec!["the mechanism underneath".into(), "what the tool does with it".into()],
            production_scenario: format!("A tool in daily use meets {title} on a busy day and someone has to fix it before lunch"),
            misconceptions: vec!["it is not magic; there is a rule and it can be observed".into()],
            evidence: "A trace showing the mechanism at work in the tool".into(),
            artifact: "A short note with the trace and the fix applied".into(),
            primary_sources: vec!["https://doc.rust-lang.org/book/".into(), "https://docs.rs/clap/latest/clap/".into()],
            related_concepts: vec![],
        },
    }
    };
    let draft = custom::normalize(CourseDraft {
        id: "custom-rust-for-cli-tools".into(),
        label: "Rust for CLI tools".into(),
        native_label: "systems programming".into(),
        short_code: "RCT".into(),
        title: "Rust for command-line tools".into(),
        summary: "Build small, fast command-line tools in Rust with care.".into(),
        context: "Teach ownership and error handling through small tools that are run and measured, comparing each choice with what the compiler and the operating system do.".into(),
        outcome: brief.outcome.clone(),
        environment: "A terminal with a Rust toolchain and a text editor.".into(),
        source_hosts: brief.trusted_hosts.clone(),
        entry_points: custom::default_stages(),
        topics: vec![
            topic("ownership", "Ownership and borrowing", "foundations", true, &[]),
            topic("errors", "Errors as values", "foundations", true, &["ownership"]),
            topic("clap", "Parsing arguments with clap", "mechanisms", true, &["errors"]),
            topic("io", "Reading and writing files", "mechanisms", false, &["ownership"]),
            topic("testing", "Tests that run in a second", "production", true, &["clap"]),
            topic("release", "Building a release binary", "synthesis", true, &["testing", "io"]),
        ],
    });
    let started = std::time::Instant::now();
    let (first, _) = generator
        .review_custom_course(&brief, &draft, &[])
        .await
        .expect("a first read");
    eprintln!(
        "first read in {:?}: {} finding(s)",
        started.elapsed(),
        first.len()
    );
    for f in &first {
        eprintln!("  [{}] {} :: {}", f.severity, f.topic, f.message);
    }
    let settled: Vec<_> = first
        .iter()
        .cloned()
        .map(|mut f| {
            f.status = "dismissed".into();
            f.note = "decided".into();
            f
        })
        .collect();
    let started = std::time::Instant::now();
    let (second, _) = generator
        .review_custom_course(&brief, &draft, &settled)
        .await
        .expect("a confirmation read");
    eprintln!(
        "confirmation read in {:?}: {} finding(s)",
        started.elapsed(),
        second.len()
    );
    for f in &second {
        eprintln!("  [{}] {} :: {}", f.severity, f.topic, f.message);
    }
    let merged = custom::merge_reviews(&settled, second.clone());
    let open = merged.iter().filter(|f| f.status == "open").count();
    eprintln!("after merge: {} finding(s), {open} open", merged.len());
    assert!(
        open <= 2,
        "a confirmation read with nothing changed should raise almost nothing; it raised {open}"
    );
}
