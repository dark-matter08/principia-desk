# The Teacher, continuous teaching agent

> Spec for evolving the generator from a **one-shot course author** into a
> **persistent teacher** that knows what you've mastered, plans what you learn
> next, varies how it teaches, and exists for the lifetime of the install.

## 1. Role

Today the agent is stateless: each `claude -p` call knows the topic and nothing
else. The Teacher inverts this. Every invocation receives a **learner dossier**
(compact, generated from the knowledge base) and acts in one standing role:

> *You are this student's long-term frontend engineering teacher. You have taught them
> for N days. You know what they have mastered, what they struggle with, and
> what comes next. Your job is to move them to staff-engineer-level judgment
> not to cover topics, but to build durable understanding.*

The Teacher decides, within guardrails, what kind of day today is, how hard
to push, and what to revisit. The app remains the authority on *enforcement*
(lock, timer, streak); the Teacher becomes the authority on *pedagogy*.

### Universal first-principles contract

Every generated teaching path uses
`src-tauri/prompts/first-principles.txt`: primary and classroom courses,
quizzes, grading feedback, exit remediation, course chat, audio lessons, and
language enrichment. The sequence is stable across subjects: establish the
smallest facts and constraints, explain them without circular terminology, use
an analogy and state its limits, derive the larger mechanism, transfer it to a
real situation, and correct mistakes from the first missing building block.
Domain adapters translate that same method into software mechanisms or
sound/symbol/word/sentence language progression. The contract is present even
on day one when no learner dossier exists.

### Classroom teacher isolation

`classroom.rs` is the routing boundary for every advisory subject. German,
Italian, JavaScript/browser internals, TypeScript, frontend architecture, and
developer tooling each have an immutable subject identity plus their own
provider, model, prompt profile, prompt version, schedule slots, sessions, and
progress. A generation call receives a snapshot of that class profile; it
never mutates `Generator`'s global provider/model and therefore cannot leak
teacher configuration into a concurrent class.

The six prompt contracts live separately under
`src-tauri/prompts/classroom/`. Each contract declares the subject and profile
version and explicitly excludes neighboring domains. Engineering classes
reuse the focus-local mastery ledger and strict course validator, but run in
ID-keyed `classroom_sessions`, so several subjects can be completed on one
date without touching the primary one-row-per-day `sessions` loop.
Their reader uses the same continuous course/exercise/chat interaction model
as the primary reader: the autosaved exercise workspace follows the reading
content, course-grounded chat stays available while the lesson is on screen,
and retrieval checks follow the exercise. The tutor receives the authoritative
learner outcome, cumulative artifact, complete course, and structured exercise.
Every answer must identify an exact course section, pass a deterministic shape
gate, and return three follow-up prompts; malformed or invalid output gets one
same-provider/model correction and never crosses to another provider. The UI
renders full Markdown/code, exposes the grounding section, and offers those
follow-ups as actions. Chat history stays bounded and session-only, and student
messages are treated as untrusted context with a 2,000-character limit.
Course generation has no bundled
content substitution: the configured provider must return a lesson with five
validated checks and a structured exercise. Malformed JSON or a missed quality
gate gets one correction pass through that same provider and model; if the
corrected lesson still fails, the real error is shown. Engineering courses
are written in point form to be read in about fourteen minutes of a
thirty-minute session (the rest is practice and the check): they target
1,500–2,100 words, cannot pass below 1,200 or above 2,600, and scale to one and
a half times that for an hour. Per-section floors prevent a long but padded
section from disguising thin mechanisms, production transfer, failure
analysis, observability, or practice. Every section opens with an italic
reading hint (`*~2 min · what to look for*`), the points that matter are
labelled callouts (`> **Key idea:**`, `> **Watch out:**`, `> **Try it:**`,
`> **Example:**`, `> **Decision:**`), and at least two mermaid diagrams picture
the mental model and the production scenario; the gate checks all three and a
lesson that overruns is condensed rather than expanded. The editorial rubric
scores coverage depth separately from mechanism depth and specificity, and
depth means precision, not length.

