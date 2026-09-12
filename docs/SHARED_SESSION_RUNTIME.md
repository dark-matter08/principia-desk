# Shared study runtime

The native foundation lives in `src-tauri/src/domain/sessions.rs`, with schema v6 in `006_study_sessions.sql`. Existing lesson IPC and the three legacy engines still own their current records. The primary engine must switch its writer, due/consumed decisions, active-session reads and history together before this runtime becomes the user-facing study engine. Engineering classroom and language adapters follow that cutover. This document records an implementation boundary, not completion of P4–P6.

## Identity and planning

A session receives a stable `study-…` ID before provider work begins. A caller-supplied request key makes an identical planning request idempotent, including after completion or a change in class settings. Reusing that key for a different selection fails. A distinct request cannot replace an owner's resumable session.

Class sessions reference the exact accepted path and curriculum snapshot. Planning snapshots the current tutor, focus preference, goal and pace alongside the subject adapter's selected lesson/unit, reason, prerequisite advice and stage sequence. Revising the path or tutor does not rewrite existing sessions. Planning against a stale path or curriculum fails before new work is created.

Earlier daily study has an explicit compatibility owner and a service date in its provenance. It does not create a class or infer a permanent subject assignment. Its date is not session identity. The daily routine has since been retired: preserve its history and resumable work through import/recovery, without creating new daily appointments or an assignment UI. Its engine, background worker, screens and commands were removed once no profile held an in-progress day; the `sessions` rows remain as history for Progress and for the one-time import.

## Lifecycle and preparation

The persisted lifecycle is `planned → preparing → ready → active ↔ paused → completed/skipped`. SQLite constrains transitions, ownership, one resumable session per owner and one logically active session. Activating advisory work pauses the previous foreground session without changing its checkpoint. Focused/strict activation needs a grant from the native focus coordinator (`enforcement.rs`): it admits one holder at a time, refuses unprepared sessions, engages the kiosk at Firm (focused) or Hard (strict), releases on completion, skip or the escape hatch, and re-engages the active holder after a restart once the webview is ready.

Planning creates a durable preparation job. Workers claim it with a bounded lease; a second worker receives no lease while the first is live. Heartbeats extend live leases. After expiry, another worker can recover the same request and invalidate the old token. Failed requests retain their identity and error until an explicit retry. Skipping cancels outstanding preparation.

Publication checks the current lease and freezes one immutable lesson version, including the adapter's content, assessment material and provenance. The lesson and ready state commit together. An identical publication retry returns the original version; a different payload or obsolete token cannot replace it. Provider calls happen outside SQLite transactions.

## Saved work and results

Checkpoints contain the active stage, reading anchor/offset and subject-specific editor/activity state. Work is keyed by session ID and uses its own revision. A delayed save after a class switch still targets its captured session. Competing revisions are rejected; a lost-response retry of the same save is safe. Lifecycle changes and checkpoint saves have separate revisions.

The existing assessment runtime now accepts a `study_session` owner. It retains its frozen rounds, answer drafts, submissions and rubric versions. Class-owned placement remains independent of a lesson's assessment and session slot.

Completion calls a native subject adapter inside one transaction. That adapter validates required work and writes evidence and schedule projections. Unfinished session assessments block completion. Any failed validation or storage write rolls back the result, grade, projection and session transition. An identical completion retry returns the original result without repeating the projection. Skipping retains unanswered work and abandons that session's active assessments; it does not abandon an independent class diagnostic.

Terminal results freeze the submitted checkpoint. Original lesson content, ownership, submitted work and result rows are immutable. Follow-up practice must create a later session/artifact revision rather than changing the original evidence.

## Verification and next integration

`tests/study_sessions.rs` exercises real SQLite connections and process-style reopenings: concurrent planning, competing preparation claims, expiry/recovery, stale worker rejection, save conflicts, foreground handoff, midnight resume, revised-path/tutor isolation, atomic grade/progress failure, result retries, terminal immutability and independent placement. Migration tests retain original assessment records and verify the v5 backup and unchanged v1–v5 ledger entries.

The next integration needs primary session import/crosswalks, compatibility IPC keyed to the stable session, native preparation worker calls, captured timer ownership and simultaneous due/consumed/history reader cutover. The same runtime then supports the classroom and CEFR adapters. Occurrences/timezones, OS enforcement coordination, shared lesson UI and evidence-based curriculum completion remain separate unfinished gates in `PRODUCT_EVOLUTION_PLAN.md`.

## Primary identity boundary

Schema v7 backfills stable IDs for every existing primary row and assigns them
atomically to subsequent rows. `primary_session_ids` and the `sessions` crosswalk
retain that mapping; the eventual shared-runtime import must reuse these IDs.
The compatibility table still permits one primary record per service date. This
is not yet the full primary FSM/storage cutover.

