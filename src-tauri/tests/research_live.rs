//! Live end-to-end proof that generated courses are grounded in fetched
//! documentation. Ignored by default: these tests use the network and the
//! configured provider's API quota.
//!
//! Run with: `cargo test --test research_live -- --ignored --nocapture`

use principia_desk_lib::{db, generator, research};

struct SeedConcept {
    title: String,
    category: String,
    focus: String,
    brief: db::CurriculumBrief,
}

fn seed_brief(slug_hint: &str) -> SeedConcept {
    let seed: serde_json::Value =
        serde_json::from_str(include_str!("../seed/concepts.json")).expect("seed parses");
    let concepts = seed.as_array().expect("seed is an array of concepts");
    let concept = concepts
        .iter()
        .find(|concept| {
            concept
                .get("slug")
                .and_then(|slug| slug.as_str())
                .is_some_and(|slug| slug.contains(slug_hint))
        })
        .expect("seed contains the requested concept");
    let brief: db::CurriculumBrief = serde_json::from_value(
        concept
            .get("curriculum")
            .expect("concept has a curriculum brief")
            .clone(),
    )
    .expect("brief parses");
    brief.validate().expect("seeded brief is valid");
    SeedConcept {
        title: concept["title"].as_str().unwrap_or_default().to_string(),
        category: concept["category"].as_str().unwrap_or_default().to_string(),
        focus: concept["focus"].as_str().unwrap_or_default().to_string(),
        brief,
    }
}

#[tokio::test]
#[ignore = "hits the live network"]
async fn curated_primary_sources_are_reachable_and_readable() {
    let concept = seed_brief("js-event-loop");
    let topic = research::course_topic(&concept.title, &concept.category);
    let researcher = research::Researcher::new();
    let sources = researcher
        .gather(&concept.focus, &topic, &concept.brief.primary_sources, 5)
        .await
        .sources;

    println!("retrieved {} source(s) for {topic}", sources.len());
    for source in &sources {
        println!(
            "  {:>6} words  {}  {}",
            source.words, source.host, source.url
        );
    }
    assert!(
        sources.len() >= 2,
        "a curated concept must retrieve at least two readable primary sources"
    );
    let block = research::format_source_material(&sources);
    assert!(block.contains("RETRIEVED SOURCE MATERIAL"));
}

#[tokio::test]
#[ignore = "hits primary documentation over the live network; no provider calls"]
async fn systems_and_shell_courses_retrieve_subject_specific_readable_sources() {
    let researcher = research::Researcher::new();
    for slug in ["cap-theorem", "lb-quoting", "bs-error-handling"] {
        let concept = seed_brief(slug);
        let topic = research::course_topic(&concept.title, &concept.category);
        let sources = researcher
            .gather(&concept.focus, &topic, &concept.brief.primary_sources, 3)
            .await
            .sources;
        println!(
            "{}: {} readable primary sources",
            concept.focus,
            sources.len()
        );
        assert!(
            sources.len() >= 2,
            "{} needs at least two readable primary references",
            concept.focus
        );
        for source in sources {
            assert!(research::is_source_for(&concept.focus, &source.url));
            assert!(!source.host.contains("mozilla"));
            assert!(source.words >= 120);
        }
    }
}

/// The full path: fetch documentation, generate through the configured
/// provider, then verify the course cites and links only reachable sources.
#[tokio::test]
#[ignore = "hits the live network and the provider API"]
async fn generated_course_cites_retrieved_sources_with_reachable_links() {
    let concept = seed_brief("js-event-loop");
    let generator = generator::Generator::new(
        "claude".into(),
        None,
        std::env::temp_dir().join("sdr-research-live"),
        std::sync::Arc::new(std::sync::Mutex::new("deepseek-chat".into())),
        std::sync::Arc::new(std::sync::Mutex::new("deepseek".into())),
        std::sync::Arc::new(std::sync::Mutex::new(String::new())),
        Default::default(),
    );

    let (course, source) = generator
        .generate_course(generator::CourseRequest {
            title: &concept.title,
            category: &concept.category,
            dossier: "",
            focus: &concept.focus,
            curriculum: &concept.brief,
            budget: generator::LessonBudget::default(),
        })
        .await
        .expect("grounded course generation should succeed");

    println!("provider: {source}");
    println!("words: {}", course.markdown.split_whitespace().count());
    println!("reading list:");
    for resource in &course.resources {
        println!("  {}", resource.url);
    }

    generator::validate_generated_course(&course).expect("course passes the quality gate");
    assert!(
        course.resources.len() >= 3,
        "the learner needs a real reading list, got {}",
        course.resources.len()
    );
    let credible = course
        .resources
        .iter()
        .filter(|resource| research::is_credible_source(&resource.url))
        .count();
    assert!(
        credible >= 2,
        "at least two reading-list entries must be primary documentation, got {credible}"
    );
    let inline = course
        .resources
        .iter()
        .filter(|resource| course.markdown.contains(resource.url.trim_end_matches('/')))
        .count();
    assert!(
        inline >= 2,
        "the course body must attribute at least two sources inline, got {inline}"
    );
}
