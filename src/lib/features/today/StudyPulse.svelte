<script lang="ts">
  import type { StudyPulse } from '$lib/ipc';
  import AnimatedNumber from './AnimatedNumber.svelte';
  import { Flame, CalendarCheck, BookOpenCheck, Target, Trophy } from 'lucide-svelte';

  let { pulse }: { pulse: StudyPulse } = $props();

  /** Weeks as columns, seven rows each, Monday at the top. */
  const weeks = $derived.by(() => {
    const columns: StudyPulse['days'][] = [];
    for (let i = 0; i < pulse.days.length; i += 7) columns.push(pulse.days.slice(i, i + 7));
    return columns;
  });
  const busiest = $derived(Math.max(1, ...pulse.days.map((day) => day.minutes)));
  /** Intensity 0..4, by minutes against the busiest day so a light week still shows shape. */
  function level(minutes: number): number {
    if (minutes <= 0) return 0;
    const share = minutes / busiest;
    return share > 0.75 ? 4 : share > 0.5 ? 3 : share > 0.25 ? 2 : 1;
  }
  /** Month labels over the first column that starts a new month. */
  const months = $derived.by(() => {
    const labels: { column: number; label: string }[] = [];
    let last = '';
    weeks.forEach((week, column) => {
      const month = week[0]?.date.slice(0, 7) ?? '';
      if (month && month !== last) {
        labels.push({ column, label: new Date(`${week[0].date}T12:00:00`).toLocaleDateString(undefined, { month: 'short' }) });
        last = month;
      }
    });
    return labels;
  });
  const streakDates = $derived.by(() => {
    const set = new Set<string>();
    const today = new Date(`${pulse.today}T12:00:00`);
    // The streak may have started yesterday; mark its days back from the latest studied one.
    const days = [...pulse.days].reverse();
    let index = days.findIndex((day) => day.completed > 0);
    if (index < 0 || pulse.streak === 0) return set;
    const latest = new Date(`${days[index].date}T12:00:00`);
    if ((today.getTime() - latest.getTime()) / 86400000 > 1) return set;
    for (let count = 0; count < pulse.streak && index < days.length; index += 1) {
      if (days[index].completed > 0) { set.add(days[index].date); count += 1; } else break;
    }
    return set;
  });
  const weekShare = $derived(pulse.week_target_minutes > 0 ? Math.min(1, pulse.week_minutes / pulse.week_target_minutes) : 0);
  let tip = $state<{ day: StudyPulse['days'][number]; x: number; y: number; below: boolean } | null>(null);
  function show(day: StudyPulse['days'][number], event: MouseEvent | FocusEvent) {
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    const below = rect.top < 72;
    tip = {
      day,
      x: Math.min(Math.max(rect.left + rect.width / 2, 120), window.innerWidth - 120),
      y: below ? rect.bottom + 8 : rect.top - 8,
      below,
    };
  }
  function dayLabel(date: string) {
    return new Date(`${date}T12:00:00`).toLocaleDateString(undefined, { weekday: 'short', month: 'short', day: 'numeric' });
  }
  function hours(minutes: number) {
    return minutes >= 60 ? `${(minutes / 60).toFixed(minutes % 60 ? 1 : 0)} h` : `${minutes} min`;
  }
</script>

