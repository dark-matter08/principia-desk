<script lang="ts">
  /** Stable frame for every class lesson: cluster bar, identity with its
   *  progress ring, the stage stepper, save state, the session clock, reading
   *  preferences, the return action and a reading-progress line. */
  import type { Snippet } from 'svelte';
  import type { LessonStage } from '../../ipc';
  import type { StageLink } from './types';
  import ClusterBar from '../../components/ClusterBar.svelte';
  import { ArrowLeft, Check, Lock, Minus, Plus } from 'lucide-svelte';

  let {
    route,
    status,
    code,
    eyebrow,
    title,
    subtitle,
    chips = [],
    stages,
    stage,
    stageMinutes = {},
    saveMessage,
    saveError = false,
    minutes,
    locked = false,
    returnLabel = 'pause class',
    onreturn,
    onstage,
    scroller = $bindable<HTMLElement | undefined>(undefined),
    onscroll,
    actions,
    children,
  }: {
    route: string;
    status: string;
    code: string;
    eyebrow: string;
    title: string;
    subtitle: string;
    /** Short facts shown as chips beside the eyebrow: a level, a mode. */
    chips?: { label: string; tone?: 'accent' | 'good' | 'muted' }[];
    stages: StageLink[];
    stage: LessonStage;
    /** Minutes budgeted per stage, shown under the stepper labels. */
    stageMinutes?: Partial<Record<LessonStage, number>>;
    saveMessage: string;
    saveError?: boolean;
    minutes: number;
    locked?: boolean;
    returnLabel?: string;
    onreturn: () => void;
    onstage: (stage: LessonStage) => void;
    scroller?: HTMLElement | undefined;
    onscroll?: () => void;
    actions?: Snippet;
    children: Snippet;
  } = $props();

  const stored = typeof localStorage !== 'undefined' ? Number(localStorage.getItem('sdr-font-size')) : 0;
  let fontSize = $state(stored || 15);
  function bump(delta: number) {
    fontSize = Math.min(21, Math.max(13, fontSize + delta));
    if (typeof localStorage !== 'undefined') localStorage.setItem('sdr-font-size', String(fontSize));
  }

  const stageIndex = $derived(Math.max(0, stages.findIndex((link) => link.id === stage)));
  /** Stages behind the current one, as a share of the whole, for the ring. */
  const stageShare = $derived(stages.length > 1 ? stageIndex / (stages.length - 1) : 0);
  const RING = 2 * Math.PI * 22;

  /** How far the reader has scrolled, for the line under the header. */
  let readingProgress = $state(0);
  function scrolled() {
    if (scroller) {
      const span = scroller.scrollHeight - scroller.clientHeight;
      readingProgress = span > 0 ? Math.min(1, scroller.scrollTop / span) : 0;
    }
    onscroll?.();
  }

  /** Time in this sitting, from when the lesson opened. */
  const opened = Date.now();
  let elapsed = $state(0);
  $effect(() => {
    const timer = setInterval(() => (elapsed = Math.floor((Date.now() - opened) / 1000)), 1000);
    return () => clearInterval(timer);
  });
  const clock = $derived(`${String(Math.floor(elapsed / 60)).padStart(2, '0')}:${String(elapsed % 60).padStart(2, '0')}`);
  const overBudget = $derived(elapsed > minutes * 60);
</script>

