<script lang="ts">
  /** Frozen multiple-choice questions with saved answers and corrections.
   *  Subjects add their own production fields and submit action around it. */
  import type { Snippet } from 'svelte';

  import type { CheckCorrection, CheckQuestion } from './types';
  import Markdown from '../../components/Markdown.svelte';

  let {
    name,
    questions,
    answers,
    corrections = null,
    disabled = false,
    onchoose,
    children,
  }: {
    name: string;
    questions: CheckQuestion[];
    answers: number[];
    corrections?: CheckCorrection[] | null;
    disabled?: boolean;
    onchoose: (index: number, choiceIndex: number) => void;
    children?: Snippet;
  } = $props();
</script>

<div class="question-list">
  {#each questions as question, index (question.id)}
    {@const correction = corrections?.find((item) => item.question_id === question.id)}
    <fieldset class:incorrect={correction && !correction.correct} class:correct={correction?.correct}>
      <legend>
        <span class="question-number mono">{String(index + 1).padStart(2, '0')}</span>
        <Markdown markdown={question.prompt} inline />
      </legend>
      {#if question.meta}<p class="meta mono"><Markdown markdown={question.meta} inline /></p>{/if}
      <div class="choice-list">
        {#each question.choices as choice, choiceIndex}
          <label
            class:selected={answers[index] === choiceIndex}
            class:right={!!correction && choice === correction.correct_answer}
            class:wrong={!!correction && answers[index] === choiceIndex && !correction.correct}
          >
            <input
              type="radio"
              name={`${name}-${question.id}`}
              value={choiceIndex}
              checked={answers[index] === choiceIndex}
              {disabled}
              onchange={() => onchoose(index, choiceIndex)}
            />
            <span class="choice-key mono">{String.fromCharCode(65 + choiceIndex)}</span>
            <span><Markdown markdown={choice} inline /></span>
          </label>
        {/each}
      </div>
      {#if correction}
        <div class="correction" class:good={correction.correct} role="status">
          <strong>{#if correction.correct}Correct{:else}Correct answer: <Markdown markdown={correction.correct_answer} inline />{/if}</strong>
          <p><Markdown markdown={correction.explanation} inline /></p>
        </div>
      {/if}
    </fieldset>
  {/each}
  {#if children}{@render children()}{/if}
</div>

<style>
  .question-list { display: grid; gap: 10px; margin-top: 16px; }
  fieldset { border: 1px solid var(--node-border); border-radius: var(--radius-control); padding: 11px; min-width: 0; }
  fieldset.correct { border-color: var(--green); }
  fieldset.incorrect { border-color: var(--red); }
  legend { padding: 0 5px; font-size: 11px; line-height: 1.45; }
  .question-number { color: var(--accent); margin-right: 6px; font-size: 8px; }
  .meta { color: var(--faint); font-size: 8px; margin: 2px 0 9px; text-transform: lowercase; }
  .choice-list { display: grid; gap: 5px; }
  .choice-list label {
    display: flex; align-items: flex-start; gap: 8px; color: var(--muted); font-size: 10px; line-height: 1.4;
    cursor: pointer; padding: 5px; border: 1px solid transparent; border-radius: var(--radius-detail);
  }
  .choice-list label:hover { background: var(--surface-2); }
  .choice-list label.selected { border-color: var(--node-border); color: var(--text); }
  .choice-list label.right { border-color: var(--green); }
  .choice-list label.wrong { border-color: var(--red); }
  .choice-list input { margin-top: 2px; accent-color: var(--accent); }
  .choice-key { color: var(--faint); font-size: 8px; margin-top: 3px; }
  .correction { margin-top: 9px; border-top: 1px solid var(--node-border); padding-top: 8px; font-size: 10px; }
  .correction.good strong { color: var(--green); }
  .correction p { margin: 4px 0 0; color: var(--muted); line-height: 1.5; }
  input:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
</style>
