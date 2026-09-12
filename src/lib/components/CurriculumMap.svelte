<script lang="ts">
  import { courseDefinition } from '../catalog';
  import type { CurriculumConceptView, CurriculumMapView, CurriculumPhase } from '../ipc';
  import { CheckCircle2, Circle, Clock3, Link2, X, Search, ChevronDown } from 'lucide-svelte';

  import type { PathChange } from '../contracts/classes';

  let {
    map,
    onclose,
    embedded = false,
    onrevise,
    revising = false,
    onchallenge,
  }: {
    map: CurriculumMapView;
    onclose?: () => void;
    embedded?: boolean;
    /** Bypass or include topics on the accepted route (engineering classes with a path). */
    onrevise?: (change: PathChange) => void;
    revising?: boolean;
    /** Open a unit challenge for a phase (engineering classes with a path). */
    onchallenge?: (phase: CurriculumPhase, label: string) => void;
  } = $props();

  const BADGES: Record<CurriculumConceptView['path_status'], { label: string; tone: string; meaning: string }> = {
    completed_here: { label: 'Completed here', tone: 'ok', meaning: 'You finished a lesson on this topic in this class.' },
    prior_knowledge_checked: { label: 'Prior knowledge checked', tone: 'teal', meaning: 'A unit challenge showed you already know this, so no lesson is scheduled.' },
    bypassed_by_choice: { label: 'Set aside by choice', tone: 'muted', meaning: 'You chose to skip it. It was never assessed and earns no credit.' },
    needs_refresher: { label: 'Needs refresher', tone: 'warn', meaning: 'A sample for this topic was not demonstrated in your starting-point check, so a short refresher runs before the work that depends on it.' },
    not_assessed: { label: 'Not assessed', tone: 'muted', meaning: 'Before your starting point and never sampled: unknown rather than failed.' },
    bridge: { label: 'Bridge lesson', tone: 'warn', meaning: 'A gap seen during practice. A short lesson is queued before more dependent work.' },
    in_progress: { label: 'In progress', tone: 'accent', meaning: 'A lesson on this topic is open right now.' },
    taught: { label: 'Taught', tone: 'teal', meaning: 'A lesson on this topic is done. It counts as mastered once the checks and the retrieval that follow show it stuck.' },
    upcoming: { label: 'Upcoming', tone: 'faint', meaning: 'On your route and still ahead of you.' },
  };
  /** Only the states actually present, so the legend explains what is on screen. */
  const legend = $derived([...new Set(map.concepts.map((c) => c.path_status))].map((status) => ({ status, ...BADGES[status] })).filter((row) => row.label));
  const canBypass = (status: CurriculumConceptView['path_status']) => ['upcoming', 'in_progress', 'taught', 'needs_refresher'].includes(status);
  const canInclude = (status: CurriculumConceptView['path_status']) => ['bypassed_by_choice', 'not_assessed', 'prior_knowledge_checked'].includes(status);

  const uid = $props.id();
  let query = $state('');
  const matches = $derived(map.concepts.filter(c => `${c.title} ${c.learner_outcome}`.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase())));
  const phases = $derived([
    ...(courseDefinition(map.focus)?.entry_points ?? []).map((entry) => ({
      id: entry.id as CurriculumPhase, label: entry.label,
    })),
    { id: 'elective' as CurriculumPhase, label: 'Electives' },
  ].filter((phase) => map.concepts.some((concept) => concept.phase === phase.id)));
  const coreCount = $derived(map.concepts.filter((concept) => concept.core).length);
  function prerequisiteTitles(slugs: string[]) {
    return slugs.map((slug) => map.concepts.find((concept) => concept.slug === slug)?.title ?? slug).join('; ');
  }

  function conceptsFor(phase: CurriculumPhase) {
    return matches.filter((concept) => concept.phase === phase);
  }

  function complete(state: string) {
    return state === 'mastered' || state === 'maintenance';
  }

  /** Stages folded to their head, remembered per course; a search unfolds the ones with matches. */
  const foldKey = $derived(`principia-map-folds:${map.focus}`);
  let folded = $state<Record<string, boolean>>({});
  $effect(() => {
    const key = foldKey;
    try { folded = JSON.parse(localStorage.getItem(key) ?? '{}'); } catch { folded = {}; }
  });
  function toggle(phase: string) {
    folded = { ...folded, [phase]: !folded[phase] };
    try { localStorage.setItem(foldKey, JSON.stringify(folded)); } catch { /* memory only */ }
  }
  const isFolded = (phase: string) => !!folded[phase] && !query.trim();
