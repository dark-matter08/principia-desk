<script lang="ts">
  /**
   * The way out, taught once and kept where it can be read again: the key
   * combination as keycaps, the four-command ladder beside a console that
   * plays the exchange through, and the exits that need no console at all.
   */
  import { onMount } from 'svelte';
  import { api, isTauri, type RecoveryStatus } from '../../ipc';
  import { Play, RotateCcw, TerminalSquare, Usb, Timer, Keyboard, CircleAlert } from 'lucide-svelte';
  import { guessStatus } from './ladder';

  let {
    phrase = '',
    autoplay = false,
    showOpen = true,
  }: {
    /** The learner's own phrase, so the walkthrough reads as theirs. */
    phrase?: string;
    /** Play the walkthrough as soon as the guide is on screen. */
    autoplay?: boolean;
    /** Offer to open the real console. */
    showOpen?: boolean;
  } = $props();

  /** The desk names the combination for the platform it runs on; until it
   *  answers, the browser's guess stands in. */
  let status = $state<RecoveryStatus>(guessStatus());
  let answered = $state(false);
  const keys = $derived(status.combination.keys);
  const LADDER = $derived(status.ladder);

  const CODE = 'K7PM2X';
  const shownPhrase = $derived(phrase.trim() || 'your escape phrase');
  const commands = $derived([`unlock`, `confirm ${CODE}`, `phrase ${shownPhrase}`, `release`]);

  interface Line { kind: 'in' | 'out'; text: string }
  const script = $derived<{ rung: number; lines: Line[] }[]>([
    { rung: 0, lines: [
      { kind: 'in', text: commands[0] },
      { kind: 'out', text: 'desk: LOCKED' },
      { kind: 'out', text: 'focused session: linux-bash · concept 3' },
      { kind: 'out', text: '' },
      { kind: 'out', text: 'Releasing pauses the session with its work intact and breaks your streak.' },
      { kind: 'out', text: `Challenge code: ${CODE}` },
      { kind: 'out', text: `To continue, type:  confirm ${CODE}` },
    ] },
    { rung: 1, lines: [{ kind: 'in', text: commands[1] }, { kind: 'out', text: 'Confirmed.' }, { kind: 'out', text: 'Now type:  phrase <your escape phrase>' }] },
    { rung: 2, lines: [{ kind: 'in', text: commands[2] }, { kind: 'out', text: 'Phrase accepted.' }, { kind: 'out', text: 'Type:  release' }] },
    { rung: 3, lines: [{ kind: 'in', text: commands[3] }, { kind: 'out', text: 'Released. The lock is down.' }, { kind: 'out', text: 'The session is paused with its work intact; resume it from the desk when you are ready.' }] },
  ]);

  let shown = $state<Line[]>([]);
  let typing = $state('');
  let rung = $state(-1);
  let playing = $state(false);
  let played = $state(false);
  let run = 0;
  let term = $state<HTMLElement | undefined>(undefined);
  let root = $state<HTMLElement | undefined>(undefined);

  const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));
  function scroll() { term?.scrollTo({ top: term.scrollHeight }); }

  async function play() {
    const mine = ++run;
    shown = []; typing = ''; rung = -1; playing = true; played = false;
    for (const beat of script) {
      if (mine !== run) return;
      rung = beat.rung;
      for (const line of beat.lines) {
        if (mine !== run) return;
        if (line.kind === 'in') {
          await wait(500);
          for (let i = 1; i <= line.text.length; i++) {
            if (mine !== run) return;
            typing = line.text.slice(0, i);
            await wait(line.text.length > 40 ? 12 : 38);
          }
          await wait(220);
          shown = [...shown, line]; typing = ''; scroll();
        } else {
          await wait(70);
          shown = [...shown, line]; scroll();
        }
      }
      await wait(650);
    }
    if (mine !== run) return;
    rung = 4; playing = false; played = true;
  }

  function stop() { run += 1; playing = false; }

  onMount(() => {
    api.recoveryStatus().then((fresh) => { status = fresh; answered = true; }).catch(() => {});
    // Play once: straight away when asked to, otherwise the first time the
    // walkthrough scrolls into view, so it is seen without being hunted for.
    if (autoplay) {
      void play();
      return stop;
    }
    if (!root || typeof IntersectionObserver === 'undefined') return stop;
    const watcher = new IntersectionObserver((entries) => {
      if (entries.some((entry) => entry.isIntersecting) && !played && !playing) {
        void play();
        watcher.disconnect();
      }
    }, { threshold: 0.35 });
    watcher.observe(root);
    return () => { watcher.disconnect(); stop(); };
  });

  let opening = $state(false);
  async function open() {
    opening = true;
    try { await api.openRecoveryConsole(); } finally { opening = false; }
  }
