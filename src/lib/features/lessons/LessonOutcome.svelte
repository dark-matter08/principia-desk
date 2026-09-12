<script lang="ts">
  /** Submit action before grading; result card and return action after. */
  import { CheckCircle2, RotateCcw, Send } from 'lucide-svelte';

  let {
    result = null,
    busy = false,
    disabled = false,
    submitLabel,
    busyLabel = 'recording…',
    hint = '',
    returnLabel,
    onsubmit,
    onreturn,
  }: {
    result?: { passed: boolean; score: number; headline: string; message: string; extra?: string | null } | null;
    busy?: boolean;
    disabled?: boolean;
    submitLabel: string;
    busyLabel?: string;
    hint?: string;
    returnLabel: string;
    onsubmit: () => void;
    onreturn: () => void;
  } = $props();
</script>

{#if result}
  <section class="result-card" class:passed={result.passed} aria-live="polite">
    {#if result.passed}<CheckCircle2 size={18} />{:else}<RotateCcw size={18} />{/if}
    <div>
      <strong>{Math.round(result.score * 100)}% · {result.headline}</strong>
      <p>{result.message}</p>
      {#if result.extra}<p class="extra">{result.extra}</p>{/if}
    </div>
  </section>
  <button class="cta mono-cta submit" type="button" onclick={onreturn}>{returnLabel}</button>
{:else}
  <button class="cta mono-cta submit" type="button" disabled={disabled || busy} onclick={onsubmit}>
    <Send size={13} /> {busy ? busyLabel : submitLabel}
  </button>
  {#if hint && disabled && !busy}<p class="hint">{hint}</p>{/if}
{/if}

<style>
  .result-card { display: flex; gap: 9px; border: 1px solid var(--amber); border-radius: var(--radius-control); padding: 10px; margin-top: 13px; color: var(--amber); }
  .result-card.passed { border-color: var(--green); color: var(--green); }
  .result-card p { margin: 3px 0 0; color: var(--muted); font-size: 9px; line-height: 1.5; }
  .result-card .extra { color: var(--text); }
  .submit { width: 100%; margin-top: 12px; display: flex; justify-content: center; align-items: center; gap: 6px; }
  .hint { color: var(--faint); font-size: 9px; margin: 8px 0 0; line-height: 1.5; }
</style>
