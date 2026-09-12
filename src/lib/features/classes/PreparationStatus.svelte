<script lang="ts">
  import { app } from '../../stores.svelte';
  import AgentLog from '../../components/AgentLog.svelte';
  import StatusLED from '../../components/StatusLED.svelte';
  let expanded = $state(false);
  let now = $state(Date.now());
  const uid = $props.id();
  $effect(() => { const timer = setInterval(() => now = Date.now(), 1000); return () => clearInterval(timer); });
  const seconds = $derived(Math.max(0, Math.floor((now - (app.preparingClass?.startedAt ?? now)) / 1000)));
</script>
{#if app.preparingClass}
  <section class="preparation" aria-label="Lesson preparation">
    <div class="summary"><StatusLED tone="pending" /><div class="copy"><strong role="status">Preparing {app.preparingClass.label}</strong><p>{app.preparingClass.agent} · {app.preparingClass.model} <span aria-hidden="true">· {Math.floor(seconds/60)}:{String(seconds%60).padStart(2,'0')}</span></p></div><button type="button" class="ghost mono-ghost" aria-expanded={expanded} aria-controls={uid} onclick={() => expanded = !expanded}>{expanded ? 'Hide activity' : 'Show activity'}</button></div>
    <p class="hint">Your tutor is researching, writing and checking the lesson. This can take a few minutes. You can browse the desk while it prepares.</p>
    <div id={uid} hidden={!expanded}><AgentLog /></div>
  </section>
{/if}
<style>
  .preparation { flex-shrink: 0; margin: 14px 28px 0; padding: 13px 16px; background: var(--node-bg); border: 1px solid var(--violet); border-radius: var(--radius-panel); }
  .summary { display: flex; align-items: center; gap: 12px; } .copy { flex: 1; min-width: 0; } strong { font-size: 13px; font-weight: 500; } p { color: var(--muted); font: 10px/1.5 var(--font-mono); margin: 5px 0 0; overflow-wrap: anywhere; }
  .hint { font: 11px/1.5 var(--font-body); margin: 8px 0 0; } button { font-size: 10px; white-space: nowrap; } [hidden] { display: none; } .preparation :global(.alog) { width: 100%; margin-top: 12px; max-height: 130px; }
  @media(max-width:720px) { .preparation { margin: 12px 18px 0; } .hint { font-size: 10px; } }
</style>
