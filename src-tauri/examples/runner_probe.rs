//! Exercise the production runner transport against an explicit QA database.
//! Inventory is read-only; --live sends a bounded connection prompt for saved models.
use principia_desk_lib::agents::{self, configuration, Route, RunRequest, Runner, RunnerId};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let path = PathBuf::from(args.first().ok_or(
        "usage: runner_probe QA_DATABASE [--live] [RUNNER_ID] [--model=ID] [--scratch-root=PATH] [--lesson-smoke]",
    )?);
    let live = args.iter().any(|arg| arg == "--live");
    let selected = args.iter().skip(1).find_map(|arg| RunnerId::parse(arg));
    let model_override = args.iter().find_map(|arg| arg.strip_prefix("--model="));
    let lesson_smoke = args.iter().any(|arg| arg == "--lesson-smoke");
    if model_override.is_some() && selected.is_none() {
        return Err("--model requires a runner ID".into());
    }
    let conn =
        rusqlite::Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let scratch_root = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--scratch-root="))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let scratch = scratch_root.join(format!(
        "principia-runner-probe-{:032x}",
        rand::random::<u128>()
    ));
    let runner = Runner {
        claude_bin: "claude".into(),
        codex_bin: Some("codex".into()),
        scratch_dir: scratch.clone(),
        database: Some(path.clone()),
        feed: Default::default(),
    };
    for id in RunnerId::ALL {
        if selected.is_some_and(|selected| selected != id) {
            continue;
        }
        let mut setup = configuration::read(&conn, id)?;
        if let Some(model) = model_override {
            setup.models = vec![model.to_string()];
        }
        let info = runner.info(id, &setup.custom_command).await;
        println!(
            "{}",
            serde_json::json!({"runner":id,"available":info.available,"detail":info.detail,"models":setup.models})
        );
        if !live || !info.available {
            continue;
        }
        for model in setup.models {
            let route = Route {
                runner: id,
                model: agents::effective_model(id, &model),
                custom_command: setup.custom_command.clone(),
            };
            if lesson_smoke {
                let mut request = RunRequest::new(route, "Return a JSON object with keys title and explanation. Explain Linux file permissions in 80 words or fewer.");
                request.json = true;
                request.timeout = std::time::Duration::from_secs(120);
                request.purpose = "lesson-smoke".into();
                match runner.run(&request, None).await {
                    Ok(result) => println!(
                        "{}",
                        serde_json::json!({"runner":id,"model":result.model,"valid_json":result.json.as_ref().is_some_and(|v| v["title"].is_string() && v["explanation"].is_string()),"usage":result.usage})
                    ),
                    Err(error) => println!(
                        "{}",
                        serde_json::json!({"runner":id,"model":model,"error":error.to_string()})
                    ),
                }
            } else {
                let result = runner.check(route).await;
                println!("{}", serde_json::to_string(&result)?);
            }
        }
    }
    if scratch.exists() {
        std::fs::remove_dir_all(scratch)?;
    }
    Ok(())
}
