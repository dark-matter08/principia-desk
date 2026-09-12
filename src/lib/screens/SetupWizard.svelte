<script lang="ts">
  import { api } from '../ipc';
  import { app } from '../stores.svelte';
  import ClusterBar from '../components/ClusterBar.svelte';
  import RunnerSetup from '../features/runners/RunnerSetup.svelte';
  import SearchSetup from '../features/runners/SearchSetup.svelte';
  import RecoveryGuide from '../features/recovery/RecoveryGuide.svelte';
  import type { RecoveryStatus, SearchSettingsView } from '../ipc';
  import type { RunnerInfo } from '../contracts/agents';
  import { guessStatus } from '../features/recovery/ladder';
  import { ArrowLeft, ArrowRight, Bot, Check, Globe, KeyRound, Pencil, Rocket, ShieldCheck, TerminalSquare, X } from 'lucide-svelte';

  let model = $state('opus');
  let agent = $state('claude');
  let customBin = $state('');
  let phrase = $state('I am choosing to skip my training today and I accept the broken streak');
  let phrase2 = $state('');
  let submitting = $state(false);
  let error = $state('');
  let step = $state(0);

  const STEPS = [
    { title: 'Tutor', Icon: Bot, ahead: 'runner & model', heading: 'Choose the tutor that writes your lessons', blurb: 'Pick a runner and the model it should use. You can keep a shortlist of models per provider and change any of this later, per class.' },
    { title: 'Search', Icon: Globe, ahead: 'optional', heading: 'Give the tutor a way to look things up', blurb: 'Optional. A tutor on Ollama, OpenRouter or a bare API cannot browse; with a search engine set, the desk finds and fetches documentation itself and hands the tutor only pages it retrieved. Leave it off and lessons use the pages the curriculum names.' },
    { title: 'Recovery', Icon: KeyRound, ahead: 'phrase & way out', heading: 'Set your break-glass phrase, and learn the way out', blurb: 'An enforced session holds the machine until the lesson is done. This phrase ends one early: it is deliberately long so it cannot be typed on reflex, and using it breaks your streak. Below it, the recovery console: a key combination that works even if the desk goes blank.' },
    { title: 'Deploy', Icon: Rocket, ahead: 'review & launch', heading: 'Review and deploy', blurb: 'Everything below is what the desk starts with. Deploy writes it to your profile and opens the desk; the first thing you do there is choose a class.' },
  ];
  const RECOVERY = 2;
  const DEPLOY = 3;

  /** The search choice, read for the rail and the review; it is saved as it is made. */
  let search = $state<SearchSettingsView | null>(null);
  $effect(() => {
    if (step < 1) return;
    api.getSearchSettings().then((view) => (search = view)).catch(() => (search = null));
  });
  const SEARCH_LABELS: Record<string, string> = { none: 'Off', searxng: 'SearXNG', brave: 'Brave Search', tavily: 'Tavily' };
  const searchLine = $derived(
    !search ? '…' : `${SEARCH_LABELS[search.provider] ?? search.provider}${search.provider === 'none' ? '' : search.available ? ' · working' : ' · setup needed'}`,
  );

  /** Runner names, so the rail and the review say "Claude Code", not "claude". */
  let runners = $state<RunnerInfo[]>([]);
  $effect(() => {
    api.listAgentRunners().then((list) => (runners = list)).catch(() => (runners = []));
  });
  const tutorName = $derived(agent === 'custom' ? 'Custom CLI' : (runners.find((runner) => runner.provider === agent)?.label ?? agent));
  const tutorLine = $derived(`${tutorName} · ${model || 'runner default'}`);

  /** The recovery console as this platform names it, read for the review. */
  let recovery = $state<RecoveryStatus>(guessStatus());
  $effect(() => {
    if (step < RECOVERY) return;
    api.recoveryStatus().then((status) => (recovery = status)).catch(() => {});
  });

  /** What each step has settled on, for the rail under its name. */
  function settled(index: number): string {
    if (index > step) return STEPS[index].ahead;
    if (index === 0) return tutorLine;
    if (index === 1) return searchLine;
    if (index === RECOVERY) return phraseReady ? 'phrase confirmed' : 'phrase pending';
    return 'ready';
  }

  const phraseReady = $derived(phrase.trim().length >= 40 && phrase.trim() === phrase2.trim());
  const phraseProblem = $derived(
    phrase.trim().length < 40
      ? `escape phrase must be at least 40 characters · ${phrase.trim().length}/40`
      : phrase.trim() !== phrase2.trim()
        ? 'phrases do not match'
        : '',
  );

  function canLeave(index: number) {
    return index === RECOVERY ? phraseReady : true;
  }
  function goto(next: number) {
    if (next > step && !canLeave(step)) {
      error = phraseProblem;
      return;
    }
    error = '';
    step = Math.max(0, Math.min(STEPS.length - 1, next));
  }

  async function finish() {
    error = '';
    if (!phraseReady) {
      step = RECOVERY;
      error = phraseProblem;
      return;
    }
    submitting = true;
    try {
      if (agent === 'custom') customBin = (await api.getRunnerConfiguration(agent)).custom_command;
      await api.completeSetup(phrase.trim(), 'advisory', model, agent, customBin);
      await app.refresh();
    } catch (e) {
      error = String(e);
    } finally {
      submitting = false;
    }
  }
