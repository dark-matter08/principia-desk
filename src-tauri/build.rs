fn main() {
    generate_catalog();
    tauri_build::build()
}

fn generate_catalog() {
    use serde_json::Value;
    use std::{env, fs, path::PathBuf};
    println!("cargo:rerun-if-changed=seed/catalog.json");
    let manifest: Value = serde_json::from_str(
        &fs::read_to_string("seed/catalog.json").expect("read catalog manifest"),
    )
    .expect("valid catalog JSON");
    assert_eq!(manifest["schema_version"], 1);
    let courses = manifest["courses"].as_array().expect("catalog courses");
    let mut generated = String::from("pub const COURSES: &[CourseDefinition] = &[\n");
    let mut engineering = Vec::new();
    for course in courses {
        let id = course["id"].as_str().expect("course id");
        let kind = match course["kind"].as_str().expect("course kind") {
            "engineering" => {
                engineering.push(id);
                "Engineering"
            }
            "language" => "Language",
            other => panic!("unknown course kind: {other}"),
        };
        generated.push_str(&format!("CourseDefinition {{ kind: SubjectKind::{kind},\n"));
        for field in [
            "id",
            "course_id",
            "label",
            "native_label",
            "short_code",
            "title",
            "summary",
            "version",
            "context",
            "outcome",
            "environment",
        ] {
            generated.push_str(&format!(
                "{field}: {:?},\n",
                course[field].as_str().expect(field)
            ));
        }
        generated.push_str(&format!(
            "prompt_profile: {:?},\n",
            format!("classroom.{id}")
        ));
        let prompt = course["prompt_path"].as_str().expect("prompt path");
        assert_eq!(prompt, format!("prompts/classroom/{id}.txt"));
        println!("cargo:rerun-if-changed={prompt}");
        generated.push_str(&format!(
            "prompt: include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), {:?})),\n",
            format!("/{prompt}")
        ));
        for field in [
            "prerequisite_courses",
            "source_hosts",
            "capabilities",
            "reference_lessons",
        ] {
            let values = course[field]
                .as_array()
                .expect(field)
                .iter()
                .map(|value| format!("{:?}", value.as_str().expect(field)))
                .collect::<Vec<_>>()
                .join(",");
            generated.push_str(&format!("{field}: &[{values}],\n"));
        }
        generated.push_str("bundled_lessons: &[\n");
        for lesson in course["reference_lessons"]
            .as_array()
            .expect("reference lessons")
        {
            let slug = lesson.as_str().expect("reference slug");
            assert!(
                !slug.is_empty()
                    && slug
                        .bytes()
                        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == b'-')
            );
            let path = format!("seed/fallback_courses/{slug}.json");
            println!("cargo:rerun-if-changed={path}");
            generated.push_str(&format!(
                "include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), {:?})),\n",
                format!("/{path}")
            ));
        }
        generated.push_str("],\n");
        generated.push_str("entry_points: &[\n");
        for point in course["entry_points"].as_array().expect("entry points") {
            generated.push_str(&format!(
                "EntryPoint {{ id: {:?}, label: {:?} }},\n",
                point["id"].as_str().expect("entry id"),
                point["label"].as_str().expect("entry label")
            ));
        }
        generated.push_str("] },\n");
    }
    generated.push_str("];\n");
    generated.push_str(&format!(
        "pub const ENGINEERING_IDS: &[&str] = &{engineering:?};\n"
    ));
    let generated_path =
        PathBuf::from(env::var_os("OUT_DIR").expect("Cargo output directory")).join("catalog.rs");
    fs::write(generated_path, generated).expect("write generated catalog");
}
