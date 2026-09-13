# Course catalog and curriculum authoring

`src-tauri/seed/catalog.json` is the inventory for Principia Desk. Each entry supplies a stable subject/course ID, curriculum version, labels, outcome, environment, recommended prerequisite courses, entry stages, capabilities, primary-source hosts and bundled reference lessons.

Cargo's build script compiles the manifest and prompt files into native definitions exposed by `get_catalog`. `focus.rs` and `classroom.rs` retain compatibility aliases rather than separate subject lists. The browser imports the same manifest. Generate its ID unions after changing course identities:

```sh
npm run catalog:generate
npm run check
```

`catalog:check` rejects stale generated types. Never edit `src/lib/catalog.generated.ts` directly.

Engineering curriculum briefs live in `src-tauri/seed/concepts.json`. Every topic needs a concrete outcome, mechanisms, scenario, misconception, observable evidence, artifact and primary references. Prerequisites must exist in the same course, have no later tier and form an acyclic graph. `related_concepts` names cross-course connections; these are teaching context, not automatic placement credit. IDs and historical learning fields are preserved when authored metadata refreshes.

The bundled catalog has seven engineering courses and two CEFR language courses; a learner's own classes (`docs/CUSTOM_CLASSES.md`) join it at runtime. System Design has 72 topics; Linux Bash and Bash Scripting each start with 18. Course length follows its scope; a course is not padded to a fixed number of sessions.

Keep entry-stage labels aligned with authored core work. The labels describe the curriculum structure; a learner's chosen starting point, the placement check and the accepted path that decides what is eligible live in `domain/enrollment.rs`, `domain/placement.rs` and `domain/classes.rs`, and read these stages as their map.

`reference_lessons` names JSON files under `seed/fallback_courses`. Their role is browser preview content and auxiliary assessment material. They are never silently substituted for a requested generated engineering lesson. Keep examples executable and questions grounded in the accompanying explanation. The two initial shell examples also live under `labs/`; update the corresponding script and bundled example together.

Primary documentation hosts are course-specific. A source must pass that course's policy before retrieval; shell and distributed-systems courses use their authored references instead of MDN discovery. PDF links can remain useful learner resources, but the current prose retriever only extracts text/HTML/Markdown, so include enough readable primary pages as well.

Useful checks:

```sh
cargo test --manifest-path src-tauri/Cargo.toml --lib --tests
npm test
# Explicit network-only check; does not call a lesson provider:
cargo test --manifest-path src-tauri/Cargo.toml --test research_live systems_and_shell_courses_retrieve_subject_specific_readable_sources -- --ignored --nocapture
```

`tests/catalog.rs` checks native catalog/seed ownership and preserves historical records across refresh/reopen. `src/lib/catalog.test.ts` checks preview curriculum, subject-specific references and grading against their actual answer keys. Full migration and placement validation remain separate workstreams in `PRODUCT_EVOLUTION_PLAN.md`.
