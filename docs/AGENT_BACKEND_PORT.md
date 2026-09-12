# Runner setup and selection in Principia Desk

The requested Remote Ledger behavior is its **CLI / API / local model runner
system**, connected to Principia Desk's existing teaching backend. Reference:
Remote Ledger commit `19997d3d253e48c93911364b35ef619b835e50a2`, especially
`RunnerChoice.tsx`, `OllamaSetup.tsx`, `app/ollama.ts`, `ollama.server.ts` and
`app/llm/`. Attribution ships in [THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md).
No Remote Ledger credentials, application data or visual theme are copied.

## Configuration and active selection

The runner library and active tutor are separate boxes. The library is collapsed
by default in Settings and expanded for initial setup. Configuring a runner does
not select it for study.

- The library groups CLI agents, API providers and local models. Only the selected
  provider's editor is open; adding a key or editing its model shortlist does not
  switch the active tutor.
- Model catalogues support name/ID search and five results per page. Models can be
  added to a shortlist or entered by ID. Each runner saves its own shortlist and,
  for custom CLIs, its command.
- The active tutor box selects a runner and one of its saved models. The current
  model stays available if it was removed from the shortlist, until the learner
  chooses another one. **Use for study** applies the choice. Setup and class
  editors apply it through their enclosing save action.
- Ollama has Installed, Browse and Downloads tabs, search, size filtering and five
  models per page. Installed rows add models to the shortlist; secondary controls
  expose testing and removal. Downloads continue across navigation in the same
  app process. No installation or model download starts automatically.

`agents/configuration.rs` persists runner setup separately from the existing
`agent`, `model` and `custom_agent_bin` active settings. The distinction also
exists in the browser preview. Provider keys remain in native credential storage;
no secret is saved in the model-shortlist configuration.

## Supported routes

| Route | Runners | Setup |
|---|---|---|
| CLI | Claude Code, Codex, Cursor Agent, Gemini CLI, custom command | Executable detection, install documentation, existing CLI authentication, model IDs/aliases |
| API | Anthropic, OpenAI, Google Gemini, OpenRouter, Groq, Mistral, DeepSeek | Per-provider keys, live model catalogues, selected model, actual-response connection test |
| Local | Ollama | Install/start/detect, installed models, reference shelf, RAM estimates, streamed downloads, test/remove |

OpenRouter uses its public live catalogue for prices and capabilities and defaults
to free models only. Unknown prices stay unknown. Ollama requires a loopback
endpoint and rejects cloud-backed or embedding-only weights as local tutors.
The reference shelf is descriptive; the current teaching integration sends text
prompts, not image attachments or embedding requests.

macOS key entry uses the existing Keychain service. Environment credentials take
priority. Windows/Linux currently require environment keys; native secret storage
and native validation on those systems remain follow-ups. Live catalogue results
can change with provider availability and account access. A detected CLI or stored
key is not proof that a selected model can answer.

## Teaching integration

The existing generator still owns learning prompts, source retrieval, output
schemas, repairs and quality gates. All runner choices use those same contracts.
Source documentation is fetched by Principia Desk even when inference is local.
Course writing and course chat stay bound to the chosen provider. Optional
fallback has its own runner/model and applies to auxiliary grading, planning,
narration and language enrichment.

Supporting transport fixes honor selected CLI/API models, isolate each CLI call's
output, drain stdout/stderr together, and stop Unix process groups on timeout or
cancellation. Connection tests use the same adapters as generation and validate
the answer. Migration v4 records call metadata without prompts or responses.
No activity dashboard is required for configuring and selecting runners.

Broader tool orchestration, pricing/budget reconciliation and Windows descendant
process cancellation are not claimed as complete Remote Ledger parity. They are
separate from the user-requested runner setup experience and the outstanding
class/path/session work in [the product plan](PRODUCT_EVOLUTION_PLAN.md).

## Validation

The runner suite covers CLI concurrency/cancellation, selected models, explicit
fallback, health responses, HTTP fixtures for each new API/local adapter,
provider URL/auth formats, catalogue filtering and prices, local capability checks
and download records. The configuration regression test verifies that saving
another provider, model list or custom command leaves the active tutor untouched.
Frontend regressions cover shortlist isolation, retaining a current model,
search/pagination on a 123-model catalogue and local non-chat exclusions.

At this iteration, 13 runner/configuration Rust tests and 48 frontend tests pass;
Svelte reports zero errors or warnings. The preceding full Rust suite passed 188
tests with six external/live tests ignored. The packaged macOS build and strict Clippy pass. In the isolated desktop QA app:

- Both library and active-tutor boxes fit on the initial Settings viewport.
- The live OpenRouter catalogue contained 430 models; a Qwen search reduced it to
  53, and the page control changed results from 1–5 to 6–10.
- Saving two OpenRouter model IDs preserved the active `custom` / `opus` settings
  in SQLite. Choosing OpenRouter in the lower box then exposed only those two
  saved options in the custom dropdown, without applying that choice.
- The local library detected the installed but stopped Ollama daemon, started it,
  showed two installed models, disabled the embedding-only model as a tutor, and
  verified a real response from the existing `qwen2.5:7b` weights.

QA used `com.darkmatter.principia-desk.assessment-qa`, not the production learner
database. No keys were changed, paid provider generations requested, or model
weights downloaded or removed. Authenticated hosted model access is not implied
by fixture tests.

Later live execution tests, CLI isolation changes and remaining long-lesson
failures are recorded in [Runner execution verification](RUNNER_EXECUTION_QA.md).
