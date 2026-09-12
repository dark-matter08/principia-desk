<script lang="ts">
  import { onMount } from 'svelte';
  import type { ClassroomSubjectId } from '../../catalog.generated';
  import type { EnrollmentConfiguration, EntryChoice } from '../../contracts/enrollment';
  import { courseDefinition } from '../../catalog';
  import { enrollmentEditor } from './enrollment-controllers';
  import type { EnrollmentEditorState } from './enrollment-editor';
  import NodeCard from '../../components/NodeCard.svelte';
  import Dropdown from '../../components/Dropdown.svelte';
  import { api } from '../../ipc';
  import { app } from '../../stores.svelte';
  import type { PathRecommendation } from '../../contracts/placement';
  import PlacementCheck from './PlacementCheck.svelte';
  import PathPreview from './PathPreview.svelte';
  import FlowStage from '../../components/FlowStage.svelte';
  import { ArrowLeft, Check, Compass, ListStart, Layers, BookOpen, Save, Target } from 'lucide-svelte';

  let { courseId, onclose, embedded = false }: { courseId: ClassroomSubjectId; onclose: () => void; embedded?: boolean } = $props();
  const uid = $props.id();
  const course = $derived(courseDefinition(courseId)!);
  let editor: ReturnType<typeof enrollmentEditor>;
  let view = $state<EnrollmentEditorState>({ options: null, draft: null, configuration: null, status: 'loading', error: '' });
  let configuration = $state<EnrollmentConfiguration | null>(null);
  let search = $state('');
  let closing = $state(false);
  let checking = $state(false);
  let path = $state<PathRecommendation | null>(null);
  let opening = $state(false);
  let operationError = $state('');
  let accepting = $state(false);
  const routes = [
    { id: 'foundations', title: 'Start from scratch', description: 'Assume nothing. The first lessons are written for someone who has never done this before: every term explained, one small step at a time.', icon: Layers },
    { id: 'diagnostic', title: 'Help me find my level', description: 'Use a short, optional check to help identify a starting point and gaps.', icon: Compass },
    { id: 'manual', title: 'Choose my starting point', description: 'Pick a course stage and tell us which topics already feel familiar.', icon: ListStart },
  ] as const;
  const entryOptions = $derived(view.options?.entry_points.map((point) => ({ value: point.id, label: point.label })) ?? []);
  /** A class without a question bank offers the two routes that need none. */
  const availableRoutes = $derived(routes.filter((route) => route.id !== 'diagnostic' || (view.options?.diagnostic_available ?? true)));
  const familiarity = $derived(view.options?.familiarity_options.filter((option) => option.label.toLowerCase().includes(search.toLowerCase())) ?? []);

  onMount(() => {
    editor = enrollmentEditor(courseId);
    const unsubscribe = editor.subscribe((next) => {
      view = next;
      configuration = next.configuration;
    });
    void editor.load();
    return () => { unsubscribe(); void editor.flush(); };
  });
  function changed() { if (configuration) editor.edit($state.snapshot(configuration)); }
  function choose(route: EntryChoice['route']) {
    if (!configuration || !view.options) return;
    configuration.entry = route === 'manual'
      ? { route, entry_point: view.options.entry_points[0].id, familiar_competencies: [] }
      : { route };
    changed();
  }
  function toggleFamiliar(id: string) {
    if (configuration?.entry.route !== 'manual') return;
    const selected = configuration.entry.familiar_competencies;
    configuration.entry.familiar_competencies = selected.includes(id) ? selected.filter((value) => value !== id) : [...selected, id];
    changed();
  }
  async function close() {
    closing = true;
    if (view.status === 'loading' || !configuration || await editor.flush()) onclose();
    closing = false;
  }
  async function openNext(diagnostic: boolean) {
    if (!configuration || opening) return;
    opening = true; operationError = '';
    try {
      changed();
      if (!await editor.flush() || !view.draft) return;
      if (diagnostic) checking = true;
      else { checking = false; path = await api.getPathRecommendation(view.draft.id, view.draft.revision); }
    } catch (error) { operationError = String(error); }
    finally { opening = false; }
  }
  async function includeFoundations() { path = null; choose('foundations'); await openNext(false); }
  async function accept() {
    if (!path || accepting) return;
    accepting = true; operationError = '';
    try {
      await api.acceptClassPath({ draft_id: path.draft_id, expected_revision: path.draft_revision, recommendation_id: path.id });
      editor.accepted();
      await app.refresh(); onclose();
    } catch (error) { operationError = String(error); }
    finally { accepting = false; }
  }
