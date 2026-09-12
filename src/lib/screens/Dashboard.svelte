<script lang="ts">
  import { tick } from 'svelte';
  import { api, type DashboardView, type ProgressEntry, type ProgressLesson } from '../ipc';
  import { app } from '../stores.svelte';
  import Markdown from '../components/Markdown.svelte';
  import ExerciseWorkspace from '../components/ExerciseWorkspace.svelte';
  import Dropdown from '../components/Dropdown.svelte';
  import CourseGlyph from '../components/CourseGlyph.svelte';
  import { ArrowLeft, ArrowUpRight, BookOpen, ChevronLeft, ChevronRight, Hammer, Layers, Search, X } from 'lucide-svelte';
  import DownloadMenu from '../features/lessons/DownloadMenu.svelte';

  let data = $state<DashboardView | null>(null);
  let selected = $state('');
  let search = $state('');
  let status = $state('');
  let page = $state(0);
  let reload = $state(0);
  let busy = $state(true);
  let error = $state('');
  let viewing = $state<ProgressLesson | null>(null);
  let viewingEntry = $state<ProgressEntry | null>(null);
  let opening = $state('');
  let archiveTab = $state<'read' | 'exercise'>('read');
  let compactTab = $state<'summary' | 'history'>('summary');
  let opener: HTMLButtonElement | null = null;
  const selectedClass = $derived(data?.classes.find(c => c.subject_id === selected));
  /** The ledger lists the classes you actually study: active ones, plus any
   *  paused class that still holds completed sessions. */
  const ledgerClasses = $derived((data?.classes ?? []).filter(c => c.enabled || c.completed_sessions > 0 || c.subject_id === selected));
  const classOptions = $derived([{value:'',label:'All classes'},...ledgerClasses.map(c=>({value:c.subject_id,label:c.label}))]);
  const statusOptions = [{value:'',label:'All results'},{value:'completed',label:'Completed'},{value:'in_progress',label:'In progress'},{value:'skipped',label:'Skipped'}];
  const pageCount = $derived(Math.max(1,Math.ceil((data?.history_total??0)/(data?.page_size??8))));
  const recentCompleted = $derived(data?.activity.reduce((sum,day)=>sum+day.completed,0)??0);
  const activeDays = $derived(data?.activity.filter(day=>day.completed>0).length??0);

  $effect(() => {
    const query = {subject_id:selected || null,search,status:status || null,page};
    reload;
    let disposed = false;
    busy = true;
    const timer = setTimeout(() => {
      api.getDashboard(query).then(result => { if (!disposed) { data=result; error=''; } })
        .catch(e => { if (!disposed) error=String(e); })
        .finally(() => { if (!disposed) busy=false; });
    }, search ? 180 : 0);
    return () => { disposed=true; clearTimeout(timer); };
  });
  function chooseClass(id: string) { selected=id; page=0; }
  function changeSearch(value: string) { search=value; page=0; }
  function dateLabel(date: string, year = false) {
    return new Date(`${date}T12:00:00`).toLocaleDateString(undefined,{day:'numeric',month:'short',...(year?{year:'numeric' as const}:{})});
  }
  function statusLabel(value: string) { return value==='in_progress' ? 'In progress' : value==='completed' ? 'Completed' : value==='skipped' ? 'Skipped' : value; }
  function classLabel(id: string) { return data?.classes.find(c=>c.subject_id===id)?.label ?? 'Earlier study'; }
  async function openLesson(entry: ProgressEntry, button: HTMLButtonElement) {
    if (opening) return;
    opener=button;
    opening=`${entry.source}:${entry.owner_id}`;
    try {
      const lesson=await api.getProgressLesson(entry.source,entry.owner_id);
      if (!lesson) throw new Error('This session has no saved lesson to read.');
      viewing=lesson; viewingEntry=entry; archiveTab='read';
      await tick(); document.getElementById('progress-archive-heading')?.focus();
    } catch(e) { error=String(e); }
    finally { opening=''; }
  }
  async function closeLesson() { viewing=null; viewingEntry=null; await tick(); opener?.focus(); }

  function openClass() {
    const program=app.state?.classroom_programs.find(c=>c.subject_id===selected);
    if (program) { app.classSelection=program.subject_id; app.classTab='curriculum'; }
    app.navigate('classes');
  }