</script>

<section class="curriculum-map" class:embedded aria-labelledby={`${uid}-title`}>
  <header>
    <div>
      <span class="eyebrow mono">{coreCount} CORE TOPICS · {map.completed_sessions} SESSIONS COMPLETED</span>
      <h3 id={`${uid}-title`}>{map.label} learning map</h3>
      <p>{map.month_outcome}</p>
      {#if map.path}
        <dl class="coverage mono" aria-label="Route and coverage">
          <div><dt>Taught</dt><dd>{map.path.taught ?? 0} / {map.path.coverage_total} core</dd></div>
          <div><dt>Mastered on your route</dt><dd>{map.path.required_done} / {map.path.required_total}</dd></div>
          <div><dt>Course coverage</dt><dd>{map.path.coverage_done} / {map.path.coverage_total} core mastered</dd></div>
          <div><dt>Route</dt><dd>revision {map.path.revision} · from {map.path.entry_label}</dd></div>
          <div><dt>Set aside</dt><dd>{map.path.bypassed} bypassed · {map.path.checked} checked · {map.path.refreshers} refreshers{#if map.path.bridges} · {map.path.bridges} bridges{/if}</dd></div>
        </dl>
        <p class="coverage-note">Taught counts a lesson done; mastered needs the checks and retrieval after it to hold. Set-aside and checked topics count toward neither denominator; revising the route never rewrites earlier results.</p>
        {#if legend.length}
          <details class="legend"><summary>What these labels mean</summary>
            <dl>{#each legend as row (row.status)}<div><dt><span class={`badge tone-${row.tone}`}>{row.label}</span></dt><dd>{row.meaning}</dd></div>{/each}</dl>
            {#if onrevise}<p class="coverage-note"><strong>Check out</strong> removes a topic from your route without a lesson, and it stops counting toward your progress. <strong>Include</strong> puts one back. Neither awards credit, and neither changes work you have already done.</p>{/if}
          </details>
        {/if}
      {/if}
      {#if map.bridge_proposals.length && onrevise}
        <section class="bridges" aria-label="Suggested bridge lessons">
          <span class="eyebrow mono">GAP SEEN IN PRACTICE</span>
          <ul>
            {#each map.bridge_proposals as proposal (proposal.topic.id + proposal.before.id)}
              <li>
                <div><strong>{proposal.topic.label}</strong> before more work on <strong>{proposal.before.label}</strong><p>The check on {proposal.before.label} scored {Math.round(proposal.score * 100)}% and depends on {proposal.topic.label}, which was set aside. A short bridge lesson comes next if you accept; your starting point does not change.</p></div>
                <span class="route-actions"><button type="button" class="cta mono-cta" disabled={revising} onclick={() => onrevise?.({ kind: 'accept_bridge', topic: proposal.topic.id, before: proposal.before.id })}>Take the bridge</button><button type="button" class="ghost mono-ghost" disabled={revising} onclick={() => onrevise?.({ kind: 'decline_bridge', topic: proposal.topic.id, before: proposal.before.id })}>Not now</button></span>
              </li>
            {/each}
          </ul>
        </section>
      {/if}
    </div>
    {#if !embedded && onclose}<button type="button" class="close" onclick={onclose} aria-label="Close curriculum map">
      <X size={15} />
    </button>{/if}
  </header>

  <label class="search"><Search size={15} /><input type="search" aria-label="Search curriculum topics" placeholder="Find a topic or outcome…" bind:value={query} /><span class="mono">{matches.length} topics</span></label>
  {#if !matches.length}<p class="empty" role="status">No topics match your search.</p>{/if}
  <div class="phase-stack">
    {#each phases.filter(p => conceptsFor(p.id).length) as phase (phase.id)}
      {@const concepts = conceptsFor(phase.id)}
      {@const done = concepts.filter((concept) => complete(concept.mastery_state)).length}
      {@const taught = concepts.filter((concept) => concept.mastery_state !== 'unseen').length}
      <section class:current={map.current_phase === phase.id} class:folded={isFolded(phase.id)} class="phase" aria-label={phase.label}>
        <div class="phase-head">
          <button type="button" class="phase-fold" aria-expanded={!isFolded(phase.id)} aria-controls={`${uid}-${phase.id}`} onclick={() => toggle(phase.id)}>
            <span class="chevron" aria-hidden="true"><ChevronDown size={15} /></span>
            <strong>{phase.label}</strong>
            <span class="count mono" title={`${taught} taught · ${done} mastered · ${concepts.length} topics`}>{taught}/{concepts.length}{#if done} · {done} mastered{/if}</span>
            {#if map.current_phase === phase.id}<small class="you-are-here">current phase</small>{/if}
            {#if isFolded(phase.id)}<small class="folded-note mono">{concepts.length} {concepts.length === 1 ? 'topic' : 'topics'} folded away</small>{/if}
          </button>
          {#if onchallenge && map.path && (map.challenges_available ?? true) && phase.id !== 'elective' && concepts.some((concept) => concept.required)}<button type="button" class="ghost mono-ghost challenge-button" disabled={revising} onclick={() => onchallenge?.(phase.id, phase.label)}>Unit challenge</button>{/if}
        </div>
        <ol id={`${uid}-${phase.id}`} hidden={isFolded(phase.id)}>
          {#each concepts as concept (concept.slug)}
            <li class:complete={complete(concept.mastery_state)} class:elective={!concept.core}>
              <span class="state">
                {#if complete(concept.mastery_state)}
                  <CheckCircle2 size={13} />
                {:else if concept.mastery_state === 'unseen'}
                  <Circle size={13} />
                {:else}
                  <Clock3 size={13} />
                {/if}
              </span>
              <div>
                <strong>{concept.title}</strong>
                <p>{concept.learner_outcome}</p>
                <span class="meta mono">
                  <span class={`badge tone-${BADGES[concept.path_status]?.tone ?? 'faint'}`} title={BADGES[concept.path_status]?.meaning ?? ''}>{BADGES[concept.path_status]?.label ?? concept.path_status}</span>
                  {#if concept.required}<span class="badge tone-accent">required</span>{/if}
                  {concept.mastery_state === 'unseen' ? 'not taught yet' : concept.mastery_state}
                  {#if concept.prerequisites.length}
                    · <Link2 size={10} /> {prerequisiteTitles(concept.prerequisites)}
                  {/if}
                </span>
                {#if onrevise && map.path}
                  <span class="route-actions">
                    {#if canBypass(concept.path_status)}<button type="button" class="ghost mono-ghost" disabled={revising} onclick={() => onrevise?.({ kind: 'bypass', topics: [concept.slug] })}>Check out</button>{/if}
                    {#if canInclude(concept.path_status)}<button type="button" class="ghost mono-ghost" disabled={revising} onclick={() => onrevise?.({ kind: 'include', topics: [concept.slug] })}>Include</button>{/if}
                  </span>
                {/if}
              </div>
            </li>
          {/each}
        </ol>
      </section>
    {/each}
  </div>
</section>

<style>
  .coverage { display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: 8px 16px; margin: 12px 0 0; font-size: 10px; }
  .coverage div { display: grid; gap: 2px; } .coverage dt { color: var(--faint); letter-spacing: 0.08em; text-transform: uppercase; font-size: 9px; } .coverage dd { margin: 0; color: var(--fg); }
  .coverage-note { margin-top: 8px !important; font-size: 11px !important; color: var(--faint) !important; }
  .legend { margin-top: 10px; } .legend summary { cursor: pointer; font-size: 11px; color: var(--violet-fg); }
  .legend dl { display: grid; gap: 7px; margin: 9px 0 4px; } .legend dl > div { display: grid; grid-template-columns: 190px 1fr; gap: 10px; align-items: baseline; }
  .legend dt { margin: 0; } .legend dd { margin: 0; color: var(--muted); font-size: 11px; line-height: 1.55; }
  @media (max-width: 620px) { .legend dl > div { grid-template-columns: 1fr; gap: 2px; } }
  .badge { display: inline-block; border: 1px solid var(--node-border); border-radius: var(--radius-detail); padding: 1px 6px; margin-right: 6px; font-size: 9px; letter-spacing: 0.06em; text-transform: uppercase; }
  .badge.tone-ok { color: var(--green); border-color: var(--green); } .badge.tone-teal { color: var(--teal-fg); border-color: var(--teal-fg); } .badge.tone-warn { color: var(--led-warn); border-color: var(--led-warn); }
  .badge.tone-accent { color: var(--accent); border-color: var(--accent); } .badge.tone-muted { color: var(--muted); } .badge.tone-faint { color: var(--faint); }
  .route-actions { display: flex; gap: 6px; margin-top: 6px; } .route-actions button { padding: 3px 8px; font-size: 9px; }
  .challenge-button { flex: none; margin: 0; padding: 4px 10px; font-size: 9px; }
  .bridges { margin-top: 14px; border: 1px dashed var(--led-warn); border-radius: var(--radius-panel); padding: 12px 14px; } .bridges ul { list-style: none; padding: 0; margin: 8px 0 0; display: grid; gap: 10px; } .bridges li { display: flex; justify-content: space-between; gap: 14px; align-items: flex-start; } .bridges li p { margin: 4px 0 0; font-size: 12px; color: var(--muted); line-height: 1.5; } .bridges .route-actions { flex: none; margin-top: 0; }
  .curriculum-map.embedded { padding: 0; border: 0; background: none; }
  .search { display: flex; align-items: center; gap: 10px; border: 1px solid var(--node-border); border-radius: var(--radius-control); padding: 0 12px; background: var(--bg); color: var(--muted); margin-bottom: 20px; } .search:focus-within { outline: 1px solid var(--accent); } .search input { width: 100%; min-width: 0; border: 0; outline: none; background: none; color: var(--fg); font-size: 13px; padding: 12px 0; } .search span { flex-shrink: 0; font-size: 10px; } .empty { color: var(--muted); font-size: 13px; }
  .curriculum-map {
    grid-column: 1 / -1;
    border: 1px solid var(--border);
    border-radius: var(--radius-panel);
    background: var(--surface);
    padding: 16px;
  }
  header {
    display: flex;
    justify-content: space-between;
    gap: 18px;
    align-items: flex-start;
    margin-bottom: 16px;
  }
  h3 {
    margin: 4px 0 6px;
    color: var(--fg);
    font-size: 18px;
  }
  header p {
    margin: 0;
    max-width: 920px;
    color: var(--muted);
    line-height: 1.55;
    font-size: 14px;
  }
  .eyebrow,
  .meta,
  .phase-head span,
  .you-are-here {
    font-size: 11px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }
  .eyebrow,
  .you-are-here {
    color: var(--ok-fg);
  }
  .close {
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    border: 1px solid var(--border);
    border-radius: var(--radius-panel);
    background: transparent;
    color: var(--muted);
  }
  .close:focus-visible { outline: 2px solid var(--accent); outline-offset: 3px; }
  /* One stage after another across the full width; inside each, the topics as a grid of boxes. */
  .phase-stack {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding-bottom: 6px;
  }
  .phase {
    border: 1px solid var(--border);
    border-radius: var(--radius-panel);
    background: var(--surface-2);
    padding: 10px 12px 12px;
  }
  .phase.current {
    border-color: var(--ok-fg);
  }
  .phase.folded { padding-bottom: 10px; }
  .phase-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    color: var(--fg);
    font-size: 14px;
  }
  .phase-fold {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
    padding: 4px 8px 4px 2px;
    border: 0;
    border-radius: var(--radius-detail);
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .phase-fold:hover { background: var(--surface); }
  .phase-fold:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
  .chevron { display: inline-grid; place-items: center; color: var(--muted); transition: transform 0.18s ease; }
  .phase.folded .chevron { transform: rotate(-90deg); }
  .phase-head .count, .folded-note {
    color: var(--muted);
  }
  .you-are-here {
    margin-left: 4px;
  }
  .folded-note { font-size: 10px; letter-spacing: 0.04em; text-transform: none; color: var(--faint); }
  ol {
    list-style: none;
    padding: 0;
    margin: 10px 0 0;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(min(100%, 280px), 1fr));
    gap: 8px;
  }
  ol[hidden] { display: none; }
  li {
    display: grid;
    grid-template-columns: 16px 1fr;
    gap: 8px;
    padding: 10px 12px;
    border: 1px solid var(--node-border);
    border-left: 2px solid var(--border);
    border-radius: var(--radius-control);
    background: var(--surface);
  }
  li.complete {
    border-left-color: var(--ok-fg);
  }
  li.elective {
    border-left-style: dashed;
  }
  .state {
    color: var(--muted);
    padding-top: 1px;
  }
  li.complete .state {
    color: var(--ok-fg);
  }
  li strong {
    display: block;
    color: var(--fg);
    font-size: 14px;
    line-height: 1.35;
  }
  li p {
    color: var(--muted);
    font-size: 11px;
    line-height: 1.45;
    margin: 4px 0 6px;
  }
  .meta {
    color: var(--muted);
    display: block;
    text-transform: none;
    letter-spacing: normal;
    font-family: var(--font-body);
    font-size: 12px;
    line-height: 1.5;
  }
</style>
