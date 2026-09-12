import type { LessonStage } from '../../ipc';

/**
 * Which learning stage a scrolled lesson is showing. The exercise begins the
 * practice stage and the knowledge check begins the check stage; feedback is
 * only reached by submitting the check.
 */
export function lessonStageFor(scrollTop: number, viewportHeight: number, exerciseTop: number | null, checkTop: number | null): LessonStage {
  const reach = scrollTop + viewportHeight * 0.6;
  if (checkTop !== null && reach >= checkTop) return 'check';
  if (exerciseTop !== null && reach >= exerciseTop) return 'practice';
  return 'learn';
}
