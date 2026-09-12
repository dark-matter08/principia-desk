//! Remote Ledger's direct API and local-model choices, using the existing lesson contract.
use super::{
    adapters::{self, AdapterResult},
    models::ModelOption,
    *,
};
use crate::generator::{GenError, Result};
use serde_json::{json, Value};

pub fn key(id: RunnerId) -> Option<String> {
    let provider = id.key_name()?;
    std::env::var(format!("{}_API_KEY", provider.to_uppercase()))
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| {
            if id == RunnerId::GoogleApi {
                std::env::var("GEMINI_API_KEY")
                    .ok()
                    .filter(|v| !v.trim().is_empty())
            } else {
                None
            }
        })
        .or_else(|| crate::keychain::get_secret(provider))
}
pub fn base(id: RunnerId) -> Result<String> {
    Ok(match id {
        RunnerId::OpenaiApi => "https://api.openai.com/v1",
        RunnerId::AnthropicApi => "https://api.anthropic.com/v1",
        RunnerId::GoogleApi => "https://generativelanguage.googleapis.com/v1beta",
        RunnerId::OpenrouterApi => "https://openrouter.ai/api/v1",
        RunnerId::GroqApi => "https://api.groq.com/openai/v1",
        RunnerId::MistralApi => "https://api.mistral.ai/v1",
        RunnerId::DeepseekApi => "https://api.deepseek.com",
        RunnerId::OllamaApi => return Ok(format!("{}/v1", local::base()?)),
        _ => return Err(GenError::Api("not an API runner".into())),
    }
    .into())
}
pub fn authorize(
    request: reqwest::RequestBuilder,
    id: RunnerId,
    key: Option<&str>,
) -> reqwest::RequestBuilder {
    let Some(key) = key else {
        return request;
    };
    match id {
        RunnerId::AnthropicApi => request
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01"),
        RunnerId::GoogleApi => request.header("x-goog-api-key", key),
        _ => request.bearer_auth(key),
    }
}
pub fn body(req: &RunRequest, known: Option<&ModelOption>) -> Value {
    let id = req.route.runner;
    if id == RunnerId::AnthropicApi {
        let prompt = if req.json {
            format!(
                "{}\nReturn the requested JSON object without markdown fences.",
                req.prompt
            )
        } else {
            req.prompt.clone()
        };
        let mut body = json!({"model":req.route.model,"max_tokens":req.max_tokens,"messages":[{"role":"user","content":prompt}]});
        if let Some(system) = &req.system {
            body["system"] = json!(system);
        }
        return body;
    }
    if id == RunnerId::GoogleApi {
        let mut config = json!({"maxOutputTokens":req.max_tokens});
        if req.json {
            config["responseMimeType"] = json!("application/json");
        }
        let mut body = json!({"contents":[{"role":"user","parts":[{"text":req.prompt}]}],"generationConfig":config});
        if let Some(system) = &req.system {
            body["systemInstruction"] = json!({"parts":[{"text":system}]});
        }
        return body;
    }
    let mut body = adapters::deepseek_body(req);
    body.as_object_mut().unwrap().remove("thinking");
    if id == RunnerId::OllamaApi && req.json && req.tools.is_empty() {
        if let Some(schema) = &req.output_schema {
            body["response_format"] = json!({"type":"json_schema","json_schema":{"name":"principia_output","strict":true,"schema":schema}});
            body["temperature"] = json!(0);
        }
    }
    if id == RunnerId::OpenaiApi {
        body.as_object_mut().unwrap().remove("max_tokens");
        body["max_completion_tokens"] = json!(req.max_tokens);
        body["store"] = json!(false);
    }
    if id == RunnerId::OpenrouterApi {
        body["usage"] = json!({"include":true});
        if let Some(reasoning) = reasoning_budget(known, req.max_tokens) {
            body["reasoning"] = reasoning;
        }
        // Capability claims come from OpenRouter's actual catalogue.
        if known.is_some_and(|m| !m.json_mode) {
            body.as_object_mut().unwrap().remove("response_format");
        }
    }
    body
}

