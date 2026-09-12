<script lang="ts">
  import { api, type LanguageSessionResult, type LessonStage } from '../ipc';
  import { tick, untrack } from 'svelte';
  import { app } from '../stores.svelte';
  import { LessonSession, restoredWork } from '../features/lessons/lesson-session.svelte';
  import type { StageLink } from '../features/lessons/types';
  import LessonShell from '../features/lessons/LessonShell.svelte';
  import KnowledgeCheck from '../features/lessons/KnowledgeCheck.svelte';
  import LessonOutcome from '../features/lessons/LessonOutcome.svelte';
  import Dropdown from '../components/Dropdown.svelte';
  import Markdown from '../components/Markdown.svelte';
  import { Headphones, MessageSquareText, Pause, Play, Volume2 } from 'lucide-svelte';

  const confidenceOptions = [
    { value: 1, label: '1 · Needed the script' }, { value: 2, label: '2 · Many pauses' },
    { value: 3, label: '3 · Understandable with help' }, { value: 4, label: '4 · Mostly clear' },
    { value: 5, label: '5 · Clear and independent' },
  ];
  const lesson = $derived(app.languageLesson);
  let session = $state<LessonSession | null>(null);
  let writingResponse = $state('');
  let speakingCompleted = $state(false);
  let listened = $state(false);
  let confidence = $state(3);
  let speaking = $state(false);
  let submitting = $state(false);
  let result = $state<LanguageSessionResult | null>(null);
  let scroller = $state<HTMLElement | undefined>(undefined);
  let practiceSection = $state<HTMLElement | undefined>(undefined);
  let checkSection = $state<HTMLElement | undefined>(undefined);
  let restoredFor: string | null = null;

  const anchors = () => ({ practice: practiceSection, check: checkSection });
  const work = () => ({ writing_response: writingResponse, speaking_completed: speakingCompleted, listened, confidence });

  $effect(() => {
    if (!lesson) return;
    const current = untrack(() => session);
    if (current?.sessionId === lesson.session_id) return;
    current?.dispose();
    session = new LessonSession(lesson);
    const saved = restoredWork(lesson);
    writingResponse = typeof saved.writing_response === 'string' ? saved.writing_response : '';
    speakingCompleted = saved.speaking_completed === true;
    listened = saved.listened === true;
    confidence = typeof saved.confidence === 'number' && saved.confidence >= 1 && saved.confidence <= 5 ? saved.confidence : 3;
    result = lesson.outcome ?? null;
  });

  $effect(() => {
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
    stopSpeaking();
  });

  const stages = $derived<StageLink[]>([
    { id: 'learn', label: 'Learn', available: true },
    { id: 'practice', label: 'Practice', available: true },
    { id: 'check', label: 'Check', available: true },
    { id: 'feedback', label: 'Feedback', available: !!result },
  ]);

  function goto(stage: LessonStage) {
    if (!scroller) return;
    const target = stage === 'learn' ? null : stage === 'practice' ? practiceSection : checkSection;
    if (target) target.scrollIntoView({ behavior: 'smooth', block: 'start' });
    else scroller.scrollTo({ top: 0, behavior: 'smooth' });
  }

  function track() {
    if (session && scroller) session.trackPosition(scroller, anchors(), !!result);
  }

  /** A Focused/Strict session that engaged the kiosk cannot be paused here. */
  const lockedHere = $derived(app.isFocusLocked(lesson?.session_id));

  async function leave() {
    stopSpeaking();
    if (result) {
      await app.finishLanguage();
      return;
    }
    if (lockedHere || !session) return;
    try {
      await session.pause(work());
    } catch (error) {
      app.error = String(error);
    }
    app.screen = 'idle';
    void app.refresh();
  }

  function stopSpeaking() {
    if (typeof speechSynthesis !== 'undefined') speechSynthesis.cancel();
    speaking = false;
  }

  function speak(text: string) {
    if (typeof speechSynthesis === 'undefined' || !lesson) {
      app.error = 'Speech synthesis is not available in this webview.';
      return;
    }
    speechSynthesis.cancel();
    const utterance = new SpeechSynthesisUtterance(text);
    utterance.lang = lesson.speech_locale;
    utterance.rate = lesson.level === 'A1' ? 0.82 : lesson.level === 'A2' ? 0.9 : 0.96;
    utterance.onstart = () => {
      speaking = true;
      if (!listened) {
        listened = true;
        session?.saveWork(work());
      }
    };
    utterance.onend = () => (speaking = false);
    utterance.onerror = () => {
      speaking = false;
      app.error = 'The selected system voice could not play this passage.';
    };
    speechSynthesis.speak(utterance);
  }

  async function submit() {
    if (!lesson || !session || !session.complete || submitting) return;
    submitting = true;
    try {
      if (session.study) {
        await session.flush();
        const { roundId, revision } = session.round;
        result = await api.submitClassLanguageCheck(lesson.session_id, roundId, revision, work());
      } else {
        result = await api.submitLanguageSession({ session_id: Number(lesson.session_id), answers: session.answers, ...work() });
      }
      session.stage = 'feedback';
      stopSpeaking();
    } catch (error) {
      app.error = String(error);
    } finally {
      submitting = false;
    }
  }
