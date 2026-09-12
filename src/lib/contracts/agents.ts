export type RunnerId = 'claude-cli' | 'codex-cli' | 'cursor-cli' | 'gemini-cli' | 'deepseek-api' | 'custom-cli' | 'anthropic-api' | 'openai-api' | 'google-api' | 'openrouter-api' | 'groq-api' | 'mistral-api' | 'ollama-api';
export interface RunnerInfo { kind: 'cli' | 'api' | 'local'; provider: string; default_model: string; saved_model: string | null; needs_key: string | null; id: RunnerId; label: string; available: boolean; status: string; detail: string; web: boolean; tools: boolean; metered: boolean }
export interface HealthCheck { runner: RunnerId; model: string; ok: boolean; detail: string; duration_ms: number }
export interface AgentPolicy { fallback_agent: string | null; fallback_model: string; monthly_budget_usd: number }
export interface AgentCall { id: string; started_at: string; runner: string; model: string; purpose: string; status: string; duration_ms: number | null; cost_usd: number | null; input_tokens: number | null; output_tokens: number | null; tokens_estimated: boolean; error_kind: string | null; fallback_of: string | null }

export interface ModelOption { id: string; label: string; free: boolean; input_usd_per_million: number | null; output_usd_per_million: number | null; context_length: number | null; tools: boolean; json_mode: boolean }
export interface ModelCatalog { models: ModelOption[]; source: string; error: string | null }
export interface LocalModel { name: string; size_bytes: number; modified: string }
export interface LocalStatus { installed: boolean; running: boolean; version: string | null; models: LocalModel[]; memory_gb: number | null; install_command: string; can_install: boolean }
export interface LocalPull { model: string; status: string; percent: number | null; done: boolean; error: string | null }
export interface RunnerConfiguration { runner: RunnerId; models: string[]; custom_command: string }
