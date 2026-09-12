# Changelog

All notable changes to Principia Desk. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions are git tags.

## [Unreleased]

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
