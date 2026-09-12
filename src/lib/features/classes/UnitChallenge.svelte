<script lang="ts">
  /** Check out of a unit by demonstrating its entry samples. Owned by the
   *  class, independent of lessons, appointments and enforcement. */
  import { onMount } from 'svelte';
  import { api, type ClassroomSubjectId } from '../../ipc';
  import type { UnitChallengeView } from '../../contracts/challenges';
  import { assessmentEditor, type AssessmentEditorState } from '../assessments/work-editor';
  import NodeCard from '../../components/NodeCard.svelte';
  import ChoiceList from '../../components/ChoiceList.svelte';
  import Markdown from '../../components/Markdown.svelte';
  import { ArrowLeft, ArrowRight, Check, ShieldCheck } from 'lucide-svelte';
  let { courseId, unit, unitLabel, pathRevision, onclose, onapplied }: { courseId: ClassroomSubjectId; unit: string; unitLabel: string; pathRevision: number; onclose: () => void; onapplied: () => Promise<void> | void } = $props();
  let check = $state<UnitChallengeView | null>(null);
  let work = $state<AssessmentEditorState | null>(null);
  let editor = $state<ReturnType<typeof assessmentEditor>>();
  let index = $state(0);
  let busy = $state(false);
  let error = $state('');
  let alive = false;
  let unsubscribe: (() => void) | undefined;
  const question = $derived(check?.questions[index]);
  const selected = $derived(question ? work?.responses[question.id]?.answer ?? '' : '');
  const recovery = $derived(work?.status === 'conflict' || work?.status === 'error');
  const label = (verdict: string) => verdict === 'passed' ? 'Sample demonstrated' : verdict === 'needs_practice' ? 'Needs practice' : 'Skipped';
  function install(value: UnitChallengeView) {
    if (!alive) return;
    unsubscribe?.(); editor = undefined; work = null; check = value;
    if (!value.submitted) {
      const roundId = value.round_id;
      editor = assessmentEditor({ roundId, revision: value.revision, responses: value.responses }, (id, response, revision) => api.saveUnitChallengeResponse(courseId, roundId, revision, id, response));
      work = editor.snapshot();
      unsubscribe = editor.subscribe((state) => { work = state; });
      const next = value.questions.findIndex((q) => work?.responses[q.id]?.status === 'draft');
      index = next < 0 ? value.questions.length : next;
      if (work?.status === 'saving') void editor.flush();
    }
  }
  async function run(action: () => Promise<void>) {
    if (busy) return;
    busy = true; error = '';
    try { await action(); } catch (cause) { error = String(cause); }
    finally { busy = false; }
  }
  function load() {
    return run(async () => {
      const existing = await api.getUnitChallenge(courseId);
      install(existing && existing.unit === unit && existing.applied_revision === null ? existing : await api.startUnitChallenge(courseId, unit));
    });
  }
  onMount(() => { alive = true; void load(); return () => { alive = false; unsubscribe?.(); void editor?.flush(); }; });
  async function close() { if (editor) await editor.flush(); onclose(); }
  function edit(value: string) { if (question && editor) editor.edit(question.id, { answer: value, status: 'draft' }); }
  async function confirm(skip: boolean) {
    if (!question || !editor || recovery || (!skip && !selected)) return;
    const current = question;
    await run(async () => {
      editor!.edit(current.id, { answer: skip ? '' : selected, status: skip ? 'skipped' : 'answered' });
      if (await editor!.flush() && alive) index += 1;
    });
  }
  async function submit() {
    if (!check || !editor || recovery) return;
    await run(async () => {
      if (!await editor!.flush()) return;
      const saved = editor!.snapshot();
      install(await api.submitUnitChallengeRound(courseId, saved.roundId, saved.revision));
    });
  }
  async function apply() {
    if (!check) return;
    const current = check;
    await run(async () => {
      const path = await api.applyUnitChallenge(courseId, current.attempt_id, pathRevision);
      install({ ...current, applied_revision: path.revision });
      await onapplied();
    });
  }
