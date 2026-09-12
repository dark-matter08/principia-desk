<script lang="ts">
  import { tick, type Snippet } from 'svelte';
  import { app } from '../stores.svelte';
  import { DESTINATIONS } from './navigation';
  import ClusterBar from '../components/ClusterBar.svelte';
  import PreparationStatus from '../features/classes/PreparationStatus.svelte';
  import mark from '../../../docs/logo-mark.svg';
  import { BookOpen, ScrollText, SlidersHorizontal, Sun, TrendingUp } from 'lucide-svelte';
  import type { Destination } from './navigation';
  let { children }: { children: Snippet } = $props();
  /** A glyph per route; the tile it sits in lights for the route in view. */
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const GLYPHS: Record<Destination, any> = { today: Sun, classes: BookOpen, progress: TrendingUp, logs: ScrollText, settings: SlidersHorizontal };
  const selected = $derived(app.screen === 'dashboard' ? 'progress' : app.destination);
  let content: HTMLElement;
  $effect(() => { selected; void tick().then(() => content?.scrollTo(0, 0)); });
</script>

<div class="desk blueprint">
  <a class="skip-link" href="#desk-content">Skip to content</a>
  <ClusterBar route={selected} status={app.locked ? 'focused session in progress' : 'all systems nominal'} tone={app.locked ? 'warn' : 'ok'} />
  <header class="command-bar">
    <button class="brand" onclick={() => app.navigate('today')} aria-label="Principia Desk home"><img src={mark} alt="" /><span>Principia Desk<small class="mono">PERSONAL LEARNING SYSTEM</small></span></button>
    <nav aria-label="Main navigation">
      {#each DESTINATIONS as destination, index}
        {@const Glyph = GLYPHS[destination.id]}
        <button class:selected={selected === destination.id} aria-current={selected === destination.id ? 'page' : undefined} onclick={() => app.navigate(destination.id)}>
          <span class="route-glyph" aria-hidden="true"><Glyph size={13} /></span>
          <span class="route-text"><span class="route-index">{String(index + 1).padStart(2, '0')}</span>{destination.label}</span>
        </button>
      {/each}
    </nav>
  </header>
  {#if app.preparingClass}<PreparationStatus />{/if}
  <main id="desk-content" tabindex="-1" bind:this={content}>{@render children()}</main>
</div>

<style>
  .desk { display: flex; flex-direction: column; flex: 1; min-height: 0; }
  .command-bar { position: relative; z-index: 2; flex-shrink: 0; display: flex; align-items: center; justify-content: space-between; gap: 28px; margin: 18px 28px 0; padding-bottom: 18px; border-bottom: 1px dashed var(--node-border); }
  .brand { display: flex; align-items: center; gap: 11px; border: 0; background: none; color: var(--fg); text-align: left; font: 20px/1.2 var(--font-display); cursor: pointer; padding: 0; flex-shrink: 0; }
  .brand img { width: 36px; height: 36px; }
  .brand small { display: block; margin-top: 6px; font-size: 8px; color: var(--faint); letter-spacing: 1.3px; }
  /* The routes: one rail, a glyph tile per route, and the route in view
     lit: a tinted chip with a bright top edge and a glowing tile. */
  nav { display: flex; flex-wrap: wrap; gap: 3px; padding: 4px; border: 1px solid var(--node-border); border-radius: 11px; background: color-mix(in srgb, var(--node-bg) 85%, transparent); }
  nav button { position: relative; display: flex; align-items: center; gap: 8px; border: 1px solid transparent; border-radius: 8px; background: transparent; color: var(--muted); font: 11px var(--font-mono); letter-spacing: 0.4px; padding: 5px 12px 5px 6px; cursor: pointer; transition: color 0.15s ease, background 0.15s ease, border-color 0.15s ease; }
  nav button:hover { color: var(--fg); background: var(--surface); }
  nav button:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
  .route-glyph { display: grid; place-items: center; width: 24px; height: 24px; border-radius: 6px; background: var(--surface-2); color: var(--faint); transition: background 0.18s ease, color 0.18s ease, box-shadow 0.18s ease; }
  nav button:hover .route-glyph { color: var(--muted); }
  .route-text { display: inline-flex; align-items: baseline; gap: 6px; }
  .route-index { color: var(--faint); font-size: 9px; letter-spacing: 0.6px; }
  nav button.selected { color: var(--fg); border-color: color-mix(in srgb, var(--accent) 45%, var(--node-border)); background: linear-gradient(180deg, color-mix(in srgb, var(--accent) 14%, var(--surface)), var(--surface)); box-shadow: inset 0 1px 0 color-mix(in srgb, var(--accent) 55%, transparent), 0 8px 20px -12px color-mix(in srgb, var(--accent) 80%, transparent); }
  nav button.selected .route-glyph { background: var(--accent); color: var(--accent-fg); box-shadow: 0 0 12px color-mix(in srgb, var(--accent) 45%, transparent); }
  nav button.selected .route-index { color: var(--accent); }
  main { min-height: 0; min-width: 0; overflow-y: auto; display: flex; flex-direction: column; flex: 1; }
  .skip-link { position: fixed; top: -60px; left: 12px; padding: 8px 12px; background: var(--surface); color: var(--fg); z-index: 90; }
  .skip-link:focus { top: 8px; }
  @media (max-width: 940px) { .command-bar { gap: 18px; } nav button { padding: 5px 9px 5px 5px; gap: 6px; } .route-index { display: none; } }
  @media (max-width: 720px) { .command-bar { align-items: flex-start; flex-direction: column; margin: 14px 18px 0; gap: 16px; } nav { width: 100%; gap: 3px; } nav button { justify-content: center; flex: 1; font-size: 10px; padding: 5px 4px; } .brand { font-size: 19px; } }
  @media (max-width: 420px) { nav button { gap: 4px; } .route-glyph { width: 20px; height: 20px; } }
</style>
