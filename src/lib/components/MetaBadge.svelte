<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    tone = 'muted',
    children,
  }: { tone?: 'teal' | 'amber' | 'violet' | 'red' | 'muted'; children: Snippet } = $props();

  const tones: Record<string, { fg: string; border: string }> = {
    teal: { fg: 'var(--ok-fg)', border: '#2b4a3f' },
    amber: { fg: 'var(--warn-fg)', border: '#4a3a22' },
    violet: { fg: 'var(--violet-fg)', border: '#3a3360' },
    red: { fg: 'var(--bad-fg)', border: '#4a2a2a' },
    muted: { fg: 'var(--muted)', border: 'var(--node-border)' },
  };
</script>

<span class="meta-badge" style="color: {tones[tone].fg}; border-color: {tones[tone].border};">
  {@render children()}
</span>

<style>
  .meta-badge {
    font-family: var(--font-mono);
    font-size: 11px;
    background: var(--node-bg);
    border: 1px solid;
    border-radius: var(--radius-detail);
    padding: 4px 12px;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    white-space: nowrap;
  }
</style>
