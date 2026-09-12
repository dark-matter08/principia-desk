<script lang="ts">
  import { untrack } from 'svelte';
  import { api, type ClassroomProgramView, type CurriculumMapView, type FocusArea } from '../../ipc';
  import type { AcceptedPath, PathChange } from '../../contracts/classes';
  import { app } from '../../stores.svelte';
  import { courseDefinition, isCustomCourse } from '../../catalog';
  import { LayoutDashboard, Settings2, Compass, BookOpen, CalendarClock, Play, ArrowRight, PenLine, FileJson } from 'lucide-svelte';
  import PracticeBank from './PracticeBank.svelte';
  import CourseGlyph from '../../components/CourseGlyph.svelte';
  import CurriculumMap from '../../components/CurriculumMap.svelte';
  import ClassSettings from './ClassSettings.svelte';
  import ClassSchedule from './ClassSchedule.svelte';
  import EnrollmentSetup from './EnrollmentSetup.svelte';
  import PathPreview from './PathPreview.svelte';
  import UnitChallenge from './UnitChallenge.svelte';
  import type { ClassTab } from './class-navigation';
  let { program, initialTab = 'overview', requested = null, ontabchange }: { program: ClassroomProgramView; initialTab?: ClassTab; /** A tab asked for by name after mount, with when, so the same ask twice still lands. */ requested?: { tab: ClassTab; at: number } | null; ontabchange?: (tab: ClassTab) => void } = $props();
  const uid = $props.id();
  const course = $derived(courseDefinition(program.subject_id));
  /** A class the learner made: it can be edited as a new version and exported as a file. */
  const custom = $derived(isCustomCourse(program.subject_id));
  async function exportClass() {
    try { const result = await api.exportCustomCourse(program.subject_id); app.notify(`Class file written: ${result.file_name}`, { label: 'Show file', run: () => void api.revealExport(result.path) }); }
    catch (cause) { app.error = String(cause); }
  }
  const preparing = $derived(app.preparingClass !== null);
  const active = $derived(app.state?.active_classroom_sessions.find(s => s.subject_id === program.subject_id));
  const slots = $derived(app.state?.classroom_slots.filter(s => s.subject_id === program.subject_id) ?? []);
  /** What the study times run to; the figure is theirs, not a setting. */
  const sessionLength = $derived.by(() => {
    const minutes = slots.filter(s => s.enabled).flatMap(s => s.weekdays.map(day => s.durations[String(day)] ?? program.session_minutes));
    if (!minutes.length) return { value: String(program.session_minutes), unit: 'min', note: 'until a study time is set' };
    const low = Math.min(...minutes), high = Math.max(...minutes);
    return low === high
      ? { value: String(low), unit: 'min', note: `from your study times · ${program.agent} / ${program.model}` }
      : { value: `${low}–${high}`, unit: 'min', note: `varies by day · ${program.agent} / ${program.model}` };
  });
  let tab = $state<ClassTab>(untrack(() => initialTab));
  let visited = $state<ClassTab[]>(untrack(() => [initialTab]));
  let opening = $state(false);
  /** A shared-runtime lesson whose preparation has not finished: Learn now retries it. */
  const pending = $derived(!!active && active.runtime === 'study' && ['planned', 'preparing'].includes(active.lifecycle));
  /** Another class's focused session holds the desk: this class waits. */
  const heldElsewhere = $derived(app.heldByOtherClass(program.subject_id));
  const heldLabel = $derived(heldElsewhere ? (app.state?.classroom_programs.find(p => p.subject_id === heldElsewhere)?.label ?? heldElsewhere) : '');
  async function open(slotId: number | null = null, occurrenceId: string | null = null) { if (opening) return; opening = true; try { if (active && !pending) await app.resumeClass(program.subject_id); else await app.startClass(program.subject_id, slotId, program.completed, occurrenceId); } finally { opening = false; } }
  let busy = $state(false), error = $state('');
  async function discard() {
    if (!active || busy) return;
    busy = true; error = '';
    try { await api.skipClassLesson(active.session_id); await app.refresh(); } catch (cause) { error = String(cause); } finally { busy = false; }
  }
  let map = $state<CurriculumMapView | null>(null), mapLoading = $state(false), mapError = $state('');
  let path = $state<AcceptedPath | null>(null), pathLoading = $state(false), pathError = $state(''), pathLoaded = $state(false), editingPath = $state(false);
  const tabs = [
    { id: 'overview', label: 'Overview', icon: LayoutDashboard },
    { id: 'settings', label: 'Settings', icon: Settings2 },
    { id: 'entry', label: 'Starting point', icon: Compass },
    { id: 'curriculum', label: 'Curriculum', icon: BookOpen },
    { id: 'schedule', label: 'Schedule', icon: CalendarClock },
  ] as const;
  function select(id: ClassTab) { tab = id; ontabchange?.(id); if (!visited.includes(id)) visited = [...visited, id]; }
  $effect(() => { const ask = requested; if (ask) untrack(() => select(ask.tab)); });
  function key(event: KeyboardEvent, index: number) {
    if (!['ArrowLeft','ArrowRight','Home','End'].includes(event.key)) return;
    event.preventDefault();
    const next = event.key === 'Home' ? 0 : event.key === 'End' ? tabs.length - 1 : (index + (event.key === 'ArrowRight' ? 1 : -1) + tabs.length) % tabs.length;
    select(tabs[next].id); document.getElementById(`${uid}-tab-${tabs[next].id}`)?.focus();
  }
  async function toggle() {
    if (busy) return;
    if (!program.enabled && !program.accepted_path) { select('entry'); return; }
    if (!program.enabled && !slots.some(slot => slot.enabled)) { select('schedule'); return; }
    busy = true; error = '';
    try {
      await api.configureClassroomProgram({ subject_id: program.subject_id, enabled: !program.enabled, agent: program.agent, model: program.model, custom_agent_bin: program.custom_agent_bin, session_minutes: program.session_minutes, start_level: program.language_progress?.start_level ?? null, target_level: program.language_progress?.target_level ?? null, weekly_minutes: program.language_progress?.weekly_minutes ?? null });
      await app.refresh();
    } catch (cause) { error = String(cause); } finally { busy = false; }
  }
  async function loadMap() { mapLoading = true; mapError = ''; try { map = await api.getCurriculumMap(program.subject_id as FocusArea); } catch (cause) { mapError = String(cause); } finally { mapLoading = false; } }
  async function loadPath() { pathLoading = true; pathError = ''; try { path = await api.getClassPath(program.subject_id); pathLoaded = true; } catch (cause) { pathError = String(cause); } finally { pathLoading = false; } }
  $effect(() => { if (tab === 'curriculum' && program.kind === 'engineering') untrack(() => { if (!map && !mapLoading && !mapError) void loadMap(); }); });
  $effect(() => { if (tab === 'entry') untrack(() => { if (!pathLoaded && !pathLoading && !pathError) void loadPath(); }); });
  async function setupClosed() { editingPath = false; await loadPath(); }
  let revising = $state(false);
  let challenge = $state<{ unit: string; label: string } | null>(null);
  async function challengeApplied() { path = await api.getClassPath(program.subject_id); pathLoaded = true; await loadMap(); await app.refresh(); }
  /** Bypass or include a topic on the accepted route; the map and program refresh afterwards. */
  async function revisePath(change: PathChange) {
    if (revising) return;
    revising = true; mapError = '';
    try {
      const current = path ?? await api.getClassPath(program.subject_id);
      if (!current) throw new Error('Accept a learning path before changing the route.');
      path = await api.reviseClassPath({ course_id: program.subject_id, expected_revision: current.revision, change });
      pathLoaded = true;
      await loadMap();
      await app.refresh();
    } catch (cause) { mapError = String(cause); }
    finally { revising = false; }
  }
