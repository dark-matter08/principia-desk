<script lang="ts">
  import { untrack } from 'svelte';
  import { api } from '$lib/ipc';
  import { shortlistOptions } from '$lib/features/runners/model-list';
  import Dropdown from './Dropdown.svelte';
  let { value = $bindable('default'), agent = 'claude', revision = 0, disabled = false, onconfigure, onchange, hideLabel = false }: { value?: string; agent?: string; revision?: number; disabled?: boolean; onconfigure?: () => void; onchange?: () => void; hideLabel?: boolean } = $props();
  let models = $state<string[]>([]); let query = $state(''); let loading = $state(false); let error = $state('');
  let sequence = 0;
  const options = $derived(shortlistOptions(models, value).filter(m => m.label.toLowerCase().includes(query.toLowerCase())));
  async function load(runner: string) {
    const token = ++sequence; loading = true; error = '';
    try { const setup = await api.getRunnerConfiguration(runner); if (token === sequence) models = setup.models; }
    catch (e) { if (token === sequence) error = String(e); }
    finally { if (token === sequence) loading = false; }
  }
  $effect(() => { const runner = agent; void revision; untrack(() => { models = []; query = ''; void load(runner); }); return () => { sequence++; }; });
</script>

<div class="model-picker">
  {#if models.length > 8}<input {disabled} aria-label="Search saved models" placeholder="Find a saved model…" bind:value={query} />{/if}
  <Dropdown label="Model" {hideLabel} {value} {options} disabled={loading || disabled} onchange={selected => { value = selected; onchange?.(); }} placeholder={loading ? 'Loading saved models…' : value} />
  <div class="foot"><span>{models.length} saved {models.length === 1 ? 'model' : 'models'}</span>{#if onconfigure}<button type="button" onclick={onconfigure}>Manage models ↗</button>{/if}</div>
  {#if query && !options.length}<p role="status">No saved models match.</p>{/if}
  {#if error}<p class="error" role="status">{error}</p>{/if}
</div>
<style>
  .model-picker { display: grid; gap: 7px; min-width: 0; align-content: start; }
  .foot { display: flex; align-items: center; justify-content: space-between; gap: 8px; flex-wrap: wrap; color: var(--muted); font: 9px/1.5 var(--font-mono); }
  button { border: 0; background: transparent; color: var(--accent); font-size: 10px; padding: 0; cursor: pointer; }
  input { background: var(--bg); border: 1px solid var(--node-border); border-radius: var(--radius-control); color: var(--text); padding: 8px; width: 100%; font-size: 11px; }
  p { font-size: 11px; color: var(--muted); margin: 0; overflow-wrap: anywhere; } .error { color: var(--led-err); }
</style>