<section class="pulse" aria-label="Study pulse">
  <div class="stats">
    <div class="stat streak" class:alight={pulse.streak > 0}>
      <span class="flame" aria-hidden="true"><Flame size={22} /></span>
      <div><strong><AnimatedNumber value={pulse.streak} /></strong><span class="mono">day streak</span></div>
      <small class="mono">best {pulse.longest_streak}</small>
    </div>
    <div class="stat"><span class="glyph"><BookOpenCheck size={16} /></span><div><strong><AnimatedNumber value={pulse.completed_sessions} /></strong><span class="mono">lessons done</span></div></div>
    <div class="stat"><span class="glyph"><CalendarCheck size={16} /></span><div><strong><AnimatedNumber value={pulse.study_days} /></strong><span class="mono">study days</span></div></div>
    <div class="stat"><span class="glyph"><Trophy size={16} /></span><div><strong>{#if pulse.pass_rate == null}—{:else}<AnimatedNumber value={pulse.pass_rate * 100} suffix="%" />{/if}</strong><span class="mono">checks passed</span></div></div>
    <div class="stat this-week">
      <span class="glyph"><Target size={16} /></span>
      <div class="week-body">
        <strong><AnimatedNumber value={pulse.week_minutes} /><em class="mono"> / {pulse.week_target_minutes || '—'} min</em></strong>
        <span class="mono">this week · {Math.round(weekShare * 100)}% of target</span>
        <div class="week-track" role="progressbar" aria-label="This week's study minutes against the target" aria-valuemin="0" aria-valuemax={pulse.week_target_minutes} aria-valuenow={pulse.week_minutes}><i style:width={`${weekShare * 100}%`}></i></div>
      </div>
    </div>
  </div>

  <div class="map">
    <div class="months mono" aria-hidden="true">
      {#each months as month (month.column)}<span style:grid-column={month.column + 2}>{month.label}</span>{/each}
    </div>
    <div class="grid">
      <div class="weekdays mono" aria-hidden="true"><span>Mon</span><span></span><span>Wed</span><span></span><span>Fri</span><span></span><span></span></div>
      {#each weeks as week, column (week[0]?.date ?? column)}
        <div class="week">
          {#each week as day, row (day.date)}
            <button
              type="button"
              class={`cell level-${level(day.minutes)}`}
              class:today={day.date === pulse.today}
              class:streak={streakDates.has(day.date)}
              style:animation-delay={`${Math.min(900, (column * 7 + row) * 4)}ms`}
              aria-label={`${dayLabel(day.date)}: ${day.completed ? `${day.completed} lesson${day.completed === 1 ? '' : 's'}, ${hours(day.minutes)}` : 'no study'}`}
              onmouseenter={(event) => show(day, event)}
              onfocus={(event) => show(day, event)}
              onmouseleave={() => (tip = null)}
              onblur={() => (tip = null)}
            ></button>
          {/each}
        </div>
      {/each}
    </div>
    {#if tip}
      <div class="tip" class:below={tip.below} style:left={`${tip.x}px`} style:top={`${tip.y}px`} role="tooltip">
        <strong>{dayLabel(tip.day.date)}</strong>
        <span>{tip.day.completed ? `${tip.day.completed} lesson${tip.day.completed === 1 ? '' : 's'} · ${hours(tip.day.minutes)}${tip.day.classes.length ? ` · ${tip.day.classes.join(' ')}` : ''}` : 'No study'}</span>
      </div>
    {/if}
    <div class="legend mono" aria-hidden="true"><span>less</span><i class="cell level-0"></i><i class="cell level-1"></i><i class="cell level-2"></i><i class="cell level-3"></i><i class="cell level-4"></i><span>more</span></div>
  </div>
</section>

<style>
  /* One row: the figures fill the width the heat map leaves, and both
     columns stand the same height. They stack on a narrow window. */
  .pulse { display: grid; grid-template-columns: minmax(0, 1fr); gap: 12px; align-items: stretch; }
  @media (min-width: 1100px) { .pulse { grid-template-columns: minmax(0, 1fr) auto; } }
  .stats { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); grid-auto-rows: 1fr; gap: 8px; min-width: 0; }
  .this-week { grid-column: span 2; }
  .stat { display: flex; align-items: center; gap: 10px; padding: 10px 12px; border: 1px solid var(--node-border); border-radius: var(--radius-panel); background: var(--node-bg); min-width: 0; animation: rise 500ms cubic-bezier(0.22, 1, 0.36, 1) backwards; }
  .stat:nth-child(2) { animation-delay: 60ms; } .stat:nth-child(3) { animation-delay: 120ms; } .stat:nth-child(4) { animation-delay: 180ms; } .stat:nth-child(5) { animation-delay: 240ms; }
  .stat strong { display: block; font-size: 20px; line-height: 1.1; color: var(--fg); font-weight: 500; }
  .stat strong em { font-style: normal; font-size: 11px; color: var(--muted); }
  .stat > div > span { display: block; margin-top: 4px; font-size: 9px; letter-spacing: 1px; text-transform: uppercase; color: var(--muted); }
  .stat small { margin-left: auto; font-size: 9px; color: var(--faint); white-space: nowrap; }
  .glyph { display: grid; place-items: center; width: 30px; height: 30px; flex: none; border-radius: var(--radius-control); background: var(--surface-2); color: var(--accent); }
  .streak { border-color: color-mix(in srgb, var(--accent) 35%, var(--node-border)); background: linear-gradient(135deg, color-mix(in srgb, var(--accent) 12%, var(--node-bg)), var(--node-bg) 70%); }
  .flame { display: grid; place-items: center; width: 30px; height: 30px; flex: none; border-radius: var(--radius-control); background: color-mix(in srgb, var(--accent) 18%, var(--surface-2)); color: var(--faint); }
  .streak.alight .flame { color: var(--accent); animation: flicker 1.6s ease-in-out infinite; filter: drop-shadow(0 0 8px color-mix(in srgb, var(--accent) 70%, transparent)); }
  .this-week { grid-column: span 2; }
  .week-body { flex: 1; min-width: 0; }
  .week-track { position: relative; height: 6px; margin-top: 10px; border-radius: 3px; background: var(--surface-2); overflow: hidden; }
  .week-track i { position: absolute; inset: 0 auto 0 0; border-radius: 3px; background: linear-gradient(90deg, var(--accent), var(--ok-fg)); transition: width 900ms cubic-bezier(0.22, 1, 0.36, 1); }

  /* Squares stay square and small, the size a contribution graph is read at;
     the card is as wide as the half year, no wider. */
  .map { position: relative; display: flex; flex-direction: column; justify-content: center; padding: 12px 16px 10px; border: 1px solid var(--node-border); border-radius: var(--radius-panel); background: var(--node-bg); width: fit-content; max-width: 100%; overflow-x: auto; }
  .months { display: grid; grid-template-columns: 26px repeat(26, 14px); gap: 0 3px; margin-bottom: 6px; font-size: 8.5px; color: var(--faint); height: 12px; }
  .months span { white-space: nowrap; }
  .grid { display: grid; grid-template-columns: 26px repeat(26, 14px); gap: 3px; }
  .weekdays { display: grid; grid-template-rows: repeat(7, 14px); gap: 3px; font-size: 8px; color: var(--faint); align-items: center; }
  .week { display: grid; grid-template-rows: repeat(7, 14px); gap: 3px; }
  .cell { width: 14px; height: 14px; padding: 0; border: 0; border-radius: 3px; background: var(--surface-2); cursor: default; animation: pop 420ms cubic-bezier(0.22, 1, 0.36, 1) both; transition: transform 140ms cubic-bezier(0.22, 1, 0.36, 1), box-shadow 140ms ease, filter 140ms ease; }
  /* A square lifts under the pointer so the day you are reading about stands out. */
  button.cell:hover, button.cell:focus-visible { transform: scale(1.35); z-index: 3; filter: brightness(1.25); box-shadow: 0 0 0 2px var(--node-bg), 0 0 0 3.5px color-mix(in srgb, var(--accent) 70%, transparent), 0 6px 16px rgba(0, 0, 0, 0.45); }
  button.cell.level-0:hover, button.cell.level-0:focus-visible { background: color-mix(in srgb, var(--accent) 18%, var(--surface-2)); }
  button.cell:focus-visible { outline: none; }
  .cell.level-1 { background: color-mix(in srgb, var(--accent) 28%, var(--surface-2)); }
  .cell.level-2 { background: color-mix(in srgb, var(--accent) 50%, var(--surface-2)); }
  .cell.level-3 { background: color-mix(in srgb, var(--accent) 75%, var(--surface-2)); }
  .cell.level-4 { background: var(--accent); box-shadow: 0 0 8px color-mix(in srgb, var(--accent) 55%, transparent); }
  .cell.streak { box-shadow: inset 0 0 0 1.5px color-mix(in srgb, var(--fg) 60%, transparent); }
  .cell.today { outline: 1.5px solid var(--fg); outline-offset: 1px; }
  .tip { position: fixed; transform: translate(-50%, -100%); z-index: 40; padding: 7px 10px; border: 1px solid var(--node-border); border-radius: var(--radius-control); background: var(--surface); box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35); white-space: nowrap; pointer-events: none; animation: tip-in 140ms cubic-bezier(0.22, 1, 0.36, 1) both; }
  .tip.below { transform: translate(-50%, 0); }
  .tip::after { content: ''; position: absolute; left: 50%; bottom: -5px; width: 8px; height: 8px; transform: translateX(-50%) rotate(45deg); background: var(--surface); border-right: 1px solid var(--node-border); border-bottom: 1px solid var(--node-border); }
  .tip.below::after { bottom: auto; top: -5px; border: 0; border-left: 1px solid var(--node-border); border-top: 1px solid var(--node-border); }
  @keyframes tip-in { from { opacity: 0; translate: 0 4px; } to { opacity: 1; translate: 0 0; } }
  .tip strong { display: block; font-size: 11px; font-weight: 500; color: var(--fg); }
  .tip span { display: block; margin-top: 2px; font-size: 10px; color: var(--muted); }
  .legend { display: flex; align-items: center; gap: 4px; justify-content: flex-end; margin-top: 10px; font-size: 8.5px; color: var(--faint); }
  .legend .cell { width: 10px; height: 10px; animation: none; }
  .legend span { margin: 0 4px; }

  @keyframes rise { from { opacity: 0; transform: translateY(8px); } to { opacity: 1; transform: none; } }
  @keyframes pop { from { opacity: 0; transform: scale(0.4); } to { opacity: 1; transform: scale(1); } }
  @keyframes flicker { 0%, 100% { transform: scale(1) rotate(-2deg); opacity: 1; } 50% { transform: scale(1.12) rotate(3deg); opacity: 0.85; } }
  @media (prefers-reduced-motion: reduce) { .stat, .cell, .tip { animation: none; } .streak.alight .flame { animation: none; } .week-track i, .cell { transition: none; } button.cell:hover { transform: none; } }
</style>
