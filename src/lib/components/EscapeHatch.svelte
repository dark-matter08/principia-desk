<script lang="ts">
  import { api } from '../ipc';
  import { app } from '../stores.svelte';
  import { ShieldAlert, X } from 'lucide-svelte';

  let open = $state(false);
  let phrase = $state('');
  let typed = $state('');
  let error = $state('');

  async function reveal() {
    phrase = await api.getEscapePhrase();
    open = true;
  }

  async function attempt() {
    error = '';
    const ok = await api.escapeSession(typed).catch((e) => {
      error = String(e);
      return false;
    });
    if (ok) {
      open = false;
      await app.escaped();
    } else if (!error) {
      error = 'phrase does not match';
      typed = '';
    }
  }

  function noPaste(e: ClipboardEvent) {
    e.preventDefault();
  }
</script>

<div class="hatch">
  {#if !open}
    <button class="hatch-link mono" onclick={reveal}>
      <ShieldAlert size={12} /> break glass
    </button>
  {:else}
    <div class="hatch-panel">
      <div class="hatch-head mono">
        <span class="bg-tag">BREAK GLASS</span>
        <span class="hatch-warn">
          circuit breaker: releases the lock: a focused class lesson is paused with its work
          kept, an in-progress daily session is marked skipped
        </span>
      </div>
      <svg class="phrase-svg" viewBox="0 0 640 28" preserveAspectRatio="xMidYMid meet">
        <text x="320" y="19" text-anchor="middle">{phrase}</text>
      </svg>
      <input
        class="mono"
        type="text"
        placeholder="type the phrase above exactly; paste disabled"
        bind:value={typed}
        onpaste={noPaste}
        onkeydown={(e) => e.key === 'Enter' && attempt()}
      />
      <div class="hatch-actions">
        <button class="ghost mono-ghost" onclick={() => (open = false)}>stand down</button>
        <button class="ghost mono-ghost danger" onclick={attempt}>trip breaker</button>
      </div>
      {#if error}<p class="hatch-error mono"><X size={11} /> {error}</p>{/if}
    </div>
  {/if}
</div>

<style>
  .hatch {
    position: fixed;
    bottom: 14px;
    right: 18px;
    z-index: 50;
    max-width: 480px;
  }
  /* Fixed colors (not theme vars): must be legible on BOTH the dark noir
     screens and the reader's light paper theme. */
  .hatch-link {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: rgba(31, 19, 22, 0.92);
    border: 1px dashed #793030;
    border-radius: var(--radius-detail);
    color: #d8918e;
    font-size: 10px;
    letter-spacing: 1px;
    padding: 6px 13px;
    cursor: pointer;
    opacity: 0.85;
  }
  .hatch-link:hover {
    opacity: 1;
    color: #f09595;
    border-color: #a04545;
  }
  .hatch-panel {
    background: #1f1316;
    border: 1px dashed #793030;
    border-radius: var(--radius-panel);
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .hatch-head {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .bg-tag {
    font-size: 9px;
    color: var(--led-err);
    border: 1px solid #793030;
    border-radius: var(--radius-detail);
    padding: 3px 6px;
    letter-spacing: 1px;
    white-space: nowrap;
  }
  .hatch-warn {
    font-size: 10px;
    color: #a05050;
  }
  .phrase-svg {
    width: 100%;
    pointer-events: none;
  }
  .phrase-svg text {
    font-family: var(--font-mono);
    font-size: 13px;
    fill: var(--muted);
  }
  input {
    font-size: 12px;
    background: var(--bg);
    border: 1px solid #3a2228;
    color: #d8b0a8;
  }
  input:focus {
    border-color: #793030;
  }
  .hatch-actions {
    display: flex;
    justify-content: space-between;
  }
  .danger {
    color: var(--led-err);
    border-color: #793030;
  }
  .hatch-error {
    margin: 0;
    font-size: 11px;
    color: var(--bad-fg);
  }
</style>
