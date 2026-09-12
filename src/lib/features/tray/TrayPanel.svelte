<script lang="ts">
  import { onMount } from 'svelte';
  import { api, onEvent, type AppStateView, type DashboardView } from '../../ipc';
  import { nextScheduledClass, formatClassCountdown, formatClassTime } from '../../next-class';
  import { Play, Pause, Bell, CalendarClock, Flame, Layers, Sparkles, ChevronRight, Power, Settings2 } from 'lucide-svelte';
  import mark from '../../../../docs/logo-mark.svg';

  let desk = $state<AppStateView | null>(null);
  let dashboard = $state<DashboardView | null>(null);
  let now = $state(new Date());
  let busy = $state(false);
  let error = $state('');
  let card = $state<HTMLDivElement | null>(null);
  /** The card's motion: it drops in from under the menu bar and lifts away when the panel closes. */
  let phase = $state<'in' | 'shown' | 'out'>('in');
  let phaseTimer: ReturnType<typeof setTimeout> | null = null;
  function enter() {
    if (phaseTimer) clearTimeout(phaseTimer);
    phase = 'in';
    // Two frames so the start state is painted before the transition runs.
    requestAnimationFrame(() => requestAnimationFrame(() => (phase = 'shown')));
  }
  function leave() {
    if (phaseTimer) clearTimeout(phaseTimer);
    phase = 'out';
    // Back to the start state once the window is hidden, ready for the next entrance.
    phaseTimer = setTimeout(() => (phase = 'in'), 400);
  }

  async function load() {
    try {
      const [s, d] = await Promise.all([api.getAppState(), api.getDashboard().catch(() => null)]);
      desk = s; dashboard = d; error = '';
    } catch (e) { error = String(e); }
  }
  onMount(() => {
    void load();
    const tick = setInterval(() => (now = new Date()), 15_000);
    const subscriptions = ['alarm:state', 'classroom:state', 'classroom:owed'].map((name) => onEvent(name, () => void load()));
    // Each time the panel is shown the card drops in; when it closes, it lifts away first.
    subscriptions.push(onEvent('tray:refresh', () => { enter(); void load(); }));
    subscriptions.push(onEvent('tray:leave', () => leave()));
    // The window carries no dead space: report the card's height as it changes.
    const observer = new ResizeObserver(() => { if (card) void api.sizeTrayPanel(card.offsetHeight + 24); });
    if (card) observer.observe(card);
    enter();
    return () => { clearInterval(tick); observer.disconnect(); void Promise.all(subscriptions).then((offs) => offs.forEach((off) => off())); };
  });

  const alarm = $derived(desk?.alarm ?? null);
  const paused = $derived(!!desk?.schedule_paused);
  const next = $derived(nextScheduledClass(desk, now));
  const active = $derived((desk?.classroom_programs ?? []).filter((p) => p.enabled));
  const today = $derived(desk?.appointments ?? []);
  const done = $derived(today.filter((a) => a.disposition === 'completed').length);
  const due = $derived(today.filter((a) => a.disposition === 'due').length);
  const ahead = $derived(today.filter((a) => a.disposition === 'scheduled').length);
  const week = $derived((dashboard?.activity ?? []).slice(-7).reduce((sum, day) => sum + day.completed, 0));
  const locked = $derived(!!desk?.focus?.locked);

  /** One line and one colour for the whole desk, the way a health panel leads. */
  const health = $derived.by(() => {
    if (alarm && !alarm.snoozed_until && alarm.readiness === 'preparing') return { word: 'Preparing', tone: 'warn', line: `${alarm.label} is due. The lesson is being written; the alarm rings when it is ready.` };
    if (alarm && !alarm.snoozed_until && alarm.readiness === 'failed') return { word: 'Needs a retry', tone: 'due', line: `${alarm.label} is due but its lesson could not be prepared.` };
    if (alarm && !alarm.snoozed_until) return { word: 'Due now', tone: 'due', line: `${alarm.label} is waiting. The alarm stops when you start.` };
    if (alarm?.snoozed_until) return { word: 'Snoozed', tone: 'warn', line: `${alarm.label} rings again at ${clock(alarm.snoozed_until)}.` };
    if (locked) return { word: 'In session', tone: 'ok', line: 'A focused lesson holds the desk.' };
    if (paused) return { word: 'Paused', tone: 'muted', line: 'No appointments will fire until you resume.' };
    if (next) return { word: 'On track', tone: 'ok', line: `${next.label} ${formatClassCountdown(next, now)}.` };
    if (!active.length) return { word: 'Idle', tone: 'muted', line: 'No active classes yet. Open the desk to enrol.' };
    return { word: 'Clear', tone: 'ok', line: 'Nothing due. Add a study time to keep the streak.' };
  });

  function clock(value: string) {
    const d = new Date(value);
    return Number.isNaN(d.getTime()) ? '' : d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
  }
  async function run(action: () => Promise<unknown>) {
    if (busy) return; busy = true; error = '';
    try { await action(); await load(); } catch (e) { error = String(e); } finally { busy = false; }
  }
  const start = () => alarm && run(() => api.startFromTray(alarm.occurrence_id));
  const snooze = (minutes: number) => alarm && run(() => api.snoozeAlarm(alarm.occurrence_id, minutes));
  const open = () => run(() => api.showDesk());
  const togglePause = () => run(() => (paused ? api.resumeSchedule() : api.pauseSchedule()));
  const quit = () => run(() => api.quitDesk());