Retrieval reports what it did. Every source it skipped is logged with its
reason (a timeout, a 404, too few readable words), and a page it could not
reach may be read from a mirror: the GNU Bash, coreutils, grep, sed, awk,
find, make and tar manual pages fall back to the same material on man7.org,
quoting the section about the page's own topic. A lesson written with no
retrieved documentation carries a note naming the hosts that could not be
reached, shown above the lesson in the reader, and a link the curriculum
itself names stays in the reading list even when it cannot be reached at
the time, marked as such.

A class activates only once its learner has chosen a starting point: start
from scratch, find their level with the placement check, or pick a stage.
The desk no longer accepts a default path on their behalf, because a lesson
pitched at the wrong level was the result. A learner starting from scratch
gets beginner lessons for every foundations topic: the tutor's contract
declares an absolute beginner (every term defined before use, one doable step
at a time, an exercise that is a first success, no internals), the editorial
audit judges the draft as a beginner would meet it, and the reader and the
PDF name the fixed sections for that level (`How it works`, `Try it
yourself`, `Common mistakes and how to spot them`). Later phases on the same
path are taught at the standard level.

Tutors without web access (Ollama, OpenRouter, any bare chat API) can be
given a search engine in Settings → Web search: a SearXNG instance (a URL
and no key; Remote Ledger's local instance on port 8899 is the default),
the Brave Search API or Tavily (a key each, kept in the system keychain).
The desk searches for itself, restricted to the subject's allowed hosts,
fetches what it finds, and hands the tutor only pages it retrieved; a
search result is a candidate, never a citation. With search off, lessons
use the curriculum's own sources and their mirrors.

German and Italian deliberately do not reuse the frontend-engineering dossier.
`language.rs` owns their CEFR evidence model with seven strands: listening,
reading, spoken interaction, spoken production, writing, grammar, and
vocabulary/pragmatics. A class-specific agent may enrich the teaching material
in a curated unit, but Rust preserves the scenario, can-do descriptor, phase,
and assessment questions. Generated material must pass a depth, dialogue,
phrase, and production-task gate; otherwise the bundled lesson is used.
The A1 sequence begins from first principles: alphabet and letter names,
sound-spelling correspondences, basic sentence construction, counting, and
number construction are explicit curated foundations. Their staged passes
complete before later scenario units, and enrichment cannot remove the
first-principles section.
Calendar milestones guide pacing, but only scenario completion and skill
evidence can advance a level.

Every classroom slot is advisory. It may surface a reminder, but it never
engages the kiosk, mutates the primary `sessions` row, or affects the primary
enforcement decision. Completed classroom work does contribute to the combined
learning streak and next-course dossier. If the primary session is owed, it
wins and classroom starts/resumes remain disabled until primary completion or
skip.

**Default model: `opus`** via the Claude CLI. Configurable in settings. Course
generation, its one syntax/structural correction when needed, and the final
quality editor use that exact provider and model. The editor scores mechanism
depth, specificity, production transfer, dossier adherence, exercise
alignment, and source discipline. There is no alternate-provider or
bundled-course substitution for engineering courses or course-grounded
quizzes. Grading and non-course auxiliary calls remain latency-sensitive.

**Retrieval before generation.** The Teacher does not supply its own sources.
The app fetches up to five primary documents per lesson (the concept's curated
sources, plus topic-specific pages discovered via MDN search, restricted to an
allowlist of specifications, vendor, and maintainer documentation), extracts
their prose, and injects it as quoted material the course must teach from and
cite inline. A course must link at least three retrieved documents inline; one
same-provider correction pass is allowed before rejection. Every reading-list
URL is verified over the network, and unverifiable links are removed rather
than shown, so a hallucinated citation costs the learner a link instead of
misleading them. When retrieval fails outright the lesson still runs, without
unverified links, and the reason is logged.

**Prose is not requested as JSON.** DeepSeek's JSON mode returns roughly a
third of the prose it writes in plain markdown, so the course body, and any
per-section deepening pass for a section under its word floor, is requested as
markdown, while structured payloads keep the JSON contract.

## 2. Knowledge base (the learner model)

A per-concept mastery ledger, derived from data we already collect (attempts,
carryover, scores) plus notes the Teacher writes after each grading pass.

### 2.1 Mastery lifecycle

Each concept moves through a state machine:

```
unseen → introduced → practicing → mastered → maintenance
                ↘ struggling ↗            ↘ decayed → (re-enters practicing)
```

| State        | Meaning                                                | Transition rule (initial tuning)                       |
| ------------ | ------------------------------------------------------ | ------------------------------------------------------ |
| unseen       | never taught                                            |,                                                      |
| introduced   | course read, first quiz not yet taken                   | session completed                                       |
| practicing   | quizzed at least once, not yet consistent               | first quiz on the topic                                 |
| struggling   | a quiz exposed a serious misconception                  | topic quiz score < 50%                                   |
| mastered     | consistent demonstrated understanding                   | ≥ 80% across 2 quiz encounters ≥ 7 days apart           |
| maintenance  | mastered; only spaced pop-quiz checks                   | automatic after mastered                                |
| decayed      | maintenance check failed                                | pop-quiz miss on a mastered topic                       |

### 2.2 Storage

New tables (SQLite, alongside existing ones):

```sql
mastery   (concept_id PK, state, score_ema, encounters, last_seen_date,
           next_review_date,      -- spaced repetition: 7d → 21d → 60d
           teacher_notes)         -- ≤ 280 chars, written by the agent
profile   (key PK, value)         -- durable free-form: 'weak_areas',
                                  -- 'learning_style_notes', 'days_taught', 'goals'
```

`teacher_notes` is the agent's memory of *you* on that topic ("confuses
linearizability with serializability; ASCII diagrams landed well"). The
grading call is extended to emit these notes; the next encounter with that
topic injects them back. The knowledge base is therefore **self-maintaining**:
no call ever depends on conversation history, only on the dossier built from
these tables.

### 2.3 The dossier

A ~1-page markdown block compiled by Rust and prepended to every Teacher call:

```
Day 47 of teaching. Streak 12d. Multi-topic days used: 9.
MASTERED (11): consistent-hashing, caching-strategies, …
STRUGGLING (2): consensus-raft (score 40%, notes: "confuses term vs index"),
                exactly-once-delivery (failed carryover ×2)
PRACTICING (6): …    DUE FOR REVIEW (3): cap-theorem (last seen 21d ago), …
RECENT COURSES: [last 5 dates + titles]
RECENT EXIT-CHECK MISCONCEPTIONS: [up to 6 distinct missed objectives + notes]
PROFILE: weak_areas=consensus, formal consistency models;
         responds well to concrete numbers and failure stories.
```

## 3. Curriculum: from a wheel to a personal path

The flat least-picked-random wheel became a **progressive curriculum** and then
a personal path per class: selection follows the accepted starting point, path
revisions, accepted bridge lessons and prerequisite order. An optional random
choice among eligible next lessons remains available behind "Choose for me".

- Every selectable concept carries a complete curriculum brief in
  `concepts.json`: learner outcome, irreducible mechanisms, production
  scenario, misconceptions, observable evidence, cumulative artifact, vetted
  primary sources, phase, core/elective status, and cross-track relationships.
- The visible phase arc is foundations → mechanisms → production → synthesis,
  followed by electives. Every track has at least a 30-session core path.
- **The wheel only shows unlocked concepts**: tier 1 unlocks when at least 70%
  of its prereq set is `practicing+`; tiers 2–3 require every prerequisite.
  Among equally fresh concepts, core material and the earliest current phase
  win before tier and debt-aware weighting.
- Weighting within unlocked: struggling-adjacent and due-for-review-adjacent
  topics get higher weight; the Teacher can also pin tomorrow's topic during
  pre-generation ("they just failed quorum questions twice, next lesson:
  quorums revisited via a different angle") with a stated reason, surfaced in
  the UI as `scheduler override — reason: …`.
- Pool exhausted ≠ done: tier-3 synthesis topics are generative (the Teacher
  invents composite design exercises from mastered components), so the
  curriculum never runs dry.

## 4. Session types: not every day is a lecture

At pre-generation time the Teacher picks between the two shipped session
types, within app-enforced bounds. The app guarantees a pop-quiz retrieval
checkpoint after every seven completed sessions when at least six concepts
have been practiced, never on consecutive days:

| Type            | Cadence (guardrail)            | Shape                                                                 |
| --------------- | ------------------------------ | --------------------------------------------------------------------- |
| **lesson**      | default                        | the shared runtime's Recall → Learn → Practice → Check → Feedback pass on the next topic of the personal path |
| **pop-quiz**    | weekly checkpoint plus review-debt overrides; never 2 in a row | no new topic. 12 previously attempted questions, weighted toward `struggling`, `decayed`, and due-for-review concepts. Misses update mastery. |

A retrieval session is recall, check and feedback without a new lesson, served when a topic's spaced review is due. Audio is a delivery mode for
a normal lesson, not a separate session type. Every normal lesson also appends
up to two due/struggling spaced-retrieval questions. Design-lab and dedicated
remediation-day variants remain future work; the current Teacher instead uses
days 7/14/21/30 as cumulative practical-work checkpoints and uses the dossier
to reteach recorded misconceptions inside later lessons.

## 5. Extra topics: the classroom, not extensions

The "one more topic" extension loop was retired: after Completion, extra
learning happens in the **classroom**, any enabled subject starts a session
any time, and a completed track is never re-served automatically. Completed
modules stay out of the wheel (mastered/maintenance concepts are excluded),
retrieval lives in pop-quiz and spaced-review days, and the only way back to a
finished module is an explicit **revisit**. The dossier no longer tracks
`multi-topic days`; appetite shows up as classroom subject choice instead.

## 5a. Audio mode (NotebookLM-style listening days)

Some days the student wants to *listen*, not read, hands busy, ears free.
Audio mode keeps the contract intact: the session still locks, the timer still
runs, and the adaptive exit check still verifies same-day comprehension. The
next session's quiz tests the course again after spacing. Only the medium changes.

### Flow

1. User toggles **`audio: on`** for tomorrow (Completion screen or Idle), or at
   lock-in if the audio is already rendered.
2. During nightly pre-generation the Teacher produces a **dialogue script**
   instead of (or alongside) the markdown course: two hosts, a teacher voice
   and a curious-student voice who asks exactly the questions a learner would
   covering the same outline and key takeaways. Dialogue beats monologue for
   retention while multitasking; this is the NotebookLM trick.
3. The script is rendered to a WAV/M4A **offline, overnight**, generation
   latency is hidden in the same window that already hides course generation.
4. Session day: the course-reader becomes an **audio player node**
   (`sdr://broadcast · 2 hosts · 28:40`): play/pause (pauses the TTL), ±15s,
   transcript scrubber (the script doubles as captions), speed 0.8–1.5×.
   The adaptive exit check and next-session quiz are generated from the same
   course, so listening is still followed by retrieval and feedback.

### Engine: VibeVoice on Apple Silicon

**Yes, VibeVoice is a good fit, specifically because of our pre-generation
architecture.** Assessment:

| Engine | Role | Notes |
| --- | --- | --- |
| **VibeVoice 1.5B / Large** via [mlx-audio](https://github.com/Blaizzy/mlx-audio) | **primary**, overnight render | Purpose-built for exactly this: long-form (up to ~90 min), multi-speaker (up to 4), natural turn-taking podcast audio. MIT licensed, runs locally on M-series through MLX quantized conversions. Slow generation doesn't matter at 3am. |
| **VibeVoice-RealTime 0.5B** | optional, live fallback | Microsoft's 2026 streaming variant (<300 ms latency); usable when the user requests audio at lock-in with nothing pre-rendered. Lower fidelity, single-speaker bias. |
| **macOS `say` / AVSpeechSynthesizer** | guaranteed fallback | Zero dependencies, instant, offline. Robotic but never broken, the "bundled course" of audio. |

Caveats to design around:
- **Heavy optional dependency**: Python env + several-GB model weights. Ship as
  an opt-in feature that installs lazily on first enable (`uv`-managed venv in
  app-support dir), never as part of the base app. Audio mode greys out with
  `engine: not provisioned` until installed.
- **Render pipeline is a queue job** like course generation: `generation_jobs`
  gains an `audio` job type; failure falls through VibeVoice → `say` → text
  mode, so an audio day can never block the session.
- **Verify-don't-trust**: hallucinated/garbled segments are rare but real;
  keep the transcript visible so the student can fall back to reading any
  section, and cap rendered length at the script, never freeform.

## 6. Pipeline changes (mapping to today's code)

| Pipeline area | Shipped behavior |
| --- | --- |
| course generation | `generate_course(title, category, dossier, focus, curriculum)` receives the standing Teacher prompt, full curriculum brief, track context, month outcome, and unified learner dossier |
| course/quiz output | parsed structured JSON is checked by deterministic gates, then engineering courses receive one same-provider editorial pass |
| quiz | five fresh, course-grounded questions; malformed or invalid output gets one same-provider correction and never unrelated bundled substitution |
| grade | verdicts + plain-language feedback + private teacher notes, with the dossier available to the grader |
| selection | accepted path first (bridges, then required topics in phase order), prerequisite graph, complete foundations for tier 2/3, then debt-aware weighting |
| model/provider | exact configured provider/model for engineering course, correction, editor, and aligned assessment; errors surface after the allowed same-provider correction |
| pre-generation | plans lesson vs pop-quiz, then builds the course/quiz required for that day |

Everything stays headless one-shot CLI calls; continuity lives entirely in
SQLite + the dossier. No daemon-resident agent, no conversation state to lose.

## 7. UI vocabulary (topology language)

- Knowledge base node: `mastery-store`dashboard gets a per-category mastery
  grid (unseen→maintenance as LED states) replacing/augmenting pool-coverage.
- Pop-quiz day ClusterBar: `sdr://audit · surprise compliance check`.
- Design-lab: `sdr://loadtest · scenario exercise`.
- Teacher override on the wheel: `scheduler override` MetaBadge with reason.
- Wheel growth: locked tiers rendered as greyed "provisioning…" slots.

## 8. Build order

1. **M1, Knowledge base**: `mastery` + `profile` tables, transitions computed
   from existing attempts at grading time, dossier builder, dashboard mastery
   grid. (No prompt changes yet; pure substrate.)
2. **M2, Teacher voice**: `teacher_system.txt` + dossier injected into
   course/quiz/grade prompts; grading emits `teacher_notes`; model → opus
   default with downgrade chain.
3. **M3, Curriculum**: tiers + prereqs in seed data, unlock logic, weighted
   wheel, Teacher topic-override in nightly planning call.
4. **M4, Session types**: pop-quiz day first (cheapest, pure reuse), then
   remediation, then design-lab (needs free-text editor + rubric grading).
5. **M5, Classroom** (replaced the elastic-days loop): per-subject programs,
   schedules, completion-aware rotation, and opt-in revisits.
6. **M6, Audio mode**: dialogue-script prompt + audio player UI on macOS
   `say` first (proves the flow with zero deps), then the VibeVoice/mlx-audio
   provisioned engine as the quality tier.

Current engineering generation fails visibly after its permitted same-provider
repair/editor path; it never silently changes provider, topic, or course.
