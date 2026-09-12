<script lang="ts">
  /**
   * One topic of a class being built: its title and stage on the closed
   * card, every field of its brief when open, and the validator's
   * objections beside the field they name.
   */
  import type { DraftIssue, DraftTopic } from '../../../ipc';
  import { slugify } from '../../../custom-preview';
  import { autosize } from '../../../actions/autosize';
  import Dropdown from '../../../components/Dropdown.svelte';
  import { ChevronDown, ChevronUp, Trash2, ArrowUp, ArrowDown, Plus, X, CircleAlert, GripVertical } from 'lucide-svelte';

  let {
    topic = $bindable(),
    index,
    stages,
    others,
    issues = [],
    open = $bindable(false),
    flash = false,
    dragging = false,
    onremove,
    onmove,
    onchange,
    ondragstart,
    ondragend,
  }: {
    topic: DraftTopic;
    index: number;
    stages: { id: string; label: string }[];
    /** Every other topic, for prerequisites. */
    others: DraftTopic[];
    issues?: DraftIssue[];
    open?: boolean;
    /** Lit for a moment after a jump, so the card that was meant is the one seen. */
    flash?: boolean;
    /** Being dragged to a new place; the card leaves a faint outline behind. */
    dragging?: boolean;
    onremove: () => void;
    onmove: (direction: -1 | 1) => void;
    onchange: () => void;
    /** The grip was taken; the parent runs the drag. */
    ondragstart?: (event: DragEvent) => void;
    ondragend?: () => void;
  } = $props();

  /** The card is only draggable from its grip, so text in its fields stays selectable. */
  let armed = $state(false);

  const tier = (phase: string) => ({ foundations: 0, mechanisms: 1, production: 2 }[phase] ?? 3);
  /** Topics that may be prerequisites: same or earlier stage, not itself. */
  const candidates = $derived(others.filter((other) => other.slug !== topic.slug && other.slug && tier(other.curriculum.phase) <= tier(topic.curriculum.phase)));
  let slugTouched = $state(false);

  function titled(value: string) {
    topic.title = value;
    if (!slugTouched) topic.slug = slugify(value);
    onchange();
  }
  function togglePrereq(slug: string) {
    topic.prereqs = topic.prereqs.includes(slug) ? topic.prereqs.filter((p) => p !== slug) : [...topic.prereqs, slug];
    onchange();
  }
  function setList(field: 'mechanisms' | 'misconceptions' | 'primary_sources', i: number, value: string) {
    topic.curriculum[field][i] = value;
    onchange();
  }
  function addTo(field: 'mechanisms' | 'misconceptions' | 'primary_sources') {
    topic.curriculum[field] = [...topic.curriculum[field], ''];
    onchange();
  }
  function dropFrom(field: 'mechanisms' | 'misconceptions' | 'primary_sources', i: number) {
    topic.curriculum[field] = topic.curriculum[field].filter((_, j) => j !== i);
    onchange();
  }
  const phases = $derived([...stages, { id: 'elective', label: 'Elective' }].map((stage) => ({ value: stage.id, label: stage.label, description: stage.id === 'elective' ? 'off the core route' : stage.id })));
</script>

<article
  class="topic"
  class:open
  class:flawed={issues.length > 0}
  class:flash
  class:dragging
  id={`topic-${topic.slug || index}`}
  draggable={armed}
  ondragstart={(event) => { if (!armed) { event.preventDefault(); return; } ondragstart?.(event); }}
  ondragend={() => { armed = false; ondragend?.(); }}
