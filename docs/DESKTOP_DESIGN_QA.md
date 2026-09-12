# Desktop design verification, 2026-09-09

Tested the packaged macOS Tauri app from `tauri://localhost`, using the real Rust
commands and SQLite storage. The QA build uses identifier
`com.darkmatter.principia-desk.qa`, separate from the existing production data.
The shipping identifier was `com.darkmatter.system-design-roulette` when this
check ran; it is now `com.darkmatter.principia-desk`, and the first launch under
the new identity adopts the profile written under the old one.

The native display name is **Principia Desk**. The original blueprint grid,
Fraunces headings, compact mono controls, node panels and app mark remain the
design reference. New course glyphs are inline SVG, with no remote asset dependency.

## Verified

- Packaged app starts, completes setup and relaunches with persisted preferences.
- All nine native catalog courses render. Searching `bash` finds Linux Bash and
  Bash Scripting; study actions and management controls occupy separate rows.
- Custom language-level dropdown opens above card boundaries without a native
  translucent menu. Arrow + Enter selects; Escape cancels an uncommitted change;
  Tab advances focus; type-ahead finds B1.
- German A2 → B1 settings save through native IPC, are present in SQLite and
  remain visible after relaunch. Database integrity check returns `ok`.
- Bash Scripting enrollment uses the same dropdown; manual entry at Integration
  and capstone autosaves through the native enrollment command. The UI continues
  to state that path acceptance is pending implementation.
- The provider dropdown flips above its trigger near the window bottom. Explicit
  trigger focus handles WebKit pointer behavior, and ResizeObserver keeps the
  menu aligned when layout changes.
- At 640 × 540 the native navigation remains visible, and the enrollment menu
  fits the window and opens upward. Clicking outside dismisses it without changing
  the value. The saved Bash Scripting starting stage remains after relaunch.
- A native relaunch exposed dark text and missing navigation paint. Explicit noir
  text/color-scheme, a dark native title bar and a positioned navigation header
  restore the intended contrast; the corrected Today screen was inspected.
- Source audit finds no `<select>`, `<option>`, browser time inputs or JS native
  alert/confirm/prompt calls in the app. Remaining semantic inputs are styled.

## Checks and limits

`npm run check` reports no Svelte errors or warnings. All 34 existing frontend
tests pass. `npm run tauri build -- --debug --bundles app` completes with the QA
identifier override. All three scheduler unit tests pass after updating the macOS
fallback bundle path; scheduler registration identities are retained. Rust
formatting and `git diff --check` pass. See the product evolution plan for remaining implementation
and release gates.

Native testing uses `--debug-day` to suppress kiosk and OS scheduler changes.
This pass does not certify hard enforcement, live provider generation, signing,
notarization, or Windows/Linux runtime behavior. It verifies the packaged macOS
UI, native persistence and the changed controls.

## Schedule overlap verification, 2026-09-10

Rebuilt the packaged debug app with the QA identifier and opened the existing
isolated profile, which upgraded from schema v2 to v7 with a pre-upgrade backup;
SQLite integrity returned `ok`. Driven through the native window with
AppleScript clicks, direct-digit time entry and accessibility focus:

- TypeScript: Schedule → Add study time → 09:00 Mon–Fri saved; Activate class
  succeeded and the header showed Active with the time listed as due.
- JavaScript & Browser: 09:15 Mon–Fri showed the OVERLAPS ANOTHER STUDY TIME
  block listing all five weekdays against TypeScript 09:00 (30 min) and kept
  Save disabled; changing the minutes to 30 cleared it, the 09:30 time saved and
  the class activated. SQLite held both manual rules.
- TypeScript Settings: 45 minutes per session submitted the form and displayed
  "TypeScript on Monday at 09:00 overlaps JavaScript & Browser on Monday at
  09:30 (30 min) and 4 other overlaps"; `session_minutes` stayed 30.

The planner's conflict preview and commit refusal are covered by native tests
(`planner_previews_conflicts_without_writing_and_refuses_to_commit_them`) and
the shared frontend helper tests; its rendering reuses the editor's block and
was not separately captured. Screenshots for this pass were kept under
`/tmp/principia-shots/` (13-js-conflict, 15-js-saved, 19-ts-length-conflict).
