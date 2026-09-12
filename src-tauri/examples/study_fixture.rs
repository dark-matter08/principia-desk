//! Native QA harness for the shared-runtime engineering adapter: publish a
//! bundled reference lesson into a class's planned session (or fail that
//! preparation on request) without a provider, webview or OS enforcement.
//! Run against a disposable QA database while the app is closed.
use principia_desk_lib::{
    classroom::{self, StoredEngineeringLesson, StoredQuestion},
    db,
    domain::sessions::{self, PreparedLesson, Status},
    generator,
    subjects::engineering,
};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        return Err(
            "usage: study_fixture DISPOSABLE_QA_DATABASE SUBJECT_ID [--fail|--skip]".into(),
        );
    }
    let fail = args.iter().any(|arg| arg == "--fail");
    let skip = args.iter().any(|arg| arg == "--skip");
    let path = PathBuf::from(&args[0]);
    let subject = args[1].as_str();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let conn = db::open(&path)?;
    let program = classroom::program_row(&conn, subject)?;
    if skip {
        // Dispose of the class's resumable session through the real skip path
        // so its projections and appointment resolve exactly as in the app.
        let resumable = if program.kind == "language" {
            principia_desk_lib::subjects::language::resumable(&conn, subject)?
        } else {
            engineering::resumable(&conn, subject)?
        };
        let Some(session) = resumable else {
            println!(
                "{}",
                serde_json::json!({"skipped": null, "note": "no resumable session"})
            );
            return Ok(());
        };
        engineering::skip(&conn, &session.id)?;
        println!(
            "{}",
            serde_json::json!({"skipped": session.id.0, "status": "skipped"})
        );
        return Ok(());
    }
    if program.kind == "language" {
        // Language lessons publish their curated seed without a tutor call.
        use principia_desk_lib::subjects::language;
        let session = match language::resumable(&conn, subject)? {
            Some(session) => session,
            None => language::plan(&conn, &program, None, None, &today, false)?,
        };
        let published = language::publish_curated(&conn, &session.id)?;
        println!(
            "{}",
            serde_json::json!({"session_id": published.id.0, "status": published.status, "adapter": "language"})
        );
        return Ok(());
    }
    let session = match engineering::resumable(&conn, subject)? {
        Some(session) => session,
        None => engineering::plan(&conn, &program, None, None, &today, false)?,
    };
    if !matches!(session.status, Status::Planned | Status::Preparing) {
        println!(
            "{}",
            serde_json::json!({"session_id": session.id.0, "status": session.status, "note": "already prepared"})
        );
        return Ok(());
    }
    let now = chrono::Utc::now();
    if sessions::preparation(&conn, &session.id)?.status == "failed" {
        sessions::retry_preparation(&conn, &session.id, now)?;
    }
    let lease = sessions::claim_preparation(&conn, &session.id, now, 600)?
        .ok_or("another worker holds this preparation lease")?;
    if fail {
        sessions::fail_preparation(
            &conn,
            &lease,
            "QA fixture: the tutor provider was unavailable.",
            now,
        )?;
        println!(
            "{}",
            serde_json::json!({"session_id": session.id.0, "status": "preparing", "preparation": "failed"})
        );
        return Ok(());
    }
    let chosen = engineering::selection(&session)?;
    let bundled = generator::pick_fallback(subject, &chosen.title);
    let questions: Vec<StoredQuestion> = bundled
        .questions
        .iter()
        .filter(|question| question.kind == "mcq")
        .take(5)
        .enumerate()
        .map(|(index, question)| {
            let choices = question.choices.clone().unwrap_or_default();
            let correct_index = choices
                .iter()
                .position(|choice| choice.trim() == question.correct_answer.trim())
                .ok_or_else(|| format!("bundled question {} has no matching answer", index + 1))?;
            Ok(StoredQuestion {
                id: index + 1,
                prompt: question.prompt.clone(),
                choices,
                correct_index,
                explanation: question.explanation.clone(),
                section: "Reference lesson".into(),
                learning_objective: bundled
                    .key_takeaways
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| question.prompt.clone()),
            })
        })
        .collect::<Result<_, String>>()?;
    if questions.is_empty() {
        return Err(format!(
            "bundled lesson {} has no multiple-choice questions",
            bundled.slug
        )
        .into());
    }
    let stored = StoredEngineeringLesson {
        concept_id: chosen.concept_id,
        concept_title: chosen.title.clone(),
        category: chosen.category.clone(),
        title: format!("{} (QA reference lesson)", bundled.title),
        markdown: bundled.markdown,
        resources: bundled.resources,
        review_notes: vec![],
        research_note: None,
        level: "standard".into(),
        questions,
        exercise: bundled.exercise,
        source: "qa-fixture".into(),
        path: None,
    };
    sessions::publish_preparation(
        &conn,
        &lease,
        &PreparedLesson {
            title: stored.title.clone(),
            body: serde_json::to_value(&stored)?,
            provenance: serde_json::json!({"kind":"qa_fixture","bundled_slug":bundled.slug,"note":"bundled reference lesson published for desktop QA; not generated content"}),
        },
        now,
    )?;
    let session = sessions::get(&conn, &session.id)?;
    let round = engineering::ensure_check_round(&conn, &session)?;
    println!(
        "{}",
        serde_json::json!({"session_id": session.id.0, "status": session.status, "round_id": round.id.0, "title": stored.title, "concept": chosen.slug})
    );
    Ok(())
}
