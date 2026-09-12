/**
 * The structure inside an exercise's instructions.
 *
 * The tutor writes them as Markdown in a fixed shape: a `**You will
 * produce:**` line, a `### Steps` numbered list and a `### Done when`
 * bulleted list. Parsed, they become a stepper the learner can tick through;
 * an older exercise written as prose has no plan and renders as it is.
 */
import { marked, type Tokens } from 'marked';

export interface ExercisePlan {
  /** The one-line statement of the artifact, Markdown, without its label. */
  produce: string;
  /** Each step's Markdown, in order. */
  steps: string[];
  /** Each acceptance criterion's Markdown, in order. */
  doneWhen: string[];
  /** Anything else in the instructions, Markdown, kept above the stepper. */
  rest: string;
}

/** Parse the plan out of instructions, or null when they are not shaped that way. */
export function parseExercisePlan(instructions: string): ExercisePlan | null {
  const tokens = marked.lexer(instructions);
  let produce = '';
  const steps: string[] = [];
  const doneWhen: string[] = [];
  const rest: string[] = [];
  let heading: 'steps' | 'done' | null = null;
  for (const token of tokens) {
    if (token.type === 'heading') {
      const text = (token as Tokens.Heading).text.trim().toLowerCase();
      heading = text.startsWith('step') ? 'steps' : text.startsWith('done') ? 'done' : null;
      if (!heading) rest.push(token.raw);
      continue;
    }
    if (token.type === 'list') {
      const items = (token as Tokens.List).items.map((item) => item.text.trim());
      if (heading === 'steps' || (heading === null && (token as Tokens.List).ordered && steps.length === 0)) steps.push(...items);
      else if (heading === 'done') doneWhen.push(...items);
      else rest.push(token.raw);
      continue;
    }
    if (token.type === 'paragraph') {
      const text = (token as Tokens.Paragraph).text.trim();
      const match = text.match(/^\*\*You will produce:?\*\*:?\s*(.+)$/is);
      if (match && !produce) {
        produce = match[1].trim();
        continue;
      }
    }
    if (token.type !== 'space') rest.push(token.raw);
  }
  if (steps.length === 0) return null;
  return { produce, steps, doneWhen, rest: rest.join('\n').trim() };
}
