//! Agent operation ported from Remote Ledger at 19997d3d253e48c93911364b35ef619b835e50a2.
//! Typed routes, adapters, explicit fallback, call accounting and bounded tool turns.
//! Learning prompts, source policy and acceptance gates remain owned by Principia Desk.
pub mod adapters;
pub mod api;
pub mod configuration;
pub mod local;
pub mod models;
pub mod process;
pub mod store;
pub mod types;
use crate::generator::{GenError, Result};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
pub use types::*;

#[derive(Clone)]
pub struct Runner {
    pub claude_bin: String,
    pub codex_bin: Option<String>,
    pub scratch_dir: PathBuf,
    pub database: Option<PathBuf>,
    /// Live activity lines for the interface and the execution log.
    pub feed: crate::execution_log::Feed,
    #[cfg(test)]
    pub test_deepseek: Option<(String, String)>,
}
pub fn deepseek_key() -> Option<String> {
    std::env::var("DEEPSEEK_API_KEY")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| crate::keychain::get_secret("deepseek"))
}
pub fn default_model(id: RunnerId) -> String {
    match id {
        RunnerId::ClaudeCli => "opus".into(),
        RunnerId::DeepseekApi => {
            std::env::var("DEEPSEEK_MODEL").unwrap_or_else(|_| "deepseek-flash".into())
        }
        RunnerId::AnthropicApi => "claude-sonnet-5".into(),
        RunnerId::OpenaiApi => "gpt-5.6-terra".into(),
        RunnerId::GoogleApi => "gemini-3.5-flash".into(),
        RunnerId::OpenrouterApi => "openrouter/free".into(),
        RunnerId::GroqApi => "llama-3.3-70b-versatile".into(),
        RunnerId::MistralApi => "mistral-large-latest".into(),
        RunnerId::OllamaApi => "llama3.2:3b".into(),
        _ => "default".into(),
    }
}
/// Legacy classes stored Claude's three UI aliases for every runner. Those values
/// never selected another provider's model, so retain that provider's default.
pub fn effective_model(id: RunnerId, model: &str) -> String {
    if model.is_empty()
        || (id != RunnerId::ClaudeCli && matches!(model, "opus" | "sonnet" | "haiku"))
        || (id.kind() != "cli" && model == "default")
    {
        default_model(id)
    } else {
        model.into()
    }
}
impl Runner {
    pub fn setting(&self, name: &str) -> Option<String> {
        let path = self.database.as_ref()?;
        let conn =
            rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                .ok()?;
        crate::db::get_config(&conn, name).ok().flatten()
    }
    pub fn fallback(&self, custom: &str) -> Result<Option<Route>> {
        self.database
            .as_ref()
            .map(|path| store::fallback(path, custom))
            .transpose()
            .map(Option::flatten)
    }
    pub fn log(&self, message: String) {
        self.feed.say(message);
    }
    pub fn binary(&self, id: RunnerId) -> Option<String> {
        match id {
            RunnerId::ClaudeCli => process::resolve(&self.claude_bin),
            RunnerId::CodexCli => self
                .codex_bin
                .as_deref()
                .and_then(process::resolve)
                .or_else(|| {
                    if self.codex_bin.as_deref() == Some("none") {
                        None
                    } else {
                        process::resolve("codex")
                    }
                }),
            RunnerId::CursorCli => process::resolve("cursor-agent"),
            RunnerId::GeminiCli => process::resolve("gemini"),
            _ => None,
        }
    }
    pub async fn info(&self, id: RunnerId, custom: &str) -> RunnerInfo {
        let available = match id {
            RunnerId::OllamaApi => local::reachable().await,
            _ if id.metered() => api::key(id).is_some(),
            RunnerId::CustomCli => process::command_words(
                &self
                    .setting("runner_setup_custom-cli")
                    .and_then(|raw| {
                        serde_json::from_str::<configuration::RunnerConfiguration>(&raw).ok()
                    })
                    .map(|setup| setup.custom_command)
                    .unwrap_or_else(|| custom.to_owned()),
            )
            .ok()
            .and_then(|w| process::resolve(&w[0]))
            .is_some(),
            _ => self.binary(id).is_some(),
        };
        RunnerInfo {
            kind: id.kind().into(),
            provider: id.legacy_id().into(),
            default_model: default_model(id),
            saved_model: self.setting(&format!("model_{}", id.id())),
            needs_key: id.key_name().map(str::to_owned),
            id,
            label: id.label().into(),
            available,
            status: if available {
                "configured"
            } else {
                "unavailable"
            }
            .into(),
            detail: if available {
                "Configured. Run a connection test to verify authentication and model access."
            } else if id == RunnerId::OllamaApi {
                "Ollama is stopped. Start it under On this machine."
            } else if id.metered() {
                "Add this provider’s API key to enable its models."
            } else {
                "Executable not found. Install the CLI or correct its path."
            }
            .into(),
            web: id == RunnerId::ClaudeCli,
            tools: id.supports_tools(),
            metered: id.metered(),
        }
    }
    pub async fn check(&self, route: Route) -> HealthCheck {
        let mut req = RunRequest::new(
            route.clone(),
            "Return exactly this JSON object: {\"status\":\"pong\"}",
        );
        req.purpose = "connection-test".into();
        req.timeout = Duration::from_secs(if route.runner == RunnerId::OllamaApi {
            120
        } else {
            45
        });
        // Reasoning-capable APIs count internal reasoning against the output cap.
        req.max_tokens = 4096;
        let start = Instant::now();
        let result = self.run(&req, None).await;
        let (ok, detail) = match result {
            Ok(result)
                if result.json.as_ref().is_some_and(|v| v["status"] == "pong")
                    || result.text.trim().eq_ignore_ascii_case("pong") =>
            {
                (true, "Connection and selected model verified.".into())
            }
            Ok(_) => (
                false,
                "Runner answered, but did not return the expected connection-test response.".into(),
            ),
            Err(error) => (false, connection_failure(route.runner, &error)),
        };
        HealthCheck {
            runner: route.runner,
            model: route.model,
            ok,
            detail,
            duration_ms: start.elapsed().as_millis() as u64,
        }
    }
    pub async fn run(&self, req: &RunRequest, fallback: Option<&Route>) -> Result<RunResult> {
        let id = new_call_id();
        match self.run_one(req, &id, None).await {
            Ok(result) => Ok(result),
            Err(primary_error) => {
                // No implicit provider switching. Storage errors must never trigger
                // another paid call after the first provider already answered.
                if primary_error
                    .to_string()
                    .contains("agent activity could not be saved")
                {
                    return Err(primary_error);
                }
                let Some(fallback) = fallback.filter(|r| r.runner != req.route.runner) else {
                    return Err(primary_error);
                };
                self.log(format!(
                    "{} failed; using configured fallback {}",
                    req.route.runner.label(),
                    fallback.runner.label()
                ));
                let mut next = req.clone();
                next.route = fallback.clone();
                self.run_one(&next, &new_call_id(), Some(&id)).await.map_err(|error| GenError::Api(format!("{} failed ({primary_error}); configured fallback {} also failed ({error})",req.route.runner.label(), fallback.runner.label())))
            }
        }
    }
    async fn run_one(
        &self,
        req: &RunRequest,
        id: &str,
        fallback_of: Option<&str>,
    ) -> Result<RunResult> {
        if !valid_model(&req.route.model) {
            return Err(GenError::Api("invalid model identifier".into()));
        }
        // Each tool turn passes through this same gate and accounting path.
        if let Some(path) = &self.database {
            store::begin(path, id, req, fallback_of)?;
        }
        let start = Instant::now();
        self.log(format!(
            "{} · {} · {}",
            req.route.runner.label(),
            req.route.model,
            req.purpose
        ));
        let output = if !req.tools.is_empty() && !req.route.runner.supports_tools() {
            Err(GenError::Api(format!(
                "{} does not expose application tool calls",
                req.route.runner.label()
            )))
        } else if req.route.runner == RunnerId::DeepseekApi {
            adapters::run_deepseek(self, req).await
        } else if req.route.runner.kind() != "cli" {
            api::run(self, req).await
        } else {
            adapters::run_cli(self, req, id).await
        };
        let duration_ms = start.elapsed().as_millis() as u64;
        match output {
            Ok(result) => {
                if let Some(path) = &self.database {
                    store::finish(
                        path,
                        id,
                        &result.model,
                        duration_ms,
                        Some(&result.usage),
                        None,
                    )?;
                }
                self.log(format!(
                    "{} · finished in {:.1}s",
                    req.route.runner.label(),
                    duration_ms as f64 / 1000.0
                ));
                Ok(RunResult {
                    call_id: id.into(),
                    runner: req.route.runner,
                    model: result.model,
                    json: if req.json {
                        parse_json(&result.text)
                    } else {
                        None
                    },
                    text: result.text,
                    usage: result.usage,
                    duration_ms,
                    tool_calls: result.tool_calls,
                })
            }
            Err(error) => {
                let kind = match &error {
                    GenError::NoBinary => "executable-missing",
                    GenError::Timeout(_) => "timeout",
                    GenError::BadExit(_, _) => "process-failed",
                    GenError::Parse(_) | GenError::ProviderResponse { .. } => "invalid-response",
                    GenError::Quality(_) => "quality-failed",
                    GenError::Io(_) => "io-failed",
                    GenError::Api(_) => "provider-failed",
                };
                if let Some(path) = &self.database {
                    let (model, usage) = match &error {
                        GenError::ProviderResponse { model, usage, .. } => {
                            (model.as_str(), Some(usage.as_ref()))
                        }
                        _ => (req.route.model.as_str(), None),
                    };
                    store::finish(path, id, model, duration_ms, usage, Some(kind))?;
                }
                self.log(format!("{} · {kind}", req.route.runner.label()));
                Err(error)
            }
        }
    }
    /// App-owned tools only. No tool execution after the final bounded model turn.
    /// Unlike a raw prompt asking to browse, every returned fact can be tied to
    /// material the application actually fetched and retained.
    pub async fn run_with_tools<F, Fut>(
        &self,
        req: &RunRequest,
        max_steps: usize,
        execute: F,
    ) -> Result<RunResult>
    where
        F: Fn(ToolCall) -> Fut,
        Fut: std::future::Future<Output = Result<String>>,
    {
        let mut turn = req.clone();
        if turn.messages.is_empty() {
            if let Some(system) = &req.system {
                turn.messages
                    .push(ChatMessage::text("system", system.clone()));
            }
            turn.messages
                .push(ChatMessage::text("user", req.prompt.clone()));
        }
        let steps = max_steps.clamp(1, 12);
        for step in 0..steps {
            let final_turn = step + 1 == steps;
            turn.json = final_turn && req.json;
            if final_turn {
                turn.tools.clear();
            }
            let mut result = self.run(&turn, None).await?;
            if result.tool_calls.is_empty() {
                if req.json {
                    result.json = parse_json(&result.text);
                }
                return Ok(result);
            }
            if final_turn {
                return Err(GenError::Api(
                    "runner requested a tool after the final answer limit".into(),
                ));
            }
            if result.tool_calls.len() > 16 {
                return Err(GenError::Api(
                    "runner requested too many tools in one turn".into(),
                ));
            }
            turn.messages.push(ChatMessage {
                role: "assistant".into(),
                content: result.text.clone(),
                tool_call_id: None,
                tool_calls: result.tool_calls.clone(),
            });
            for call in result.tool_calls {
                let id = call.id.clone();
                let output = if req.tools.iter().any(|t| t.name == call.name) {
                    match tokio::time::timeout(Duration::from_secs(30),execute(call)).await {
                        Ok(Ok(text)) => text.chars().take(24_000).collect(),
                        Ok(Err(_)) => "Tool failed. Use available evidence or explain what remains unknown.".into(),
                        Err(_) => "Tool timed out. Use available evidence or explain what remains unknown.".into(),
                    }
                } else {
                    "Unknown tool. Use only the supplied tool definitions.".into()
                };
                turn.messages.push(ChatMessage {
                    role: "tool".into(),
                    content: output,
                    tool_call_id: Some(id),
                    tool_calls: vec![],
                });
            }
        }
        unreachable!()
    }
}
pub fn new_call_id() -> String {
    format!("call-{:032x}", rand::random::<u128>())
}

