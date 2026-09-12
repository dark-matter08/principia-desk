//! Native counterparts of Remote Ledger's app/llm/types.ts. See THIRD_PARTY_NOTICES.md.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RunnerId {
    ClaudeCli,
    CodexCli,
    CursorCli,
    GeminiCli,
    DeepseekApi,
    CustomCli,
    AnthropicApi,
    OpenaiApi,
    GoogleApi,
    OpenrouterApi,
    GroqApi,
    MistralApi,
    OllamaApi,
}
impl RunnerId {
    pub const ALL: [Self; 13] = [
        Self::ClaudeCli,
        Self::CodexCli,
        Self::CursorCli,
        Self::GeminiCli,
        Self::DeepseekApi,
        Self::CustomCli,
        Self::AnthropicApi,
        Self::OpenaiApi,
        Self::GoogleApi,
        Self::OpenrouterApi,
        Self::GroqApi,
        Self::MistralApi,
        Self::OllamaApi,
    ];
    pub fn kind(self) -> &'static str {
        match self {
            Self::ClaudeCli
            | Self::CodexCli
            | Self::CursorCli
            | Self::GeminiCli
            | Self::CustomCli => "cli",
            Self::OllamaApi => "local",
            _ => "api",
        }
    }
    pub fn metered(self) -> bool {
        self.kind() == "api"
    }
    pub fn supports_tools(self) -> bool {
        matches!(
            self,
            Self::DeepseekApi
                | Self::OpenaiApi
                | Self::OpenrouterApi
                | Self::GroqApi
                | Self::MistralApi
                | Self::OllamaApi
        )
    }
    pub fn key_name(self) -> Option<&'static str> {
        self.metered().then(|| self.legacy_id())
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "claude" | "claude-cli" => Some(Self::ClaudeCli),
            "codex" | "codex-cli" => Some(Self::CodexCli),
            "cursor" | "cursor-cli" => Some(Self::CursorCli),
            "gemini" | "gemini-cli" => Some(Self::GeminiCli),
            "deepseek" | "deepseek-api" => Some(Self::DeepseekApi),
            "custom" | "custom-cli" => Some(Self::CustomCli),
            "anthropic" | "anthropic-api" => Some(Self::AnthropicApi),
            "openai" | "openai-api" => Some(Self::OpenaiApi),
            "google" | "google-api" => Some(Self::GoogleApi),
            "openrouter" | "openrouter-api" => Some(Self::OpenrouterApi),
            "groq" | "groq-api" => Some(Self::GroqApi),
            "mistral" | "mistral-api" => Some(Self::MistralApi),
            "ollama" | "ollama-api" => Some(Self::OllamaApi),
            _ => None,
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Self::ClaudeCli => "claude-cli",
            Self::CodexCli => "codex-cli",
            Self::CursorCli => "cursor-cli",
            Self::GeminiCli => "gemini-cli",
            Self::DeepseekApi => "deepseek-api",
            Self::CustomCli => "custom-cli",
            Self::AnthropicApi => "anthropic-api",
            Self::OpenaiApi => "openai-api",
            Self::GoogleApi => "google-api",
            Self::OpenrouterApi => "openrouter-api",
            Self::GroqApi => "groq-api",
            Self::MistralApi => "mistral-api",
            Self::OllamaApi => "ollama-api",
        }
    }
    pub fn legacy_id(self) -> &'static str {
        self.id().split('-').next().unwrap()
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::ClaudeCli => "Claude Code",
            Self::CodexCli => "Codex",
            Self::CursorCli => "Cursor Agent",
            Self::GeminiCli => "Gemini CLI",
            Self::DeepseekApi => "DeepSeek API",
            Self::CustomCli => "Custom CLI",
            Self::AnthropicApi => "Anthropic API",
            Self::OpenaiApi => "OpenAI API",
            Self::GoogleApi => "Google Gemini API",
            Self::OpenrouterApi => "OpenRouter",
            Self::GroqApi => "Groq API",
            Self::MistralApi => "Mistral API",
            Self::OllamaApi => "Ollama",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub args: Value,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
}
impl ChatMessage {
    pub fn text(role: &str, content: String) -> Self {
        Self {
            role: role.into(),
            content,
            tool_call_id: None,
            tool_calls: vec![],
        }
    }
}

/// A frozen route. A fallback has its own model; a primary model is never sent to it.
#[derive(Debug, Clone)]
pub struct Route {
    pub runner: RunnerId,
    pub model: String,
    pub custom_command: String,
}
#[derive(Debug, Clone)]
pub struct RunRequest {
    pub route: Route,
    pub purpose: String,
    pub owner: Option<String>,
    pub system: Option<String>,
    pub prompt: String,
    pub json: bool,
    /// Optional output contract. Adapters send it only where structured output
    /// is supported; callers still validate the returned value themselves.
    pub output_schema: Option<serde_json::Value>,
    pub allow_web: bool,
    pub max_tokens: u32,
    pub timeout: Duration,
    pub tools: Vec<ToolDef>,
    pub messages: Vec<ChatMessage>,
}
impl RunRequest {
    pub fn new(route: Route, prompt: &str) -> Self {
        Self {
            route,
            purpose: "generation".into(),
            owner: None,
            system: None,
            prompt: prompt.into(),
            json: true,
            output_schema: None,
            allow_web: false,
            max_tokens: 16_384,
            timeout: Duration::from_secs(300),
            tools: vec![],
            messages: vec![],
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_tokens: u64,
    /// Unknown costs stay unknown; zero means known free, never missing pricing.
    pub cost_usd: Option<f64>,
    pub metered: bool,
    pub tokens_estimated: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunResult {
    pub call_id: String,
    pub runner: RunnerId,
    pub model: String,
    pub text: String,
    pub json: Option<Value>,
    pub usage: Usage,
    pub duration_ms: u64,
    pub tool_calls: Vec<ToolCall>,
}
#[derive(Debug, Clone, Serialize)]
pub struct RunnerInfo {
    pub kind: String,
    pub provider: String,
    pub default_model: String,
    pub saved_model: Option<String>,
    pub needs_key: Option<String>,
    pub id: RunnerId,
    pub label: String,
    pub available: bool,
    /// Installation/key presence is not proof of authentication or model access.
    pub status: String,
    pub detail: String,
    pub web: bool,
    pub tools: bool,
    pub metered: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct HealthCheck {
    pub runner: RunnerId,
    pub model: String,
    pub ok: bool,
    pub detail: String,
    pub duration_ms: u64,
}

pub fn valid_model(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && !value.chars().any(|c| c.is_control() || c.is_whitespace())
}

/// Recover the first complete value, preserving braces/escapes inside JSON strings.
pub fn parse_json(text: &str) -> Option<Value> {
    if let Ok(value) = serde_json::from_str(text.trim()) {
        return Some(value);
    }
    for (start, ch) in text.char_indices() {
        if ch != '{' && ch != '[' {
            continue;
        }
        let mut stream = serde_json::Deserializer::from_str(&text[start..]).into_iter::<Value>();
        if let Some(Ok(value)) = stream.next() {
            return Some(value);
        }
    }
    None
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPolicy {
    pub fallback_agent: Option<String>,
    pub fallback_model: String,
    pub monthly_budget_usd: f64,
}
