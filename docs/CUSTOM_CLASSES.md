# Your own classes

The desk ships nine courses. A learner can also make a class of their own: name what they want to be able to do, let the tutor draft a curriculum for it (or write one by hand), edit it until it reads right, have the tutor check it, verify that its sources exist, and enroll in it like any other class. A class made this way runs on the same machinery as a bundled one: the same lesson generation, the same session plans, blocks, retrieval, appointments and enforcement, the same starting-point flow, the same export of lessons. Nothing about it is a lesser mode.

This document is the plan and the contract. Section 1 is the learner's process, step by step. Section 2 is what each step needs underneath. Section 3 is what stays out of scope for the first cut, and why.

## 1. The learner's process

### 1.1 Where it starts

Two places, both labelled **New class**: the button beside "Browse courses" on the Classes page and the last card in the course browser. Either opens the class builder, a screen with a five-step rail like the setup wizard: **Brief → Draft → Review → Verify → Publish**.

### 1.2 Brief: what you want to be able to do

The learner writes, in their own words:

- a **title** ("Rust for command-line tools");
- the **outcome** in one or two sentences ("build and ship a small CLI with clean error handling, tests and a release binary");
- what they **already know** that bears on it (optional; "comfortable in Python, never touched a systems language");
- **sources they trust** (optional; hostnames such as `doc.rust-lang.org`, `docs.rs`). The tutor adds the obvious ones; the learner can strike any.

