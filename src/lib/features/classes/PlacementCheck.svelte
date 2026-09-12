<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '../../ipc';
  import type { EnrollmentDraft } from '../../contracts/enrollment';
  import type { DiagnosticView } from '../../contracts/placement';
  import { assessmentEditor, type AssessmentEditorState } from '../assessments/work-editor';
  import NodeCard from '../../components/NodeCard.svelte';
  import ChoiceList from '../../components/ChoiceList.svelte';
  import Markdown from '../../components/Markdown.svelte';
  import { promptDialog } from '../../components/dialog.svelte';
  import { ArrowLeft, Compass, Check, ArrowRight } from 'lucide-svelte';
  let { draft, onclose, onrecommend, embedded = false }: { draft: EnrollmentDraft; onclose: () => void; onrecommend: () => void; embedded?: boolean } = $props();
  let check = $state<DiagnosticView | null>(null);
  let work = $state<AssessmentEditorState | null>(null);
  let editor = $state<ReturnType<typeof assessmentEditor>>();
  let index = $state(0);
  /** The briefing is shown until the learner starts, unless answers already exist. */
  let briefed = $state(false);
  let busy = $state(false);
  let error = $state('');
  let alive = false;
  let unsubscribe: (() => void) | undefined;
  const question = $derived(check?.questions[index]);
  /** A learner's own class: a written key can be disputed from its result. */
  const custom = $derived(draft.course.course_id.startsWith('custom-'));
  let disputed = $state<string[]>([]);
  async function dispute(questionId: string) {
    const reason = await promptDialog('This key is wrong?', { label: 'What is wrong with it', placeholder: 'kept with the question' }, { message: 'The question is set aside and left out of every check until the bank is written again.', confirm: 'Set it aside' });
    if (reason === null) return;
    try { await api.voidCustomQuestion(draft.course.course_id, questionId, reason); disputed = [...disputed, questionId]; }
    catch (cause) { error = String(cause); }
  }
  const selected = $derived(question ? work?.responses[question.id]?.answer ?? '' : '');
  const recovery = $derived(work?.status === 'conflict' || work?.status === 'error');
  const demonstrated = $derived(check?.criteria.filter((row) => row.verdict === 'passed').length ?? 0);
  const label = (verdict: string) => verdict === 'passed' ? 'Sample demonstrated' : verdict === 'needs_practice' ? 'Needs practice' : 'Unassessed';
  function install(value: DiagnosticView) {
    if (!alive) return;
    unsubscribe?.(); editor = undefined; work = null; check = value;
    if (!value.submitted) {
      const roundId = value.round_id;
      editor = assessmentEditor({ roundId, revision: value.revision, responses: value.responses }, (id, response, revision) => api.savePlacementResponse(draft.id, roundId, revision, id, response));
      work = editor.snapshot();
      unsubscribe = editor.subscribe((state) => { work = state; });
      const next = value.questions.findIndex((q) => work?.responses[q.id]?.status === 'draft');
      index = next < 0 ? value.questions.length : next;
      briefed = briefed || value.ordinal > 1 || Object.values(work?.responses ?? {}).some((r) => r.status !== 'draft' || r.answer);
      if (work?.status === 'saving') void editor.flush();
    }
  }
  async function run(action: () => Promise<void>) {
    if (busy) return;
    busy = true; error = '';
    try { await action(); } catch (cause) { error = String(cause); }
    finally { busy = false; }
  }
  function load() { return run(async () => install(await api.getPlacementCheck(draft.id) ?? await api.startPlacementCheck(draft.id, draft.revision))); }
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
      install(await api.submitPlacementRound(draft.id, saved.roundId, saved.revision));
    });
  }
  async function recover(keepLocal: boolean) {
    await run(async () => {
      const saved = await api.getPlacementCheck(draft.id);
      if (!saved || saved.submitted) throw new Error('This check is already submitted or changed. Return to setup and reopen it; local answers remain retained.');
      await editor?.resolve({ roundId: saved.round_id, revision: saved.revision, responses: saved.responses }, keepLocal);
    });
  }
  async function finish() {
    if (!check) return;
    await run(async () => { const saved = await api.finishPlacementCheck(draft.id, check!.round_id); install(saved); if (alive && saved.matches_draft) onrecommend(); });
  }
