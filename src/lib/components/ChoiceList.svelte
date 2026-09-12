<script lang="ts">
  import Markdown from './Markdown.svelte';
  let { options, value = $bindable(''), label, disabled = false, onchange }: {
    options: readonly { id: string; text: string }[]; value?: string; label: string; disabled?: boolean;
    onchange?: (value: string) => void;
  } = $props();
  function select(id: string, button: HTMLButtonElement) { if (!disabled) { button.focus(); value = id; onchange?.(id); } }
  function navigate(event: KeyboardEvent, index: number) {
    const direction = ['ArrowDown', 'ArrowRight'].includes(event.key) ? 1 : ['ArrowUp', 'ArrowLeft'].includes(event.key) ? -1 : 0;
    if (!direction && !['Home', 'End'].includes(event.key)) return;
    event.preventDefault();
    const next = event.key === 'Home' ? 0 : event.key === 'End' ? options.length - 1 : (index + direction + options.length) % options.length;
    const button = (event.currentTarget as HTMLButtonElement).parentElement?.querySelectorAll<HTMLButtonElement>('button')[next];
    if (button) select(options[next].id, button);
  }
</script>
<div class="choices" role="radiogroup" aria-label={label}>
  {#each options as option, i}
    <button class="choice" class:selected={value === option.id} role="radio" aria-checked={value === option.id}
      tabindex={value === option.id || (!options.some((item) => item.id === value) && i === 0) ? 0 : -1}
      {disabled} onclick={(event) => select(option.id, event.currentTarget)} onkeydown={(event) => navigate(event, i)}>
      <span class="key mono">{String.fromCharCode(65 + i)}</span><Markdown markdown={option.text} compact />
    </button>
  {/each}
</div>
<style>
  .choices { display: flex; flex-direction: column; gap: 10px; }
  .choice { display: flex; align-items: flex-start; gap: 12px; width: 100%; min-width: 0; text-align: left; background: var(--bg); color: var(--fg); border: 1px solid var(--node-border); border-radius: var(--radius-control); padding: 12px 14px; font: 14px/1.5 var(--font-body); cursor: pointer; }
  .choice:hover { border-color: var(--muted); }
  .choice.selected { border-color: var(--violet); background: var(--surface-2); }
  .choice:focus-visible { outline: 2px solid var(--accent); outline-offset: 3px; }
  .choice:disabled { cursor: default; opacity: .65; }
  .key { font-size: 11px; color: var(--muted); border: 1px solid var(--node-border); border-radius: var(--radius-detail); padding: 1px 7px; flex-shrink: 0; }
  .selected .key { color: var(--teal-fg); border-color: var(--teal); }
  .choice :global(.md) { flex: 1; min-width: 0; }
</style>
