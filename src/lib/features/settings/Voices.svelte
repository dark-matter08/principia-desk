<script lang="ts">
  /**
   * The voices a lesson can be heard in, in the runner setup's shape: the
   * engines down the left, and for the one in hand its state, one key to
   * install or remove it, and its voices as a searchable, paged grid of
   * small cards: hear one, download one, see which are ready. A class
   * picks from what is ready here.
   */
  import { onMount } from 'svelte';
  import { AudioLines, Download, Play, Trash2, Check, X, Loader, Search, ArrowLeft, ArrowRight, Square, Speaker, Cpu, Sparkles, Mic, RefreshCw } from 'lucide-svelte';
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const ICONS: Record<string, any> = { system: Speaker, piper: Cpu, kokoro: Sparkles, vibevoice: Mic };
  import { api, type AudioStatus, type AudioEngineStatus, type SearxngStep } from '$lib/ipc';
  import { assetUrl } from '$lib/features/lessons/asset';
  import { confirmDialog } from '$lib/components/dialog.svelte';
  import NodeCard from '$lib/components/NodeCard.svelte';

  type EngineId = AudioEngineStatus['id'];
  interface Row { id: string; label: string; language: string; hint: string; size_mb: number; installed: boolean }
  const PAGE = 12;

  let status = $state<AudioStatus | null>(null);
  let engine = $state<EngineId>('system');
  let query = $state('');
  let page = $state(0);
  let busy = $state('');
  let error = $state('');
  let steps = $state<SearxngStep[]>([]);
  let stepsFor = $state('');
  let playing = $state('');
  let systemVoices = $state<Row[]>([]);
  let player: HTMLAudioElement | null = null;

  const current = $derived(status?.engines.find((e) => e.id === engine) ?? null);
  const rows = $derived.by((): Row[] => (engine === 'system' ? systemVoices : (status?.voices ?? []).filter((v) => v.engine === engine).map((v) => ({ ...v }))));
  const matching = $derived.by(() => {
    const needle = query.trim().toLowerCase();
    return needle ? rows.filter((r) => `${r.label} ${r.id} ${r.language} ${r.hint}`.toLowerCase().includes(needle)) : rows;
  });
  const pages = $derived(Math.max(1, Math.ceil(matching.length / PAGE)));
  const shown = $derived(matching.slice(Math.min(page, pages - 1) * PAGE, Math.min(page, pages - 1) * PAGE + PAGE));
  const readyCount = (e: AudioEngineStatus) => (e.id === 'system' ? systemVoices.length : (status?.voices ?? []).filter((v) => v.engine === e.id && v.installed).length);
  const stateLabel = $derived(!current ? '' : !current.ready ? (current.installable ? 'NOT INSTALLED' : 'NOT ON THIS MACHINE') : current.source === 'desk' ? 'INSTALLED BY THE DESK' : 'FOUND');

  onMount(() => {
    void load();
    readSystemVoices();
    if (typeof speechSynthesis !== 'undefined') speechSynthesis.addEventListener('voiceschanged', readSystemVoices);
    return () => { stopAll(); if (typeof speechSynthesis !== 'undefined') speechSynthesis.removeEventListener('voiceschanged', readSystemVoices); };
  });

  function readSystemVoices() {
    if (typeof speechSynthesis === 'undefined') return;
    const all = speechSynthesis.getVoices();
    const english = all.filter((v) => v.lang.toLowerCase().startsWith('en'));
    systemVoices = (english.length ? english : all).map((v) => ({ id: v.name, label: v.name, language: v.lang, hint: v.localService ? 'on this computer' : 'network voice', size_mb: 0, installed: true }));
  }
  async function load() {
    try { status = await api.getAudioStatus(); } catch (e) { error = String(e); }
  }
  function choose(next: EngineId) { engine = next; query = ''; page = 0; stopAll(); }
  function stopAll() { player?.pause(); player = null; if (typeof speechSynthesis !== 'undefined') speechSynthesis.cancel(); playing = ''; }

  async function installEngine(target: AudioEngineStatus) {
    busy = `engine:${target.id}`; error = ''; steps = []; stepsFor = target.id;
    try { steps = await api.installAudioEngine(target.id); await load(); }
    catch (e) { error = String(e); }
    finally { busy = ''; }
  }
  async function removeEngine(target: AudioEngineStatus) {
    const ok = await confirmDialog(`Remove the desk's ${target.label}?`, 'Its environment under the profile is deleted; downloaded voices and anything installed elsewhere on this machine stay. Classes set to it fall back to the system voice until it is back.', { confirm: 'Remove', danger: true });
    if (!ok) return;
    busy = `engine:${target.id}`; error = '';
    try { await api.removeAudioEngine(target.id); await load(); }
    catch (e) { error = String(e); }
    finally { busy = ''; }
  }
  async function installVoice(row: Row) {
    busy = `voice:${row.id}`; error = ''; steps = []; stepsFor = row.id;
    try { steps = await api.installVoice(row.id); await load(); }
    catch (e) { error = String(e); }
    finally { busy = ''; }
  }
  async function removeVoice(row: Row) {
    busy = `voice:${row.id}`; error = '';
    try { await api.removeVoice(row.id); await load(); }
    catch (e) { error = String(e); }
    finally { busy = ''; }
  }
  async function hear(row: Row) {
    if (playing === row.id) { stopAll(); return; }
    stopAll();
    error = '';
    if (engine === 'system') {
      if (typeof speechSynthesis === 'undefined') { error = 'This webview has no speech voices.'; return; }
      const line = new SpeechSynthesisUtterance('A cache is a sticky note on the fridge; the database is the filing cabinet in the basement.');
      const chosen = speechSynthesis.getVoices().find((v) => v.name === row.id);
      if (chosen) line.voice = chosen;
      line.onend = () => { playing = ''; };
      playing = row.id;
      speechSynthesis.speak(line);
      return;
    }
    busy = `preview:${row.id}`;
    try {
      const path = await api.previewVoice(engine, row.id);
      player = new Audio(assetUrl(path));
      player.onended = () => { playing = ''; };
      playing = row.id;
      await player.play();
    } catch (e) { error = String(e); playing = ''; }
    finally { busy = ''; }
  }
  /** What one press on a card does: hear it when it is here, fetch it when it is not. */
  function act(row: Row) {
    if (!current) return;
    if (row.installed && current.ready) void hear(row);
    else if (engine === 'piper' && current.ready) void installVoice(row);
  }
