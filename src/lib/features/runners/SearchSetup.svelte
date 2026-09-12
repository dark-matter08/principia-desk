<script lang="ts">
  import { onMount } from 'svelte';
  import { Globe, Search, Server, KeyRound, Sparkles, CircleOff, ExternalLink } from 'lucide-svelte';
  import { api, type SearchProvider, type SearchResult, type SearchSettingsView } from '$lib/ipc';
  import NodeCard from '$lib/components/NodeCard.svelte';

  const PROVIDERS: { id: SearchProvider; label: string; hint: string; Icon: typeof Globe }[] = [
    { id: 'searxng', label: 'SearXNG', hint: 'Runs on your machine, no key. Asks the public engines for you.', Icon: Server },
    { id: 'brave', label: 'Brave Search', hint: '2,000 queries a month free. One key, nothing to run.', Icon: KeyRound },
    { id: 'tavily', label: 'Tavily', hint: 'Cleaned page text as well as links. One key, nothing to run.', Icon: Sparkles },
    { id: 'none', label: 'Off', hint: 'Lessons use only the pages the curriculum names, and their mirrors.', Icon: CircleOff },
  ];

  let settings = $state<SearchSettingsView | null>(null);
  let busy = $state('');
  let error = $state('');
  let url = $state('');
  let key = $state('');
  let query = $state('bash redirection operators');
  let results = $state<SearchResult[] | null>(null);
  const provider = $derived(settings?.provider ?? 'none');
  const keySet = $derived(provider === 'brave' ? !!settings?.brave_key_set : provider === 'tavily' ? !!settings?.tavily_key_set : false);
  const tone = $derived(!settings ? 'idle' : settings.available ? 'ok' : provider === 'none' ? 'idle' : 'warn');

  onMount(() => { void load(); });

  async function load() {
    busy = 'load';
    try { settings = await api.getSearchSettings(); url = settings.searxng_url; }
    catch (e) { error = String(e); }
    finally { busy = ''; }
  }
  async function choose(next: SearchProvider) {
    if (busy || next === provider) return;
    busy = 'provider'; error = ''; results = null;
    try { settings = await api.setSearchSettings(next, url || settings?.default_searxng_url || ''); url = settings.searxng_url; }
    catch (e) { error = String(e); }
    finally { busy = ''; }
  }
  async function saveUrl() {
    busy = 'url'; error = '';
    try { settings = await api.setSearchSettings(provider, url); url = settings.searxng_url; }
    catch (e) { error = String(e); }
    finally { busy = ''; }
  }
  async function saveKey() {
    busy = 'key'; error = '';
    try { settings = await api.setSearchKey(provider, key); key = ''; }
    catch (e) { error = String(e); }
    finally { busy = ''; }
  }
  async function test() {
    busy = 'test'; error = ''; results = null;
    try { results = await api.testSearch(query); }
    catch (e) { error = String(e); }
    finally { busy = ''; }
  }
  function host(link: string) {
    try { return new URL(link).host.replace(/^www\./, ''); } catch { return link; }
  }
</script>

