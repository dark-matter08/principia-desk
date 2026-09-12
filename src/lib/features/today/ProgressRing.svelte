<script lang="ts">
  import type { Snippet } from 'svelte';
  /**
   * A circular gauge. The arc animates to its value; a `pulse` ring breathes,
   * for the moment a class is due.
   */
  let {
    value,
    size = 180,
    stroke = 8,
    tone = 'var(--accent)',
    track = 'var(--surface-2)',
    pulse = false,
    label = '',
    children,
  }: { value: number; size?: number; stroke?: number; tone?: string; track?: string; pulse?: boolean; label?: string; children?: Snippet } = $props();
  const radius = $derived((size - stroke) / 2);
  const circumference = $derived(2 * Math.PI * radius);
  const offset = $derived(circumference * (1 - Math.min(1, Math.max(0, value))));
</script>

<div class="ring" class:pulse style:width={`${size}px`} style:height={`${size}px`} role={label ? 'img' : undefined} aria-label={label || undefined}>
  <svg viewBox={`0 0 ${size} ${size}`} width={size} height={size} aria-hidden="true">
    <circle class="track" cx={size / 2} cy={size / 2} r={radius} stroke={track} stroke-width={stroke} fill="none" />
    <circle class="arc" cx={size / 2} cy={size / 2} r={radius} stroke={tone} stroke-width={stroke} fill="none" stroke-linecap="round" stroke-dasharray={circumference} stroke-dashoffset={offset} transform={`rotate(-90 ${size / 2} ${size / 2})`} />
  </svg>
  <div class="center">{#if children}{@render children()}{/if}</div>
</div>

<style>
  .ring { position: relative; display: grid; place-items: center; flex: none; }
  .ring svg { position: absolute; inset: 0; }
  .arc { transition: stroke-dashoffset 900ms cubic-bezier(0.22, 1, 0.36, 1); filter: drop-shadow(0 0 6px color-mix(in srgb, currentColor 0%, transparent)); }
  .center { position: relative; display: flex; flex-direction: column; align-items: center; justify-content: center; text-align: center; gap: 2px; }
  .ring.pulse::before { content: ''; position: absolute; inset: -6px; border-radius: 50%; border: 2px solid var(--accent); opacity: 0; animation: ripple 1.8s ease-out infinite; }
  .ring.pulse::after { content: ''; position: absolute; inset: -6px; border-radius: 50%; border: 2px solid var(--accent); opacity: 0; animation: ripple 1.8s ease-out infinite 0.9s; }
  @keyframes ripple { 0% { transform: scale(0.92); opacity: 0.7; } 100% { transform: scale(1.18); opacity: 0; } }
  @media (prefers-reduced-motion: reduce) { .arc { transition: none; } .ring.pulse::before, .ring.pulse::after { animation: none; opacity: 0.5; } }
</style>
