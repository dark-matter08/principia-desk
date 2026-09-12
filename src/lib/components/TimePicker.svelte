<script lang="ts">
  /**
   * Keyboard-accessible hour and minute segments with direct-digit entry.
   */
  let {
    value = $bindable('19:00'),
    cron = true,
    compact = false,
    label = 'Time picker',
  }: { value?: string; cron?: boolean; compact?: boolean; label?: string } = $props();

  const hour = $derived(Number(value.split(':')[0]) || 0);
  const minute = $derived(Number(value.split(':')[1]) || 0);

  // Which segment is focused, and a per-segment typed-digit buffer so the
  // user can type "1" then "9" to get 19 without the first keystroke jumping.
  let focused = $state<'h' | 'm' | null>(null);
  let buffer = '';

  function commit(h: number, m: number) {
    value = `${String(h).padStart(2, '0')}:${String(m).padStart(2, '0')}`;
  }

  function step(seg: 'h' | 'm', delta: number) {
    buffer = '';
    if (seg === 'h') commit((hour + delta + 24) % 24, minute);
    else commit(hour, (minute + delta + 60) % 60);
  }

  function onKey(seg: 'h' | 'm', e: KeyboardEvent) {
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      step(seg, 1);
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      step(seg, -1);
    } else if (/^[0-9]$/.test(e.key)) {
      e.preventDefault();
      buffer = (buffer + e.key).slice(-2);
      const n = Number(buffer);
      const max = seg === 'h' ? 23 : 59;
      const v = n > max ? Number(e.key) : n;
      if (n > max) buffer = e.key;
      if (seg === 'h') commit(v, minute);
      else commit(hour, v);
      // Two digits (or an unambiguous first digit) → hop to minutes.
      if (seg === 'h' && (buffer.length === 2 || v > 2)) {
        buffer = '';
        const el = e.currentTarget as HTMLElement;
        (el.parentElement?.parentElement?.querySelector('[data-seg="m"]') as HTMLElement)?.focus();
      }
    }
  }

  function onWheel(seg: 'h' | 'm', e: WheelEvent) {
    e.preventDefault();
    step(seg, e.deltaY < 0 ? 1 : -1);
  }
</script>

<div class="tp mono" class:compact role="group" aria-label={label}>
  {#each [{ seg: 'h' as const, val: hour }, { seg: 'm' as const, val: minute }] as s, i}
    {#if i === 1}<div class="colon" class:lit={focused !== null}>:</div>{/if}
    <div class="seg-col">
      <button type="button" class="chev" tabindex="-1" aria-label={`Increase ${s.seg === 'h' ? 'hour' : 'minute'}`} onclick={() => step(s.seg, 1)}>▴</button>
      <div
        class="seg"
        class:focused={focused === s.seg}
        data-seg={s.seg}
        role="spinbutton"
        aria-label={s.seg === 'h' ? 'Hour' : 'Minute'}
        aria-valuenow={s.val}
        aria-valuemin="0"
        aria-valuemax={s.seg === 'h' ? 23 : 59}
        tabindex="0"
        onfocus={() => { focused = s.seg; buffer = ''; }}
        onblur={() => (focused = focused === s.seg ? null : focused)}
        onkeydown={(e) => onKey(s.seg, e)}
        onwheel={(e) => onWheel(s.seg, e)}
      >
        {String(s.val).padStart(2, '0')}
      </div>
      <button type="button" class="chev" tabindex="-1" aria-label={`Decrease ${s.seg === 'h' ? 'hour' : 'minute'}`} onclick={() => step(s.seg, -1)}>▾</button>
    </div>
  {/each}
  {#if cron}
    <div class="cron-read">
      <span class="cron-label">cron</span>
      <span class="cron-expr">{minute} {hour} * * *</span>
    </div>
  {/if}
</div>

<style>
  .tp {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: var(--bg);
    border: 1px solid var(--node-border);
    border-radius: var(--radius-panel);
    padding: 6px 10px;
  }
  .tp.compact { gap: 3px; padding: 3px 6px; }
  .compact .seg { font-size: 17px; padding: 2px 4px; }
  .compact .colon { font-size: 16px; }
  .compact .chev { padding: 1px 4px; }
  .seg-col {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 1px;
  }
  .seg {
    font-size: 24px;
    line-height: 1.2;
    color: var(--fg);
    background: var(--surface);
    border: 1px solid transparent;
    border-radius: var(--radius-control);
    padding: 2px 9px;
    cursor: ns-resize;
    outline: none;
    transition: border-color 0.15s ease, background 0.15s ease;
  }
  .seg.focused {
    border-color: var(--accent);
    background: var(--surface-2);
    color: var(--accent);
  }
  .colon {
    font-size: 22px;
    color: var(--faint);
    padding-bottom: 1px;
    transition: color 0.15s ease;
  }
  .colon.lit {
    color: var(--accent);
    animation: led-pulse 1.2s ease infinite;
  }
  .chev {
    background: none;
    border: none;
    color: var(--faint);
    font-size: 9px;
    line-height: 1;
    padding: 2px 8px;
    cursor: pointer;
  }
  .chev:hover {
    color: var(--accent);
  }
  .cron-read {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    margin-left: 8px;
    padding-left: 12px;
    border-left: 1px dashed var(--node-divider);
  }
  .cron-label {
    font-size: 9px;
    letter-spacing: 1px;
    text-transform: uppercase;
    color: var(--faint);
  }
  .cron-expr {
    font-size: 12px;
    color: var(--led-ok);
    white-space: nowrap;
  }
</style>