<NodeCard Icon={Globe} name="web-search" badge={settings ? (settings.available ? 'working' : provider === 'none' ? 'off' : 'setup needed') : '…'} badgeTone={settings?.available ? 'teal' : provider === 'none' ? 'muted' : 'amber'}>
  <div class="head"><strong>Web search for the tutor</strong><p>A tutor on Ollama, OpenRouter or a bare API cannot look anything up. With an engine set, the desk searches and fetches the documentation itself and hands the tutor only pages it retrieved, still held to each subject's source allowlist.</p></div>

  <div class="providers" role="radiogroup" aria-label="Search provider">
    {#each PROVIDERS as option (option.id)}
      <button type="button" class="provider" class:active={provider === option.id} class:off={option.id === 'none'} role="radio" aria-checked={provider === option.id} disabled={!!busy} onclick={() => choose(option.id)}>
        <span class="art" aria-hidden="true"><option.Icon size={18} /></span>
        <span class="text"><span class="label">{option.label}</span><span class="hint">{option.hint}</span></span>
        {#if provider === option.id}<span class="led" class:ok={tone === 'ok'} class:warn={tone === 'warn'} aria-hidden="true"></span>{/if}
      </button>
    {/each}
  </div>

  {#if provider !== 'none'}
    <div class="console" aria-label="Search configuration">
      <div class="console-head mono">
        <span class="led" class:ok={tone === 'ok'} class:warn={tone === 'warn'} aria-hidden="true"></span>
        <span class="state">{settings?.why ?? 'checking…'}</span>
      </div>

      {#if provider === 'searxng'}
        <div class="row">
          <label class="field">
            <span class="mono">ADDRESS</span>
            <input class="mono" type="url" bind:value={url} placeholder={settings?.default_searxng_url} spellcheck="false" onkeydown={(event) => { if (event.key === 'Enter') void saveUrl(); }} />
          </label>
          <button type="button" class="ghost mono-ghost" disabled={!!busy} onclick={saveUrl}>{busy === 'url' ? 'saving…' : 'save'}</button>
        </div>
        <p class="note mono">Remote Ledger's local instance answers on port 8899 and can be shared. The JSON API must be on: <code>json</code> under <code>search.formats</code> in settings.yml, or it answers 403.</p>
      {:else}
        <div class="row">
          <label class="field">
            <span class="mono">{provider === 'brave' ? 'BRAVE SEARCH' : 'TAVILY'} API KEY <em class:set={keySet}>{keySet ? '· set' : '· not set'}</em></span>
            <input class="mono" type="password" bind:value={key} placeholder={keySet ? 'paste a new key to replace it' : 'paste the key'} autocomplete="off" spellcheck="false" onkeydown={(event) => { if (event.key === 'Enter') void saveKey(); }} />
          </label>
          <button type="button" class="ghost mono-ghost" disabled={!!busy || (!key && !keySet)} onclick={saveKey}>{busy === 'key' ? 'saving…' : key ? 'save key' : 'clear key'}</button>
        </div>
        <p class="note mono">Kept in the system keychain, never in the profile database or its exports. {#if provider === 'brave'}<a href="https://brave.com/search/api/" target="_blank" rel="noreferrer">Get a key <ExternalLink size={9} /></a>{:else}<a href="https://tavily.com" target="_blank" rel="noreferrer">Get a key <ExternalLink size={9} /></a>{/if}</p>
      {/if}

      <div class="row try">
        <label class="field">
          <span class="mono">TRY A SEARCH</span>
          <input class="mono" type="text" bind:value={query} spellcheck="false" onkeydown={(event) => { if (event.key === 'Enter') void test(); }} />
        </label>
        <button type="button" class="ghost mono-ghost" disabled={!!busy || !settings?.available} onclick={test}><Search size={11} /> {busy === 'test' ? 'searching…' : 'search'}</button>
      </div>
      {#if results}
        {#if results.length === 0}
          <p class="note mono">No results came back for that query.</p>
        {:else}
          <ol class="results">
            {#each results as result, index (result.url)}
              <li>
                <span class="index mono">{String(index + 1).padStart(2, '0')}</span>
                <div class="result-body">
                  <strong>{result.title || result.url}</strong>
                  <span class="meta mono"><span class="host">{host(result.url)}</span>{#if result.engine} · via {result.engine}{/if}</span>
                  {#if result.snippet}<p>{result.snippet}</p>{/if}
                </div>
              </li>
            {/each}
          </ol>
        {/if}
      {/if}
    </div>
  {/if}
  {#if error}<p class="error mono" role="alert">{error}</p>{/if}
</NodeCard>

<style>
  .head { margin-bottom: 14px; } .head strong { display: block; font-size: 14px; font-weight: 500; } .head p { margin: 5px 0 0; font-size: 11px; color: var(--muted); line-height: 1.6; max-width: 76ch; }

  /* The engines as cards, like the runner kinds: an icon tile, the name, a
     line on what it costs to run, and a light when it is the one in use. */
  .providers { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 8px; }
  .provider { position: relative; display: flex; align-items: center; gap: 11px; min-height: 64px; padding: 10px 12px; text-align: left; color: var(--muted); background: var(--bg); border: 1px solid var(--node-border); border-radius: var(--radius-panel); cursor: pointer; transition: border-color 140ms ease, background 140ms ease, transform 140ms ease; }
  .provider:hover:not(:disabled) { border-color: var(--muted); transform: translateY(-1px); }
  .provider:disabled { cursor: default; }
  .provider.active { color: var(--fg); border-color: var(--accent); background: color-mix(in srgb, var(--accent) 7%, var(--bg)); box-shadow: 0 0 0 1px color-mix(in srgb, var(--accent) 35%, transparent), 0 8px 22px color-mix(in srgb, var(--accent) 12%, transparent); }
  .provider.off.active { border-color: var(--muted); background: var(--surface); box-shadow: none; }
  .art { flex: none; display: grid; place-items: center; width: 38px; height: 38px; border-radius: var(--radius-control); background: var(--surface-2); border: 1px solid var(--node-border); color: var(--muted); transition: background 140ms ease, color 140ms ease, box-shadow 140ms ease; }
  .provider.active .art { background: linear-gradient(160deg, color-mix(in srgb, var(--accent) 34%, var(--surface-2)), color-mix(in srgb, var(--accent) 12%, var(--surface-2))); color: var(--accent); border-color: color-mix(in srgb, var(--accent) 55%, var(--node-border)); box-shadow: 0 0 14px color-mix(in srgb, var(--accent) 35%, transparent); }
  .provider.off.active .art { background: var(--surface-2); color: var(--fg); border-color: var(--node-border); box-shadow: none; }
  .text { display: flex; flex-direction: column; gap: 3px; min-width: 0; }
  .label { font-size: 12px; font-weight: 500; }
  .hint { font-size: 9.5px; line-height: 1.4; color: var(--muted); overflow: hidden; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; }
  .provider .led { position: absolute; top: 10px; right: 10px; }

  .led { display: inline-block; width: 7px; height: 7px; border-radius: 50%; background: var(--led-idle); flex: none; }
  .led.ok { background: var(--led-ok); box-shadow: 0 0 8px var(--led-ok); animation: led-breathe 2.4s ease-in-out infinite; }
  .led.warn { background: var(--led-warn); box-shadow: 0 0 8px var(--led-warn); }
  @keyframes led-breathe { 0%, 100% { opacity: 1; } 50% { opacity: 0.55; } }

  /* The chosen engine's controls, in one console block. */
  .console { margin-top: 12px; border: 1px solid var(--node-border); border-radius: var(--radius-panel); background: var(--bg); overflow: hidden; }
  .console-head { display: flex; align-items: center; gap: 9px; padding: 9px 14px; border-bottom: 1px solid var(--node-border); background: var(--surface); font-size: 10px; color: var(--muted); letter-spacing: 0.3px; }
  .console-head .state { color: var(--fg); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .row { display: flex; align-items: flex-end; gap: 8px; padding: 14px 14px 0; }
  .row.try { padding: 16px 14px 14px; margin-top: 4px; border-top: 1px dashed var(--node-divider); }
  .field { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 6px; }
  .field > span { font-size: 8.5px; letter-spacing: 1.1px; color: var(--faint); display: flex; gap: 6px; }
  .field em { font-style: normal; color: var(--led-warn); } .field em.set { color: var(--led-ok); }
  .field input { width: 100%; min-width: 0; min-height: 34px; padding: 7px 11px; font-size: 12px; color: var(--fg); background: var(--surface); border: 1px solid var(--node-border); border-radius: var(--radius-control); outline: none; transition: border-color 120ms ease, box-shadow 120ms ease; }
  .field input:focus { border-color: var(--accent); box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 18%, transparent); }
  .field input::placeholder { color: var(--faint); }
  .row :global(button) { flex: none; }
  .note { margin: 8px 14px 14px; font-size: 9.5px; line-height: 1.6; color: var(--muted); }
  .note code { font-family: inherit; color: var(--fg); }
  .note a { display: inline-flex; align-items: center; gap: 3px; color: var(--accent); text-decoration: none; margin-left: 4px; }
  .results { list-style: none; margin: 0; padding: 0 14px 6px; }
  .results li { display: flex; gap: 10px; padding: 10px 0; border-top: 1px dashed var(--node-divider); }
  .index { flex: none; padding-top: 2px; font-size: 9px; color: var(--accent); }
  .result-body { min-width: 0; } .result-body strong { display: block; font-size: 12px; font-weight: 500; color: var(--fg); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .meta { display: block; margin-top: 2px; font-size: 9px; color: var(--faint); } .host { color: var(--muted); }
  .result-body p { margin: 4px 0 0; font-size: 11px; line-height: 1.5; color: var(--muted); overflow: hidden; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; }
  .error { color: var(--led-err); font-size: 10.5px; margin-top: 10px; overflow-wrap: anywhere; }
  @media (max-width: 900px) { .providers { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
  @media (max-width: 560px) { .row { flex-direction: column; align-items: stretch; } .provider .hint { display: none; } }
  @media (prefers-reduced-motion: reduce) { .led.ok { animation: none; } .provider:hover:not(:disabled) { transform: none; } }
</style>
