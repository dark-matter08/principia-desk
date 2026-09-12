<script lang="ts">
  import type { Snippet } from 'svelte';
  let { number, last = false, children }: { number: string; last?: boolean; children: Snippet } = $props();
</script>

<div class="flow-stage" class:last>
  <span class="step mono" aria-hidden="true">{number}</span>
  {@render children()}
</div>

<style>
  /* The numbered service rail from main's SetupWizard, shared by the new forms. */
  .flow-stage { position: relative; padding-left: 38px; margin-bottom: 22px; min-width: 0; }
  .step { position: absolute; left: 0; top: 10px; width: 24px; text-align: center; font-size: 11px; color: var(--faint); border: 1px solid var(--node-border); border-radius: var(--radius-control); padding: 3px 0; background: var(--bg); }
  .flow-stage:not(.last)::after { content: ''; position: absolute; left: 11px; top: 100%; height: 22px; border-left: 1.5px dashed var(--violet); opacity: 0.7; }
  .flow-stage.last { margin-bottom: 0; }
  @media (max-width: 480px) { .flow-stage { padding-left: 30px; } .step { width: 22px; font-size: 10px; } }
</style>