</script>
<section class="check" class:embedded aria-label="Starting-point check">
  <button class="ghost mono-ghost" onclick={close}><ArrowLeft size={13} /> Back to setup</button>
  <header><p class="eyebrow mono">PLACEMENT · {draft.course.course_id}</p><h2>Find your starting point</h2><p>{briefed ? 'Take your time. Skip anything unfamiliar; your answers are saved as you go.' : 'Before you begin, here is exactly what the check covers and how it is used.'}</p></header>
  {#if check && !check.submitted && !briefed}
    <NodeCard Icon={Compass} name="what-to-expect" badge={`${check.questions.length} questions · ~${check.estimated_minutes} min`} badgeTone="violet">
      <p class="scope">{check.scope_note}</p>
      <ol class="stages" aria-label="Stages the check samples">
        {#each check.stages as stage, n (stage.id)}
          <li><span class="stage-name"><b class="mono">{n + 1}</b>{stage.label}<i class="mono">{stage.questions} questions</i></span><span class="stage-samples">{stage.samples.join(' · ')}</span></li>
        {/each}
      </ol>
      <dl class="rules">
        <div><dt>How it places you</dt><dd>Lessons start at the first stage you do not fully demonstrate. Passing every question in a stage is what lets you skip it.</dd></div>
        <div><dt>Skipping</dt><dd>Skip anything unfamiliar. A skip counts as not demonstrated, never as wrong, and it is not shown to anyone.</dd></div>
        <div><dt>Saving</dt><dd>Every answer saves as you go. You can close the app and pick up where you left off.</dd></div>
        <div><dt>What it is not</dt><dd>Not a grade and not course credit. It only decides where lessons begin, and you can move that point yourself afterwards.</dd></div>
      </dl>
      <div class="actions"><button class="cta mono-cta" onclick={() => (briefed = true)}><ArrowRight size={13} /> Start the check</button></div>
    </NodeCard>
  {:else if check}
    <p class="scope">{check.scope_note} About {check.estimated_minutes} minutes for the initial check.</p>
    {#if !check.matches_draft}<p class="warning" role="status">Your setup changed after this check began. These answers retain their original context. Return to setup to choose another route, or finish this check before starting a new one.</p>{/if}
    {#if !check.submitted}
      {#if question}
        <NodeCard Icon={Compass} name={`placement-check · ${index + 1}/${check.questions.length}`} badge={check.ordinal > 1 ? 'follow-up' : 'entry sample'} badgeTone="violet">
          <p class="criterion mono">{question.label}</p><div class="prompt"><Markdown markdown={question.prompt} /></div>
          <ChoiceList options={question.choices} value={selected} label="Diagnostic answer choices" disabled={busy || recovery} onchange={edit} />
          <div class="actions">
            {#if index > 0}<button class="ghost mono-ghost" onclick={() => (index -= 1)} disabled={busy}>Previous</button>{/if}
            <button class="ghost mono-ghost" onclick={() => confirm(true)} disabled={busy || recovery}>Skip / I don't know</button>
            <button class="cta mono-cta" onclick={() => confirm(false)} disabled={!selected || busy || recovery}><ArrowRight size={13} /> Save & next</button>
          </div>
        </NodeCard>
      {:else}
        <NodeCard Icon={Check} name="answers-saved" badge="ready" badgeTone="teal"><p>Your answers and skips are saved. Review them or see what the samples suggest.</p><div class="actions"><button class="ghost mono-ghost" onclick={() => (index = 0)}>Review answers</button><button class="cta mono-cta" onclick={submit} disabled={busy || recovery}>See results</button></div></NodeCard>
      {/if}
      <p class="save-state mono" role="status">{work?.status === 'saved' ? 'Answers saved on this device' : work?.status === 'saving' ? 'Saving…' : ''}</p>
      {#if recovery}<div class="warning" role="alert"><p>{work?.error} Local work is retained.</p><div class="actions">
        {#if work?.status === 'error'}<button class="ghost mono-ghost" onclick={() => editor?.flush()}>Retry save</button>{/if}
        <button class="ghost mono-ghost" onclick={() => recover(false)}>Use saved answers</button>
        {#if editor?.hasLocalChanges()}<button class="ghost mono-ghost" onclick={() => recover(true)}>Keep local answers</button>{/if}
      </div></div>{/if}
    {:else}
      <NodeCard Icon={Compass} name="entry-evidence" badge="samples only" badgeTone="teal">
        <div class="score">
          <p class="tally"><strong>{demonstrated}</strong> <span>of {check.criteria.length} sampled criteria demonstrated</span></p>
          <div class="meter" role="img" aria-label={`${demonstrated} of ${check.criteria.length} sampled criteria demonstrated`}><i style:width={`${check.criteria.length ? (demonstrated / check.criteria.length) * 100 : 0}%`}></i></div>
          <p class="reading">{demonstrated === check.criteria.length
            ? 'Every sample held up, so the suggested route starts after the material they cover.'
            : demonstrated === 0
              ? 'No sample held up, so the suggested route starts from the foundations.'
              : `The suggested route starts after the last pair that held up, and the ${check.criteria.length - demonstrated} that did not become refreshers.`}</p>
        </div>
        {#each [{ key: 'passed', title: 'Demonstrated' }, { key: 'needs_practice', title: 'Needs practice' }, { key: 'unknown', title: 'Skipped' }] as group (group.key)}
          {@const rows = check.criteria.filter((row) => row.verdict === group.key)}
          {#if rows.length}
            <h4 class="group mono">{group.title} · {rows.length}</h4>
            <div class="results">{#each rows as row (row.id)}<details><summary><span>{row.label}</span><span class:passed={row.verdict === 'passed'} class="verdict mono">{label(row.verdict)}</span></summary>{#each row.evidence as item, i}<div class="evidence"><p class="mono">{i === 0 ? 'Initial sample' : 'Follow-up'} · {label(item.verdict)}</p><p>{item.explanation}</p><p><strong>Expected:</strong> {item.expected_answer}</p>{#if custom}{#if disputed.includes(item.question_id)}<p class="disputed mono">key disputed · set aside until the bank is corrected</p>{:else}<button type="button" class="dispute" onclick={() => dispute(item.question_id)}>This key is wrong</button>{/if}{/if}</div>{/each}</details>{/each}</div>
          {/if}
        {/each}
        <p class="scope">Every stage of this course was sampled, three skills each. What you did not demonstrate becomes a refresher or the place your lessons start; nothing is left unmeasured.</p>
        <div class="actions">
          {#if check.can_follow_up}<button class="ghost mono-ghost" disabled={busy} onclick={() => run(async () => install(await api.continuePlacementCheck(draft.id, check!.round_id)))}>Try prerequisite follow-up · up to 2 questions</button>{/if}
          {#if !check.completed}<button class="cta mono-cta" disabled={busy} onclick={finish}>Use these results</button>
          {:else if check.matches_draft}<button class="cta mono-cta" onclick={onrecommend}>Review suggested path</button>{/if}
          {#if check.completed}<button class="ghost mono-ghost" disabled={busy} onclick={() => run(async () => install(await api.startPlacementCheck(draft.id, draft.revision, true)))}>Start a new check</button>{/if}
        </div>
      </NodeCard>
    {/if}
  {:else if busy}<p role="status">Loading your saved check…</p>{/if}
  {#if error}<p class="warning" role="alert">{error}</p>{#if !check}<button class="ghost mono-ghost" onclick={load}>Retry loading</button>{/if}{/if}
</section>
<style>
  .check { width: min(820px, 100%); padding: 26px 28px 48px; margin: auto; }
  header { margin: 24px 0 16px; } h2 { font: 30px var(--font-display); margin: 8px 0; }
  header p, .scope { color: var(--muted); font-size: 13px; line-height: 1.65; }
  .eyebrow, .criterion { font-size: 10px; color: var(--violet-fg); letter-spacing: .7px; text-transform: uppercase; }
  .prompt { font-size: 16px; margin: 12px 0 18px; }
  .actions { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 10px; margin-top: 20px; }
  .save-state { color: var(--muted); font-size: 10px; }
  .warning { border-left: 2px solid var(--led-warn); background: var(--warn-bg); color: var(--warn-fg); padding: 10px 14px; font-size: 12px; line-height: 1.7; }
  .score { border: 1px solid var(--accent); border-radius: var(--radius-panel); padding: 13px 15px; margin-bottom: 16px; display: grid; gap: 8px; }
  .tally strong { font: 30px var(--font-display); color: var(--fg); } .tally span { color: var(--muted); font-size: 13px; }
  .tally { margin: 0; display: flex; align-items: baseline; gap: 8px; }
  .meter { height: 5px; border-radius: 3px; background: var(--bg); overflow: hidden; } .meter i { display: block; height: 100%; background: var(--accent); }
  .reading { margin: 0; font-size: 13px; line-height: 1.6; }
  .group { font-size: 10px; letter-spacing: .7px; color: var(--faint); margin: 16px 0 2px; text-transform: uppercase; }
  .stages { list-style: none; margin: 14px 0 0; padding: 0; display: grid; gap: 1px; border: 1px solid var(--node-border); border-radius: var(--radius-panel); overflow: hidden; background: var(--node-border); }
  .stages li { display: grid; gap: 4px; padding: 10px 13px; background: var(--surface); }
  .stage-name { display: flex; align-items: center; gap: 10px; font-size: 13px; }
  .stage-name b { display: grid; place-items: center; width: 20px; height: 20px; border: 1px solid var(--node-border); border-radius: var(--radius-detail); font-size: 10px; color: var(--violet-fg); }
  .stage-name i { margin-left: auto; font-style: normal; font-size: 9px; color: var(--faint); letter-spacing: .5px; }
  .stage-samples { font-size: 11px; line-height: 1.55; color: var(--muted); }
  .rules { display: grid; gap: 10px; margin: 16px 0 0; }
  .rules div { display: grid; grid-template-columns: 130px 1fr; gap: 12px; }
  .rules dt { font-size: 10px; letter-spacing: .6px; text-transform: uppercase; color: var(--faint); padding-top: 2px; }
  .rules dd { margin: 0; font-size: 12px; line-height: 1.6; }
  @media(max-width:620px) { .rules div { grid-template-columns: 1fr; gap: 3px; } }
  .results details { border-bottom: 1px dashed var(--node-divider); padding: 13px 0; }
  summary { cursor: pointer; font-size: 13px; line-height: 1.6; }
  .verdict { display: inline-block; margin-left: 12px; color: var(--muted); font-size: 10px; }
  .passed { color: var(--teal-fg); }
  .evidence { margin: 10px 0; padding: 0 12px; border-left: 1px solid var(--node-border); color: var(--muted); font-size: 12px; }
  .dispute { padding: 2px 0; border: 0; background: transparent; color: var(--warn-fg); font: 11px var(--font-body); cursor: pointer; text-decoration: underline dotted; }
  .disputed { font-size: 9.5px; letter-spacing: 0.5px; color: var(--warn-fg); }
  .evidence .mono { font-size: 10px; color: var(--fg); }
  @media(max-width:620px) { .check { padding: 20px 16px; } .actions { justify-content: flex-start; } }
  .check.embedded { width: 100%; max-width: 1000px; padding: 0; }
</style>
