<script lang="ts">
  /**
   * The class builder: a learner's own class from a brief to a published
   * course, on a five-step rail. Brief (what you want to be able to do),
   * Draft (the tutor writes it, you write it, or a file), Review (edit
   * until it reads right), Verify (the tutor reads it back; the sources
   * are fetched), Publish (make it a class; the starting point and study
   * times are set from the class itself, like any other).
   */
  import { onMount, untrack } from 'svelte';
  import { api, type CourseBrief, type CourseDraft, type CustomCourseView } from '../../../ipc';
  import type { RunnerInfo } from '../../../contracts/agents';
  import { app } from '../../../stores.svelte';
  import { validateDraft } from '../../../custom-preview';
  import { autosize } from '../../../actions/autosize';
  import Dropdown from '../../../components/Dropdown.svelte';
  import ModelPicker from '../../../components/ModelPicker.svelte';
  import { confirmDialog, promptDialog } from '../../../components/dialog.svelte';
  import CurriculumEditor from './CurriculumEditor.svelte';
  import { ArrowLeft, ArrowRight, Bot, Check, FileJson, FileUp, PenLine, Rocket, ShieldCheck, Sparkles, Trash2, Globe, CircleAlert, CircleCheck, CircleDashed, X, Save, ListChecks, Wand2, BookOpenCheck, Lock, RotateCcw, ChevronDown } from 'lucide-svelte';

  let { id = null, onclose, onpublished }: { id?: string | null; onclose: () => void; onpublished: (id: string) => void } = $props();

  const STEPS = [
    { key: 'brief', title: 'Brief', Icon: Sparkles, ahead: 'what you want' },
    { key: 'draft', title: 'Draft', Icon: Bot, ahead: 'tutor, hand or file' },
    { key: 'review', title: 'Review', Icon: PenLine, ahead: 'edit the topics' },
    { key: 'verify', title: 'Verify', Icon: ShieldCheck, ahead: 'read back, fetch, confirm' },
    { key: 'enroll', title: 'Publish', Icon: Rocket, ahead: 'make it a class' },
  ] as const;
  type StepKey = (typeof STEPS)[number]['key'];
  let step = $state<StepKey>('brief');
  const stepIndex = $derived(STEPS.findIndex((s) => s.key === step));

  let view = $state<CustomCourseView | null>(null);
  let brief = $state<CourseBrief>({ title: '', outcome: '', background: '', trusted_hosts: [], agent: app.state?.agent ?? 'claude', model: app.state?.model ?? 'opus', custom_agent_bin: app.state?.custom_agent_bin ?? '' });
  let draft = $state<CourseDraft | null>(null);
  let hostInput = $state('');
  let runners = $state<RunnerInfo[]>([]);
  let busy = $state<'' | 'create' | 'draft' | 'review' | 'sources' | 'bank' | 'fix' | 'publish' | 'save' | 'import' | 'delete' | 'export' | 'settle'>('');
  let error = $state('');
  let saved = $state<'idle' | 'saving' | 'saved'>('idle');
  let fileInput = $state<HTMLInputElement | undefined>(undefined);
  /** The topic the editor is asked to open and light up after a jump. */
  let focus = $state<{ slug: string; at: number } | null>(null);
  /** The finding the tutor is working on, so its row says so. */
  let fixing = $state<number | null>(null);
  let logTail = $derived(app.genLog.slice(-6));
  /** When the tutor's current call began, for the clock beside the feed; ticks while it runs. */
  let busySince = $state<number | null>(null);
  let now = $state(Date.now());
  $effect(() => {
    if (busySince === null) return;
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(timer);
  });
  const DOING: Record<string, string> = { draft: 'drafting the curriculum', review: 'reading the draft back', fix: 'changing the draft', sources: 'fetching the sources', bank: 'writing the question bank' };
  const elapsed = $derived.by(() => {
    if (busySince === null) return '';
    const total = Math.max(0, Math.floor((now - busySince) / 1000));
    const m = Math.floor(total / 60), sec = total % 60;
    return m ? `${m}m ${String(sec).padStart(2, '0')}s` : `${sec}s`;
  });
  /** Gates fold to their head once done, and can be folded or unfolded by hand. */
  let foldedGate = $state<Record<string, boolean>>({});
  const isFolded = (key: string, done: boolean) => foldedGate[key] ?? done;
  const toggleGate = (key: string, done: boolean) => (foldedGate = { ...foldedGate, [key]: !isFolded(key, done) });
  /** The scrolling pane; a new step starts at its top. */
  let pane = $state<HTMLDivElement | undefined>(undefined);
  $effect(() => { void step; pane?.scrollTo({ top: 0 }); });

  const runnerOptions = $derived([...runners].sort((a, b) => Number(b.available) - Number(a.available)).map((r) => ({ value: r.provider, label: r.label, description: `${r.kind === 'local' ? 'Local' : r.kind.toUpperCase()} · ${r.available ? 'configured' : 'setup needed'}` })));
  const briefReady = $derived(brief.title.trim().length > 0 && brief.outcome.trim().split(/\s+/).filter(Boolean).length >= 5);
  const issues = $derived(draft ? validateDraft(draft) : []);
  const drafted = $derived(!!draft && (draft.topics.length > 1 || draft.topics[0]?.slug !== 'first-topic' || !!draft.summary));
  /** Sources that did not answer and are still in the draft, one row per address. */
  const unreachable = $derived.by(() => {
    const urls = new Set((draft?.topics ?? []).flatMap((t) => t.curriculum.primary_sources));
    const seen = new Set<string>();
    return (view?.sources ?? []).filter((s) => s.state !== 'reachable' && urls.has(s.url) && !seen.has(s.url) && seen.add(s.url));
  });
  const checks = $derived(view?.checks);
  const reviewDone = $derived(!!checks && checks.reviewed && checks.open_findings === 0);
  const sourcesDone = $derived(!!checks && checks.fetched && checks.unchecked_sources === 0 && checks.pending_sources === 0);
  const readDone = $derived(!!checks?.read);
  const canEnroll = $derived(!!checks && checks.blockers.length === 0);
  const settled = $derived((view?.review ?? []).filter((f) => f.status !== 'open').length);
  const bankQuestions = $derived(view?.bank?.questions ?? []);
  const voided = $derived(bankQuestions.filter((q) => q.voided));
  const bankStale = $derived.by(() => { const d = draft; return !!d && bankQuestions.some((q) => !d.topics.some((t) => `${d.id}-${t.slug}` === q.competency)); });

  onMount(() => {
    api.listAgentRunners().then((list) => (runners = list)).catch(() => {});
    if (id) void load(id);
    return () => { if (pollTimer) clearInterval(pollTimer); };
  });

  async function load(courseId: string) {
    try {
      const loaded = await api.getCustomCourse(courseId);
      adopt(loaded);
      step = untrack(() => activeStep(loaded));
    } catch (cause) { error = String(cause); }
  }
  /**
   * The step a class opens on: the first one that still has work in it,
   * not the last one finished. A draft with editor issues opens on Review;
   * a clean draft with checks to pass opens on Verify; a class that passes
   * every check opens on Enroll.
   */
  function activeStep(loaded: CustomCourseView): StepKey {
    if (loaded.working === 'draft') return 'draft';
    const hasDraft = loaded.draft.topics.length > 1 || loaded.draft.topics[0]?.slug !== 'first-topic' || !!loaded.draft.summary;
    if (!hasDraft) return 'draft';
    if (loaded.issues.length) return 'review';
    if (loaded.checks.blockers.length) return 'verify';
    return 'enroll';
  }
  function adopt(next: CustomCourseView) {
    view = next;
    brief = { ...next.brief };
    draft = structuredClone(next.draft);
    // A call the desk is still running (started here or before the page was
    // left) shows as such, and the view is fetched again until it ends.
    if (next.working) { busy = next.working; busySince ??= Date.now(); fixing = next.working_at ?? null; watch(next.id); }
    else if (busy === 'draft' || busy === 'review' || busy === 'sources' || busy === 'bank' || busy === 'fix') { busy = ''; busySince = null; fixing = null; }
  }
  /**
   * A view fetched while the tutor is still working: the findings it has
   * settled so far, the draft as it stands, which finding it is on. Taken
   * unless a save of the learner's own edits is pending, so a keystroke is
   * never overwritten by the tutor's copy.
   */
  function adoptInFlight(fresh: CustomCourseView) {
    if (saveTimer || saved === 'saving') { fixing = fresh.working_at ?? null; return; }
    adopt(fresh);
  }

  /** While the desk works, ask again every few seconds; the desk also says when it is done. */
  let pollTimer: ReturnType<typeof setInterval> | null = null;
  function watch(courseId: string) {
    if (pollTimer) return;
    pollTimer = setInterval(async () => {
      try {
        const fresh = await api.getCustomCourse(courseId);
        if (!fresh.working) {
          if (pollTimer) { clearInterval(pollTimer); pollTimer = null; }
          adopt(fresh);
          if (drafted && step === 'draft') step = 'review';
        } else {
          adoptInFlight(fresh);
        }
      } catch { /* the next tick asks again */ }
    }, 4000);
  }
  // The desk announces each step of a call with a state refresh; read the
  // class again then. Only the refresh is a dependency: the view and the
  // busy flag change with every adoption, and tracking them would fetch
  // again on each fetch without end.
  $effect(() => {
    void app.state;
    untrack(() => {
      const current = view?.id;
      if (!current || !busy || !pollTimer) return;
      api.getCustomCourse(current).then((fresh) => {
        if (!fresh.working) {
          if (pollTimer) { clearInterval(pollTimer); pollTimer = null; }
          adopt(fresh);
          if (drafted && step === 'draft') step = 'review';
        } else {
          adoptInFlight(fresh);
        }
      }).catch(() => {});
    });
  });

  function addHost() {
    const host = hostInput.trim().replace(/^https?:\/\//, '').replace(/\/.*$/, '').toLowerCase();
    if (host && !brief.trusted_hosts.includes(host)) brief.trusted_hosts = [...brief.trusted_hosts, host];
    hostInput = '';
  }

  /** Leaving the brief creates the class, or saves the brief of an existing one. */
  async function leaveBrief() {
    if (!briefReady || busy) return;
    busy = 'create'; error = '';
    try {
      addHost();
      if (!view) adopt(await api.createCustomCourse($state.snapshot(brief), 'manual'));
      else adopt(await api.saveCustomCourseBrief(view.id, $state.snapshot(brief)));
      step = 'draft';
    } catch (cause) { error = String(cause); } finally { busy = ''; }
  }

  async function askTutor() {
    if (!view || busy) return;
    busy = 'draft'; busySince = Date.now(); error = '';
    watch(view.id);
    try { adopt(await api.draftCustomCourse(view.id)); step = 'review'; }
    catch (cause) { error = String(cause); busy = ''; busySince = null; }
    finally { if (pollTimer) { clearInterval(pollTimer); pollTimer = null; } if (!view?.working) { busy = ''; busySince = null; } }
  }
  function writeByHand() { step = 'review'; }
  async function importFile(event: Event) {
    const file = (event.currentTarget as HTMLInputElement).files?.[0];
    if (!file || busy) return;
    busy = 'import'; error = '';
    try {
      const text = await file.text();
      const imported = await api.importCustomCourse(text);
      // The import is its own class; a blank one made for it is dropped.
      if (view && !drafted && view.status === 'draft') { try { await api.deleteCustomCourseDraft(view.id); } catch { /* keep going */ } }
      adopt(imported);
      step = 'review';
      await app.refresh();
    } catch (cause) { error = String(cause); } finally { busy = ''; if (fileInput) fileInput.value = ''; }
  }

  /** The draft is saved a moment after it stops changing. */
  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  function changed() {
    saved = 'saving';
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(() => void save(), 700);
  }
  async function save() {
    if (!view || !draft) return;
    try {
      const next = await api.saveCustomCourseDraft(view.id, $state.snapshot(draft));
      view = next;
      saved = 'saved';
    } catch (cause) { error = String(cause); saved = 'idle'; }
  }
  async function flush() { if (saveTimer) { clearTimeout(saveTimer); saveTimer = null; } await save(); }

  async function tutorCall(kind: 'review' | 'sources' | 'bank' | 'fix', call: (id: string) => Promise<CustomCourseView>) {
    if (!view || busy) return;
    await flush();
    busy = kind; busySince = Date.now(); error = '';
    watch(view.id);
    try { adopt(await call(view.id)); } catch (cause) { error = String(cause); busy = ''; busySince = null; }
    finally { if (pollTimer) { clearInterval(pollTimer); pollTimer = null; } if (!view?.working) { busy = ''; busySince = null; fixing = null; } }
  }
  const review = () => tutorCall('review', (id) => api.reviewCustomCourse(id));
  /** What the sources job started here is doing: a fetch of them all, or a hunt for replacements. */
  let sourcesJob = $state<'fetch' | 'replace' | null>(null);
  const verifySources = () => { sourcesJob = 'fetch'; return tutorCall('sources', (id) => api.verifyCustomCourseSources(id)).finally(() => (sourcesJob = null)); };
  const writeBank = () => tutorCall('bank', (id) => api.writeCustomCourseBank(id));
  /** The tutor makes the change a finding asks for; the draft in the editor follows. */
  function fixWithTutor(index: number) { fixing = index; return tutorCall('fix', (id) => api.fixCustomCourseFinding(id, index)); }
  /** Every open finding in turn, each change saved before the next. */
  const fixAll = () => tutorCall('fix', (id) => api.fixAllCustomCourseFindings(id));
  /** A quick change of the class's record, without the tutor. */
  async function quick(call: (id: string) => Promise<CustomCourseView>) {
    if (!view || busy) return;
    busy = 'settle'; error = '';
    try { adopt(await call(view.id)); } catch (cause) { error = String(cause); } finally { busy = ''; }
  }
  async function settle(index: number, status: 'fixed' | 'dismissed' | 'open') {
    let note = status === 'fixed' ? 'by hand' : '';
    if (status === 'dismissed') {
      const reason = await promptDialog('Dismiss this finding?', { label: 'Why it does not apply', placeholder: 'kept with the finding' }, { message: 'A dismissed finding counts as settled. The reason stays with the class so you can see later why it was left.', confirm: 'Dismiss' });
      if (reason === null) return;
      note = reason;
    }
    await quick((id) => api.resolveCustomCourseFinding(id, index, status, note));
  }
  const acceptSource = (url: string, accepted: boolean) => quick((id) => api.acceptCustomCourseSource(id, url, accepted));
  /** The source being replaced, so its row says so. */
  let replacing = $state<string | null>(null);
  function replaceSource(url: string) { replacing = url; sourcesJob = 'replace'; return tutorCall('sources', (id) => api.replaceCustomCourseSource(id, url)).finally(() => { replacing = null; sourcesJob = null; }); }
  const replaceAllSources = () => { sourcesJob = 'replace'; return tutorCall('sources', (id) => api.replaceAllCustomCourseSources(id)).finally(() => (sourcesJob = null)); };
  const markRead = (read: boolean) => quick((id) => api.markCustomCourseRead(id, read));
  async function publish() {
    if (!view || busy) return;
    await flush();
    busy = 'publish'; error = '';
    try {
      const published = await api.publishCustomCourse(view.id);
      adopt(published);
      await app.refresh();
      app.notify(`${published.draft.label} is published as version ${published.version}.`);
      onpublished(published.id);
    } catch (cause) { error = String(cause); } finally { busy = ''; }
  }
  async function exportFile() {
    if (!view || busy) return;
    await flush();
    busy = 'export'; error = '';
    try {
      const result = await api.exportCustomCourse(view.id);
      app.notify(`Class file written: ${result.file_name}`, { label: 'Show file', run: () => void api.revealExport(result.path) });
    } catch (cause) { error = String(cause); } finally { busy = ''; }
  }
  async function discard() {
    if (!view || busy) return;
    if (!(await confirmDialog(`Delete the draft "${view.draft.label || view.brief.title}"?`, 'The brief, the draft and every check go with it. This cannot be undone.', { confirm: 'Delete draft', danger: true }))) return;
    busy = 'delete'; error = '';
    try { await api.deleteCustomCourseDraft(view.id); await app.refresh(); onclose(); }
    catch (cause) { error = String(cause); } finally { busy = ''; }
  }
  /** Open the editor on a topic: the card opens, scrolls into view and blinks. */
  function jumpTo(topic: string) {
    step = 'review';
    focus = { slug: topic, at: Date.now() };
  }
  function go(next: StepKey) {
    // Coming back to the editor by the rail starts at the top, not on the last jump.
    focus = null;
    if (next === 'brief') { step = 'brief'; return; }
    if (!view) return;
    if ((next === 'verify' || next === 'enroll') && !drafted) return;
    if (next === 'enroll' && !canEnroll) { step = 'verify'; return; }
    step = next;
  }
</script>

{#snippet liveFeed(what: string)}
  <!-- The runner answers only when it finishes, so the clock under its last line is what moves. -->
  <div class="feed-wrap">
    {#if logTail.length}<pre class="feed mono" aria-live="polite">{logTail.join('\n')}</pre>{/if}
    <div class="feed-live mono"><span class="beacon" aria-hidden="true"></span><span class="feed-what">{what}</span><b>{elapsed}</b><span class="dots" aria-hidden="true"></span><span class="feed-hint">the Logs page shows every line</span></div>
  </div>
{/snippet}

<div class="builder" bind:this={pane}>
  <header class="top">
    <div>
      <span class="meta-label">{view?.status === 'published' ? `YOUR CLASS · VERSION ${view.version}` : 'NEW CLASS'}</span>
      <h2>{view?.draft.label || brief.title || 'A class of your own'}</h2>
    </div>
    <div class="top-actions">
      {#if view && draft}<span class="save-state mono" role="status">{saved === 'saving' ? 'saving…' : saved === 'saved' ? 'draft saved' : ''}</span>{/if}
      {#if view && view.status === 'draft'}<button type="button" class="ghost mono-ghost small danger" onclick={discard} disabled={!!busy}><Trash2 size={12} />Delete draft</button>{/if}
      <button type="button" class="ghost mono-ghost small" onclick={onclose} disabled={busy === 'publish'}><X size={12} />Close</button>
    </div>
  </header>

  <ol class="steps" aria-label="Builder steps">
    {#each STEPS as item, index (item.key)}
      {@const Icon = item.Icon}
      <li class:done={index < stepIndex} class:current={index === stepIndex} class:ahead={index > stepIndex}>
        <button type="button" disabled={index > stepIndex && !(view && drafted)} aria-current={index === stepIndex ? 'step' : undefined} onclick={() => go(item.key)}>
          <span class="step-tile">{#if index < stepIndex}<Check size={14} strokeWidth={2.4} />{:else}<Icon size={15} />{/if}</span>
          <span class="step-text"><span class="step-no mono">{index < stepIndex ? 'done' : index === stepIndex ? 'now' : `0${index + 1}`}</span><span class="step-name">{item.title}</span><span class="step-sub">{item.ahead}</span></span>
        </button>
      </li>
    {/each}
  </ol>

  {#if error}<p class="error" role="alert"><CircleAlert size={12} /> {error}</p>{/if}

  {#if step === 'brief'}
    <section class="panel" aria-label="Brief">
      <div class="panel-head"><h3>What do you want to be able to do?</h3><p>The tutor designs the whole course from this, so say it the way you would to a friend who teaches. Everything here can be changed later.</p></div>
      <div class="brief-grid">
        <label class="field span-2"><span>Title</span><input bind:value={brief.title} placeholder="Rust for command-line tools" /></label>
        <label class="field span-2"><span>The outcome <small>what you will have made or be able to do at the end</small></span><textarea use:autosize={{ min: 3, max: 8, value: brief.outcome }} bind:value={brief.outcome} placeholder="Build and ship a small CLI with clean error handling, tests and a release binary."></textarea></label>
        <label class="field span-2"><span>What you already know <small>optional; it decides where foundations start</small></span><textarea use:autosize={{ min: 2, max: 6, value: brief.background }} bind:value={brief.background} placeholder="Comfortable in Python; never touched a systems language."></textarea></label>
        <div class="field span-2">
          <span>Documentation you trust <small>optional hostnames; the tutor adds the obvious ones</small></span>
          <div class="hosts">
            {#each brief.trusted_hosts as host (host)}<span class="host mono">{host}<button type="button" aria-label={`Remove ${host}`} onclick={() => (brief.trusted_hosts = brief.trusted_hosts.filter((h) => h !== host))}><X size={11} /></button></span>{/each}
            <input class="mono host-input" placeholder="doc.rust-lang.org" bind:value={hostInput} onkeydown={(e) => { if (e.key === 'Enter' || e.key === ',') { e.preventDefault(); addHost(); } }} onblur={addHost} />
          </div>
        </div>
        <div class="field"><span class="mono pick-label">TUTOR <small>drafts the course and teaches it</small></span><Dropdown label="Tutor" hideLabel value={brief.agent} options={runnerOptions} onchange={(value) => { brief.agent = value; brief.model = runners.find((r) => r.provider === value)?.saved_model ?? runners.find((r) => r.provider === value)?.default_model ?? brief.model; }} /></div>
        <div class="field"><span class="mono pick-label">MODEL</span>{#key brief.agent}<ModelPicker agent={brief.agent} bind:value={brief.model} hideLabel />{/key}</div>
      </div>
      <footer class="nav"><span class="hint mono">{briefReady ? 'ready' : 'a title and a sentence of outcome are needed'}</span><button type="button" class="cta mono-cta" disabled={!briefReady || !!busy} onclick={leaveBrief}>{busy === 'create' ? 'Saving…' : 'Continue'} <ArrowRight size={13} /></button></footer>
    </section>

  {:else if step === 'draft'}
    <section class="panel" aria-label="Draft">
      <div class="panel-head"><h3>Who writes the first draft?</h3><p>All three lead to the same editor. Nothing is published until the last step.</p></div>
      <div class="ways">
        <button type="button" class="way" class:busy={busy === 'draft'} disabled={!!busy} onclick={askTutor}>
          <span class="way-tile"><Bot size={20} /></span>
          <strong>Ask the tutor</strong>
          <small>{brief.agent} · {brief.model || 'runner default'} designs the stages and 12 to 36 topics with sources, held to the desk's checks.</small>
          {#if busy === 'draft'}<span class="working mono">drafting… a whole course takes three to six minutes; you can leave this page and come back</span>{/if}
        </button>
        <button type="button" class="way" disabled={!!busy} onclick={writeByHand}>
          <span class="way-tile"><PenLine size={20} /></span>
          <strong>Write it yourself</strong>
          <small>Start from an empty course with one topic. The editor says what each topic still needs.</small>
        </button>
        <button type="button" class="way" disabled={!!busy} onclick={() => fileInput?.click()}>
          <span class="way-tile"><FileUp size={20} /></span>
          <strong>Import a file</strong>
          <small>A <span class="mono">.principia-class.json</span> exported from this or another desk. It opens in the editor as a new class.</small>
          {#if busy === 'import'}<span class="working mono">reading…</span>{/if}
        </button>
        <input bind:this={fileInput} type="file" accept=".json,application/json" hidden onchange={importFile} />
      </div>
      {#if busy === 'draft'}{@render liveFeed(DOING.draft)}{/if}
      {#if drafted}<footer class="nav"><span class="hint mono">a draft exists · {draft?.topics.length} topics</span><button type="button" class="ghost mono-ghost" onclick={() => go('review')}>Open the draft <ArrowRight size={12} /></button></footer>{/if}
      <footer class="nav back"><button type="button" class="ghost mono-ghost" onclick={() => (step = 'brief')} disabled={!!busy}><ArrowLeft size={12} />Back to the brief</button></footer>
    </section>

  {:else if step === 'review' && draft}
    <section class="panel bare" aria-label="Review">
      <div class="panel-head"><h3>Edit until it reads right</h3><p>The rail lists what the desk would refuse to publish. Cards open to every field the tutor teaches from.</p></div>
      <CurriculumEditor bind:draft nativeIssues={view?.issues ?? []} {focus} onchange={changed} />
      <footer class="nav">
        <button type="button" class="ghost mono-ghost" onclick={() => (step = 'draft')}><ArrowLeft size={12} />Back</button>
        <span class="hint mono">{issues.length ? `${issues.length} to fix before publishing` : 'ready'}</span>
        <button type="button" class="ghost mono-ghost" onclick={exportFile} disabled={!!busy}><FileJson size={12} />Export class file</button>
        <button type="button" class="cta mono-cta" onclick={async () => { await flush(); step = 'verify'; }}>Continue <ArrowRight size={13} /></button>
      </footer>
    </section>

  {:else if step === 'verify' && draft && view && checks}
    <section class="panel" aria-label="Verify">
      <div class="panel-head"><h3>Three checks before you publish</h3><p>In order, each unlocking the next: the tutor reads the draft back and every finding is settled; every source is fetched and the unreachable ones replaced or kept knowingly; you confirm your own read-through. None of them changes the draft on its own.</p></div>
      <ol class="gates">
        <li class="gate" class:done={reviewDone} class:current={!reviewDone} class:folded={isFolded('review', reviewDone)}>
          <div class="gate-head">
            <button type="button" class="gate-fold" aria-expanded={!isFolded('review', reviewDone)} aria-label="Fold the review" onclick={() => toggleGate('review', reviewDone)}><ChevronDown size={14} /></button>
            <span class="gate-no mono" aria-hidden="true">{#if reviewDone}<Check size={13} strokeWidth={2.6} />{:else}01{/if}</span>
            <span class="check-tile"><Bot size={16} /></span>
            <div><strong>The tutor reads it back</strong><small>One full read against a fixed bar: outcomes that cannot be observed, missing or wrong prerequisites, topics that are one, stages that jump, sources that do not support their topic. Every finding is then fixed by the tutor, fixed by you, or dismissed with a reason. A later read only confirms the changes; it does not bring a new batch.</small></div>
            <span class="gate-keys">
              {#if checks.open_findings > 0}<button type="button" class="ghost mono-ghost" onclick={fixAll} disabled={!!busy}><Wand2 size={12} />{busy === 'fix' ? `Fixing… ${checks.open_findings} left` : `Fix all ${checks.open_findings}`}</button>{/if}
              <button type="button" class="ghost mono-ghost" onclick={review} disabled={!!busy}>{busy === 'review' ? 'Reading…' : checks.reviewed ? 'Read it again' : 'Read it back'}</button>
            </span>
          </div>
          <p class="gate-state mono">
            {#if !checks.reviewed}not read yet · required{:else if !view.review.length}read back with nothing to raise{:else}{view.review.length} finding{view.review.length === 1 ? '' : 's'} · {checks.open_findings ? `${checks.open_findings} open` : 'all settled'}{/if}
            {#if checks.reviewed && !checks.review_current} · read before your last edits; another read is optional and only confirms them{/if}
          </p>
          {#if busy === 'review' || busy === 'fix'}{@render liveFeed(DOING[busy])}{/if}
          {#if view.review.length && !isFolded('review', reviewDone)}
            <ol class="findings">
              {#each view.review as finding, i (i)}
                <li class={finding.severity} class:settled={finding.status !== 'open'}>
                  <span class="sev-col"><span class="sev mono">{finding.severity}</span><span class="pill mono {finding.status}">{finding.status}</span>{#if finding.carried}<span class="pill mono earlier" title="Settled in an earlier read; the tutor did not raise it again">earlier read</span>{/if}</span>
                  <div class="finding-body">
                    <p>{finding.message}</p>
                    {#if finding.fix}<p class="fix"><b>Proposed:</b> {finding.fix}</p>{/if}
                    {#if finding.status !== 'open' && finding.note}<p class="note mono">{finding.status === 'dismissed' ? 'dismissed: ' : 'fixed '}{finding.note}</p>{/if}
                    <div class="finding-actions">
                      {#if finding.status === 'open'}
                        <button type="button" class="ghost mono-ghost small" onclick={() => fixWithTutor(i)} disabled={!!busy}><Wand2 size={11} />{busy === 'fix' && fixing === i ? 'the tutor is changing it…' : busy === 'fix' ? 'waiting its turn' : 'Fix with the tutor'}</button>
                        {#if finding.topic}<button type="button" class="text" onclick={() => jumpTo(finding.topic)}>Open {finding.topic} <ArrowRight size={11} /></button>{/if}
                        <button type="button" class="text" onclick={() => settle(i, 'fixed')} disabled={!!busy}><Check size={11} /> Fixed by hand</button>
                        <button type="button" class="text quiet" onclick={() => settle(i, 'dismissed')} disabled={!!busy}>Dismiss</button>
                      {:else}
                        {#if finding.topic}<button type="button" class="text" onclick={() => jumpTo(finding.topic)}>Open {finding.topic} <ArrowRight size={11} /></button>{/if}
                        <button type="button" class="text quiet" onclick={() => settle(i, 'open')} disabled={!!busy}><RotateCcw size={11} /> Reopen</button>
                      {/if}
                    </div>
                  </div>
                </li>
              {/each}
            </ol>
          {/if}
        </li>

        <li class="gate" class:locked={!reviewDone} class:done={sourcesDone} class:current={reviewDone && !sourcesDone} class:folded={isFolded('sources', sourcesDone)}>
          <div class="gate-head">
            <button type="button" class="gate-fold" aria-expanded={!isFolded('sources', sourcesDone)} aria-label="Fold the source check" onclick={() => toggleGate('sources', sourcesDone)}><ChevronDown size={14} /></button>
            <span class="gate-no mono" aria-hidden="true">{#if sourcesDone}<Check size={13} strokeWidth={2.6} />{:else if !reviewDone}<Lock size={11} />{:else}02{/if}</span>
            <span class="check-tile"><Globe size={16} /></span>
            <div><strong>Fetch every source</strong><small>The desk requests each primary source the way a lesson would. One that does not answer can be swapped for another the desk finds (the search engine on the course's hosts, then the tutor's proposals, each fetched before it is taken), replaced in the editor, or kept knowingly; a lesson that cannot fetch a source says so, as bundled lessons do.</small></div>
            <span class="gate-keys">
              {#if checks.pending_sources > 0}<button type="button" class="ghost mono-ghost" onclick={replaceAllSources} disabled={!!busy || !reviewDone}><Wand2 size={12} />{sourcesJob === 'replace' && !replacing ? `Finding… ${checks.pending_sources} left` : `Find another for all ${checks.pending_sources}`}</button>{/if}
              <button type="button" class="ghost mono-ghost" onclick={verifySources} disabled={!!busy || !reviewDone}>{sourcesJob === 'fetch' ? 'Fetching…' : checks.fetched ? 'Fetch again' : 'Fetch sources'}</button>
            </span>
          </div>
          <p class="gate-state mono">
            {#if !reviewDone}after the review{:else if !checks.fetched}not fetched yet · required{:else}<span class="ok"><CircleCheck size={11} /> {view.sources.filter((s) => s.state === 'reachable').length} reachable</span>{#if unreachable.length} · <span class:warn={checks.pending_sources > 0}><CircleDashed size={11} /> {unreachable.length} did not answer{checks.pending_sources ? `, ${checks.pending_sources} to settle` : ', all kept knowingly'}</span>{/if}{#if checks.unchecked_sources} · <span class="warn"><CircleAlert size={11} /> {checks.unchecked_sources} added since the last fetch; fetch again</span>{/if}{/if}
          </p>
          {#if busy === 'sources'}{@render liveFeed(sourcesJob === 'replace' ? 'finding another source' : DOING.sources)}{/if}
          {#if reviewDone && unreachable.length && !isFolded('sources', sourcesDone)}
            <ul class="sources">
              {#each unreachable as check (check.url)}
                <li class:kept={check.accepted}>
                  <span class="state mono" class:off={check.state === 'off-host'}>{check.state}</span>
                  <span class="url mono" title={check.url}>{check.url}</span>
                  {#if !check.accepted}<button type="button" class="text" onclick={() => replaceSource(check.url)} disabled={!!busy}><Wand2 size={11} /> {replacing === check.url ? 'finding another…' : 'Find another'}</button>{/if}
                  <button type="button" class="text" onclick={() => jumpTo(check.topic)}>{check.topic} <ArrowRight size={11} /></button>
                  <button type="button" class="text" class:quiet={check.accepted} onclick={() => acceptSource(check.url, !check.accepted)} disabled={!!busy}>{check.accepted ? 'Kept · undo' : 'Keep anyway'}</button>
                </li>
              {/each}
            </ul>
          {/if}
        </li>

        <li class="gate" class:locked={!sourcesDone} class:done={readDone} class:current={sourcesDone && !readDone} class:folded={isFolded('read', readDone)}>
          <div class="gate-head">
            <button type="button" class="gate-fold" aria-expanded={!isFolded('read', readDone)} aria-label="Fold the read-through" onclick={() => toggleGate('read', readDone)}><ChevronDown size={14} /></button>
            <span class="gate-no mono" aria-hidden="true">{#if readDone}<Check size={13} strokeWidth={2.6} />{:else if !sourcesDone}<Lock size={11} />{:else}03{/if}</span>
            <span class="check-tile"><BookOpenCheck size={16} /></span>
            <div><strong>Your own read-through</strong><small>Open every topic in the editor and read the course header as the tutor will. The tutor's review does not replace yours; an edit after you confirm asks for another look.</small></div>
          </div>
          <label class="confirm-row" class:muted={!sourcesDone} hidden={isFolded('read', readDone)}>
            <input type="checkbox" checked={readDone} disabled={!!busy || !sourcesDone} onchange={(e) => markRead(e.currentTarget.checked)} />
            <span>I have read every topic and the course header of this version</span>
          </label>
          <p class="gate-state mono">{#if !sourcesDone}after the sources{:else if readDone}confirmed for this version{:else}not confirmed · required{/if}</p>
        </li>

        <li class="gate optional" class:locked={!readDone} class:folded={isFolded('bank', false)}>
          <div class="gate-head">
            <button type="button" class="gate-fold" aria-expanded={!isFolded('bank', false)} aria-label="Fold the question bank" onclick={() => toggleGate('bank', false)}><ChevronDown size={14} /></button>
            <span class="gate-no mono" aria-hidden="true">{#if !readDone}<Lock size={11} />{:else}04{/if}</span>
            <span class="check-tile"><ListChecks size={16} /></span>
            <div><strong>Write the question bank <em class="mono">optional</em></strong><small>Three cited four-choice questions per stage, on the course's core topics. They power the placement check and unit challenges; without them the class starts from scratch or a stage you choose. Held to the same shape as the bundled banks.</small></div>
            <button type="button" class="ghost mono-ghost" onclick={writeBank} disabled={!!busy || issues.length > 0 || !readDone} title={issues.length ? 'Fix the editor first' : ''}>{busy === 'bank' ? 'Writing…' : bankQuestions.length ? 'Write it again' : 'Write the bank'}</button>
          </div>
          {#if busy === 'bank'}{@render liveFeed(DOING.bank)}{/if}
          {#if bankQuestions.length && !isFolded('bank', false)}
            <div class="source-summary mono"><span class="ok"><CircleCheck size={11} /> {bankQuestions.length - voided.length} usable</span>{#if voided.length}<span class="warn"><CircleAlert size={11} /> {voided.length} disputed</span>{/if}{#if bankStale}<span class="warn"><CircleAlert size={11} /> some questions point at topics no longer in the draft; write it again</span>{/if}</div>
            <ol class="bank-list">
              {#each bankQuestions as q (q.id)}
                <li class:voided={q.voided}><span class="stage mono">{q.entry_point}</span><div class="q"><p>{q.prompt}</p><small>key: {q.choices.find((c) => c.id === q.answer)?.text ?? q.answer}{#if q.source} · <span class="mono">{q.source}</span>{/if}{#if q.voided} · <b>disputed:</b> {q.void_reason || 'no reason given'}{/if}</small></div></li>
              {/each}
            </ol>
          {:else}
            <p class="gate-state mono">{#if !readDone}after your read-through{:else}not written yet · it can also be written after the class is published, from its Curriculum tab{/if}</p>
          {/if}
        </li>
      </ol>
      <footer class="nav">
        <button type="button" class="ghost mono-ghost" onclick={() => go('review')}><ArrowLeft size={12} />Back to the editor</button>
        <span class="hint mono" class:warn-text={!canEnroll}>{canEnroll ? 'ready to publish' : checks.blockers[0]}</span>
        <button type="button" class="cta mono-cta" disabled={!canEnroll} onclick={() => (step = 'enroll')}>Continue <ArrowRight size={13} /></button>
      </footer>
    </section>

  {:else if step === 'enroll' && draft && view}
    <section class="panel" aria-label="Enroll">
      <div class="panel-head"><h3>{view.status === 'published' ? `Publish version ${view.version + 1}` : 'Publish the class'}</h3><p>{view.status === 'published' ? 'The class keeps its history and its accepted path; the new version is what lessons are planned from next.' : 'Publishing makes the class real: it joins your classes with a custom badge and opens like any other. From there you set its starting point and study times when you are ready.'}</p></div>
      <ul class="manifest">
        <li><span class="mf-label mono">CLASS</span><strong>{draft.label}</strong><small>{draft.short_code} · {draft.native_label || 'your own course'}</small></li>
        <li><span class="mf-label mono">TOPICS</span><strong>{draft.topics.length}</strong><small>{draft.topics.filter((t) => t.curriculum.core).length} core · {draft.entry_points.map((e) => e.label).join(' → ')}</small></li>
        <li><span class="mf-label mono">TUTOR</span><strong>{brief.agent} / {brief.model || 'runner default'}</strong><small>from the brief; changeable in the class's Settings tab</small></li>
        <li><span class="mf-label mono">CHECKS</span><strong class:warn={!canEnroll}>{canEnroll ? 'all pass' : view.checks.blockers[0]}</strong><small>{view.review.length ? `${settled} of ${view.review.length} finding${view.review.length === 1 ? '' : 's'} settled` : 'read back, nothing raised'} · {view.sources.length ? `${unreachable.length} source${unreachable.length === 1 ? '' : 's'} kept unreachable` : 'sources fetched'} · read-through {view.checks.read ? 'confirmed' : 'pending'} · {bankQuestions.length ? `${bankQuestions.length - voided.length} questions` : 'no question bank'}</small></li>
      </ul>
      <p class="after mono">then, from the class: a starting point ({bankQuestions.length ? 'from scratch, a stage, or the placement check' : 'from scratch or a stage; the placement check needs the question bank'}) → study times → the class activates</p>
      <footer class="nav">
        <button type="button" class="ghost mono-ghost" onclick={() => (step = 'verify')}><ArrowLeft size={12} />Back</button>
        <span class="hint mono">{canEnroll ? 'ready' : 'the desk will refuse until every check passes'}</span>
        <button type="button" class="ghost mono-ghost" onclick={exportFile} disabled={!!busy}><Save size={12} />Export class file</button>
        <button type="button" class="cta mono-cta" onclick={publish} disabled={!!busy || !canEnroll}><Rocket size={13} />{busy === 'publish' ? 'Publishing…' : view.status === 'published' ? `Publish v${view.version + 1}` : 'Publish'}</button>
      </footer>
    </section>
  {/if}
</div>

<style>
  /* The builder is the scrolling pane inside the workspace's details column. */
  .builder { display: flex; flex-direction: column; gap: 16px; padding: 22px 26px 40px; min-width: 0; flex: 1; min-height: 0; overflow-y: auto; overflow-x: hidden; overscroll-behavior: contain; scrollbar-gutter: stable; }
  /* The tutor's last lines, and under them a clock that moves while the runner is silent. */
  .feed-wrap { display: flex; flex-direction: column; border: 1px solid var(--node-border); border-radius: var(--radius-control); background: #0b0d10; overflow: hidden; }
  .feed-wrap .feed { border: 0; border-radius: 0; }
  .feed-live { display: flex; align-items: center; gap: 8px; padding: 8px 12px; border-top: 1px solid var(--node-divider); font-size: 10.5px; letter-spacing: 0.4px; color: var(--accent); }
  .feed-live b { font-weight: 500; font-variant-numeric: tabular-nums; } .feed-hint { margin-left: auto; color: var(--faint); font-size: 9.5px; }
  .beacon { position: relative; display: inline-block; flex: none; width: 8px; height: 8px; border-radius: 50%; background: var(--accent); box-shadow: 0 0 8px var(--accent); }
  .beacon::after { content: ''; position: absolute; inset: -4px; border-radius: 50%; border: 1px solid var(--accent); animation: ring 1.6s ease-out infinite; }
  @keyframes ring { from { transform: scale(0.5); opacity: 0.9; } to { transform: scale(1.8); opacity: 0; } }
  .dots::after { content: ''; animation: dots 1.5s steps(4, end) infinite; } @keyframes dots { 0% { content: ''; } 25% { content: '.'; } 50% { content: '..'; } 75% { content: '...'; } }
  .top { display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; }
  .top h2 { margin: 6px 0 0; font: 24px var(--font-display); }
  .top-actions { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; justify-content: flex-end; }
  .save-state { font-size: 9px; letter-spacing: 0.6px; color: var(--faint); min-width: 70px; text-align: right; }
  .error { display: flex; align-items: center; gap: 7px; margin: 0; padding: 10px 12px; border-left: 2px solid var(--led-err); background: var(--surface); color: var(--led-err); font-size: 12px; overflow-wrap: anywhere; }

  /* The rail, as the setup wizard draws it. */
  .steps { display: flex; gap: 8px; list-style: none; margin: 0; padding: 0; }
  .steps li { position: relative; flex: 1; min-width: 0; padding-bottom: 12px; }
  .steps li::after { content: ''; position: absolute; left: 0; right: 0; bottom: 0; height: 2px; border-radius: 2px; background: var(--node-border); transition: background 0.3s; }
  .steps li.done::after { background: var(--accent); }
  .steps li.current::after { background: linear-gradient(90deg, var(--accent) 0 40%, var(--node-border) 100%); }
  .steps button { display: flex; align-items: flex-start; gap: 10px; width: 100%; padding: 0 6px 0 0; border: 0; background: transparent; color: var(--faint); text-align: left; cursor: pointer; }
  .steps button:disabled { cursor: default; }
  .step-tile { flex: none; display: grid; place-items: center; width: 34px; height: 34px; border: 1px solid var(--node-border); border-radius: 10px; background: var(--surface); color: var(--faint); transition: all 0.25s; }
  .steps .current .step-tile { border-color: var(--accent); background: linear-gradient(180deg, color-mix(in srgb, var(--accent) 22%, var(--surface)), var(--surface)); color: var(--accent); box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 16%, transparent); }
  .steps .done .step-tile { border-color: color-mix(in srgb, var(--led-ok) 55%, var(--node-border)); background: var(--ok-bg); color: var(--ok-fg); }
  .step-text { display: flex; flex-direction: column; gap: 1px; min-width: 0; padding-top: 1px; }
  .step-no { font-size: 8.5px; letter-spacing: 1.2px; text-transform: uppercase; color: var(--faint); } .steps .current .step-no { color: var(--accent); } .steps .done .step-no { color: var(--ok-fg); }
  .step-name { font-size: 13px; color: var(--faint); } .steps .current .step-name { color: var(--fg); font-weight: 500; } .steps .done .step-name { color: var(--muted); }
  .step-sub { font-size: 10.5px; color: var(--faint); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; } .steps .current .step-sub, .steps .done .step-sub { color: var(--muted); }

  .panel { display: flex; flex-direction: column; gap: 16px; padding: 18px; border: 1px solid var(--node-border); border-radius: var(--radius-panel); background: var(--node-bg); }
  .panel.bare { padding: 0; border: 0; background: transparent; }
  .panel-head h3 { margin: 0 0 6px; font: 19px var(--font-display); } .panel-head p { margin: 0; font-size: 12.5px; line-height: 1.6; color: var(--muted); max-width: 70ch; }
  .brief-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 14px 18px; } .span-2 { grid-column: 1 / -1; }
  .field { display: flex; flex-direction: column; gap: 6px; min-width: 0; } .field > span { font-size: 12px; color: var(--muted); } .field > span small { margin-left: 6px; color: var(--faint); font-size: 10px; }
  .pick-label { font-size: 9px; letter-spacing: 1.2px; color: var(--faint); }
  input:not([type='checkbox']), textarea { width: 100%; padding: 10px 12px; border: 1px solid var(--node-border); border-radius: var(--radius-control); background: var(--bg); color: var(--fg); font: 13px/1.5 var(--font-body); } textarea { display: block; }
  input:not([type='checkbox']):focus, textarea:focus { outline: none; border-color: var(--accent); }
  .hosts { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; padding: 6px; border: 1px solid var(--node-border); border-radius: var(--radius-control); background: var(--bg); }
  .host { display: inline-flex; align-items: center; gap: 5px; padding: 4px 6px 4px 9px; border: 1px solid var(--node-border); border-radius: 6px; background: var(--surface); font-size: 10.5px; color: var(--fg); }
  .host button { display: grid; place-items: center; width: 16px; height: 16px; border: 0; border-radius: 3px; background: transparent; color: var(--faint); cursor: pointer; } .host button:hover { color: var(--led-err); }
  .host-input { flex: 1; min-width: 160px; border: 0; background: transparent; padding: 4px 6px; font-size: 11.5px; }
  .nav { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; padding-top: 14px; border-top: 1px dashed var(--node-divider); } .nav .cta { margin-left: auto; display: inline-flex; align-items: center; gap: 7px; } .nav.back { border-top: 0; padding-top: 0; }
  .hint { font-size: 9.5px; letter-spacing: 0.5px; color: var(--faint); }
  .ways { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 12px; }
  .way { position: relative; display: flex; flex-direction: column; align-items: flex-start; gap: 8px; padding: 16px; border: 1px solid var(--node-border); border-radius: var(--radius-panel); background: var(--surface); color: var(--fg); text-align: left; cursor: pointer; transition: border-color 0.15s, transform 0.12s; }
  .way:hover:not(:disabled) { border-color: color-mix(in srgb, var(--accent) 50%, var(--node-border)); transform: translateY(-1px); }
  .way:disabled { opacity: 0.6; cursor: default; } .way.busy { opacity: 1; border-color: var(--accent); }
  .way-tile { display: grid; place-items: center; width: 42px; height: 42px; border: 1px solid color-mix(in srgb, var(--accent) 35%, var(--node-border)); border-radius: 11px; background: linear-gradient(180deg, color-mix(in srgb, var(--accent) 18%, var(--surface)), var(--surface)); color: var(--accent); }
  .way strong { font-size: 13.5px; font-weight: 500; } .way small { font-size: 11px; line-height: 1.5; color: var(--muted); }
  .working { font-size: 9.5px; letter-spacing: 0.5px; color: var(--accent); animation: pulse 1.2s ease-in-out infinite; } @keyframes pulse { 50% { opacity: 0.4; } }
  .feed { margin: 0; padding: 10px 12px; max-height: 140px; overflow: hidden; border: 1px solid var(--node-border); border-radius: var(--radius-control); background: #0b0d10; color: var(--muted); font-size: 10.5px; line-height: 1.6; white-space: pre-wrap; }
  /* The three gates and the optional fourth, in the order they unlock. */
  .gates { display: flex; flex-direction: column; gap: 10px; margin: 0; padding: 0; list-style: none; }
  .gate { position: relative; display: flex; flex-direction: column; gap: 12px; padding: 14px 16px; border: 1px solid var(--node-border); border-radius: var(--radius-panel); background: var(--surface); transition: border-color 0.2s, opacity 0.2s; }
  .gate.current { border-color: color-mix(in srgb, var(--accent) 50%, var(--node-border)); }
  .gate.done { border-color: color-mix(in srgb, var(--led-ok) 45%, var(--node-border)); }
  .gate.locked { opacity: 0.55; }
  /* Folded to its head and state line; done gates start that way. */
  .gate-fold { flex: none; display: grid; place-items: center; width: 24px; height: 24px; margin: 6px 0 0 -4px; border: 0; border-radius: var(--radius-detail); background: transparent; color: var(--muted); cursor: pointer; transition: transform 0.18s ease; }
  .gate-fold:hover { background: var(--node-bg); color: var(--fg); } .gate.folded .gate-fold { transform: rotate(-90deg); }
  .gate.folded .gate-head small { display: none; }
  .gate.folded > :not(.gate-head):not(.gate-state):not(.feed-wrap) { display: none; }
  /* The words keep their width; when the keys do not fit beside them they drop to a row of their own, right-aligned. */
  .gate-head { display: flex; gap: 12px; align-items: flex-start; flex-wrap: wrap; } .gate-head > div { flex: 1 1 380px; min-width: 0; } .gate-head strong { display: flex; align-items: center; gap: 8px; font-size: 13px; font-weight: 500; } .gate-head strong em { font-style: normal; font-size: 8.5px; letter-spacing: 1px; text-transform: uppercase; color: var(--faint); } .gate-head small { display: block; margin-top: 4px; font-size: 11px; line-height: 1.5; color: var(--muted); max-width: 72ch; }
  .gate-no { flex: none; display: grid; place-items: center; width: 26px; height: 26px; margin-top: 5px; border: 1px solid var(--node-border); border-radius: 8px; background: var(--node-bg); color: var(--faint); font-size: 9.5px; letter-spacing: 0.5px; }
  .gate.current .gate-no { border-color: var(--accent); color: var(--accent); box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 16%, transparent); }
  .gate.done .gate-no { border-color: color-mix(in srgb, var(--led-ok) 55%, var(--node-border)); background: var(--ok-bg); color: var(--ok-fg); }
  .check-tile { flex: none; display: grid; place-items: center; width: 36px; height: 36px; border-radius: 10px; background: var(--surface-2); color: var(--accent); }
  .gate-state { display: flex; flex-wrap: wrap; align-items: center; gap: 4px; margin: 0; font-size: 10px; letter-spacing: 0.5px; color: var(--muted); } .gate-state span { display: inline-flex; align-items: center; gap: 5px; } .gate-state .ok { color: var(--ok-fg); } .gate-state .warn { color: var(--warn-fg); }
  .confirm-row { display: flex; align-items: center; gap: 10px; padding: 10px 12px; border: 1px dashed var(--node-border); border-radius: var(--radius-control); background: var(--node-bg); font-size: 12.5px; cursor: pointer; } .confirm-row.muted { cursor: default; color: var(--faint); }
  .findings { display: flex; flex-direction: column; gap: 8px; margin: 0; padding: 0; list-style: none; }
  .findings li { display: flex; gap: 10px; padding: 10px 12px; border: 1px solid var(--node-border); border-left-width: 3px; border-radius: var(--radius-control); background: var(--node-bg); }
  .findings li.high { border-left-color: var(--led-err); } .findings li.medium { border-left-color: var(--warn-fg); } .findings li.low { border-left-color: var(--node-border); }
  .findings li.settled { border-left-color: color-mix(in srgb, var(--led-ok) 55%, var(--node-border)); } .findings li.settled .finding-body > p:first-child { color: var(--muted); }
  .sev-col { flex: none; display: flex; flex-direction: column; gap: 5px; width: 74px; padding-top: 3px; }
  .sev { font-size: 9px; letter-spacing: 0.8px; text-transform: uppercase; color: var(--muted); }
  .pill { align-self: flex-start; padding: 1px 6px; border-radius: 999px; font-size: 8.5px; letter-spacing: 0.6px; text-transform: uppercase; } .pill.open { background: var(--warn-bg); color: var(--warn-fg); } .pill.fixed { background: var(--ok-bg); color: var(--ok-fg); } .pill.dismissed { background: var(--surface-2); color: var(--faint); } .pill.earlier { background: transparent; border: 1px dashed var(--node-border); color: var(--faint); white-space: nowrap; }
  .gate-keys { display: inline-flex; flex-wrap: wrap; justify-content: flex-end; gap: 8px; margin-left: auto; }
  .gate-head > .ghost:last-child { margin-left: auto; }
  .finding-body { display: flex; flex-direction: column; gap: 4px; min-width: 0; flex: 1; } .finding-body p { margin: 0; font-size: 12px; line-height: 1.5; } .fix { color: var(--muted); font-size: 11.5px !important; } .fix b { color: var(--fg); font-weight: 500; }
  .note { font-size: 10px !important; letter-spacing: 0.3px; color: var(--ok-fg); } .findings li.settled .note { color: var(--muted); }
  .finding-actions { display: flex; flex-wrap: wrap; align-items: center; gap: 6px 14px; margin-top: 4px; }
  .text { display: inline-flex; align-items: center; gap: 4px; padding: 0; border: 0; background: transparent; color: var(--accent); font: 11px var(--font-body); cursor: pointer; } .text.quiet { color: var(--faint); } .text:disabled { opacity: 0.5; cursor: default; }
  .source-summary { display: flex; gap: 14px; font-size: 10px; letter-spacing: 0.5px; color: var(--muted); } .source-summary span { display: inline-flex; align-items: center; gap: 5px; } .source-summary .ok { color: var(--ok-fg); } .source-summary .warn { color: var(--warn-fg); }
  .sources { display: flex; flex-direction: column; gap: 5px; margin: 0; padding: 0; list-style: none; max-height: 260px; overflow-y: auto; }
  .sources li { display: flex; align-items: center; gap: 10px; font-size: 11px; } .sources li.kept { opacity: 0.7; } .state { flex: none; width: 76px; font-size: 9px; letter-spacing: 0.6px; color: var(--warn-fg); } .state.off { color: var(--led-err); } .url { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--muted); font-size: 10.5px; }
  .warn-text { color: var(--warn-fg); }
  .bank-list { display: flex; flex-direction: column; gap: 5px; margin: 0; padding: 0; list-style: none; max-height: 320px; overflow-y: auto; }
  .bank-list li { display: flex; gap: 10px; padding: 8px 10px; border: 1px solid var(--node-border); border-radius: var(--radius-control); background: var(--node-bg); } .bank-list li.voided { opacity: 0.6; border-style: dashed; }
  .bank-list .stage { flex: none; width: 82px; font-size: 9px; letter-spacing: 0.6px; color: var(--accent); padding-top: 2px; }
  .bank-list .q { display: flex; flex-direction: column; gap: 3px; min-width: 0; } .bank-list .q p { margin: 0; font-size: 12px; line-height: 1.45; } .bank-list .q small { font-size: 10.5px; color: var(--muted); overflow-wrap: anywhere; } .bank-list .q small b { color: var(--warn-fg); font-weight: 500; }
  .manifest { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 10px; margin: 0; padding: 0; list-style: none; }
  .manifest li { display: flex; flex-direction: column; gap: 3px; padding: 12px 14px; border: 1px solid var(--node-border); border-radius: var(--radius-panel); background: var(--surface); }
  .mf-label { font-size: 9px; letter-spacing: 1.2px; color: var(--faint); } .manifest strong { font-size: 13px; font-weight: 500; } .manifest strong.warn { color: var(--warn-fg); } .manifest small { font-size: 11px; color: var(--muted); line-height: 1.45; }
  .after { margin: 0; font-size: 9.5px; letter-spacing: 0.4px; color: var(--faint); line-height: 1.6; }
  .danger { --key: var(--led-err); }
  @media (max-width: 900px) { .ways, .manifest, .brief-grid { grid-template-columns: 1fr; } .step-text { display: none; } .steps button { justify-content: center; padding: 0; } }
</style>
