<script lang="ts">
  import { api, type ExerciseOwner, type ExerciseView } from '../ipc';
  import { createDraftSaver, type SaveStatus } from '../draft-save';
  import Markdown from './Markdown.svelte';
  import { parseExercisePlan } from '../features/lessons/exercise-plan';
  import { Lightbulb, Copy, Check, RotateCcw, CircleCheck, Timer } from 'lucide-svelte';

  let {
    courseId,
    classroomSessionId,
    studySessionId,
    minutes,
  }: { courseId?: number; classroomSessionId?: number; studySessionId?: string; minutes?: number } = $props();

  let exercise = $state<ExerciseView | null>(null);
  let loading = $state(true);
  let loadError = $state('');
  let draft = $state('');
  let evidence = $state('');
  let tradeoff = $state('');
  let scaleFailure = $state('');
  let completed = $state(false);
  let revealedHints = $state(0);
  let saveStatus = $state<SaveStatus>('idle');
  let completionStatus = $state<'idle' | 'saving' | 'saved' | 'error'>('idle');
  let completionError = $state('');
  let copied = $state(false);
  let saver: ReturnType<typeof createDraftSaver> | undefined;
  let loadVersion = 0;
  const owner: ExerciseOwner = $derived(
    studySessionId !== undefined
      ? { study_session_id: studySessionId }
      : classroomSessionId !== undefined
        ? { classroom_session_id: classroomSessionId }
        : { course_id: courseId },
  );
  const draftWords = $derived(draft.trim() ? draft.trim().split(/\s+/).length : 0);

  /** The steps and criteria inside the instructions, when they are shaped that way. */
  const plan = $derived(exercise ? parseExercisePlan(exercise.instructions) : null);
  /**
   * Which steps and criteria are ticked. Kept on this machine per owner, like
   * the draft's recovery copy: a tick is a bookmark, not evidence.
   */
  let ticks = $state<{ steps: boolean[]; done: boolean[] }>({ steps: [], done: [] });
  const ticksKey = $derived(`principia-exercise-ticks:${studySessionId ?? (classroomSessionId !== undefined ? `classroom:${classroomSessionId}` : `course:${courseId}`)}`);
  $effect(() => {
    const key = ticksKey;
    const current = plan;
    if (!current) return;
    let saved: { steps?: boolean[]; done?: boolean[] } = {};
    try { saved = JSON.parse(localStorage.getItem(key) ?? '{}'); } catch { saved = {}; }
    ticks = {
      steps: current.steps.map((_, index) => !!saved.steps?.[index]),
      done: current.doneWhen.map((_, index) => !!saved.done?.[index]),
    };
  });
  function tick(kind: 'steps' | 'done', index: number) {
    const next = { steps: [...ticks.steps], done: [...ticks.done] };
    next[kind][index] = !next[kind][index];
    ticks = next;
    try { localStorage.setItem(ticksKey, JSON.stringify(next)); } catch { /* a tick is a convenience */ }
  }
  const stepsDone = $derived(ticks.steps.filter(Boolean).length);
  const currentStep = $derived(ticks.steps.findIndex((done) => !done));

  /**
   * The reflection is stored as one text with three labelled lines, so the
   * record stays readable on its own and older free-text notes still load
   * (they land in the evidence field).
   */
  const LABELS = { evidence: 'Evidence', tradeoff: 'Trade-off', scale: '10× failure' } as const;
  function splitReflection(text: string) {
    const pick = (label: string) => text.match(new RegExp(`^${label.replace('×', '[×x]')}:\\s*(.*)$`, 'mi'))?.[1]?.trim() ?? '';
    const parts = { evidence: pick(LABELS.evidence), tradeoff: pick(LABELS.tradeoff), scale: pick(LABELS.scale) };
    if (!parts.evidence && !parts.tradeoff && !parts.scale) parts.evidence = text.trim();
    return parts;
  }
  function joinReflection(): string {
    return [
      `${LABELS.evidence}: ${evidence.trim()}`,
      `${LABELS.tradeoff}: ${tradeoff.trim()}`,
      `${LABELS.scale}: ${scaleFailure.trim()}`,
    ].join('\n');
  }

  $effect(() => {
    const capturedOwner = { ...owner };
    const ownerKey = studySessionId !== undefined
      ? `study:${studySessionId}`
      : classroomSessionId !== undefined
        ? `classroom:${classroomSessionId}` : `course:${courseId}`;
    const version = ++loadVersion;
    let disposed = false;
    const currentSaver = createDraftSaver(
      ownerKey,
      (text) => api.saveExerciseDraft(capturedOwner, text),
      (status) => { if (!disposed) saveStatus = status; },
    );
    saver = currentSaver;
    loading = true;
    loadError = '';
    exercise = null;
    revealedHints = 0;
    saveStatus = 'idle';
    completionStatus = 'idle';
    completionError = '';
    currentSaver.flush()
      .then(() => api.getExercise(capturedOwner))
      .then((e) => {
        if (disposed) return;
        exercise = e;
        const recovered = currentSaver.recoveredDraft();
        draft = recovered ?? e?.draft ?? e?.starter_code ?? '';
        const parts = splitReflection(e?.reflection ?? '');
        evidence = parts.evidence;
        tradeoff = parts.tradeoff;
        scaleFailure = parts.scale;
        completed = e?.completed ?? false;
        loading = false;
        if (recovered !== null && e) currentSaver.schedule(recovered);
      })
      .catch((e) => {
        if (disposed) return;
        loadError = String(e);
        loading = false;
      });
    return () => {
      disposed = true;
      void currentSaver.flush();
      if (version === loadVersion) saver = undefined;
    };
  });

  function scheduleSave() {
    if (!exercise) return;
    saver?.schedule(draft);
  }

  function onDraftInput(e: Event) {
    draft = (e.target as HTMLTextAreaElement).value;
    scheduleSave();
  }

  function resetToStarter() {
    if (!exercise?.starter_code) return;
    draft = exercise.starter_code;
    scheduleSave();
  }

  async function copyStarter() {
    if (!exercise?.starter_code) return;
    try {
      await navigator.clipboard.writeText(exercise.starter_code);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {
      // Clipboard API unavailable — nothing to recover, ignore silently.
    }
  }

  function revealNextHint() {
    if (exercise && revealedHints < exercise.hints.length) revealedHints += 1;
  }

  async function saveCompletion(nextCompleted: boolean) {
    if (!exercise) return;
    const capturedOwner = { ...owner };
    const version = loadVersion;
    completionError = '';
    if (nextCompleted && (!evidence.trim() || !tradeoff.trim() || !scaleFailure.trim())) {
      completionStatus = 'error';
      completionError = 'Fill in all three: what proves it works, the trade-off you chose, and what breaks at 10×.';
      return;
    }
    const savedReflection = joinReflection();
    completionStatus = 'saving';
    try {
      await saver?.flush();
      if (version !== loadVersion) return;
      if (saveStatus === 'error') throw new Error('Save the exercise draft before marking it complete.');
      await api.saveExerciseCompletion(capturedOwner, nextCompleted, savedReflection);
      if (version !== loadVersion) return;
      completed = nextCompleted;
      completionStatus = 'saved';
    } catch (e) {
      if (version !== loadVersion) return;
      completionStatus = 'error';
      completionError = String(e);
    }
  }
</script>

<div class="exercise-workspace">
  {#if loading}
    <p class="mono dim" role="status">loading exercise…</p>
  {:else if loadError}
    <p class="mono err" role="alert">{loadError}</p>
  {:else if !exercise}
    <p class="mono dim">No structured exercise was generated for this course.</p>
  {:else}
    <header class="ex-head">
      <span class="eyebrow mono"><span>PRACTICE</span>{#if minutes}<span class="eyebrow-sep">·</span><Timer size={10} /><span>ABOUT {minutes} MIN</span>{/if}</span>
      <h3>{exercise.title}</h3>
      {#if exercise.deliverable}
        <p class="deliverable"><span class="deliverable-label"><CircleCheck size={13} /> Done looks like</span> <Markdown markdown={exercise.deliverable} inline /></p>
      {/if}
    </header>

    {#if plan}
      {#if plan.produce}<p class="produce"><strong>You will produce</strong> <Markdown markdown={plan.produce} inline /></p>{/if}
      {#if plan.rest}<div class="ex-instructions"><Markdown markdown={plan.rest} /></div>{/if}
      <section class="stepper" aria-label="Steps">
        <div class="stepper-head">
          <span class="mono">STEPS · {stepsDone} of {plan.steps.length} done</span>
          <div class="stepper-track" role="progressbar" aria-valuemin="0" aria-valuemax={plan.steps.length} aria-valuenow={stepsDone} aria-label="Steps done"><i style:width={`${plan.steps.length ? (stepsDone / plan.steps.length) * 100 : 0}%`}></i></div>
        </div>
        <ol class="steps">
          {#each plan.steps as step, index (index)}
            <li class:done={ticks.steps[index]} class:current={index === currentStep}>
              <button type="button" class="step-tick" role="checkbox" aria-checked={!!ticks.steps[index]} aria-label={`Step ${index + 1} done`} onclick={() => tick('steps', index)}>
                {#if ticks.steps[index]}<Check size={13} />{:else}<span class="mono">{index + 1}</span>{/if}
              </button>
              <div class="step-body"><Markdown markdown={step} compact /></div>
            </li>
          {/each}
        </ol>
      </section>
      {#if plan.doneWhen.length}
        <section class="done-when" aria-label="Done when">
          <span class="mono">DONE WHEN</span>
          <ul>
            {#each plan.doneWhen as item, index (index)}
              <li class:done={ticks.done[index]}>
                <button type="button" class="done-tick" role="checkbox" aria-checked={!!ticks.done[index]} aria-label={`Criterion ${index + 1} met`} onclick={() => tick('done', index)}>{#if ticks.done[index]}<Check size={11} />{/if}</button>
                <div class="done-body"><Markdown markdown={item} compact /></div>
              </li>
            {/each}
          </ul>
        </section>
      {/if}
    {:else}
      <div class="ex-instructions">
        <Markdown markdown={exercise.instructions} />
      </div>
    {/if}

    {#if exercise.starter_code}
      <div class="starter-block">
        <div class="starter-head mono">
          <span>starter code</span>
          <div class="starter-actions">
            <button class="ghost mono-ghost small" onclick={copyStarter}>
              {#if copied}<Check size={11} /> copied{:else}<Copy size={11} /> copy{/if}
            </button>
            <button class="ghost mono-ghost small" onclick={resetToStarter}>
              <RotateCcw size={11} /> reset draft to starter
            </button>
          </div>
        </div>
        <pre class="starter-code mono">{exercise.starter_code}</pre>
      </div>
    {/if}

    <section class="draft-block" aria-labelledby="exercise-draft-title">
      <div class="block-head">
        <div>
          <h4 id="exercise-draft-title">Your work</h4>
          <p>Build the artifact here. Markdown is fine; it autosaves, and leaving never loses it.</p>
        </div>
        <span class="save-status mono" class:err={saveStatus === 'error'} aria-live="polite">
          {saveStatus === 'saving'
            ? 'saving…'
            : saveStatus === 'saved'
              ? 'saved'
              : saveStatus === 'error'
                ? 'save failed; recovery copy retained'
                : draftWords > 0 ? `${draftWords} words` : ''}
        </span>
        {#if saveStatus === 'error'}
          <button type="button" class="ghost mono-ghost small" onclick={() => saver?.flush()}>Retry save</button>
        {/if}
      </div>
      <textarea
        class="draft-input mono"
        value={draft}
        oninput={onDraftInput}
        placeholder="Write or paste your artifact here…"
        rows="14"
        aria-label="exercise draft"
      ></textarea>
    </section>

    <section class="evidence-block" class:complete={completed} aria-labelledby="exercise-evidence-title">
      <div class="block-head">
        <div>
          <h4 id="exercise-evidence-title">{completed ? 'Practice evidence recorded' : 'Close the learning loop'}</h4>
          <p>Three short answers. They are what turns a finished artifact into evidence.</p>
        </div>
        {#if completed}<Check size={18} aria-hidden="true" />{/if}
      </div>
      <div class="evidence-fields">
        <label class="evidence-field">
          <span class="field-label"><span class="field-num mono">1</span> What proves it works?</span>
          <input type="text" bind:value={evidence} disabled={completionStatus === 'saving'} placeholder="the test, trace or measurement you ran" />
        </label>
        <label class="evidence-field">
          <span class="field-label"><span class="field-num mono">2</span> Which trade-off did you choose?</span>
          <input type="text" bind:value={tradeoff} disabled={completionStatus === 'saving'} placeholder="what you gave up, and why it was worth it" />
        </label>
        <label class="evidence-field">
          <span class="field-label"><span class="field-num mono">3</span> What breaks at 10× scale or team size?</span>
          <input type="text" bind:value={scaleFailure} disabled={completionStatus === 'saving'} placeholder="the first thing to give way, and the signal" />
        </label>
      </div>
      <div class="evidence-actions">
        <button
          class="cta mono-cta complete-button"
          class:complete={completed}
          onclick={() => saveCompletion(true)}
          disabled={completionStatus === 'saving'}
        >
          <Check size={12} />
          {completionStatus === 'saving'
            ? 'saving…'
            : completed
              ? 'update evidence'
              : 'mark practice complete'}
        </button>
        {#if completed}
          <button class="ghost mono-ghost small" onclick={() => saveCompletion(false)}>
            reopen
          </button>
        {/if}
        {#if completionStatus === 'saved'}
          <span class="mono completion-status">saved to learner record</span>
        {/if}
      </div>
      {#if completionError}
        <p class="mono completion-error" role="alert">{completionError}</p>
      {/if}
    </section>

    {#if exercise.hints.length > 0}
      <div class="hints-block">
        <div class="hints-head mono"><Lightbulb size={12} /> hints · {revealedHints} of {exercise.hints.length} shown</div>
        {#each exercise.hints.slice(0, revealedHints) as hint, i}
          <p class="hint"><span class="hint-num mono">{i + 1}</span> <span><Markdown markdown={hint} inline /></span></p>
        {/each}
        {#if revealedHints < exercise.hints.length}
          <button class="ghost mono-ghost small" onclick={revealNextHint}>
            {revealedHints === 0 ? 'stuck? reveal the first hint' : `reveal hint ${revealedHints + 1}`}
          </button>
        {/if}
      </div>
    {/if}
  {/if}
</div>

<style>
  .exercise-workspace {
    /* The same width as the reading above it, so the pane does not narrow. */
    width: 100%;
    margin: 0;
    padding: 28px 0 80px;
  }
  .dim {
    color: var(--faint);
    font-size: 12px;
  }
  .err {
    color: var(--bad-fg);
    font-size: 12px;
  }
  .eyebrow {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    color: var(--accent);
    font-size: 9px;
    letter-spacing: 1.4px;
  }
  .eyebrow-sep {
    color: var(--faint);
  }
  .ex-head h3 {
    font-size: 22px;
    margin: 6px 0 10px;
    line-height: 1.25;
  }
  .deliverable {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 4px 8px;
    margin: 0;
    padding: 10px 14px;
    border: 1px solid color-mix(in srgb, var(--ok-fg) 40%, var(--border));
    border-radius: var(--radius-panel);
    background: color-mix(in srgb, var(--ok-fg) 7%, var(--surface));
    font-size: 13px;
    line-height: 1.5;
    color: var(--fg);
  }
  .deliverable-label {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    color: var(--ok-fg);
    font-family: var(--font-mono);
    font-size: 10px;
    letter-spacing: 0.8px;
    text-transform: uppercase;
    white-space: nowrap;
  }

  /* Prose instructions (older exercises, or anything beside the plan). */
  .ex-instructions {
    margin: 18px 0 6px;
    font-size: 14px;
  }
  .ex-instructions :global(.md p) {
    line-height: 1.6;
  }
  .produce {
    margin: 18px 0 14px;
    font-size: 14.5px;
    line-height: 1.6;
  }
  .produce strong {
    display: inline-block;
    margin-right: 6px;
    font-family: var(--font-mono);
    font-size: 9.5px;
    font-weight: 500;
    letter-spacing: 1px;
    text-transform: uppercase;
    color: var(--accent);
  }

  /* The stepper: numbered, tickable, the next step lit. Text stays inline
     inside each step, so code spans and sentences flow together. */
  .stepper { margin: 10px 0 18px; }
  .stepper-head { display: flex; align-items: center; gap: 14px; margin-bottom: 10px; }
  .stepper-head > span { font-size: 10px; letter-spacing: 1.2px; color: var(--muted); white-space: nowrap; }
  .stepper-track { flex: 1; height: 4px; border-radius: 2px; background: var(--surface-2); overflow: hidden; }
  .stepper-track i { display: block; height: 100%; border-radius: 2px; background: linear-gradient(90deg, var(--accent), var(--ok-fg)); transition: width 400ms cubic-bezier(0.22, 1, 0.36, 1); }
  .steps { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 6px; }
  .steps li {
    position: relative;
    display: grid;
    grid-template-columns: 26px minmax(0, 1fr);
    gap: 12px;
    align-items: start;
    padding: 11px 14px;
    border: 1px solid var(--node-border);
    border-radius: var(--radius-control);
    background: color-mix(in srgb, var(--surface) 70%, transparent);
    font-size: 14px;
    line-height: 1.55;
    transition: border-color 160ms ease, background 160ms ease, opacity 160ms ease;
  }
  .steps li.current { border-color: color-mix(in srgb, var(--accent) 55%, var(--node-border)); background: color-mix(in srgb, var(--accent) 7%, var(--surface)); }
  .steps li.done { opacity: 0.62; }
  .steps li.done .step-body { text-decoration: line-through; text-decoration-color: color-mix(in srgb, var(--muted) 60%, transparent); }
  .step-tick {
    width: 26px; height: 26px; margin-top: 1px; padding: 0;
    display: grid; place-items: center;
    border: 1.5px solid var(--node-border); border-radius: 50%;
    background: var(--surface-2); color: var(--muted); cursor: pointer;
    font-size: 11px; transition: background 160ms ease, border-color 160ms ease, transform 120ms ease;
  }
  .step-tick:hover { transform: scale(1.08); border-color: var(--accent); color: var(--accent); }
  .steps li.current .step-tick { border-color: var(--accent); color: var(--accent); background: color-mix(in srgb, var(--accent) 14%, var(--surface-2)); }
  .steps li.done .step-tick { background: var(--ok-fg); border-color: var(--ok-fg); color: var(--ok-bg); }
  .step-body { min-width: 0; }
  .done-when { margin: 0 0 6px; }
  .done-when > span { display: block; margin-bottom: 8px; font-size: 10px; letter-spacing: 1.2px; color: var(--muted); }
  .done-when ul { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 4px; }
  .done-when li { display: grid; grid-template-columns: 18px minmax(0, 1fr); gap: 10px; align-items: start; padding: 5px 0; font-size: 14px; line-height: 1.5; }
  .done-when li.done .done-body { color: var(--muted); }
  .done-tick { width: 16px; height: 16px; margin-top: 3px; padding: 0; display: grid; place-items: center; border: 1.5px solid var(--ok-fg); border-radius: 4px; background: transparent; color: var(--ok-bg); cursor: pointer; transition: background 140ms ease; }
  .done-when li.done .done-tick { background: var(--ok-fg); }
  .step-tick:focus-visible, .done-tick:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
  .starter-block {
    margin: 20px 0;
  }
  .starter-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 10.5px;
    letter-spacing: 0.5px;
    color: var(--faint);
    margin-bottom: 6px;
  }
  .starter-actions {
    display: flex;
    gap: 6px;
  }
  .ghost.small {
    padding: 3px 10px;
    font-size: 10.5px;
  }
  .starter-code {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-panel);
    padding: 14px 16px;
    font-size: 12.5px;
    line-height: 1.6;
    overflow-x: auto;
    white-space: pre;
    margin: 0;
  }
  .block-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px 16px;
    margin-bottom: 10px;
  }
  .block-head h4 {
    margin: 0 0 3px;
    color: var(--fg);
    font-size: 14px;
  }
  .block-head p {
    margin: 0;
    font-size: 12px;
    line-height: 1.5;
    color: var(--muted);
  }
  .draft-block {
    margin: 26px 0;
  }
  .save-status {
    flex: none;
    font-size: 10px;
    letter-spacing: 0.5px;
    color: var(--faint);
    padding-top: 3px;
  }
  .save-status.err {
    color: var(--bad-fg);
  }
  .draft-input {
    width: 100%;
    font-size: 13px;
    line-height: 1.65;
    resize: vertical;
    min-height: 260px;
    padding: 14px 16px;
  }
  .evidence-block {
    margin: 22px 0;
    padding: 18px;
    border: 1px solid var(--border);
    border-radius: var(--radius-panel);
    background: color-mix(in srgb, var(--surface) 82%, transparent);
  }
  .evidence-block.complete {
    border-color: color-mix(in srgb, var(--good-fg) 45%, var(--border));
  }
  .evidence-fields {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .evidence-field {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .field-label {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: 12.5px;
    color: var(--fg);
  }
  .field-num {
    display: inline-grid;
    place-items: center;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    border: 1px solid var(--node-border);
    color: var(--accent);
    font-size: 9.5px;
  }
  .evidence-field input {
    width: 100%;
    font-size: 13px;
    padding: 9px 12px;
  }
  .evidence-actions {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 14px;
  }
  .complete-button {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 9px 16px;
    font-size: 11px;
  }
  .complete-button.complete {
    background: color-mix(in srgb, var(--good-fg) 22%, var(--surface));
    color: var(--good-fg);
  }
  .completion-status {
    color: var(--muted);
    font-size: 10px;
  }
  .completion-error {
    margin: 8px 0 0;
    color: var(--bad-fg);
    font-size: 10.5px;
  }
  .hints-block {
    margin-top: 22px;
    padding-top: 16px;
    border-top: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .hints-head {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 10.5px;
    letter-spacing: 0.5px;
    color: var(--faint);
  }
  .hint {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    font-size: 13px;
    line-height: 1.5;
    color: var(--fg);
    margin: 0;
  }
  .hint-num {
    flex: none;
    display: inline-grid;
    place-items: center;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    color: var(--accent);
    font-size: 9.5px;
  }
</style>
