use super::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelOption {
    pub id: String,
    pub label: String,
    pub free: bool,
    pub input_usd_per_million: Option<f64>,
    pub output_usd_per_million: Option<f64>,
    pub context_length: Option<u64>,
    pub tools: bool,
    pub json_mode: bool,
    /// Provider-advertised reasoning controls, absent for non-reasoners/routers.
    #[serde(default)]
    pub reasoning: Option<Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCatalog {
    pub models: Vec<ModelOption>,
    pub source: String,
    pub error: Option<String>,
}
type Cache = HashMap<RunnerId, (Instant, ModelCatalog)>;
static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
pub fn invalidate(id: RunnerId) {
    if let Some(cache) = CACHE.get() {
        cache.lock().unwrap().remove(&id);
    }
}
pub fn free_model_id(id: &str) -> bool {
    id == "openrouter/free" || id.ends_with(":free")
}
pub fn option(id: &str) -> ModelOption {
    ModelOption {
        id: id.into(),
        label: id.into(),
        free: false,
        input_usd_per_million: None,
        output_usd_per_million: None,
        context_length: None,
        tools: false,
        json_mode: true,
        reasoning: None,
    }
}
fn price(value: &Value) -> Option<f64> {
    value
        .as_str()
        .and_then(|v| v.parse::<f64>().ok())
        .or_else(|| value.as_f64())
        .filter(|v| v.is_finite() && *v >= 0.0)
        .map(|v| v * 1_000_000.0)
}
pub fn decode(id: RunnerId, value: Value) -> Vec<ModelOption> {
    let rows = if id == RunnerId::GoogleApi || id == RunnerId::OllamaApi {
        &value["models"]
    } else {
        &value["data"]
    };
    let mut out: Vec<_> = rows
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|row| {
            let name = row["id"]
                .as_str()
                .or_else(|| row["name"].as_str())?
                .trim_start_matches("models/");
            if id == RunnerId::GoogleApi
                && !row["supportedGenerationMethods"]
                    .as_array()
                    .is_some_and(|a| a.iter().any(|v| v == "generateContent"))
            {
                return None;
            }
            if name.contains("embedding")
                || name.contains("embed-")
                || name.contains("whisper")
                || name.contains("tts")
                || name.contains("dall-e")
            {
                return None;
            }
            let mut model = option(name);
            model.label = row["displayName"]
                .as_str()
                .or_else(|| row["name"].as_str())
                .unwrap_or(name)
                .trim_start_matches("models/")
                .into();
            if id == RunnerId::OpenrouterApi {
                model.reasoning = row.get("reasoning").filter(|v| v.is_object()).cloned();
                model.input_usd_per_million = price(&row["pricing"]["prompt"]);
                model.output_usd_per_million = price(&row["pricing"]["completion"]);
                model.free = model.input_usd_per_million == Some(0.0)
                    && model.output_usd_per_million == Some(0.0);
                model.context_length = row["context_length"].as_u64();
                let params = row["supported_parameters"].as_array();
                model.tools = params.is_some_and(|p| p.iter().any(|v| v == "tools"));
                model.json_mode = params.is_some_and(|p| {
                    p.iter()
                        .any(|v| v == "response_format" || v == "structured_outputs")
                });
            }
            if id == RunnerId::OllamaApi {
                model.free = true;
            }
            Some(model)
        })
        .collect();
    out.sort_by(|a, b| b.free.cmp(&a.free).then(a.label.cmp(&b.label)));
    out.dedup_by(|a, b| a.id == b.id);
    out
}
/// Model identifiers published by each provider, used before a key is saved so
/// a shortlist can be built offline. The live `/models` catalogue replaces this
/// as soon as a key exists, and any other identifier can be typed in by hand.
/// Sources: each provider's own model documentation, checked 2026-09-11.
/// Model names each CLI documents for its `--model` flag. A CLI has no
/// catalogue endpoint, so these come from its own reference pages (checked
/// 2026-09-11); every one of these CLIs also accepts any other identifier,
/// which the shortlist's "Add by ID" control covers.
pub fn cli(id: RunnerId) -> Vec<ModelOption> {
    let rows: &[(&str, &str)] = match id {
        RunnerId::ClaudeCli => &[
            ("default", "Runner default · your account's model"),
            ("best", "Best available to your account"),
            ("fable", "Latest Fable"),
            ("opus", "Latest Opus"),
            ("sonnet", "Latest Sonnet"),
            ("haiku", "Latest Haiku"),
            ("opusplan", "Opus while planning, Sonnet while executing"),
            ("fable[1m]", "Fable · 1M-token context"),
            ("opus[1m]", "Opus · 1M-token context"),
            ("sonnet[1m]", "Sonnet · 1M-token context"),
            ("claude-fable-5-1", "Claude Fable 5.1"),
            ("claude-opus-5", "Claude Opus 5"),
            ("claude-sonnet-5", "Claude Sonnet 5"),
            ("claude-haiku-4-5", "Claude Haiku 4.5"),
        ],
        RunnerId::CodexCli => &[
            ("default", "Runner default · your account's model"),
            ("gpt-6-astra", "GPT-6 Astra"),
            ("gpt-5.6-sol", "GPT-5.6 Sol"),
            ("gpt-5.6-terra", "GPT-5.6 Terra"),
            ("gpt-5.6-luna", "GPT-5.6 Luna"),
            ("gpt-5.3-codex-spark", "GPT-5.3 Codex Spark"),
            ("gpt-5.5", "GPT-5.5"),
        ],
        RunnerId::GeminiCli => &[
            ("default", "Runner default · your account's model"),
            ("auto", "Auto · the CLI picks per request"),
            ("pro", "Pro tier alias"),
            ("flash", "Flash tier alias"),
            ("flash-lite", "Flash-Lite tier alias"),
            ("gemini-3.1-pro-preview", "Gemini 3.1 Pro (preview)"),
            ("gemini-3.5-flash", "Gemini 3.5 Flash"),
            ("gemini-3-flash", "Gemini 3 Flash"),
            ("gemini-3.1-flash-lite", "Gemini 3.1 Flash-Lite"),
            ("gemini-2.5-pro", "Gemini 2.5 Pro"),
            ("gemini-2.5-flash", "Gemini 2.5 Flash"),
            ("gemini-2.5-flash-lite", "Gemini 2.5 Flash-Lite"),
        ],
        RunnerId::CursorCli => &[
            ("default", "Runner default · your account's model"),
            ("auto", "Auto · Cursor selects the model"),
            ("auto-smart", "Cursor Router · balances cost and capability"),
            ("composer-2.5", "Composer 2.5"),
            ("composer-2", "Composer 2"),
            ("claude-opus-5", "Claude Opus 5"),
            ("claude-opus-5-fast", "Claude Opus 5 Fast"),
            ("gpt-5.6-sol", "GPT-5.6 Sol"),
            ("gemini-3.1-pro", "Gemini 3.1 Pro"),
            ("grok-4.6", "Grok 4.6"),
        ],
        _ => &[("default", "Runner default · your account's model")],
    };
    rows.iter()
        .map(|(id, label)| {
            let mut model = option(id);
            model.label = (*label).into();
            model
        })
        .collect()
}
pub fn bundled(id: RunnerId) -> Vec<ModelOption> {
    let rows: &[(&str, &str)] = match id {
        RunnerId::AnthropicApi => &[
            ("claude-fable-5-1", "Claude Fable 5.1"),
            ("claude-opus-5", "Claude Opus 5"),
            ("claude-sonnet-5", "Claude Sonnet 5"),
            ("claude-haiku-4-5-20251001", "Claude Haiku 4.5"),
        ],
        RunnerId::OpenaiApi => &[
            ("gpt-6-astra", "GPT-6 Astra"),
            ("gpt-5.6-sol", "GPT-5.6 Sol"),
            ("gpt-5.6-terra", "GPT-5.6 Terra"),
            ("gpt-5.6-luna", "GPT-5.6 Luna"),
            ("gpt-5.5", "GPT-5.5"),
            ("gpt-5.5-pro", "GPT-5.5 Pro"),
            ("gpt-5.4-mini", "GPT-5.4 mini"),
            ("gpt-5.3-codex", "GPT-5.3 Codex"),
        ],
        RunnerId::GoogleApi => &[
            ("gemini-3.8-flash", "Gemini 3.8 Flash"),
            ("gemini-3.7-flash", "Gemini 3.7 Flash"),
            ("gemini-3.6-flash", "Gemini 3.6 Flash"),
            ("gemini-3.5-flash", "Gemini 3.5 Flash"),
            ("gemini-3.5-flash-lite", "Gemini 3.5 Flash-Lite"),
            ("gemini-3.1-pro-preview", "Gemini 3.1 Pro (preview)"),
            ("gemini-2.5-pro", "Gemini 2.5 Pro"),
            ("gemini-2.5-flash", "Gemini 2.5 Flash"),
        ],
        RunnerId::GroqApi => &[
            ("llama-3.3-70b-versatile", "Llama 3.3 70B Versatile"),
            ("llama-3.1-8b-instant", "Llama 3.1 8B Instant"),
            ("openai/gpt-oss-120b", "GPT-OSS 120B"),
            ("openai/gpt-oss-20b", "GPT-OSS 20B"),
            ("groq/compound", "Groq Compound"),
            ("groq/compound-mini", "Groq Compound Mini"),
        ],
        RunnerId::MistralApi => &[
            ("mistral-large-latest", "Mistral Large 3"),
            ("mistral-medium-latest", "Mistral Medium 3.5"),
            ("mistral-small-latest", "Mistral Small 4"),
            ("ministral-14b-latest", "Ministral 3 14B"),
            ("ministral-8b-latest", "Ministral 3 8B"),
            ("ministral-3b-latest", "Ministral 3 3B"),
            ("codestral-latest", "Codestral"),
        ],
        RunnerId::DeepseekApi => &[
            ("deepseek-flash", "DeepSeek V4.1 Flash"),
            ("deepseek-v4-pro", "DeepSeek V4 Pro"),
        ],
        _ => &[],
    };
    rows.iter()
        .map(|(id, label)| {
            let mut model = option(id);
            model.label = (*label).into();
            model
        })
        .collect()
}
pub async fn discover(_runner: &Runner, id: RunnerId, refresh: bool) -> Result<ModelCatalog> {
    if id.kind() == "cli" {
        return Ok(ModelCatalog {
            models: cli(id),
            source: "Documented CLI model names · any other ID can be added by hand".into(),
            error: None,
        });
    }
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if !refresh && id != RunnerId::OllamaApi {
        if let Some((at, result)) = cache.lock().unwrap().get(&id) {
            if at.elapsed() < Duration::from_secs(6 * 3600) {
                return Ok(result.clone());
            }
        }
    }
    let key = api::key(id);
    if id.metered() && id != RunnerId::OpenrouterApi && key.is_none() {
        return Ok(ModelCatalog {
            models: bundled(id),
            source: "Published models · save a key to load the live catalogue".into(),
            error: None,
        });
    }
    let url = if id == RunnerId::OllamaApi {
        format!("{}/api/tags", local::base()?)
    } else {
        format!("{}/models", api::base(id)?)
    };
    let request = api::authorize(
        reqwest::Client::new()
            .get(url)
            .timeout(Duration::from_secs(12)),
        id,
        key.as_deref(),
    );
    let found = async {
        let response = request
            .send()
            .await
            .map_err(|e| GenError::Api(e.without_url().to_string()))?;
        if !response.status().is_success() {
            return Err(GenError::Api(format!(
                "model discovery returned {}",
                response.status()
            )));
        }
        let value = response
            .json::<Value>()
            .await
            .map_err(|_| GenError::Parse("unreadable model catalogue".into()))?;
        Ok::<_, GenError>(decode(id, value))
    }
    .await;
    match found {
        Ok(mut models) => {
            if id == RunnerId::OpenrouterApi && !models.iter().any(|m| m.id == "openrouter/free") {
                let mut free = option("openrouter/free");
                free.label = "Free Models Router".into();
                free.free = true;
                models.insert(0, free);
            }
            let result = ModelCatalog {
                models,
                source: "Live provider catalogue".into(),
                error: None,
            };
            cache
                .lock()
                .unwrap()
                .insert(id, (Instant::now(), result.clone()));
            Ok(result)
        }
        Err(error) => {
            let mut result = cache
                .lock()
                .unwrap()
                .get(&id)
                .map(|(_, v)| v.clone())
                .unwrap_or(ModelCatalog {
                    models: vec![],
                    source: "Model catalogue unavailable; enter a model ID".into(),
                    error: None,
                });
            result.error = Some(error.to_string());
            Ok(result)
        }
    }
}
