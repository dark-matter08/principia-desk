<script lang="ts">
  import { api, type AppointmentView, type ClassroomSlotView, type ClassroomSubjectId, type StudyPulse as StudyPulseView } from '../ipc';
  import { app } from '../stores.svelte';
  import { formatClassCountdown, formatClassTime, nextScheduledClass } from '../next-class';
  import NodeCard from '../components/NodeCard.svelte';
  import StatusLED from '../components/StatusLED.svelte';
  import ProgressRing from '../features/today/ProgressRing.svelte';
  import StudyPulse from '../features/today/StudyPulse.svelte';
  import { ArrowRight, BookOpen, Clock, Play, Pause, Activity, ScrollText, Sparkles } from 'lucide-svelte';
  import { dayStart } from '../features/classes/schedule-conflicts';
  let now = $state(new Date()), busy = $state(false);
  /** The habit view; loaded once per visit and again after anything that completes a lesson. */
  let pulse = $state<StudyPulseView | null>(null);
  let pulseFor = '';
  $effect(() => {
    // Refresh when the desk's state changes (a lesson finished, a class activated).
    const key = `${app.state?.active_classroom_sessions.length ?? 0}:${(app.state?.classroom_programs ?? []).map(p => `${p.subject_id}${p.enabled ? 1 : 0}${p.progress}`).join(',')}`;
    if (key === pulseFor) return;
    pulseFor = key;
    api.getStudyPulse().then((view) => (pulse = view)).catch(() => {});
  });
  /** How much of the last day before the next class has run down: the ring empties as the class approaches. */
  const RING_WINDOW = 24 * 60 * 60 * 1000;
  const ringValue = $derived.by(() => {
    if (!next) return 0;
    if (next.due) return 1;
    const remaining = next.at.getTime() - now.getTime();
    return Math.min(1, Math.max(0, 1 - remaining / RING_WINDOW));
  });
  const greeting = $derived.by(() => {
    const hour = now.getHours();
    return hour < 5 ? 'Late night' : hour < 12 ? 'Good morning' : hour < 18 ? 'Good afternoon' : 'Good evening';
  });
  async function snooze(minutes: number) {
    if (!alarm || busy) return; busy = true;
    try { await api.snoozeAlarm(alarm.occurrence_id, minutes); await app.refresh(); } catch (e) { app.error = String(e); } finally { busy = false; }
  }
  const blocks = $derived((app.state?.blocks ?? []).filter(b => b.next !== 'done'));
  const BREAK_MINUTES = 5;
  /** When each block last had no session open, so the suggested break counts down from then. */
  let breakSince = $state<Record<string, number>>({});
  $effect(() => {
    for (const block of blocks) {
      if (block.in_session) { if (breakSince[block.occurrence_id]) breakSince = { ...breakSince, [block.occurrence_id]: 0 }; }
      else if (!breakSince[block.occurrence_id]) breakSince = { ...breakSince, [block.occurrence_id]: Date.now() };
    }
  });
  function breakLeft(id: string) { const since = breakSince[id]; if (!since) return 0; return Math.max(0, BREAK_MINUTES * 60 - Math.floor((now.getTime() - since) / 1000)); }
  function mmss(seconds: number) { return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`; }
  function hoursLeft(minutes: number) { return minutes >= 60 ? `${Math.floor(minutes / 60)} h ${minutes % 60} min` : `${minutes} min`; }
  async function continueBlock(block: { course_id: ClassroomSubjectId; occurrence_id: string }) {
    if (busy) return; busy = true;
    try { await app.startClass(block.course_id, null, false, block.occurrence_id); } finally { busy = false; }
  }
  async function retrieval(block: { course_id: ClassroomSubjectId; occurrence_id: string }) {
    if (busy) return; busy = true;
    try { await app.startReview(block.course_id, block.occurrence_id); } finally { busy = false; }
  }
  async function endBlock(id: string) {
    if (busy) return; busy = true;
    try { await api.endBlock(id); await app.refresh(); } catch (e) { app.error = String(e); } finally { busy = false; }
  }
  function untilClock(value: string) { const d = new Date(value); return Number.isNaN(d.getTime()) ? '' : d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' }); }
  const next = $derived(nextScheduledClass(app.state, now));
  const resumable = $derived(app.state?.active_classroom_sessions ?? []);
  const activeClasses = $derived((app.state?.classroom_programs ?? []).filter(p => p.enabled));
  const slots = $derived((app.state?.classroom_slots ?? []).filter(s => s.enabled && activeClasses.some(p => p.subject_id === s.subject_id)));
  const dueSlots = $derived(slots.filter(s => s.owed));
  const alarm = $derived(app.state?.alarm ?? null);
  const alarmSlot = $derived(alarm ? slots.find(s => s.occurrence_id === alarm.occurrence_id) ?? null : null);
  const missed = $derived((app.state?.appointments ?? []).filter(a => a.disposition === 'missed'));
  /** Engineering classes with spaced review due and no saved session in the way. */
  const reviews = $derived(activeClasses.filter(p => p.kind === 'engineering' && p.review_due > 0 && !resumable.some(s => s.subject_id === p.subject_id)));
  const upcoming = $derived(slots.filter(s => !s.owed).sort((a,b) => a.next_fire_at.localeCompare(b.next_fire_at)).slice(0,4));
  /** The class whose focused session holds the desk; other classes wait. */
  const held = $derived(app.state?.focus?.course_id ?? null);
  const heldLabel = $derived(held ? (app.state?.classroom_programs.find(p => p.subject_id === held)?.label ?? held) : '');
  const heldElsewhere = (subject: string) => held !== null && held !== subject;
  $effect(() => { const timer = setInterval(() => now = new Date(),1000); return () => clearInterval(timer); });
  async function pause() {
    if (busy) return; busy = true;
    try { if (app.state?.schedule_paused) await api.resumeSchedule(); else await api.pauseSchedule(); await app.refresh(); } catch (cause) { app.error = String(cause); } finally { busy = false; }
  }
  async function start(slot: ClassroomSlotView) {
    if (busy) return; busy = true;
    try { const program = activeClasses.find(p => p.subject_id === slot.subject_id); await app.startClass(slot.subject_id,slot.id,program?.completed ?? false,slot.occurrence_id); } finally { busy = false; }
  }
  async function makeUp(appointment: AppointmentView) {
    if (busy) return; busy = true;
    try { const program = activeClasses.find(p => p.subject_id === appointment.course_id); await app.startClass(appointment.course_id as ClassroomSubjectId,null,program?.completed ?? false,appointment.id); } finally { busy = false; }
  }
  async function skipAppointment(appointment: AppointmentView) {
    if (busy) return; busy = true;
    try { await api.skipAppointment(appointment.id); await app.refresh(); } catch (cause) { app.error = String(cause); } finally { busy = false; }
  }
  /** When a due rule started today: the day's own time when it has one. */
  function todayStart(slot: ClassroomSlotView) {
    const weekday = now.getDay() === 0 ? 7 : now.getDay();
    const [hour, minute] = dayStart(slot.starts, weekday, slot.hour, slot.minute);
    return `${String(hour).padStart(2,'0')}:${String(minute).padStart(2,'0')}`;
  }
  function appointment(slot: ClassroomSlotView) {
    // The next firing carries the day's own start time, which may differ from the rule's.
    const date = new Date(slot.next_fire_at);
    return Number.isNaN(date.getTime()) ? 'Time unavailable' : date.toLocaleDateString(undefined,{weekday:'short',month:'short',day:'numeric'}) + ' · ' + `${String(date.getHours()).padStart(2,'0')}:${String(date.getMinutes()).padStart(2,'0')}`;
  }
</script>
<div class="today page-frame">
  {#if app.state?.enforcement_disarmed}<p class="recovery mono" role="status">ENFORCEMENT DISARMED · a principia-unlock release token is present.</p>{/if}
  {#if alarm}
    <section class="alarm" class:snoozed={!!alarm.snoozed_until} role="alert" aria-live="assertive" aria-label="Study alarm">
      <div class="alarm-copy">
        <span class="mono">{alarm.snoozed_until ? `SNOOZED · RINGS AGAIN AT ${untilClock(alarm.snoozed_until)}` : alarm.readiness === 'ready' ? 'STUDY ALARM · RINGING' : alarm.readiness === 'preparing' ? 'STUDY TIME · LESSON PREPARING' : 'STUDY TIME · PREPARATION FAILED'}</span>
        <strong>{alarm.label} is due{alarm.queued ? `, and ${alarm.queued} more ${alarm.queued === 1 ? 'is' : 'are'} waiting` : ''}.</strong>
        {#if alarm.readiness === 'preparing'}
          <p>Your tutor is still preparing the lesson, so the alarm is holding. It rings the moment the lesson is ready, and starting then is immediate.</p>
        {:else if alarm.readiness === 'failed'}
          <p>The tutor could not prepare this lesson{alarm.error ? `: ${alarm.error.slice(0, 160)}${alarm.error.length > 160 ? '…' : ''}` : ''}. Retry prepares it again; the alarm rings once it is ready.</p>
        {:else}
          <p>The alarm stops when you start the lesson. You can break the glass once you are in it, but not before.</p>
        {/if}
      </div>
      <div class="alarm-actions">
        {#if alarmSlot && alarm.readiness !== 'preparing'}<button class="cta mono-cta" disabled={busy || !!app.preparingClass} onclick={() => start(alarmSlot)}><Play size={13} /> {alarm.readiness === 'failed' ? `Retry ${alarm.label}` : `Start ${alarm.label}`}</button>{/if}
        {#if alarm.readiness === 'preparing'}<span class="mono preparing-note">preparing…</span>{/if}
        {#if !alarm.snoozed_until && alarm.readiness === 'ready'}{#each [5, 10, 15] as minutes (minutes)}<button class="ghost mono-ghost" disabled={busy} onclick={() => snooze(minutes)}>Snooze {minutes} min</button>{/each}{/if}
      </div>
    </section>
  {/if}
  {#each blocks as block (block.occurrence_id)}
    <section class="block" aria-label={`Study block: ${block.label}`}>
      <div class="block-head">
        <span class="mono">STUDY BLOCK · {block.label} · {hoursLeft(block.duration_minutes)}</span>
        <span class="mono muted">{block.lessons_completed} lesson{block.lessons_completed === 1 ? '' : 's'} done · {hoursLeft(block.remaining_minutes)} left</span>
      </div>
      <div class="block-track" role="progressbar" aria-label="Block time used" aria-valuemin="0" aria-valuemax={block.duration_minutes} aria-valuenow={block.elapsed_minutes}><i style:width={`${Math.min(100, (block.elapsed_minutes / Math.max(1, block.duration_minutes)) * 100)}%`}></i></div>
      {#if block.in_session}
        <p>A lesson from this block is open. Resume it from the saved sessions below.</p>
      {:else if block.next === 'topic'}
        <div class="block-row">
          <div>
            <strong>{breakLeft(block.occurrence_id) > 0 ? `Break · ${mmss(breakLeft(block.occurrence_id))}` : 'Ready for the next topic'}</strong>
            <p>The next topic on your route, sized to {block.next_minutes} minutes. A topic always ends today, so only whole topics start.</p>
          </div>
          <button class="cta mono-cta" disabled={busy || !!app.preparingClass} onclick={() => continueBlock(block)}><Play size={13} /> {breakLeft(block.occurrence_id) > 0 ? 'Skip the break, start now' : 'Start next topic'}</button>
        </div>
      {:else if (app.state?.classroom_programs ?? []).find(p => p.subject_id === block.course_id)?.kind === 'language'}
        <p><strong>{block.next_minutes} minutes left.</strong> Not enough for a whole pass, and language retrieval sessions are not built yet, so end the block when you are ready.</p>
      {:else}
        <div class="block-row">
          <div>
            <strong>{block.next_minutes} minutes left: retrieval practice</strong>
            <p>Not enough for a whole topic, so the rest of the block goes to earlier topics, which spaced repetition needs anyway.</p>
          </div>
          <button class="cta mono-cta" disabled={busy || !!app.preparingClass} onclick={() => retrieval(block)}><BookOpen size={13} /> Start retrieval</button>
        </div>
      {/if}
      {#if !block.in_session}<button class="ghost mono-ghost end" disabled={busy} onclick={() => endBlock(block.occurrence_id)}>End block for today</button>{/if}
    </section>
  {/each}
  <section class="hero" class:due={!!next?.due} aria-labelledby="today-title">
    <div class="hero-copy">
      <div class="meta-label">{greeting.toUpperCase()} · {now.toLocaleDateString(undefined,{weekday:'long',month:'short',day:'numeric'})}</div>
      <h1 id="today-title">{resumable.length ? 'Your desk is waiting' : dueSlots.length ? 'Time for your next class' : next ? 'Next class on the way' : 'Make room for learning'}</h1>
      <p class="sub">{resumable.length ? resumable[0].title : dueSlots.length ? `${dueSlots.length} class appointment${dueSlots.length === 1 ? ' is' : 's are'} due now.` : next ? `${next.label} · ${formatClassTime(next,now)}.` : 'Your class schedules bring the next session here.'}</p>
      <div class="hero-actions">
        {#if resumable.length}
          <button class="cta mono-cta" disabled={heldElsewhere(resumable[0].subject_id)} onclick={() => app.resumeClass(resumable[0].subject_id)}><Play size={14} />Resume {resumable[0].label}</button>
        {:else if dueSlots.length}
          <button class="cta mono-cta" disabled={busy || !!app.preparingClass || heldElsewhere(dueSlots[0].subject_id)} onclick={() => start(dueSlots[0])}><Play size={14} />Start {dueSlots[0].label}</button>
        {:else}
          <button class="cta mono-cta" onclick={() => app.openClass()}><BookOpen size={14} />Open classes</button>
        {/if}
        <button class="bracket" onclick={() => app.navigate('progress')}><span class="bracket-well"><ScrollText size={12} /></span>Progress ledger</button>
        <button class="bracket" onclick={pause} disabled={busy}><span class="bracket-well">{#if app.state?.schedule_paused}<Play size={12} />{:else}<Pause size={12} />{/if}</span>{app.state?.schedule_paused ? 'Resume appointments' : 'Pause appointments'}</button>
      </div>
      <div class="hero-facts mono">
        <span><i class="dot violet"></i>{activeClasses.length} active {activeClasses.length === 1 ? 'class' : 'classes'}</span>
        <span><i class="dot teal"></i>{slots.length} study {slots.length === 1 ? 'time' : 'times'}</span>
        {#if pulse?.streak}<span><i class="dot amber"></i>{pulse.streak}-day streak</span>{/if}
      </div>
      {#if held}<p class="held mono" role="status">A focused {heldLabel} session holds the desk. Other classes wait until it finishes.</p>{/if}
    </div>
    <div class="hero-ring">
      <div class="glow" aria-hidden="true"></div>
      <ProgressRing value={app.state?.schedule_paused ? 0 : ringValue} size={206} stroke={7} pulse={!!next?.due} label={next ? `${next.label} in ${formatClassCountdown(next,now)}` : 'No upcoming class'}>
        <span class="ring-eyebrow mono">{app.state?.schedule_paused ? 'PAUSED' : next?.due ? 'DUE NOW' : 'NEXT CLASS'}</span>
        {@const countdown = next ? formatClassCountdown(next, now) : '—:—:—'}
        <span class="ring-count mono" class:idle={!next} class:long={countdown.length > 8}>{countdown}</span>
        <span class="ring-label">{next?.label ?? 'No upcoming appointments'}</span>
        <span class="ring-when mono">{app.state?.schedule_paused ? 'appointments paused' : formatClassTime(next,now)}</span>
      </ProgressRing>
    </div>
  </section>

  <div class="desk">
  {#if pulse}
    <section class="pulse-section" aria-label="Study habit">
      <div class="section-head"><span class="eyebrow mono"><Activity size={11} /> STUDY PULSE · 26 WEEKS</span><span class="mono muted">{pulse.week_sessions} {pulse.week_sessions === 1 ? 'lesson' : 'lessons'} this week</span></div>
      <StudyPulse {pulse} />
    </section>
  {/if}
  <div class="desk-main">
  {#if resumable.length}
    <section class="saved" aria-label="Saved sessions"><NodeCard Icon={Play} name="saved-sessions" badge="resumable" badgeTone="teal">
      {#each resumable as session}{@const pending = session.runtime === 'study' && ['planned', 'preparing'].includes(session.lifecycle)}<div class="study-row"><div><strong>{session.label}</strong><p>{session.title}{pending ? ' · preparation did not finish' : ''}</p></div>{#if pending}<button class="ghost mono-ghost" disabled={busy || !!app.preparingClass || heldElsewhere(session.subject_id)} onclick={() => app.startClass(session.subject_id)}>Retry<ArrowRight size={12} /></button>{:else}<button class="ghost mono-ghost" disabled={heldElsewhere(session.subject_id)} onclick={() => app.resumeClass(session.subject_id)}>Resume<ArrowRight size={12} /></button>{/if}</div>{/each}
    </NodeCard></section>
  {/if}
  <div class="overview">
    <section aria-label="Class agenda"><NodeCard Icon={Clock} name="class-agenda" badge={dueSlots.length ? dueSlots.length+' due' : missed.length ? missed.length+' missed' : 'upcoming'} badgeTone={dueSlots.length || missed.length ? 'amber' : 'teal'}>
      {#each dueSlots as slot}<div class="study-row"><div><strong>{slot.label}</strong><p class="mono">{todayStart(slot)} · due</p></div><button class="ghost mono-ghost" disabled={busy || !!app.preparingClass || resumable.some(s => s.subject_id === slot.subject_id) || heldElsewhere(slot.subject_id)} onclick={() => start(slot)}>Start class</button></div>{/each}
      {#each reviews as program (program.subject_id)}<div class="study-row"><div><strong>{program.label}</strong><p class="mono">{program.review_due} {program.review_due === 1 ? 'topic' : 'topics'} · review due</p></div><button class="ghost mono-ghost" disabled={busy || !!app.preparingClass || heldElsewhere(program.subject_id)} onclick={() => app.startReview(program.subject_id)}>Start review<ArrowRight size={12} /></button></div>{/each}
      {#each missed as appointment (appointment.id)}<div class="study-row"><div><strong>{appointment.label}</strong><p class="mono">{appointment.local_date} · {appointment.local_time} · missed</p></div><div class="row-actions"><button class="ghost mono-ghost" disabled={busy || !!app.preparingClass || resumable.some(s => s.subject_id === appointment.course_id) || !activeClasses.some(p => p.subject_id === appointment.course_id) || heldElsewhere(appointment.course_id)} onclick={() => makeUp(appointment)}>Make up</button><button class="ghost mono-ghost" disabled={busy} onclick={() => skipAppointment(appointment)}>Skip</button></div></div>{/each}
      {#if !app.state?.schedule_paused}{#each upcoming as slot}<button class="agenda-link" onclick={() => app.openClass(slot.subject_id,'schedule')}><span><strong>{slot.label}</strong><small>{appointment(slot)}</small></span><ArrowRight size={13} /></button>{/each}{/if}
      {#if !dueSlots.length && !missed.length && (!upcoming.length || app.state?.schedule_paused)}<div class="queue-state"><StatusLED tone="ok" /><p>{app.state?.schedule_paused ? 'Appointments are paused.' : 'Add study times inside a class to build your week.'}</p></div>{/if}
      <button class="ghost mono-ghost" onclick={() => app.openClass(null,'schedule')}>Manage class schedules<ArrowRight size={12} /></button>
    </NodeCard></section>
    <section aria-label="Your classes"><NodeCard Icon={BookOpen} name="class-registry" badge={activeClasses.length+' active'} badgeTone="violet">
      {#each activeClasses as program (program.subject_id)}<button class="class-link" onclick={() => app.openClass(program.subject_id)}><ProgressRing value={program.progress} size={40} stroke={4} label={`${Math.round(program.progress * 100)}% of ${program.label}`}><span class="ring-code mono">{program.short_code}</span></ProgressRing><span>{program.label}<small>{program.progress_label}</small>{#if program.route?.next}<small class="next-topic"><Sparkles size={10} /> Next: {program.route.next.title}</small>{/if}</span><ArrowRight size={12} /></button>{:else}<p class="hint">Choose your first class and add its study times.</p>{/each}
      <button class="ghost mono-ghost" onclick={() => app.openClass()}>Browse courses<ArrowRight size={12} /></button>
    </NodeCard></section>
  </div>
  </div>
  </div>
</div>
<style>
  .held { margin: 10px 0 0; font-size: 11px; color: var(--led-warn); }
  .today { padding-bottom: 40px; }
  /* The hero: what to do next on the left, the time until it on the right. */
  .hero { position: relative; display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 24px 40px; align-items: center; padding: 14px 0 30px; animation: rise 520ms cubic-bezier(0.22, 1, 0.36, 1) backwards; }
  .hero-copy { min-width: 0; }
  h1 { font-size: 38px; line-height: 1.08; margin: 10px 0 8px; letter-spacing: -0.01em; }
  .sub { color: var(--muted); font-size: 14px; margin: 0 0 20px; max-width: 560px; }
  .hero-actions { display: flex; gap: 10px; flex-wrap: wrap; align-items: center; }
  .hero-actions .cta { display: inline-flex; align-items: center; gap: 8px; }
  .hero-facts { display: flex; gap: 16px; flex-wrap: wrap; margin-top: 18px; font-size: 9.5px; letter-spacing: 0.6px; color: var(--muted); }
  .hero-facts span { display: inline-flex; align-items: center; gap: 6px; }
  .dot { width: 6px; height: 6px; border-radius: 50%; display: inline-block; } .dot.violet { background: var(--violet-fg, var(--muted)); } .dot.teal { background: var(--ok-fg); } .dot.amber { background: var(--accent); }
  .hero-ring { position: relative; display: grid; place-items: center; padding: 10px; }
  .glow { position: absolute; width: 260px; height: 260px; border-radius: 50%; background: radial-gradient(circle, color-mix(in srgb, var(--accent) 22%, transparent), transparent 62%); filter: blur(6px); animation: breathe 5s ease-in-out infinite; }
  .hero.due .glow { animation-duration: 1.6s; background: radial-gradient(circle, color-mix(in srgb, var(--accent) 40%, transparent), transparent 64%); }
  .ring-eyebrow { font-size: 8.5px; letter-spacing: 1.4px; color: var(--accent); }
  .ring-count { font-size: 30px; line-height: 1; color: var(--fg); font-variant-numeric: tabular-nums; white-space: nowrap; margin: 4px 0 2px; } .ring-count.idle { color: var(--faint); }
  /* With days in front, the figure shrinks to stay inside the ring. */
  .ring-count.long { font-size: 22px; letter-spacing: -0.3px; margin: 6px 0 4px; }
  .ring-label { font-size: 13px; font-weight: 500; color: var(--fg); max-width: 150px; line-height: 1.25; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ring-when { font-size: 9px; color: var(--muted); margin-top: 3px; }
  /* `backwards`, not `both`: a finished transform, even an identity one,
     would trap the heat map's fixed-position tip inside this section. */
  /* Below the hero: the habit first, one row across the whole desk (the
     figures beside the heat map), then what there is to do. */
  .desk { display: grid; grid-template-columns: minmax(0, 1fr); gap: 26px; align-items: start; }
  .desk-main { min-width: 0; }
  .pulse-section { min-width: 0; animation: rise 520ms cubic-bezier(0.22, 1, 0.36, 1) 120ms backwards; }
  .section-head { display: flex; align-items: center; justify-content: space-between; gap: 12px; margin-bottom: 12px; } .eyebrow { display: inline-flex; align-items: center; gap: 6px; color: var(--accent); font-size: 9px; letter-spacing: 1.4px; } .muted { font-size: 9.5px; color: var(--muted); }
  .ring-code { font-size: 9px; color: var(--accent); }
  .next-topic { display: flex; align-items: center; gap: 5px; color: var(--accent); }
  @keyframes rise { from { opacity: 0; transform: translateY(10px); } to { opacity: 1; transform: none; } }
  @keyframes breathe { 0%, 100% { transform: scale(0.94); opacity: 0.7; } 50% { transform: scale(1.06); opacity: 1; } }
  @media (prefers-reduced-motion: reduce) { .hero, .pulse-section { animation: none; } .glow { animation: none; } }
  @media (max-width: 760px) { .hero { grid-template-columns: 1fr; justify-items: center; text-align: center; } .hero-actions, .hero-facts { justify-content: center; } }
  .overview { display: grid; grid-template-columns: repeat(auto-fit, minmax(300px, 1fr)); gap: 20px; align-items: stretch; }
  /* The two panels share a row: each card fills its column, and the footer
     control sits on the same baseline in both whatever the rows above it. */
  .overview > section { display: flex; min-width: 0; } .overview > section :global(.node) { flex: 1; display: flex; flex-direction: column; } .overview > section :global(.node-body) { flex: 1; display: flex; flex-direction: column; } .overview > section :global(.node-body > .ghost:last-child) { margin-top: auto; align-self: flex-start; }
  .saved { margin: 0 0 22px; } .study-row { display: flex; justify-content: space-between; align-items: center; gap: 16px; padding: 12px 0; border-bottom: 1px dashed var(--node-divider); margin-bottom: 8px; } .study-row strong { font-size: 13px; font-weight: 500; } .study-row p { font-size: 12px; color: var(--muted); margin: 5px 0 0; } .hint { font-size: 12px; color: var(--muted); margin: 5px 0 12px; } .queue-state { display: flex; gap: 9px; align-items: center; font-size: 12px; color: var(--muted); margin-bottom: 12px; }
  .class-link,.agenda-link { display: flex; align-items: center; gap: 12px; text-align: left; width: 100%; background: none; border: 0; border-bottom: 1px dashed var(--node-divider); padding: 12px 0; margin-bottom: 8px; color: var(--fg); font: 13px var(--font-body); cursor: pointer; } .class-link > span:nth-child(2),.agenda-link > span { flex: 1; min-width: 0; } small { color: var(--muted); display: block; font-size: 11px; margin-top: 5px; } .agenda-link strong { font-weight: 500; }
  .row-actions { display: flex; gap: 6px; flex-shrink: 0; }
  .alarm { display: flex; align-items: center; justify-content: space-between; gap: 16px; flex-wrap: wrap; padding: 14px 16px; border: 1px solid var(--accent); border-radius: var(--radius-panel); background: color-mix(in srgb, var(--accent) 10%, var(--surface)); }
  .alarm.snoozed { border-style: dashed; background: var(--surface); }
  .preparing-note { font-size: 10px; color: var(--muted); align-self: center; }
  .alarm-copy { display: grid; gap: 4px; min-width: 0; } .alarm-copy .mono { font-size: 9px; letter-spacing: .7px; color: var(--accent); } .alarm.snoozed .alarm-copy .mono { color: var(--muted); }
  .alarm-copy strong { font-size: 14px; } .alarm-copy p { margin: 0; font-size: 11px; color: var(--muted); line-height: 1.55; }
  .alarm-actions { display: flex; gap: 8px; flex-wrap: wrap; }
  .block { display: grid; gap: 10px; padding: 14px 16px; border: 1px solid var(--violet); border-radius: var(--radius-panel); background: var(--surface); }
  .block-head { display: flex; justify-content: space-between; gap: 12px; flex-wrap: wrap; } .block-head .mono { font-size: 9px; letter-spacing: .7px; color: var(--violet-fg); } .block-head .muted { color: var(--muted); }
  .block-track { height: 4px; border-radius: 2px; background: var(--bg); overflow: hidden; } .block-track i { display: block; height: 100%; background: var(--violet-fg); }
  .block p { margin: 0; font-size: 11px; color: var(--muted); line-height: 1.55; }
  .block-row { display: flex; align-items: center; justify-content: space-between; gap: 16px; flex-wrap: wrap; } .block-row strong { display: block; font-size: 13px; margin-bottom: 3px; }
  .block .end { justify-self: end; }
  .recovery { color: var(--bad-fg); border: 1px dashed var(--led-err); background: var(--bad-bg); padding: 9px 12px; border-radius: var(--radius-control); font-size: 11px; }
  @media(max-width:620px) { .today { padding: 22px 18px; } .overview { grid-template-columns: 1fr; } h1 { font-size: 28px; } .study-row { flex-wrap: wrap; } }
</style>
