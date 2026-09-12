<script lang="ts">
  import { untrack } from 'svelte';
  import { app } from '../../stores.svelte';
  import type { ClassroomSubjectId } from '../../ipc';
  import CourseGlyph from '../../components/CourseGlyph.svelte';
  import Dropdown from '../../components/Dropdown.svelte';
  import { Search, X, Plus, PenLine } from 'lucide-svelte';
  import ClassDetail from './ClassDetail.svelte';
  import ClassBuilder from './builder/ClassBuilder.svelte';
  import { api, type CustomCourseSummary } from '../../ipc';
  import { filterClasses, type ClassFilter, type ClassTab } from './class-navigation';

  const initialSelection = app.classSelection;
  const initialTab = app.classTab;
  let query = $state('');
  let filter = $state<ClassFilter>('all');
  let selected = $state<ClassroomSubjectId | null>(null);
  let visited = $state<ClassroomSubjectId[]>([]);
  const tabsByClass = new Map<ClassroomSubjectId, ClassTab>();
  let searchInput: HTMLInputElement;
  const programs = $derived(app.state?.classroom_programs ?? []);
  const slots = $derived(app.state?.classroom_slots ?? []);
  const sessions = $derived(app.state?.active_classroom_sessions ?? []);
  const visible = $derived(filterClasses(programs, query, filter, id => slots.some(s => s.subject_id === id) || sessions.some(s => s.subject_id === id)));
  const options = [{ value: 'all', label: 'All classes' }, { value: 'active', label: 'Active' }, { value: 'paused', label: 'Paused' }, { value: 'completed', label: 'Completed' }];
  $effect(() => { if (!selected && programs.length) select(app.classSelection ?? programs[0].subject_id); });
  // A class asked for by name while the workspace is open: switch to it and
  // hand its detail the tab.
  $effect(() => {
    const request = app.classRequest;
    if (!request) return;
    untrack(() => {
      if (!programs.some((p) => p.subject_id === request.id)) return;
      tabsByClass.set(request.id, request.tab);
      app.builder = null;
      select(request.id);
    });
  });
  /** Classes being built: drafts not yet published. */
  let drafts = $state<CustomCourseSummary[]>([]);
  async function loadDrafts() { try { drafts = (await api.listCustomCourses()).filter((c) => c.status === 'draft'); } catch { drafts = []; } }
  $effect(() => { void app.state; void app.builder; void loadDrafts(); });
  const building = $derived(app.builder);
  function closeBuilder() { app.builder = null; void loadDrafts(); }
  /** A class just published opens like any other, on its overview; the starting point and study times are set from there. */
  function published(id: string) { app.builder = null; void loadDrafts(); tabsByClass.set(id as ClassroomSubjectId, 'overview'); select(id as ClassroomSubjectId); }
  function select(id: ClassroomSubjectId) {
    selected = id;
    app.classSelection = id;
    app.classTab = tabsByClass.get(id) ?? (id === initialSelection ? initialTab : 'overview');
    if (!visited.includes(id)) visited = [...visited, id];
  }
  function rememberTab(id: ClassroomSubjectId, tab: ClassTab) {
    tabsByClass.set(id, tab);
    if (selected === id) app.classTab = tab;
  }
  function move(event: KeyboardEvent) {
    if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return;
    const buttons = [...(event.currentTarget as HTMLElement).closest('nav')!.querySelectorAll<HTMLButtonElement>('[data-course]')];
    const index = buttons.indexOf(event.target as HTMLButtonElement);
    if (index < 0) return;
    event.preventDefault();
    const next = event.key === 'Home' ? 0 : event.key === 'End' ? buttons.length - 1 : (index + (event.key === 'ArrowDown' ? 1 : -1) + buttons.length) % buttons.length;
    buttons[next]?.focus();
  }
  function clear() { query = ''; filter = 'all'; searchInput?.focus(); }
</script>

