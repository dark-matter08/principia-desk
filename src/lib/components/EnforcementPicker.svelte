<script lang="ts">
  /** Kiosk strictness selector with honest, plain-language explanations and
   *  a hard-mode warning. Used in setup and on the idle screen. */
  import { TriangleAlert } from 'lucide-svelte';

  let {
    value = $bindable('hard'),
  }: { value?: string } = $props();

  const LEVELS = [
    {
      id: 'advisory',
      name: 'ADVISORY',
      tag: 'honor system',
      tone: 'teal',
      desc: 'The window comes to front and stays on top, but nothing is blocked: you can switch apps, the session just stays owed and waits. For people who only need a nudge.',
      blocks: 'blocks: nothing',
    },
    {
      id: 'firm',
      name: 'FIRM',
      tag: 'level 1000 · escapable',
      tone: 'amber',
      desc: 'Full-screen above the menu bar, focus snaps back every 300ms, other displays sealed by the chaos lab, media paused and muted. Switching away is useless, but Force Quit and ⌘⌥⎋ still work if something goes wrong.',
      blocks: 'blocks: Dock, menu bar · keeps: Force Quit',
    },
    {
      id: 'hard',
      name: 'HARD',
      tag: 'no mercy',
      tone: 'red',
      desc: 'Everything in FIRM, plus ⌘Tab, Force Quit, ⌘⌥⎋, logout and shutdown are disabled while locked. The only exits are: finish the session, pass a perfect adaptive exit-check round, or type the break-glass phrase (streak resets).',
      blocks: 'blocks: ⌘Tab, Force Quit, logout, shutdown',
    },
  ];
</script>

<div class="picker">
  {#each LEVELS as l}
    <button
      class="lvl mono"
      class:active={value === l.id}
      class:t-teal={l.tone === 'teal'}
      class:t-amber={l.tone === 'amber'}
      class:t-red={l.tone === 'red'}
      onclick={() => (value = l.id)}
    >
      <span class="lvl-name">{l.name}</span>
      <span class="lvl-tag">{l.tag}</span>
    </button>
  {/each}
</div>
{#each LEVELS.filter((l) => l.id === value) as l}
  <p class="lvl-desc">{l.desc}</p>
  <p class="lvl-blocks mono">{l.blocks}</p>
{/each}
{#if value === 'hard'}
  <div class="hard-warn mono">
    <TriangleAlert size={11} /> HARD means it: while locked, this machine does nothing else. If the app ever misbehaves
    mid-lock, recovery needs another machine or Safe Mode (see README → Recovery). The
    white-screen guard and the ~/principia-unlock release token remain as last resorts.
  </div>
{/if}

<style>
  .picker {
    display: flex;
    gap: 8px;
    margin-top: 8px;
  }
  .lvl {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 3px;
    align-items: flex-start;
    background: var(--bg);
    border: 1px solid var(--node-border);
    border-radius: var(--radius-control);
    padding: 8px 11px;
    cursor: pointer;
    text-align: left;
  }
  .lvl-name {
    font-size: 11px;
    letter-spacing: 1.5px;
    color: var(--muted);
  }
  .lvl-tag {
    font-size: 8.5px;
    color: var(--faint);
    letter-spacing: 0.5px;
  }
  .lvl:hover {
    border-color: var(--muted);
  }
  .lvl.active.t-teal {
    border-color: var(--led-ok);
    background: var(--surface-2);
  }
  .lvl.active.t-teal .lvl-name {
    color: var(--led-ok);
  }
  .lvl.active.t-amber {
    border-color: var(--led-warn);
    background: var(--surface-2);
  }
  .lvl.active.t-amber .lvl-name {
    color: var(--led-warn);
  }
  .lvl.active.t-red {
    border-color: var(--led-err);
    background: var(--surface-2);
  }
  .lvl.active.t-red .lvl-name {
    color: var(--led-err);
  }
  .lvl-desc {
    font-size: 12.5px;
    color: var(--muted);
    line-height: 1.55;
    margin: 10px 0 2px;
  }
  .lvl-blocks {
    font-size: 10px;
    color: var(--faint);
    letter-spacing: 0.5px;
    margin: 0;
  }
  .hard-warn {
    margin-top: 10px;
    font-size: 10.5px;
    line-height: 1.6;
    color: var(--bad-fg);
    background: var(--bad-bg);
    border: 1px dashed var(--led-err);
    border-radius: var(--radius-control);
    padding: 9px 12px;
  }
</style>