The active tutor is shown and can be changed here (it becomes the class's tutor). The brief is saved as a draft the moment it is typed, so leaving and coming back loses nothing.

### 1.3 Draft: the tutor writes the curriculum, or you do

Three ways forward, all always available:

1. **Ask the tutor.** One call produces the whole course definition: a short code and native label, a one-line summary, the context (how the subject should be taught), the outcome (the bounded artefact the course ends in), the working environment, the source hosts, four stages, and 12 to 36 topics. Every topic carries what the bundled curricula carry: a title, a category, a stage, whether it is core, its prerequisites among the other topics, a learner outcome, at least two named mechanisms, a production scenario, misconceptions, the evidence a lesson must show, the artefact it leaves behind, and at least two primary-source URLs on the allowed hosts. The call streams to the execution feed like lesson preparation does, and a draft that fails the validator is sent back once with the reasons before the learner sees it.
2. **Write it yourself.** An empty course with one stage and one topic, in the same editor as step 3.
3. **Import a file.** A `.principia-class.json` exported from this or another desk (see 1.7).

### 1.4 Review: edit until it reads right

The editor shows the course header (title, summary, outcome, context, environment, hosts) and the topics grouped by stage. A topic is a card that opens to its fields. The learner can add, remove and reorder topics, move them between stages, mark them core or elective, edit any field, and pick prerequisites from the other topics. Validation runs as they type and is listed beside the rail:

- at least two stages, each with at least one core topic;
- 6 to 60 topics, titles unique, every prerequisite an earlier or same-stage topic, no cycles;
- each topic's brief passes the same checks as the bundled seed (`CurriculumBrief::validate`);
- every primary source is an absolute URL on one of the course's hosts (the editor offers to add a missing host);
- the summary, outcome and context are non-trivial.

Nothing can be saved as a course while a check fails; the draft itself is always saveable.

### 1.5 Verify: three checks, in order, all required

The Verify step is a sequence. Each check unlocks the next, and the class cannot be published until all three pass; `publish` refuses with the same reasons the step shows.

1. **The tutor reads it back.** A second call reads the finished draft against a fixed bar (`class_builder::CURRICULUM_BAR`: what counts as a high, medium or low problem, and what is not a problem at all) and returns findings: a topic whose outcome is not measurable, a prerequisite that should exist and does not, two topics that are one, a stage that jumps, a source that does not support the topic it is attached to. The first read is the full one: every problem in one pass, with no cap, so the list is complete the first time. Each finding has a severity, a proposed fix and a status. Every finding has to leave *open* before the next check unlocks, in one of three ways: **Fix with the tutor** asks the tutor to make the change (a patch the desk applies: header fields, topics added or replaced by slug, topics removed, a new order); **Open the topic** jumps into the editor, where the card opens, scrolls into view and blinks, and **Fixed by hand** marks it once the edit is made; **Dismiss** records a reason and counts as settled. **Fix all** runs the tutor over every open finding in turn, each change saved before the next. A settled finding can be reopened. A later read is a *confirmation* read, not a second audit: the tutor is given every earlier finding with what the learner did about it and raises only what the changes broke or a high-weight problem it missed; it is told not to bring back medium or low problems that were already there, so the list closes instead of growing. A fresh finding that repeats an earlier one keeps its settlement; earlier findings the tutor does not raise again stay in the list marked *earlier read*, settled or still open (`custom::merge_reviews`). The step says when the read was made on an earlier draft; another read after settling everything is optional.
2. **Fetch every source.** The desk fetches every primary source through the research client (the same one lessons use, with the same host allowlist and the mirrors it knows). Each URL is marked reachable, redirected, or unreachable. One that did not answer blocks publishing until it is settled: **Find another** asks the desk for a replacement (the configured search engine on the course's hosts first, then the tutor's proposals, each candidate fetched before it is taken, `Generator::replace_custom_course_source`) and writes it into every topic that cited the dead address; **Find another for all** does that for every pending source in turn; or it is replaced in the editor, or **kept knowingly** (a lesson that cannot fetch a source says so, as bundled lessons do). A URL added after the last fetch blocks until the sources are fetched again. Acceptance survives a re-fetch while the address is the same.
3. **Your own read-through.** A confirmation that the learner has read every topic and the course header of this version. It is tied to the draft as it stands: an edit after it asks for another confirmation.

The question bank (section 2.7) comes fourth and stays optional; it unlocks after the read-through and can also be written after the class is published.

The writing rules (`docs/WRITING_RULES.md`) apply to the draft as to everything else: em dashes are removed when it is saved, and a field or a topic that reads like a machine wrote it is listed in the editor's rail and blocks publishing until it is changed.

### 1.6 Publish

Publishing makes the course real: it appears in the catalogue with a **custom** badge, gets a class record, and opens like any other class, on its overview. The starting point (foundations, a declared stage, or the placement check if the class has a question bank) and the study times are then set from the class itself, whenever the learner is ready.

Everything after that is the same as any class: schedule, activation, lessons prepared ahead of each study time, blocks, retrieval, PDFs.

### 1.7 Later: edit, export, import

- **Edit curriculum** on the class's Curriculum tab reopens the builder on the published course. Saving publishes a new version; classes already enrolled keep their accepted path, and the class overview says the curriculum has a newer version with a one-click re-accept that keeps the entry point and marks the changed topics.
- **Export** writes one file, `<class>.principia-class.json`, with the course definition, the topics, the prompt and the version, under `Documents/Principia Desk/classes/`. It contains no progress and no keys.
- **Import** reads such a file into the builder at the Review step, with a new id if one already exists.

## 2. What each step needs underneath

### 2.1 A runtime course source

The catalogue is compiled from `seed/catalog.json` into `&'static CourseDefinition`s and everything from enrollment to research to lesson generation looks courses up by id through `catalog::course`. A custom course has to answer the same lookups. The catalogue therefore grows a registry: custom definitions are read from the database at startup (and on every save), each leaked into a `'static` definition, and `catalog::course`, `catalog::all` and `catalog::engineering_ids` answer from the compiled list first and the registry second. The handful of places that iterate `COURSES` directly move to `catalog::all`. Leaking is deliberate: a definition is a few kilobytes, a learner makes a handful of courses, and a definition must outlive every reference the runtime holds.

Custom courses are engineering-kind only. The language runtime is a different machine (scenarios, passes, CEFR bands) and is not what "my own class" means.

### 2.2 Storage (migration v13)

- `custom_courses`: `id` (the subject id, `custom-<slug>`), `version` (integer, `v<n>` outward), `status` (`draft` or `published`), `origin` (`tutor`, `manual`, `import`), `brief_json` (the learner's brief), `draft_json` (the editable draft: definition fields and topics), `definition_json` (the last published definition), `prompt` (the published teaching prompt), `review_json` (the last tutor review, each finding with its status and note), `sources_json` (the last source check, each URL with whether it was accepted), `checks_json` (v15: the hashes of the draft the tutor last read and the draft the learner confirmed), `created_at`, `updated_at`, `published_at`.
- Topics of a published course live in the existing `concepts` table with `focus = <course id>`, slugs prefixed with the course id so they can never collide with the bundled seed; `brief_json` carries the brief. Republishing refreshes metadata by slug and never touches progress columns, exactly as the bundled seed does on upgrade.
- `classroom_programs` gets a row when a course is published, with the class's tutor from the brief.

### 2.3 The teaching prompt

Bundled courses have hand-written prompts. A custom course's prompt is rendered from one template (`prompts/classroom/custom.txt`) with the course's title, context, outcome, environment and hosts, carrying `PROMPT PROFILE: custom.<id>.v<n>` so the same version check applies. The generator's contract-with-goal wrapper, the session plan, the beginner contract and the lesson-shape rules apply unchanged.

### 2.4 The tutor calls

Three calls on the generator, each a bare-JSON exchange with the configured runner through the existing `run_exact_for` path (typed parse, one same-provider repair):

- `draft_course(brief)` → the draft; validated; one correction round with the validator's reasons.
- `review_course(draft, earlier)` → findings `{ severity, topic, message, fix?, status, note, carried }`; the first read is full, later ones confirm against the earlier findings. The draft, the read and the fix prompts share one bar so they agree on what a problem is.
- `fix_custom_course_finding(brief, draft, finding)` → a `DraftPatch` (`note`, `header`, `topics`, `remove`, `order`) that `class_builder::apply_patch` folds into the draft; a patch that leaves the draft worse against the validator goes back once with the reasons and is dropped if still worse.
- `write_course_questions(draft)` → per-topic four-choice questions with a cited source, held to `validate_generated_quiz` plus a host check.

Each call is announced in the execution feed with its own run so the Logs page shows it.

### 2.5 The snapshot and the path

`enrollment::course_snapshot` builds a custom course's snapshot from the database (definition, topics, prompt, no reference lessons) and hashes it as usual, so accepted paths and drift detection work unchanged. The frontend's `courseDefinition(id)` reads a registry filled from `get_catalog` on every state refresh, so class detail, the curriculum map and enrollment render a custom course like a bundled one.

### 2.6 Commands

`get_custom_course_draft`, `save_custom_course_draft`, `draft_custom_course` (tutor), `review_custom_course` (tutor), `fix_custom_course_finding` and `fix_all_custom_course_findings` (tutor), `resolve_custom_course_finding` (fixed by hand, dismissed with a reason, or reopened), `verify_custom_course_sources`, `replace_custom_course_source` and `replace_all_custom_course_sources` (search, then tutor), `accept_custom_course_source`, `mark_custom_course_read`, `publish_custom_course`, `export_custom_course`, `import_custom_course`, `delete_custom_course` (draft only; a published course with history is retired, not deleted).

The view carries `checks`: whether the review exists and is current, how many findings are open, whether the sources were fetched, how many were added since or are unreachable and not accepted, whether the read-through is confirmed, and `blockers`, the ordered reasons publishing is refused. The marks behind it (the draft the tutor last read, the draft the learner confirmed) are in `custom_courses.checks_json` (migration v15).

### 2.7 The question bank (done)

A third card on the Verify step asks the tutor for the bank: three cited four-choice questions per stage on core topics of that stage, in the placement check's own shape (`placement::Question` with a `source`, and `voided`/`void_reason` for disputes). `class_builder::bank_from_written` holds the tutor's answer to that shape and sends the reasons back once. The bank is stored on the class (migration v14) and registered with the course; `placement::bank` and `has_bank` answer for a custom course from the registry, leaving voided questions out, and a bank counts only while every stage still has three usable questions. Enrollment options and the curriculum map say whether the check and unit challenges are available. A learner disputes a key from the placement result (**This key is wrong**); the question is set aside and listed as disputed in the builder until the bank is written again.

### 2.7a Banks for bundled classes

The same writer serves a bundled class: `class_builder::draft_of_bundled` reads the course definition and its concepts as a draft (slugs bare, so the questions' competencies are the concept slugs), and `write_course_bank` keeps the result in `course_banks` (migration v16), registered in front of the authored bank for `placement::has_bank` and `placement::bank`. **Restore the bundled bank** deletes the row. Every engineering class's Curriculum tab carries a question-bank strip that says which bank stands (bundled, written, none), with the keys to write one or go back.

### 2.7a Practice questions

Every engineering class's Curriculum tab has a **Practice questions** strip: extra cited questions the tutor writes eight at a time for a chosen stage (`Generator::write_practice_questions`, held to the shared question shape by `class_builder::practice_from_written`, electives allowed, prompts already held not repeated), kept in `course_banks` (migration v16) and shown in a practice sheet where a learner picks an answer, sees the key, the explanation and the source, and can dispute a key. They are never sampled by the placement check or the unit challenges, which draw on a separate bank the learner does not see beforehand (the authored one for a bundled class, the one written in the builder for a learner's own class; the strip says which and how many). A bundled class reads as a draft for the writer through `class_builder::draft_of_bundled` (concept slugs bare, so competencies match the course's concepts).

### 2.8 Trying the tutor calls for real

`cargo test --test custom_tutor_live -- --ignored --nocapture` runs the drafting, review and bank calls, and a first read followed by a confirmation read, against a real runner (Ollama and `qwen2.5:7b` by default; `PRINCIPIA_LIVE_AGENT` and `PRINCIPIA_LIVE_MODEL` choose another) and prints what came back. On 2026-09-12 with the 7B model: a draft of 6 to 9 topics in 80 to 135 s that parsed, normalized and passed all but one or two validator checks after the correction round; a review of six findings with proposed fixes; and a bank that the shape check refused with a named reason (a stage two questions short). Larger models are expected to meet the 12-to-36 topic ask and the three-per-stage rule; the checks stand either way.

## 3. Later passes

- **Retrieval from the bank.** Written questions could feed retrieval sessions before a lesson has been taught on the topic; today retrieval draws on the previous lesson's own questions.
- **Editing single questions** in the builder rather than writing the whole bank again after a dispute.
- **Re-accepting a path after a curriculum edit** with the changed topics marked.
- **Retiring a published class.** Today a published class can be paused like any class but not removed; a retire action that hides it from the catalogue while keeping its lessons is the missing piece.
- **Sharing**: an import from a URL, and a catalogue of shared classes. Not now.
