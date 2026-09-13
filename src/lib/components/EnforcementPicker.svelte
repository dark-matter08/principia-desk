<script lang="ts">
  /** Kiosk strictness selector with honest, plain-language explanations and
   *  a hard-mode warning. Used in setup and on the idle screen. */
  import { TriangleAlert } from 'lucide-svelte';
  import { PLATFORM, PLATFORM_NAME, SWITCH_KEY } from '$lib/platform';

  let {
    value = $bindable('hard'),
  }: { value?: string } = $props();

  // What each policy does depends on what the platform lets an application
  // do to it. macOS lets the desk hide the Dock and, in HARD, disable
  // application switching, Force Quit, logout and shutdown; Windows and Linux
  // do not hand that to an application, so there HARD holds the desk the way
  // FIRM does and the ways out are the same.
  const mac = PLATFORM === 'macos';
  const switchKey = SWITCH_KEY[PLATFORM];
  const quitKey = mac ? 'Force Quit' : PLATFORM === 'windows' ? 'the Task Manager' : 'the system keys';
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
      desc: mac
        ? 'Full-screen above the menu bar, focus snaps back every 300ms, other displays sealed, media paused and muted. Switching away is useless, but Force Quit and ⌘⌥⎋ still work if something goes wrong.'
        : `Full screen, always on top, focus snaps back every 300ms, other displays sealed${PLATFORM === 'linux' ? ', media paused and muted where playerctl and pactl exist' : ''}. Switching away is useless, but ${switchKey} and ${quitKey} still work if something goes wrong.`,
      blocks: mac ? 'blocks: Dock, menu bar · keeps: Force Quit' : `holds: the screen, the focus · keeps: ${switchKey}, ${quitKey}`,
    },
    {
      id: 'hard',
      name: 'HARD',
      tag: mac ? 'no mercy' : 'as FIRM on this platform',
      tone: 'red',
      desc: mac
        ? 'Everything in FIRM, plus ⌘Tab, Force Quit, ⌘⌥⎋, logout and shutdown are disabled while locked. The only exits are: finish the session, pass a perfect adaptive exit-check round, or type the break-glass phrase (streak resets).'
        : `${PLATFORM_NAME[PLATFORM]} does not let an application disable ${switchKey}, ${quitKey}, logout or shutdown, so HARD holds the desk the way FIRM does. The lesson still ends only by finishing it, the break-glass phrase, the recovery console or a release token.`,
      blocks: mac ? 'blocks: ⌘Tab, Force Quit, logout, shutdown' : `holds: the screen, the focus · keeps: ${switchKey}, ${quitKey}`,
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
{#if value === 'hard' && mac}
  <div class="hard-warn mono">
    <TriangleAlert size={11} /> HARD means it: while locked, this machine does nothing else. If the app ever misbehaves
    mid-lock, the recovery console (Control + Option + Shift + U, or five presses in ten seconds), the
    ~/principia-unlock release token and the three-hour dead man's switch are what remain. Keep a USB stick by the desk.
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
