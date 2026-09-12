<script lang="ts">
  import { tick } from 'svelte';
  import type { PathRecommendation, PathTopic } from '../../contracts/placement';
  import type { EnrollmentConfiguration } from '../../contracts/enrollment';
  type TutorPreference = EnrollmentConfiguration['tutor'];
  import { app } from '../../stores.svelte';
  import { ArrowLeft, ArrowRight, Check, Flag, Search } from 'lucide-svelte';
  let { path, onclose, onfoundations, onaccept, accepting = false, error = '', acceptedRevision, embedded = false, tutor = null }: { path: PathRecommendation; onclose: () => void; onfoundations: () => void; onaccept?: () => void; accepting?: boolean; error?: string; acceptedRevision?: number; embedded?: boolean; tutor?: TutorPreference | null } = $props();
  /** The tutor this class starts on. It is the desk's active tutor unless the class already had its own. */
  const tutorLine = $derived(tutor ? `${tutor.provider === 'custom' ? 'Custom CLI' : tutor.provider} · ${tutor.model || 'runner default'}` : '');
  const inheritsDesk = $derived(!!tutor && !!app.state && tutor.provider === app.state.agent && tutor.model === app.state.model);
  $effect(() => { path.id; if (embedded) return; void tick().then(() => document.getElementById('desk-content')?.scrollTo(0, 0)); });

  const passed = $derived(path.criteria.filter((c) => c.verdict === 'passed').length);
  const sampled = $derived(path.criteria.length);
  const SOURCE: Record<string, string> = { diagnostic: 'a short check', manual: 'the stage you chose', foundations: 'starting from scratch' };

  /** Reasons written per topic are usually one sentence repeated down the
   *  list. Say it once above the group, and only annotate the exceptions. */
  function sharedReason(items: PathTopic[]) {
    const first = items[0]?.reason?.trim() ?? '';
    return first && items.every((item) => item.reason?.trim() === first) ? first : '';
  }
  const refresherNote = $derived(sharedReason(path.refreshers));
  const earlierNote = $derived(sharedReason(path.earlier_topics));

  let tab = $state<'refreshers' | 'earlier'>('refreshers');
  let query = $state('');
  $effect(() => { if (!path.refreshers.length) tab = 'earlier'; });
  const shown = $derived(tab === 'refreshers' ? path.refreshers : path.earlier_topics);
  const note = $derived(tab === 'refreshers' ? refresherNote : earlierNote);
  const matches = $derived(
    query.trim()
      ? shown.filter((item) => item.label.toLowerCase().includes(query.trim().toLowerCase()))
      : shown,
  );
</script>

