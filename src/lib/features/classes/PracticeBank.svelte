<script lang="ts">
  /**
   * Extra questions to practise with, stage by stage, written by the tutor
   * on request and kept apart from the bank the placement check and the
   * unit challenges draw on. Nothing here is graded or stored: pick an
   * answer, see the key and why, move on.
   */
  import { api, onEvent, type PracticeBankView, type BankQuestion, type ClassroomSubjectId } from '../../ipc';
  import { app } from '../../stores.svelte';
  import Dropdown from '../../components/Dropdown.svelte';
  import { confirmDialog, promptDialog } from '../../components/dialog.svelte';
  import { ListChecks, Wand2, ChevronDown, CircleCheck, CircleX, ExternalLink, Trash2, EyeOff } from 'lucide-svelte';

  let { courseId, currentPhase = null }: { courseId: ClassroomSubjectId; currentPhase?: string | null } = $props();

  let bank = $state<PracticeBankView | null>(null);
  let writing = $state(false);
  let open = $state(false);
  let stage = $state('');
  let error = $state('');
  /** The answers picked in this sitting, by question id. */
  let picked = $state<Record<string, string>>({});
  let shown = $state<string>('all');

  async function load() {
    try { bank = await api.getPracticeBank(courseId); writing = bank.working; error = ''; }
    catch (cause) { error = String(cause); }
  }
  $effect(() => { void courseId; picked = {}; void load(); });
  $effect(() => {
    let off: (() => void) | undefined;
    void onEvent('classroom:state', () => void load()).then((unlisten) => (off = unlisten));
    return () => off?.();
  });
  $effect(() => { if (!stage && bank) stage = currentPhase ?? bank.per_stage[0]?.stage ?? ''; });

  const stageOptions = $derived((bank?.per_stage ?? []).map((s) => ({ value: s.stage, label: s.label, description: `${s.count} to practise` })));
  const visible = $derived((bank?.questions ?? []).filter((q) => shown === 'all' || q.entry_point === shown));
  const labelOf = (id: string) => bank?.per_stage.find((s) => s.stage === id)?.label ?? id;
  const tried = $derived(visible.filter((q) => picked[q.id]).length);
  const right = $derived(visible.filter((q) => picked[q.id] && picked[q.id] === q.answer).length);

  async function write() {
    if (writing || !stage) return;
    writing = true; error = '';
    try {
      bank = await api.writePracticeQuestions(courseId, stage);
      open = true; shown = stage;
      app.notify(`${bank.usable} practice questions on hand for this class.`);
    } catch (cause) { error = String(cause); } finally { writing = false; }
  }
  async function dispute(question: BankQuestion) {
    const reason = await promptDialog('This key is wrong?', { label: 'What is wrong with it', placeholder: 'kept with the question' }, { message: 'The question stays listed as disputed and leaves the count.', confirm: 'Set it aside' });
    if (reason === null) return;
    try { bank = await api.voidPracticeQuestion(courseId, question.id, reason); } catch (cause) { error = String(cause); }
  }
  async function clear() {
    if (!(await confirmDialog('Drop every practice question?', 'The tutor can write new ones at any time. The bank the checks draw on is not touched.', { confirm: 'Drop them', danger: true }))) return;
    try { bank = await api.clearPracticeBank(courseId); picked = {}; } catch (cause) { error = String(cause); }
  }
</script>