>
  <header class="topic-head">
    <span class="grip" role="button" tabindex="-1" aria-label="Drag to reorder" title="Drag to reorder" onmousedown={() => (armed = true)} onmouseup={() => (armed = false)} onmouseleave={() => { if (!dragging) armed = false; }}><GripVertical size={13} /></span>
    <button type="button" class="disclosure" aria-expanded={open} onclick={() => (open = !open)}>
      <span class="no mono">{String(index + 1).padStart(2, '0')}</span>
      <span class="head-text">
        <strong>{topic.title.trim() || 'Untitled topic'}</strong>
        <small class="mono">{topic.curriculum.core ? 'core' : 'elective'} · {topic.category || 'no category'}{#if topic.prereqs.length} · after {topic.prereqs.join(', ')}{/if}</small>
      </span>
      {#if issues.length}<span class="flag mono" title={issues.map((i) => i.message).join('\n')}><CircleAlert size={11} /> {issues.length}</span>{/if}
      {#if open}<ChevronUp size={14} />{:else}<ChevronDown size={14} />{/if}
    </button>
    <span class="head-actions">
      <button type="button" class="icon" aria-label="Move topic up" onclick={() => onmove(-1)}><ArrowUp size={13} /></button>
      <button type="button" class="icon" aria-label="Move topic down" onclick={() => onmove(1)}><ArrowDown size={13} /></button>
      <button type="button" class="icon danger" aria-label={`Remove ${topic.title || 'topic'}`} onclick={onremove}><Trash2 size={13} /></button>
    </span>
  </header>

  {#if open}
    <div class="body">
      {#if issues.length}
        <ul class="issues" aria-label="What the validator objects to">
          {#each issues as issue (issue.message)}<li><CircleAlert size={11} /> {issue.message}</li>{/each}
        </ul>
      {/if}
      <div class="grid">
        <label class="field span-2"><span>Title</span><input value={topic.title} oninput={(e) => titled(e.currentTarget.value)} placeholder="What the lesson is called" /></label>
        <label class="field"><span>Slug <small>the topic's id</small></span><input class="mono" value={topic.slug} oninput={(e) => { slugTouched = true; topic.slug = slugify(e.currentTarget.value); onchange(); }} /></label>
        <label class="field"><span>Category <small>groups related topics</small></span><input value={topic.category} oninput={(e) => { topic.category = e.currentTarget.value; onchange(); }} placeholder="fundamentals" /></label>
        <div class="field"><span>Stage</span><Dropdown label="Stage" hideLabel value={topic.curriculum.phase} options={phases} onchange={(value) => { topic.curriculum.phase = value as DraftTopic['curriculum']['phase']; onchange(); }} /></div>
        <label class="field check"><input type="checkbox" checked={topic.curriculum.core} onchange={(e) => { topic.curriculum.core = e.currentTarget.checked; onchange(); }} /><span>Core topic <small>on every route through this stage</small></span></label>
      </div>

      <div class="field">
        <span>Builds on <small>prerequisites, from the same or an earlier stage</small></span>
        {#if candidates.length}
          <div class="chips">
            {#each candidates as other (other.slug)}
              <button type="button" class="chip" class:on={topic.prereqs.includes(other.slug)} aria-pressed={topic.prereqs.includes(other.slug)} onclick={() => togglePrereq(other.slug)}>{other.title.trim() || other.slug}</button>
            {/each}
          </div>
        {:else}<p class="hint">No earlier topic yet.</p>{/if}
      </div>

      <label class="field"><span>Learner outcome <small>what you can do after the lesson, observable, 8+ words</small></span><textarea use:autosize={{ min: 2, max: 8, value: topic.curriculum.learner_outcome }} value={topic.curriculum.learner_outcome} oninput={(e) => { topic.curriculum.learner_outcome = e.currentTarget.value; onchange(); }}></textarea></label>
      <div class="field">
        <span>Mechanisms <small>at least two, named</small></span>
        {#each topic.curriculum.mechanisms as item, i (i)}
          <div class="row"><input value={item} oninput={(e) => setList('mechanisms', i, e.currentTarget.value)} placeholder="a mechanism the lesson explains" /><button type="button" class="icon" aria-label="Remove mechanism" onclick={() => dropFrom('mechanisms', i)}><X size={12} /></button></div>
        {/each}
        <button type="button" class="text" onclick={() => addTo('mechanisms')}><Plus size={11} /> add a mechanism</button>
      </div>
      <label class="field"><span>Production scenario <small>where this matters at work, 8+ words</small></span><textarea use:autosize={{ min: 2, max: 8, value: topic.curriculum.production_scenario }} value={topic.curriculum.production_scenario} oninput={(e) => { topic.curriculum.production_scenario = e.currentTarget.value; onchange(); }}></textarea></label>
      <div class="field">
        <span>Misconceptions <small>at least one</small></span>
        {#each topic.curriculum.misconceptions as item, i (i)}
          <div class="row"><input value={item} oninput={(e) => setList('misconceptions', i, e.currentTarget.value)} placeholder="a common wrong belief" /><button type="button" class="icon" aria-label="Remove misconception" onclick={() => dropFrom('misconceptions', i)}><X size={12} /></button></div>
        {/each}
        <button type="button" class="text" onclick={() => addTo('misconceptions')}><Plus size={11} /> add a misconception</button>
      </div>
      <div class="grid">
        <label class="field"><span>Evidence <small>what proves it, 6+ words</small></span><textarea use:autosize={{ min: 2, max: 8, value: topic.curriculum.evidence }} value={topic.curriculum.evidence} oninput={(e) => { topic.curriculum.evidence = e.currentTarget.value; onchange(); }}></textarea></label>
        <label class="field"><span>Artifact <small>what the lesson leaves behind, 6+ words</small></span><textarea use:autosize={{ min: 2, max: 8, value: topic.curriculum.artifact }} value={topic.curriculum.artifact} oninput={(e) => { topic.curriculum.artifact = e.currentTarget.value; onchange(); }}></textarea></label>
      </div>
      <div class="field">
        <span>Primary sources <small>at least two URLs on the course's hosts</small></span>
        {#each topic.curriculum.primary_sources as item, i (i)}
          <div class="row"><input class="mono" value={item} oninput={(e) => setList('primary_sources', i, e.currentTarget.value)} placeholder="https://…" /><button type="button" class="icon" aria-label="Remove source" onclick={() => dropFrom('primary_sources', i)}><X size={12} /></button></div>
        {/each}
        <button type="button" class="text" onclick={() => addTo('primary_sources')}><Plus size={11} /> add a source</button>
      </div>
    </div>
  {/if}
</article>

<style>
  .topic { border: 1px solid var(--node-border); border-radius: var(--radius-control); background: var(--bg); transition: border-color 0.15s ease; scroll-margin-top: 14px; }
  .topic.open { border-color: color-mix(in srgb, var(--accent) 45%, var(--node-border)); }
  .topic.flawed { border-left: 3px solid var(--warn-fg); }
  .topic.dragging { opacity: 0.35; border-style: dashed; }
  .grip { flex: none; display: grid; place-items: center; width: 18px; height: 26px; margin-left: 2px; border-radius: var(--radius-detail); color: var(--faint); cursor: grab; }
  .grip:hover { color: var(--accent); background: var(--surface); } .topic.dragging .grip { cursor: grabbing; }
  /* The card a jump landed on blinks twice, then settles. */
  .topic.flash { animation: flash 0.7s ease-in-out 3; }
  @keyframes flash { 0%, 100% { box-shadow: 0 0 0 0 transparent; border-color: color-mix(in srgb, var(--accent) 45%, var(--node-border)); } 50% { box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 35%, transparent), 0 0 24px color-mix(in srgb, var(--accent) 30%, transparent); border-color: var(--accent); } }
  .topic-head { display: flex; align-items: center; gap: 6px; padding: 4px 6px 4px 4px; }
  .disclosure { flex: 1; display: flex; align-items: center; gap: 10px; min-width: 0; padding: 7px 8px; border: 0; background: transparent; color: var(--fg); text-align: left; cursor: pointer; border-radius: var(--radius-detail); }
  .disclosure:hover { background: var(--surface); }
  .no { flex: none; width: 24px; height: 24px; display: grid; place-items: center; border-radius: 6px; background: var(--surface-2); color: var(--accent); font-size: 9.5px; }
  .head-text { display: flex; flex-direction: column; gap: 2px; min-width: 0; flex: 1; }
  .head-text strong { font-size: 12.5px; font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .head-text small { font-size: 9px; letter-spacing: 0.4px; color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .flag { display: inline-flex; align-items: center; gap: 4px; padding: 2px 7px; border-radius: 999px; background: var(--warn-bg); color: var(--warn-fg); font-size: 9px; }
  .head-actions { display: inline-flex; gap: 2px; flex: none; }
  .icon { display: inline-grid; place-items: center; width: 26px; height: 26px; border: 1px solid transparent; border-radius: var(--radius-detail); background: transparent; color: var(--muted); cursor: pointer; }
  .icon:hover { border-color: var(--node-border); color: var(--fg); background: var(--surface); }
  .icon.danger:hover { color: var(--led-err); }
  .body { display: flex; flex-direction: column; gap: 14px; padding: 6px 14px 16px; border-top: 1px dashed var(--node-divider); }
  .issues { display: flex; flex-direction: column; gap: 4px; margin: 10px 0 0; padding: 10px 12px; list-style: none; border-left: 2px solid var(--warn-fg); background: var(--surface); }
  .issues li { display: flex; align-items: center; gap: 7px; font-size: 11px; color: var(--warn-fg); }
  .grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px 16px; }
  .span-2 { grid-column: 1 / -1; }
  .field { display: flex; flex-direction: column; gap: 6px; min-width: 0; }
  .field > span { font-size: 11px; color: var(--muted); } .field > span small { margin-left: 6px; color: var(--faint); font-size: 10px; }
  .field.check { flex-direction: row; align-items: center; gap: 9px; padding-top: 18px; } .field.check input { width: auto; }
  input, textarea { width: 100%; padding: 8px 10px; border: 1px solid var(--node-border); border-radius: var(--radius-control); background: var(--node-bg); color: var(--fg); font: 12px/1.5 var(--font-body); }
  input.mono { font-family: var(--font-mono); font-size: 11px; }
  textarea { display: block; }
  input:focus, textarea:focus { outline: none; border-color: var(--accent); }
  .row { display: flex; align-items: center; gap: 6px; }
  .text { align-self: flex-start; display: inline-flex; align-items: center; gap: 5px; padding: 2px 0; border: 0; background: transparent; color: var(--accent); font: 11px var(--font-body); cursor: pointer; }
  .chips { display: flex; flex-wrap: wrap; gap: 5px; }
  .chip { padding: 4px 9px; border: 1px solid var(--node-border); border-radius: 999px; background: transparent; color: var(--muted); font: 10.5px var(--font-body); cursor: pointer; }
  .chip.on { border-color: var(--accent); background: color-mix(in srgb, var(--accent) 14%, var(--surface)); color: var(--fg); }
  .hint { margin: 0; font-size: 11px; color: var(--faint); }
  @media (max-width: 640px) { .grid { grid-template-columns: 1fr; } }
</style>
