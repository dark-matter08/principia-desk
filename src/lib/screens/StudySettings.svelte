<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '../ipc';
  import { app } from '../stores.svelte';
  import NodeCard from '../components/NodeCard.svelte';
  import { ShieldAlert, Bot, Globe, Archive, RefreshCw, AudioLines } from 'lucide-svelte';
  import RunnerSetup from '../features/runners/RunnerSetup.svelte';
  import RunnerFallback from '../features/runners/RunnerFallback.svelte';
  import SearchSetup from '../features/runners/SearchSetup.svelte';
  import RecoveryGuide from '../features/recovery/RecoveryGuide.svelte';
  import ProfileData from '../features/settings/ProfileData.svelte';
  import Updates from '../features/settings/Updates.svelte';
  import Voices from '../features/settings/Voices.svelte';
  let agent = $state('claude');
  let customBin = $state('');
  let model = $state('opus');
  onMount(() => { agent = app.state?.agent ?? 'claude'; customBin = app.state?.custom_agent_bin ?? ''; model = app.state?.model ?? 'opus'; });
  async function saveTutor() { await api.selectRunner(agent, model, customBin); await app.refresh(); }

  /** One tab per service, the way a class page is laid out: no scrolling past
   *  four cards to reach the fifth. The tab in view is remembered for the
   *  session so a return lands where you were. */
  type TabId = 'tutor' | 'search' | 'voices' | 'recovery' | 'data' | 'updates';
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const TABS: { id: TabId; label: string; icon: any; hint: string }[] = [
    { id: 'tutor', label: 'Tutor', icon: Bot, hint: 'runners, models, keys' },
    { id: 'search', label: 'Web search', icon: Globe, hint: 'SearXNG, Brave, Tavily' },
    { id: 'voices', label: 'Voices', icon: AudioLines, hint: 'listening mode' },
    { id: 'recovery', label: 'Recovery', icon: ShieldAlert, hint: 'the ways out' },
    { id: 'data', label: 'Your data', icon: Archive, hint: 'export, import' },
    { id: 'updates', label: 'Updates', icon: RefreshCw, hint: 'the desk itself' },
  ];
  const KEY = 'principia-settings-tab';
  const remembered = typeof sessionStorage !== 'undefined' ? (sessionStorage.getItem(KEY) as TabId | null) : null;
  const first: TabId = remembered && TABS.some((t) => t.id === remembered) ? remembered : 'tutor';
  let tab = $state<TabId>(first);
  let visited = $state<TabId[]>([first]);
  function select(next: TabId) {
    tab = next;
    if (!visited.includes(next)) visited = [...visited, next];
    try { sessionStorage.setItem(KEY, next); } catch { /* private mode */ }
  }
  function key(event: KeyboardEvent, index: number) {
    const delta = event.key === 'ArrowRight' ? 1 : event.key === 'ArrowLeft' ? -1 : 0;
    if (!delta) return;
    event.preventDefault();
    const next = TABS[(index + delta + TABS.length) % TABS.length].id;
    select(next);
    (document.getElementById(`settings-tab-${next}`) as HTMLElement | null)?.focus();
  }
</script>

<div class="settings-page page-frame">
  <header><div class="meta-label">CONFIGURATION · TUTOR · SEARCH · VOICES · RECOVERY · DATA · UPDATES</div><h1>Configure your desk</h1><p>Each service has its own tab. Class-specific tutor preferences live with the class.</p></header>

  <div class="tabs" role="tablist" aria-label="Settings sections">
    {#each TABS as item, index}
      <button role="tab" id={`settings-tab-${item.id}`} aria-selected={tab === item.id} aria-controls={`settings-panel-${item.id}`} tabindex={tab === item.id ? 0 : -1} class:selected={tab === item.id} onclick={() => select(item.id)} onkeydown={(event) => key(event, index)}>
        <item.icon size={13} />
        <span class="tab-text">{item.label}<small>{item.hint}</small></span>
        {#if item.id === 'updates' && app.update}<span class="dot" aria-label="update available"></span>{/if}
        {#if item.id === 'recovery' && app.state?.enforcement_disarmed}<span class="dot warn" aria-label="release token present"></span>{/if}
      </button>
    {/each}
  </div>

  {#each TABS as item}
    <div class="panel" id={`settings-panel-${item.id}`} role="tabpanel" aria-labelledby={`settings-tab-${item.id}`} hidden={tab !== item.id}>
      {#if visited.includes(item.id)}
        {#if item.id === 'tutor'}
          <RunnerSetup bind:agent bind:model bind:customBin onUse={saveTutor} onKeyChanged={() => app.refresh()} />
          <RunnerFallback />
        {:else if item.id === 'search'}
          <SearchSetup />
        {:else if item.id === 'voices'}
          <Voices />
        {:else if item.id === 'recovery'}
          <NodeCard Icon={ShieldAlert} name="recovery" badge={app.state?.enforcement_disarmed ? 'disarmed · release token present' : 'armed · standby'} badgeTone={app.state?.enforcement_disarmed ? 'amber' : 'red'}>
            <div class="recovery">
              <p class="recovery-lead">The way out of an enforced session, kept here so it can always be read. Every route below works without the main window, and none of them needs a terminal.</p>
              <RecoveryGuide />
            </div>
          </NodeCard>
        {:else if item.id === 'data'}
          <ProfileData />
        {:else if item.id === 'updates'}
          <Updates />
        {/if}
      {/if}
    </div>
  {/each}
</div>
<style>
  .settings-page { padding-bottom: 48px; }
  header { margin-bottom: 18px; }
  h1 { font-size: 28px; margin: 8px 0 6px; }
  p { font-size: 13px; color: var(--muted); margin: 0; }
  header p { max-width: 56ch; }
  /* The rail from the class page: a tab per service, the one in view underlined in the accent. */
  .tabs { display: flex; gap: 6px; margin-bottom: 22px; border-bottom: 1px solid var(--node-border); overflow-x: auto; }
  .tabs button { position: relative; display: flex; align-items: center; gap: 9px; flex-shrink: 0; border: 0; border-bottom: 2px solid transparent; border-radius: 0; background: none; padding: 12px 12px 12px 8px; font: 11px var(--font-mono); color: var(--muted); cursor: pointer; white-space: nowrap; }
  .tabs button:hover { color: var(--fg); }
  .tabs button.selected { color: var(--accent); border-bottom-color: var(--accent); }
  .tabs button:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
  .tab-text { display: flex; flex-direction: column; align-items: flex-start; gap: 3px; line-height: 1; }
  .tab-text small { font-size: 8.5px; letter-spacing: 0.6px; color: var(--faint); text-transform: uppercase; }
  .tabs button.selected .tab-text small { color: color-mix(in srgb, var(--accent) 70%, var(--muted)); }
  .dot { width: 6px; height: 6px; border-radius: 50%; background: var(--accent); box-shadow: 0 0 8px var(--accent); align-self: flex-start; }
  .dot.warn { background: var(--led-warn); box-shadow: 0 0 8px var(--led-warn); }
  .panel { display: flex; flex-direction: column; gap: 22px; min-width: 0; }
  .panel[hidden] { display: none; }
  .recovery { display: flex; flex-direction: column; gap: 16px; }
  .recovery-lead { max-width: 70ch; font-size: 12px; line-height: 1.55; }
  @media (max-width: 700px) { .tabs button :global(svg) { display: none; } .tab-text small { display: none; } }
</style>
