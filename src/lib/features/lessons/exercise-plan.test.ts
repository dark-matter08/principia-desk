import { describe, expect, it } from 'vitest';
import { parseExercisePlan } from './exercise-plan';

describe('exercise plan', () => {
  it('reads the produce line, the steps and the done-when criteria', () => {
    const plan = parseExercisePlan(
      '**You will produce:** a file `transcript-day1.md`.\n\n### Steps\n1. Create `transcript-day1.md`.\n2. Write predictions for every row first.\n\n### Done when\n- Each row has a prediction.\n- Statuses are recorded.',
    );
    expect(plan).not.toBeNull();
    expect(plan?.produce).toBe('a file `transcript-day1.md`.');
    expect(plan?.steps).toEqual(['Create `transcript-day1.md`.', 'Write predictions for every row first.']);
    expect(plan?.doneWhen).toEqual(['Each row has a prediction.', 'Statuses are recorded.']);
    expect(plan?.rest).toBe('');
  });

  it('keeps other prose and accepts a bare numbered list as the steps', () => {
    const plan = parseExercisePlan('Some context first.\n\n1. One\n2. Two');
    expect(plan?.steps).toEqual(['One', 'Two']);
    expect(plan?.rest).toContain('Some context first.');
    expect(plan?.doneWhen).toEqual([]);
  });

  it('has no plan for prose instructions', () => {
    expect(parseExercisePlan('Build the thing, measure it, write it up.')).toBeNull();
  });
});