fn connection_failure(runner: RunnerId, error: &GenError) -> String {
    let detail = error.to_string();
    let lower = detail.to_ascii_lowercase();
    if runner == RunnerId::ClaudeCli
        && lower.contains("--safe-mode")
        && lower.contains("unknown option")
    {
        return "This Claude Code version does not support isolated study calls. Update Claude Code to version 2.1.169 or later, then test again.".into();
    }
    if runner == RunnerId::CursorCli && lower.contains("authentication required") {
        return "Cursor Agent is installed but is not signed in. Run `cursor-agent login` in Terminal, then test again.".into();
    }
    if runner == RunnerId::CodexCli
        && lower.contains("unexpected argument")
        && lower.contains("--ignore-user-config")
    {
        return "This Codex CLI version does not support isolated study calls. Update Codex CLI, then test again.".into();
    }
    if runner == RunnerId::GeminiCli
        && (lower.contains("projectidrequirederror")
            || lower.contains("requires setting the google_cloud_project"))
    {
        return "Gemini CLI's account requires a Google Cloud project. Set GOOGLE_CLOUD_PROJECT in your Gemini CLI environment (for example ~/.gemini/.env), then test again.".into();
    }
    if matches!(error, GenError::Timeout(_)) {
        return format!("{} did not finish the connection test in time. Check its authentication and model availability, then retry. Local models may need time to load into memory.", runner.label());
    }
    detail
}

#[cfg(test)]
mod tests;
