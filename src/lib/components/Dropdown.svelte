<script module lang="ts">
  export interface DropdownOption<T extends string | number = string | number> {
    value: T;
    label: string;
    description?: string;
    disabled?: boolean;
  }
</script>

<script lang="ts" generics="T extends string | number">
  import { tick } from 'svelte';
  import { Check, ChevronDown } from 'lucide-svelte';

  let { value = $bindable(), options, label, disabled = false, onchange, placeholder = 'Choose an option', hideLabel = false }: {
    value?: T;
    options: readonly DropdownOption<T>[];
    label: string;
    disabled?: boolean;
    onchange?: (value: T) => void;
    placeholder?: string;
    hideLabel?: boolean;
  } = $props();
  const id = $props.id();
  let open = $state(false);
  let active = $state(-1);
  let geometry = $state({ left: 0, top: 0, width: 0, maxHeight: 300 });
  let trigger: HTMLButtonElement;
  let menu: HTMLDivElement | undefined;
  let typed = '';
  let typedAt = 0;
  const selected = $derived(options.find((option) => option.value === value));
  const enabled = $derived(options.map((option, index) => option.disabled ? -1 : index).filter((index) => index >= 0));
  const unavailable = $derived(disabled || enabled.length === 0);

  $effect(() => { if (unavailable) open = false; });

  function position() {
    if (!trigger || !open) return;
    const rect = trigger.getBoundingClientRect();
    const margin = 10;
    const gap = 6;
    if (rect.bottom < 0 || rect.top > window.innerHeight) { open = false; return; }
    const below = window.innerHeight - rect.bottom - gap - margin;
    const above = rect.top - gap - margin;
    const height = Math.min(menu?.scrollHeight ?? 300, 320);
    const upwards = below < height && above > below;
    const maxHeight = Math.max(40, Math.min(320, upwards ? above : below));
    const width = Math.min(Math.max(rect.width, 220), window.innerWidth - margin * 2);
    geometry = {
      left: Math.max(margin, Math.min(rect.left, window.innerWidth - width - margin)),
      top: upwards ? Math.max(margin, rect.top - gap - Math.min(height, maxHeight)) : rect.bottom + gap,
      width,
      maxHeight,
    };
  }

  // Mount above scroll panes and card boundaries without an OS-owned select menu.
  function mountMenu(node: HTMLDivElement) {
    menu = node;
    (document.getElementById('app-root') ?? document.body).appendChild(node);
    position();
    const outside = (event: PointerEvent) => {
      const target = event.target as Node;
      if (!trigger.contains(target) && !node.contains(target)) open = false;
    };
    const scroll = (event: Event) => { if (!node.contains(event.target as Node)) position(); };
    const blur = () => { open = false; };
    window.addEventListener('pointerdown', outside, true);
    window.addEventListener('scroll', scroll, true);
    window.addEventListener('resize', position);
    window.addEventListener('blur', blur);
    const observer = new ResizeObserver(position);
    observer.observe(node);
    observer.observe(trigger);
    return { destroy() {
      observer.disconnect();
      window.removeEventListener('pointerdown', outside, true);
      window.removeEventListener('scroll', scroll, true);
      window.removeEventListener('resize', position);
      window.removeEventListener('blur', blur);
      node.remove();
      menu = undefined;
    } };
  }

  async function highlight(index: number) {
    active = index;
    await tick();
    menu?.querySelector<HTMLElement>(`[data-index="${index}"]`)?.scrollIntoView({ block: 'nearest' });
  }
  function expand(index = options.findIndex((option) => option.value === value && !option.disabled)) {
    if (unavailable) return;
    // WebKit does not focus a button on every pointer click.
    trigger.focus();
    open = true;
    void highlight(index < 0 ? enabled[0] : index);
  }
  function focusTrigger(event: PointerEvent) {
    if (unavailable || event.button !== 0) return;
    // WebKit can blur a focused button on pointer press, before click runs.
    // Keep focus here so blur cannot close the menu and make click reopen it.
    event.preventDefault();
    trigger.focus({ preventScroll: true });
  }
  function commit(index: number) {
    const option = options[index];
    if (!option || option.disabled || unavailable) return;
    const changed = value !== option.value;
    value = option.value;
    open = false;
    typed = '';
    trigger.focus();
    if (changed) onchange?.(option.value);
  }
  function keydown(event: KeyboardEvent) {
    if (unavailable || event.metaKey || event.ctrlKey || event.isComposing) return;
    if (event.key === 'Tab') { open = false; return; }
    if (event.key === 'Escape') {
      if (open) { event.preventDefault(); event.stopPropagation(); open = false; }
      typed = '';
      return;
    }
    if (['Enter', ' ', 'ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) {
      event.preventDefault();
      typed = '';
      if (event.key === 'Enter' || event.key === ' ') { if (open) commit(active); else expand(); }
      else if (event.key === 'Home' || event.key === 'End') expand(event.key === 'Home' ? enabled[0] : enabled[enabled.length - 1]);
      else if (!open) expand();
      else {
        const next = Math.max(0, Math.min(enabled.length - 1, enabled.indexOf(active) + (event.key === 'ArrowDown' ? 1 : -1)));
        void highlight(enabled[next]);
      }
    } else if (event.key.length === 1 && !event.altKey) {
      event.preventDefault();
      const now = Date.now();
      typed = now - typedAt > 700 ? event.key : typed + event.key;
      typedAt = now;
      const query = [...typed].every((letter) => letter === typed[0]) ? typed[0] : typed;
      const start = query.length === 1 ? active + 1 : Math.max(0, active);
      for (let offset = 0; offset < options.length; offset++) {
        const index = (start + offset) % options.length;
        if (!options[index].disabled && options[index].label.toLocaleLowerCase().startsWith(query.toLocaleLowerCase())) { expand(index); break; }
      }
    }
  }
</script>

<div class="dropdown" class:disabled={unavailable}>
  <label class:visually-hidden={hideLabel} id={`${id}-label`} for={`${id}-trigger`}>{label}</label>
  <button bind:this={trigger} id={`${id}-trigger`} type="button" class="dropdown-trigger" class:open disabled={unavailable}
    role="combobox" aria-haspopup="listbox" aria-expanded={open} aria-controls={open ? `${id}-list` : undefined}
    aria-labelledby={`${id}-label ${id}-value`} aria-activedescendant={open && active >= 0 ? `${id}-option-${active}` : undefined}
    onpointerdown={focusTrigger}
    onclick={() => { typed = ''; if (open) open = false; else expand(); }} onkeydown={keydown} onblur={() => (open = false)}>
    <span id={`${id}-value`} class:placeholder={!selected}>{selected?.label ?? placeholder}</span><ChevronDown size={14} />
  </button>
</div>
{#if open}
  <div use:mountMenu id={`${id}-list`} role="listbox" aria-labelledby={`${id}-label`} class="dropdown-menu"
    style={`left:${geometry.left}px;top:${geometry.top}px;width:${geometry.width}px;max-height:${geometry.maxHeight}px`}>
    {#each options as option, index (option.value)}
      <div id={`${id}-option-${index}`} role="option" aria-selected={value === option.value} aria-disabled={option.disabled || undefined}
        tabindex="-1" class="dropdown-option" class:highlighted={active === index} class:chosen={value === option.value}
        data-index={index} onpointermove={() => { if (!option.disabled) active = index; }}
        onpointerdown={(event) => event.preventDefault()} onclick={() => commit(index)} onkeydown={keydown}>
        <span class="option-copy"><span>{option.label}</span>{#if option.description}<small>{option.description}</small>{/if}</span>
        <span class="option-check">{#if value === option.value}<Check size={13} />{/if}</span>
      </div>
    {/each}
  </div>
{/if}

<style>
  .visually-hidden { position: absolute; width: 1px; height: 1px; padding: 0; margin: -1px; overflow: hidden; clip-path: inset(50%); white-space: nowrap; }
  .dropdown { display: grid; gap: 7px; min-width: 0; width: 100%; }
  label { color: var(--muted); font: 11px/1.5 var(--font-mono); cursor: pointer; }
  .dropdown-trigger { display: flex; align-items: center; justify-content: space-between; gap: 12px; width: 100%; min-width: 0; min-height: 38px; padding: 9px 11px; border: 1px solid var(--node-border); border-radius: var(--radius-control); background: var(--bg); color: var(--fg); font: 12px/1.5 var(--font-mono); text-align: left; cursor: pointer; }
  .dropdown-trigger > span { overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
  .dropdown-trigger :global(svg) { flex: 0 0 auto; color: var(--muted); transition: transform 120ms ease; }
  .dropdown-trigger.open :global(svg) { transform: rotate(180deg); color: var(--accent); }
  .dropdown-trigger:hover:not(:disabled), .dropdown-trigger.open { border-color: var(--accent); }
  .dropdown-trigger:focus-visible { outline: 2px solid var(--accent); outline-offset: 3px; }
  .dropdown-trigger:disabled { opacity: 0.45; cursor: not-allowed; }
  .placeholder { color: var(--muted); }
  .dropdown-menu { position: fixed; z-index: 120; overflow-y: auto; overscroll-behavior: contain; padding: 5px; border: 1px solid var(--node-border); border-radius: var(--radius-control); background: var(--node-bg); box-shadow: 0 12px 30px #0006, 0 2px 6px #0005; color: var(--fg); font: 12px/1.5 var(--font-mono); scrollbar-width: thin; scrollbar-color: var(--node-border) transparent; }
  .dropdown-option { display: flex; align-items: center; gap: 14px; min-height: 36px; padding: 8px 10px; border: 1px solid transparent; border-radius: var(--radius-control); cursor: pointer; }
  .dropdown-option.highlighted { background: var(--surface-2); border-color: var(--violet); color: var(--violet-fg); }
  .dropdown-option.chosen .option-check { color: var(--led-ok); }
  .dropdown-option[aria-disabled='true'] { opacity: 0.4; cursor: not-allowed; }
  .option-copy { flex: 1; min-width: 0; overflow-wrap: anywhere; }
  .option-copy small { display: block; font: 11px/1.5 var(--font-body); color: var(--muted); margin-top: 3px; }
  .option-check { display: flex; width: 13px; flex: 0 0 auto; }
  @media (prefers-reduced-motion: reduce) { .dropdown-trigger :global(svg) { transition: none; } }
</style>
