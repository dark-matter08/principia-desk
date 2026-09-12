<script lang="ts">
  import { untrack } from 'svelte';
  import { Plus, Check, X, Search } from 'lucide-svelte';
  import { api } from '$lib/ipc';
  import type { ModelCatalog } from '$lib/contracts/agents';
  import { modelPage } from './model-list';
  let { runner, models = $bindable([]), freeOnly = false, revision = 0 }: { runner: string; models?: string[]; freeOnly?: boolean; revision?: number } = $props();
  let catalogue = $state<ModelCatalog>({ models: [], source: '', error: null });
  let query = $state(''); let page = $state(0); let manual = $state(''); let manualOpen = $state(false);
  let loading = $state(false); let error = $state(''); let sequence = 0;
  /** A page is a grid of cards, so it holds more than a list did without growing taller. */
  const PAGE = 12;
  const results = $derived(modelPage(catalogue.models, query, page, freeOnly, PAGE));
  async function load(refresh: boolean) {
    const token = ++sequence; loading = true;
    try { const result = await api.getRunnerModels(runner, refresh); if (token === sequence) catalogue = result; }
    catch (e) { if (token === sequence) error = String(e); }
    finally { if (token === sequence) loading = false; }
  }
  $effect(() => { void runner; void revision; untrack(() => { query = ''; page = 0; void load(false); }); return () => { sequence++; }; });
  function toggle(id: string) { models = models.includes(id) ? models.filter(m => m !== id) : [...models, id]; }
  function addManual() {
    const id = manual.trim();
    if (!id || id.length > 160 || /\s/.test(id)) { error = 'Enter a model ID without spaces.'; return; }
    if (!models.includes(id)) models = [...models, id]; manual = ''; error = '';
  }
</script>

