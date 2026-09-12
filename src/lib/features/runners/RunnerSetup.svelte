<script lang="ts">
  import { onMount, untrack, tick } from 'svelte';
  import { Library, Zap, Settings2, Terminal, KeyRound, HardDrive, Globe, Wrench, Coins, CircleCheck, CircleDashed, Compass, BookOpenCheck, PenLine, ShieldCheck, ListChecks } from 'lucide-svelte';
  import { api } from '$lib/ipc';
  import type { RunnerInfo, HealthCheck } from '$lib/contracts/agents';
  import NodeCard from '$lib/components/NodeCard.svelte';
  import Dropdown from '$lib/components/Dropdown.svelte';
  import ModelPicker from '$lib/components/ModelPicker.svelte';
  import RunnerLibrary from './RunnerLibrary.svelte';
  let { agent = $bindable('claude'), model = $bindable('opus'), customBin = $bindable(''), allowKeyEditing = true, onUse, onKeyChanged, onchange, initiallyExpanded = false, selectionHint = 'Applied when you save this setup.' }: {
    agent?: string; model?: string; customBin?: string; allowKeyEditing?: boolean; onUse?: () => Promise<void>; onKeyChanged?: () => void | Promise<void>; onchange?: () => void; initiallyExpanded?: boolean; selectionHint?: string;
  } = $props();
  let runners = $state<RunnerInfo[]>([]); let loading = $state(true); let expanded = $state(false); let editing = $state('claude');
  let libraryHeader: HTMLDivElement | undefined;
  let applied = $state({ agent: '', model: '', command: '' });
  const pending = $derived(!!onUse && !!applied.agent && (agent !== applied.agent || model !== applied.model || (agent === 'custom' && customBin !== applied.command)));
  let error = $state(''); let busy = $state(''); let result = $state<HealthCheck | null>(null); let saved = $state(false); let revision = $state(0);
  const picked = $derived(runners.find(r => r.provider === agent));
  const KIND_LABEL: Record<string, string> = { cli: 'CLI agent', api: 'API provider', local: 'Local model' };
  const kindIcon = $derived(picked?.kind === 'api' ? KeyRound : picked?.kind === 'local' ? HardDrive : Terminal);
  const options = $derived([...runners].sort((a,b) => Number(b.available) - Number(a.available)).map(r => ({ value: r.provider, label: r.label, description: `${r.kind === 'local' ? 'Local' : r.kind.toUpperCase()} · ${r.available ? 'configured' : 'setup needed'}` })));
  $effect(() => { void agent; void model; void customBin; untrack(() => { result = null; saved = false; }); });
  async function refresh() { runners = await api.listAgentRunners(); revision++; }
  onMount(() => { expanded = initiallyExpanded; void (async () => { try { await refresh(); editing = agent; applied = { agent, model, command: customBin }; } catch (e) { error = String(e); } finally { loading = false; } })(); });
  async function configurationChanged() { await refresh(); if (editing === 'custom' && agent === 'custom') { customBin = (await api.getRunnerConfiguration('custom')).custom_command; onchange?.(); } await onKeyChanged?.(); }
  async function configure() { editing = agent; expanded = true; await tick(); libraryHeader?.scrollIntoView({ block: 'start', behavior: 'smooth' }); }
  async function choose(provider: string) {
    busy = 'selection'; error = '';
    try {
      const setup = await api.getRunnerConfiguration(provider);
      const runner = runners.find(r => r.provider === provider);
      agent = provider;
      model = runner?.saved_model && setup.models.includes(runner.saved_model) ? runner.saved_model : setup.models[0] || runner?.default_model || 'default';
      if (provider === 'custom') customBin = setup.custom_command;
      onchange?.();
    } catch (e) { error = String(e); } finally { busy = ''; }
  }
  async function commandForSelection() {
    if (agent !== 'custom') return customBin;
    return customBin || (await api.getRunnerConfiguration(agent)).custom_command;
  }
  async function test() {
    busy = 'test'; error = ''; result = null;
    const selection = [agent,model];
    try { const checked = await api.testAgentConnection(agent, await commandForSelection(), model); if (selection[0] === agent && selection[1] === model) result = checked; }
    catch (e) { error = String(e); } finally { busy = ''; }
  }
  async function use() {
    busy = 'save'; error = '';
    try { customBin = await commandForSelection(); await onUse?.(); applied = { agent, model, command: customBin }; await refresh(); saved = true; }
    catch (e) { error = String(e); } finally { busy = ''; }
  }