</script>

<div class="panel" class:due={health.tone === 'due'} class:entering={phase === 'in'} class:leaving={phase === 'out'} bind:this={card}>
  <header class="head">
    <div>
      <p class="eyebrow mono">PRINCIPIA DESK</p>
      <h1>Desk: <span class={`tone-${health.tone}`}>{health.word}</span></h1>
      <p class="line">{health.line}</p>
    </div>
    <img class="mark" src={mark} alt="" />
  </header>

  {#if alarm}
    <section class="alarm" class:snoozed={!!alarm.snoozed_until} aria-label="Study alarm">
      <div class="alarm-top"><Bell size={15} /><strong>{alarm.label}</strong><span class="mono">{alarm.snoozed_until ? `until ${clock(alarm.snoozed_until)}` : alarm.readiness === 'ready' ? 'ringing' : alarm.readiness === 'preparing' ? 'preparing…' : 'failed'}</span></div>
      <p>{alarm.readiness === 'preparing' ? 'The tutor is writing the lesson. The alarm rings when it is ready.' : alarm.readiness === 'failed' ? 'Preparation failed. Retry prepares it again.' : alarm.queued ? `${alarm.queued} more ${alarm.queued === 1 ? 'appointment is' : 'appointments are'} waiting behind it.` : 'Only starting the lesson ends this.'}</p>
      <div class="alarm-actions">
        {#if alarm.readiness !== 'preparing'}<button class="cta mono-cta primary" disabled={busy} onclick={start}><Play size={13} /> {alarm.readiness === 'failed' ? 'Retry' : 'Start'} {alarm.label}</button>{/if}
        {#if !alarm.snoozed_until && alarm.readiness === 'ready'}<div class="snoozes">{#each [5, 10, 15] as minutes (minutes)}<button disabled={busy} onclick={() => snooze(minutes)}>{minutes}m</button>{/each}</div>{/if}
      </div>
    </section>
  {/if}

  <section class="tiles" aria-label="Desk at a glance">
    <div class="tile">
      <span class="tile-head"><CalendarClock size={14} /> Next class</span>
      {#if paused}<strong class="tone-muted">Paused</strong><small>resume to schedule</small>
      {:else if next}<strong>{next.label}</strong><small>{formatClassTime(next, now)} · {formatClassCountdown(next, now)}</small>
      {:else}<strong class="tone-muted">None</strong><small>no study times set</small>{/if}
    </div>
    <div class="tile">
      <span class="tile-head"><Layers size={14} /> Today</span>
      <strong>{done}<em>/{today.length}</em></strong>
      <small>{due ? `${due} due` : ahead ? `${ahead} ahead` : today.length ? 'all done' : 'nothing scheduled'}</small>
    </div>
    <div class="tile">
      <span class="tile-head"><Flame size={14} /> Streak</span>
      <strong>{dashboard?.streak ?? 0}<em> days</em></strong>
      <small>{week} session{week === 1 ? '' : 's'} this week</small>
    </div>
    <div class="tile">
      <span class="tile-head"><Sparkles size={14} /> Tutor</span>
      <strong class="clip">{desk?.agent ?? '—'}</strong>
      <small class="clip">{desk?.model || 'runner default'} · {active.length} active class{active.length === 1 ? '' : 'es'}</small>
    </div>
  </section>

  {#if today.length}
    <section class="schedule" aria-label="Today's appointments">
      <p class="eyebrow mono">TODAY</p>
      <ul>
        {#each today.slice(0, 6) as appointment (appointment.id)}
          <li><span class="mono time">{appointment.local_time}</span><span class="who">{appointment.label}</span><span class={`badge state-${appointment.disposition}`}>{appointment.disposition}</span></li>
        {/each}
      </ul>
    </section>
  {/if}

  {#if error}<p class="error" role="alert">{error}</p>{/if}

  <footer class="foot">
    <button class="ghost mono-ghost small foot-btn" disabled={busy} onclick={open}><ChevronRight size={14} /> Open desk</button>
    <button class="ghost mono-ghost small foot-btn" disabled={busy} onclick={togglePause}>{#if paused}<Play size={13} /> Resume{:else}<Pause size={13} /> Pause{/if}</button>
    <button class="foot-btn icon" title={alarm && !alarm.snoozed_until && alarm.readiness === 'ready' ? 'The alarm is ringing' : locked ? 'A focused session holds the desk' : 'Quit'} aria-label="Quit Principia Desk" disabled={busy || (!!alarm && !alarm.snoozed_until && alarm.readiness === 'ready') || locked} onclick={quit}><Power size={14} /></button>
    <button class="foot-btn icon" title="Open settings" aria-label="Open settings" disabled={busy} onclick={open}><Settings2 size={14} /></button>
  </footer>
</div>

<style>
  /* The card drops from under the menu bar and settles; on the way out it lifts and fades, quicker. */
  .panel { box-sizing: border-box; width: 356px; margin: 12px; display: grid; gap: 14px; align-content: start; padding: 18px 18px 14px; border-radius: 18px; border: 1px solid var(--node-border); background: color-mix(in srgb, var(--bg) 96%, black); color: var(--fg); font-family: var(--font-body); box-shadow: 0 14px 40px rgba(0, 0, 0, .5); opacity: 1; transform: translateY(0) scale(1); transform-origin: 50% 0; transition: opacity .2s ease-out, transform .3s cubic-bezier(.16, 1, .3, 1); will-change: opacity, transform; }
  .panel.entering { opacity: 0; transform: translateY(-18px) scale(.97); transition: none; }
  .panel.leaving { opacity: 0; transform: translateY(-10px) scale(.985); transition: opacity .15s ease-in, transform .17s ease-in; }
  @media (prefers-reduced-motion: reduce) { .panel, .panel.entering, .panel.leaving { transition: none; opacity: 1; transform: none; } }
  .panel.due { border-color: var(--accent); }
  .head { display: flex; align-items: flex-start; justify-content: space-between; gap: 12px; }
  .eyebrow { font-size: 9px; letter-spacing: .8px; color: var(--faint); margin: 0 0 4px; }
  h1 { font: 22px var(--font-display); margin: 0 0 4px; }
  .line { margin: 0; font-size: 12px; line-height: 1.5; color: var(--muted); }
  .mark { width: 44px; height: 44px; flex-shrink: 0; opacity: .9; }
  .tone-due { color: var(--accent); } .tone-warn { color: var(--led-warn, var(--accent)); } .tone-ok { color: var(--led-ok); } .tone-muted { color: var(--muted); }

  .alarm { border: 1px solid var(--accent); border-radius: 14px; padding: 12px 14px; background: color-mix(in srgb, var(--accent) 12%, var(--surface)); display: grid; gap: 8px; }
  .alarm.snoozed { border-style: dashed; background: var(--surface); }
  .alarm-top { display: flex; align-items: center; gap: 8px; } .alarm-top strong { font-size: 14px; } .alarm-top .mono { margin-left: auto; font-size: 10px; color: var(--accent); } .alarm.snoozed .alarm-top .mono { color: var(--muted); }
  .alarm p { margin: 0; font-size: 11px; color: var(--muted); line-height: 1.5; }
  .alarm-actions { display: flex; align-items: center; gap: 8px; }
  .primary { flex: 1; display: flex; align-items: center; justify-content: center; gap: 7px; }
  .snoozes { display: flex; gap: 4px; } .snoozes button { padding: 8px 9px; border: 1px solid var(--node-border); border-radius: 9px; background: var(--bg); color: var(--fg); font: 11px var(--font-mono); cursor: pointer; }

  .tiles { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }
  .tile { display: grid; gap: 5px; padding: 12px 13px; border-radius: 14px; background: var(--surface); border: 1px solid var(--node-border); min-width: 0; }
  .tile-head { display: flex; align-items: center; gap: 6px; font-size: 11px; color: var(--muted); }
  .tile strong { font: 20px var(--font-display); line-height: 1.1; } .tile strong em { font-style: normal; font-size: 13px; color: var(--muted); }
  .tile small { font-size: 10px; color: var(--muted); line-height: 1.4; }
  .clip { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  .schedule ul { list-style: none; margin: 6px 0 0; padding: 0; display: grid; gap: 4px; }
  .schedule li { display: flex; align-items: center; gap: 10px; padding: 7px 10px; border-radius: 10px; background: var(--surface); font-size: 12px; }
  .time { font-size: 11px; color: var(--muted); } .who { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .badge { font: 9px var(--font-mono); padding: 2px 6px; border-radius: 6px; border: 1px solid var(--node-border); color: var(--muted); text-transform: uppercase; letter-spacing: .5px; }
  .state-due { color: var(--accent); border-color: var(--accent); } .state-completed { color: var(--led-ok); border-color: var(--led-ok); } .state-missed { color: var(--led-err); border-color: var(--led-err); }

  .error { margin: 0; font-size: 11px; color: var(--led-err); }
  .foot { display: flex; align-items: center; gap: 8px; padding-top: 4px; border-top: 1px solid var(--node-border); }
  .foot-btn { display: flex; align-items: center; gap: 6px; }
  .foot-btn.icon { margin-left: auto; padding: 8px; border: 1px solid transparent; border-radius: 10px; background: transparent; color: var(--fg); cursor: pointer; } .foot-btn.icon + .foot-btn.icon { margin-left: 0; }
  .foot-btn.icon:hover:not(:disabled) { background: var(--surface); border-color: var(--node-border); } .foot-btn.icon:disabled { opacity: .35; cursor: default; }
</style>
