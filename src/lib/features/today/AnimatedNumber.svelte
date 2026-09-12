<script lang="ts">
  /**
   * A number that counts up to its value when it first appears, and again
   * whenever the value changes. Under reduced motion it simply shows.
   */
  let { value, decimals = 0, duration = 900, prefix = '', suffix = '' }: { value: number; decimals?: number; duration?: number; prefix?: string; suffix?: string } = $props();
  let shown = $state(0);
  let frame = 0;
  const reduced = typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;
  $effect(() => {
    const target = value;
    cancelAnimationFrame(frame);
    if (reduced || !Number.isFinite(target)) { shown = target; return; }
    const from = shown;
    const started = performance.now();
    const step = (now: number) => {
      const t = Math.min(1, (now - started) / duration);
      const eased = 1 - Math.pow(1 - t, 3);
      shown = from + (target - from) * eased;
      if (t < 1) frame = requestAnimationFrame(step);
      else shown = target;
    };
    frame = requestAnimationFrame(step);
    return () => cancelAnimationFrame(frame);
  });
</script>

<span class="number mono">{prefix}{shown.toFixed(decimals)}{suffix}</span>

<style>
  .number { font-variant-numeric: tabular-nums; }
</style>
