<script lang="ts">
  import type { SessionPlan } from '../../ipc';
  import { readingMinutes, type LessonSection } from './sections';
  import { iconSvg } from './icons';
  import { Check } from 'lucide-svelte';

  export type MapStage = 'practice' | 'check';

  let {
    sections,
    current,
    stage = null,
    plan,
    variant = 'rail',
    onjump,
    onstage,
  }: {
    sections: LessonSection[];
    /** The rail sits beside the reading; the strip sticks above it on a narrow window. */
    variant?: 'rail' | 'strip';
    /** Index of the section the reader is in. */
    current: number;
    /** Set once the reader has scrolled past the lesson into practice or the check. */
    stage?: MapStage | null;
    plan: SessionPlan;
    onjump: (section: LessonSection) => void;
    onstage?: (stage: MapStage) => void;
  } = $props();

  const hinted = $derived(readingMinutes(sections));
  const readMinutes = $derived(hinted > 0 ? hinted : plan.learn_minutes);
  const total = $derived(readMinutes + plan.practice_minutes + plan.check_minutes);
  const stages: { id: MapStage; label: string; icon: string; minutes: number }[] = $derived([
    { id: 'practice', label: 'Practice', icon: 'list-checks', minutes: plan.practice_minutes },
    { id: 'check', label: 'Check', icon: 'circle-check', minutes: plan.check_minutes },
  ]);
</script>