</script>

<div class="runner-workspace">
  <NodeCard Icon={Library} name="runner-library" badge="configuration" badgeTone="muted">
    <div class="library-summary" bind:this={libraryHeader}><div><strong>Runners &amp; models</strong><p>Set up providers and keep a shortlist of models for each.</p><span class="counts mono">5 CLI · 7 API · Ollama local</span></div><button type="button" class="bracket manage" aria-expanded={expanded} onclick={() => expanded = !expanded}><span class="bracket-well"><Settings2 size={12} /></span>{expanded ? 'Close library' : 'Configure'}</button></div>
    <div class="library-content" hidden={!expanded}>
      {#if loading}<p class="hint" role="status">Loading runner library…</p>{:else}<RunnerLibrary {runners} bind:selected={editing} {allowKeyEditing} onchanged={configurationChanged} />{/if}
    </div>
  </NodeCard>
  <NodeCard Icon={Zap} name="active-tutor" badge={picked?.label ?? agent} badgeTone="violet">
    {@const KindIcon = kindIcon}
    <div class="tutor">
      <div class="tutor-top">
      <div class="tutor-who">
      <div class="tutor-id">
        <span class="tutor-tile" class:ready={picked?.available} aria-hidden="true"><KindIcon size={20} /><small>{picked ? (picked.kind === 'local' ? 'LOCAL' : picked.kind.toUpperCase()) : '…'}</small></span>
        <div class="tutor-copy">
          <strong>Active tutor</strong>
          <p>Writes every lesson, the practice and the checks, with the runner and model chosen here.</p>
          <ul class="traits mono" aria-label="What this runner offers">
            <li class="trait state" class:ok={picked?.available}>{#if picked?.available}<CircleCheck size={11} /> configured{:else}<CircleDashed size={11} /> setup needed{/if}</li>
            {#if picked}<li class="trait">{KIND_LABEL[picked.kind] ?? picked.kind}</li>{/if}
            {#if picked?.web}<li class="trait"><Globe size={10} /> browses</li>{/if}
            {#if picked?.tools}<li class="trait"><Wrench size={10} /> tools</li>{/if}
            {#if picked?.metered}<li class="trait"><Coins size={10} /> metered</li>{/if}
          </ul>
        </div>
      </div>
      <!-- What the tutor does with each study time, in the order it happens. -->
      <ol class="pipeline" aria-label="What the tutor does for each lesson">
        <li><span class="step-icon"><Compass size={12} /></span><span class="step-text"><b>plans</b></span></li>
        <li><span class="step-icon"><BookOpenCheck size={12} /></span><span class="step-text"><b>reads</b></span></li>
        <li><span class="step-icon"><PenLine size={12} /></span><span class="step-text"><b>writes</b></span></li>
        <li><span class="step-icon"><ShieldCheck size={12} /></span><span class="step-text"><b>audits</b></span></li>
        <li><span class="step-icon"><ListChecks size={12} /></span><span class="step-text"><b>checks</b></span></li>
      </ol>
      <p class="pipeline-note mono">prepared ~20 min before each study time · a class may name its own tutor</p>
      </div>
      <div class="selection">
        <div class="pick"><span class="pick-label mono">RUNNER</span><Dropdown label="Runner" hideLabel value={agent} {options} disabled={loading || !!busy} onchange={choose} /><span class="pick-foot">{agent === 'custom' ? 'Uses the command saved in the runner library.' : picked ? picked.detail : ' '}</span></div>
        <div class="pick"><span class="pick-label mono">MODEL</span>{#key agent}<ModelPicker {agent} bind:value={model} {revision} disabled={!!busy} onconfigure={configure} {onchange} hideLabel />{/key}</div>
      </div>
      </div>
      <div class="tutor-foot">
        <span class="state-line mono" role="status" class:pending>{#if pending}<i class="led warn"></i> selection not applied yet{:else if onUse}<i class="led ok"></i> applied · {picked?.label ?? agent} / {model || 'runner default'}{:else}<i class="led idle"></i> {selectionHint}{/if}</span>
        <div class="actions">{#if onUse}<button type="button" class="cta mono-cta" disabled={!!busy || loading} onclick={use}>{busy === 'save' ? 'Applying…' : saved ? 'Active tutor saved ✓' : 'Use for study'}</button>{/if}<button type="button" class="ghost mono-ghost" disabled={!!busy || loading} onclick={test}>{busy === 'test' ? 'Testing…' : 'Test connection'}</button></div>
      </div>
      {#if result}<div class="test-result" class:ok={result.ok} role="status"><span class="mono"><i class="led" class:ok={result.ok} class:err={!result.ok}></i>{result.runner} · {result.model} · {(result.duration_ms/1000).toFixed(1)}s</span><p>{result.detail}</p></div>{/if}
      {#if error}<p class="error" role="alert">{error}</p>{/if}
    </div>
  </NodeCard>
</div>

<style>
  .runner-workspace { display: grid; gap: 16px; }
  .pending { color: var(--accent); font-size: 9px; margin-top: 12px; }
  .library-summary { scroll-margin-top: 160px; display: flex; align-items: center; justify-content: space-between; gap: 14px; } strong { display: block; font-size: 14px; font-weight: 500; } p { margin: 5px 0 0; font-size: 11px; color: var(--muted); line-height: 1.6; }
  .counts { display: block; margin-top: 9px; color: var(--faint); font-size: 9px; }
  .manage { white-space: nowrap; flex: none; }
  .library-content { padding-top: 20px; } .library-content[hidden] { display: none; }
  /* The active tutor: who writes the lessons, drawn as a control panel: the
     runner's tile and traits, the two picks, then the state and the keys. */
  .tutor { display: flex; flex-direction: column; gap: 16px; }
  /* Who the tutor is on the left; the two picks stacked on the right. */
  .tutor-top { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); gap: 22px 28px; align-items: start; }
  .tutor-who { display: flex; flex-direction: column; gap: 14px; min-width: 0; }
  .tutor-id { display: flex; gap: 14px; align-items: flex-start; }
  /* The pipeline: five steps on one track, each a lit stop with a word and a line. */
  .pipeline { position: relative; display: grid; grid-template-columns: repeat(5, minmax(0, 1fr)); gap: 6px; margin: 2px 0 0; padding: 10px 10px 8px; list-style: none; border: 1px dashed var(--node-border); border-radius: var(--radius-control); background: color-mix(in srgb, var(--surface) 50%, transparent); }
  .pipeline::before { content: ''; position: absolute; left: 22px; right: 22px; top: 21px; height: 1px; background: repeating-linear-gradient(90deg, color-mix(in srgb, var(--accent) 45%, var(--node-border)) 0 6px, transparent 6px 10px); }
  .pipeline li { position: relative; display: flex; flex-direction: column; align-items: center; gap: 5px; min-width: 0; text-align: center; }
  .step-icon { display: grid; place-items: center; width: 22px; height: 22px; border-radius: 50%; border: 1px solid color-mix(in srgb, var(--accent) 45%, var(--node-border)); background: var(--node-bg); color: var(--accent); box-shadow: 0 0 0 3px var(--node-bg); }
  .step-text b { font: 500 10px var(--font-mono); letter-spacing: 0.4px; color: var(--fg); }
  .pipeline-note { margin: 0; font-size: 8.5px; letter-spacing: 0.4px; color: var(--faint); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .tutor-tile { flex: none; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 4px; width: 56px; height: 56px; border: 1px solid var(--node-border); border-radius: 12px; background: var(--surface-2); color: var(--muted); transition: border-color 0.2s, background 0.2s, color 0.2s, box-shadow 0.2s; }
  .tutor-tile small { font: 7.5px var(--font-mono); letter-spacing: 1px; color: var(--faint); }
  .tutor-tile.ready { border-color: color-mix(in srgb, var(--accent) 45%, var(--node-border)); background: linear-gradient(180deg, color-mix(in srgb, var(--accent) 22%, var(--surface)), var(--surface)); color: var(--accent); box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 10%, transparent), 0 10px 24px -14px var(--accent); }
  .tutor-tile.ready small { color: color-mix(in srgb, var(--accent) 70%, var(--muted)); }
  .tutor-copy { min-width: 0; } .tutor-copy strong { display: block; font-size: 14px; font-weight: 500; } .tutor-copy p { margin: 4px 0 0; font-size: 11px; color: var(--muted); line-height: 1.6; max-width: 60ch; }
  .traits { display: flex; flex-wrap: wrap; gap: 5px; margin: 9px 0 0; padding: 0; list-style: none; }
  .trait { display: inline-flex; align-items: center; gap: 5px; padding: 3px 8px; border: 1px solid var(--node-border); border-radius: 999px; font-size: 9px; letter-spacing: 0.5px; color: var(--muted); background: var(--surface); }
  .trait.state { color: var(--warn-fg); border-color: color-mix(in srgb, var(--warn-fg) 35%, var(--node-border)); } .trait.state.ok { color: var(--ok-fg); border-color: color-mix(in srgb, var(--ok-fg) 35%, var(--node-border)); background: var(--ok-bg); }
  .selection { display: grid; grid-template-columns: minmax(0, 1fr); gap: 14px; align-items: start; padding: 0 0 0 28px; border-left: 1px dashed var(--node-divider); }
  .pick { display: grid; gap: 7px; min-width: 0; align-content: start; }
  .pick-label { font-size: 9px; letter-spacing: 1.2px; color: var(--faint); }
  .pick-foot { font: 9px/1.5 var(--font-mono); color: var(--muted); min-height: 14px; overflow-wrap: anywhere; }
  .tutor-foot { display: flex; align-items: center; justify-content: space-between; gap: 14px; flex-wrap: wrap; padding-top: 14px; border-top: 1px dashed var(--node-divider); }
  .state-line { display: inline-flex; align-items: center; gap: 7px; font-size: 9.5px; letter-spacing: 0.5px; color: var(--muted); } .state-line.pending { color: var(--warn-fg); }
  .led { width: 6px; height: 6px; border-radius: 50%; background: var(--led-idle); flex: none; } .led.ok { background: var(--led-ok); box-shadow: 0 0 6px var(--led-ok); } .led.warn { background: var(--led-warn); box-shadow: 0 0 6px var(--led-warn); } .led.err { background: var(--led-err); box-shadow: 0 0 6px var(--led-err); }
  .hint { color: var(--muted); font-size: 10px; line-height: 1.6; } .actions { display: flex; gap: 9px; align-items: center; flex-wrap: wrap; margin-left: auto; }

  .test-result { border: 1px solid var(--node-border); border-radius: var(--radius-control); padding: 11px 12px; font-size: 10px; background: var(--surface); } .test-result.ok { border-color: color-mix(in srgb, var(--led-ok) 50%, var(--node-border)); } .test-result > span { display: inline-flex; align-items: center; gap: 7px; font-size: 9.5px; letter-spacing: 0.5px; } .test-result p { margin: 6px 0 0; overflow-wrap: anywhere; } .error { color: var(--led-err); overflow-wrap: anywhere; }
  @media(max-width:760px) { .tutor-top { grid-template-columns: 1fr; } .selection { padding: 16px 0 0; border-left: 0; border-top: 1px dashed var(--node-divider); } }
  @media(max-width:620px) { .library-summary { align-items: flex-start; } .tutor-foot { flex-direction: column; align-items: flex-start; } .actions { margin-left: 0; } }
</style>
