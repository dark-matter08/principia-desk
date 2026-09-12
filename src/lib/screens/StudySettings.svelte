<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '../ipc';
  import { app } from '../stores.svelte';
  import NodeCard from '../components/NodeCard.svelte';
  import FlowStage from '../components/FlowStage.svelte';
  import { ShieldAlert } from 'lucide-svelte';
  import RunnerSetup from '../features/runners/RunnerSetup.svelte';
  import RunnerFallback from '../features/runners/RunnerFallback.svelte';
  import SearchSetup from '../features/runners/SearchSetup.svelte';
  import RecoveryGuide from '../features/recovery/RecoveryGuide.svelte';
  let agent = $state('claude');
  let customBin = $state('');
  let model = $state('opus');
  onMount(() => { agent = app.state?.agent ?? 'claude'; customBin = app.state?.custom_agent_bin ?? ''; model = app.state?.model ?? 'opus'; });
  async function saveTutor() { await api.selectRunner(agent, model, customBin); await app.refresh(); }

</script>

<div class="settings-page page-frame">
  <header><div class="meta-label">CONFIGURATION · TUTOR · SEARCH · RECOVERY</div><h1>Configure your desk</h1><p>Each service has its own controls. Class-specific tutor preferences live with the class.</p></header>
  <FlowStage number="01"><section aria-label="Tutor configuration">
    <RunnerSetup bind:agent bind:model bind:customBin onUse={saveTutor} onKeyChanged={() => app.refresh()} />
    <RunnerFallback />
  </section></FlowStage>
  <FlowStage number="02"><section aria-label="Web search"><SearchSetup /></section></FlowStage>
  <FlowStage number="03" last><section aria-label="Recovery"><NodeCard Icon={ShieldAlert} name="recovery" badge={app.state?.enforcement_disarmed ? 'disarmed · release token present' : 'armed · standby'} badgeTone={app.state?.enforcement_disarmed ? 'amber' : 'red'}>
    <div class="recovery">
      <p class="recovery-lead">The way out of an enforced session, kept here so it can always be read. Every route below works without the main window, and none of them needs a terminal.</p>
      <RecoveryGuide />
    </div>
  </NodeCard></section></FlowStage>
</div>
<style>
  .settings-page { padding-bottom: 48px; }
  header { margin-bottom: 26px; }
  h1 { font-size: 28px; margin: 8px 0 6px; }
  p { font-size: 13px; color: var(--muted); margin: 0; }
  header p { max-width: 56ch; }
  .recovery { display: flex; flex-direction: column; gap: 16px; }
  .recovery-lead { max-width: 70ch; font-size: 12px; line-height: 1.55; }
</style>
