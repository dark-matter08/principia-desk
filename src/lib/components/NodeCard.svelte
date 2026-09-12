<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    Icon = undefined,
    name,
    badge = '',
    badgeTone = 'muted',
    accent = 'var(--node-border)',
    children,
  }: {
    /** Lucide icon component (e.g. Clock, Lock). lucide-svelte ships
     *  Svelte-4-style classes, so the prop stays loosely typed. */
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    Icon?: any;
    name: string;
    badge?: string;
    badgeTone?: 'teal' | 'amber' | 'violet' | 'red' | 'muted';
    accent?: string;
    children: Snippet;
  } = $props();

  const tones: Record<string, { fg: string; bg: string }> = {
    teal: { fg: 'var(--ok-fg)', bg: 'var(--ok-bg)' },
    amber: { fg: 'var(--warn-fg)', bg: 'var(--warn-bg)' },
    violet: { fg: 'var(--violet-fg)', bg: 'var(--violet-bg)' },
    red: { fg: 'var(--bad-fg)', bg: 'var(--bad-bg)' },
    muted: { fg: 'var(--muted)', bg: 'var(--surface-2)' },
  };
</script>

<div class="node" style="border-color: {accent};">
  <div class="node-head">
    <span class="node-name">
      {#if Icon}<span class="node-icon"><Icon size={12} /></span>{/if}{name}
    </span>
    {#if badge}
      <span class="node-badge" style="color: {tones[badgeTone].fg}; background: {tones[badgeTone].bg};">
        {badge}
      </span>
    {/if}
  </div>
  <div class="node-body">
    {@render children()}
  </div>
</div>

<style>
  .node {
    background: var(--node-bg);
    border: 1px solid var(--node-border);
    border-radius: var(--radius-panel);
  }
  .node-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--node-divider);
  }
  .node-name {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--muted);
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .node-icon {
    display: inline-flex;
    color: var(--faint);
  }
  .node-badge {
    font-family: var(--font-mono);
    font-size: 10px;
    padding: 1px 8px;
    border-radius: var(--radius-detail);
    white-space: nowrap;
  }
  .node-body {
    padding: 14px 12px;
  }
</style>
