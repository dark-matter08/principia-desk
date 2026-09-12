<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { api, onEvent, type ExecutionRun, type ExecutionLogLine } from '../ipc';
  import { app } from '../stores.svelte';
  import { RefreshCw, Radio, CircleCheck, CircleX, Loader, ScrollText } from 'lucide-svelte';

  let runs = $state<ExecutionRun[]>([]);
  let selected = $state<string | 'live'>('live');
  let lines = $state<ExecutionLogLine[]>([]);
  let loading = $state(false);
  let error = $state('');
  let follow = $state(true);
  let console_: HTMLDivElement | undefined = $state();
  /** Ticks once a second while a run is in flight, so its duration counts up. */
  let now = $state(Date.now());
  const live = $derived(runs.filter((r) => r.outcome === 'running'));
  $effect(() => {
    if (!live.length) return;
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(timer);
  });
  const ACTIVITY: Record<string, string> = { lesson: 'preparing a lesson', draft: 'drafting the curriculum', review: 'reading the draft back', sources: 'fetching the sources', bank: 'writing the question bank', fix: 'changing the draft' };
  const doing = (run: ExecutionRun) => ACTIVITY[run.activity] ?? run.activity;
  /** A run's length: to now while it runs, to its last line once it ended. */
  function elapsed(run: ExecutionRun) {
    const start = new Date(run.started_at).getTime();
    const end = run.outcome === 'running' ? now : new Date(run.last_at).getTime();
    return duration(Math.max(0, end - start));
  }
  function duration(ms: number) {
    const total = Math.floor(ms / 1000);
    const h = Math.floor(total / 3600), m = Math.floor((total % 3600) / 60), s = total % 60;
    return h ? `${h}h ${String(m).padStart(2, '0')}m ${String(s).padStart(2, '0')}s` : m ? `${m}m ${String(s).padStart(2, '0')}s` : `${s}s`;
  }
  /** Runs are re-read when a line arrives, at most every few seconds, so a finished run settles without a refresh. */
  let runsReadAt = 0;
  function loadRunsSoon() { if (Date.now() - runsReadAt > 3000) { runsReadAt = Date.now(); void loadRuns(); } }

  async function loadRuns() {
    try { runs = await api.listExecutionRuns(); } catch (e) { error = String(e); }
  }
  async function loadLines() {
    loading = true;
    try {
      lines = selected === 'live' ? await api.getRecentExecutionLog(400) : await api.getExecutionLog(selected);
      error = '';
      if (follow) { await tick(); console_?.scrollTo({ top: console_.scrollHeight }); }
    } catch (e) { error = String(e); } finally { loading = false; }
  }
  onMount(() => {
    void loadRuns(); void loadLines();
    const subscriptions = [
      onEvent('gen:log', () => { loadRunsSoon(); if (selected === 'live' || current?.outcome === 'running') void loadLines(); }),
      onEvent('preparation:state', () => { void loadRuns(); void loadLines(); }),
      onEvent('classroom:state', () => { void loadRuns(); if (selected === 'live') void loadLines(); }),
    ];
    return () => { void Promise.all(subscriptions).then((offs) => offs.forEach((off) => off())); };
  });
  function pick(id: string | 'live') { selected = id; follow = id === 'live'; void loadLines(); }
  function clock(value: string) { const d = new Date(value); return Number.isNaN(d.getTime()) ? value : d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit', second: '2-digit' }); }
  function day(value: string) { const d = new Date(value); return Number.isNaN(d.getTime()) ? '' : d.toLocaleDateString(undefined, { weekday: 'short', month: 'short', day: 'numeric' }); }
  function tone(line: string) {
    const l = line.toLowerCase();
    if (l.includes('failed') || l.includes('error') || l.includes('did not pass') || l.includes('refused')) return 'bad';
    if (l.includes('finished') || l.includes('ready') || l.includes('published') || l.includes('passed')) return 'ok';
    if (l.includes('correct') || l.includes('audit') || l.includes('retry')) return 'warn';
    return '';
  }
  const current = $derived(selected === 'live' ? null : runs.find((r) => r.run_id === selected) ?? null);
</script>

<div class="logs-page page-frame">
  <header>
    <div class="meta-label">EXECUTION LOG · WHAT THE TUTOR DID</div>
    <h1>Logs</h1>
    <p>Every line the runner reported while preparing a lesson, kept for thirty days so a slow or failed preparation can be read back.</p>
  </header>
  <div class="layout">
    <aside class="runs" aria-label="Preparation runs">
      <button class="run" class:on={selected === 'live'} class:alive={live.length > 0} onclick={() => pick('live')}>
        <span class="run-head">{#if live.length}<span class="beacon" aria-hidden="true"></span>{:else}<Radio size={13} />{/if} Live tail</span>
        {#if live.length}
          <small class="ticking">{live[0].label} · {doing(live[0])} · <b class="mono">{elapsed(live[0])}</b>{#if live.length > 1} · and {live.length - 1} more{/if}</small>
        {:else}
          <small>{app.preparingClass ? `Preparing ${app.preparingClass.label}…` : 'Latest lines from any run'}</small>
        {/if}
      </button>
      <div class="runs-head"><span class="mono">RUNS · {runs.length}</span><button class="icon" aria-label="Refresh runs" onclick={() => { void loadRuns(); void loadLines(); }}><RefreshCw size={12} /></button></div>
      <div class="run-list">
        {#each runs as run (run.run_id)}
          <button class="run" class:on={selected === run.run_id} class:alive={run.outcome === 'running'} onclick={() => pick(run.run_id)}>
            <span class="run-head">
              {#if run.outcome === 'ready' || run.outcome === 'done'}<CircleCheck size={13} class="ok" />{:else if run.outcome === 'failed'}<CircleX size={13} class="bad" />{:else if run.outcome === 'running'}<span class="spinner" aria-hidden="true"><Loader size={13} /></span>{:else}<ScrollText size={13} />{/if}
              {run.label}
              {#if run.activity !== 'lesson'}<span class="activity mono">{run.activity}</span>{/if}
            </span>
            {#if run.outcome === 'running'}
              <small class="ticking">{doing(run)} · <b class="mono">{elapsed(run)}</b> · since {clock(run.started_at)} · {run.lines} lines</small>
            {:else}
              <small>{day(run.started_at)} · {clock(run.started_at)} → {clock(run.last_at)} · {elapsed(run)} · {run.lines} lines · <em class={`state-${run.outcome}`}>{run.outcome}</em></small>
            {/if}
          </button>
        {/each}
        {#if !runs.length}<p class="empty">No runs yet. The first lesson preparation appears here as it happens.</p>{/if}
      </div>
    </aside>
    <section class="console-wrap" aria-label="Log lines">
      <div class="console-head">
        <span class="mono head-label">
          {#if selected === 'live'}LIVE · LAST 400 LINES{:else}RUN · {current?.label ?? selected} · {current?.outcome ?? ''}{/if}
          {#if (selected === 'live' && live.length) || current?.outcome === 'running'}
            {@const run = selected === 'live' ? live[0] : current}
            {#if run}<span class="head-live"><span class="beacon" aria-hidden="true"></span>{doing(run)} · <b>{elapsed(run)}</b></span>{/if}
          {/if}
        </span>
        <label class="follow"><input type="checkbox" bind:checked={follow} /> follow</label>
      </div>
      {#if current?.error}<p class="run-error"><strong>Outcome:</strong> {current.error}</p>{/if}
      <div class="console" bind:this={console_}>
        {#each lines as line (line.id)}
          <div class={`line ${tone(line.line)}`}><span class="at">{clock(line.at)}</span>{#if selected === 'live' && line.course_id}<span class="who">{line.course_id}</span>{/if}<span class="text">{line.line}</span></div>
        {/each}
        {#if (selected === 'live' && live.length) || current?.outcome === 'running'}
          {@const run = selected === 'live' ? live[0] : current}
          {#if run}
            <!-- The runner answers only when it finishes; this line keeps time while it works. -->
            <div class="line working"><span class="at">{clock(new Date(now).toISOString())}</span>{#if selected === 'live'}<span class="who">{run.course_id ?? run.label}</span>{/if}<span class="text"><span class="cursor" aria-hidden="true"></span>{doing(run)} · {elapsed(run)}<span class="dots" aria-hidden="true"></span></span></div>
          {/if}
        {/if}
        {#if !lines.length && !loading}<p class="empty">{selected === 'live' ? 'Nothing logged yet.' : 'This run has no lines.'}</p>{/if}
        {#if loading && !lines.length}<p class="empty">Reading…</p>{/if}
      </div>
      {#if error}<p class="run-error" role="alert">{error}</p>{/if}
    </section>
  </div>
</div>

<style>
  .logs-page { display: flex; flex-direction: column; flex: 1; min-height: 600px; gap: 14px; }
  header p { font-size: 13px; color: var(--muted); margin: 0; max-width: 60ch; } h1 { font-size: 28px; margin: 8px 0 6px; }
  .layout { display: grid; grid-template-columns: 300px minmax(0, 1fr); gap: 14px; flex: 1; min-height: 0; }
  .runs { display: flex; flex-direction: column; gap: 8px; min-height: 0; }
  .runs-head { display: flex; align-items: center; justify-content: space-between; padding: 6px 4px 0; } .runs-head .mono { font-size: 9px; letter-spacing: .7px; color: var(--faint); }
  .icon { display: grid; place-items: center; padding: 5px; border: 1px solid var(--node-border); border-radius: var(--radius-detail); background: var(--bg); color: var(--muted); cursor: pointer; }
  .run-list { display: flex; flex-direction: column; gap: 6px; overflow-y: auto; min-height: 0; }
  .run { display: grid; gap: 4px; text-align: left; padding: 10px 12px; border: 1px solid var(--node-border); border-radius: var(--radius-control); background: var(--surface); color: var(--fg); cursor: pointer; }
  .run.on { border-color: var(--violet); background: var(--surface-2); }
  .run-head { display: flex; align-items: center; gap: 7px; font-size: 12px; } .run small { font-size: 10px; color: var(--muted); line-height: 1.5; }
  .run em { font-style: normal; } .state-ready, .state-done { color: var(--led-ok); } .state-failed { color: var(--led-err); } .state-running { color: var(--accent); }
  .run.alive { border-color: color-mix(in srgb, var(--accent) 55%, var(--node-border)); box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent) 12%, transparent); animation: breathe 2.4s ease-in-out infinite; }
  @keyframes breathe { 50% { box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent) 12%, transparent), 0 0 18px color-mix(in srgb, var(--accent) 18%, transparent); } }
  .activity { margin-left: auto; padding: 1px 6px; border-radius: 999px; background: var(--surface-2); color: var(--faint); font-size: 8.5px; letter-spacing: 0.8px; text-transform: uppercase; }
  .ticking { color: var(--muted); } .ticking b { font-weight: 500; color: var(--accent); font-variant-numeric: tabular-nums; }
  /* A running run breathes; its beacon pulses; its loader turns. */
  .beacon { position: relative; display: inline-block; width: 8px; height: 8px; border-radius: 50%; background: var(--accent); box-shadow: 0 0 8px var(--accent); }
  .beacon::after { content: ''; position: absolute; inset: -4px; border-radius: 50%; border: 1px solid var(--accent); animation: ring 1.6s ease-out infinite; }
  @keyframes ring { from { transform: scale(0.5); opacity: 0.9; } to { transform: scale(1.8); opacity: 0; } }
  .spinner { display: inline-grid; place-items: center; color: var(--accent); animation: turn 1.1s linear infinite; } @keyframes turn { to { transform: rotate(360deg); } }
  .head-label { display: inline-flex; align-items: center; gap: 12px; }
  .head-live { display: inline-flex; align-items: center; gap: 8px; color: var(--accent); letter-spacing: 0.4px; text-transform: none; } .head-live b { font-weight: 500; font-variant-numeric: tabular-nums; }
  .line.working .text { color: var(--accent); }
  .cursor { display: inline-block; width: 7px; height: 12px; margin: 0 6px -2px 0; background: var(--accent); animation: blink 1s steps(2, start) infinite; } @keyframes blink { to { visibility: hidden; } }
  .dots::after { content: ''; animation: dots 1.5s steps(4, end) infinite; } @keyframes dots { 0% { content: ''; } 25% { content: '.'; } 50% { content: '..'; } 75% { content: '...'; } }
  .run :global(.ok) { color: var(--led-ok); } .run :global(.bad) { color: var(--led-err); }
  .console-wrap { display: flex; flex-direction: column; min-height: 0; border: 1px solid var(--node-border); border-radius: var(--radius-panel); background: #0b0910; overflow: hidden; }
  .console-head { display: flex; align-items: center; justify-content: space-between; padding: 8px 12px; border-bottom: 1px solid var(--node-border); } .console-head .mono { font-size: 9px; letter-spacing: .7px; color: var(--faint); }
  .follow { display: flex; align-items: center; gap: 6px; font-size: 10px; color: var(--muted); }
  .console { flex: 1; min-height: 0; overflow: auto; padding: 10px 12px; font: 11px/1.7 var(--font-mono); color: var(--fg); }
  .line { display: grid; grid-template-columns: minmax(70px, max-content) auto minmax(0, 1fr); gap: 10px; white-space: pre-wrap; overflow-wrap: anywhere; animation: arrive 0.35s ease-out; }
  .line .at { white-space: nowrap; font-variant-numeric: tabular-nums; }
  @keyframes arrive { from { opacity: 0; transform: translateY(3px); } }
  .line .at { color: var(--faint); } .line .who { color: var(--violet-fg); } .line .text { min-width: 0; }
  .line.ok .text { color: var(--led-ok); } .line.bad .text { color: var(--led-err); } .line.warn .text { color: var(--accent); }
  .run-error { margin: 0; padding: 8px 12px; font-size: 11px; color: var(--led-err); border-bottom: 1px solid var(--node-border); }
  .empty { color: var(--muted); font-size: 11px; padding: 10px 0; }
  @media (max-width: 860px) { .layout { grid-template-columns: 1fr; } .runs { max-height: 220px; } }
</style>