<div class="lesson-shell blueprint">
  <ClusterBar {route} {status} tone="ok" />
  <header class="lesson-head">
    <div class="head-row">
      <button
        class="return-button mono"
        type="button"
        onclick={onreturn}
        disabled={locked}
        title={locked ? 'Focused session: finish the check, or use the escape hatch.' : undefined}
      >
        {#if locked}<Lock size={13} />{:else}<ArrowLeft size={14} />{/if}
        <span>{locked ? 'focused · finish the check' : returnLabel}</span>
      </button>

      <div class="identity">
        <div class="glyph" aria-hidden="true">
          <svg viewBox="0 0 52 52" width="52" height="52">
            <circle cx="26" cy="26" r="22" fill="none" stroke="var(--surface-2)" stroke-width="2.5" />
            <circle class="glyph-arc" cx="26" cy="26" r="22" fill="none" stroke="var(--accent)" stroke-width="2.5" stroke-linecap="round" stroke-dasharray={RING} stroke-dashoffset={RING * (1 - stageShare)} transform="rotate(-90 26 26)" />
          </svg>
          <span class="class-code mono">{code}</span>
        </div>
        <div class="identity-text">
          <div class="facts">
            <span class="eyebrow mono">{eyebrow}</span>
            {#each chips as chip (chip.label)}<span class={`chip mono ${chip.tone ?? 'muted'}`}>{chip.label}</span>{/each}
          </div>
          <h1>{title}</h1>
          <p>{subtitle}</p>
        </div>
      </div>

      <div class="lesson-actions">
        <div class="toolbar">
          <div class="reading-prefs mono" role="group" aria-label="Reading size">
            <span class="aa" aria-hidden="true">Aa</span>
            <button type="button" onclick={() => bump(-1)} disabled={fontSize <= 13} aria-label="Smaller text"><Minus size={11} /></button>
            <span class="size">{fontSize}</span>
            <button type="button" onclick={() => bump(1)} disabled={fontSize >= 21} aria-label="Larger text"><Plus size={11} /></button>
          </div>
          {#if actions}<span class="divider" aria-hidden="true"></span>{@render actions()}{/if}
        </div>
      </div>
    </div>

    <div class="rail-row">
      <nav class="stepper" aria-label="Lesson stages">
        {#each stages as link, index (link.id)}
          {@const done = index < stageIndex}
          {@const current = stage === link.id}
          <button
            type="button"
            class="step"
            class:current
            class:done
            disabled={!link.available}
            aria-current={current ? 'step' : undefined}
            onclick={() => onstage(link.id)}
          >
            <span class="step-mark mono">{#if done}<Check size={11} />{:else}{String(index + 1).padStart(2, '0')}{/if}</span>
            <span class="step-text">
              <span class="step-label mono">{link.label}</span>
              {#if stageMinutes[link.id] != null}<span class="step-minutes mono">{link.id === 'learn' || link.id === 'recall' ? '~' : ''}{stageMinutes[link.id]} min</span>{/if}
            </span>
          </button>
          {#if index < stages.length - 1}<span class="step-track" class:filled={index < stageIndex} aria-hidden="true"></span>{/if}
        {/each}
      </nav>
      <div class="session mono" role="status">
        <span class:err={saveError}>{saveMessage}</span>
        <span class="sep" aria-hidden="true">·</span>
        <span class="clock" class:over={overBudget} title={`Time in this sitting; the session is planned for about ${minutes} minutes`}>{clock} <em>/ {minutes} min</em></span>
      </div>
    </div>
    <div class="reading-line" aria-hidden="true"><i style:transform={`scaleX(${readingProgress})`}></i></div>
  </header>
  <main class="reading-layout" bind:this={scroller} onscroll={scrolled} style={`--reading-font: ${fontSize}px`}>
    {@render children()}
  </main>
</div>

<style>
  .lesson-shell { min-height: 100vh; display: flex; flex: 1; flex-direction: column; background: var(--bg); overflow: hidden; }
  .lesson-head {
    position: relative;
    padding: 12px 22px 0;
    border-bottom: 1px solid var(--node-border);
    background: linear-gradient(180deg, color-mix(in srgb, var(--node-bg) 92%, var(--accent) 8%), var(--node-bg) 60%);
  }
  .head-row { display: grid; grid-template-columns: auto minmax(0, 1fr) auto; align-items: center; gap: 18px; }
  .return-button {
    justify-self: start;
    border: 1px solid var(--node-border);
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--muted);
    min-height: 34px;
    padding: 6px 11px 6px 9px;
    display: inline-flex;
    align-items: center;
    gap: 7px;
    cursor: pointer;
    font-size: 10px;
    letter-spacing: 0.4px;
    white-space: nowrap;
    transition: color 120ms ease, border-color 120ms ease, transform 120ms ease;
  }
  .return-button:hover:not(:disabled) { color: var(--fg); border-color: var(--muted); transform: translateX(-1px); }
  .return-button:disabled { cursor: not-allowed; opacity: 0.8; color: var(--led-warn); border-color: color-mix(in srgb, var(--led-warn) 40%, var(--node-border)); }

  .identity { display: flex; align-items: center; gap: 14px; min-width: 0; }
  .glyph { position: relative; width: 52px; height: 52px; flex: none; display: grid; place-items: center; }
  .glyph svg { position: absolute; inset: 0; }
  .glyph-arc { transition: stroke-dashoffset 700ms cubic-bezier(0.22, 1, 0.36, 1); }
  .class-code { position: relative; color: var(--accent); font-size: 12px; letter-spacing: 0.5px; }
  .identity-text { min-width: 0; }
  .facts { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
  .eyebrow { color: var(--accent); font-size: 9px; letter-spacing: 1.4px; }
  .chip { display: inline-flex; align-items: center; padding: 2px 7px; border-radius: 999px; border: 1px solid var(--node-border); font-size: 8px; letter-spacing: 1px; text-transform: uppercase; color: var(--muted); }
  .chip.accent { color: var(--accent); border-color: color-mix(in srgb, var(--accent) 45%, var(--node-border)); background: color-mix(in srgb, var(--accent) 10%, transparent); }
  .chip.good { color: var(--ok-fg); border-color: color-mix(in srgb, var(--ok-fg) 45%, var(--node-border)); background: color-mix(in srgb, var(--ok-fg) 8%, transparent); }
  h1 { font-size: 20px; line-height: 1.2; margin: 4px 0 3px; overflow: hidden; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; }
  .identity p { color: var(--muted); font-size: 10px; margin: 0; }

  .lesson-actions { justify-self: end; display: flex; align-items: center; }
  .toolbar { display: inline-flex; align-items: center; gap: 4px; padding: 3px; border: 1px solid var(--node-border); border-radius: calc(var(--radius-control) + 3px); background: color-mix(in srgb, var(--surface) 80%, transparent); }
  /* Only the toolbar's own controls take its flat look; a menu that opens
     from one keeps its own. */
  .toolbar > :global(button), .toolbar > :global(.download > button) { min-height: 28px; border: 0 !important; border-radius: var(--radius-control) !important; background: transparent !important; color: var(--muted); font-size: 9.5px; padding: 5px 9px; transition: background 120ms ease, color 120ms ease; }
  .toolbar > :global(button:hover:not(:disabled)), .toolbar > :global(.download > button:hover:not(:disabled)) { background: var(--surface-2) !important; color: var(--fg); }
  .toolbar > :global(button.active), .toolbar > :global(.download > button[aria-expanded='true']) { background: color-mix(in srgb, var(--accent) 16%, var(--surface-2)) !important; color: var(--accent); }
  .toolbar :global(.quality) { padding: 0 8px; color: var(--faint); font-size: 8px; display: inline-flex; align-items: center; gap: 5px; }
  .divider { width: 1px; height: 18px; background: var(--node-border); margin: 0 2px; }
  .reading-prefs { display: inline-flex; align-items: center; gap: 2px; color: var(--faint); font-size: 9px; padding: 0 2px; }
  .reading-prefs .aa { font-family: var(--font-body); font-size: 11px; color: var(--muted); margin-right: 2px; }
  .reading-prefs .size { min-width: 18px; text-align: center; color: var(--muted); }
  .reading-prefs button { width: 24px; height: 24px; display: grid; place-items: center; padding: 0 !important; border: 0; border-radius: var(--radius-detail); background: transparent; color: var(--muted); cursor: pointer; }
  .reading-prefs button:hover:not(:disabled) { background: var(--surface-2); color: var(--fg); }
  .reading-prefs button:disabled { opacity: 0.35; cursor: default; }

  .rail-row { display: flex; align-items: center; justify-content: space-between; gap: 18px; padding: 12px 0 10px; }
  .stepper { display: flex; align-items: center; gap: 0; min-width: 0; }
  .step { display: inline-flex; align-items: center; gap: 8px; padding: 4px 6px; border: 0; background: transparent; color: var(--muted); cursor: pointer; border-radius: var(--radius-control); transition: color 120ms ease; }
  .step:hover:not(:disabled) { color: var(--fg); }
  .step:disabled { opacity: 0.45; cursor: default; }
  .step-mark { display: grid; place-items: center; width: 22px; height: 22px; border-radius: 50%; border: 1.5px solid var(--node-border); font-size: 8.5px; color: var(--faint); transition: background 200ms ease, border-color 200ms ease, color 200ms ease; }
  .step-text { display: flex; flex-direction: column; gap: 1px; text-align: left; }
  .step-label { font-size: 9px; letter-spacing: 1px; text-transform: uppercase; }
  .step-minutes { font-size: 8px; color: var(--faint); letter-spacing: 0.4px; }
  .step.done .step-mark { background: color-mix(in srgb, var(--ok-fg) 18%, transparent); border-color: var(--ok-fg); color: var(--ok-fg); }
  .step.done { color: var(--faint); }
  .step.current .step-mark { background: var(--accent); border-color: var(--accent); color: var(--accent-fg); box-shadow: 0 0 0 4px color-mix(in srgb, var(--accent) 18%, transparent); }
  .step.current { color: var(--fg); }
  .step.current .step-label { color: var(--accent); }
  .step-track { width: clamp(18px, 4vw, 46px); height: 1.5px; background: var(--node-border); margin: 0 4px; transition: background 300ms ease; }
  .step-track.filled { background: var(--ok-fg); }
  .session { display: inline-flex; align-items: center; gap: 8px; color: var(--faint); font-size: 9px; white-space: nowrap; }
  .session .sep { color: var(--border); }
  .session .err { color: var(--red); }
  .clock { color: var(--muted); font-variant-numeric: tabular-nums; }
  .clock em { font-style: normal; color: var(--faint); }
  .clock.over { color: var(--led-warn); }
  .reading-line { position: absolute; left: 0; right: 0; bottom: -1px; height: 2px; background: transparent; }
  .reading-line i { display: block; height: 100%; width: 100%; transform-origin: left; background: linear-gradient(90deg, var(--accent), var(--ok-fg)); transform: scaleX(0); transition: transform 120ms linear; }
  .reading-layout { position: relative; flex: 1; min-height: 0; overflow-y: auto; }
  .return-button:focus-visible, .step:focus-visible, .reading-prefs button:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
  @media (prefers-reduced-motion: reduce) { .glyph-arc, .step-mark, .step-track, .reading-line i, .return-button { transition: none; } }
  @media (max-width: 960px) {
    .head-row { grid-template-columns: auto minmax(0, 1fr); }
    .lesson-actions { grid-column: 1 / -1; justify-self: start; }
    .rail-row { flex-wrap: wrap; }
    .step-minutes { display: none; }
  }
</style>
