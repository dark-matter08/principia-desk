import { describe, expect, it } from 'vitest';
import { lessonStageFor } from './lesson-stage';

describe('lessonStageFor', () => {
  it('moves from learn to practice to check as the lesson scrolls', () => {
    expect(lessonStageFor(0, 700, 3000, 5000)).toBe('learn');
    expect(lessonStageFor(2700, 700, 3000, 5000)).toBe('practice');
    expect(lessonStageFor(4700, 700, 3000, 5000)).toBe('check');
  });
  it('stays in learn when a lesson has no exercise or check sections yet', () => {
    expect(lessonStageFor(9000, 700, null, null)).toBe('learn');
    expect(lessonStageFor(9000, 700, null, 5000)).toBe('check');
  });
});