<section class="path" class:embedded aria-label="Suggested learning path">
  {#if !embedded || !acceptedRevision}<button class="ghost mono-ghost" onclick={onclose} disabled={accepting}><ArrowLeft size={13} /> {acceptedRevision ? 'Back to classes' : 'Change starting point'}</button>{/if}

  <header>
    <p class="eyebrow mono">PERSONAL PATH · {acceptedRevision ? `ACCEPTED REVISION ${acceptedRevision}` : 'PREVIEW'} · FROM {(SOURCE[path.route] ?? path.route).toUpperCase()}</p>
    <div class="hero">
      <div>
        <h2>Your lessons start at <em>{path.entry_label}</em></h2>
        <p class="lead">{path.explanation}</p>
      </div>
      {#if sampled}
        <div class="score" role="img" aria-label={`${passed} of ${sampled} sampled criteria demonstrated`}>
          <strong>{passed}<span>/{sampled}</span></strong>
          <small>samples demonstrated</small>
          <div class="meter"><i style:width={`${(passed / sampled) * 100}%`}></i></div>
        </div>
      {/if}
    </div>
  </header>

  <!-- The route as one line: what is behind you, what comes first, where you
       begin, and what is still required at the end. -->
  <ol class="route" aria-label="Your route">
    <li>
      <span class="count mono">{path.earlier_topics.length}</span>
      <span class="step-name">set aside</span>
      <span class="step-note">{path.earlier_topics.length ? 'kept, never deleted' : 'nothing skipped'}</span>
    </li>
    <li>
      <span class="count mono amber">{path.refreshers.length}</span>
      <span class="step-name">refreshers first</span>
      <span class="step-note">{path.refreshers.length ? 'before the work that needs them' : 'no gaps found'}</span>
    </li>
    <li class="here">
      <span class="count mono"><Check size={14} /></span>
      <span class="step-name">{path.entry_label}</span>
      <span class="step-note">your first lesson</span>
    </li>
    <li>
      <span class="count mono"><Flag size={13} /></span>
      <span class="step-name">final assessment</span>
      <span class="step-note">always required</span>
    </li>
  </ol>

  {#if path.refreshers.length || path.earlier_topics.length}
    <section class="detail" aria-label="Topics on this route">
      <div class="detail-head">
        <div class="tabs" role="group" aria-label="Which topics to show">
          {#if path.refreshers.length}<button type="button" class:on={tab === 'refreshers'} aria-pressed={tab === 'refreshers'} onclick={() => { tab = 'refreshers'; query = ''; }}>Refreshers <span>{path.refreshers.length}</span></button>{/if}
          {#if path.earlier_topics.length}<button type="button" class:on={tab === 'earlier'} aria-pressed={tab === 'earlier'} onclick={() => { tab = 'earlier'; query = ''; }}>Set aside <span>{path.earlier_topics.length}</span></button>{/if}
        </div>
        {#if shown.length > 8}
          <label class="find field-group"><Search size={13} /><input aria-label="Find a topic" placeholder="Find a topic…" bind:value={query} /></label>
        {/if}
      </div>
      {#if note}<p class="note">{note}</p>{/if}
      <ul class="topics">
        {#each matches as item (item.id)}
          <li><span>{item.label}</span>{#if !note && item.reason}<small>{item.reason}</small>{/if}</li>
        {/each}
        {#if !matches.length}<li class="empty">No topic matches “{query}”.</li>{/if}
      </ul>
    </section>
  {/if}

  <section class="outcome" aria-label="Required outcome">
    <p>{path.required_outcome}</p>
  </section>

  <footer>
    {#if !acceptedRevision && tutorLine}<p class="tutor-line"><span class="mono">TUTOR</span> {tutorLine}{inheritsDesk ? ', from your desk settings' : ''}. Lessons are written by it once the class is active; change it any time in this class's Settings tab.</p>{/if}
    <p>{acceptedRevision ? 'This accepted path guides future lessons. Saved lessons and their original results remain in your history.' : 'Accepting saves this path. A saved study time is required to activate the class. Existing work and schedules are preserved.'}</p>
    {#if error}<p class="activation-error" role="alert">{error}</p>{/if}
    <div class="actions">
      <button class="ghost mono-ghost" onclick={onclose} disabled={accepting}>{acceptedRevision ? (embedded ? 'Overview' : 'Back to classes') : 'Adjust setup'}</button>
      <button class="ghost mono-ghost" onclick={onfoundations} disabled={accepting}>{acceptedRevision ? 'Revise starting point' : 'Include foundations'}</button>
      {#if onaccept}<button class="cta mono-cta" onclick={onaccept} disabled={accepting}>{accepting ? 'Saving path…' : 'Accept learning path'} <ArrowRight size={13} /></button>{/if}
    </div>
  </footer>
</section>

<style>
  .path { width: min(880px, 100%); margin: auto; padding: 26px 28px 48px; display: grid; gap: 20px; align-content: start; }
  .path > button { justify-self: start; }
  header { display: grid; gap: 10px; }
  .eyebrow { font-size: 10px; color: var(--faint); letter-spacing: .7px; }
  .hero { display: flex; align-items: flex-start; justify-content: space-between; gap: 24px; flex-wrap: wrap; }
  h2 { font: 28px/1.25 var(--font-display); margin: 0 0 8px; max-width: 20ch; }
  h2 em { font-style: normal; color: var(--accent); }
  .lead { font-size: 13px; line-height: 1.65; color: var(--muted); margin: 0; max-width: 62ch; }

  .score { flex-shrink: 0; display: grid; gap: 4px; justify-items: end; min-width: 132px; }
  .score strong { font: 26px var(--font-display); color: var(--fg); line-height: 1; }
  .score strong span { color: var(--muted); font-size: 16px; }
  .score small { font: 9px var(--font-mono); color: var(--faint); letter-spacing: .6px; text-transform: uppercase; }
  .meter { width: 100%; height: 3px; border-radius: 2px; background: var(--surface-2); overflow: hidden; }
  .meter i { display: block; height: 100%; background: var(--accent); }

  .route { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 1px; list-style: none; margin: 0; padding: 0; border: 1px solid var(--node-border); border-radius: var(--radius-panel); background: var(--node-border); overflow: hidden; }
  .route li { display: grid; gap: 5px; padding: 13px 14px; background: var(--surface); align-content: start; }
  .route .here { background: color-mix(in srgb, var(--accent) 9%, var(--surface)); }
  .count { display: flex; align-items: center; gap: 4px; font-size: 17px; color: var(--fg); line-height: 1; }
  .count.amber { color: var(--led-warn, var(--accent)); }
  .route .here .count { color: var(--accent); }
  .step-name { font-size: 12px; color: var(--fg); }
  .route .here .step-name { font-weight: 500; }
  .step-note { font-size: 10px; color: var(--muted); line-height: 1.45; }

  .detail { border: 1px solid var(--node-border); border-radius: var(--radius-panel); padding: 12px 14px 14px; display: grid; gap: 10px; }
  .detail-head { display: flex; align-items: center; justify-content: space-between; gap: 12px; flex-wrap: wrap; }
  .tabs { display: flex; gap: 2px; }
  .tabs button { display: flex; align-items: center; gap: 6px; padding: 6px 10px; border: 1px solid transparent; border-radius: var(--radius-control); background: transparent; color: var(--muted); font-size: 12px; cursor: pointer; }
  .tabs button:hover:not(.on) { color: var(--fg); background: var(--surface); }
  .tabs .on { color: var(--fg); background: var(--surface-2); border-color: var(--node-border); }
  .tabs span { font: 9px var(--font-mono); padding: 2px 5px; border-radius: var(--radius-detail); background: var(--bg); color: var(--muted); }
  .tabs .on span { background: var(--accent); color: var(--bg); }
  .find { display: flex; align-items: center; gap: 7px; padding: 0 10px; border: 1px solid var(--node-border); border-radius: var(--radius-control); color: var(--muted); background: var(--bg); }
  .find input { width: 150px; border: 0; outline: none; background: none; color: var(--fg); font-size: 11px; padding: 7px 0; }
  .note { margin: 0; font-size: 11px; line-height: 1.6; color: var(--muted); }

  .topics { list-style: none; margin: 0; padding: 0; display: grid; grid-template-columns: repeat(auto-fill, minmax(230px, 1fr)); gap: 2px 14px; max-height: 232px; overflow-y: auto; }
  .topics li { display: grid; gap: 2px; padding: 6px 0; border-bottom: 1px solid var(--node-divider); }
  .topics span { font-size: 12px; color: var(--fg); line-height: 1.45; }
  .topics small { font-size: 10px; color: var(--muted); line-height: 1.5; }
  .topics .empty { color: var(--muted); font-size: 12px; border: 0; }

  .outcome { border-left: 2px solid var(--violet); padding-left: 14px; display: grid; gap: 8px; }
  .outcome > p { margin: 0; font-size: 13px; line-height: 1.65; }

  footer { display: grid; gap: 12px; }
  footer > p { margin: 0; font: 11px/1.7 var(--font-mono); color: var(--muted); border-left: 2px solid var(--node-border); padding-left: 12px; }
  .tutor-line { font-family: var(--font-body) !important; font-size: 12px !important; color: var(--fg) !important; } .tutor-line .mono { font-size: 9px; letter-spacing: .7px; color: var(--faint); margin-right: 8px; }
  .activation-error { color: var(--led-err); }
  .actions { display: flex; flex-wrap: wrap; gap: 10px; justify-content: flex-end; }

  @media (max-width: 720px) {
    .path { padding: 20px 16px; }
    .route { grid-template-columns: repeat(2, minmax(0, 1fr)); }
    .score { justify-items: start; }
  }
  .path.embedded { width: 100%; max-width: 1000px; padding: 0; }
</style>
