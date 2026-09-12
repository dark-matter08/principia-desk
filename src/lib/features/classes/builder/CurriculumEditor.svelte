<script lang="ts">
  /**
   * The course as the learner edits it: the header that tells the tutor what
   * the course is and how to teach it, the four stages in the course's own
   * words, and the topics grouped by stage. Validation runs as they type;
   * what the desk objects to is listed beside the rail and on each card.
   */
  import { untrack } from 'svelte';
  import type { CourseDraft, DraftIssue, DraftTopic } from '../../../ipc';
  import { blankTopic, STAGES, validateDraft } from '../../../custom-preview';
  import { autosize } from '../../../actions/autosize';
  import TopicCard from './TopicCard.svelte';
  import { Plus, X, CircleAlert, CircleCheck, ChevronDown, ArrowDown, PenLine } from 'lucide-svelte';

  let {
    draft = $bindable(),
    nativeIssues = [],
    focus = null,
    onchange,
  }: {
    draft: CourseDraft;
    /** What the desk said after the last save; the local mirror runs between saves. */
    nativeIssues?: DraftIssue[];
    /** A topic to open, scroll to and light up; `at` makes the same slug jump again. */
    focus?: { slug: string; at: number } | null;
    onchange: () => void;
  } = $props();

  /**
   * The four stages drawn as what they are: a plinth being laid, a crank
   * turning a rod, a works with its roofline, two threads meeting in one.
   */
  const STAGE_MARKS: Record<string, string> = {
    foundations: '<path d="M3 20h18" /><path d="M6 20v-4h12v4" /><path d="M8.5 16v-4h7v4" /><path d="M11 12V8h2v4" /><path d="M9 5h6" />',
    mechanisms: '<circle cx="7.5" cy="14.5" r="4.5" /><circle cx="7.5" cy="14.5" r="1" /><path d="M10.5 11.5 16 6" /><rect x="15" y="3" width="6" height="6" rx="1.2" /><path d="M7.5 5.5v2.5" /><path d="M2.5 14.5H1" />',
    production: '<path d="M2 21h20" /><path d="M4 21V11l5 3v-3l5 3v-3l6 3v7" /><path d="M8 18h2" /><path d="M13 18h2" /><path d="M17 4v4" /><path d="M15 4h4" />',
    synthesis: '<path d="M3 5c6 0 6 14 12 14h5" /><path d="M3 19c6 0 6-14 12-14h5" /><circle cx="12" cy="12" r="1.6" fill="currentColor" stroke="none" /><path d="m18 3 2 2-2 2" /><path d="m18 17 2 2-2 2" />',
  };
  const STAGE_HINTS: Record<string, string> = { foundations: 'the ideas and the first observations', mechanisms: 'how it works underneath', production: 'using it under real constraints', synthesis: 'the capstone that produces the outcome' };
  const issues = $derived(validateDraft(draft));
  const headerIssues = $derived(issues.filter((issue) => !issue.at.startsWith('topics/')));
  const topicIssues = (slug: string) => issues.filter((issue) => issue.at === `topics/${slug}`);
  const byStage = $derived(
    [...STAGES.map((stage) => ({ ...stage, label: draft.entry_points.find((e) => e.id === stage.id)?.label ?? stage.label })), { id: 'elective', label: 'Electives' }].map((stage) => ({
      ...stage,
      topics: draft.topics.map((topic, index) => ({ topic, index })).filter(({ topic }) => topic.curriculum.phase === stage.id),
    })),
  );
  let openSlug = $state<string | null>(null);
  let hostInput = $state('');
  /** The stage whose name is being edited in its tile; the others show it clamped. */
  let editingStage = $state<string | null>(null);
  /** Stage sections folded to their head. A jump into a folded stage unfolds it. */
  let folded = $state<Record<string, boolean>>({});
  /** The course header folded to one line, so the topics are a shorter scroll away. */
  let courseFolded = $state(false);
  /** A rail issue on a header field unfolds the header before scrolling to the field. */
  function jumpToField(at: string) {
    courseFolded = false;
    setTimeout(() => document.getElementById(`field-${at}`)?.scrollIntoView({ block: 'center', behavior: 'smooth' }), 40);
  }
  let flashStage = $state<string | null>(null);
  const countIn = (stage: string) => draft.topics.filter((t) => t.curriculum.phase === stage).length;
  function jumpToStage(stage: string) {
    folded = { ...folded, [stage]: false };
    setTimeout(() => {
      document.getElementById(`stage-${stage}`)?.scrollIntoView({ block: 'start', behavior: 'smooth' });
      flashStage = stage;
      setTimeout(() => (flashStage = null), 1600);
    }, 40);
  }
  /** The card a jump landed on, lit until the blink ends. */
  let flashSlug = $state<string | null>(null);
  let flashTimer: ReturnType<typeof setTimeout> | null = null;

  // Only `focus` is a dependency here: `jump` reads and writes the fold and
  // open state, and an effect that tracked those would re-run on its own
  // writes without end, taking the page's reactivity down with it.
  $effect(() => {
    const target = focus;
    if (target) untrack(() => jump(`topics/${target.slug}`));
  });

  function addHost() {
    const host = hostInput.trim().replace(/^https?:\/\//, '').replace(/\/.*$/, '').toLowerCase();
    if (host && !draft.source_hosts.includes(host)) draft.source_hosts = [...draft.source_hosts, host];
    hostInput = '';
    onchange();
  }
  function dropHost(host: string) { draft.source_hosts = draft.source_hosts.filter((h) => h !== host); onchange(); }
  function addTopic(phase: DraftTopic['curriculum']['phase']) {
    const topic = blankTopic(phase);
    topic.slug = `topic-${draft.topics.length + 1}`;
    draft.topics = [...draft.topics, topic];
    openSlug = topic.slug;
    onchange();
  }
  function removeTopic(index: number) {
    const gone = draft.topics[index]?.slug;
    draft.topics = draft.topics.filter((_, i) => i !== index).map((topic) => ({ ...topic, prereqs: topic.prereqs.filter((p) => p !== gone) }));
    onchange();
  }
  function moveTopic(index: number, direction: -1 | 1) {
    const target = index + direction;
    if (target < 0 || target >= draft.topics.length) return;
    const topics = [...draft.topics];
    [topics[index], topics[target]] = [topics[target], topics[index]];
    draft.topics = topics;
    onchange();
  }

  /**
   * Dragging a card by its grip. The drop lands before or after the card
   * under the pointer (its upper or lower half), or at the end of a stage
   * when the pointer is over the stage's empty space; dropping in another
   * stage moves the topic there.
   */
  let dragging = $state<number | null>(null);
  let drop = $state<{ index: number; after: boolean } | { stage: string } | null>(null);
  function dragStart(index: number, event: DragEvent) {
    dragging = index;
    if (event.dataTransfer) { event.dataTransfer.effectAllowed = 'move'; event.dataTransfer.setData('text/plain', String(index)); }
  }
  function dragEnd() { dragging = null; drop = null; }
  function overCard(index: number, event: DragEvent) {
    if (dragging === null || dragging === index) return;
    event.preventDefault();
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    const after = event.clientY > rect.top + rect.height / 2;
    if (!drop || !('index' in drop) || drop.index !== index || drop.after !== after) drop = { index, after };
  }
  function overStage(stage: string, event: DragEvent) {
    if (dragging === null) return;
    event.preventDefault();
    if (folded[stage]) folded = { ...folded, [stage]: false };
    if (!drop || !('stage' in drop) || drop.stage !== stage) drop = { stage };
  }
  function dropHere(stage: string, event: DragEvent) {
    event.preventDefault();
    const from = dragging;
    const target = drop;
    dragEnd();
    if (from === null || !target) return;
    const topics = [...draft.topics];
    const [moved] = topics.splice(from, 1);
    let at = topics.length;
    if ('index' in target) {
      const anchor = target.index > from ? target.index - 1 : target.index;
      at = anchor + (target.after ? 1 : 0);
    } else {
      const last = topics.map((t) => t.curriculum.phase as string).lastIndexOf(stage);
      at = last < 0 ? topics.length : last + 1;
    }
    if (moved.curriculum.phase !== stage) {
      moved.curriculum.phase = stage as DraftTopic['curriculum']['phase'];
      // An elective is off the core route by definition.
      if (stage === 'elective') moved.curriculum.core = false;
    }
    topics.splice(at, 0, moved);
    draft.topics = topics;
    onchange();
  }
  function jump(at: string) {
    const slug = at.replace(/^topics\//, '');
    const phase = draft.topics.find((t) => t.slug === slug)?.curriculum.phase;
    if (phase) folded = { ...folded, [phase]: false };
    openSlug = slug;
    flashSlug = null;
    if (flashTimer) clearTimeout(flashTimer);
    // The card opens first, then the scroll and the blink land on it.
    setTimeout(() => {
      document.getElementById(`topic-${slug}`)?.scrollIntoView({ block: 'start', behavior: 'smooth' });
      flashSlug = slug;
      flashTimer = setTimeout(() => (flashSlug = null), 2300);
    }, 40);
  }
</script>

<div class="editor">
  <aside class="rail" aria-label="What still needs work">
    {#if issues.length}
      <span class="rail-label mono"><CircleAlert size={11} /> {issues.length} {issues.length === 1 ? 'THING' : 'THINGS'} TO FIX</span>
      <ul>
        {#each issues.slice(0, 24) as issue, i (i)}
          <li><button type="button" onclick={() => (issue.at.startsWith('topics/') ? jump(issue.at) : jumpToField(issue.at))}><span class="mono">{issue.at.replace('topics/', '')}</span>{issue.message}</button></li>
        {/each}
        {#if issues.length > 24}<li class="more mono">and {issues.length - 24} more</li>{/if}
      </ul>
    {:else}
      <span class="rail-label mono ok"><CircleCheck size={11} /> READY TO PUBLISH</span>
      <p>Every check passes. {draft.topics.length} topics across four stages.</p>
    {/if}
    {#if nativeIssues.length && !issues.length}
      <p class="native"><CircleAlert size={11} /> The desk still objects: {nativeIssues[0].message}</p>
    {/if}
    <dl class="counts mono">
      <div><dt>topics</dt><dd>{draft.topics.length}</dd></div>
      <div><dt>core</dt><dd>{draft.topics.filter((t) => t.curriculum.core).length}</dd></div>
      <div><dt>sources</dt><dd>{draft.topics.reduce((n, t) => n + t.curriculum.primary_sources.filter(Boolean).length, 0)}</dd></div>
      <div><dt>hosts</dt><dd>{draft.source_hosts.length}</dd></div>
    </dl>
  </aside>

  <div class="sheet">
    <section class="block" class:folded={courseFolded} aria-label="The course">
      <div class="stage-head">
        <button type="button" class="fold" aria-expanded={!courseFolded} aria-controls="course-header-fields" onclick={() => (courseFolded = !courseFolded)}>
          <span class="chevron" class:down={courseFolded} aria-hidden="true"><ChevronDown size={14} /></span>
          <span class="eyebrow mono">THE COURSE{#if courseFolded}<small>{draft.label || 'unnamed'} · {draft.short_code || 'no code'} · {draft.source_hosts.length} {draft.source_hosts.length === 1 ? 'host' : 'hosts'}{#if headerIssues.length} · {headerIssues.length} to fix{/if}</small>{/if}</span>
        </button>
      </div>
      <div id="course-header-fields" class="course-fields" hidden={courseFolded}>
      <div class="grid">
        <label class="field span-2" id="field-label"><span>Class name <small>as it reads in the list</small></span><input value={draft.label} oninput={(e) => { draft.label = e.currentTarget.value; onchange(); }} /></label>
        <label class="field" id="field-short_code"><span>Short code <small>1–4 letters</small></span><input class="mono" maxlength="4" value={draft.short_code} oninput={(e) => { draft.short_code = e.currentTarget.value.toUpperCase(); onchange(); }} /></label>
        <label class="field" id="field-native_label"><span>Field <small>two or three words, lowercase</small></span><input value={draft.native_label} oninput={(e) => { draft.native_label = e.currentTarget.value; onchange(); }} placeholder="systems programming" /></label>
        <label class="field span-2" id="field-title"><span>Full title</span><input value={draft.title} oninput={(e) => { draft.title = e.currentTarget.value; onchange(); }} /></label>
        <label class="field span-2" id="field-summary"><span>Summary <small>one sentence</small></span><textarea use:autosize={{ min: 1, max: 4, value: draft.summary }} value={draft.summary} oninput={(e) => { draft.summary = e.currentTarget.value; onchange(); }}></textarea></label>
        <label class="field span-2" id="field-outcome"><span>Outcome <small>what you will have made and can defend at the end</small></span><textarea use:autosize={{ min: 3, max: 10, value: draft.outcome }} value={draft.outcome} oninput={(e) => { draft.outcome = e.currentTarget.value; onchange(); }}></textarea></label>
        <label class="field span-2" id="field-context"><span>How it should be taught <small>the tutor reads this before every lesson</small></span><textarea use:autosize={{ min: 3, max: 10, value: draft.context }} value={draft.context} oninput={(e) => { draft.context = e.currentTarget.value; onchange(); }}></textarea></label>
        <label class="field span-2" id="field-environment"><span>Working environment</span><textarea use:autosize={{ min: 1, max: 4, value: draft.environment }} value={draft.environment} oninput={(e) => { draft.environment = e.currentTarget.value; onchange(); }} placeholder="A terminal, a text editor and …"></textarea></label>
      </div>
      <div class="field" id="field-source_hosts">
        <span>Documentation hosts <small>every lesson is taught from and cites these</small></span>
        <div class="hosts">
          {#each draft.source_hosts as host (host)}<span class="host mono">{host}<button type="button" aria-label={`Remove ${host}`} onclick={() => dropHost(host)}><X size={11} /></button></span>{/each}
          <input class="mono host-input" placeholder="docs.example.org" bind:value={hostInput} onkeydown={(e) => { if (e.key === 'Enter' || e.key === ',') { e.preventDefault(); addHost(); } }} onblur={addHost} />
        </div>
      </div>
      {#if headerIssues.length}<ul class="issues">{#each headerIssues as issue (issue.at + issue.message)}<li><CircleAlert size={11} /> <span class="mono">{issue.at}</span> {issue.message}</li>{/each}</ul>{/if}
      </div>
    </section>

    <section class="block" aria-label="Stages" id="field-entry_points">
      <span class="eyebrow mono">THE FOUR STAGES <small>in this course's own words</small></span>
      <div class="stages">
        {#each draft.entry_points as entry, i (entry.id)}
          <!-- The tile goes to its stage's topics; the name inside it is edited in place. -->
          <div class="stage-tile" class:editing={editingStage === entry.id} role="group" aria-label={`Stage ${i + 1}, ${entry.id}`}>
            <button type="button" class="stage-go" aria-label={`Go to the ${entry.label || entry.id} topics`} title="Go to this stage's topics" onclick={() => jumpToStage(entry.id)}>
              <span class="stage-mark" aria-hidden="true">
                <svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">{@html STAGE_MARKS[entry.id] ?? STAGE_MARKS.foundations}</svg>
              </span>
            </button>
            <span class="stage-no mono">{String(i + 1).padStart(2, '0')}</span>
            <span class="stage-id mono">{entry.id}</span>
            {#if editingStage === entry.id}
              <!-- svelte-ignore a11y_autofocus -->
              <textarea autofocus use:autosize={{ min: 2, max: 3, value: entry.label }} value={entry.label} oninput={(e) => { draft.entry_points[i].label = e.currentTarget.value; onchange(); }} onblur={() => (editingStage = null)} onkeydown={(e) => { if (e.key === 'Enter' || e.key === 'Escape') { e.preventDefault(); editingStage = null; } }} placeholder={STAGE_HINTS[entry.id]}></textarea>
            {:else}
              <button type="button" class="stage-label" class:placeholder={!entry.label.trim()} title={entry.label.trim() ? `${entry.label}\nclick to rename` : 'click to name this stage'} onclick={() => (editingStage = entry.id)}>{entry.label.trim() || STAGE_HINTS[entry.id]}</button>
            {/if}
            <span class="stage-foot mono"><span>{countIn(entry.id)} {countIn(entry.id) === 1 ? 'topic' : 'topics'}</span><PenLine size={10} aria-hidden="true" /></span>
          </div>
        {/each}
      </div>
    </section>

    {#each byStage as stage (stage.id)}
      <section class="block stage" class:folded={folded[stage.id]} class:flash={flashStage === stage.id} aria-label={stage.label} id={`stage-${stage.id}`}>
        <!-- A card dragged over a folded stage's head unfolds it, so it can land inside. -->
        <div class="stage-head" role="presentation" ondragover={() => { if (dragging !== null && folded[stage.id]) folded = { ...folded, [stage.id]: false }; }}>
          <button type="button" class="fold" aria-expanded={!folded[stage.id]} aria-controls={`stage-${stage.id}-topics`} onclick={() => (folded = { ...folded, [stage.id]: !folded[stage.id] })}>
            <span class="chevron" aria-hidden="true"><ChevronDown size={14} /></span>
            <span class="eyebrow mono">{stage.id === 'elective' ? 'ELECTIVES' : `STAGE · ${stage.label.toUpperCase()}`} <b>{stage.topics.length}</b>{#if folded[stage.id]}<small>folded</small>{/if}</span>
          </button>
          <button type="button" class="ghost mono-ghost small" onclick={() => { folded = { ...folded, [stage.id]: false }; addTopic(stage.id as DraftTopic['curriculum']['phase']); }}><Plus size={12} />Add topic</button>
        </div>
        <div class="topics" id={`stage-${stage.id}-topics`} hidden={!!folded[stage.id] && dragging === null} class:receiving={dragging !== null} class:landing={!!drop && 'stage' in drop && drop.stage === stage.id} role="list" ondragover={(event) => overStage(stage.id, event)} ondrop={(event) => dropHere(stage.id, event)}>
          {#each stage.topics as { topic, index } (index)}
            <div class="slot" role="listitem" class:before={!!drop && 'index' in drop && drop.index === index && !drop.after} class:after={!!drop && 'index' in drop && drop.index === index && drop.after} ondragover={(event) => { event.stopPropagation(); overCard(index, event); }} ondrop={(event) => { event.stopPropagation(); dropHere(stage.id, event); }}>
              <TopicCard
                bind:topic={draft.topics[index]}
                {index}
                stages={draft.entry_points}
                others={draft.topics}
                issues={topicIssues(topic.slug)}
                open={openSlug === topic.slug}
                flash={flashSlug === topic.slug}
                dragging={dragging === index}
                onremove={() => removeTopic(index)}
                onmove={(direction) => moveTopic(index, direction)}
                onchange={() => { openSlug = draft.topics[index]?.slug ?? openSlug; onchange(); }}
                ondragstart={(event) => dragStart(index, event)}
                ondragend={dragEnd}
              />
            </div>
          {/each}
          {#if !stage.topics.length}
            <p class="empty">{dragging !== null ? 'Drop here to move the topic into this stage.' : stage.id === 'elective' ? 'No electives. Optional.' : 'No topics in this stage yet; it needs at least one core topic.'}</p>
          {/if}
        </div>
      </section>
    {/each}
  </div>
</div>

<style>
  .editor { display: grid; grid-template-columns: 220px minmax(0, 1fr); gap: 18px; align-items: start; }
  .rail { position: sticky; top: 8px; display: flex; flex-direction: column; gap: 10px; padding: 12px; border: 1px solid var(--node-border); border-radius: var(--radius-panel); background: var(--node-bg); }
  .rail-label { display: inline-flex; align-items: center; gap: 6px; font-size: 9px; letter-spacing: 1.2px; color: var(--warn-fg); } .rail-label.ok { color: var(--ok-fg); }
  .rail ul { display: flex; flex-direction: column; gap: 3px; margin: 0; padding: 0; list-style: none; max-height: 40vh; overflow-y: auto; }
  .rail li button { display: flex; flex-direction: column; gap: 1px; width: 100%; padding: 6px 8px; border: 1px solid transparent; border-radius: var(--radius-detail); background: transparent; color: var(--muted); font: 10.5px/1.4 var(--font-body); text-align: left; cursor: pointer; }
  .rail li button:hover { background: var(--surface); border-color: var(--node-border); color: var(--fg); }
  .rail li button .mono { font-size: 8.5px; letter-spacing: 0.5px; color: var(--accent); }
  .rail .more { font-size: 9px; color: var(--faint); padding: 4px 8px; }
  .rail p { margin: 0; font-size: 11px; line-height: 1.5; color: var(--muted); }
  .native { display: flex; gap: 6px; color: var(--warn-fg) !important; }
  .counts { display: grid; grid-template-columns: 1fr 1fr; gap: 6px; margin: 4px 0 0; padding-top: 10px; border-top: 1px dashed var(--node-divider); }
  .counts div { display: flex; justify-content: space-between; font-size: 9.5px; color: var(--muted); } .counts dd { margin: 0; color: var(--fg); }
  .sheet { display: flex; flex-direction: column; gap: 14px; min-width: 0; }
  .block { display: flex; flex-direction: column; gap: 12px; padding: 16px; border: 1px solid var(--node-border); border-radius: var(--radius-panel); background: var(--node-bg); }
  .eyebrow { font-size: 9px; letter-spacing: 1.3px; color: var(--accent); } .eyebrow small { margin-left: 8px; color: var(--faint); letter-spacing: 0.4px; } .eyebrow b { margin-left: 8px; color: var(--muted); }
  .grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px 16px; } .span-2 { grid-column: 1 / -1; }
  .field { display: flex; flex-direction: column; gap: 6px; min-width: 0; } .field > span { font-size: 11px; color: var(--muted); } .field > span small { margin-left: 6px; color: var(--faint); font-size: 10px; }
  input, textarea { width: 100%; padding: 9px 11px; border: 1px solid var(--node-border); border-radius: var(--radius-control); background: var(--bg); color: var(--fg); font: 12.5px/1.5 var(--font-body); }
  input.mono { font-family: var(--font-mono); font-size: 11.5px; } textarea { display: block; }
  input:focus, textarea:focus { outline: none; border-color: var(--accent); }
  .hosts { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; padding: 6px; border: 1px solid var(--node-border); border-radius: var(--radius-control); background: var(--bg); }
  .host { display: inline-flex; align-items: center; gap: 5px; padding: 4px 6px 4px 9px; border: 1px solid var(--node-border); border-radius: 6px; background: var(--surface); font-size: 10.5px; color: var(--fg); }
  .host button { display: grid; place-items: center; width: 16px; height: 16px; border: 0; border-radius: 3px; background: transparent; color: var(--faint); cursor: pointer; } .host button:hover { color: var(--led-err); }
  .host-input { flex: 1; min-width: 160px; border: 0; background: transparent; padding: 4px 6px; }
  .issues { display: flex; flex-direction: column; gap: 4px; margin: 0; padding: 10px 12px; list-style: none; border-left: 2px solid var(--warn-fg); background: var(--surface); }
  .issues li { display: flex; align-items: center; gap: 7px; font-size: 11px; color: var(--warn-fg); } .issues li .mono { color: var(--muted); }
  /* Four tiles, one a stage: its mark, its number top right, its id, and its name in the course's words.
     Square while the columns are narrow; once they widen the tile turns landscape, the mark beside the words. */
  .stages { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 10px; container: stages / inline-size; }
  /* The width is the column's; the square comes from the height following it, never the other way round. */
  .stage-tile { position: relative; display: flex; flex-direction: column; gap: 4px; width: 100%; min-width: 0; aspect-ratio: 1 / 1; max-height: 200px; padding: 12px 12px 10px; border: 1px solid var(--node-border); border-radius: var(--radius-panel); background: linear-gradient(180deg, var(--surface), var(--bg)); overflow: hidden; transition: border-color 0.15s, box-shadow 0.15s; cursor: text; }
  .stage-tile:hover { border-color: color-mix(in srgb, var(--accent) 40%, var(--node-border)); }
  .stage-tile:focus-within { border-color: var(--accent); box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 14%, transparent); }
  .stage-mark { display: grid; place-items: center; width: 38px; height: 38px; border: 1px solid color-mix(in srgb, var(--accent) 30%, var(--node-border)); border-radius: 10px; background: color-mix(in srgb, var(--accent) 10%, transparent); color: var(--accent); }
  .stage-no { position: absolute; top: 12px; right: 12px; font-size: 9.5px; letter-spacing: 1px; color: var(--faint); }
  .stage-id { font-size: 9px; letter-spacing: 1.2px; text-transform: uppercase; color: var(--faint); }
  .stage-tile { cursor: default; }
  .stage-go { display: flex; align-items: flex-start; margin-bottom: 4px; padding: 0; border: 0; background: transparent; color: inherit; text-align: left; cursor: pointer; }
  .stage-go:hover .stage-mark { border-color: var(--accent); background: color-mix(in srgb, var(--accent) 18%, transparent); }
  .stage-go:focus-visible { outline: none; } .stage-go:focus-visible .stage-mark { box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 22%, transparent); }
  /* The name: two lines at most, then an ellipsis; the full text is a click away. */
  .stage-label { display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; margin-top: 2px; padding: 4px 0 0; border: 0; background: transparent; color: var(--fg); font: 13.5px/1.4 var(--font-body); text-align: left; cursor: text; }
  .stage-label:hover { color: var(--accent); } .stage-label.placeholder { color: var(--faint); font-style: italic; }
  .stage-tile textarea { flex: 1; min-height: 0; margin-top: 2px; padding: 4px 0 0; border: 0; border-radius: 0; background: transparent; font: 13.5px/1.4 var(--font-body); color: var(--fg); }
  .stage-tile textarea::placeholder { color: var(--faint); font-style: italic; }
  .stage-tile textarea:focus { border: 0; }
  .stage-foot { display: flex; align-items: center; justify-content: space-between; margin-top: auto; padding-top: 6px; font-size: 9px; letter-spacing: 0.6px; color: var(--faint); }
  /* Four columns and three gaps: a row of 830px gives 200px tiles, 670px gives 160px, 1150px gives 280px. */
  @container stages (max-width: 830px) { .stage-label, .stage-tile textarea { font-size: 12px; } .stage-mark { width: 32px; height: 32px; } }
  @container stages (max-width: 670px) { .stage-label, .stage-tile textarea { font-size: 11px; } .stage-id { font-size: 8px; } .stage-mark { width: 28px; height: 28px; } .stage-mark svg { width: 18px; height: 18px; } }
  /* Room to spare: the tile drops from a square to four by three, then to landscape with the mark beside the words. */
  @container stages (min-width: 710px) { .stage-tile { aspect-ratio: 4 / 3; } }
  @container stages (min-width: 1150px) {
    .stage-tile { display: grid; grid-template-columns: auto minmax(0, 1fr); grid-template-areas: 'mark id' 'mark name' 'mark foot'; grid-template-rows: auto minmax(0, 1fr) auto; column-gap: 14px; row-gap: 2px; aspect-ratio: auto; min-height: 118px; max-height: none; padding: 14px 16px 12px; }
    .stage-go { grid-area: mark; margin: 0; } .stage-mark { width: 44px; height: 44px; }
    .stage-id { grid-area: id; padding-right: 32px; } .stage-label, .stage-tile textarea { grid-area: name; margin-top: 0; } .stage-foot { grid-area: foot; margin-top: 6px; }
  }
  .stage-head { display: flex; align-items: center; justify-content: space-between; gap: 10px; }
  .fold { display: inline-flex; align-items: center; gap: 8px; min-width: 0; padding: 4px 6px 4px 2px; border: 0; border-radius: var(--radius-detail); background: transparent; color: inherit; text-align: left; cursor: pointer; }
  .fold:hover { background: var(--surface); }
  .chevron { display: inline-grid; place-items: center; color: var(--muted); transition: transform 0.18s ease; } .stage.folded .chevron { transform: rotate(-90deg); }
  .fold small { margin-left: 8px; color: var(--faint); letter-spacing: 0.4px; text-transform: none; }
  .stage.folded, .block.folded { gap: 0; }
  .course-fields { display: flex; flex-direction: column; gap: 12px; } .course-fields[hidden] { display: none; }
  .chevron.down { transform: rotate(-90deg); }
  .stage.flash { animation: stage-flash 0.8s ease-in-out 2; }
  @keyframes stage-flash { 50% { border-color: var(--accent); box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 28%, transparent); } }
  .topics { display: flex; flex-direction: column; gap: 8px; border-radius: var(--radius-control); transition: box-shadow 0.15s; }
  /* Folded away: the class's display would otherwise win over the hidden attribute. */
  .topics[hidden] { display: none; }
  /* While a card is in the air, every stage is a place it can land. */
  .topics.receiving { min-height: 44px; box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent) 25%, transparent); }
  .topics.landing { box-shadow: inset 0 0 0 1px var(--accent), inset 0 0 18px color-mix(in srgb, var(--accent) 12%, transparent); }
  .slot { position: relative; }
  /* The insertion line where the card would land. */
  .slot::before, .slot::after { content: ''; position: absolute; left: 6px; right: 6px; height: 2px; border-radius: 2px; background: var(--accent); box-shadow: 0 0 8px var(--accent); opacity: 0; pointer-events: none; transition: opacity 0.1s; }
  .slot::before { top: -5px; } .slot::after { bottom: -5px; }
  .slot.before::before, .slot.after::after { opacity: 1; }
  .empty { margin: 0; padding: 6px 4px; font-size: 11px; color: var(--faint); }
  @media (max-width: 900px) { .editor { grid-template-columns: 1fr; } .rail { position: static; } .stages { grid-template-columns: repeat(2, minmax(0, 1fr)); } .stage-tile { aspect-ratio: auto; } }
  @media (max-width: 640px) { .grid { grid-template-columns: 1fr; } }
</style>