{#if sections.length && variant === 'rail'}
  <!-- The rail: beside the reading on a wide window, always in view. -->
  <nav class="lesson-map rail" aria-label="Lesson sections">
    <div class="rail-head">
      <span class="eyebrow mono">THIS SESSION</span>
      <span class="total mono">about {total} min</span>
      <ul class="plan mono" aria-label="How the time divides">
        <li><span class="dot read"></span>read ~{readMinutes}</li>
        <li><span class="dot practise"></span>practise {plan.practice_minutes}</li>
        <li><span class="dot check"></span>check {plan.check_minutes}</li>
      </ul>
    </div>
    <ol class="parts">
      {#each sections as section (section.id)}
        <li>
          <button
            type="button"
            class="part"
            class:current={stage === null && section.index === current}
            class:done={stage !== null || section.index < current}
            onclick={() => onjump(section)}
            aria-current={stage === null && section.index === current ? 'location' : undefined}
            title={section.purpose || section.display}
          >
            <span class="part-icon" aria-hidden="true">{@html iconSvg(section.icon, 13)}</span>
            <span class="part-text">
              <span class="part-title">{section.display}</span>
              {#if section.hint}<span class="part-minutes mono">~{section.hint.minutes} min</span>{/if}
            </span>
            {#if stage !== null || section.index < current}<span class="part-done" aria-label="read"><Check size={10} /></span>{/if}
          </button>
        </li>
      {/each}
    </ol>
    {#if onstage}
      <ol class="parts stages" aria-label="After the reading">
        {#each stages as item (item.id)}
          <li>
            <button type="button" class="part" class:current={stage === item.id} class:done={stage === 'check' && item.id === 'practice'} onclick={() => onstage?.(item.id)} aria-current={stage === item.id ? 'location' : undefined}>
              <span class="part-icon stage-icon" aria-hidden="true">{@html iconSvg(item.icon, 13)}</span>
              <span class="part-text"><span class="part-title">{item.label}</span><span class="part-minutes mono">{item.minutes} min</span></span>
            </button>
          </li>
        {/each}
      </ol>
    {/if}
  </nav>

{:else if sections.length && variant === 'strip'}
  <!-- The strip: a sticky row of the same parts when the window is narrow. -->
  <nav class="lesson-map strip" aria-label="Lesson sections">
    <ol class="strip-parts">
      {#each sections as section (section.id)}
        <li>
          <button type="button" class="strip-part" class:current={stage === null && section.index === current} class:done={stage !== null || section.index < current} onclick={() => onjump(section)} title={`${section.display}${section.hint ? ` · ~${section.hint.minutes} min` : ''}`} aria-label={section.display} aria-current={stage === null && section.index === current ? 'location' : undefined}>
            <span aria-hidden="true">{@html iconSvg(section.icon, 14)}</span>
          </button>
        </li>
      {/each}
      {#if onstage}
        {#each stages as item (item.id)}
          <li class="strip-stage">
            <button type="button" class="strip-part" class:current={stage === item.id} onclick={() => onstage?.(item.id)} title={`${item.label} · ${item.minutes} min`} aria-label={item.label} aria-current={stage === item.id ? 'location' : undefined}>
              <span aria-hidden="true">{@html iconSvg(item.icon, 14)}</span>
            </button>
          </li>
        {/each}
      {/if}
    </ol>
    <span class="strip-label">{stage ? stages.find((item) => item.id === stage)?.label : sections[current]?.display}</span>
  </nav>
{/if}

<style>
  .eyebrow { color: var(--accent); font-size: 9px; letter-spacing: 1.4px; }

  /* --- Rail ---------------------------------------------------------------- */
  .rail {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px 10px 12px 12px;
    border: 1px solid var(--node-border);
    border-radius: var(--radius-panel);
    background: color-mix(in srgb, var(--surface) 70%, transparent);
  }
  .rail-head { display: flex; flex-direction: column; gap: 5px; padding: 0 2px 6px; border-bottom: 1px dashed var(--node-divider); }
  .total { font-size: 9.5px; color: var(--muted); }
  .plan { display: flex; flex-wrap: wrap; gap: 4px 10px; margin: 2px 0 0; padding: 0; list-style: none; font-size: 8.5px; letter-spacing: 0.4px; color: var(--muted); }
  .plan li { display: inline-flex; align-items: center; gap: 5px; white-space: nowrap; }
  .dot { width: 6px; height: 6px; border-radius: 50%; background: var(--accent); }
  .dot.practise { background: var(--ok-fg); }
  .dot.check { background: var(--violet-fg, var(--muted)); }
  .parts { display: flex; flex-direction: column; gap: 2px; margin: 0; padding: 0; list-style: none; }
  .parts.stages { padding-top: 8px; border-top: 1px dashed var(--node-divider); }
  .part {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 34px;
    padding: 5px 6px;
    border: 1px solid transparent;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--muted);
    text-align: left;
    cursor: pointer;
    font: inherit;
  }
  .part:hover { background: var(--surface); color: var(--fg); }
  .part:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
  .part.current { border-color: color-mix(in srgb, var(--accent) 55%, var(--node-border)); background: color-mix(in srgb, var(--accent) 9%, var(--surface)); color: var(--fg); }
  .part.done { color: var(--faint); }
  .part.done .part-icon { color: var(--faint); }
  .part-icon { flex: none; display: grid; place-items: center; width: 22px; height: 22px; border-radius: var(--radius-detail); background: var(--surface-2); color: var(--accent); }
  .stage-icon { color: var(--ok-fg); }
  .part-text { display: flex; flex-direction: column; gap: 1px; min-width: 0; }
  .part-title { font-size: 11px; line-height: 1.25; }
  .part-minutes { font-size: 8px; letter-spacing: 0.5px; color: var(--faint); }
  .part-done { margin-left: auto; color: var(--ok-fg); display: inline-flex; }

  /* --- Strip --------------------------------------------------------------- */
  .strip {
    display: flex;
    position: sticky;
    top: 12px;
    z-index: 5;
    align-items: center;
    gap: 12px;
    margin: 12px 0 22px;
    padding: 6px 10px;
    border: 1px solid var(--node-border);
    border-radius: var(--radius-panel);
    background: color-mix(in srgb, var(--bg) 92%, transparent);
    backdrop-filter: blur(8px);
  }
  .strip-parts { display: flex; gap: 3px; margin: 0; padding: 0; list-style: none; }
  .strip-stage:first-of-type { margin-left: 8px; }
  .strip-part { display: grid; place-items: center; width: 28px; height: 28px; border: 1px solid transparent; border-radius: var(--radius-detail); background: transparent; color: var(--muted); cursor: pointer; }
  .strip-part:hover { background: var(--surface); color: var(--fg); }
  .strip-part.current { border-color: color-mix(in srgb, var(--accent) 55%, var(--node-border)); background: color-mix(in srgb, var(--accent) 12%, var(--surface)); color: var(--accent); }
  .strip-part.done { color: var(--faint); }
  .strip-label { font-size: 11px; color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }

  /* Whichever fits: the rail beside the reading, else the strip above it. */
  @media (min-width: 1180px) {
    .strip { display: none; }
  }
</style>