</script>

{#if checking && view.draft}
  <PlacementCheck {embedded} draft={view.draft} onclose={() => (checking = false)} onrecommend={() => openNext(false)} />
{:else if path}
  <PathPreview {embedded} {path} tutor={configuration?.tutor ?? null} onclose={() => (path = null)} onfoundations={includeFoundations} onaccept={accept} {accepting} error={operationError} />
{:else}
<section class="enrollment" class:embedded aria-labelledby={`${uid}-title`}>
  {#if !embedded}<button class="back-link" onclick={close} disabled={closing}><ArrowLeft size={15} /> Classes</button>{/if}
  <header>
    <p class="eyebrow mono">{course.label} · Setup draft</p>
    <h1 id={`${uid}-title`}>Start from what you know</h1>
    <p class="intro">Choose your goal and starting preferences. Your setup is saved so you can return to it later.</p>
  </header>

  {#if view.status === 'loading'}
    <p role="status">Loading your course setup…</p>
  {:else if configuration && view.options}
    <p class="notice">Review and accept your path, then add a study time to activate this class. Starting preferences and diagnostic samples remain separate from completed lessons and mastery.</p>
    <FlowStage number="01"><NodeCard Icon={BookOpen} name={course.title} badge={course.version} badgeTone="violet">
    <div class="course-context">
      <p>{course.outcome}</p>
      <p><strong>Environment:</strong> {course.environment}</p>
      {#if course.prerequisite_courses.length}
        <p><strong>Recommended preparation:</strong> {course.prerequisite_courses.map((id) => courseDefinition(id)?.label ?? id).join(', ')} or equivalent experience.</p>
      {/if}
    </div>
    </NodeCard></FlowStage>

    <FlowStage number="02"><NodeCard Icon={Compass} name="entry-profile" badge={configuration.entry.route} badgeTone="amber">
    <fieldset class="entry-options">
      <legend>Your starting point</legend>
      <div class="route-grid">
        {#each availableRoutes as route}
          <label class:selected={configuration.entry.route === route.id}>
            <span class="route-top"><route.icon size={14} /><input type="radio" name={`${uid}-entry-route`} value={route.id} checked={configuration.entry.route === route.id} onchange={() => choose(route.id)} /></span>
            <strong>{route.title}</strong><span>{route.description}</span>
          </label>
        {/each}
      </div>
      {#if view.options && view.options.diagnostic_available === false}<p class="no-bank">The placement check needs a question bank, which this class does not have yet. Start from scratch or choose a stage.</p>{/if}
    </fieldset>
    {#if configuration.entry.route === 'diagnostic'}
      <p class="notice">About 3–8 minutes, with optional prerequisite follow-ups. No tutor connection or focus lock is required. Skip anything unfamiliar; answers remain saved.</p>
      <button class="cta mono-cta" onclick={() => openNext(true)} disabled={opening || view.status === 'conflict'}><Compass size={14} /> Start / resume check</button>
    {:else if configuration.entry.route === 'manual'}
      <div class="manual-entry">
        <div class="field"><Dropdown label={course.kind === 'language' ? 'Declared starting band' : 'Starting course stage'}
          bind:value={configuration.entry.entry_point} options={entryOptions} onchange={changed} /></div>
        <p class="hint">This is a starting preference. Familiarity is recorded separately from skills demonstrated through practice.</p>
        <details>
          <summary>Topics I already know <span>({configuration.entry.familiar_competencies.length} selected)</span></summary>
          <label class="field search"><span>Find a topic or skill</span><input type="search" bind:value={search} placeholder="Search familiar topics" /></label>
          <div class="familiar-list">
            {#each familiarity as option}
              <label><input type="checkbox" checked={configuration.entry.familiar_competencies.includes(option.id)} onchange={() => toggleFamiliar(option.id)} /><span>{option.label}</span></label>
            {:else}<p>No matching topics.</p>{/each}
          </div>
        </details>
      </div>
    {/if}

    </NodeCard></FlowStage>

    <FlowStage number="03" last><NodeCard Icon={Target} name="your-goal" badge={configuration.goal.note.trim() ? 'noted' : 'optional'} badgeTone={configuration.goal.note.trim() ? 'teal' : 'muted'}>
    <div class="form-grid goal-grid">
      <fieldset>
        <legend>Your goal</legend>
        {#if configuration.goal.kind === 'language_level'}
          <div class="field"><Dropdown label="Target band" bind:value={configuration.goal.target_level} options={entryOptions} onchange={changed} /></div>
        {/if}
        <label class="field"><span>What would you like to do with this knowledge? <small>Optional</small></span><textarea rows="4" maxlength="4000" bind:value={configuration.goal.note} oninput={changed} placeholder="For example, automate a repeatable task at work."></textarea></label>
      </fieldset>
      <aside class="goal-aside">
        <span class="goal-aside-label">WHY IT MATTERS</span>
        <p>The tutor reads this before writing every lesson, and leans the examples, the practice and the checks towards it. A sentence is enough; a paragraph is better.</p>
        <p class="goal-aside-pace">Pace is not asked for here. How long each session runs, and how much of the week you study, follow the study times you set on the class's Schedule tab.</p>
      </aside>
    </div>
    </NodeCard></FlowStage>
  {/if}

  {#if operationError}<p class="save-error" role="alert">{operationError}</p>{/if}

  {#if view.error}
    <div class="save-error" role="alert">
      <p>{view.error}</p>
      {#if view.options}
        <div class="error-actions">
          {#if view.status !== 'conflict'}<button class="ghost mono-ghost" onclick={() => editor.flush()}>Retry save</button>{/if}
          <button class="ghost mono-ghost" onclick={() => editor.resolve(false)}>Use saved choices</button>
          <button class="ghost mono-ghost" onclick={() => editor.resolve(true)}>Keep my choices</button>
        </div>
      {:else}<button class="ghost mono-ghost" onclick={() => editor.load()}>Retry loading</button>{/if}
    </div>
  {/if}
  <footer>
    <p class="save-status" role="status" aria-live="polite">
      {#if view.status === 'saved'}<Check size={15} /> {view.draft ? 'Draft saved' : 'Choose a starting point to save your draft'}
      {:else if view.status === 'dirty' || view.status === 'saving'}Saving your draft…
      {:else if view.status === 'error' || view.status === 'conflict'}Your changes need attention before saving.{/if}
    </p>
    <div class="error-actions">
      <button class="ghost mono-ghost" onclick={close} disabled={closing || view.status === 'loading'}><Save size={14} /> {closing ? 'Saving…' : 'Save & return'}</button>
      <button class="cta mono-cta" onclick={() => openNext(false)} disabled={opening || view.status === 'loading' || view.status === 'conflict'}>Review path</button>
    </div>
  </footer>
</section>
{/if}

<style>
  .enrollment { width: min(820px, 100%); margin: 0 auto; padding: 26px 28px 48px; }
  header { margin: 22px 0; }
  h1 { font-size: 28px; }
  .eyebrow { color: var(--faint); font-size: 10px; letter-spacing: 1px; text-transform: uppercase; margin: 0 0 8px; }
  .intro, .hint, .course-context p { color: var(--muted); font-size: 13px; }
  .intro { max-width: 690px; }
  .back-link { font: 11px var(--font-mono); display: flex; align-items: center; gap: 8px; border: 0; background: none; color: var(--muted); cursor: pointer; padding: 0; }
  .course-context { padding: 0; }
  .course-context p:first-child { margin-top: 0; }
  .course-context p:last-child { margin-bottom: 0; }
  fieldset { border: 0; padding: 0; margin: 0; min-width: 0; }
  legend { font: 10px var(--font-mono); letter-spacing: 1px; text-transform: uppercase; color: var(--muted); margin-bottom: 14px; }
  .route-grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 12px; }
  .route-grid label { display: flex; flex-direction: column; gap: 9px; padding: 12px; border: 1px solid var(--border); border-radius: var(--radius-control); cursor: pointer; background: var(--bg); }
  .route-grid label.selected { border-color: var(--violet); background: var(--surface-2); }
  .route-top { display: flex; justify-content: space-between; color: var(--accent); }
  .route-grid strong { font: 11px/1.55 var(--font-mono); letter-spacing: 0.5px; text-transform: uppercase; }
  .route-grid label > span:last-child { font-size: 12px; color: var(--muted); line-height: 1.55; }
  input[type='radio'], input[type='checkbox'] { accent-color: var(--accent); flex: 0 0 auto; }
  .manual-entry { margin-top: 18px; padding-top: 16px; border-top: 1px dashed var(--node-divider); }
  .notice { border-left: 2px solid var(--violet); padding: 8px 12px; margin: 0 0 20px 38px; color: var(--muted); font: 11px/1.6 var(--font-mono); }
  .entry-options + .notice { margin: 14px 0 0; }
  .no-bank { margin: 10px 0 0; font-size: 11px; color: var(--muted); }
  .field { display: flex; flex-direction: column; gap: 7px; margin-bottom: 15px; font-size: 13px; min-width: 0; }
  .field small { color: var(--muted); font-weight: 400; margin-left: 5px; }
  .field input, textarea { border: 1px solid var(--border); background: var(--bg); color: var(--fg); padding: 10px 12px; border-radius: var(--radius-control); font: inherit; width: 100%; }
  textarea { resize: vertical; }
  .manual-entry > .field { max-width: 430px; }
  .form-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 22px; }
  .goal-grid { grid-template-columns: minmax(0, 3fr) minmax(220px, 2fr); align-items: start; }
  .goal-aside { display: flex; flex-direction: column; gap: 10px; padding: 14px 16px; border: 1px dashed var(--node-border); border-radius: var(--radius-control); }
  .goal-aside-label { font: 9px var(--font-mono); letter-spacing: 1.2px; color: var(--accent); }
  .goal-aside p { margin: 0; font-size: 12px; line-height: 1.55; color: var(--muted); }
  .goal-aside-pace { padding-top: 10px; border-top: 1px dashed var(--node-border); }
  summary { cursor: pointer; font-size: 13px; font-weight: 500; }
  summary span { color: var(--muted); font-weight: 400; margin-left: 8px; }
  .search { margin-top: 15px; }
  .familiar-list { max-height: 270px; overflow-y: auto; display: grid; grid-template-columns: 1fr 1fr; gap: 5px 16px; }
  .familiar-list label { display: flex; align-items: flex-start; gap: 8px; font-size: 12px; padding: 5px; }
  footer { display: flex; flex-wrap: wrap; gap: 14px; align-items: center; justify-content: space-between; margin-top: 28px; }
  .save-status { font: 10px var(--font-mono); color: var(--muted); display: flex; gap: 6px; align-items: center; }
  .save-error { margin-top: 18px; padding: 14px; border-radius: var(--radius-control); border: 1px dashed var(--led-err); color: var(--bad-fg); background: var(--bad-bg); font-size: 13px; }
  .error-actions { display: flex; flex-wrap: wrap; gap: 10px; }
  :is(button, input, textarea, summary):focus-visible { outline: 2px solid var(--accent); outline-offset: 3px; }
  @media (max-width: 620px) { .route-grid { grid-template-columns: 1fr; } .route-grid label { gap: 5px; } }
  @media (max-width: 620px) { .enrollment { padding: 22px 18px; } .form-grid, .goal-grid, .familiar-list { grid-template-columns: 1fr; } footer button { width: 100%; } }
  .enrollment.embedded { width: 100%; max-width: 1000px; padding: 0; } .embedded header { margin-top: 0; }
</style>