/// Reserve room for the requested answer instead of spending the entire output
/// allowance on optional reasoning. Use only advertised controls; mandatory
/// reasoners retain reasoning, and dynamic routers retain provider defaults.
fn reasoning_budget(known: Option<&ModelOption>, output_tokens: u32) -> Option<Value> {
    let controls = known?.reasoning.as_ref()?;
    if controls["mandatory"] != true
        && controls["supported_efforts"]
            .as_array()
            .is_some_and(|efforts| efforts.iter().any(|effort| effort == "none"))
    {
        return Some(json!({"effort":"none"}));
    }
    if controls["supports_max_tokens"] == true && output_tokens >= 4096 {
        return Some(json!({"max_tokens":2048}));
    }
    if let Some(efforts) = controls.get("supported_efforts") {
        for effort in ["low", "minimal", "medium", "high", "xhigh", "max"] {
            if efforts.is_null()
                || efforts
                    .as_array()
                    .is_some_and(|list| list.iter().any(|value| value == effort))
            {
                return Some(json!({"effort":effort}));
            }
        }
    }
    (controls["mandatory"] != true).then(|| json!({"enabled":false}))
}
pub fn parse(req: &RunRequest, value: Value) -> Result<AdapterResult> {
    let id = req.route.runner;
    let result = match id {
        RunnerId::AnthropicApi => {
            let text = value["content"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|p| p["type"] == "text")
                .filter_map(|p| p["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n");
            let u = &value["usage"];
            let cached = u["cache_read_input_tokens"].as_u64().unwrap_or(0);
            AdapterResult {
                text,
                model: value["model"].as_str().unwrap_or(&req.route.model).into(),
                tool_calls: vec![],
                usage: Usage {
                    input_tokens: u["input_tokens"].as_u64().unwrap_or(0)
                        + cached
                        + u["cache_creation_input_tokens"].as_u64().unwrap_or(0),
                    output_tokens: u["output_tokens"].as_u64().unwrap_or(0),
                    cached_tokens: cached,
                    cost_usd: None,
                    metered: true,
                    tokens_estimated: u["input_tokens"].as_u64().is_none(),
                },
            }
        }
        RunnerId::GoogleApi => {
            let text = value["candidates"][0]["content"]["parts"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|p| p["thought"] != true)
                .filter_map(|p| p["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n");
            let u = &value["usageMetadata"];
            AdapterResult {
                text,
                model: value["modelVersion"]
                    .as_str()
                    .unwrap_or(&req.route.model)
                    .into(),
                tool_calls: vec![],
                usage: Usage {
                    input_tokens: u["promptTokenCount"].as_u64().unwrap_or(0),
                    output_tokens: u["candidatesTokenCount"].as_u64().unwrap_or(0)
                        + u["thoughtsTokenCount"].as_u64().unwrap_or(0),
                    cached_tokens: u["cachedContentTokenCount"].as_u64().unwrap_or(0),
                    cost_usd: None,
                    metered: true,
                    tokens_estimated: u["promptTokenCount"].as_u64().is_none(),
                },
            }
        }
        _ => return adapters::parse_compatible(value, req),
    };
    let finish = if id == RunnerId::AnthropicApi {
        value["stop_reason"].as_str()
    } else {
        value["candidates"][0]["finishReason"].as_str()
    };
    adapters::validate_response(
        result,
        req,
        finish,
        value["promptFeedback"]["blockReason"].is_string(),
    )
}
pub async fn run(runner: &Runner, req: &RunRequest) -> Result<AdapterResult> {
    let id = req.route.runner;
    let key = key(id);
    if id.metered() && key.is_none() {
        return Err(GenError::Api(format!(
            "{} needs an API key. Add it under My own API key.",
            id.label()
        )));
    }
    if id == RunnerId::OllamaApi {
        local::validate_chat_model(&req.route.model).await?;
    }
    let catalog = if id == RunnerId::OpenrouterApi {
        models::discover(runner, id, false).await.ok()
    } else {
        None
    };
    let known = catalog
        .as_ref()
        .and_then(|c| c.models.iter().find(|m| m.id == req.route.model));
    if id == RunnerId::OpenrouterApi
        && runner.setting("openrouter_free_only").as_deref() != Some("false")
        && !models::free_model_id(&req.route.model)
        && !known.is_some_and(|m| m.free)
    {
        return Err(GenError::Api(
            "OpenRouter is set to free models only. Choose a free model or change that setting."
                .into(),
        ));
    }
    send(req, &base(id)?, key.as_deref(), known).await
}
pub(super) fn request_url(id: RunnerId, endpoint: &str, model: &str) -> Result<reqwest::Url> {
    let mut url = reqwest::Url::parse(endpoint).map_err(|e| GenError::Api(e.to_string()))?;
    let mut segments = url
        .path_segments_mut()
        .map_err(|_| GenError::Api("invalid provider URL".into()))?;
    match id {
        RunnerId::AnthropicApi => {
            segments.push("messages");
        }
        RunnerId::GoogleApi => {
            segments.push("models").push(&format!(
                "{}:generateContent",
                model.trim_start_matches("models/")
            ));
        }
        _ => {
            segments.push("chat").push("completions");
        }
    }
    drop(segments);
    Ok(url)
}
pub(super) async fn send(
    req: &RunRequest,
    endpoint: &str,
    key: Option<&str>,
    known: Option<&ModelOption>,
) -> Result<AdapterResult> {
    let id = req.route.runner;
    let url = request_url(id, endpoint, &req.route.model)?;
    let client = reqwest::Client::builder()
        .timeout(req.timeout)
        .build()
        .map_err(|e| GenError::Api(e.to_string()))?;
    let mut post = authorize(client.post(url), id, key);
    if id == RunnerId::OpenrouterApi {
        post = post
            .header(
                "HTTP-Referer",
                "https://github.com/dark-matter08/system-design-roulette",
            )
            .header("X-Title", "Principia Desk");
    }
    let response = post.json(&body(req, known)).send().await.map_err(|e| {
        GenError::Api(format!(
            "{} connection failed: {}",
            id.label(),
            e.without_url()
        ))
    })?;
    let status = response.status();
    let value = response.json::<Value>().await.map_err(|_| {
        GenError::Api(format!(
            "{} returned an unreadable response ({status})",
            id.label()
        ))
    })?;
    if !status.is_success() || value.get("error").is_some_and(|e| !e.is_null()) {
        let detail = value["error"]["message"]
            .as_str()
            .or_else(|| value["error"].as_str())
            .unwrap_or("provider request failed");
        return Err(GenError::Api(format!(
            "{} {status}: {}",
            id.label(),
            detail.chars().take(400).collect::<String>()
        )));
    }
    parse(req, value)
}