</script>

<div class="progress-page page-frame">
  <header class="page-heading"><div><div class="meta-label">LEARNING LEDGER</div><h1>Progress</h1></div><p>A little practice, a clearer picture.</p></header>
  {#if error}<div class="error" role="alert"><span>{error}</span><button class="ghost mono-ghost" onclick={() => reload++}>Retry</button><button class="icon-button" aria-label="Dismiss progress error" onclick={() => error=''}><X size={15}/></button></div>{/if}
  <div class="progress-workspace" hidden={viewing!==null} aria-busy={busy}>
    <aside aria-label="Progress by class">
      <div class="sidebar-heading mono">YOUR CLASSES<span>{data ? ledgerClasses.length : '—'}</span></div>
      <button class="all-classes" class:selected={!selected} aria-pressed={!selected} onclick={() => chooseClass('')}><Layers size={18}/><span>All classes<small>Your complete learning record</small></span></button>
      <nav class="class-list" aria-label="Filter progress by class">
        {#each ledgerClasses as course (course.subject_id)}
          <button class="class-row" class:selected={selected===course.subject_id} aria-pressed={selected===course.subject_id} onclick={() => chooseClass(course.subject_id)}>
            <CourseGlyph courseId={course.subject_id} size={30}/><span class="class-copy"><strong>{course.label}</strong><span class="class-meta"><span>{course.completed_sessions} completed</span><span>{Math.round(course.progress*100)}%</span></span><span class="coverage-track" aria-label={course.progress_label}><i style:width={`${Math.max(0,Math.min(100,course.progress*100))}%`}></i></span></span>
          </button>
        {/each}
      </nav>
      <p class="sidebar-note">Coverage follows each class’s curriculum. Completed sessions reflect practice recorded here.</p>
    </aside>
    <section class="overview" aria-label="Learning overview">
      <div class="mobile-filter"><Dropdown hideLabel label="Progress for class" value={selected} options={classOptions} onchange={chooseClass}/></div>
      <div class="overview-heading"><div><h2>{selectedClass?.label??'Across your desk'}</h2>{#if selectedClass}<p>{selectedClass.progress_label}</p>{/if}</div>{#if selected}<button class="ghost mono-ghost" onclick={openClass}>Curriculum <ArrowUpRight size={13}/></button>{:else}<span class="scope mono">ALL TIME</span>{/if}</div>
      <div class="compact-tabs" aria-label="Progress view"><span>{selectedClass?.label??'All classes'}</span><button aria-pressed={compactTab==='summary'} class:active={compactTab==='summary'} onclick={()=>compactTab='summary'}>Overview</button><button aria-pressed={compactTab==='history'} class:active={compactTab==='history'} onclick={()=>compactTab='history'}>History</button></div>
      <div class="summary-grid" class:hide-on-compact={compactTab!=='summary'}>
        <div class="metric"><span class="meta-label">SESSIONS COMPLETED</span><strong>{data?.completed_sessions??'—'}</strong><small>Finished learning sessions</small></div>
        <div class="metric"><span class="meta-label">DAYS OF PRACTICE</span><strong>{data?.study_days??'—'}</strong><small>Days with a completed session</small></div>
        <div class="metric"><span class="meta-label">CURRENT STREAK</span><strong>{data?.streak??'—'}<em> {data?.streak===1?'day':'days'}</em></strong><small>Consecutive days of practice</small></div>
      </div>
      <section class="activity" class:hide-on-compact={compactTab!=='summary'} aria-label="Activity in the last 28 days">
        <div class="activity-heading"><span class="meta-label">LAST 28 DAYS</span><span><b>{recentCompleted}</b> completed · <b>{activeDays}</b> active days</span></div>
        <div class="activity-days" role="img" aria-label={`${recentCompleted} completed sessions on ${activeDays} of the last 28 days`}>
          {#each data?.activity??[] as day}<span class:practiced={day.completed>0} class:multiple={day.completed>1} title={`${dateLabel(day.date,true)}: ${day.completed} completed`}><i style:height={`${Math.min(100,day.completed*24+16)}%`}></i></span>{/each}
        </div>
        <div class="activity-caption"><span>{data?.activity[0] ? dateLabel(data.activity[0].date) : '28 days ago'}</span><span>Today</span></div>
      </section>
      <section class="history-panel" class:hide-on-compact={compactTab!=='history'} aria-label="Lesson history">
        <div class="history-heading"><h3>Lesson history</h3><span class="mono" role="status">{busy?'Updating…':`${data?.history_total??0} sessions`}</span></div>
        <div class="history-tools"><div class="search"><Search size={15}/><input type="search" aria-label="Search lesson history" placeholder="Find a lesson…" value={search} oninput={event=>changeSearch(event.currentTarget.value)}/>{#if search}<button aria-label="Clear lesson search" onclick={()=>changeSearch('')}><X size={14}/></button>{/if}</div><Dropdown hideLabel label="Filter lesson result" value={status} options={statusOptions} onchange={value=>{status=value;page=0;}}/></div>
        <!-- The bounded history region must accept keyboard focus for scrolling. -->
        <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
        <div class="history-scroll" role="region" tabindex="0" aria-label="Scrollable lesson history">
          {#if !data}<div class="empty" role="status"><BookOpen size={25}/><h3>{error?'Could not load progress':'Loading your learning record…'}</h3></div>
          {:else if !data.history.length}<div class="empty"><BookOpen size={28}/><h3>{search||status?'No lessons match':'Your learning record starts here'}</h3><p>{search||status?'Try another search or result filter.':'Complete a class session to see your practice take shape.'}</p><button class="ghost mono-ghost" onclick={()=>{if(search||status){search='';status='';page=0;}else openClass();}}>{search||status?'Clear filters':'Explore classes'} <ArrowUpRight size={13}/></button></div>
          {:else}<table><thead><tr><th scope="col">Lesson / class</th><th scope="col">Date</th><th scope="col">Result</th><th scope="col" class="score">Score</th></tr></thead><tbody>
            {#each data.history as entry (`${entry.source}:${entry.owner_id}`)}<tr><td class="lesson-cell">{#if entry.can_read}<button disabled={busy||!!opening} onclick={event=>openLesson(entry,event.currentTarget)} aria-label={`Read ${entry.title}`}><span>{entry.title}</span><ArrowUpRight size={13}/></button>{:else}<strong>{entry.title}</strong>{/if}<small>{classLabel(entry.subject_id)}{entry.source==='primary'?' · Earlier daily study':''}{opening===`${entry.source}:${entry.owner_id}`?' · Opening…':''}</small></td><td class="date-cell">{dateLabel(entry.date,true)}</td><td><span class="result" class:completed={entry.status==='completed'} class:skipped={entry.status==='skipped'}>{statusLabel(entry.status)}</span></td><td class="score mono">{entry.score==null?'—':`${Math.round(entry.score*100)}%`}</td></tr>{/each}
          </tbody></table>{/if}
        </div>
        <footer class="pagination"><span>{data?.history_total?`${data.page*data.page_size+1}–${Math.min((data.page+1)*data.page_size,data.history_total)} of ${data.history_total}`:'No sessions yet'}</span><div><button class="icon-button" aria-label="Previous history page" disabled={busy||!data||data.page===0} onclick={()=>page=Math.max(0,(data?.page??0)-1)}><ChevronLeft size={16}/></button><span>Page {(data?.page??0)+1} / {pageCount}</span><button class="icon-button" aria-label="Next history page" disabled={busy||!data||data.page+1>=pageCount} onclick={()=>page=(data?.page??0)+1}><ChevronRight size={16}/></button></div></footer>
      </section>
    </section>
  </div>
  {#if viewing}
    <section class="archive" aria-label="Saved lesson">
      <header><button class="ghost mono-ghost" onclick={closeLesson}><ArrowLeft size={14}/> Progress</button><div><h2 id="progress-archive-heading" tabindex="-1">{viewing.title}</h2><p>{dateLabel(viewing.date,true)} · Saved lesson</p></div>{#if viewingEntry}<DownloadMenu source={viewingEntry.source} ownerId={viewingEntry.owner_id} />{/if}</header>
      <div class="archive-tabs" aria-label="Saved lesson sections"><button class:active={archiveTab==='read'} aria-pressed={archiveTab==='read'} onclick={()=>archiveTab='read'}><BookOpen size={14}/> Lesson</button>{#if viewing.course_id!==null||viewing.classroom_session_id!==null||viewing.study_session_id!==null}<button class:active={archiveTab==='exercise'} aria-pressed={archiveTab==='exercise'} onclick={()=>archiveTab='exercise'}><Hammer size={14}/> Practice</button>{/if}</div>
      <div class="archive-scroll"><article class="theme-scholar reader-card">{#if archiveTab==='exercise'}<ExerciseWorkspace courseId={viewing.course_id??undefined} classroomSessionId={viewing.classroom_session_id??undefined} studySessionId={viewing.study_session_id??undefined}/>{:else}<Markdown markdown={viewing.markdown}/>{/if}</article></div>
    </section>
  {/if}
</div>

<style>
  .progress-page { container:progress / size; display:flex; flex-direction:column; flex:1; min-height:0; min-width:0; overflow:hidden; }
  .page-heading { display:flex; align-items:flex-end; justify-content:space-between; gap:16px; margin-bottom:18px; flex-shrink:0; } .page-heading h1 { font-size:28px; margin:6px 0 0; } .meta-label { font:9px var(--font-mono); letter-spacing:1.1px; color:var(--muted); } .page-heading p { color:var(--muted); font-size:12px; margin:0 0 4px; }
  .progress-workspace { display:grid; grid-template-columns:238px minmax(0,1fr); flex:1; min-height:0; overflow:hidden; background:var(--node-bg); border:1px solid var(--node-border); border-radius:var(--radius-panel); box-shadow:0 8px 30px #0002; }
  [hidden] { display:none!important; } aside { display:flex; flex-direction:column; min-height:0; border-right:1px solid var(--node-border); background:var(--bg); }
  .sidebar-heading { display:flex; justify-content:space-between; padding:18px 16px 12px; font-size:10px; color:var(--muted); letter-spacing:1px; } .sidebar-heading span { color:var(--violet-fg); }
  .all-classes,.class-row { display:flex; align-items:center; gap:10px; text-align:left; color:var(--fg); background:none; border:1px solid transparent; cursor:pointer; }
  .all-classes { flex-shrink:0; margin:0 8px 8px; padding:12px 10px; font-size:12px; } .all-classes>:global(svg) { color:var(--violet-fg); } .all-classes small { display:block; color:var(--muted); font-size:9px; margin-top:6px; }
  .all-classes.selected,.class-row.selected { background:var(--surface-2); border-color:var(--violet); box-shadow:inset 3px 0 var(--violet-fg); } .all-classes:hover,.class-row:hover { background:var(--surface); }
  .class-list { padding:4px 8px; min-height:0; overflow-y:auto; overscroll-behavior:contain; flex:1; border-top:1px solid var(--node-divider); } .class-row { width:100%; padding:8px; margin:3px 0; } .class-copy { min-width:0; flex:1; } .class-copy strong { display:block; font-size:11px; line-height:1.45; font-weight:500; } .class-meta { display:flex; justify-content:space-between; gap:5px; color:var(--muted); font:9px var(--font-mono); margin:4px 0; }
  .coverage-track { display:block; height:3px; background:var(--node-border); border-radius:2px; overflow:hidden; } .coverage-track i { display:block; height:100%; background:var(--led-ok); } .sidebar-note { flex-shrink:0; padding:12px 16px; margin:0; font-size:10px; line-height:1.6; color:var(--muted); border-top:1px solid var(--node-divider); }
  .overview { min-width:0; min-height:0; display:flex; flex-direction:column; padding:20px 22px 0; } .overview-heading { display:flex; align-items:center; justify-content:space-between; gap:12px; margin-bottom:12px; flex-shrink:0; } h2 { font:22px/1.2 var(--font-display); margin:0; } .overview-heading p { font-size:11px; color:var(--muted); margin:7px 0 0; } .scope { font-size:9px; color:var(--muted); border:1px solid var(--node-border); padding:5px 7px; border-radius:var(--radius-detail); white-space:nowrap; }
  .summary-grid { display:grid; grid-template-columns:repeat(3,minmax(0,1fr)); gap:12px; flex-shrink:0; } .metric { font-size:10px; line-height:1.4; padding:10px 14px; border:1px solid var(--node-border); border-radius:var(--radius-control); background:var(--bg); } .metric .meta-label { display:block; } .metric strong { display:block; font:27px/1.2 var(--font-display); margin:6px 0 0; } .metric em { margin-left:4px; color:var(--muted); font:11px var(--font-mono); font-style:normal; } .metric small { display:none; color:var(--muted); font-size:9px; }
  .activity { display:grid; grid-template-columns:180px minmax(0,1fr); gap:12px; align-items:center; font-size:10px; line-height:1.4; flex-shrink:0; padding:12px 0; border-bottom:1px dashed var(--node-border); } .activity-heading { display:flex; flex-direction:column; align-items:flex-start; gap:6px; } .activity-heading>span:last-child { color:var(--muted); font-size:10px; } .activity-heading b { font-weight:500; color:var(--fg); } .activity-days { display:grid; grid-template-columns:repeat(28,minmax(0,1fr)); gap:4px; margin:0; height:25px; } .activity-days>span { display:flex; align-items:flex-end; border-radius:3px; background:var(--surface); overflow:hidden; } .activity-days i { width:100%; min-height:3px; background:var(--node-border); } .activity-days .practiced i { background:var(--led-ok); opacity:.65; } .activity-days .multiple i { opacity:1; } .activity-caption { display:none; justify-content:space-between; font:9px var(--font-mono); color:var(--muted); }
  .history-panel { min-height:0; display:flex; flex-direction:column; flex:1; } .history-heading { font-size:11px; line-height:1.4; display:flex; align-items:center; justify-content:space-between; padding:12px 0 10px; flex-shrink:0; } h3 { font-size:13px; font-weight:500; margin:0; } .history-heading>span { color:var(--muted); font-size:9px; } .history-tools { display:grid; grid-template-columns:minmax(0,1fr) 160px; gap:12px; padding-bottom:12px; flex-shrink:0; }
  .search { display:flex; align-items:center; gap:8px; padding:0 10px; border:1px solid var(--node-border); border-radius:var(--radius-control); background:var(--bg); color:var(--muted); } .search:focus-within { outline:1px solid var(--accent); border-color:var(--accent); } .search input { min-width:0; width:100%; padding:9px 0; background:none; border:0; outline:none; font:11px var(--font-body); color:var(--fg); } .search input::-webkit-search-cancel-button { display:none; } .search button { display:flex; padding:4px; border:0; color:var(--muted); background:none; }
  .history-scroll { overflow:auto; flex:1; min-height:0; overscroll-behavior:contain; scrollbar-gutter:stable; } table { width:100%; border-collapse:collapse; font-size:11px; text-align:left; } th { position:sticky; top:0; z-index:1; background:var(--node-bg); font:9px var(--font-mono); color:var(--muted); padding:8px 7px 10px; border-bottom:1px solid var(--node-border); white-space:nowrap; } td { padding:10px 7px; border-bottom:1px solid var(--node-divider); } tr:last-child td { border-bottom:0; } .lesson-cell { width:52%; } .lesson-cell button { display:flex; align-items:center; gap:6px; text-align:left; border:0; background:none; color:var(--fg); font:11px/1.5 var(--font-body); padding:0; cursor:pointer; } .lesson-cell button :global(svg) { flex-shrink:0; color:var(--accent); } .lesson-cell button:hover { color:var(--accent); } .lesson-cell strong { font-weight:400; } .lesson-cell small { display:block; color:var(--muted); font-size:9px; margin-top:4px; } .date-cell { white-space:nowrap; color:var(--muted); font-size:10px; } .score { text-align:right; white-space:nowrap; font-size:10px; } .result { display:inline-block; padding:4px 6px; border-radius:var(--radius-detail); font:9px var(--font-mono); background:var(--warn-bg); color:var(--warn-fg); white-space:nowrap; } .result.completed { color:var(--ok-fg); background:var(--ok-bg); } .result.skipped { color:var(--muted); background:var(--surface); }
  .pagination { line-height:1.4; display:flex; align-items:center; justify-content:space-between; flex-shrink:0; padding:11px 0; border-top:1px solid var(--node-border); font:9px var(--font-mono); color:var(--muted); } .pagination>div { display:flex; align-items:center; gap:10px; } .icon-button { display:inline-flex; align-items:center; justify-content:center; width:29px; height:29px; padding:0; border:1px solid var(--node-border); background:var(--bg); color:var(--fg); cursor:pointer; } button:disabled { opacity:.4; cursor:default; } .ghost { display:inline-flex; align-items:center; justify-content:center; gap:6px; font-size:10px; } .empty { display:flex; flex-direction:column; justify-content:center; align-items:center; min-height:180px; height:100%; gap:12px; padding:24px; color:var(--muted); text-align:center; } .empty :global(svg) { color:var(--accent); } .empty h3 { color:var(--fg); } .empty p { font-size:11px; margin:0; line-height:1.6; }
  .compact-tabs { display:none; }
  @container progress (min-height:501px) {
    .page-heading { margin-bottom:12px; }
    .overview { padding-top:14px; }
    .overview-heading { margin-bottom:10px; }
    .overview-heading h2 { font-size:20px; }
    .metric { display:flex; align-items:center; justify-content:space-between; gap:10px; padding:10px 12px; }
    .metric .meta-label { max-width:100px; line-height:1.6; }
    .metric strong { margin:0; flex-shrink:0; }
    .activity { padding:9px 0; }
    .history-heading { padding:8px 0; }
    .history-tools { padding-bottom:8px; }
    td { padding-top:8px; padding-bottom:8px; }
    .all-classes { padding-top:9px; padding-bottom:9px; }
    .class-row { padding-top:6px; padding-bottom:6px; margin:2px 0; }
    .sidebar-note { padding-top:9px; padding-bottom:9px; }
  }
  @container progress (max-height:500px) { .page-heading .meta-label { display:none; } .page-heading h1 { font-size:23px; margin:0; } .page-heading { margin-bottom:10px; } .overview-heading { display:none; } .compact-tabs { display:flex; align-items:center; gap:6px; margin-bottom:12px; flex-shrink:0; } .compact-tabs span { flex:1; min-width:0; font-size:11px; color:var(--muted); } .compact-tabs button { border:1px solid var(--node-border); padding:7px 10px; font:10px var(--font-mono); background:var(--bg); color:var(--muted); } .compact-tabs button.active { background:var(--violet-bg); color:var(--violet-fg); border-color:var(--violet); } .hide-on-compact { display:none!important; } .history-heading { display:none; } .history-scroll { min-height:0; } }
  @container progress (max-height:500px) and (max-width:780px) { .overview { position:relative; } .mobile-filter { width:calc(100% - 152px); margin-bottom:12px; } .compact-tabs { position:absolute; top:16px; right:16px; margin:0; } .compact-tabs span { display:none; } }
  .error { display:flex; gap:12px; align-items:center; padding:10px 12px; margin-bottom:12px; border:1px solid var(--bad-fg); border-radius:var(--radius-control); color:var(--bad-fg); background:var(--bad-bg); font-size:11px; } .error>span { flex:1; } .mobile-filter { display:none; }
  .archive { display:flex; flex-direction:column; flex:1; min-height:0; border:1px solid var(--node-border); border-radius:var(--radius-panel); background:var(--node-bg); overflow:hidden; } .archive>header { display:flex; align-items:center; gap:22px; padding:16px 20px; flex-shrink:0; } .archive>header>div { flex:1; min-width:0; } .archive h2 { font-size:20px; } .archive header p { font:10px var(--font-mono); color:var(--muted); margin:7px 0 0; } .archive-tabs { display:flex; gap:8px; padding:0 20px 12px; border-bottom:1px solid var(--node-border); flex-shrink:0; } .archive-tabs button { display:flex; gap:7px; align-items:center; padding:8px 12px; color:var(--muted); background:var(--bg); border:1px solid var(--node-border); font:11px var(--font-mono); cursor:pointer; } .archive-tabs button.active { border-color:var(--violet); color:var(--violet-fg); background:var(--violet-bg); } .archive-scroll { flex:1; min-height:0; overflow:auto; padding:20px; } .reader-card { margin:0 auto; max-width:800px; padding:24px 32px; border-radius:var(--radius-panel); background:var(--bg); color:var(--fg); }

  @media(max-width:1050px) { .progress-workspace { grid-template-columns:205px minmax(0,1fr); } .overview { padding:16px 16px 0; } .summary-grid { gap:8px; } .metric { padding:12px 10px; } .metric .meta-label { font-size:8px; letter-spacing:.5px; } .metric small { display:none; } }
  @media(max-width:780px) { .progress-workspace { grid-template-columns:minmax(0,1fr); } aside { display:none; } .mobile-filter { display:block; margin-bottom:12px; } .page-heading p { display:none; } .overview-heading { margin-bottom:12px; } }
  @media(max-width:520px) { .activity { grid-template-columns:145px minmax(0,1fr); gap:8px; } .activity-days { gap:2px; } .progress-page { padding:16px 12px; } .overview { padding:12px 10px 0; } .overview-heading h2 { font-size:20px; } .scope { display:none; } .summary-grid { gap:5px; } .metric { padding:10px 8px; } .metric .meta-label { font-size:7px; } .metric strong { font-size:25px; } .metric em { font-size:9px; } .history-tools { grid-template-columns:minmax(0,1fr) 135px; gap:8px; } th,td { padding-left:4px; padding-right:4px; } .lesson-cell { width:auto; } .date-cell { white-space:normal; } .result { font-size:8px; padding:4px; } .archive>header { align-items:flex-start; gap:10px; padding:12px; } .archive-scroll { padding:10px; } .reader-card { padding:18px; } }
  @media(max-height:720px) { .progress-page { padding-top:14px; padding-bottom:14px; } .page-heading { margin-bottom:12px; } .overview-heading { margin-bottom:10px; } .metric { padding-top:9px; padding-bottom:9px; } .metric strong { margin:5px 0; font-size:25px; } .metric small { display:none; } .activity { padding:10px 0; } .activity-days { height:18px; margin-top:7px; } .history-heading { padding-top:10px; } }
</style>