<div class="model-library">
  <div class="line"><span class="eyebrow">MODEL SHORTLIST <b>{models.length}</b>{#if loading}<i class="loading" role="status">· loading catalogue…</i>{/if}</span><button type="button" class="text-action" onclick={() => manualOpen = !manualOpen}>Add by ID</button></div>
  <div class="chips" aria-label="Models in this runner’s shortlist">
    {#each models as id, index (id)}<span class="chip"><span class="chip-no mono">{String(index + 1).padStart(2, '0')}</span><span class="chip-id">{id}</span><button type="button" class="chip-off" aria-label={`Remove ${id} from shortlist`} onclick={() => toggle(id)}><X size={11} /></button></span>{/each}
    {#if !models.length}<p>No saved models yet. Add models from the catalogue below.</p>{/if}
  </div>
  {#if manualOpen}<div class="manual"><input aria-label="Model ID to add" placeholder="Provider model ID" bind:value={manual} onkeydown={e => { if (e.key === 'Enter') { e.preventDefault(); addManual(); } }} /><button type="button" class="ghost mono-ghost small" onclick={addManual}>Add</button></div>{/if}
  <div class="search-row"><div class="search field-group"><Search size={13} /><input aria-label="Search model catalogue" placeholder="Search models by name or ID…" value={query} oninput={e => { query = e.currentTarget.value; page = 0; }} /></div><button type="button" class="ghost mono-ghost small" disabled={loading} onclick={() => load(true)}>{loading ? 'Loading…' : 'Refresh'}</button></div>
  <div class="catalogue" aria-label="Model catalogue" aria-busy={loading}>
    {#if loading && !catalogue.models.length}
      {#each [0, 1, 2, 3, 4, 5] as row (row)}
        <div class="model-row skeleton" aria-hidden="true"><span class="model-copy"><span class="bar wide"></span><span class="bar"></span></span></div>
      {/each}
    {/if}
    {#each results.items as item (item.id)}
      <button type="button" class="model-row" class:added={models.includes(item.id)} aria-pressed={models.includes(item.id)} onclick={() => toggle(item.id)}>
        <span class="model-copy"><strong>{item.label}</strong><small>{item.id}</small></span>
        <span class="model-foot"><span class="price">{item.free ? 'FREE' : item.input_usd_per_million === null ? '' : `$${item.input_usd_per_million.toFixed(2)} / $${item.output_usd_per_million?.toFixed(2) ?? '?'}`}</span><span class="mark">{#if models.includes(item.id)}<Check size={12} />{:else}<Plus size={12} />{/if}</span></span>
      </button>
    {/each}
    {#if !results.count && !loading}<p class="empty">{query ? 'No models match your search.' : 'No catalogue is available. Add a model by ID, or configure a provider key first.'}</p>{/if}
  </div>
  <div class="pagination"><span>{loading && !catalogue.models.length ? 'reading catalogue…' : `${results.count} models`}{results.count ? ` · ${results.current * PAGE + 1}–${Math.min((results.current + 1) * PAGE, results.count)}` : ''}</span><div><button type="button" class="ghost mono-ghost small" aria-label="Previous model page" disabled={results.current === 0} onclick={() => page = results.current - 1}>←</button><span>{results.current + 1} / {results.pages}</span><button type="button" class="ghost mono-ghost small" aria-label="Next model page" disabled={results.current + 1 >= results.pages} onclick={() => page = results.current + 1}>→</button></div></div>
  {#if runner === 'openrouter'}<p>Prices show input / output per million tokens.</p>{/if}
  {#if catalogue.error || error}<p class="error" role="status">{error || catalogue.error}</p>{/if}
</div>

<style>
  .model-library { display: grid; gap: 10px; min-width: 0; }
  .line,.pagination,.pagination > div { display: flex; justify-content: space-between; align-items: center; gap: 9px; }
  .eyebrow,.pagination,.price { color: var(--muted); font: 9px/1.5 var(--font-mono); } .eyebrow { letter-spacing: .7px; } b { color: var(--accent); margin-left: 5px; }
  .chips { display: flex; flex-wrap: wrap; gap: 5px; max-height: 96px; overflow-y: auto; }
  /* A saved model is a tag: its place in the list on a lit stub, the ID, and a
     well that takes it off the list. */
  .chip { position: relative; display: inline-flex; align-items: stretch; max-width: 100%; border: 1px solid var(--node-border); border-radius: 6px; background: linear-gradient(180deg, var(--surface), color-mix(in srgb, var(--surface) 60%, var(--bg))); color: var(--text); font: 10px/1.5 var(--font-mono); overflow: hidden; transition: border-color 0.15s ease, box-shadow 0.15s ease; }
  .chip:hover { border-color: color-mix(in srgb, var(--accent) 45%, var(--node-border)); box-shadow: 0 6px 16px -12px var(--accent); }
  .chip-no { display: grid; place-items: center; padding: 0 6px; font-size: 8.5px; letter-spacing: 0.6px; color: var(--accent); background: color-mix(in srgb, var(--accent) 14%, var(--surface-2)); border-right: 1px solid color-mix(in srgb, var(--accent) 25%, var(--node-border)); }
  .chip-id { padding: 5px 9px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .chip-off { display: grid; place-items: center; width: 24px; padding: 0; border: 0; border-left: 1px solid var(--node-divider); background: transparent; color: var(--faint); cursor: pointer; transition: color 0.15s ease, background 0.15s ease; }
  .chip-off:hover { color: var(--led-err); background: color-mix(in srgb, var(--led-err) 12%, transparent); } .chip-off:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
  .search-row,.manual { display: flex; gap: 7px; } .search:focus-within { border-color: var(--accent); } .search input { outline: none; } .search { border-radius: var(--radius-control); overflow: hidden; display: flex; align-items: center; gap: 7px; flex: 1; min-width: 0; border: 1px solid var(--node-border); padding-left: 9px; background: var(--bg); color: var(--muted); }
  input { width: 100%; min-width: 0; padding: 8px; background: var(--bg); color: var(--text); border: 1px solid var(--node-border); font-size: 11px; } .search input { border: 0; }
  /* The catalogue is a grid of small cards: more models in view, less height. */
  .catalogue { display: grid; grid-template-columns: repeat(auto-fill, minmax(180px, 1fr)); gap: 6px; padding-top: 10px; border-top: 1px solid var(--node-border); }
  .model-row { display: flex; flex-direction: column; justify-content: space-between; gap: 8px; text-align: left; padding: 9px 10px; min-height: 66px; width: 100%; background: var(--bg); border: 1px solid var(--node-border); border-radius: var(--radius-control); color: var(--muted); cursor: pointer; transition: border-color 120ms ease, background 120ms ease; }
  .model-row:hover { border-color: var(--muted); } .model-row.added { border-color: var(--accent); background: color-mix(in srgb, var(--accent) 8%, var(--bg)); }
  .model-copy { min-width: 0; } strong { color: var(--text); font-size: 11px; font-weight: 500; display: block; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; } small { font-size: 9px; display: block; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .model-foot { display: flex; align-items: center; justify-content: space-between; gap: 6px; } .mark { display: grid; place-items: center; width: 18px; height: 18px; border-radius: 50%; border: 1px solid var(--node-border); color: var(--muted); } .model-row.added .mark { background: var(--accent); border-color: var(--accent); color: var(--accent-fg); } .model-row :global(svg) { flex-shrink: 0; }
  button:disabled { opacity: .4; cursor: default; }
  .text-action { color: var(--accent); background: transparent; border: 0; font-size: 10px; cursor: pointer; }
  .loading { font-style: normal; color: var(--accent); margin-left: 6px; }
  .skeleton { pointer-events: none; } .skeleton .model-copy { display: grid; gap: 7px; padding: 2px 0; }
  .bar { display: block; height: 9px; width: 42%; border-radius: var(--radius-detail); background: linear-gradient(90deg, var(--surface) 25%, var(--surface-2) 37%, var(--surface) 63%); background-size: 320% 100%; animation: catalogue-shimmer 1.3s ease-in-out infinite; }
  .bar.wide { width: 68%; height: 11px; }
  @keyframes catalogue-shimmer { from { background-position: 100% 0; } to { background-position: 0 0; } }
  @media (prefers-reduced-motion: reduce) { .bar { animation: none; } }
  p { margin: 0; color: var(--muted); font-size: 10px; line-height: 1.6; } .empty { padding: 15px 0; } .error { color: var(--led-err); overflow-wrap: anywhere; }
</style>