</script>

<div class="guide" bind:this={root}>
  <div class="combo">
    <div class="keys" aria-label={`Press ${status.combination.label}`}>
      {#each keys as key, index (index)}
        {#if index > 0}<span class="plus" aria-hidden="true">+</span>{/if}
        <kbd class="keycap" class:word={key.glyph.length > 1}><span class="glyph">{key.glyph}</span>{#if key.name}<span class="name">{key.name}</span>{/if}</kbd>
      {/each}
    </div>
    <div class="combo-text">
      <p class="combo-title"><span class="combo-label mono">{status.combination.label}</span> opens the recovery console <em>above every window</em>, including a locked desk.</p>
      <p class="combo-sub">The combination is registered with {status.combination.platform}, not the page, so a blank or frozen screen cannot swallow it. Nothing changes until you type <code>release</code>.</p>
      <p class="combo-state mono" class:ok={answered && status.registered} class:warn={answered && !status.registered}>
        {#if !answered}<span class="led idle"></span> {isTauri ? 'asking the desk…' : `preview · ${status.combination.platform} keys shown`}
        {:else if status.registered}<span class="led ok"></span> registered with {status.combination.platform}
        {:else}<CircleAlert size={11} /> {status.combination.platform} has not taken the combination{/if}
      </p>
      {#if answered && !status.registered}<p class="combo-note">{status.combination.note}</p>{/if}
    </div>
  </div>

  <div class="walk">
    <ol class="ladder" aria-label="The four commands, in order">
      {#each LADDER as step, index (step.command)}
        <li class="rung" class:active={rung === index} class:done={rung > index}>
          <span class="rung-no mono">{index + 1}</span>
          <span class="rung-body">
            <code class="rung-cmd">{step.command}</code>
            <span class="rung-what">{step.what}</span>
          </span>
        </li>
      {/each}
    </ol>

    <div class="term-wrap">
      <div class="term-bar">
        <span class="term-dots" aria-hidden="true"><i></i><i></i><i></i></span>
        <span class="term-title mono"><TerminalSquare size={11} /> principia recovery · walkthrough</span>
        <button type="button" class="term-play mono" onclick={() => void play()} aria-label={played ? 'Play the walkthrough again' : 'Play the walkthrough'}>
          {#if playing}<span class="live"></span> playing{:else if played}<RotateCcw size={10} /> replay{:else}<Play size={10} /> play{/if}
        </button>
      </div>
      <div class="term" bind:this={term} aria-live="off">
        {#if !shown.length && !typing}
          <div class="term-idle">press play to watch the four steps end a locked session</div>
        {/if}
        {#each shown as line, index (index)}
          <div class={`tl ${line.kind}`}>{#if line.kind === 'in'}<span class="ps">principia&gt;</span>{/if}{line.text}</div>
        {/each}
        {#if typing || playing}
          <div class="tl in"><span class="ps">principia&gt;</span>{typing}<span class="caret"></span></div>
        {/if}
        {#if played}
          <div class="tl released">✓ desk released · the console closes on its own</div>
        {/if}
      </div>
    </div>
  </div>

  <div class="exits">
    <span class="exits-label mono">IF THE CONSOLE CANNOT OPEN</span>
    <ul>
      <li><span class="exit-icon"><Keyboard size={12} /></span><span>Press the combination <b>{status.valve_presses} times within {status.valve_seconds} seconds</b>. The lock releases on its own, no window needed.</span></li>
      <li><span class="exit-icon"><Usb size={12} /></span><span>Plug in a USB stick with a file named <code>principia-unlock</code> at its root. The lock drops within a second.</span></li>
      <li><span class="exit-icon"><Timer size={12} /></span><span>Wait. A lock lets go by itself after <b>three hours</b>, whatever the app believes.</span></li>
    </ul>
  </div>

  {#if showOpen}
    <div class="try">
      <button type="button" class="ghost mono-ghost" onclick={() => void open()} disabled={opening || !isTauri}><TerminalSquare size={12} /> Open the console now</button>
      <span class="try-hint">{isTauri ? 'It is safe to look: the desk is untouched until the fourth command.' : 'Available in the desktop app.'}</span>
    </div>
  {/if}
</div>

<style>
  .guide { display: flex; flex-direction: column; gap: 18px; }

  /* --- The combination ------------------------------------------------- */
  .combo { display: flex; align-items: center; gap: 22px; flex-wrap: wrap; }
  .keys { display: inline-flex; align-items: center; gap: 8px; flex: none; }
  .plus { color: var(--faint); font-size: 14px; }
  .keycap {
    display: inline-flex; flex-direction: column; align-items: center; justify-content: center; gap: 1px;
    min-width: 46px; height: 46px; padding: 0 10px;
    border: 1px solid color-mix(in srgb, var(--accent) 45%, var(--node-border));
    border-bottom-width: 4px;
    border-radius: 8px;
    background: linear-gradient(180deg, color-mix(in srgb, var(--accent) 16%, var(--surface-2)), var(--surface));
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--accent) 12%, transparent), 0 6px 18px -10px color-mix(in srgb, var(--accent) 70%, transparent);
    color: var(--fg); font: 500 15px/1 var(--font-mono);
    animation: keypress 3.6s ease-in-out infinite;
  }
  .keycap:nth-child(3) { animation-delay: 0.08s; }
  .keycap:nth-child(5) { animation-delay: 0.16s; }
  .keycap:nth-child(7) { animation-delay: 0.24s; }
  .keycap .glyph { font-size: 15px; }
  .keycap.word .glyph { font-size: 12px; }
  .keycap .name { font-size: 7.5px; letter-spacing: 0.8px; color: var(--muted); text-transform: uppercase; }
  @keyframes keypress {
    0%, 12%, 100% { transform: translateY(0); border-bottom-width: 4px; }
    6% { transform: translateY(3px); border-bottom-width: 1px; }
  }
  @media (prefers-reduced-motion: reduce) { .keycap { animation: none; } }
  .combo-text { flex: 1; min-width: 240px; }
  .combo-title { margin: 0 0 4px; font-size: 13px; color: var(--fg); }
  .combo-title em { font-style: normal; color: var(--accent); }
  .combo-label { font-size: 11px; padding: 1px 6px; border-radius: 4px; background: var(--surface-2); color: var(--fg); white-space: nowrap; }
  .combo-sub { margin: 0; font-size: 11px; line-height: 1.55; color: var(--muted); }
  .combo-state { display: inline-flex; align-items: center; gap: 6px; margin: 6px 0 0; font-size: 9.5px; letter-spacing: 0.6px; color: var(--faint); }
  .combo-state.ok { color: var(--ok-fg); }
  .combo-state.warn { color: var(--warn-fg); }
  .combo-note { margin: 4px 0 0; font-size: 10.5px; line-height: 1.5; color: var(--warn-fg); }
  .led { width: 6px; height: 6px; border-radius: 50%; background: var(--led-idle); }
  .led.ok { background: var(--led-ok); box-shadow: 0 0 6px var(--led-ok); }
  .combo-sub code, .exits code { font-family: var(--font-mono); font-size: 10.5px; padding: 1px 5px; border-radius: 4px; background: var(--surface-2); color: var(--fg); }

  /* --- The ladder and its console --------------------------------------- */
  .walk { display: grid; grid-template-columns: minmax(240px, 5fr) minmax(280px, 6fr); gap: 16px; }
  @media (max-width: 760px) { .walk { grid-template-columns: 1fr; } }
  .ladder { display: flex; flex-direction: column; gap: 6px; margin: 0; padding: 0; list-style: none; }
  .rung { display: flex; gap: 10px; padding: 8px 10px; border: 1px solid var(--node-border); border-radius: var(--radius-control); background: color-mix(in srgb, var(--surface) 70%, transparent); transition: border-color 0.25s, background 0.25s; }
  .rung.active { border-color: color-mix(in srgb, var(--accent) 60%, var(--node-border)); background: color-mix(in srgb, var(--accent) 9%, var(--surface)); }
  .rung.done { opacity: 0.75; }
  .rung-no { flex: none; display: grid; place-items: center; width: 20px; height: 20px; border-radius: 50%; font-size: 10px; background: var(--surface-2); color: var(--muted); }
  .rung.active .rung-no { background: var(--accent); color: var(--accent-fg); }
  .rung.done .rung-no { background: var(--ok-bg); color: var(--ok-fg); }
  .rung-body { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .rung-cmd { font-family: var(--font-mono); font-size: 12px; color: var(--fg); }
  .rung-what { font-size: 11px; line-height: 1.45; color: var(--muted); }

  .term-wrap { display: flex; flex-direction: column; border: 1px solid var(--node-border); border-radius: var(--radius-panel); background: #0b0d10; overflow: hidden; min-height: 200px; }
  .term-bar { display: flex; align-items: center; gap: 10px; padding: 6px 10px; border-bottom: 1px solid var(--node-divider); background: color-mix(in srgb, var(--surface) 60%, #0b0d10); }
  .term-dots { display: inline-flex; gap: 4px; }
  .term-dots i { width: 8px; height: 8px; border-radius: 50%; background: var(--node-border); }
  .term-dots i:first-child { background: var(--led-err); }
  .term-title { display: inline-flex; align-items: center; gap: 6px; flex: 1; font-size: 9.5px; letter-spacing: 0.6px; color: var(--muted); }
  .term-play { display: inline-flex; align-items: center; gap: 5px; padding: 3px 8px; border: 1px solid var(--node-border); border-radius: var(--radius-detail); background: transparent; color: var(--muted); font-size: 9.5px; letter-spacing: 0.5px; cursor: pointer; }
  .term-play:hover { color: var(--fg); border-color: var(--accent); }
  .live { width: 6px; height: 6px; border-radius: 50%; background: var(--accent); animation: pulse 1s ease-in-out infinite; }
  @keyframes pulse { 50% { opacity: 0.3; } }
  .term { flex: 1; max-height: 230px; overflow-y: auto; padding: 10px 12px; font: 11px/1.65 var(--font-mono); white-space: pre-wrap; overflow-wrap: anywhere; }
  .term-idle { color: var(--faint); font-style: italic; }
  .tl:empty::before { content: '\00a0'; }
  .tl.in { color: var(--fg); margin-top: 4px; }
  .tl.out { color: var(--muted); }
  .tl.released { margin-top: 8px; color: var(--ok-fg); }
  .ps { color: var(--accent); margin-right: 7px; }
  .caret { display: inline-block; width: 6px; height: 12px; margin-left: 2px; vertical-align: -2px; background: var(--accent); animation: blink 1s steps(2) infinite; }
  @keyframes blink { 50% { opacity: 0; } }

  /* --- The exits that need no console ----------------------------------- */
  .exits { display: flex; flex-direction: column; gap: 8px; padding: 12px 14px; border: 1px dashed var(--node-border); border-radius: var(--radius-control); }
  .exits-label { font-size: 9px; letter-spacing: 1.2px; color: var(--accent); }
  .exits ul { display: grid; grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); gap: 10px 18px; margin: 0; padding: 0; list-style: none; }
  .exits li { display: flex; gap: 9px; font-size: 11px; line-height: 1.5; color: var(--muted); }
  .exits li b { color: var(--fg); font-weight: 500; }
  .exit-icon { flex: none; display: grid; place-items: center; width: 22px; height: 22px; border-radius: var(--radius-detail); background: var(--surface-2); color: var(--accent); }

  .try { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; }
  .try .ghost { display: inline-flex; align-items: center; gap: 6px; }
  .try-hint { font-size: 11px; color: var(--faint); }
</style>