Primary lesson IPC now receives a captured `session_id`: recall and review,
lesson selection, reader preparation, reading start/completion, audio, exit
checks and resource opening no longer choose their owner from the current date.
The frontend captures the ID when each lesson screen opens and remounts the
screen when that ID changes. Feedback reloads its immutable native result rather
than sharing a root-level cache. Out-of-order state refreshes and another
session's timer events cannot replace the current owner. Startup resumes
unfinished primary work before a later pending day, preserving its original
subject. Emergency recovery remains a native global action so it works without
a healthy or correctly positioned lesson screen.

The reading worker uses an explicit owner and total duration. Each charged
second is saved to that still-active reader before its event is published;
failed saves retain the previous counter for retry. Timer events carry the
session ID and the UI ignores another session's events. Reopening the reader
re-establishes its native timer from stored reading time. Duplicate starts share
one worker and read the counter under the timer's guard; late preparation replies
from an unmounted reader do not restart it. Completion joins the
primary state, introduced-concept update and next quiz job in one transaction.

Desktop QA verified a September 19 fixture resumed under a September 20 test
clock, normal Quit at 17 saved seconds, restart with the same ID and remaining
time, and completion of only the original day. The September 20 row stayed
pending with zero reading time. Debug mode also now exempts native window/quit
handling and webview shortcuts from its simulated UI lock. Real enforcement
keeps its existing protections.

Still required: migrate primary state and content into `study_sessions` and
`lesson_versions`, bind the preparation worker and assessment ownership to that
runtime, cut over due/consumed/history projections together, and support legacy
assignment and multiple voluntary sessions without using a date as identity.
## Primary import preflight

`storage/primary_import.rs` now inspects schema-v7 primary learning records in one
read-only SQLite snapshot. It validates the frozen migration ledger and identity
crosswalks, retains original SQLite types/bytes (including unfamiliar JSON), and
reports ambiguous content or missing provenance as recovery work. Pending days
do not acquire a subject from a pre-drawn topic or the current configuration.
Unattached archive documents and unfinished assessment/exercise work are retained.

The fingerprint can be checked again under the eventual cutover's write
reservation. A timer tick, draft save or worker publication invalidates an older
snapshot; edits to another class do not. `primary_import_report` takes an explicit
database path and prints ownership/recovery metadata without lesson bodies,
answers or credentials.

Nine regressions cover exact-byte retention, ambiguous ownership, schema/ledger
validation, concurrent snapshots, legacy main/PR upgrades and stale-plan detection.
This is preparation for the migration: it neither registers a migration nor
switches production writers/readers to the shared runtime.

## Engineering adapter

`subjects/engineering.rs` is the first subject adapter on this runtime. `plan`
selects the next topic on the accepted path (or an explicit revisit), validates a
scheduled rule once per service date across both session stores, and records a
planned session whose selection names the concept, appointment and reason.
`prepare` claims the lease, runs `generate_classroom_course` with the tutor
frozen in the session context and the path revision the session references,
then publishes one lesson version or records the failure for retry. The
knowledge check is an `exit_check` assessment attempt owned by the session,
frozen from the published questions; `save_answer` and `submit` use its round
and revision. `submit` moves the checkpoint to feedback, then `finish`es the
session with a projection that records the submission, `classroom_exit_attempts`
rows keyed by `study_session_id` (schema v8), and mastery. `skip` finishes
without credit. Exercise drafts and completion are checkpoint work.

Readers that must move together with this writer now include shared-runtime
sessions: `slot_state`, `has_active_session`, `delete_slot`, `active_sessions`,
`completed_lesson_count`, the Progress history/archive and the dossier's
misconception feed. Legacy in-progress classroom rows remain resumable through
the compatibility commands until they finish. The `study_fixture` example
publishes a bundled reference lesson into a planned session (or fails the
preparation with `--fail`) for desktop QA without a provider.

## Primary import

`storage/primary_import::apply` imports finished daily-routine sessions as
terminal runtime sessions (`owner_kind = 'daily_routine'`) that reuse the v7
`primary-…` IDs. Completed days get a lesson version (`legacy_lesson` with the
stored markdown, resources, round-one exit checks and exercise, or
`legacy_retrieval` for quiz-only days), a checkpoint with the reading time and
step, and a `study_results` row whose outcome carries the legacy score, reading
time and assessment attempt IDs. Skipped days import without content. The
crosswalk row `('primary_import', date)` makes the import idempotent; open work
and rows with recovery reasons (including a course missing from the catalog)
are left to the compatibility engine. Legacy assessment attempts keep their
`legacy_primary` owner. The app runs the import at startup and logs a summary.
