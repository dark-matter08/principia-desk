<script lang="ts">
  /**
   * The lessons this class has held, newest first, as a paged grid of small
   * cards: the date, the title, how it ended and the score. A card opens the
   * lesson again on its own screen (read, listen, download, the check as it
   * was answered); its download key saves it without opening it.
   */
  import { api, type DashboardView, type ProgressEntry } from '../../ipc';
  import { app } from '../../stores.svelte';
  import { Search, X, ChevronLeft, ChevronRight, ArrowUpRight, BookOpen, Loader } from 'lucide-svelte';
  import Dropdown from '../../components/Dropdown.svelte';
  import DownloadMenu from '../lessons/DownloadMenu.svelte';

  let { courseId, label }: { courseId: string; label: string } = $props();

  let data = $state<DashboardView | null>(null);
  let busy = $state(false);
  let error = $state('');
  let search = $state('');
  let status = $state('');
  let page = $state(0);
  let opening = $state('');

  const statusOptions = [{ value: '', label: 'Every result' }, { value: 'completed', label: 'Completed' }, { value: 'in_progress', label: 'Open' }, { value: 'skipped', label: 'Skipped' }];
  const pages = $derived(Math.max(1, Math.ceil((data?.history_total ?? 0) / (data?.page_size ?? 8))));
  const filtered = $derived(!!search.trim() || !!status);

  $effect(() => {
    const query = { subject_id: courseId, search, status: status || null, page };
    let disposed = false;
    busy = true;
    const timer = setTimeout(() => {
      api.getDashboard(query)
        .then((result) => { if (!disposed) { data = result; error = ''; } })
        .catch((cause) => { if (!disposed) error = String(cause); })
        .finally(() => { if (!disposed) busy = false; });
    }, search ? 180 : 0);
    return () => { disposed = true; clearTimeout(timer); };
  });

  function dateLabel(date: string) {
    return new Date(`${date}T12:00:00`).toLocaleDateString(undefined, { weekday: 'short', day: 'numeric', month: 'short', year: 'numeric' });
  }
  function resultLabel(entry: ProgressEntry) {
    return entry.status === 'completed' ? 'completed' : entry.status === 'skipped' ? 'skipped' : 'open';
  }
  /** A shared-runtime lesson opens on its own screen; the earlier stores open in the ledger's reader. */
  async function open(entry: ProgressEntry) {
    if (opening) return;
    opening = `${entry.source}:${entry.owner_id}`;
    try {
      if (entry.source === 'study') await app.revisitLesson(entry.owner_id);
      else { app.progressRequest = entry; app.navigate('progress'); }
    } finally { opening = ''; }
  }
</script>

