<script lang="ts">
  /**
   * The recovery console: a terminal in its own window, above everything,
   * where four commands in order end an enforced session. It keeps its own
   * copy of the ladder text so it reads even before the desk answers.
   */
  import { onMount, tick } from 'svelte';
  import { api, isTauri } from '../../ipc';
  import { shortcutLabel } from './ladder';

  interface Line { kind: 'in' | 'out' | 'note'; text: string }
  let lines = $state<Line[]>([]);
  let input = $state('');
  let busy = $state(false);
  let history = $state<string[]>([]);
  let cursor = $state(-1);
  let field = $state<HTMLInputElement | undefined>(undefined);
  let log = $state<HTMLElement | undefined>(undefined);

  async function say(kind: Line['kind'], text: string) {
    lines = [...lines, { kind, text }];
    await tick();
    log?.scrollTo({ top: log.scrollHeight });
  }

  async function submit() {
    const line = input.trim();
    if (!line || busy) return;
    input = '';
    history = [line, ...history].slice(0, 50);
    cursor = -1;
    await say('in', line);
    busy = true;
    try {
      const reply = await api.recoveryCommand(line);
      for (const text of reply.lines) await say('out', text);
      if (reply.released) {
        setTimeout(() => void api.closeRecoveryConsole(), 1800);
      } else if (reply.close) {
        await api.closeRecoveryConsole();
      }
    } catch (error) {
      await say('note', String(error));
    } finally {
      busy = false;
      field?.focus();
    }
  }

  function key(event: KeyboardEvent) {
    if (event.key === 'Enter') { event.preventDefault(); void submit(); }
    else if (event.key === 'ArrowUp') { event.preventDefault(); if (cursor + 1 < history.length) { cursor += 1; input = history[cursor]; } }
    else if (event.key === 'ArrowDown') { event.preventDefault(); if (cursor > 0) { cursor -= 1; input = history[cursor]; } else { cursor = -1; input = ''; } }
    else if (event.key === 'Escape') { void api.closeRecoveryConsole(); }
  }

  onMount(() => {
    void (async () => {
      const label = await api.recoveryStatus().then((status) => status.combination.label).catch(() => shortcutLabel());
      await say('note', `PRINCIPIA RECOVERY CONSOLE · opened with ${label}`);
      try {
        const reply = await api.recoveryCommand('help');
        for (const text of reply.lines) await say('out', text);
      } catch (error) {
        await say('note', isTauri ? String(error) : 'Preview: the desk is not attached; commands are simulated.');
      }
      field?.focus();
    })();
  });
</script>

<div class="console" onclick={() => field?.focus()} role="presentation">
  <div class="log" bind:this={log} aria-live="polite">
    {#each lines as line, index (index)}
      <div class={`line ${line.kind}`}>{#if line.kind === 'in'}<span class="prompt">principia&gt;</span>{/if}{line.text}</div>
    {/each}
  </div>
  <div class="entry">
    <span class="prompt">principia&gt;</span>
    <input bind:this={field} bind:value={input} onkeydown={key} disabled={busy} spellcheck="false" autocomplete="off" autocapitalize="off" aria-label="Recovery command" />
  </div>
  <div class="foot">type <b>help</b> for the steps · <b>close</b> or Esc to leave · the desk is untouched until <b>release</b></div>
</div>

<style>
  .console { display: flex; flex-direction: column; height: 100vh; padding: 14px 16px 12px; font: 12px/1.7 var(--font-mono); color: var(--fg); background: var(--bg); }
  .log { flex: 1; min-height: 0; overflow-y: auto; white-space: pre-wrap; overflow-wrap: anywhere; }
  .line:empty::before { content: '\00a0'; }
  .line.in { color: var(--fg); margin-top: 6px; }
  .line.out { color: var(--muted); }
  .line.note { color: var(--accent); }
  .prompt { color: var(--accent); margin-right: 8px; }
  .entry { display: flex; align-items: center; gap: 0; margin-top: 8px; padding-top: 8px; border-top: 1px solid var(--node-border); }
  .entry input { flex: 1; min-width: 0; padding: 4px 0; border: 0; outline: none; background: transparent; color: var(--fg); font: inherit; caret-color: var(--accent); }
  .foot { margin-top: 6px; font-size: 10px; color: var(--faint); }
  .foot b { color: var(--muted); font-weight: 500; }
</style>
