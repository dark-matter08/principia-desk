# Runner execution verification

This record distinguishes a connected transport from a complete, accepted lesson.
Tests use disposable QA databases and normal provider authentication. No production
learner database is used. The OpenRouter credential remains in native Keychain;
neither these records nor the developer probes contain a key.

## September 10, 2026 checkpoint

| Runner/model | Live connection | Full lesson |
|---|---|---|
| Claude Code: opus, sonnet, haiku | All three pass through the current production adapter, about 3–4 seconds. A packaged desktop Sonnet check also passed. | An earlier Sonnet run with retrieved sources completed every stage and saved a Linux Bash lesson. The resumed run executed all calls but its final audit rejected factual and attribution problems while GNU documentation was unreachable. Latest GUI handoff still needs inspection. |
| Codex: default | Passes through the isolated adapter in about 18 seconds; an earlier packaged desktop check passed in about 38 seconds. | The resumed prose/metadata pipeline completed, passed final review and persisted a 4,149-word Linux Bash lesson with five checks and a structured exercise in its disposable database. GUI handoff remains unverified. |
| OpenRouter: openrouter/free | Passed; the router selected `nex-agi/nex-n2.5-mini:free`. | The resumed router completed prose, metadata, depth expansion and review, then hit its output limit during a whole-lesson revision. The router selected different underlying models across calls; one took 700.6 seconds. The fixed Nex free model advertised a `none` reasoning effort that the adapter had overlooked. After fixing that selection, requests completed in seconds, but final review still rejected executable-example defects. A complete accepted free-model lesson is not verified. |
| OpenRouter: qwen/qwen3-14b | Connection and small JSON generation pass. | The resumed pipeline passed validation and final review and persisted a 5,737-word Linux Bash lesson with five checks and a structured exercise. This exceeds the 3,500–4,500-word writing target despite meeting the current deterministic floor; expansion length remains a tuning item. GUI handoff remains unverified. |
| Ollama: installed qwen2.5:7b | Live responses pass, including the earlier native local-library check. | Separating prose eliminated the original whole-lesson JSON failure. Subsequent runs exposed malformed assessment fields and a missing editorial score. Explicit metadata/audit schemas are now sent through Ollama structured outputs; the full retry is in progress. |
| Cursor Agent: default | Installed, but the live check requires `cursor-agent login`. | Not runnable with the current account state. |
| Gemini CLI: default | Installed with cached authentication, but its account requires `GOOGLE_CLOUD_PROJECT`. | Not runnable without the user's project configuration. |
| Custom CLI | Argument, model substitution and output transport pass with an explicit synthetic fixture. | A fixture is not evidence of a connected external model. |
| Anthropic API, OpenAI API | Protocol fixtures pass. No live keys available. | Live tests explicitly deferred to the user. |
| Google, Groq, Mistral, DeepSeek APIs | Protocol fixtures pass. No live keys available. | Live availability remains unverified. |

## Changes behind these checks

Claude study calls use safe mode, medium effort and an explicit tool set. On
macOS a fixed shell `exec` launch preserves separate arguments and stdin; this
resolved the reproduced native connection startup failure. Codex study calls
retain CLI authentication but ignore personal coding configuration, use medium
effort, and disable shell work, delegation, apps and hooks. Explicit model IDs are
still passed through; `default` uses the CLI's default for this isolated session.
Custom CLI remains available for a deliberately configured external workflow.
Older CLIs that lack the isolation flags receive an update instruction.

The application retrieves teaching sources before writing. Writing turns now use
that material directly instead of starting another CLI research workflow or
executing learner exercises. Prompts follow the current subject and starting
point and no longer invent once-a-day teaching history or require frontend UI
artifacts from terminal courses. Citation requirements account for the sources
actually retrieved.

Compatible API responses accept public text blocks and distinguish truncation,
refusal and empty output. Unusable responses retain actual model, token and cost
metadata; hidden reasoning is never substituted for the answer. OpenRouter
reasoning controls follow the selected model's advertised capabilities. The
short connection check reserves a larger output allowance for reasoning models.

Long lesson prose is now generated as Markdown, with a separate smaller typed
assessment request. Incorrect question fields are repaired without replacing the
body. Correct-answer text must identify exactly one distinct choice; letters and
indices are never silently mapped to an answer. Depth corrections visit only
sections below their floor once the total is sufficient.

The final editor audits the existing draft without re-emitting it. Blocking
issues name a specific section or the assessment; only those sections are
rewritten, and the corrected result must pass another full audit. Course
quality failures have a distinct error category, rather than being presented as
connection or JSON parsing failures. Required headings and both small JSON
contracts are explicit. Long JSON repair retains the original request and typed
error and never silently truncates a response.

For Ollama, metadata and audit requests also send the exact JSON schema through
its [documented structured-output interface](https://docs.ollama.com/capabilities/structured-outputs).
Prose and other adapters retain their existing wire format; model answers still
pass application validation. Contract tests cover schema propagation, all
required fields, four choices and five questions. No new model was downloaded.

GNU documentation requests timed out during resumed QA. Link checks now run in
bounded parallel batches and do not repeat a transport timeout as a GET. Missing
source evidence is kept distinct from a broken provider. An editor rejection of
unsupported or inaccurate content is not bypassed to obtain a green lesson test.

The desktop shows preparation status across navigation and rejects duplicate
Start requests in both frontend and native code. Failure releases the guard.
Progress adapts to the panel space left by this status area. That layout was
verified in a packaged desktop app during a real preparation request.

## Reproducible checks

`runner_probe` defaults to inventory only. `--live` exercises saved models through
the production adapter; `--lesson-smoke` requests a small structured answer.
`lesson_probe DISPOSABLE_QA_DATABASE SUBJECT_ID --live` runs the real engineering
preparation and persistence pipeline without a webview or OS enforcement. It
rejects an already-active lesson so resume cannot masquerade as a new generation.
Neither tool is a substitute for native GUI verification or a learner completing
the resulting exercise and checks.

The current suite passes 238 Rust tests, with six live/external tests ignored,
plus 52 frontend tests. Svelte reports zero errors/warnings and strict Clippy
passes. The packaged macOS build succeeds.

The earlier compact Progress design was re-inspected in the native app after
workspace recovery. A further spacing refinement is implemented but its latest
native screenshot is pending. Later, window capture returned `cgWindowNotFound`
for both QA apps and Chrome. Startup instrumentation confirms native storage
initialization and a visible 1100×760 logical window, but that is not a substitute
for inspecting its rendering or handoff. All QA profiles remain separate from
production. The shared-runtime migration, class occurrence/focus ownership and
P4–P9 gates remain unfinished in [the product plan](PRODUCT_EVOLUTION_PLAN.md).

The weekly balance reached the user's 2% cutoff during final verification. Feature work stopped; remaining work is recorded in the product plan. The local structured-output retry was still running at the cutoff and must not be counted as a completed lesson.
