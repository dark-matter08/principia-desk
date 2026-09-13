<script lang="ts">
  import { onMount } from 'svelte';
  import { Globe, Search, Server, KeyRound, Sparkles, CircleOff, ExternalLink, Download, Play, Square, Check, X, Loader } from 'lucide-svelte';
  import { api, type SearchProvider, type SearchResult, type SearchSettingsView, type SecretStoreView, type SearxngStatus, type SearxngStep } from '$lib/ipc';
  import NodeCard from '$lib/components/NodeCard.svelte';

  const PROVIDERS: { id: SearchProvider; label: string; hint: string; Icon: typeof Globe }[] = [
    { id: 'searxng', label: 'SearXNG', hint: 'Runs on your machine, no key. Asks the public engines for you.', Icon: Server },
    { id: 'brave', label: 'Brave Search', hint: '2,000 queries a month free. One key, nothing to run.', Icon: KeyRound },
    { id: 'tavily', label: 'Tavily', hint: 'Cleaned page text as well as links. One key, nothing to run.', Icon: Sparkles },
    { id: 'none', label: 'Off', hint: 'Lessons use only the pages the curriculum names, and their mirrors.', Icon: CircleOff },
  ];

  let settings = $state<SearchSettingsView | null>(null);
  let store = $state<SecretStoreView | null>(null);
  let engine = $state<SearxngStatus | null>(null);
  let steps = $state<SearxngStep[]>([]);
  let installing = $state(false);
  let engineOpen = $state(false);
  let busy = $state('');
  let error = $state('');
  let url = $state('');
  let key = $state('');
  let query = $state('bash redirection operators');
  let results = $state<SearchResult[] | null>(null);
  const provider = $derived(settings?.provider ?? 'none');
  const keySet = $derived(provider === 'brave' ? !!settings?.brave_key_set : provider === 'tavily' ? !!settings?.tavily_key_set : false);
  const tone = $derived(!settings ? 'idle' : settings.available ? 'ok' : provider === 'none' ? 'idle' : 'warn');

  onMount(() => { void load(); api.getSecretStore().then((s) => (store = s)).catch(() => {}); });

  async function load() {
    busy = 'load';
    try { settings = await api.getSearchSettings(); url = settings.searxng_url; }
    catch (e) { error = String(e); }
    finally { busy = ''; }
    void probe();
  }
  /** What this machine has of SearXNG, so the block below can offer the right key. */
  async function probe() {
    if (settings?.provider !== 'searxng') return;
    try { engine = await api.searxngStatus(); } catch { engine = null; }
  }
  async function installUv() {
    busy = 'uv'; error = ''; steps = [];
    try { steps = await api.installUv(); await probe(); }
    catch (e) { error = String(e); }
    finally { busy = ''; }
  }
  async function install() {
    installing = true; error = ''; steps = [];
    try {
      steps = await api.searxngInstall();
      settings = await api.getSearchSettings();
      await probe();
    } catch (e) { error = String(e); }
    finally { installing = false; }
  }
  async function start() {
    busy = 'start'; error = '';
    try { engine = await api.searxngStart(); settings = await api.getSearchSettings(); }
    catch (e) { error = String(e); }
    finally { busy = ''; }
  }
  async function stop() {
    busy = 'stop'; error = '';
    try { engine = await api.searxngStop(); settings = await api.getSearchSettings(); }
    catch (e) { error = String(e); }
    finally { busy = ''; }
  }
  async function choose(next: SearchProvider) {
    if (busy || next === provider) return;
    busy = 'provider'; error = ''; results = null;
    try { settings = await api.setSearchSettings(next, url || settings?.default_searxng_url || ''); url = settings.searxng_url; }
    catch (e) { error = String(e); }
    finally { busy = ''; }
    void probe();
  }
  async function saveUrl() {
    busy = 'url'; error = '';
    try { settings = await api.setSearchSettings(provider, url); url = settings.searxng_url; }
    catch (e) { error = String(e); }
    finally { busy = ''; }
    void probe();
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

        <!-- What this machine has of it, and the one key that moves it along:
             start what is installed, install what is not, stop what the desk started. -->
        <div class="engine" aria-label="SearXNG on this machine">
          {#if !engine}
            <p class="engine-line mono"><Loader size={11} class="spin" /> looking for SearXNG on this machine…</p>
          {:else if engine.running}
            <p class="engine-line mono"><span class="led ok" aria-hidden="true"></span> answering at {engine.url}{#if engine.version}{' · '}{engine.version}{/if}{#if engine.shared_with_ledger}{' · '}Remote Ledger's install, shared{/if}</p>
            {#if engine.pid}<button type="button" class="ghost mono-ghost small" disabled={!!busy || installing} onclick={stop}><Square size={11} /> {busy === 'stop' ? 'stopping…' : 'stop'}</button>{/if}
          {:else if !engine.local}
            <p class="engine-line mono"><span class="led warn" aria-hidden="true"></span> {engine.url} is not this machine; the desk can only install and start an instance on its own address.</p>
          {:else if engine.installed}
            <p class="engine-line mono"><span class="led warn" aria-hidden="true"></span> installed at {engine.home}{#if engine.shared_with_ledger}{' '}(Remote Ledger's install, shared){/if}, not running{#if !engine.json_enabled}{' · '}the JSON API is off in its settings.yml{/if}</p>
            <button type="button" class="cta mono-cta" disabled={!!busy || installing} onclick={start}><Play size={12} /> {busy === 'start' ? 'starting…' : 'Start SearXNG'}</button>
            {#if engine.log_tail}<details class="log"><summary class="mono">why it is not running</summary><pre class="mono">{engine.log_tail}</pre></details>{/if}
          {:else}
            <p class="engine-line mono"><span class="led warn" aria-hidden="true"></span> not installed on this machine.</p>
            <p class="engine-note">Installs to <code>{engine.home}</code>: a shallow clone and an isolated Python, nothing else on the machine touched. {#if engine.has_uv}uv is here, so it fetches its own Python 3.12 and takes about a minute.{:else if engine.python}Python {engine.python_version} at {engine.python}{#if engine.python_too_new} is newer than SearXNG pins for; it is tried anyway, and uv would be surer{:else}; installing uv would make it faster{/if}.{:else}No Python 3.10 or later was found. Install uv and it fetches its own Python 3.12:{/if}{#if !engine.has_git} Without git the source comes as an archive.{/if}</p>
            {#if engine.python_install}
              <div class="uv-row">
                <button type="button" class="cta mono-cta" disabled={!!busy || installing} onclick={installUv}><Download size={12} /> {busy === 'uv' ? 'installing uv…' : 'Install uv for me'}</button>
                <span class="engine-note">or run it yourself, then press refresh:</span>
                <pre class="mono cmd">{engine.python_install}</pre>
              </div>
            {/if}
            <button type="button" class="cta mono-cta" disabled={!!busy || installing || !engine.can_install} onclick={install}><Download size={12} /> {installing ? 'installing…' : 'Install SearXNG'}</button>
          {/if}
          {#if installing}<p class="engine-note">Every step is reported on the Logs page as it runs. A first install clones SearXNG and builds its environment; give it a few minutes.</p>{/if}
          {#if steps.length}
            <ol class="steps">
              {#each steps as step, index (index)}
                <li class:failed={!step.ok}><span class="mark" aria-hidden="true">{#if step.ok}<Check size={11} />{:else}<X size={11} />{/if}</span><span class="step-body"><span>{step.step}</span>{#if !step.ok && step.output}<pre class="mono">{step.output}</pre>{/if}</span></li>
              {/each}
            </ol>
          {/if}
        </div>
      {:else}
        <div class="row">
          <label class="field">
            <span class="mono">{provider === 'brave' ? 'BRAVE SEARCH' : 'TAVILY'} API KEY <em class:set={keySet}>{keySet ? '· set' : '· not set'}</em></span>
            <input class="mono" type="password" bind:value={key} placeholder={keySet ? 'paste a new key to replace it' : 'paste the key'} autocomplete="off" spellcheck="false" onkeydown={(event) => { if (event.key === 'Enter') void saveKey(); }} />
          </label>
          <button type="button" class="ghost mono-ghost" disabled={!!busy || (!key && !keySet)} onclick={saveKey}>{busy === 'key' ? 'saving…' : key ? 'save key' : 'clear key'}</button>
        </div>
        <p class="note mono">Kept in {store?.label ?? 'your system\'s secret store'}, never in the profile database or its exports. {#if provider === 'brave'}<a href="https://brave.com/search/api/" target="_blank" rel="noreferrer">Get a key <ExternalLink size={9} /></a>{:else}<a href="https://tavily.com" target="_blank" rel="noreferrer">Get a key <ExternalLink size={9} /></a>{/if}</p>
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
  /* The engine block: one status line, one key, the steps when it installs. */
  .engine { display: flex; flex-direction: column; align-items: flex-start; gap: 8px; margin: 0 14px 14px; padding: 12px 14px; border: 1px dashed var(--node-divider); border-radius: var(--radius-control); }
  .engine-line { display: flex; align-items: center; gap: 8px; margin: 0; font-size: 10px; color: var(--fg); overflow-wrap: anywhere; }
  .engine-note { margin: 0; font-size: 10.5px; line-height: 1.6; color: var(--muted); max-width: 72ch; }
  .engine-note code { font-family: var(--font-mono); font-size: 10px; color: var(--fg); }
  .uv-row { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  .engine .cmd { margin: 0; padding: 8px 10px; font-size: 10.5px; background: var(--surface); border: 1px solid var(--node-border); border-radius: var(--radius-control); color: var(--fg); user-select: all; }
  .engine :global(.spin) { animation: spin 1.1s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .log { align-self: stretch; } .log summary { font-size: 9.5px; color: var(--muted); cursor: pointer; }
  .log pre { margin: 6px 0 0; max-height: 160px; overflow: auto; padding: 8px 10px; font-size: 9.5px; line-height: 1.5; color: var(--muted); background: var(--surface); border: 1px solid var(--node-border); border-radius: var(--radius-control); white-space: pre-wrap; overflow-wrap: anywhere; }
  .steps { align-self: stretch; list-style: none; margin: 4px 0 0; padding: 0; display: flex; flex-direction: column; gap: 4px; }
  .steps li { display: flex; gap: 8px; font-size: 11px; color: var(--muted); }
  .steps li.failed { color: var(--led-err); }
  .steps .mark { flex: none; display: grid; place-items: center; width: 16px; height: 16px; border-radius: 50%; background: color-mix(in srgb, var(--led-ok) 18%, transparent); color: var(--led-ok); }
  .steps li.failed .mark { background: color-mix(in srgb, var(--led-err) 18%, transparent); color: var(--led-err); }
  .step-body { display: flex; flex-direction: column; gap: 4px; min-width: 0; }
  .step-body pre { margin: 0; font-size: 9.5px; white-space: pre-wrap; overflow-wrap: anywhere; color: var(--muted); }
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