</script>

<div class="boot blueprint">
  <ClusterBar route="bootstrap" status="awaiting deploy" tone="warn" />
  <div class="boot-body">
    <header class="boot-head">
      <h1>Bootstrap your training cluster</h1>
      <p class="sub">Four short steps: your tutor, its search, your way out, then deploy.</p>
    </header>

    <ol class="steps" aria-label="Setup steps">
      {#each STEPS as item, index (item.title)}
        {@const Icon = item.Icon}
        <li class:done={index < step} class:current={index === step} class:ahead={index > step}>
          <button
            type="button"
            aria-current={index === step ? 'step' : undefined}
            disabled={index > step}
            onclick={() => goto(index)}
            title={index > step ? `${item.title}: after the steps before it` : item.title}
          >
            <span class="step-tile" aria-hidden="true">
              {#if index < step}<Check size={15} strokeWidth={2.4} />{:else}<Icon size={16} />{/if}
            </span>
            <span class="step-text">
              <span class="step-no mono">{index < step ? 'done' : index === step ? 'now' : `0${index + 1}`}</span>
              <span class="step-name">{item.title}</span>
              <span class="step-sub" class:pending={index === RECOVERY && index === step && !phraseReady}>{settled(index)}</span>
            </span>
          </button>
        </li>
      {/each}
    </ol>

    <section class="panel" aria-label={STEPS[step].title}>
      <h2>{STEPS[step].heading}</h2>
      <p class="sub">{STEPS[step].blurb}</p>

      {#if step === 0}
        <RunnerSetup bind:agent bind:model bind:customBin initiallyExpanded onKeyChanged={() => app.refresh()} />
      {:else if step === 1}
        <SearchSetup />
      {:else if step === RECOVERY}
        <div class="break-glass" class:ready={phraseReady}>
          <div class="bg-tag mono"><span class="bg-tag-line"></span>BREAK<br />GLASS<span class="bg-tag-line"></span></div>
          <div class="bg-fields">
            <label class="bg-label mono" for="escape-phrase">ESCAPE_PHRASE · circuit breaker · trips the streak to 0</label>
            <input id="escape-phrase" class="bg-input mono" type="text" bind:value={phrase} />
            <div class="bg-meter" role="progressbar" aria-label="Phrase length" aria-valuemin="0" aria-valuemax="40" aria-valuenow={Math.min(40, phrase.trim().length)}>
              <span class="bg-meter-fill" style={`width: ${Math.min(100, (phrase.trim().length / 40) * 100)}%`}></span>
              <span class="bg-meter-text mono">{phrase.trim().length} / 40 characters{phrase.trim().length >= 40 ? ' · long enough' : ' · keep going'}</span>
            </div>
            <input
              class="bg-input mono"
              type="text"
              aria-label="Repeat the escape phrase"
              placeholder="type it again to confirm"
              bind:value={phrase2}
            />
            <p class="bg-state mono" role="status">
              {phraseReady ? '✓ phrase confirmed' : phraseProblem}
            </p>
          </div>
        </div>
        <div class="recovery-guide">
          <div class="rg-head"><span class="meta-label">THE WAY OUT</span><h3>If the desk ever goes blank</h3></div>
          <RecoveryGuide {phrase} autoplay />
        </div>
      {:else}
        <div class="manifest">
          <ul class="manifest-grid" aria-label="What the desk starts with">
            <li class="mf-tile">
              <span class="mf-icon"><Bot size={16} /></span>
              <span class="mf-body">
                <span class="mf-label mono">TUTOR</span>
                <span class="mf-value">{tutorName}</span>
                <span class="mf-sub">{agent === 'custom' ? (customBin || 'command set in the library') : `model · ${model || 'runner default'}`}</span>
              </span>
              <span class="mf-led ok" aria-hidden="true"></span>
              <button type="button" class="mf-edit mono" onclick={() => goto(0)}><Pencil size={10} /> edit</button>
            </li>
            <li class="mf-tile">
              <span class="mf-icon"><Globe size={16} /></span>
              <span class="mf-body">
                <span class="mf-label mono">SEARCH</span>
                <span class="mf-value">{search ? (SEARCH_LABELS[search.provider] ?? search.provider) : '…'}</span>
                <span class="mf-sub">{!search || search.provider === 'none' ? 'lessons use the pages the curriculum names' : search.available ? 'working · the tutor can look things up' : 'setup needed · lessons fall back to curriculum pages'}</span>
              </span>
              <span class="mf-led" class:ok={search?.provider !== 'none' && search?.available} class:warn={search?.provider !== 'none' && search && !search.available} class:off={!search || search.provider === 'none'} aria-hidden="true"></span>
              <button type="button" class="mf-edit mono" onclick={() => goto(1)}><Pencil size={10} /> edit</button>
            </li>
            <li class="mf-tile">
              <span class="mf-icon"><KeyRound size={16} /></span>
              <span class="mf-body">
                <span class="mf-label mono">ESCAPE PHRASE</span>
                <span class="mf-value">{phrase.trim().length} characters · confirmed</span>
                <span class="mf-sub">ends an enforced session early · breaks the streak</span>
              </span>
              <span class="mf-led ok" aria-hidden="true"></span>
              <button type="button" class="mf-edit mono" onclick={() => goto(RECOVERY)}><Pencil size={10} /> edit</button>
            </li>
            <li class="mf-tile">
              <span class="mf-icon"><TerminalSquare size={16} /></span>
              <span class="mf-body">
                <span class="mf-label mono">RECOVERY CONSOLE</span>
                <span class="mf-keys" aria-label={recovery.combination.label}>
                  {#each recovery.combination.keys as key, index (index)}{#if index > 0}<i>+</i>{/if}<kbd>{key.glyph}</kbd>{/each}
                </span>
                <span class="mf-sub">{recovery.registered ? `registered with ${recovery.combination.platform} · opens above a locked desk` : `${recovery.combination.platform} has not taken the combination yet · the release token still works`}</span>
              </span>
              <span class="mf-led" class:ok={recovery.registered} class:warn={!recovery.registered} aria-hidden="true"></span>
              <button type="button" class="mf-edit mono" onclick={() => goto(RECOVERY)}><Pencil size={10} /> view</button>
            </li>
            <li class="mf-tile wide">
              <span class="mf-icon"><ShieldCheck size={16} /></span>
              <span class="mf-body">
                <span class="mf-label mono">ENFORCEMENT</span>
                <span class="mf-value">Advisory to start</span>
                <span class="mf-sub">Nothing is locked yet. Each class carries its own policy (advisory, focused or strict) in its Settings tab, and every lock keeps the ways out above.</span>
              </span>
              <span class="mf-led off" aria-hidden="true"></span>
            </li>
          </ul>

          <ol class="launch" aria-label="What deploy does">
            <li><span class="launch-no mono">1</span><span><b>Writes</b> the tutor, search, phrase and policy to your profile.</span></li>
            <li><span class="launch-no mono">2</span><span><b>Opens the desk</b> on Today, with the recovery console armed.</span></li>
            <li><span class="launch-no mono">3</span><span><b>You choose a class</b>, set its starting point and add study times. Lessons are prepared ahead of each one.</span></li>
          </ol>
        </div>
      {/if}
    </section>

    {#if error}<p class="error mono"><X size={12} /> {error}</p>{/if}

    <div class="nav-row">
      <button class="ghost mono-ghost" disabled={step === 0 || submitting} onclick={() => goto(step - 1)}>
        <ArrowLeft size={13} /> Back
      </button>
      <span class="hint mono">Step {step + 1} of {STEPS.length}</span>
      {#if step < STEPS.length - 1}
        <button class="cta mono-cta" disabled={!canLeave(step)} onclick={() => goto(step + 1)}>
          Continue <ArrowRight size={13} />
        </button>
      {:else}
        <button class="cta mono-cta" onclick={finish} disabled={submitting}>
          {#if !submitting}<Rocket size={14} />{/if}{submitting ? '… deploying' : 'deploy to prod'}
        </button>
      {/if}
    </div>
  </div>
</div>

<style>
  .boot {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow-y: auto;
    animation: fade-in 0.35s ease;
  }
  .boot-body {
    width: min(1040px, 94vw);
    margin: 0 auto;
    padding: 34px 24px 64px;
  }
  .boot-head {
    margin-bottom: 22px;
  }
  h1 {
    font-size: 28px;
    margin-bottom: 6px;
  }
  h2 {
    font-size: 17px;
    margin: 0 0 6px;
  }
  .sub {
    color: var(--muted);
    font-size: 13px;
    margin: 0;
    max-width: 56ch;
  }

  /* step rail: four tiles on one track, each naming what it settled on */
  .steps {
    display: flex;
    gap: 8px;
    list-style: none;
    margin: 0 0 22px;
    padding: 0;
  }
  .steps li {
    position: relative;
    flex: 1;
    min-width: 0;
    padding-bottom: 12px;
  }
  .steps li::after {
    /* the track: one segment per step, lit as far as the desk has come */
    content: '';
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 2px;
    border-radius: 2px;
    background: var(--node-border);
    transition: background 0.3s;
  }
  .steps li.done::after {
    background: var(--accent);
  }
  .steps li.current::after {
    background: linear-gradient(90deg, var(--accent) 0 40%, var(--node-border) 100%);
  }
  .steps button {
    position: relative;
    z-index: 1;
    display: flex;
    align-items: flex-start;
    gap: 10px;
    width: 100%;
    padding: 0 6px 0 0;
    border: 0;
    background: transparent;
    color: var(--faint);
    text-align: left;
    cursor: pointer;
  }
  .steps button:disabled {
    cursor: default;
  }
  .steps button:focus-visible .step-tile {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .step-tile {
    flex: none;
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    border: 1px solid var(--node-border);
    border-radius: 10px;
    background: var(--surface);
    color: var(--faint);
    transition: border-color 0.25s, background 0.25s, color 0.25s, box-shadow 0.25s;
  }
  .steps .current .step-tile {
    border-color: var(--accent);
    background: linear-gradient(180deg, color-mix(in srgb, var(--accent) 22%, var(--surface)), var(--surface));
    color: var(--accent);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 16%, transparent), 0 8px 22px -12px var(--accent);
  }
  .steps .done .step-tile {
    border-color: color-mix(in srgb, var(--led-ok) 55%, var(--node-border));
    background: var(--ok-bg);
    color: var(--ok-fg);
  }
  .steps .done button:hover .step-tile {
    border-color: var(--led-ok);
  }
  .step-text {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
    padding-top: 2px;
  }
  .step-no {
    font-size: 8.5px;
    letter-spacing: 1.2px;
    text-transform: uppercase;
    color: var(--faint);
  }
  .steps .current .step-no { color: var(--accent); }
  .steps .done .step-no { color: var(--ok-fg); }
  .step-name {
    font-size: 13px;
    color: var(--faint);
  }
  .steps .current .step-name { color: var(--fg); font-weight: 500; }
  .steps .done .step-name { color: var(--muted); }
  .step-sub {
    font-size: 10.5px;
    color: var(--faint);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .steps .current .step-sub { color: var(--muted); }
  .steps .done .step-sub { color: var(--muted); }
  .step-sub.pending { color: var(--warn-fg); }

  .panel {
    display: grid;
    gap: 14px;
    align-content: start;
    min-height: 260px;
  }
  .panel .sub {
    margin-bottom: 4px;
  }

  .break-glass {
    background: #1f1316;
    border: 1px dashed #793030;
    border-radius: var(--radius-panel);
    padding: 14px 16px;
    display: flex;
    gap: 16px;
    align-items: flex-start;
    transition: border-color 0.3s, background 0.3s;
  }
  .break-glass.ready { border-color: color-mix(in srgb, var(--ok-fg) 55%, #793030); background: #17191a; }
  .bg-tag {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    font-size: 10px;
    letter-spacing: 1.5px;
    color: var(--led-err);
    border: 1px solid #793030;
    border-radius: var(--radius-detail);
    padding: 8px 10px;
    text-align: center;
    line-height: 1.5;
    margin-top: 6px;
    background: repeating-linear-gradient(-45deg, transparent 0 6px, rgba(121, 48, 48, 0.18) 6px 8px);
  }
  .bg-tag-line { width: 100%; height: 1px; background: #793030; }
  .break-glass.ready .bg-tag { color: var(--ok-fg); border-color: color-mix(in srgb, var(--ok-fg) 50%, #793030); }
  .break-glass.ready .bg-tag-line { background: color-mix(in srgb, var(--ok-fg) 50%, #793030); }
  .bg-meter { position: relative; height: 18px; border-radius: 4px; background: var(--bg); overflow: hidden; border: 1px solid #3a2228; }
  .bg-meter-fill { position: absolute; inset: 0 auto 0 0; background: linear-gradient(90deg, #793030, color-mix(in srgb, var(--ok-fg) 70%, #793030)); transition: width 0.2s; }
  .bg-meter-text { position: relative; display: block; padding: 0 8px; font-size: 9.5px; line-height: 18px; color: #d8b0a8; letter-spacing: 0.4px; }
  .recovery-guide { display: flex; flex-direction: column; gap: 14px; margin-top: 18px; padding: 16px; border: 1px solid var(--node-border); border-radius: var(--radius-panel); background: var(--node-bg); }
  .rg-head h3 { margin: 4px 0 0; font-size: 15px; }
  .bg-fields {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .bg-label {
    font-size: 10px;
    color: #a05050;
    letter-spacing: 0.5px;
  }
  .bg-input {
    font-size: 12px;
    color: #d8b0a8;
    background: var(--bg);
    border: 1px solid #3a2228;
    border-radius: var(--radius-control);
    padding: 8px 12px;
  }
  .bg-input:focus {
    border-color: #793030;
  }
  .bg-state {
    font-size: 10px;
    color: #a05050;
    margin: 2px 0 0;
  }

  /* the manifest: what the desk starts with, one tile per setting */
  .manifest { display: flex; flex-direction: column; gap: 16px; }
  .manifest-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 10px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .mf-tile {
    position: relative;
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 12px 14px 12px 12px;
    border: 1px solid var(--node-border);
    border-radius: var(--radius-panel);
    background: var(--node-bg);
    transition: border-color 0.2s;
  }
  .mf-tile:hover { border-color: color-mix(in srgb, var(--accent) 40%, var(--node-border)); }
  .mf-tile.wide { grid-column: 1 / -1; }
  .mf-icon {
    flex: none;
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    border: 1px solid color-mix(in srgb, var(--accent) 35%, var(--node-border));
    border-radius: 10px;
    background: linear-gradient(180deg, color-mix(in srgb, var(--accent) 18%, var(--surface)), var(--surface));
    color: var(--accent);
  }
  .mf-body { display: flex; flex-direction: column; gap: 3px; min-width: 0; flex: 1; padding-right: 56px; }
  .mf-label { font-size: 9px; letter-spacing: 1.2px; color: var(--faint); }
  .mf-value { font-size: 13px; color: var(--fg); overflow-wrap: anywhere; }
  .mf-sub { font-size: 11px; line-height: 1.45; color: var(--muted); }
  .mf-keys { display: inline-flex; align-items: center; gap: 4px; margin: 1px 0; }
  .mf-keys kbd { display: inline-grid; place-items: center; min-width: 22px; height: 22px; padding: 0 6px; border: 1px solid color-mix(in srgb, var(--accent) 45%, var(--node-border)); border-bottom-width: 3px; border-radius: 5px; background: var(--surface); color: var(--fg); font: 500 11px/1 var(--font-mono); }
  .mf-keys i { font-style: normal; font-size: 10px; color: var(--faint); }
  .mf-led { position: absolute; top: 14px; right: 14px; width: 7px; height: 7px; border-radius: 50%; background: var(--faint); }
  .mf-led.ok { background: var(--led-ok); box-shadow: 0 0 8px var(--led-ok); }
  .mf-led.warn { background: var(--led-warn); box-shadow: 0 0 8px var(--led-warn); }
  .mf-led.off { background: var(--led-idle); }
  .mf-edit {
    position: absolute;
    right: 12px;
    bottom: 10px;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 3px 7px;
    border: 1px solid transparent;
    border-radius: var(--radius-detail);
    background: transparent;
    color: var(--faint);
    font-size: 9.5px;
    letter-spacing: 0.5px;
    cursor: pointer;
  }
  .mf-edit:hover { color: var(--accent); border-color: color-mix(in srgb, var(--accent) 40%, var(--node-border)); }

  .launch {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 12px;
    margin: 0;
    padding: 12px 14px;
    list-style: none;
    border: 1px dashed var(--node-border);
    border-radius: var(--radius-control);
  }
  .launch li { display: flex; gap: 10px; font-size: 11.5px; line-height: 1.5; color: var(--muted); }
  .launch li b { color: var(--fg); font-weight: 500; }
  .launch-no { flex: none; display: grid; place-items: center; width: 20px; height: 20px; border-radius: 50%; background: var(--surface-2); color: var(--accent); font-size: 10px; }

  .error {
    color: var(--bad-fg);
    font-size: 12px;
    margin: 14px 0 0;
  }
  .nav-row {
    margin-top: 22px;
    display: flex;
    align-items: center;
    gap: 16px;
  }
  .nav-row .cta {
    margin-left: auto;
  }
  .hint {
    font-size: 10px;
    color: var(--faint);
  }

  @media (max-width: 720px) {
    .manifest-grid { grid-template-columns: 1fr; }
    .launch { grid-template-columns: 1fr; }
  }
  @media (max-width: 560px) {
    .steps { gap: 4px; }
    .step-text { display: none; }
    .steps button { justify-content: center; padding: 0; }
  }
</style>
