<script lang="ts">
  /**
   * Draws the question `dialog.ask` is waiting on: a sheet over the desk
   * with the title, the message, an optional line of text, and the desk's
   * own keys. Escape cancels; Enter confirms.
   */
  import { tick } from 'svelte';
  import { dialog } from './dialog.svelte';
  import { CircleAlert, CircleHelp } from 'lucide-svelte';

  let value = $state('');
  let input = $state<HTMLInputElement | undefined>(undefined);
  let confirmKey = $state<HTMLButtonElement | undefined>(undefined);
  const request = $derived(dialog.current);
  const ready = $derived(!request?.field?.required || value.trim().length > 0);

  $effect(() => {
    if (!request) return;
    value = request.field?.value ?? '';
    void tick().then(() => (request.field ? input : confirmKey)?.focus());
  });

  function onkeydown(event: KeyboardEvent) {
    if (!request) return;
    if (event.key === 'Escape') { event.preventDefault(); dialog.answer(false); }
    if (event.key === 'Enter' && ready) { event.preventDefault(); dialog.answer(true, value); }
  }
</script>

<svelte:window {onkeydown} />

{#if request}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="veil" onmousedown={(event) => { if (event.target === event.currentTarget) dialog.answer(false); }}>
    <div class="sheet" role="alertdialog" aria-modal="true" aria-labelledby="dialog-title" tabindex="-1">
      <div class="sheet-head">
        <span class="tile" class:danger={request.danger}>{#if request.danger}<CircleAlert size={16} />{:else}<CircleHelp size={16} />{/if}</span>
        <div>
          <h3 id="dialog-title">{request.title}</h3>
          {#if request.message}<p>{request.message}</p>{/if}
        </div>
      </div>
      {#if request.field}
        <label class="field">
          <span>{request.field.label}{#if !request.field.required}<small>optional</small>{/if}</span>
          <input bind:this={input} bind:value placeholder={request.field.placeholder ?? ''} />
        </label>
      {/if}
      <div class="keys">
        <button type="button" class="ghost mono-ghost" onclick={() => dialog.answer(false)}>{request.cancel ?? 'Cancel'}</button>
        <button type="button" class="cta mono-cta" class:danger={request.danger} bind:this={confirmKey} disabled={!ready} onclick={() => dialog.answer(true, value)}>{request.confirm ?? 'Confirm'}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .veil { position: fixed; inset: 0; z-index: 200; display: grid; place-items: center; padding: 24px; background: color-mix(in srgb, var(--bg) 62%, transparent); backdrop-filter: blur(3px); animation: veil 0.16s ease-out; }
  @keyframes veil { from { opacity: 0; } }
  .sheet { display: flex; flex-direction: column; gap: 16px; width: min(460px, 100%); padding: 20px 22px; border: 1px solid var(--node-border); border-radius: var(--radius-panel); background: var(--node-bg); box-shadow: 0 24px 60px #0008, 0 2px 8px #0006; animation: sheet 0.18s cubic-bezier(0.2, 0.8, 0.3, 1); }
  @keyframes sheet { from { opacity: 0; transform: translateY(8px) scale(0.985); } }
  .sheet-head { display: flex; gap: 14px; align-items: flex-start; }
  .tile { flex: none; display: grid; place-items: center; width: 36px; height: 36px; border-radius: 10px; background: var(--surface-2); color: var(--accent); }
  .tile.danger { color: var(--led-err); }
  h3 { margin: 0; font: 17px var(--font-display); }
  p { margin: 6px 0 0; font-size: 12.5px; line-height: 1.55; color: var(--muted); }
  .field { display: flex; flex-direction: column; gap: 6px; }
  .field > span { font-size: 11px; color: var(--muted); } .field small { margin-left: 6px; color: var(--faint); font-size: 10px; }
  .field input { width: 100%; padding: 9px 11px; border: 1px solid var(--node-border); border-radius: var(--radius-control); background: var(--bg); color: var(--fg); font: 12.5px/1.5 var(--font-body); }
  .field input:focus { outline: none; border-color: var(--accent); }
  .keys { display: flex; justify-content: flex-end; gap: 10px; }
  .cta.danger { --accent: var(--led-err); }
</style>
