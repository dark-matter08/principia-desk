<script lang="ts">
  import { api, type EngineeringSessionResult, type LessonStage } from '../ipc';
  import { tick, untrack } from 'svelte';
  import { app } from '../stores.svelte';
  import { LessonSession, restoredWork } from '../features/lessons/lesson-session.svelte';
  import type { StageLink } from '../features/lessons/types';
  import LessonShell from '../features/lessons/LessonShell.svelte';
  import KnowledgeCheck from '../features/lessons/KnowledgeCheck.svelte';
  import LessonOutcome from '../features/lessons/LessonOutcome.svelte';
  import CourseChat from '../components/CourseChat.svelte';
  import CoursePurpose from '../components/CoursePurpose.svelte';
  import ExerciseWorkspace from '../components/ExerciseWorkspace.svelte';
  import Markdown from '../components/Markdown.svelte';
  import LessonMap, { type MapStage } from '../features/lessons/LessonMap.svelte';
  import DownloadMenu from '../features/lessons/DownloadMenu.svelte';
  import { readingMinutes, type LessonSection } from '../features/lessons/sections';
  import { ExternalLink, MessageCircle, Sparkles } from 'lucide-svelte';

  const lesson = $derived(app.engineeringLesson);
  let session = $state<LessonSession | null>(null);
  let reflection = $state('');
  let submitting = $state(false);
  let result = $state<EngineeringSessionResult | null>(null);
  let chatOpen = $state(false);
  let scroller = $state<HTMLElement | undefined>(undefined);
  let exerciseSection = $state<HTMLElement | undefined>(undefined);
  let checkSection = $state<HTMLElement | undefined>(undefined);
  let restoredFor: string | null = null;
  /** The lesson's sections as rendered, for the map and the reading position. */
  let sections = $state<LessonSection[]>([]);
  let currentSection = $state(0);
  /** Set once the reader has scrolled past the lesson into practice or the check. */
  let currentStage = $state<MapStage | null>(null);

  const anchors = () => ({ practice: exerciseSection, check: checkSection });

  $effect(() => {
    if (!lesson) return;
    const current = untrack(() => session);
    if (current?.sessionId === lesson.session_id) return;
    current?.dispose();
    session = new LessonSession(lesson);
    const work = restoredWork(lesson);
    reflection = typeof work.reflection === 'string' ? work.reflection : '';
    result = lesson.outcome ?? null;
    chatOpen = false;
  });

  $effect(() => {
    // Restore the saved reading position once per opened lesson.
    if (!lesson || !session || !scroller || restoredFor === lesson.session_id) return;
    restoredFor = lesson.session_id;
    const offset = session.restoreOffset;
    void tick().then(() => requestAnimationFrame(() => {
      if (!scroller || !session) return;
      if (offset > 0) scroller.scrollTop = offset;
      session.stage = result ? 'feedback' : session.stageAt(scroller, anchors());
    }));
  });
  $effect(() => () => {
    if (session && scroller && !result) void session.savePosition(scroller, anchors());
    session?.dispose();
  });

  /** A delayed review recalls, checks and gives feedback; there is no new lesson. */
  const retrieval = $derived(lesson?.kind === 'retrieval');
  const stages = $derived<StageLink[]>(retrieval
    ? [
        { id: 'recall', label: 'Recall', available: true },
        { id: 'check', label: 'Check', available: true },
        { id: 'feedback', label: 'Feedback', available: !!result },
      ]
    : [
        { id: 'learn', label: 'Learn', available: true },
        { id: 'practice', label: 'Practice', available: true },
        { id: 'check', label: 'Check', available: true },
        { id: 'feedback', label: 'Feedback', available: !!result },
      ]);

  function goto(stage: LessonStage) {
    if (!scroller) return;
    const target = stage === 'learn' || stage === 'recall' ? null : stage === 'practice' ? exerciseSection : checkSection;
    if (target) target.scrollIntoView({ behavior: 'smooth', block: 'start' });
    else scroller.scrollTo({ top: 0, behavior: 'smooth' });
  }

  function track() {
    if (session && scroller) session.trackPosition(scroller, anchors(), !!result);
    placeInSections();
  }

  /** The section whose heading last passed the top of the reading pane, or
   *  the practice or check once the reading is behind. */
  function placeInSections() {
    if (!scroller || sections.length === 0) return;
    const top = scroller.getBoundingClientRect().top + 140;
    let at = 0;
    for (const section of sections) {
      if (section.element.getBoundingClientRect().top <= top) at = section.index;
      else break;
    }
    currentSection = at;
    const past = (element: HTMLElement | undefined) => !!element && element.getBoundingClientRect().top <= top;
    currentStage = past(checkSection) ? 'check' : past(exerciseSection) ? 'practice' : null;
  }

  function jump(section: LessonSection) {
    section.element.scrollIntoView({ behavior: 'smooth', block: 'start' });
  }

  /** A Focused/Strict session that engaged the kiosk cannot be paused here. */
  const lockedHere = $derived(app.isFocusLocked(lesson?.session_id));


  async function leave() {
    if (result) {
      await app.finishClass();
      return;
    }
    if (lockedHere || !session) return;
    try {
      await session.pause({ reflection });
    } catch (error) {
      app.error = String(error);
    }
    app.screen = 'idle';
    void app.refresh();
  }

  /** Name the publisher so the learner can weigh a source before opening it. */
  function publisher(url: string): string {
    try {
      return new URL(url).host.replace(/^www\./, '');
    } catch {
      return 'unverified source';
    }
  }

  async function submit() {
    if (!lesson || !session || !session.complete || submitting) return;
    submitting = true;
    try {
      if (session.study) {
        await session.flush();
        const { roundId, revision } = session.round;
        result = await api.submitClassCheck(lesson.session_id, roundId, revision, reflection);
      } else {
        result = await api.submitClassroomEngineeringSession({
          session_id: Number(lesson.session_id),
          answers: session.answers,
          reflection,
        });
      }
      session.stage = 'feedback';
    } catch (error) {
      app.error = String(error);
    } finally {
      submitting = false;
    }
  }