<section class="workspace" aria-label="Class workspace">
  <aside aria-label="Class browser">
    <div class="browser-tools">
      <div class="browser-heading"><span class="mono">YOUR CLASSES</span><span class="count mono">{programs.length}</span></div>
      <button type="button" class="bracket new-class" class:on={building === 'new'} onclick={() => app.openBuilder('new')}><span class="bracket-well"><Plus size={12} /></span>New class</button>
      <div class="search"><Search size={15} /><input bind:this={searchInput} type="search" aria-label="Search classes" placeholder="Find a class…" bind:value={query} />{#if query}<button aria-label="Clear class search" onclick={() => { query = ''; searchInput.focus(); }}><X size={14} /></button>{/if}</div>
      <Dropdown label="Show classes" bind:value={filter} {options} />
    </div>
    <nav class="class-list" aria-label="Choose a class">
      {#if drafts.length}
        <div class="drafts-heading mono">DRAFTS <span>{drafts.length}</span></div>
        {#each drafts as item (item.id)}
          <button onkeydown={move} data-course class="class-row draft" class:selected={building === item.id} aria-current={building === item.id ? 'true' : undefined} onclick={() => app.openBuilder(item.id)}>
            <span class="draft-glyph"><PenLine size={15} /></span>
            <span class="row-copy"><strong>{item.label || 'Untitled class'}</strong><small><i class="pen" class:live={!!item.working}></i>{item.working ? `tutor ${item.working === 'draft' ? 'drafting' : item.working === 'review' ? 'reviewing' : item.working === 'bank' ? 'writing questions' : item.working === 'fix' ? 'changing the draft' : 'fetching sources'}…` : `${item.topics} ${item.topics === 1 ? 'topic' : 'topics'} · ${item.origin}`}<span class="progress mono">draft</span></small></span>
          </button>
        {/each}
        <div class="drafts-heading mono">CLASSES</div>
      {/if}
      {#each visible as program (program.subject_id)}
        {@const due = slots.some(s => s.subject_id === program.subject_id && s.owed)}
        {@const running = sessions.some(s => s.subject_id === program.subject_id)}
        <button onkeydown={move} data-course class="class-row" class:selected={!building && selected === program.subject_id} aria-current={!building && selected === program.subject_id ? 'true' : undefined} onclick={() => { app.builder = null; select(program.subject_id); }}>
          <CourseGlyph courseId={program.subject_id} size={34} />
          <span class="row-copy"><strong>{program.label}</strong><small><i class:enabled={program.enabled} class:due></i>{running ? 'In progress' : due ? 'Study time due' : program.completed ? 'Completed' : program.enabled ? 'Active' : 'Inactive'}<span class="progress mono">{Math.round(program.progress * 100)}%</span></small></span>
        </button>
      {:else}<div class="empty"><p>No classes match.</p><button class="ghost mono-ghost" onclick={clear}>Clear filters</button></div>{/each}
    </nav>
    <div class="browser-footer mono">{visible.length} of {programs.length} classes<span>↑ ↓ to browse</span></div>
  </aside>
  <div class="details">
    {#if building}
      {#key building}<ClassBuilder id={building === 'new' ? null : building} onclose={closeBuilder} onpublished={published} />{/key}
    {/if}
    {#each programs.filter(p => visited.includes(p.subject_id)) as program (program.subject_id)}
      <div class="detail-instance" hidden={!!building || selected !== program.subject_id}><ClassDetail {program} initialTab={tabsByClass.get(program.subject_id) ?? (program.subject_id === (initialSelection ?? programs[0]?.subject_id) ? initialTab : 'overview')} requested={app.classRequest?.id === program.subject_id ? app.classRequest : null} ontabchange={tab => rememberTab(program.subject_id, tab)} /></div>
    {/each}
    {#if !selected && !building}<div class="empty"><p role="status">Loading your classes…</p></div>{/if}
  </div>
</section>

<style>
  .workspace { display: grid; grid-template-columns: 250px minmax(0, 1fr); flex: 1; min-height: 0; min-width: 0; overflow: hidden; border: 1px solid var(--node-border); border-radius: var(--radius-panel); background: var(--node-bg); box-shadow: 0 8px 30px #0002; }
  aside { display: flex; flex-direction: column; min-height: 0; min-width: 0; border-right: 1px solid var(--node-border); background: var(--bg); }
  .browser-tools { padding: 18px 14px 14px; display: grid; gap: 12px; border-bottom: 1px solid var(--node-divider); }
  .browser-heading { display: flex; align-items: center; justify-content: space-between; color: var(--muted); font-size: 10px; letter-spacing: 1px; }
  .count { padding: 2px 6px; background: var(--surface-2); border-radius: var(--radius-detail); color: var(--violet-fg); }
  .search { display: flex; align-items: center; gap: 8px; padding: 0 10px; min-height: 39px; border: 1px solid var(--node-border); border-radius: var(--radius-control); color: var(--muted); background: var(--node-bg); }
  .search:focus-within { border-color: var(--accent); outline: 1px solid var(--accent); }
  .search input { width: 100%; min-width: 0; background: none; border: 0; outline: none; padding: 9px 0; color: var(--fg); font: 12px var(--font-body); }
  .search input::-webkit-search-cancel-button { display: none; }
  .search button { display: grid; place-items: center; padding: 4px; background: none; border: 0; color: var(--muted); cursor: pointer; }
  .class-list { padding: 8px; flex: 1; min-height: 0; overflow-y: auto; overscroll-behavior: contain; scrollbar-gutter: stable; }
  .class-row { width: 100%; display: flex; align-items: center; gap: 10px; padding: 12px 9px; border: 1px solid transparent; border-radius: var(--radius-control); background: none; text-align: left; color: var(--fg); cursor: pointer; }
  .class-row + .class-row { margin-top: 3px; }
  .class-row:hover { background: var(--node-bg); border-color: var(--node-border); }
  .class-row.selected { background: var(--surface-2); border-color: var(--violet); box-shadow: inset 3px 0 var(--violet-fg); }
  .new-class { width: 100%; justify-content: center; } .new-class.on { border-color: color-mix(in srgb, var(--accent) 45%, var(--border)); color: var(--fg); }
  .drafts-heading { display: flex; justify-content: space-between; padding: 8px 9px 4px; font-size: 9px; letter-spacing: 1px; color: var(--faint); } .drafts-heading span { color: var(--accent); }
  .draft-glyph { flex: none; display: grid; place-items: center; width: 34px; height: 34px; border: 1px dashed color-mix(in srgb, var(--accent) 45%, var(--node-border)); border-radius: 6px; color: var(--accent); background: var(--bg); }
  .row-copy small i.pen { background: var(--accent); } .row-copy small i.pen.live { animation: pulse 1.2s ease-in-out infinite; } @keyframes pulse { 50% { opacity: 0.3; } }
  .row-copy { min-width: 0; flex: 1; }
  strong { display: block; font-size: 12px; line-height: 1.45; font-weight: 500; }
  small { display: flex; gap: 5px; align-items: center; color: var(--muted); font-size: 10px; margin-top: 5px; }
  i { width: 5px; height: 5px; flex-shrink: 0; border-radius: 50%; background: var(--muted); } i.enabled { background: var(--led-ok); } i.due { background: var(--accent); }
  .progress { margin-left: auto; font-size: 9px; }
  .browser-footer { padding: 12px 14px; display: flex; justify-content: space-between; gap: 5px; border-top: 1px solid var(--node-divider); font-size: 9px; color: var(--muted); }
  .browser-footer span { font-size: 8px; }
  .details, .detail-instance { min-height: 0; min-width: 0; display: flex; flex-direction: column; flex: 1; overflow: hidden; }
  .detail-instance[hidden] { display: none; }
  .empty { padding: 20px 12px; color: var(--muted); font-size: 12px; }
  button:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
  @media(max-width:1000px) { .workspace { grid-template-columns: 218px minmax(0, 1fr); } }
  @media(max-width:700px) { .workspace { grid-template-columns: 180px minmax(0, 1fr); } .class-row :global(svg) { display: none; } .browser-tools { padding: 12px 10px; } .browser-footer span { display: none; } }
  @media(max-width:520px) { .workspace { grid-template-columns: minmax(0, 1fr); grid-template-rows: 170px minmax(0, 1fr); } aside { display: grid; grid-template-columns: 145px 1fr; border-right: 0; border-bottom: 1px solid var(--node-border); } .browser-tools { grid-row: 1 / 3; gap: 5px; border: 0; } .class-list { grid-column: 2; } .browser-footer { display: none; } .class-row { padding: 6px; } }
</style>