<section class="lessons" aria-labelledby="class-lessons-heading">
  <div class="head">
    <div><span class="eyebrow mono">LESSONS HELD</span><h3 id="class-lessons-heading">{label} so far</h3><p>Every lesson this class has held, newest first. Open one to read it again, hear it, or save it; the check stays as you answered it.</p></div>
    <span class="count mono" role="status">{busy ? 'Updating…' : `${data?.history_total ?? 0} ${data?.history_total === 1 ? 'lesson' : 'lessons'}`}</span>
  </div>
  <div class="tools">
    <div class="search"><Search size={14} /><input type="search" aria-label="Find a lesson" placeholder="Find a lesson…" value={search} oninput={(event) => { search = event.currentTarget.value; page = 0; }} />{#if search}<button type="button" aria-label="Clear the search" onclick={() => { search = ''; page = 0; }}><X size={13} /></button>{/if}</div>
    <Dropdown hideLabel label="Filter by result" value={status} options={statusOptions} onchange={(value: string) => { status = value; page = 0; }} />
  </div>

  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if !data}
    <p class="line mono"><Loader size={11} class="spin" /> reading the class's record…</p>
  {:else if !data.history.length}
    <div class="empty"><BookOpen size={26} /><h4>{filtered ? 'No lesson matches' : 'No lesson held yet'}</h4><p>{filtered ? 'Try another word or result.' : `The first ${label} lesson appears here once it is held.`}</p>{#if filtered}<button type="button" class="ghost mono-ghost small" onclick={() => { search = ''; status = ''; page = 0; }}>Clear</button>{/if}</div>
  {:else}
    <ol class="grid" aria-label="Lessons held">
      {#each data.history as entry, index (`${entry.source}:${entry.owner_id}`)}
        {@const key = `${entry.source}:${entry.owner_id}`}
        <li class="card" class:completed={entry.status === 'completed'} class:skipped={entry.status === 'skipped'}>
          <div class="card-head"><span class="index mono">{String(data.page * data.page_size + index + 1).padStart(2, '0')}</span><span class="date mono">{dateLabel(entry.date)}</span></div>
          {#if entry.can_read}
            <button type="button" class="title" disabled={!!opening} onclick={() => open(entry)}><span>{entry.title}</span><ArrowUpRight size={13} /></button>
          {:else}
            <strong class="title">{entry.title}</strong>
          {/if}
          <div class="card-foot">
            <span class="result mono">{opening === key ? 'opening…' : resultLabel(entry)}</span>
            <span class="score mono">{entry.score == null ? '—' : `${Math.round(entry.score * 100)}%`}</span>
            {#if entry.can_read}<span class="save"><DownloadMenu source={entry.source} ownerId={entry.owner_id} compact /></span>{/if}
          </div>
        </li>
      {/each}
    </ol>
    {#if pages > 1}
      <footer class="pager mono">
        <span>{data.page * data.page_size + 1}–{Math.min((data.page + 1) * data.page_size, data.history_total)} of {data.history_total}</span>
        <div><button type="button" class="icon" aria-label="Earlier page" disabled={busy || data.page === 0} onclick={() => (page = Math.max(0, (data?.page ?? 0) - 1))}><ChevronLeft size={15} /></button><span>page {data.page + 1} / {pages}</span><button type="button" class="icon" aria-label="Later page" disabled={busy || data.page + 1 >= pages} onclick={() => (page = (data?.page ?? 0) + 1)}><ChevronRight size={15} /></button></div>
      </footer>
    {/if}
  {/if}
</section>

<style>
  .lessons { max-width: 1000px; margin: 0 auto; }
  .head { display: flex; align-items: flex-end; justify-content: space-between; gap: 18px; margin-bottom: 14px; }
  .eyebrow { font-size: 10px; color: var(--muted); letter-spacing: 0.7px; }
  h3 { font: 25px/1.3 var(--font-display); margin: 8px 0 6px; }
  .head p { margin: 0; font-size: 13px; line-height: 1.6; color: var(--muted); max-width: 640px; }
  .count { flex: none; font-size: 9px; color: var(--muted); padding-bottom: 6px; }
  .tools { display: grid; grid-template-columns: minmax(0, 1fr) 170px; gap: 12px; margin-bottom: 14px; }
  .search { display: flex; align-items: center; gap: 8px; padding: 0 10px; border: 1px solid var(--node-border); border-radius: var(--radius-control); background: var(--bg); color: var(--muted); }
  .search input { flex: 1; min-width: 0; height: 34px; border: 0; background: transparent; color: var(--fg); font-size: 12px; outline: none; }
  .search button { display: grid; place-items: center; width: 22px; height: 22px; border: 0; border-radius: var(--radius-detail); background: transparent; color: var(--muted); cursor: pointer; }
  .search button:hover { color: var(--fg); background: var(--surface-2); }
  .grid { list-style: none; margin: 0; padding: 0; display: grid; grid-template-columns: repeat(auto-fill, minmax(232px, 1fr)); gap: 10px; }
  .card { display: flex; flex-direction: column; gap: 8px; min-width: 0; padding: 12px 14px; border: 1px solid var(--node-border); border-radius: var(--radius-panel); background: var(--bg); transition: border-color 120ms ease; }
  .card:hover { border-color: color-mix(in srgb, var(--accent) 45%, var(--node-border)); }
  .card-head { display: flex; align-items: center; justify-content: space-between; gap: 8px; font-size: 9px; color: var(--muted); letter-spacing: 0.4px; }
  .index { color: var(--faint); }
  .title { display: flex; align-items: flex-start; justify-content: space-between; gap: 8px; margin: 0; padding: 0; border: 0; background: none; color: var(--fg); font: 13px/1.45 var(--font-body); font-weight: 400; text-align: left; cursor: pointer; }
  .title :global(svg) { flex: none; margin-top: 3px; color: var(--accent); }
  button.title:hover:not(:disabled) { color: var(--accent); } button.title:disabled { cursor: default; }
  strong.title { cursor: default; }
  .card-foot { display: flex; align-items: center; gap: 8px; margin-top: auto; padding-top: 4px; }
  .result { padding: 3px 6px; border-radius: var(--radius-detail); font-size: 9px; background: var(--warn-bg); color: var(--warn-fg); }
  .card.completed .result { color: var(--ok-fg); background: var(--ok-bg); }
  .card.skipped .result { color: var(--muted); background: var(--surface); }
  .score { font-size: 10px; color: var(--muted); }
  .save { margin-left: auto; }
  .save :global(.download > button) { min-height: 24px; padding: 3px 7px; border: 1px solid var(--node-border); border-radius: var(--radius-control); background: var(--surface); color: var(--muted); font: 9.5px var(--font-mono); cursor: pointer; }
  .save :global(.download > button:hover:not(:disabled)) { color: var(--fg); border-color: var(--accent); }
  .pager { display: flex; align-items: center; justify-content: space-between; gap: 12px; margin-top: 14px; font-size: 9px; color: var(--muted); }
  .pager div { display: flex; align-items: center; gap: 8px; }
  .icon { display: grid; place-items: center; width: 28px; height: 28px; border: 1px solid var(--node-border); border-radius: var(--radius-control); background: var(--surface); color: var(--muted); cursor: pointer; }
  .icon:hover:not(:disabled) { color: var(--fg); } .icon:disabled { opacity: 0.4; cursor: default; }
  .line { display: flex; align-items: center; gap: 8px; margin: 0; font-size: 10.5px; color: var(--muted); }
  .empty { display: grid; justify-items: center; gap: 6px; padding: 36px 16px; border: 1px dashed var(--node-border); border-radius: var(--radius-panel); color: var(--muted); text-align: center; }
  .empty h4 { margin: 6px 0 0; font: 16px var(--font-display); color: var(--fg); } .empty p { margin: 0; font-size: 12px; }
  .error { color: var(--led-err); font-size: 12px; overflow-wrap: anywhere; }
  :global(.spin) { animation: spin 1.1s linear infinite; } @keyframes spin { to { transform: rotate(360deg); } }
  button:focus-visible, input:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
  @media (max-width: 700px) { .tools { grid-template-columns: 1fr; } .head { flex-direction: column; align-items: flex-start; } }
</style>