</script>
<section class="challenge" aria-label={`${unitLabel} unit challenge`}>
  <button class="ghost mono-ghost" onclick={close} disabled={busy}><ArrowLeft size={13} /> Back to curriculum</button>
  <header><p class="eyebrow mono">UNIT CHALLENGE · {unitLabel} · route revision {pathRevision}</p><h2>Check out of {unitLabel}</h2><p>Answer this unit's entry samples from memory. Demonstrated samples let you check out of exactly those topics; nothing here awards completion, mastery or a streak.</p></header>
  {#if check}
    {#if !check.submitted}
      {#if question}
        <NodeCard Icon={ShieldCheck} name={`unit-challenge · ${index + 1}/${check.questions.length}`} badge="entry sample" badgeTone="violet">
          <p class="criterion mono">{question.label}</p><div class="prompt"><Markdown markdown={question.prompt} /></div>
          <ChoiceList options={question.choices} value={selected} label="Challenge answer choices" disabled={busy || recovery} onchange={edit} />
          <div class="actions">
            {#if index > 0}<button class="ghost mono-ghost" onclick={() => (index -= 1)} disabled={busy}>Previous</button>{/if}
            <button class="ghost mono-ghost" onclick={() => confirm(true)} disabled={busy || recovery}>Skip / I don't know</button>
            <button class="cta mono-cta" onclick={() => confirm(false)} disabled={!selected || busy || recovery}><ArrowRight size={13} /> Save & next</button>
          </div>
        </NodeCard>
      {:else}
        <NodeCard Icon={Check} name="answers-saved" badge="ready" badgeTone="teal"><p>Your answers and skips are saved. Review them or see what the samples show.</p><div class="actions"><button class="ghost mono-ghost" onclick={() => (index = 0)}>Review answers</button><button class="cta mono-cta" onclick={submit} disabled={busy || recovery}>See results</button></div></NodeCard>
      {/if}
      <p class="save-state mono" role="status">{work?.status === 'saved' ? 'Answers saved on this device' : work?.status === 'saving' ? 'Saving…' : ''}</p>
      {#if recovery}<div class="warning" role="alert"><p>{work?.error} Local work is retained.</p><div class="actions">{#if work?.status === 'error'}<button class="ghost mono-ghost" onclick={() => editor?.flush()}>Retry save</button>{/if}<button class="ghost mono-ghost" onclick={load}>Reload saved answers</button></div></div>{/if}
    {:else}
      <NodeCard Icon={ShieldCheck} name="challenge-evidence" badge={check.applied_revision !== null ? `recorded · revision ${check.applied_revision}` : 'samples only'} badgeTone="teal">
        <div class="results">{#each check.criteria as row}<details><summary><span>{row.label}</span><span class:passed={row.verdict === 'passed'} class="verdict mono">{label(row.verdict)}</span></summary>{#each row.evidence as item}<div class="evidence"><p>{item.explanation}</p><p><strong>Expected:</strong> {item.expected_answer}</p></div>{/each}</details>{/each}</div>
        {#if check.demonstrated.length}<p><strong>Demonstrated:</strong> {check.demonstrated.map((t) => t.label).join('; ')}. Checking out marks these topics as prior knowledge checked; they leave your required work without completion credit.</p>{/if}
        {#if check.needs_practice.length}<p class="scope"><strong>Stays on your route:</strong> {check.needs_practice.map((t) => t.label).join('; ')}.</p>{/if}
        <p class="scope">Topics without a sample remain not assessed; a passed sample never bypasses their prerequisites.</p>
        <div class="actions">
          {#if check.applied_revision === null && check.demonstrated.length}<button class="cta mono-cta" disabled={busy} onclick={apply}>Check out of demonstrated topics</button>{/if}
          <button class="ghost mono-ghost" disabled={busy} onclick={() => run(async () => install(await api.startUnitChallenge(courseId, unit, true)))}>Start a new challenge</button>
          <button class="ghost mono-ghost" disabled={busy} onclick={close}>Done</button>
        </div>
      </NodeCard>
    {/if}
  {:else if busy}<p role="status">Loading the unit challenge…</p>{/if}
  {#if error}<p class="warning" role="alert">{error}</p>{#if !check}<button class="ghost mono-ghost" onclick={load}>Retry loading</button>{/if}{/if}
</section>
<style>
  .challenge { width: 100%; max-width: 1000px; margin: 0 auto 24px; }
  header { margin: 16px 0; } h2 { font: 26px var(--font-display); margin: 8px 0; }
  header p, .scope, .challenge p { color: var(--muted); font-size: 13px; line-height: 1.65; }
  .eyebrow, .criterion { font-size: 10px; color: var(--violet-fg); letter-spacing: .7px; text-transform: uppercase; }
  .prompt { font-size: 16px; margin: 12px 0 18px; }
  .actions { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 10px; margin-top: 20px; }
  .save-state { color: var(--muted); font-size: 10px; }
  .warning { border-left: 2px solid var(--led-warn); background: var(--warn-bg); color: var(--warn-fg); padding: 10px 14px; font-size: 12px; line-height: 1.7; }
  .results details { border-bottom: 1px dashed var(--node-divider); padding: 13px 0; }
  summary { cursor: pointer; font-size: 13px; line-height: 1.6; }
  .verdict { display: inline-block; margin-left: 12px; color: var(--muted); font-size: 10px; }
  .passed { color: var(--teal-fg); }
  .evidence { margin: 10px 0; padding: 0 12px; border-left: 1px solid var(--node-border); color: var(--muted); font-size: 12px; }
</style>
