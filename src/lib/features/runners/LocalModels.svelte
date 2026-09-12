<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/ipc';
  import type { HealthCheck, LocalStatus, LocalPull } from '$lib/contracts/agents';
  import { OLLAMA_MODELS, sameModel, fitsInRam, prettyBytes, chatCandidate } from './local-models';
  import { openRunnerLink } from './links';
  let { models = $bindable([]), onchanged }: { models?: string[]; onchanged: () => void | Promise<void> } = $props();
  let status = $state<LocalStatus | null>(null); let pulls = $state<LocalPull[]>([]);
  let tab = $state('installed'); let query = $state(''); let page = $state(0); let showAll = $state(false);
  let busy = $state(''); let message = $state(''); let custom = $state(''); let customOpen = $state(false);
  let result = $state<HealthCheck | null>(null); let error = $state(''); let confirmRemove = $state(''); let details = $state('');
  let mounted = true;
  const shelf = $derived(OLLAMA_MODELS.filter(m => (showAll || !status?.memory_gb || fitsInRam(m, status.memory_gb)) && `${m.label} ${m.id} ${m.caps.join(' ')}`.toLowerCase().includes(query.toLowerCase())));
  const installed = $derived((status?.models ?? []).filter(m => m.name.toLowerCase().includes(query.toLowerCase())));
  const count = $derived(tab === 'browse' ? shelf.length : installed.length);
  const pages = $derived(Math.max(1, Math.ceil(count / 5)));
  const current = $derived(Math.min(page, pages - 1));
  async function refresh() { const values = await Promise.all([api.getLocalModels(), api.getLocalPulls()]); if (mounted) [status, pulls] = values; }
  onMount(() => { mounted = true; void refresh().catch(e => error = String(e)); const timer = setInterval(() => { if (pulls.some(p => !p.done)) void refresh().catch(e => error = String(e)); }, 1200); return () => { mounted = false; clearInterval(timer); }; });
  function toggle(name: string) { models = models.some(m => sameModel(m,name)) ? models.filter(m => !sameModel(m,name)) : [...models,name]; }
  function changeTab(next: string) { tab = next; query = ''; page = 0; details = ''; }
  async function action(kind: string, model = '') {
    if (busy) return; busy = `${kind}:${model}`; error = ''; message = '';
    try {
      if (kind === 'install') message = await api.installLocalRunner();
      if (kind === 'start') { await api.startLocalRunner(); message = 'Ollama is running.'; }
      if (kind === 'pull') { await api.pullLocalModel(model); changeTab('downloads'); message = 'The download continues while you use the rest of your desk.'; }
      if (kind === 'test') result = await api.testAgentConnection('ollama', '', model);
      if (kind === 'remove') { await api.removeLocalModel(model); confirmRemove = ''; message = `Removed ${model}.`; }
      await refresh(); await onchanged();
    } catch (e) { error = String(e); } finally { busy = ''; }
  }
</script>

