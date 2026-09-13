# Contributing to Principia Desk

Principia Desk is a desktop study desk for macOS, Windows and Linux: classes with personal starting points, lessons written by your own AI tutor from primary sources, durable appointments, and enforcement you choose per class. Contributions are welcome, from a typo in a topic brief to a new course. This page says how the project works so a change lands the first time.

## Before you start

- **Read the safety rule.** A locked desk must always be escapable without a terminal: the break-glass phrase, the recovery console, the release token, the dead man's switch. A change that weakens any of them is refused whatever else it does. The history behind this rule is a colleague's laptop that needed Recovery Mode.
- **Read [DESIGN.md](DESIGN.md)** before touching the interface. One accent, one page frame, mono labels, the shared keys (`cta`, `bracket`, `ghost`). No new colours or fonts without updating it.
- **Read [docs/STORAGE.md](docs/STORAGE.md)** before touching the database. Migrations are numbered, checksummed and never edited once committed; a later change is a new migration.

## Setting up

```bash
git clone https://github.com/dark-matter08/principia-desk.git
cd principia-desk
npm install
npm run tauri dev          # the desk against a dev server
npm run dev                # the interface alone, in a browser, with mock data
```

Prerequisites: Rust 1.80+, Node 20+, and on Linux the WebKitGTK toolchain (`libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf libgtk-3-dev`). Lessons need a tutor: a Claude Code, Codex, Cursor or Gemini CLI you are signed in to, a provider key, or Ollama with a chat model. `npm run dev` needs none of that.

## The gates

Every pull request runs these; run them before you open one.

```bash
cd src-tauri && cargo fmt --check
cd src-tauri && cargo clippy --all-targets -- -D warnings
cd src-tauri && cargo test --all-targets
npm run check              # catalogue check + svelte-check
npm test                   # frontend tests
```

A change to `src-tauri/seed/catalog.json` or `seed/concepts.json` also needs `npm run catalog:generate`, which regenerates `src/lib/catalog.generated.ts`; CI checks that it is current.

## What to contribute

- **Courses and topics.** The bundled curricula live in `src-tauri/seed/concepts.json`; each topic carries a brief (outcome, mechanisms, a production scenario, misconceptions, evidence, an artefact, two or more primary sources on the course's hosts) that `CurriculumBrief::validate` holds it to. Reference lessons live in `src-tauri/seed/fallback_courses/`, the placement banks in `src-tauri/seed/diagnostics.json`. A correction to a wrong key or a dead source is the most useful small contribution there is.
- **Your own class as a proposal.** Build it in the desk (Classes › New class), export the class file, and attach it to an issue. If it is good, it becomes a bundled course.
- **Runners.** A new provider is an adapter in `src-tauri/src/agents/`; see `adapters.rs` and `api.rs` for the two shapes.
- **Platforms.** The desk builds and ships for macOS, Windows and Linux, and everything that touches the system has a branch per platform: the lock (`kiosk.rs`: the window on all three, the presentation options that hide the Dock and block switching on macOS), the scheduler (`scheduler.rs`: launchd, systemd user timers, Task Scheduler), the secret store (`keychain.rs`), media pause, the recovery hot key (`recovery.rs`) and the release token roots. A change to one of them keeps the ways out intact on all three, and CI runs the gates on each.
- **Bugs.** With the steps, what you expected, what happened, and the Logs page's run for a failed lesson.

## How a change is made

1. Branch from `main`: `feat/...`, `fix/...`, `docs/...`.
2. Keep the change to one thing. A refactor and a feature are two pull requests.
3. Tests go with the change: Rust integration tests under `src-tauri/tests/`, unit tests beside the code, Vitest for frontend logic (`*.test.ts`).
4. Write commit messages as plain sentences that say what changed and why: a short first line, then a paragraph if the why is not obvious. No trailers, no attribution lines.
5. Open the pull request against `main` with the template filled in. Pull requests from forks wait for a maintainer to approve the CI run.

## Style

- Rust: `rustfmt` defaults, Clippy clean at `-D warnings`. Errors are strings the interface can show; authority stays in Rust (lifecycles, locks, grading, selection).
- Svelte 5 with runes. Components read like the surrounding ones; comments explain the reason for a choice, not what the line does.
- Prose in the product and the docs: plain, specific, no marketing. Say what the desk does and what it refuses to do.

## Releases

Versions are tags (`v0.1.0`). The release workflow builds the installers for every platform and attaches them to a GitHub release; the changelog is [CHANGELOG.md](CHANGELOG.md).

## Questions

Open a discussion or an issue. For anything about the lock, the escape routes or a key that could reach the wrong place, see [SECURITY.md](SECURITY.md) first.