</script>

<article class="class-detail" aria-label={`${program.label} controls`}>
  <header class="class-header">
    <div class="identity"><CourseGlyph courseId={program.subject_id} size={46} /><div><div class="eyebrow mono">{program.kind === 'language' ? 'LANGUAGE' : 'ENGINEERING'} / {program.short_code}{#if custom}<span class="custom-badge">custom{#if course} · {course.version}{/if}</span>{/if}<span class:enabled={program.enabled} class="status">{program.completed ? 'Completed' : program.enabled ? 'Active' : 'Inactive'}</span></div><h2>{program.label}</h2><p>{program.native_label}</p></div></div>
    <div class="header-actions"><button class="ghost mono-ghost" onclick={toggle} disabled={busy || opening || preparing}>{busy ? 'Saving…' : program.enabled ? 'Pause class' : !program.accepted_path ? 'Set starting point' : slots.some(slot => slot.enabled) ? 'Activate class' : 'Set study times'}</button><button class="cta mono-cta" disabled={(!program.enabled && !active) || opening || preparing || !!heldElsewhere} title={heldElsewhere ? `A focused ${heldLabel} session holds the desk.` : undefined} onclick={() => open()}><Play size={13} />{opening ? 'Opening…' : pending ? 'Retry preparation' : active ? 'Resume' : program.completed ? 'Revisit' : 'Learn now'}</button>{#if pending}<button class="ghost mono-ghost" onclick={discard} disabled={busy || opening || preparing}>Discard lesson</button>{/if}</div>
    {#if heldElsewhere}<p class="held mono" role="status">A focused {heldLabel} session holds the desk. {program.label} waits until it finishes.</p>{/if}
  </header>
  {#if error}<p class="banner error" role="alert">{error}</p>{/if}
  <div class="tabs" role="tablist" aria-label={`${program.label} sections`}>
    {#each tabs as item, index}<button role="tab" id={`${uid}-tab-${item.id}`} aria-selected={tab === item.id} aria-controls={`${uid}-panel-${item.id}`} tabindex={tab === item.id ? 0 : -1} class:selected={tab === item.id} onclick={() => select(item.id)} onkeydown={event => key(event,index)}><item.icon size={14} /><span>{item.label}</span>{#if item.id === 'schedule' && slots.length}<small>{slots.length}</small>{/if}</button>{/each}
  </div>
  {#each tabs as item}
    <div class="tab-content" id={`${uid}-panel-${item.id}`} role="tabpanel" aria-labelledby={`${uid}-tab-${item.id}`} hidden={tab !== item.id} tabindex="0">
      {#if visited.includes(item.id)}
        {#if item.id === 'overview'}
          <div class="overview">
            <div class="overview-intro"><span class="eyebrow mono">THE COURSE</span><h3>{course?.summary ?? program.label}</h3><p>{course?.outcome ?? ''}</p></div>
            <div class="metrics"><div><span>Progress</span><strong>{Math.round(program.progress * 100)}<small>%</small></strong><p>{program.progress_label}</p><div class="progress-track" role="progressbar" aria-label="Course progress" aria-valuemin="0" aria-valuemax="100" aria-valuenow={Math.round(program.progress * 100)}><i style:width={`${program.progress * 100}%`}></i></div></div><div><span>Session length</span><strong>{sessionLength.value}<small>{sessionLength.unit}</small></strong><p>{sessionLength.note}</p></div><div><span>Study times</span><strong>{slots.length}</strong><button class="inline" onclick={() => select('schedule')}>{slots.length ? 'Manage schedule' : 'Set a study time'}<ArrowRight size={13} /></button></div></div>
            <div class="overview-grid"><section class="info-card"><span class="eyebrow mono">YOUR STARTING POINT</span><h4>{program.accepted_path ? `Lessons start at ${program.accepted_path.entry_label}` : 'Not chosen yet'}</h4><p>{program.accepted_path ? `${program.accepted_path.earlier_topics} earlier ${program.accepted_path.earlier_topics === 1 ? 'topic is' : 'topics are'} set aside and ${program.accepted_path.refreshers} prerequisite ${program.accepted_path.refreshers === 1 ? 'refresher comes' : 'refreshers come'} first. Revision ${program.accepted_path.revision}, from ${program.accepted_path.route === 'diagnostic' ? 'a short check' : program.accepted_path.route === 'manual' ? 'the stage you chose' : 'starting from scratch'}.` : 'Required before the class activates. Start from scratch, pick a stage, or take a short check to find your level; the first lessons are written for whichever you choose.'}</p><button class="ghost mono-ghost" onclick={() => select('entry')}>{program.accepted_path ? 'View personal path' : 'Set starting point'}<ArrowRight size={13} /></button></section>{#if program.route}<section class="info-card route-card"><span class="eyebrow mono">YOUR ROUTE · REVISION {program.route.revision}</span><dl class="route-facts"><div><dt>Completed on your route</dt><dd>{program.route.required_done} / {program.route.required_total} required</dd></div><div><dt>Course coverage</dt><dd>{program.route.coverage_done} / {program.route.coverage_total} core</dd></div><div><dt>Demonstrated prior knowledge</dt><dd>{program.route.demonstrated} {program.route.demonstrated === 1 ? 'sample' : 'samples'}</dd></div><div><dt>Needs review</dt><dd>{program.route.needs_review} {program.route.needs_review === 1 ? 'topic' : 'topics'}</dd></div></dl>{#if program.route.stale}<p class="stale" role="alert">The curriculum changed since you accepted this path. Review your starting point before the next lesson; your history and saved work are kept.</p><button class="cta mono-cta" onclick={() => select('entry')}>Review starting point<ArrowRight size={13} /></button>{:else if program.route.next}<p><strong>Next:</strong> {program.route.next.title} · {program.route.next.reason}</p>{:else}<p>Nothing is left on your accepted route; the course's final outcome check remains.</p>{/if}{#if !program.route.stale}<button class="ghost mono-ghost" onclick={() => select('curriculum')}>Review the route<ArrowRight size={13} /></button>{/if}</section>{/if}<section class="info-card"><span class="eyebrow mono">WORKING ENVIRONMENT</span><p class="environment">{course?.environment ?? ''}</p>{#if course?.prerequisite_courses.length}<p>Suggested preparation: {course.prerequisite_courses.map(id => courseDefinition(id)?.label ?? id).join(', ')} or equivalent experience.</p>{/if}<button class="ghost mono-ghost" onclick={() => select('curriculum')}>Explore curriculum<ArrowRight size={13} /></button></section></div>
            {#if pending}<p class="notice">Lesson preparation did not finish: <strong>{active?.title}</strong>. Retry it or discard it from the class header; nothing was graded.</p>{:else if active}<p class="notice">Saved session: <strong>{active.title}</strong>. Resume from the class header.</p>{:else if !program.enabled}<p class="notice">{program.accepted_path ? 'Add a study time, then activate this class when you’re ready.' : 'Choose your starting point first: start from scratch, find your level, or pick a stage. The first lessons are written for that choice. Then add a study time and activate the class.'}</p>{/if}
          </div>
        {:else if item.id === 'settings'}<ClassSettings {program} />
        {:else if item.id === 'schedule'}<ClassSchedule {program} opening={opening || preparing || !!heldElsewhere} onstart={open} onmakeup={(id) => open(null, id)} />
        {:else if item.id === 'entry'}
          {#if pathLoading}<p class="loading" role="status">Loading your starting point…</p>{:else if pathError}<p class="error" role="alert">{pathError}</p><button class="ghost mono-ghost" onclick={loadPath}>Retry</button>{:else if pathLoaded}
            {#if path && !editingPath}<PathPreview embedded path={path.recommendation} acceptedRevision={path.revision} onclose={() => select('overview')} onfoundations={() => editingPath = true} />{:else}<EnrollmentSetup embedded courseId={program.subject_id} onclose={setupClosed} />{/if}
          {/if}
        {:else if item.id === 'curriculum'}
          {#if custom}
            <div class="custom-tools"><span class="mono">YOUR OWN CLASS{#if course} · VERSION {course.version}{/if}</span><span class="custom-copy">Edit the curriculum to publish a new version; lessons already taught keep their topics. The class file carries the course, not your progress.</span><span class="custom-actions"><button class="ghost mono-ghost small" onclick={() => app.openBuilder(program.subject_id)}><PenLine size={12} />Edit curriculum</button><button class="ghost mono-ghost small" onclick={exportClass}><FileJson size={12} />Export class file</button></span></div>
          {/if}
          {#if program.kind === 'engineering'}<PracticeBank courseId={program.subject_id} currentPhase={map?.current_phase ?? null} />{/if}
          {#if program.kind === 'engineering'}
            {#if map}{#if mapError}<p class="error" role="alert">{mapError}</p>{/if}{#if challenge && program.accepted_path}<UnitChallenge courseId={program.subject_id} unit={challenge.unit} unitLabel={challenge.label} pathRevision={map.path?.revision ?? program.accepted_path.revision} onclose={() => (challenge = null)} onapplied={challengeApplied} />{/if}<CurriculumMap {map} embedded onrevise={program.accepted_path ? revisePath : undefined} {revising} onchallenge={program.accepted_path ? (unit, label) => (challenge = { unit, label }) : undefined} />{:else if mapLoading}<p class="loading" role="status">Loading curriculum…</p>{:else if mapError}<p class="error" role="alert">{mapError}</p><button class="ghost mono-ghost" onclick={loadMap}>Retry</button>{/if}
          {:else}
            <div class="language-map"><span class="eyebrow mono">LANGUAGE PATH</span><h3>{program.label} · learning bands</h3><p>{course?.outcome ?? ''}</p>{#if program.language_progress}<p class="notice">Current band: {program.language_progress.current_level} · Target: {program.language_progress.target_level} · {program.progress_label}</p>{/if}<ol>{#each course?.entry_points ?? [] as entry}<li><span class="mono">{entry.id}</span><div><h4>{entry.label}</h4></div></li>{/each}</ol><button class="ghost mono-ghost" onclick={() => select('entry')}>Review starting point<ArrowRight size={13} /></button></div>
          {/if}
        {/if}
      {/if}
    </div>
  {/each}
</article>

<style>
  .stale { color: var(--led-warn); }
  .route-facts { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 8px 14px; margin: 10px 0; } .route-facts div { display: grid; gap: 2px; } .route-facts dt { font: 9px var(--font-mono); letter-spacing: 0.08em; text-transform: uppercase; color: var(--faint); } .route-facts dd { margin: 0; font-size: 13px; color: var(--fg); }
  .held { margin: 8px 0 0; font-size: 11px; color: var(--led-warn); }
  .class-detail { display: flex; flex-direction: column; flex: 1; min-height: 0; min-width: 0; overflow: hidden; }
  .class-header { flex-shrink: 0; padding: 22px 24px; display: flex; justify-content: space-between; align-items: center; gap: 18px; background: linear-gradient(110deg,var(--surface),var(--node-bg)); }
  .identity { display: flex; align-items: center; gap: 14px; min-width: 0; } .eyebrow { font-size: 10px; color: var(--muted); letter-spacing: .7px; } h2 { font: 26px/1.2 var(--font-display); margin: 7px 0 5px; } .identity p { font: 10px var(--font-mono); margin: 0; color: var(--muted); }
  .status { color: var(--muted); background: var(--bg); border: 1px solid var(--node-border); border-radius: var(--radius-detail); margin-left: 10px; padding: 2px 6px; letter-spacing: 0; } .status.enabled { color: var(--led-ok); }
  .custom-badge { margin-left: 8px; padding: 2px 7px; border-radius: 999px; border: 1px solid color-mix(in srgb, var(--accent) 45%, var(--node-border)); background: color-mix(in srgb, var(--accent) 12%, var(--surface)); color: var(--accent); font-size: 9px; letter-spacing: 0.6px; text-transform: lowercase; }
  .custom-tools { display: flex; align-items: center; gap: 14px; flex-wrap: wrap; margin-bottom: 14px; padding: 10px 14px; border: 1px dashed color-mix(in srgb, var(--accent) 40%, var(--node-border)); border-radius: var(--radius-control); } .custom-tools > .mono { font-size: 9px; letter-spacing: 1.2px; color: var(--accent); } .custom-copy { flex: 1; min-width: 220px; font-size: 11px; color: var(--muted); line-height: 1.5; } .custom-actions { display: flex; gap: 8px; flex-wrap: wrap; }
  .header-actions { display: flex; flex-wrap: wrap; gap: 8px; flex-shrink: 0; } .header-actions button { font-size: 11px; white-space: nowrap; padding: 10px 12px; }
  .tabs { display: flex; gap: 6px; padding: 0 24px; border-top: 1px solid var(--node-divider); border-bottom: 1px solid var(--node-border); flex-shrink: 0; overflow-x: auto; background: var(--node-bg); }
  .tabs button { position: relative; display: flex; align-items: center; justify-content: center; gap: 7px; flex-shrink: 0; border: 0; border-bottom: 2px solid transparent; border-radius: 0; background: none; padding: 16px 9px 14px; font: 11px var(--font-mono); color: var(--muted); cursor: pointer; white-space: nowrap; }
  .tabs button.selected { color: var(--accent); border-bottom-color: var(--accent); } .tabs button:hover { color: var(--fg); } .tabs small { color: var(--muted); padding: 1px 5px; background: var(--surface-2); border-radius: var(--radius-detail); font-size: 9px; }
  .tab-content { flex: 1; min-height: 0; min-width: 0; overflow-y: auto; overflow-x: hidden; overscroll-behavior: contain; scrollbar-gutter: stable; padding: 24px; scroll-padding-top: 24px; }
  .tab-content[hidden] { display: none; } .tab-content:focus-visible { outline: 1px solid var(--violet); outline-offset: -3px; }
  button:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
  .overview, .language-map { max-width: 1000px; margin: 0 auto; } .overview-intro h3 { font: 25px/1.45 var(--font-display); margin: 10px 0; max-width: 760px; } p { font-size: 13px; line-height: 1.7; color: var(--muted); }
  .metrics { display: grid; grid-template-columns: 1.3fr 1fr 1fr; border: 1px solid var(--node-border); border-radius: var(--radius-panel); margin: 24px 0; overflow: hidden; }
  .metrics > div { padding: 18px; min-width: 0; } .metrics > div + div { border-left: 1px solid var(--node-border); } .metrics span { display: block; color: var(--muted); font: 10px var(--font-mono); } .metrics strong { display: block; font: 30px var(--font-display); margin-top: 12px; } .metrics strong small { font: 12px var(--font-mono); color: var(--muted); margin-left: 6px; } .metrics p { font-size: 11px; margin: 8px 0; overflow-wrap: anywhere; }
  .progress-track { height: 3px; background: var(--node-border); margin-top: 12px; border-radius: 2px; overflow: hidden; } .progress-track i { height: 100%; display: block; background: var(--led-ok); }
  .inline { display: inline-flex; align-items: center; gap: 6px; padding: 0; margin-top: 9px; background: none; border: 0; color: var(--accent); font-size: 11px; text-align: left; cursor: pointer; }
  .overview-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 16px; } .info-card { border: 1px solid var(--node-border); border-radius: var(--radius-panel); padding: 20px; background: var(--bg); } h4 { margin: 14px 0 8px; font: 19px var(--font-display); } .info-card p { font-size: 12px; } .info-card button { margin-top: 10px; font-size: 11px; } .environment { margin-top: 15px; }
  .notice { padding: 12px 15px; border-left: 2px solid var(--violet); background: var(--surface); font-size: 12px; margin-top: 20px; }
  .banner { margin: 0; padding: 9px 24px; color: var(--accent); font-size: 11px; background: var(--surface); flex-shrink: 0; } .error { color: var(--led-err); overflow-wrap: anywhere; } .loading { color: var(--muted); }
  .language-map h3 { font: 25px var(--font-display); } .language-map ol { list-style: none; padding: 0; display: grid; gap: 12px; } .language-map li { display: flex; gap: 16px; border: 1px solid var(--node-border); border-radius: var(--radius-panel); padding: 16px; } .language-map li > span { color: var(--accent); font-size: 12px; padding-top: 4px; } .language-map h4 { margin: 0; }
  @media(max-width:1100px) { .class-header { flex-wrap: wrap; padding: 18px; } .tabs { padding: 0 14px; gap: 0; } .tabs button { padding: 14px 9px 12px; } .overview-grid { grid-template-columns: 1fr; } }
  @media(max-width:700px) { .class-header { padding: 16px; gap: 12px; } .identity :global(svg) { display: none; } h2 { font-size: 23px; } .tab-content { padding: 16px; } .tabs button :global(svg) { display: none; } .metrics { grid-template-columns: 1fr; } .metrics > div + div { border-left: 0; border-top: 1px solid var(--node-border); } .overview-intro h3 { font-size: 21px; } }
</style>