{#if bank}
  <section class="practice" class:writing aria-label="Practice questions">
    <div class="strip">
      <span class="tile"><ListChecks size={15} /></span>
      <div class="copy">
        <span class="mono label">PRACTICE QUESTIONS · {bank.usable}</span>
        <span class="line">
          {#if bank.usable}{bank.per_stage.filter((s) => s.count).map((s) => `${s.label} ${s.count}`).join(' · ')}. Extra questions to try and read the answers to; nothing here is graded.
          {:else}None yet. The tutor writes eight cited questions on a stage at a time, from the class's own sources, to try and read the answers to at will.{/if}
          <span class="checks-note">The placement check and unit challenges draw on a separate bank you never see beforehand ({#if bank.checks_bank.source !== 'none'}{bank.checks_bank.source} · {bank.checks_bank.questions} questions{:else}none yet for this class{/if}).</span>
          {#if writing}<span class="working mono"><span class="beacon" aria-hidden="true"></span>the tutor is writing · the Logs page shows every line</span>{/if}
        </span>
      </div>
      <span class="keys">
        <span class="stage-pick"><Dropdown label="Stage" hideLabel value={stage} options={stageOptions} onchange={(value) => (stage = value)} /></span>
        <button type="button" class="ghost mono-ghost small" onclick={write} disabled={writing || !stage}><Wand2 size={12} />{writing ? 'Writing…' : 'Write 8 more'}</button>
        {#if bank.questions.length}<button type="button" class="ghost mono-ghost small" aria-expanded={open} onclick={() => (open = !open)}><span class="chev" class:down={open}><ChevronDown size={12} /></span>{open ? 'Hide' : 'Practise'}</button>{/if}
      </span>
    </div>
    {#if error}<p class="error" role="alert">{error}</p>{/if}

    {#if open && bank.questions.length}
      <div class="sheet">
        <div class="sheet-head">
          <span class="filter">
            <button type="button" class="chip" class:on={shown === 'all'} onclick={() => (shown = 'all')}>all {bank.usable}</button>
            {#each bank.per_stage.filter((s) => s.count) as s (s.stage)}<button type="button" class="chip" class:on={shown === s.stage} onclick={() => (shown = s.stage)}>{s.label} {s.count}</button>{/each}
          </span>
          <span class="score mono">{tried ? `${right} of ${tried} right this sitting` : 'pick an answer to see the key'}</span>
          <button type="button" class="text quiet" onclick={() => (picked = {})} disabled={!tried}>start over</button>
          <button type="button" class="text quiet" onclick={clear}><Trash2 size={11} /> drop all</button>
        </div>
        <ol class="questions">
          {#each visible as question, n (question.id)}
            {@const answered = picked[question.id]}
            <li class:voided={question.voided} class:answered>
              <div class="q-head"><span class="no mono">{String(n + 1).padStart(2, '0')}</span><span class="q-stage mono">{labelOf(question.entry_point)}</span><span class="q-label">{question.label}</span>{#if question.voided}<span class="pill mono"><EyeOff size={10} /> disputed{question.void_reason ? `: ${question.void_reason}` : ''}</span>{/if}</div>
              <p class="prompt">{question.prompt}</p>
              <div class="choices" role="group" aria-label="Choices">
                {#each question.choices as choice (choice.id)}
                  <button type="button" class="choice" class:picked={answered === choice.id} class:key={answered && choice.id === question.answer} class:miss={answered === choice.id && choice.id !== question.answer} disabled={!!answered || question.voided} onclick={() => (picked = { ...picked, [question.id]: choice.id })}>
                    <span class="choice-id mono">{choice.id}</span><span>{choice.text}</span>
                    {#if answered && choice.id === question.answer}<CircleCheck size={13} />{:else if answered === choice.id}<CircleX size={13} />{/if}
                  </button>
                {/each}
              </div>
              {#if answered}
                <div class="reveal" class:right={answered === question.answer}>
                  <strong>{answered === question.answer ? 'Right.' : `Not quite: the key is ${question.answer}.`}</strong> {question.explanation}
                  <span class="reveal-keys">
                    {#if question.source}<a class="text" href={question.source} target="_blank" rel="noreferrer"><ExternalLink size={11} /> source</a>{/if}
                    {#if !question.voided}<button type="button" class="text quiet" onclick={() => dispute(question)}>This key is wrong</button>{/if}
                  </span>
                </div>
              {/if}
            </li>
          {/each}
        </ol>
      </div>
    {/if}
  </section>
{/if}

<style>
  .practice { display: flex; flex-direction: column; gap: 10px; margin-bottom: 14px; }
  .strip { display: flex; align-items: center; gap: 14px; flex-wrap: wrap; padding: 10px 14px; border: 1px solid var(--node-border); border-radius: var(--radius-control); background: var(--surface); transition: border-color 0.2s; }
  .practice.writing .strip { border-color: color-mix(in srgb, var(--accent) 50%, var(--node-border)); }
  .tile { flex: none; display: grid; place-items: center; width: 32px; height: 32px; border-radius: 9px; background: var(--surface-2); color: var(--accent); }
  .copy { flex: 1 1 320px; min-width: 0; display: flex; flex-direction: column; gap: 3px; } .label { font-size: 9px; letter-spacing: 1.2px; color: var(--accent); } .line { font-size: 11px; color: var(--muted); line-height: 1.5; }
  .checks-note { display: block; margin-top: 2px; color: var(--faint); }
  .working { display: inline-flex; align-items: center; gap: 7px; margin-top: 4px; font-size: 10px; letter-spacing: 0.4px; color: var(--accent); }
  .beacon { position: relative; display: inline-block; width: 7px; height: 7px; border-radius: 50%; background: var(--accent); box-shadow: 0 0 8px var(--accent); }
  .beacon::after { content: ''; position: absolute; inset: -4px; border-radius: 50%; border: 1px solid var(--accent); animation: ring 1.6s ease-out infinite; }
  @keyframes ring { from { transform: scale(0.5); opacity: 0.9; } to { transform: scale(1.8); opacity: 0; } }
  .keys { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; margin-left: auto; } .stage-pick { width: 190px; }
  .chev { display: inline-grid; place-items: center; transition: transform 0.18s; } .chev.down { transform: rotate(180deg); }
  .error { margin: 0; padding: 8px 12px; border-left: 2px solid var(--led-err); background: var(--surface); color: var(--led-err); font-size: 12px; }
  .sheet { display: flex; flex-direction: column; gap: 10px; padding: 12px 14px; border: 1px solid var(--node-border); border-radius: var(--radius-panel); background: var(--node-bg); }
  .sheet-head { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; } .filter { display: flex; gap: 6px; flex-wrap: wrap; }
  .chip { padding: 4px 9px; border: 1px solid var(--node-border); border-radius: 999px; background: transparent; color: var(--muted); font: 10.5px var(--font-body); cursor: pointer; } .chip.on { border-color: var(--accent); background: color-mix(in srgb, var(--accent) 14%, var(--surface)); color: var(--fg); }
  .score { margin-left: auto; font-size: 10px; letter-spacing: 0.5px; color: var(--muted); }
  .text { display: inline-flex; align-items: center; gap: 4px; padding: 0; border: 0; background: transparent; color: var(--accent); font: 11px var(--font-body); cursor: pointer; text-decoration: none; } .text.quiet { color: var(--faint); } .text:disabled { opacity: 0.5; cursor: default; }
  .questions { display: flex; flex-direction: column; gap: 10px; margin: 0; padding: 0; list-style: none; }
  .questions li { display: flex; flex-direction: column; gap: 8px; padding: 12px 14px; border: 1px solid var(--node-border); border-radius: var(--radius-control); background: var(--surface); } .questions li.voided { opacity: 0.6; border-style: dashed; }
  .q-head { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; } .no { width: 24px; height: 24px; display: grid; place-items: center; border-radius: 6px; background: var(--surface-2); color: var(--accent); font-size: 9.5px; } .q-stage { font-size: 8.5px; letter-spacing: 0.8px; text-transform: uppercase; color: var(--faint); } .q-label { font-size: 11px; color: var(--muted); }
  .pill { display: inline-flex; align-items: center; gap: 4px; padding: 1px 7px; border-radius: 999px; background: var(--warn-bg); color: var(--warn-fg); font-size: 8.5px; letter-spacing: 0.5px; }
  .prompt { margin: 0; font-size: 13px; line-height: 1.5; }
  .choices { display: grid; grid-template-columns: repeat(auto-fill, minmax(min(100%, 260px), 1fr)); gap: 6px; }
  .choice { display: flex; align-items: center; gap: 9px; padding: 8px 10px; border: 1px solid var(--node-border); border-radius: var(--radius-control); background: var(--bg); color: var(--fg); font: 12px/1.4 var(--font-body); text-align: left; cursor: pointer; transition: border-color 0.12s; }
  .choice:hover:not(:disabled) { border-color: var(--accent); } .choice:disabled { cursor: default; }
  .choice-id { flex: none; width: 18px; height: 18px; display: grid; place-items: center; border-radius: 5px; background: var(--surface-2); color: var(--muted); font-size: 9px; }
  .choice.key { border-color: var(--led-ok); background: var(--ok-bg); color: var(--ok-fg); } .choice.key .choice-id { background: color-mix(in srgb, var(--led-ok) 25%, transparent); color: var(--ok-fg); }
  .choice.miss { border-color: var(--led-err); background: var(--bad-bg); color: var(--bad-fg); }
  .choice :global(svg:last-child) { margin-left: auto; flex: none; }
  .reveal { padding: 8px 10px; border-left: 2px solid var(--led-err); background: var(--node-bg); font-size: 12px; line-height: 1.55; color: var(--muted); } .reveal.right { border-left-color: var(--led-ok); } .reveal strong { color: var(--fg); font-weight: 500; }
  .reveal-keys { display: inline-flex; gap: 14px; margin-left: 10px; }
</style>