</script>

{#if lesson && session}
  <LessonShell
    route={`classroom/${lesson.subject_id}/${lesson.category}`}
    status={`${lesson.prompt_version} · ${lesson.agent_used}`}
    code={lesson.short_code}
    eyebrow={retrieval ? `${lesson.label} · REVIEW · ${lesson.fresh_sample ? 'fresh sample' : 'repeated sample'}` : `${lesson.label} · ${lesson.estimated_minutes} MIN`}
    chips={[
      ...(lesson.level === 'beginner' ? [{ label: 'beginner', tone: 'good' as const }] : []),
      ...(lesson.research_note ? [{ label: 'unverified sources', tone: 'accent' as const }] : []),
    ]}
    title={lesson.title}
    subtitle={`${lesson.concept_title} · ${lesson.category}`}
    {stages}
    stageMinutes={retrieval ? {} : { learn: readingMinutes(sections) || lesson.plan.learn_minutes, practice: lesson.plan.practice_minutes, check: lesson.plan.check_minutes }}
    stage={result ? 'feedback' : retrieval && session.stage === 'learn' ? 'recall' : session.stage}
    saveMessage={session.saveMessage}
    saveError={session.answerStatus === 'error'}
    minutes={lesson.estimated_minutes}
    locked={lockedHere && !result}
    returnLabel={result ? 'return to classroom' : 'pause class'}
    onreturn={leave}
    onstage={goto}
    bind:scroller
    onscroll={track}
  >
    {#snippet actions()}
      <span class="quality mono"><Sparkles size={11} /> {retrieval ? 'delayed retrieval' : 'isolated teacher'}</span>
      <DownloadMenu source={lesson.runtime === 'study' ? 'study' : 'classroom'} ownerId={lesson.session_id} compact />
      {#if !retrieval}<button
        class="chat-button"
        class:active={chatOpen}
        type="button"
        onclick={() => (chatOpen = !chatOpen)}
        aria-expanded={chatOpen}
        aria-controls="course-chat-drawer"
      >
        <MessageCircle size={12} /> {chatOpen ? 'close chat' : 'ask about this course'}
      </button>{/if}
    {/snippet}

    {#if !retrieval}<CoursePurpose whyNow={lesson.why_now} curriculum={lesson.curriculum} prerequisites={lesson.prerequisites} />{/if}

    <div class="reading-grid" class:with-rail={!retrieval && sections.length > 0}>
    {#if !retrieval && sections.length > 0}
      <aside class="map-rail">
        <LessonMap variant="rail" {sections} current={currentSection} stage={currentStage} plan={lesson.plan} onjump={jump} onstage={goto} />
      </aside>
    {/if}
    <article class="reading-pane" style="font-size: var(--reading-font)">
      {#if !retrieval && sections.length > 0}
        <LessonMap variant="strip" {sections} current={currentSection} stage={currentStage} plan={lesson.plan} onjump={jump} onstage={goto} />
      {/if}
      {#if lesson.research_note}
        <aside class="review-notes unverified" aria-label="Sources could not be retrieved for this lesson">
          <p class="mono">UNVERIFIED · no documentation was retrieved</p>
          <p class="why">{lesson.research_note}</p>
        </aside>
      {/if}
      {#if lesson.review_notes?.length}
        <aside class="review-notes" aria-label="Editor's notes on this lesson">
          <p class="mono">EDITOR'S NOTES · read these claims with care</p>
          <p class="why">The tutor's editor asked for {lesson.review_notes.length === 1 ? 'one more change' : `${lesson.review_notes.length} more changes`} than the corrections allowed, so the lesson ships with the points listed instead of being thrown away.</p>
          <ul>{#each lesson.review_notes as note (note)}<li>{note}</li>{/each}</ul>
        </aside>
      {/if}
      <Markdown markdown={lesson.markdown} lesson={!retrieval} level={lesson.level} onsections={(found) => { sections = found; placeInSections(); }} />

      {#if lesson.resources.length && !retrieval}
        <section class="sources" aria-labelledby="class-sources-title">
          <span class="eyebrow mono">PRIMARY EVIDENCE</span>
          <h2 id="class-sources-title">Continue the investigation</h2>
          <ul>
            {#each lesson.resources as resource}
              <li>
                <a href={resource.url} target="_blank" rel="noreferrer">
                  {resource.title} <ExternalLink size={11} />
                </a>
                <span class="source-host mono">{publisher(resource.url)}</span>
                {#if resource.why}<p>{resource.why}</p>{/if}
              </li>
            {/each}
          </ul>
        </section>
      {/if}

      {#if !retrieval}
        <section class="exercise-end" aria-label="course exercise" bind:this={exerciseSection}>
          <ExerciseWorkspace classroomSessionId={session.study ? undefined : Number(lesson.session_id)} studySessionId={session.study ? lesson.session_id : undefined} minutes={lesson.plan.practice_minutes} />
        </section>
      {/if}

      <aside class="practice-pane" aria-labelledby="class-check-title" bind:this={checkSection}>
        <span class="eyebrow mono">{retrieval ? 'DELAYED RETRIEVAL' : `RETRIEVAL GATE · ABOUT ${lesson.plan.check_minutes} MIN`}</span>
        <h2 id="class-check-title">{retrieval ? 'Retrieve it without the lesson' : 'Prove the mechanism'}</h2>
        <p class="practice-intro">{retrieval ? (lesson.fresh_sample ? 'These samples were not shown in your last lesson on this topic. Your result updates this topic’s review interval.' : 'These are the same questions as your last lesson on this topic; a repeated sample is not proof of fresh transfer, so the result is recorded as a repeat.') : 'Answer from the lesson’s mechanism and evidence. Your result updates only this class.'}</p>

        <KnowledgeCheck
          name="class-question"
          questions={lesson.questions.map((question) => ({ id: question.id, prompt: question.prompt, choices: question.choices, meta: question.learning_objective }))}
          answers={session.answers}
          corrections={result?.corrections ?? null}
          disabled={!!result}
          onchoose={(index, choice) => session?.choose(index, choice)}
        />

        {#if !retrieval}<label class="reflection-field">
          <span>Implementation reflection <small>(optional)</small></span>
          <textarea
            bind:value={reflection}
            disabled={!!result}
            onblur={() => session?.saveWork({ reflection })}
            placeholder="What will you test, change, or measure in a real frontend?"
          ></textarea>
        </label>{/if}

        <LessonOutcome
          result={result ? { passed: result.passed, score: result.score, headline: result.passed ? (retrieval ? 'retention confirmed' : 'evidence recorded') : (retrieval ? 'back to practice' : 'review due'), message: retrieval ? (result.fresh_sample ? 'A passed review lengthens this topic’s interval; a failed one returns it to practice.' : 'Recorded as a repeated sample: the interval is unchanged by this pass alone.') : 'Corrections remain visible above; the class never locks the app.' } : null}
          busy={submitting}
          disabled={!session.complete}
          submitLabel={retrieval ? 'check retention' : 'check and record evidence'}
          busyLabel="recording…"
          hint={`Answer all ${lesson.questions.length} questions to record evidence.`}
          returnLabel="return to classroom"
          onsubmit={submit}
          onreturn={() => app.finishClass()}
        />
      </aside>
    </article>
    </div>

    {#if !retrieval}<CourseChat classroomSessionId={session.study ? undefined : Number(lesson.session_id)} studySessionId={session.study ? lesson.session_id : undefined} bind:open={chatOpen} />{/if}
  </LessonShell>
{:else}
  <div class="empty">
    <p>No engineering class is active.</p>
    <button type="button" onclick={() => (app.screen = 'idle')}>return to classroom</button>
  </div>
{/if}

<style>
  .review-notes { margin: 0 0 22px; padding: 14px 16px; border: 1px dashed var(--led-warn, var(--accent)); border-radius: var(--radius-panel); background: var(--warn-bg, rgba(255, 200, 100, .06)); color: var(--fg); font-size: 13px; }
  .review-notes .mono { font-size: 9px; letter-spacing: .7px; color: var(--led-warn, var(--accent)); margin: 0 0 6px; }
  .review-notes.unverified { border-color: var(--bad-fg); background: var(--bad-bg); }
  .review-notes.unverified .mono { color: var(--bad-fg); }
  .review-notes.unverified .why { color: var(--fg); margin: 0; }
  .review-notes .why { margin: 0 0 8px; font-size: 12px; color: var(--muted); line-height: 1.55; }
  .review-notes ul { margin: 0; padding-left: 18px; } .review-notes li { margin: 4px 0; line-height: 1.55; font-size: 12px; }
  .quality { display: flex; align-items: center; gap: 5px; color: var(--faint); font-size: 8px; }
  .chat-button { display: inline-flex; align-items: center; gap: 6px; cursor: pointer; font-family: var(--font-mono); white-space: nowrap; }
  /* The reading measure: wide enough to use a large window without the lines running long, centred in what the rail leaves. */
  .reading-pane { width: min(100%, 1080px); margin: 0 auto; padding: 28px clamp(24px, 4vw, 56px) 64px; min-width: 0; }
  /* The section map keeps the reader company: a rail beside the reading on a
     wide window, a sticky strip above it on a narrow one (see LessonMap). */
  .reading-grid { display: grid; grid-template-columns: minmax(0, 1fr); }
  .map-rail { display: none; }
  @media (min-width: 1180px) {
    /* The rail sits against the reading column and the two are centred together. */
    .reading-grid.with-rail { grid-template-columns: 236px minmax(0, 1080px); justify-content: center; gap: 8px; padding: 0 20px; }
    .reading-grid.with-rail .map-rail { display: block; position: sticky; top: 14px; align-self: start; max-height: calc(100vh - 210px); overflow-y: auto; padding-top: 28px; scrollbar-width: thin; }
    .reading-grid.with-rail .reading-pane { margin: 0; width: 100%; }
  }
  .exercise-end { margin-top: 40px; border-top: 1px solid var(--node-border); }
  .practice-pane { margin-top: 12px; border-top: 1px solid var(--node-border); border-radius: var(--radius-panel); padding: 24px; background: var(--surface); font-size: 15px; }
  .practice-pane h2, .sources h2 { font-size: 16px; margin: 6px 0; }
  .eyebrow { color: var(--accent); font-size: 9px; letter-spacing: 1.4px; }
  .practice-intro { color: var(--muted); font-size: 11px; line-height: 1.5; }
  .sources { margin-top: 28px; border: 1px solid var(--node-border); border-radius: var(--radius-panel); padding: 16px; }
  .sources p { color: var(--muted); font-size: 12px; line-height: 1.6; }
  .sources ul { padding-left: 18px; }
  .sources a { color: var(--accent); display: inline-flex; gap: 5px; align-items: center; }
  .source-host { color: var(--faint); font-size: 9px; margin-left: 6px; text-transform: lowercase; }
  .reflection-field { display: grid; gap: 6px; margin-top: 14px; color: var(--muted); font-size: 10px; }
  .reflection-field textarea { min-height: 92px; resize: vertical; background: var(--bg); color: var(--text); border: 1px solid var(--node-border); border-radius: var(--radius-control); padding: 9px; }
  button:focus-visible, textarea:focus-visible, a:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
  .empty { min-height: 100vh; display: grid; place-content: center; gap: 10px; }
</style>
