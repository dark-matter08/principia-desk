# Changelog

All notable changes to Principia Desk. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions are git tags.

## [Unreleased]

## [0.2.3] · 2026-09-13

### Added
- **VibeVoice on every platform.** Beside the mlx-audio route on Apple Silicon, the desk installs Microsoft's official package with PyTorch under the profile: CUDA wheels when an NVIDIA driver is on the machine, processor wheels otherwise, the realtime 0.5B model fetched on the first render and the ten voice presets fetched with the engine. A machine that already has the `vibevoice` package in a Python is found as it is. On a processor it renders slower than it plays; the desk writes the audio ahead of the study time and the Logs page counts the lines.

## [0.2.2] · 2026-09-13

The first version a desk installs itself: a 0.2.1 desk sees it in Settings › Updates, checks the pipeline's signature and swaps itself for it. The desk is otherwise the same as 0.2.1.

### Changed
- The site's image build always runs the site build and reads the release with the runner's token, so the install section names the latest release instead of the one it saw last time.

## [0.2.1] · 2026-09-13

The first version an installed desk can update itself to the next one from. Keys save on every platform, SearXNG installs from the desk, and the profile travels.

### Added
- **Saved keys on every platform**: provider and search keys go into the platform's own secret store, the Credential Manager on Windows and the Secret Service (GNOME Keyring, KWallet) on Linux, next to the Keychain on macOS. Where no store answers, the key goes to a private file in the profile (`secrets.json`, mode 0600) and the interface says so. Environment keys still win. A Linux or Windows desk on 0.2.0 refused to save a key at all.
- **Updates from inside the desk**: the release's `latest.json` is read after boot and every six hours; Settings › Updates shows a newer version with its notes and installs it on request, verifying the pipeline's signature first, then relaunches. The app bundle is replaced on macOS, the installer runs on Windows, the AppImage is swapped in place, a deb or rpm goes through the package manager. A dot on the Settings tile says one is waiting.
- **SearXNG from the desk**: the search setup finds an instance already answering (Remote Ledger's is shared), starts one that is installed but stopped, or installs its own under `~/.principia-desk/searxng` (a shallow clone, an isolated Python through uv or a found interpreter, the JSON API turned on), every step on the Logs page. Start and Stop keys, the log's tail when it will not start.
- **Export and import of the profile** in Settings › Your data: one JSON archive with everything the desk knows, a checksum, a preview of what a file holds before it replaces the profile, and a backup of the record it replaces.
- **Listening**: a class can ask for its lessons as a conversation between a teacher and a student, written by the tutor once the lesson is ready and voiced with the engine and voices the class chose (Settings › Listening on the class). The lesson's **listen** key opens a dock with the transport and the transcript, and can voice a lesson on its own. Settings › Voices sets the engines up and finds the ones already on the machine: the system voice, Piper (any processor, voices downloaded one at a time), Kokoro (mlx-audio on Apple Silicon, the kokoro package elsewhere) and VibeVoice (the realtime 0.5B through mlx-audio on Apple Silicon, the official PyTorch package elsewhere). Migration v17 keeps a lesson's dialogue and where its segments are.

### Changed
- The desk describes itself as what it is, a desktop app for macOS, Windows and Linux, in the interface, the README, the contributing and security notes and the site. The enforcement picker says what each policy does on the platform it runs on: strict holds the desk the way focused does where the system does not let an application block switching, Force Quit, logout or shutdown.
- The README's recovery section covers the scheduler on each platform (launchd, systemd user timers, Task Scheduler), the release token roots and the data directory per platform; the install notes explain apt's "couldn't be accessed by user '_apt'" notice, which is not a failure.
- `docs/TEACHER.md` says what became of each part of the roulette-era plan instead of reading as a promise.
- The release workflow writes the updater artifacts, signs them with the `TAURI_SIGNING_PRIVATE_KEY` secret and publishes the manifest; the release notes are the changelog's section (`scripts/release-notes.mjs`).
- The dev server no longer watches the landing site or the docs, whose builds used to reload the running desk into a broken state.
- Settings is tabs (tutor, web search, voices, recovery, your data, updates) instead of one long page; a dot on the Settings tile says an update is waiting.
- The macOS menu bar shows the mark as a template image, tinted for a light or a dark bar, instead of the full-colour icon that read as a dark blob.

## [0.2.0] · 2026-09-12 · Principia Desk

The product that System Design Roulette became. Same database, same visual character, a different shape: classes you schedule instead of one daily roulette.

### Added
- **Nine classes** with personal starting points: Linux Bash, Bash Scripting, JavaScript & Browser, TypeScript, Frontend Architecture, Developer Tooling, System Design, German and Italian (A1–B2 scenarios with seven passes each).
- **Starting points and paths**: start from scratch, declare a stage with familiar topics, or take a placement check that samples every stage three times. Accepted paths are revisions; unit challenges, bridge lessons and route badges on the curriculum map.
- **A shared study runtime**: every lesson is a durable session with stages (learn, practice, check, feedback), saved steps and a prepare-ahead engine that writes the lesson before the study time and rings only when it is ready.
- **Durable appointments**: one per study time per day, due at its time, missed once the day passes, consumed exactly once; make-ups, skips and moves. Study times carry their own length and start per weekday; long days become blocks of whole topics with retrieval afterwards.
- **Lessons shaped for the session**: point form with reading hints, labelled callouts, at least two diagrams, a stepper for practice, inline Markdown in checks, a text-size control, and a section rail that stays in view.
- **Research grounding**: lessons are taught from pages the desk fetched from each course's allowed hosts, with mirrors for hosts that go dark, reasons for every skipped source, and an optional search engine (SearXNG, Brave, Tavily) for tutors that cannot browse.
- **Runners**: Claude Code, Codex, Cursor Agent, Gemini CLI and a custom command; Anthropic, OpenAI, Google, OpenRouter, Groq, Mistral and DeepSeek keys in the Keychain; Ollama on the machine. A runner library with model shortlists, and a tutor per class.
- **Enforcement per class**: advisory, focused or strict, with a native focus coordinator, and five ways out of a lock: finish the check, the break-glass phrase, the **recovery console** on a system-wide key combination (`Control + Option + Shift + U`) with a four-command ladder and a five-press valve, a `principia-unlock` release token on any volume, and a three-hour dead man's switch.
- **Always on**: a menu bar desk with today's appointments, a study alarm that snoozes but never dismisses, a launch agent that wakes the desk at every study time.
- **Home page**: a countdown ring, a study pulse (streaks, lessons, study days, checks passed, this week against the target) and a half-year contribution map.
- **Exports**: a lesson as PDF or CSV, filed by class; a profile export and import.
- **Execution logs**: every line a runner reports while preparing a lesson, kept thirty days, with a Logs page.
- **Your own classes**: a class builder on a five-step rail (Brief, Draft, Review, Verify, Enroll). The tutor drafts a curriculum from what you want to be able to do, reads it back, writes a question bank; the desk validates every topic, fetches every source, publishes the class into the catalog and teaches it like a bundled one. Class files export and import. Verify is three checks in order, all required before publishing: every review finding settled (fixed with the tutor, fixed by hand, or dismissed with a reason), every source fetched and the unreachable ones replaced or kept knowingly, and your own read-through confirmed.
- **Writing rules**: every request to a runner carries the rules, every answer is scrubbed of em dashes outside code and read for the tells of machine writing (Wikipedia's documented signs, as a shared catalogue), and a lesson section, question, chat reply or class draft that fails is sent back with the reason. The bundled prompts and lessons were held to the same rules.
- The desk's own confirm and prompt dialogs, and fields in the class editor that grow with their text. Topic cards drag into a new order or another stage; stage sections fold; the four stages are square tiles with their own marks that jump to their topics.
- The Logs page keeps time while a run is in flight: a beacon, a running duration on the run and in the tail, and the class builder's runs read as running, done or failed.
- **Releases carry every installer**: a version tag builds Apple Silicon and Intel disk images, a Windows MSI and NSIS installer, and a Linux `.deb`, `.rpm` and AppImage, verifies the tree first, and attaches them to the release; `npm run version:set` sets the version everywhere it is written.
- **Practice questions** on every class's Curriculum tab: the tutor writes eight cited questions at a time for a stage; pick an answer, see the key, the explanation and the source, dispute a key. Kept apart from the bank the placement check and unit challenges draw on, which is never shown beforehand.
- **Onboarding** in four steps: tutor, search, recovery (with a walkthrough of the ways out), deploy.
- A design system (DESIGN.md): one page frame, mono labels, three custom keys (call to action, bracket, ghost), route tabs on a rail.

### Changed
- Renamed from System Design Roulette to Principia Desk. Identifiers are `com.darkmatter.principia-desk`; the first launch adopts the previous profile through SQLite's backup API and leaves the original in place.
- The database is upgraded by numbered, checksummed migrations (v1–v16) with a backup before every upgrade; pace is no longer a setting, it follows the study times.
- The once-a-day routine, the roulette wheel and the legacy language management screens are retired; their data is kept.

### Contributors
- Classroom subjects, CEFR language programmes, research grounding, in-course chat and exercises, the reproducible DMG bundle and the refactors that made one runtime possible came from [@fogha](https://github.com/fogha).

## [0.1.0] · 2026-06-15 · System Design Roulette

The first stable release.

### Added
- A macOS app that takes the screen once a day: a quiz on what you learned, a roulette over the topics you have unlocked, and a thirty-minute course written by your own AI agent (Claude Code, with Codex, Cursor and Gemini fallbacks and bundled courses as the last resort).
- A curriculum of system-design concepts with tiers and prerequisites; a mastery ledger and a teacher agent that carries notes between encounters.
- Kiosk enforcement with three strictness levels, a white-screen guard, an escape hatch, a release token and schedule pause.
- launchd scheduling with wake-on-sleep and next-login catch-up; cross-platform scheduling and kiosk fallbacks.
- A tag-triggered release pipeline for macOS (arm64, x64), Windows and Linux.

[Unreleased]: https://github.com/dark-matter08/system-design-roulette/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/dark-matter08/system-design-roulette/releases/tag/v0.1.0