</script>

<NodeCard Icon={AudioLines} name="voices" badge={status ? `${status.engines.filter((e) => e.ready).length} of ${status.engines.length} engines ready` : '…'} badgeTone={status && status.engines.filter((e) => e.ready).length > 1 ? 'teal' : 'muted'}>
  <div class="head"><strong>Voices for listening</strong><p>A class can ask for its lessons as a conversation you hear: the tutor writes each prepared lesson a second time as a teacher and a student talking it through, and the desk voices it with the engine and voices chosen for the class. Set the engines up here; choose them per class in its Settings.</p></div>

  {#if !status}
    <p class="line mono"><Loader size={11} class="spin" /> looking at this machine…</p>
  {:else}
    <div class="workspace">
      <div class="engines" aria-label="Voice engines">
        {#each status.engines as item (item.id)}
          {@const Icon = ICONS[item.id]}
          <button type="button" class="engine" class:editing={engine === item.id} class:ready={item.ready} aria-pressed={engine === item.id} onclick={() => choose(item.id)}>
            <span class="engine-art" aria-hidden="true"><Icon size={16} /></span>
            <span class="engine-text"><span class="engine-label">{item.label}</span><span class="engine-hint">{item.blurb}</span></span>
            <span class="engine-state mono" class:ok={item.ready && readyCount(item) > 0} class:warn={item.ready && readyCount(item) === 0}>{item.ready ? readyCount(item) : item.installable ? '+' : '–'}</span>
          </button>
        {/each}
        <button type="button" class="ghost mono-ghost small refresh" onclick={() => { readSystemVoices(); void load(); }}><RefreshCw size={10} /> refresh detection</button>
      </div>

      <div class="editor">
        {#if current}
          <header><strong>{current.label}</strong><span class="status">{stateLabel}</span></header>
          <p class="why">{current.why}</p>
          {#if current.id !== 'system'}
            <div class="keys">
              {#if !current.ready && current.installable}
                <button type="button" class="cta mono-cta" disabled={!!busy} onclick={() => installEngine(current)}><Download size={12} /> {busy === `engine:${current.id}` ? 'installing…' : `Install ${current.label}`}</button>
              {:else if current.ready && current.source === 'desk'}
                <button type="button" class="ghost mono-ghost small" disabled={!!busy} onclick={() => removeEngine(current)}><Trash2 size={11} /> remove the desk's install</button>
              {:else if current.ready && current.installable}
                <button type="button" class="ghost mono-ghost small" disabled={!!busy} onclick={() => installEngine(current)} title="A copy under the profile, independent of the environment it was found in"><Download size={11} /> {busy === `engine:${current.id}` ? 'installing…' : "install the desk's own copy"}</button>
              {/if}
              {#if engine === 'kokoro' || engine === 'vibevoice'}<span class="fine">The first render fetches the model ({engine === 'kokoro' ? 'about 330 MB' : status.apple_silicon ? 'about 700 MB' : 'about 2 GB'}); a first "hear" does that too.</span>{/if}
            </div>
            {#if stepsFor === current.id && steps.length}
              <ol class="steps">{#each steps as step, index (index)}<li class:failed={!step.ok}><span class="mark" aria-hidden="true">{#if step.ok}<Check size={11} />{:else}<X size={11} />{/if}</span><span>{step.step}{#if !step.ok && step.output}<pre class="mono">{step.output}</pre>{/if}</span></li>{/each}</ol>
            {/if}
          {/if}

          <div class="line"><span class="eyebrow mono">VOICES <b>{readyCount(current)}</b> ready{#if engine === 'piper'}{' · '}<b>{rows.length}</b> to choose from{/if}</span></div>
          <div class="search field-group"><Search size={13} /><input aria-label="Search voices" placeholder="Search voices by name or language…" value={query} oninput={(e) => { query = e.currentTarget.value; page = 0; }} /></div>
          <div class="catalogue" aria-label={`${current.label} voices`}>
            {#each shown as row (row.id)}
              <div class="voice" class:ready={row.installed && current.ready} class:lit={playing === row.id}>
                <button type="button" class="voice-main" onclick={() => act(row)} disabled={!!busy && busy !== `preview:${row.id}` && playing !== row.id || (!row.installed && !(engine === 'piper' && current.ready)) || (row.installed && !current.ready)} title={row.installed ? (playing === row.id ? 'Stop' : 'Hear') : engine === 'piper' ? `Download, ${row.size_mb} MB` : `Comes with the ${current.label} model`}>
                  <span class="voice-copy"><strong>{row.label}</strong><small>{row.language}{#if row.size_mb && !row.installed}{' · '}{row.size_mb} MB{/if}</small></span>
                  <span class="voice-foot"><span class="hint">{row.hint}</span><span class="mark" aria-hidden="true">{#if busy === `preview:${row.id}` || busy === `voice:${row.id}`}<Loader size={11} class="spin" />{:else if playing === row.id}<Square size={9} />{:else if row.installed && current.ready}<Play size={10} />{:else if engine === 'piper'}<Download size={10} />{:else}<Check size={10} />{/if}</span></span>
                </button>
                {#if engine === 'piper' && row.installed}<button type="button" class="voice-off" aria-label={`Remove the download of ${row.label}`} disabled={!!busy} onclick={() => removeVoice(row)}><X size={10} /></button>{/if}
                {#if stepsFor === row.id && steps.length && !steps.every((s) => s.ok)}<pre class="mono voice-error">{steps.find((s) => !s.ok)?.output}</pre>{/if}
              </div>
            {/each}
            {#if !shown.length}<p class="empty">{rows.length ? 'No voice matches your search.' : engine === 'system' ? 'No speech voices reported by this system yet.' : `Install ${current.label} and its voices appear here.`}</p>{/if}
          </div>
          <div class="pagination"><span class="mono">{matching.length} voices{#if matching.length > PAGE}{' · '}{Math.min(page, pages - 1) * PAGE + 1}–{Math.min(matching.length, (Math.min(page, pages - 1) + 1) * PAGE)}{/if}</span>{#if pages > 1}<div><button type="button" class="ghost mono-ghost small" disabled={page === 0} onclick={() => (page -= 1)} aria-label="Previous page"><ArrowLeft size={11} /></button><span class="mono">{Math.min(page, pages - 1) + 1} / {pages}</span><button type="button" class="ghost mono-ghost small" disabled={page >= pages - 1} onclick={() => (page += 1)} aria-label="Next page"><ArrowRight size={11} /></button></div>{/if}</div>
        {/if}
      </div>
    </div>
    {#if status.python_install}<pre class="mono cmd">{status.python_install}</pre>{/if}
  {/if}
  {#if error}<p class="error mono" role="alert">{error}</p>{/if}
</NodeCard>

<style>
  .head { margin-bottom: 14px; } .head strong { display: block; font-size: 14px; font-weight: 500; } .head p { margin: 5px 0 0; font-size: 11px; color: var(--muted); line-height: 1.6; max-width: 76ch; }
  /* The runner setup's frame: the engines down the left, the editor right. */
  .workspace { display: grid; grid-template-columns: 236px minmax(0, 1fr); gap: 17px; }
  /* The engines as a rail of tiles: an icon tile, the name, one line on
     what it is, and the count of voices standing ready. The one in hand is
     lit the way a route card is. */
  .engines { display: grid; align-content: start; gap: 6px; }
  .engine { position: relative; display: flex; align-items: center; gap: 10px; padding: 9px 10px; text-align: left; color: var(--muted); background: var(--bg); border: 1px solid var(--node-border); border-radius: var(--radius-panel); cursor: pointer; transition: border-color 140ms ease, background 140ms ease, transform 140ms ease; }
  .engine:hover { border-color: var(--muted); transform: translateY(-1px); }
  .engine.editing { color: var(--fg); border-color: var(--accent); background: color-mix(in srgb, var(--accent) 7%, var(--bg)); box-shadow: 0 0 0 1px color-mix(in srgb, var(--accent) 35%, transparent), 0 8px 22px color-mix(in srgb, var(--accent) 12%, transparent); }
  .engine-art { flex: none; display: grid; place-items: center; width: 34px; height: 34px; border-radius: var(--radius-control); background: var(--surface-2); color: var(--muted); border: 1px solid var(--node-border); transition: background 140ms ease, color 140ms ease, box-shadow 140ms ease; }
  .engine.ready .engine-art { color: var(--fg); }
  .engine.editing .engine-art { background: linear-gradient(160deg, color-mix(in srgb, var(--accent) 34%, var(--surface-2)), color-mix(in srgb, var(--accent) 12%, var(--surface-2))); color: var(--accent); border-color: color-mix(in srgb, var(--accent) 55%, var(--node-border)); box-shadow: 0 0 14px color-mix(in srgb, var(--accent) 35%, transparent); }
  .engine-text { display: flex; flex-direction: column; gap: 2px; min-width: 0; flex: 1; }
  .engine-label { font-size: 12px; font-weight: 500; color: inherit; }
  .engine-hint { font-size: 9px; line-height: 1.35; color: var(--faint); overflow: hidden; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; }
  .engine-state { flex: none; min-width: 24px; padding: 3px 6px; border-radius: 999px; background: var(--surface-2); text-align: center; font-size: 9px; color: var(--faint); }
  .engine-state.ok { background: color-mix(in srgb, var(--led-ok) 16%, var(--surface-2)); color: var(--led-ok); }
  .engine-state.warn { background: color-mix(in srgb, var(--led-warn) 16%, var(--surface-2)); color: var(--led-warn); }
  .engine.editing .engine-state { background: color-mix(in srgb, var(--accent) 18%, var(--surface-2)); color: var(--accent); }
  .engines .refresh { justify-self: start; margin-top: 6px; }
  .editor { min-width: 0; display: grid; gap: 11px; align-content: start; border-left: 1px dashed var(--node-border); padding-left: 17px; }
  header { display: flex; justify-content: space-between; align-items: center; gap: 8px; } header strong { font-size: 13px; font-weight: 500; } .status { font: 8px var(--font-mono); color: var(--muted); letter-spacing: 0.6px; }
  .why { font-size: 10px; line-height: 1.6; color: var(--muted); margin: 0; white-space: pre-line; overflow-wrap: anywhere; }
  .keys { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; } .fine { font-size: 9.5px; color: var(--faint); }
  .line { display: flex; justify-content: space-between; align-items: center; gap: 9px; } .eyebrow { color: var(--muted); font: 9px/1.5 var(--font-mono); letter-spacing: .7px; } .eyebrow b { color: var(--accent); margin: 0 3px; }
  .search { display: flex; align-items: center; gap: 7px; border: 1px solid var(--node-border); border-radius: var(--radius-control); padding-left: 9px; background: var(--bg); color: var(--muted); } .search:focus-within { border-color: var(--accent); }
  .search input { flex: 1; min-width: 0; padding: 8px; border: 0; background: transparent; color: var(--text); font-size: 11px; outline: none; }
  /* The voices as a grid of small cards, like the model catalogue: the name, the language, a word on the voice, and one mark for what a press does. */
  .catalogue { display: grid; grid-template-columns: repeat(auto-fill, minmax(180px, 1fr)); gap: 6px; padding-top: 10px; border-top: 1px solid var(--node-border); }
  .voice { position: relative; min-width: 0; }
  .voice-main { display: flex; flex-direction: column; justify-content: space-between; gap: 8px; text-align: left; padding: 9px 10px; min-height: 66px; width: 100%; background: var(--bg); border: 1px solid var(--node-border); border-radius: var(--radius-control); color: var(--muted); cursor: pointer; transition: border-color 120ms ease, background 120ms ease; }
  .voice-main:hover:not(:disabled) { border-color: var(--muted); }
  .voice.ready .voice-main { border-color: color-mix(in srgb, var(--led-ok) 40%, var(--node-border)); }
  .voice.lit .voice-main { border-color: var(--accent); background: color-mix(in srgb, var(--accent) 8%, var(--bg)); }
  .voice-copy { min-width: 0; padding-right: 18px; } .voice-copy strong { color: var(--text); font-size: 11px; font-weight: 500; display: block; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; } .voice-copy small { font-size: 9px; display: block; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .voice-foot { display: flex; align-items: center; justify-content: space-between; gap: 6px; } .hint { font-size: 9px; color: var(--faint); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .mark { flex: none; display: grid; place-items: center; width: 18px; height: 18px; border-radius: 50%; border: 1px solid var(--node-border); color: var(--muted); }
  .voice.ready .mark { background: color-mix(in srgb, var(--led-ok) 18%, transparent); border-color: color-mix(in srgb, var(--led-ok) 50%, var(--node-border)); color: var(--led-ok); }
  .voice.lit .mark { background: var(--accent); border-color: var(--accent); color: var(--accent-fg); }
  .voice-off { position: absolute; top: 6px; right: 6px; display: grid; place-items: center; width: 18px; height: 18px; border: 0; border-radius: 50%; background: transparent; color: var(--faint); cursor: pointer; } .voice-off:hover { color: var(--led-err); background: color-mix(in srgb, var(--led-err) 12%, transparent); }
  .voice-error { margin: 4px 0 0; font-size: 9px; color: var(--led-err); white-space: pre-wrap; }
  .pagination, .pagination > div { display: flex; justify-content: space-between; align-items: center; gap: 9px; } .pagination { color: var(--muted); font-size: 9px; } .pagination > div span { padding: 0 4px; }
  .empty { grid-column: 1 / -1; padding: 15px 0; margin: 0; font-size: 10px; color: var(--muted); }
  .steps { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 4px; }
  .steps li { display: flex; gap: 8px; font-size: 10.5px; color: var(--muted); } .steps li.failed { color: var(--led-err); }
  .steps .mark { width: 16px; height: 16px; border: 0; background: color-mix(in srgb, var(--led-ok) 18%, transparent); color: var(--led-ok); } .steps li.failed .mark { background: color-mix(in srgb, var(--led-err) 18%, transparent); color: var(--led-err); }
  .steps pre { margin: 4px 0 0; font-size: 9.5px; white-space: pre-wrap; overflow-wrap: anywhere; }
  .cmd { margin: 12px 0 0; padding: 8px 10px; font-size: 10.5px; background: var(--surface); border: 1px solid var(--node-border); border-radius: var(--radius-control); color: var(--fg); user-select: all; }
  .line.mono { font-size: 10.5px; color: var(--muted); display: flex; align-items: center; gap: 6px; justify-content: flex-start; }
  :global(.spin) { animation: spin 1.1s linear infinite; } @keyframes spin { to { transform: rotate(360deg); } }
  .error { color: var(--led-err); font-size: 10.5px; margin-top: 10px; overflow-wrap: anywhere; }
  button:disabled { opacity: .4; cursor: default; }
  @media (max-width: 620px) { .workspace { grid-template-columns: 1fr; } .engines { grid-template-columns: repeat(2, 1fr); } .engines .refresh { margin-top: 0; } .editor { border-left: 0; padding-left: 0; border-top: 1px dashed var(--node-border); padding-top: 12px; } }
</style>