</script>

{#if lesson && session}
  <LessonShell
    route={`languages/${lesson.language}/${lesson.level.toLowerCase()}`}
    status="advisory practice · frontend schedule remains independent"
    code={lesson.language === 'german' ? 'DE' : 'IT'}
    eyebrow={`${lesson.label} · CEFR ${lesson.level} · pass ${lesson.phase}`}
    title={lesson.title}
    subtitle={`${lesson.phase_label} · about ${lesson.estimated_minutes} minutes`}
    {stages}
    stage={result ? 'feedback' : session.stage}
    saveMessage={session.saveMessage}
    saveError={session.answerStatus === 'error'}
    minutes={lesson.estimated_minutes}
    locked={lockedHere && !result}
    returnLabel={result ? 'return to learning programs' : 'pause and return'}
    onreturn={leave}
    onstage={goto}
    bind:scroller
    onscroll={track}
  >
    {#snippet actions()}
      <div class="evidence-status mono" aria-label="Session evidence status">
        <span class:ready={listened}>audio {listened ? '✓' : '○'}</span>
        <span class:ready={speakingCompleted}>speech {speakingCompleted ? '✓' : '○'}</span>
        <span class:ready={writingResponse.trim().length > 0}>writing {writingResponse.trim().length > 0 ? '✓' : '○'}</span>
      </div>
    {/snippet}

    <article class="reading-pane" style="font-size: var(--reading-font)">
      <div class="mission">
        <span class="meta-label">ACTION-ORIENTED MISSION</span>
        <p>{lesson.scenario}</p>
        <strong>{lesson.can_do}</strong>
      </div>

      <div class="audio-console">
        <div>
          <Volume2 size={16} />
          <span>
            <strong>Listen to the model exchange</strong>
            <small>System voice · {lesson.speech_locale} · slowed at beginner levels</small>
          </span>
        </div>
        {#if speaking}
          <button type="button" onclick={stopSpeaking}><Pause size={13} /> stop</button>
        {:else}
          <button type="button" onclick={() => speak(lesson.listen_text)}><Play size={13} /> play dialogue</button>
        {/if}
      </div>

      <div class="paper theme-scholar">
        <Markdown markdown={lesson.markdown} />
      </div>

      <section class="production-stage" aria-labelledby="production-title" bind:this={practiceSection}>
        <div class="practice-intro">
          <span class="meta-label">PRODUCTION</span>
          <h2 id="production-title">Show what you can do</h2>
          <p>Writing and speaking evidence is saved with this lesson and can be strengthened in a later pass.</p>
          {#if lesson.phase_requirement}<p class="requirement mono" role="note">This pass counts when: {lesson.phase_requirement}</p>{/if}
        </div>
        <section class="production">
          <div class="production-head">
            <MessageSquareText size={15} />
            <h3>Writing evidence</h3>
          </div>
          <p>{lesson.writing_prompt}</p>
          <textarea
            bind:value={writingResponse}
            disabled={!!result}
            rows="6"
            aria-label="Writing response in the language you are learning"
            placeholder={`Write in ${lesson.native_label}. Aim for at least 12 words; clear, level-appropriate language is enough.`}
            onblur={() => session?.saveWork(work())}
          ></textarea>
          <div class="word-count mono">{writingResponse.trim() ? writingResponse.trim().split(/\s+/).length : 0} words</div>
        </section>
        <section class="production">
          <div class="production-head">
            <Headphones size={15} />
            <h3>Speaking evidence</h3>
          </div>
          <p>{lesson.speaking_prompt}</p>
          <label class="speaking-check">
            <input type="checkbox" bind:checked={speakingCompleted} disabled={!!result} onchange={() => session?.saveWork(work())} />
            <span>I said this aloud without reading every line</span>
          </label>
          <div class="confidence"><Dropdown label="How understandable did it feel?" bind:value={confidence} options={confidenceOptions} disabled={!!result} /></div>
        </section>
      </section>

      <aside class="practice-pane" aria-labelledby="practice-title" bind:this={checkSection}>
        <div class="practice-intro">
          <span class="meta-label">RETRIEVAL</span>
          <h2 id="practice-title">Check what stuck</h2>
          <p>Answer from memory. Wrong answers return with a concrete explanation.</p>
        </div>

        <KnowledgeCheck
          name="language-question"
          questions={lesson.questions.map((question) => ({ id: question.id, prompt: question.prompt, choices: question.choices, meta: question.strand.replaceAll('_', ' ') }))}
          answers={session.answers}
          corrections={result?.corrections ?? null}
          disabled={!!result}
          onchoose={(index, choice) => session?.choose(index, choice)}
        />

        <LessonOutcome
          result={result ? {
            passed: result.passed,
            score: result.score,
            headline: result.passed ? 'evidence recorded' : 'retrieval needs another pass',
            message: result.passed
              ? `Progress remains at ${result.current_level} until every skill strand clears the level gate.`
              : result.requirement
                ? `Not counted yet: ${result.requirement} Your answers and practice are kept; this scenario will return.`
                : 'This scenario will return. Review the explanations rather than memorising choice letters.',
            extra: result.level_advanced_to ? `Level gate passed; now entering ${result.level_advanced_to}.` : null,
          } : null}
          busy={submitting}
          disabled={!session.complete}
          submitLabel="check answers and record practice"
          busyLabel="recording evidence…"
          hint={`Answer all ${lesson.questions.length} checks to submit. Writing and speaking can be added now or strengthened in a later pass.`}
          returnLabel="return to learning programs"
          onsubmit={submit}
          onreturn={() => app.finishLanguage()}
        />
      </aside>
    </article>
  </LessonShell>
{:else}
  <div class="screen">
    <p>No active language lesson.</p>
    <button class="ghost" type="button" onclick={() => (app.screen = 'idle')}>return</button>
  </div>
{/if}

<style>
  .evidence-status { display: flex; gap: 10px; color: var(--faint); font-size: 9px; letter-spacing: 0.5px; }
  .evidence-status span.ready { color: var(--green); }
  .reading-pane { width: min(100%, 920px); margin: 0 auto; padding: 24px clamp(24px, 5vw, 72px) 64px; }
  .meta-label { color: var(--accent); font-size: 9px; letter-spacing: 1.4px; font-family: var(--font-mono); }
  .mission { border: 1px solid var(--node-border); border-radius: var(--radius-panel); padding: 14px 16px; background: var(--surface); }
  .mission p { margin: 6px 0; color: var(--muted); font-size: 12px; line-height: 1.6; }
  .mission strong { font-size: 12px; }
  .audio-console { display: flex; align-items: center; justify-content: space-between; gap: 12px; margin: 14px 0; border: 1px solid var(--node-border); border-radius: var(--radius-control); padding: 10px 12px; }
  .audio-console > div { display: flex; align-items: center; gap: 10px; color: var(--accent); }
  .audio-console span { display: grid; }
  .audio-console strong { font-size: 12px; color: var(--text); }
  .audio-console small { color: var(--faint); font-size: 9px; }
  .audio-console button { display: inline-flex; align-items: center; gap: 6px; min-height: 32px; padding: 6px 10px; border: 1px solid var(--accent); border-radius: var(--radius-control); background: transparent; color: var(--accent); cursor: pointer; font: 10px var(--font-mono); }
  .paper { color: var(--fg); background: var(--bg); border: 1px solid var(--border); border-radius: var(--radius-panel); padding: 28px clamp(20px, 5vw, 48px); user-select: text; }
  .production-stage { margin-top: 32px; border-top: 1px solid var(--node-border); padding-top: 18px; }
  .practice-pane { margin-top: 24px; border-top: 1px solid var(--node-border); border-radius: var(--radius-panel); padding: 24px; background: var(--surface); font-size: 15px; }
  .practice-intro h2 { font-size: 16px; margin: 6px 0; }
  .practice-intro p { color: var(--muted); font-size: 11px; line-height: 1.5; margin: 0; }
  .requirement { margin-top: 8px !important; color: var(--led-warn) !important; font-size: 10px !important; }
  .production { margin-top: 16px; border: 1px solid var(--node-border); border-radius: var(--radius-control); padding: 14px; }
  .production-head { display: flex; align-items: center; gap: 8px; color: var(--accent); }
  .production h3 { margin: 0; font-size: 13px; }
  .production > p { color: var(--muted); font-size: 11px; line-height: 1.5; }
  textarea { width: 100%; box-sizing: border-box; resize: vertical; background: var(--bg); color: var(--text); border: 1px solid var(--node-border); border-radius: var(--radius-control); padding: 9px; font-size: 13px; }
  .word-count { color: var(--faint); font-size: 9px; margin-top: 4px; }
  .speaking-check { display: flex; align-items: center; gap: 8px; color: var(--muted); font-size: 11px; margin: 8px 0; }
  .speaking-check input { accent-color: var(--accent); }
  .confidence { margin-top: 8px; max-width: 320px; }
  button:focus-visible, input:focus-visible, textarea:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
  .screen { min-height: 100vh; display: grid; place-content: center; gap: 10px; }
</style>
