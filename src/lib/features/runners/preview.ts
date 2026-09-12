import type { RunnerInfo, ModelCatalog, LocalStatus, RunnerId, RunnerConfiguration } from '$lib/contracts/agents';

const definitions = [
  ['claude', 'Claude Code', 'opus'], ['codex', 'Codex', 'default'],
  ['cursor', 'Cursor Agent', 'default'], ['gemini', 'Gemini CLI', 'default'],
  ['custom', 'Custom CLI', 'default'], ['anthropic', 'Anthropic API', 'claude-sonnet-5'],
  ['openai', 'OpenAI API', 'gpt-5.6-terra'], ['google', 'Google Gemini API', 'gemini-3.5-flash'],
  ['openrouter', 'OpenRouter', 'openrouter/free'], ['groq', 'Groq API', 'llama-3.3-70b-versatile'],
  ['mistral', 'Mistral API', 'mistral-large-latest'], ['deepseek', 'DeepSeek API', 'deepseek-flash'],
  ['ollama', 'Ollama', 'llama3.2:3b'],
];
const cli = new Set(['claude', 'codex', 'cursor', 'gemini', 'custom']);
const saved = new Map<string, string>();
export function previewRunners(): RunnerInfo[] {
  return definitions.map(([provider, label, default_model]) => ({
    id: `${provider}-${cli.has(provider) ? 'cli' : 'api'}` as RunnerId,
    provider, label, default_model, saved_model: saved.get(provider) ?? null,
    kind: cli.has(provider) ? 'cli' : provider === 'ollama' ? 'local' : 'api',
    needs_key: cli.has(provider) || provider === 'ollama' ? null : provider,
    available: false, status: 'preview', detail: 'Detection requires the desktop app.',
    web: provider === 'claude', tools: ['openai','openrouter','groq','mistral','deepseek','ollama'].includes(provider),
    metered: !cli.has(provider) && provider !== 'ollama',
  }));
}
export function rememberPreviewModel(provider: string, model: string) { saved.set(provider, model); }
export function previewModels(provider: string): ModelCatalog {
  const runner = previewRunners().find(r => r.provider === provider || r.id === provider);
  const ids = runner?.provider === 'claude' ? ['default','opus','sonnet','haiku'] : runner?.kind === 'cli' ? ['default'] : [];
  return { models: ids.map(id => ({ id, label: id, free: false, input_usd_per_million: null, output_usd_per_million: null, context_length: null, tools: false, json_mode: false })), source: 'Browser preview · live catalogues require the desktop app', error: null };
}
export function previewLocal(): LocalStatus {
  return { installed: false, running: false, version: null, models: [], memory_gb: null, install_command: 'Open the desktop app to detect or install Ollama.', can_install: false };
}
export async function desktopRequired(): Promise<never> { throw new Error('This operation requires the desktop app. The browser preview does not contact providers or install models.'); }

const configurations = new Map<string, RunnerConfiguration>();
export function previewConfiguration(provider: string): RunnerConfiguration {
  const runner = previewRunners().find(r => r.provider === provider || r.id === provider);
  if (!runner) throw new Error('Unknown runner');
  const saved = configurations.get(runner.id);
  return structuredClone(saved ?? { runner: runner.id, models: [runner.saved_model || runner.default_model], custom_command: '' });
}
export function savePreviewConfiguration(configuration: RunnerConfiguration): RunnerConfiguration {
  configurations.set(configuration.runner, structuredClone(configuration));
  return structuredClone(configuration);
}
