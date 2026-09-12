use crate::agents::AgentPolicy;
use crate::{
    agents::{self, HealthCheck, Route, RunnerId, RunnerInfo},
    db,
    state::AppState,
};
use tauri::State;

#[tauri::command]
pub fn get_runner_configuration(
    state: State<'_, AppState>,
    runner: String,
) -> Result<agents::configuration::RunnerConfiguration, String> {
    let runner = RunnerId::parse(&runner).ok_or("unknown runner")?;
    agents::configuration::read(&state.db.0.lock().unwrap(), runner)
}

#[tauri::command]
pub fn save_runner_configuration(
    state: State<'_, AppState>,
    configuration: agents::configuration::RunnerConfiguration,
) -> Result<agents::configuration::RunnerConfiguration, String> {
    agents::configuration::save(&state.db.0.lock().unwrap(), configuration)
}

pub async fn test_connection(
    state: &AppState,
    agent: Option<String>,
    custom_bin: Option<String>,
    model: Option<String>,
) -> Result<HealthCheck, String> {
    let name = agent.unwrap_or_else(|| state.generator.current_agent());
    let runner = RunnerId::parse(&name).ok_or_else(|| format!("unknown runner: {name}"))?;
    let model = model.unwrap_or_else(|| {
        if name == state.generator.current_agent() {
            state.generator.current_model()
        } else {
            agents::default_model(runner)
        }
    });
    let route = Route {
        runner,
        model: agents::effective_model(runner, &model),
        custom_command: custom_bin.unwrap_or_else(|| state.generator.current_custom_bin()),
    };
    Ok(state.generator.runner.check(route).await)
}
#[tauri::command]
pub async fn test_agent_connection(
    state: State<'_, AppState>,
    agent: Option<String>,
    custom_bin: Option<String>,
    model: Option<String>,
) -> Result<HealthCheck, String> {
    test_connection(&state, agent, custom_bin, model).await
}
#[tauri::command]
pub async fn list_agent_runners(state: State<'_, AppState>) -> Result<Vec<RunnerInfo>, String> {
    let mut runners = Vec::new();
    for id in RunnerId::ALL {
        runners.push(
            state
                .generator
                .runner
                .info(id, &state.generator.current_custom_bin())
                .await,
        );
    }
    Ok(runners)
}

#[tauri::command]
pub async fn get_runner_models(
    state: State<'_, AppState>,
    runner: String,
    refresh: bool,
) -> Result<agents::models::ModelCatalog, String> {
    let runner = RunnerId::parse(&runner).ok_or("unknown runner")?;
    agents::models::discover(&state.generator.runner, runner, refresh)
        .await
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub fn set_runner_key(runner: String, value: String) -> Result<(), String> {
    let runner = RunnerId::parse(&runner).ok_or("unknown provider")?;
    let provider = runner
        .key_name()
        .ok_or("this runner does not use an API key")?;
    if value.len() > 4096 || value.chars().any(char::is_control) {
        return Err("invalid API key".into());
    }
    crate::keychain::set_secret(provider, value.trim())?;
    agents::models::invalidate(runner);
    Ok(())
}
#[tauri::command]
pub async fn get_local_models() -> Result<agents::local::LocalStatus, String> {
    agents::local::status().await.map_err(|e| e.to_string())
}
#[tauri::command]
pub fn get_local_pulls() -> Vec<agents::local::PullState> {
    agents::local::pulls()
}
#[tauri::command]
pub async fn install_local_runner() -> Result<String, String> {
    agents::local::install().await.map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn start_local_runner() -> Result<(), String> {
    agents::local::start().await.map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn pull_local_model(model: String) -> Result<agents::local::PullState, String> {
    agents::local::pull(model).await.map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn remove_local_model(model: String) -> Result<(), String> {
    agents::local::remove(&model)
        .await
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn select_runner(
    state: State<'_, AppState>,
    agent: String,
    model: String,
    custom_bin: String,
) -> Result<(), String> {
    let runner = RunnerId::parse(&agent).ok_or("unknown runner")?;
    if !agents::valid_model(&model) {
        return Err("enter a valid model ID".into());
    }
    if runner == RunnerId::OllamaApi {
        agents::local::validate_chat_model(&agents::effective_model(runner, &model))
            .await
            .map_err(|e| e.to_string())?;
    }
    if runner == RunnerId::CustomCli {
        agents::process::command_words(&custom_bin).map_err(|e| e.to_string())?;
    }
    let mut conn = state.db.0.lock().unwrap();
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let current = state.generator.current_agent();
    let current_model = state.generator.current_model();
    if let Some(current) = RunnerId::parse(&current) {
        db::set_config(&tx, &format!("model_{}", current.id()), &current_model)
            .map_err(|e| e.to_string())?;
    }
    for (key, value) in [
        ("agent", runner.legacy_id()),
        ("model", model.as_str()),
        ("custom_agent_bin", custom_bin.as_str()),
        (&format!("model_{}", runner.id()), model.as_str()),
    ] {
        db::set_config(&tx, key, value).map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    let mut active = state.generator.agent.lock().unwrap();
    let mut selected_model = state.generator.model.lock().unwrap();
    let mut custom = state.generator.custom_bin.lock().unwrap();
    *active = runner.legacy_id().into();
    *selected_model = model;
    *custom = custom_bin;
    Ok(())
}
#[tauri::command]
pub fn set_openrouter_free_only(state: State<'_, AppState>, free_only: bool) -> Result<(), String> {
    db::set_config(
        &state.db.0.lock().unwrap(),
        "openrouter_free_only",
        if free_only { "true" } else { "false" },
    )
    .map_err(|e| e.to_string())
}
#[tauri::command]
pub fn get_openrouter_free_only(state: State<'_, AppState>) -> bool {
    state
        .generator
        .runner
        .setting("openrouter_free_only")
        .as_deref()
        != Some("false")
}
#[tauri::command]
pub fn get_agent_activity(
    state: State<'_, AppState>,
) -> Result<Vec<agents::store::CallSummary>, String> {
    agents::store::recent(&state.data_dir.join("principia.db")).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_agent_policy(state: State<'_, AppState>) -> Result<AgentPolicy, String> {
    agents::store::policy(&state.db.0.lock().unwrap()).map_err(|e| e.to_string())
}
#[tauri::command]
pub fn set_agent_policy(state: State<'_, AppState>, policy: AgentPolicy) -> Result<(), String> {
    if !policy.monthly_budget_usd.is_finite()
        || !(0.0..=1_000_000.0).contains(&policy.monthly_budget_usd)
    {
        return Err("invalid monthly budget".into());
    }
    if let Some(name) = &policy.fallback_agent {
        if RunnerId::parse(name).is_none() || !agents::valid_model(&policy.fallback_model) {
            return Err("invalid fallback runner or model".into());
        }
    }
    let mut conn = state.db.0.lock().unwrap();
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    db::set_config(
        &tx,
        "agent_fallback",
        policy.fallback_agent.as_deref().unwrap_or(""),
    )
    .map_err(|e| e.to_string())?;
    db::set_config(&tx, "agent_fallback_model", &policy.fallback_model)
        .map_err(|e| e.to_string())?;
    db::set_config(
        &tx,
        "agent_budget_monthly_usd",
        &policy.monthly_budget_usd.to_string(),
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}
