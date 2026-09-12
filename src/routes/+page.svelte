<script lang="ts">
  import '../lib/theme.css';
  import { app, shouldShowEscapeHatch } from '../lib/stores.svelte';
  import { api } from '../lib/ipc';
  import SetupWizard from '../lib/screens/SetupWizard.svelte';
  import Today from '../lib/screens/Today.svelte';
  import Classes from '../lib/features/classes/Classes.svelte';
  import StudySettings from '../lib/screens/StudySettings.svelte';
  import Logs from '../lib/screens/Logs.svelte';
  import DeskShell from '../lib/app/DeskShell.svelte';
  import LanguageLesson from '../lib/screens/LanguageLesson.svelte';
  import ClassroomLesson from '../lib/screens/ClassroomLesson.svelte';
  import Dashboard from '../lib/screens/Dashboard.svelte';
  import EscapeHatch from '../lib/components/EscapeHatch.svelte';
  import DialogHost from '../lib/components/DialogHost.svelte';

  const isBlanker =
    typeof location !== 'undefined' && new URLSearchParams(location.search).has('blanker');
  const showEscapeHatch = $derived(shouldShowEscapeHatch(app.state));

  $effect(() => {
    if (!isBlanker) app.init().catch((cause) => (app.error = `The desk could not start: ${cause}`));
  });

  // A failure while the first screen renders would otherwise leave the
  // loading dots with nothing to say; the toast says what broke instead.
  function surface(message: string) {
    if (!app.error) app.error = message;
    // The desk's log keeps it too, so it can be read from a terminal.
    void api.reportFrontendError(message).catch(() => {});
  }
  $effect(() => {
    const onError = (event: ErrorEvent) => surface(`Something broke in the desk: ${event.message}`);
    const onRejection = (event: PromiseRejectionEvent) => surface(`Something broke in the desk: ${event.reason instanceof Error ? event.reason.message : String(event.reason)}`);
    window.addEventListener('error', onError);
    window.addEventListener('unhandledrejection', onRejection);
    return () => { window.removeEventListener('error', onError); window.removeEventListener('unhandledrejection', onRejection); };
  });

  // Block common quit/close shortcuts while locked.
  function onKeydown(e: KeyboardEvent) {
    if (!app.state?.debug_day && app.locked && e.metaKey && ['q', 'w', 'h', 'm'].includes(e.key.toLowerCase())) {
      e.preventDefault();
      e.stopPropagation();
    }
  }
</script>

<svelte:window onkeydown={onKeydown} oncontextmenu={(e) => !app.state?.debug_day && app.locked && e.preventDefault()} />

<div id="app-root" class="theme-noir">
  {#if isBlanker}
    <div style="flex: 1; background: #000;"></div>
  {:else if app.screen === 'loading'}
    <div class="screen"><p>…</p>{#if app.error}<p class="boot-error">{app.error}</p>{/if}</div>
  {:else if app.screen === 'setup'}
    <SetupWizard />
  {:else if app.screen === 'idle' || app.screen === 'dashboard'}
    <DeskShell>
      {#if app.screen === 'dashboard'}<Dashboard />
      {:else if app.destination === 'classes'}<Classes />
      {:else if app.destination === 'logs'}<Logs />
      {:else if app.destination === 'settings'}<StudySettings />
      {:else}<Today />{/if}
    </DeskShell>
  {:else if app.screen === 'language'}
    <LanguageLesson />
  {:else if app.screen === 'classroom'}
    <ClassroomLesson />
  {/if}

  {#if showEscapeHatch}
    <EscapeHatch />
  {/if}
  <DialogHost />

  {#if app.notice}
    <div class="notice-toast" role="status">
      <span>{app.notice.message}</span>
      {#if app.notice.action}<button class="more" onclick={() => { app.notice?.action?.run(); app.notice = null; }}>{app.notice.action.label}</button>{/if}
      <button onclick={() => (app.notice = null)} aria-label="Dismiss">×</button>
    </div>
  {/if}
  {#if app.error}
    <div class="error-toast" role="alert">
      <span>{app.error.length > 220 ? `${app.error.slice(0, 220)}…` : app.error}</span>
      {#if app.error.length > 220}<button class="more" onclick={() => { app.error = ''; app.navigate('logs'); }}>Open logs</button>{/if}
      <button onclick={() => (app.error = '')}>×</button>
    </div>
  {/if}
</div>

<style>
  .boot-error { max-width: 60ch; margin: 12px auto 0; padding: 10px 14px; border-left: 2px solid var(--led-err); background: var(--surface); color: var(--led-err); font-size: 12px; line-height: 1.5; text-align: left; overflow-wrap: anywhere; }
  .error-toast {
    position: fixed;
    bottom: 16px;
    left: 16px;
    background: var(--bad-bg);
    color: var(--bad-fg);
    border-radius: 10px;
    padding: 10px 14px;
    font-size: 13px;
    display: flex;
    gap: 12px;
    align-items: center;
    max-width: 480px;
    z-index: 60;
  }
  .error-toast button {
    background: none;
    border: none;
    color: inherit;
    font-size: 16px;
    cursor: pointer;
  }
  .notice-toast {
    position: fixed;
    bottom: 16px;
    left: 16px;
    display: flex;
    gap: 12px;
    align-items: center;
    max-width: 480px;
    padding: 10px 14px;
    border: 1px solid var(--node-border);
    border-radius: 10px;
    background: var(--surface);
    color: var(--fg);
    font-size: 13px;
    z-index: 60;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.25);
  }
  .notice-toast button {
    background: none;
    border: none;
    color: inherit;
    font-size: 16px;
    cursor: pointer;
  }
  .notice-toast .more,
  .error-toast .more {
    font: 500 11px var(--font-mono);
    letter-spacing: 0.5px;
    text-transform: uppercase;
    color: var(--accent);
    white-space: nowrap;
  }
</style>