<div class="local-pane">
  <div class="status-line"><strong>Ollama</strong><span class:running={status?.running}>{status ? status.running ? 'Running' : status.installed ? 'Installed · stopped' : 'Not installed' : 'Checking…'}</span><span>{status?.memory_gb ? `${status.memory_gb} GB RAM` : ''}</span><button type="button" class="ghost mono-ghost small" onclick={() => refresh().catch(e => error = String(e))}>Refresh</button></div>
  {#if status && !status.running}<div class="install"><p>{status.installed ? 'Start Ollama to see downloaded models and run them on this machine.' : status.install_command}</p>{#if status.installed}<button type="button" class="ghost mono-ghost small" disabled={!!busy} onclick={() => action('start')}>{busy.startsWith('start') ? 'Starting…' : 'Start Ollama'}</button>{:else if status.can_install}<button type="button" class="ghost mono-ghost small" disabled={!!busy} onclick={() => action('install')}>{busy.startsWith('install') ? 'Installing…' : 'Install Ollama'}</button>{:else}<a href="https://ollama.com/download" target="_blank" rel="noreferrer" onclick={e => openRunnerLink(e).catch(e => error = String(e))}>Get Ollama →</a>{/if}</div>{/if}
  <div class="tabs" aria-label="Local model library"><button type="button" class:active={tab === 'installed'} aria-pressed={tab === 'installed'} onclick={() => changeTab('installed')}>Installed <span>{status?.models.length ?? 0}</span></button><button type="button" class:active={tab === 'browse'} aria-pressed={tab === 'browse'} onclick={() => changeTab('browse')}>Browse models</button><button type="button" class:active={tab === 'downloads'} aria-pressed={tab === 'downloads'} onclick={() => changeTab('downloads')}>Downloads <span>{pulls.filter(p => !p.done).length || ''}</span></button></div>
  {#if tab === 'downloads'}
    {#each pulls as pull (pull.model)}<div class="pull"><div><strong>{pull.model}</strong><span>{pull.status}{pull.percent === null ? '' : ` · ${pull.percent}%`}</span></div><div class="track" role="progressbar" aria-label={`Downloading ${pull.model}`} aria-valuenow={pull.percent ?? undefined} aria-valuemin="0" aria-valuemax="100"><span style:width={`${pull.percent ?? 2}%`}></span></div>{#if pull.error}<p class="error">{pull.error}</p>{/if}</div>{/each}
    {#if !pulls.length}<p class="empty">No downloads yet. Browse models to add one to this machine.</p>{/if}
  {:else}
    <div class="search-row"><input aria-label={tab === 'installed' ? 'Search installed models' : 'Search local model shelf'} placeholder="Find a model…" value={query} oninput={e => { query = e.currentTarget.value; page = 0; }} />{#if tab === 'browse'}<button type="button" class="ghost mono-ghost small" aria-pressed={!showAll} onclick={() => { showAll = !showAll; page = 0; }}>{showAll ? 'All sizes' : 'Fits this machine'}</button>{/if}</div>
    {#if tab === 'installed'}
      <p>Add downloaded chat models to your shortlist, then save the runner setup below.</p>
      {#each installed.slice(current * 5, current * 5 + 5) as item (item.name)}
        <div class="model-row"><div class="row-main"><div class="model-copy"><strong>{item.name}</strong><span>{prettyBytes(item.size_bytes)}{chatCandidate(item.name) ? '' : ' · not a local chat model'}</span></div><button type="button" class="ghost mono-ghost small" disabled={!!busy || !chatCandidate(item.name)} aria-pressed={models.some(m => sameModel(m,item.name))} onclick={() => toggle(item.name)}>{models.some(m => sameModel(m,item.name)) ? 'Added ✓' : '+ Shortlist'}</button><button type="button" class="ghost mono-ghost small" aria-expanded={details === item.name} aria-label={`Manage ${item.name}`} onclick={() => details = details === item.name ? '' : item.name}>•••</button></div>
          {#if details === item.name}<div class="detail"><button type="button" disabled={!!busy || !chatCandidate(item.name)} class="ghost mono-ghost small" onclick={() => action('test',item.name)}>{busy === `test:${item.name}` ? 'Testing…' : 'Test response'}</button><button type="button" disabled={!!busy} class="ghost mono-ghost small" onclick={() => confirmRemove = item.name}>Remove weights</button>{#if confirmRemove === item.name}<p>Remove these weights from this machine?</p><button type="button" class="ghost mono-ghost small" onclick={() => action('remove',item.name)}>Remove model</button><button type="button" class="ghost mono-ghost small" onclick={() => confirmRemove = ''}>Keep it</button>{/if}{#if result?.model === item.name}<p class:error={!result.ok} role="status">{result.detail}</p>{/if}</div>{/if}
        </div>
      {/each}
    {:else}
      <p>Approximate download and RAM sizes. Expand a model for details.</p>
      {#each shelf.slice(current * 5, current * 5 + 5) as item (item.id)}
        <div class="model-row"><div class="row-main"><button type="button" class="model-copy expand" aria-expanded={details === item.id} onclick={() => details = details === item.id ? '' : item.id}><strong>{item.label} <span>{item.params}</span></strong><span>~{item.sizeGb} GB download · ~{item.ramGb} GB RAM</span></button><button type="button" class="ghost mono-ghost small" disabled={!!busy || !status?.running || pulls.some(p => sameModel(p.model,item.id) && !p.done)} onclick={() => action('pull',item.id)}>{status?.models.some(m => sameModel(m.name,item.id)) ? 'Update' : 'Download'}</button></div>{#if details === item.id}<p class="detail">{item.blurb} · {item.caps.join(' / ') || 'text'}</p>{/if}</div>
      {/each}
    {/if}
    {#if !count}<p class="empty">{query ? 'No models match your search.' : tab === 'installed' ? 'No downloaded models to show. Start Ollama or browse the model shelf.' : 'No models fit this filter.'}</p>{/if}
    <div class="pagination"><span>{count} models · page {current + 1} of {pages}</span><div><button type="button" class="ghost mono-ghost small" aria-label="Previous local model page" disabled={current === 0} onclick={() => page = current - 1}>←</button><button type="button" class="ghost mono-ghost small" aria-label="Next local model page" disabled={current + 1 >= pages} onclick={() => page = current + 1}>→</button></div></div>
    {#if tab === 'browse'}<button type="button" class="text-action" onclick={() => customOpen = !customOpen}>Download another model by name</button>{#if customOpen}<div class="search-row"><input aria-label="Ollama model tag to download" placeholder="e.g. llama3.2:3b" bind:value={custom} /><button type="button" class="ghost mono-ghost small" disabled={!!busy || !custom.trim() || !status?.running} onclick={() => action('pull',custom.trim())}>Download</button></div>{/if}{/if}
  {/if}
  {#if message}<p class="notice" role="status">{message}</p>{/if}{#if error}<p class="error" role="alert">{error}</p>{/if}
</div>

<style>
  .local-pane { display: grid; gap: 12px; } p { color: var(--muted); font-size: 10px; line-height: 1.6; margin: 0; overflow-wrap: anywhere; }
  .status-line { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; } .status-line strong { font-size: 13px; } .status-line span { color: var(--muted); font: 9px var(--font-mono); } .status-line .running { color: var(--led-ok); } .status-line button { margin-left: auto; }
  button:disabled { opacity: .4; cursor: default; }
  .install { display: flex; align-items: center; justify-content: space-between; gap: 10px; border: 1px dashed var(--node-border); padding: 11px; } a { color: var(--accent); font-size: 11px; }
  .tabs { display: flex; gap: 2px; border-bottom: 1px solid var(--node-border); }
  .tabs button { position: relative; display: flex; align-items: center; gap: 6px; padding: 9px 11px; margin-bottom: -1px; color: var(--muted); border: 0; background: transparent; font-size: 11px; line-height: 1; cursor: pointer; border-radius: var(--radius-detail) var(--radius-detail) 0 0; }
  .tabs button:hover:not(.active) { color: var(--text); background: var(--surface); }
  .tabs button::after { content: ''; position: absolute; inset: auto 0 0; height: 2px; background: transparent; }
  .tabs .active { color: var(--accent); } .tabs .active::after { background: var(--accent); }
  .tabs button:focus-visible { outline: 1px solid var(--accent); outline-offset: -3px; }
  .tabs span { min-width: 15px; padding: 2px 4px; border-radius: var(--radius-detail); background: var(--surface-2); color: var(--muted); font: 9px/1.3 var(--font-mono); text-align: center; }
  .tabs .active span { background: var(--accent); color: var(--bg); }
  .search-row { display: flex; gap: 8px; } input { min-width: 0; flex: 1; border: 1px solid var(--node-border); background: var(--bg); color: var(--text); font-size: 11px; padding: 9px; }
  .model-row { border-bottom: 1px solid var(--node-border); padding-bottom: 10px; } .row-main { display: flex; align-items: center; gap: 7px; min-height: 37px; } .model-copy { flex: 1; min-width: 0; display: grid; gap: 5px; } .model-copy strong { color: var(--text); font-size: 11px; font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; } .model-copy span { color: var(--muted); font-size: 9px; } .expand { padding: 0; border: 0; background: transparent; text-align: left; cursor: pointer; }
  .detail { padding-top: 10px; display: flex; flex-wrap: wrap; gap: 8px; } .detail p { flex-basis: 100%; }
  .pagination,.pagination > div { display: flex; justify-content: space-between; align-items: center; gap: 7px; } .pagination span { color: var(--muted); font: 9px var(--font-mono); } .empty { padding: 14px 0; }
  .text-action { justify-self: start; border: 0; background: transparent; padding: 0; color: var(--accent); font-size: 10px; cursor: pointer; }
  .pull { display: grid; gap: 8px; font-size: 10px; } .pull > div:first-child { display: flex; justify-content: space-between; gap: 10px; } .track { height: 4px; background: var(--bg); } .track span { display: block; height: 100%; background: var(--accent); } .notice { color: var(--led-ok); } .error { color: var(--led-err); }
</style>
