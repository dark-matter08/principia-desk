<script lang="ts">
  /** Live terminal-style feed of the agent working (gen:log events) —
   *  shown wherever the user would otherwise stare at a spinner. */
  import { app } from '../stores.svelte';

  let el = $state<HTMLElement | null>(null);
  const lines = $derived(app.genLog.slice(-12));

  $effect(() => {
    void lines.length;
    el?.scrollTo({ top: el.scrollHeight });
  });
</script>

<div class="alog mono" bind:this={el}>
  <div class="alog-head">AGENT_LOG · tail -f</div>
  {#if lines.length === 0}
    <div class="aline dim">waiting for agent output…</div>
  {/if}
  {#each lines as line, i}
    <div class="aline" class:dim={i < lines.length - 3}>{line}</div>
  {/each}
  <div class="aline cursor">▌</div>
</div>

<style>
  .alog {
    width: min(680px, 90vw);
    max-height: 220px;
    overflow-y: auto;
    background: var(--bg);
    border: 1px solid var(--node-border);
    border-radius: var(--radius-panel);
    padding: 10px 14px;
    text-align: left;
  }
  .alog-head {
    font-size: 9px;
    letter-spacing: 1.5px;
    color: var(--faint);
    margin-bottom: 7px;
  }
  .aline {
    font-size: 11px;
    line-height: 1.75;
    color: var(--led-ok);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .aline.dim {
    color: var(--faint);
  }
  .aline.cursor {
    color: var(--accent);
    animation: led-pulse 1s steps(2) infinite;
  }
</style>
