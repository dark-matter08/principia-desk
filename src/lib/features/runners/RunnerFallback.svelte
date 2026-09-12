<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/ipc';
  import type { RunnerInfo, AgentPolicy } from '$lib/contracts/agents';
  import Dropdown from '$lib/components/Dropdown.svelte';
  import ModelPicker from '$lib/components/ModelPicker.svelte';
  let policy = $state<AgentPolicy | null>(null);
  let runners = $state<RunnerInfo[]>([]);
  let agent = $state(''); let model = $state('default'); let expanded = $state(false);
  let busy = $state(false); let message = $state(''); let error = $state('');
  onMount(() => { void (async () => { try { [policy, runners] = await Promise.all([api.getAgentPolicy(),api.listAgentRunners()]); agent = policy.fallback_agent ?? ''; model = policy.fallback_model; } catch (e) { error = String(e); } })(); });
  const options = $derived([{ value: '', label: 'No fallback' }, ...runners.map(r => ({ value: r.provider, label: r.label, description: r.available ? 'Configured · test model access before use' : r.detail }))]);
  async function save() { if (!policy) return; busy = true; error = ''; message = ''; try { await api.setAgentPolicy({ ...policy, fallback_agent: agent || null, fallback_model: model }); message = agent ? 'Fallback saved.' : 'Fallback disabled.'; } catch (e) { error = String(e); } finally { busy = false; } }
</script>
<div class="fallback">
  <button type="button" class="disclosure mono" aria-expanded={expanded} onclick={() => expanded = !expanded}>Optional fallback <span>{expanded ? '−' : '+'}</span></button>
  {#if expanded}
    <p>If the selected runner fails, grading, planning and language enrichment can try this runner with its own model. Course writing and course chat keep their chosen runner. Choose an API fallback only if you intend to send that work to its provider.</p>
    <Dropdown label="Fallback runner" value={agent} {options} onchange={value => { agent = value; model = runners.find(r => r.provider === value)?.saved_model || runners.find(r => r.provider === value)?.default_model || 'default'; message = ''; }} />
    {#if agent}{#key agent}<ModelPicker {agent} bind:value={model} />{/key}{/if}
    <button type="button" class="save mono" disabled={busy || !policy} onclick={save}>{busy ? 'Saving…' : 'Save fallback'}</button>
    {#if message}<p role="status">{message}</p>{/if}
  {/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
</div>
<style>
  .fallback { border-top: 1px dashed var(--node-border); margin-top: 20px; padding-top: 14px; display: grid; gap: 12px; }
  .disclosure { display: flex; justify-content: space-between; border: 0; padding: 0; color: var(--muted); background: transparent; font-size: 11px; cursor: pointer; }
  p { margin: 0; color: var(--muted); font-size: 11px; line-height: 1.6; }
  .save { justify-self: start; padding: 8px 11px; background: var(--bg); border: 1px solid var(--node-border); color: var(--text); font-size: 10px; cursor: pointer; }
  .error { color: var(--led-err); }
</style>
