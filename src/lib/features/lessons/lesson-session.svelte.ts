import { api, type LessonCheckView, type LessonCheckpoint, type LessonReadingPosition, type LessonStage } from '../../ipc';
import type { AssessmentRoundId } from '../../contracts/assessments';
import { lessonStageFor } from '../classes/lesson-stage';

export type SaveState = 'saved' | 'saving' | 'error';

/** The part of any lesson view the shared session behavior relies on. */
export interface LessonLike {
  session_id: string;
  runtime: 'legacy' | 'study';
  checkpoint: LessonCheckpoint | null;
  check: LessonCheckView | null;
  questions: { id: number }[];
}

/** Section anchors that mark where the practice and check stages begin. */
export interface StageAnchors {
  practice: HTMLElement | null | undefined;
  check: HTMLElement | null | undefined;
}

/** Saved multiple-choice answers from the frozen check round, `-1` when unanswered. */
export function restoreAnswers(lesson: LessonLike): number[] {
  const restored: number[] = Array(lesson.questions.length).fill(-1);
  if (!lesson.check) return restored;
  for (const [index, question] of lesson.questions.entries()) {
    const saved = lesson.check.responses[String(question.id)];
    const choice = saved?.status === 'answered' ? Number(saved.answer) : NaN;
    if (Number.isInteger(choice) && choice >= 0) restored[index] = choice;
  }
  return restored;
}

/** Copy of the work saved with the lesson checkpoint, or nothing. */
export function restoredWork(lesson: LessonLike): Record<string, unknown> {
  return lesson.checkpoint?.body.work ?? {};
}

/**
 * Same save, recovery and navigation behavior for every class lesson.
 *
 * Shared-runtime lessons persist each answer against their frozen check round
 * (revisions carried forward through one serialized queue), merge production
 * work into the checkpoint, and record the reading position and stage after a
 * short pause. Legacy lessons keep everything in memory until submission.
 */
export class LessonSession {
  readonly sessionId: string;
  readonly study: boolean;
  answers = $state<number[]>([]);
  answerStatus = $state<SaveState>('saved');
  answerError = $state('');
  /** Stage the learner is currently on, as last computed from the scroll position. */
  stage = $state<LessonStage>('learn');
  private roundId: AssessmentRoundId | null;
  private checkRevision: number;
  private queue: Promise<void> = Promise.resolve();
  private questionIds: number[];
  private positionTimer: ReturnType<typeof setTimeout> | undefined;
  private lastPosition = '';
  private readonly savedOffset: number;

  constructor(lesson: LessonLike) {
    this.sessionId = lesson.session_id;
    this.study = lesson.runtime === 'study';
    this.questionIds = lesson.questions.map((question) => question.id);
    this.answers = restoreAnswers(lesson);
    this.roundId = lesson.check?.round_id ?? null;
    this.checkRevision = lesson.check?.revision ?? 0;
    this.stage = lesson.checkpoint?.body.stage ?? 'learn';
    this.savedOffset = lesson.checkpoint?.body.reading.offset ?? 0;
  }

  get complete(): boolean {
    return this.answers.length > 0 && this.answers.every((answer) => answer >= 0);
  }

  get unanswered(): number {
    return this.answers.filter((answer) => answer < 0).length;
  }

  /** Human-readable save state for the shell header. */
  get saveMessage(): string {
    if (!this.study) return 'answers kept until you submit';
    if (this.answerStatus === 'saving') return 'saving answers…';
    if (this.answerStatus === 'error') return `answers not saved: ${this.answerError}`;
    return 'answers saved with this lesson';
  }

  /** The reading offset to restore once the lesson is on screen. */
  get restoreOffset(): number {
    return this.study ? this.savedOffset : 0;
  }

  choose(index: number, choiceIndex: number) {
    const next = [...this.answers];
    next[index] = choiceIndex;
    this.answers = next;
    if (!this.study || !this.roundId) return;
    const round = this.roundId;
    const questionId = this.questionIds[index];
    this.enqueue(async () => {
      const saved = await api.saveClassCheckAnswer(this.sessionId, round, this.checkRevision, questionId, choiceIndex);
      this.checkRevision = saved.revision;
    });
  }

  /** Merge production work (writing, listening, reflection…) into the checkpoint. */
  saveWork(work: Record<string, unknown>) {
    if (!this.study) return;
    this.enqueue(async () => {
      await api.saveClassLessonWork({ session_id: this.sessionId, expected_revision: null, work });
    });
  }

  private enqueue(task: () => Promise<void>) {
    this.answerStatus = 'saving';
    this.queue = this.queue.then(async () => {
      try {
        await task();
        this.answerStatus = 'saved';
        this.answerError = '';
      } catch (error) {
        this.answerStatus = 'error';
        this.answerError = String(error);
      }
    });
  }

  /** Wait for pending saves; a failed save is an error the caller must show. */
  async flush(): Promise<void> {
    await this.queue;
    if (this.answerStatus === 'error') {
      throw new Error(this.answerError || 'Saved answers could not be stored. Try again.');
    }
  }

  /** Round identity captured for submission, after `flush()`. */
  get round(): { roundId: AssessmentRoundId; revision: number } {
    if (!this.roundId) throw new Error('The knowledge check is not ready yet.');
    return { roundId: this.roundId, revision: this.checkRevision };
  }

  /** Debounced reading position and stage; call from the scroll container. */
  trackPosition(scroller: HTMLElement, anchors: StageAnchors, submitted: boolean) {
    this.stage = submitted ? 'feedback' : this.stageAt(scroller, anchors);
    if (!this.study || submitted) return;
    clearTimeout(this.positionTimer);
    this.positionTimer = setTimeout(() => void this.savePosition(scroller, anchors), 700);
  }

  stageAt(scroller: HTMLElement, anchors: StageAnchors): LessonStage {
    return lessonStageFor(
      Math.round(scroller.scrollTop),
      scroller.clientHeight,
      anchors.practice?.offsetTop ?? null,
      anchors.check?.offsetTop ?? null,
    );
  }

  async savePosition(scroller: HTMLElement, anchors: StageAnchors): Promise<void> {
    if (!this.study) return;
    const offset = Math.round(scroller.scrollTop);
    const stage = this.stageAt(scroller, anchors);
    const key = `${offset}:${stage}`;
    if (key === this.lastPosition) return;
    this.lastPosition = key;
    const reading: LessonReadingPosition = { anchor: null, offset };
    try {
      await api.saveClassLessonWork({ session_id: this.sessionId, expected_revision: null, stage, reading });
    } catch {
      this.lastPosition = '';
    }
  }

  /** Flush pending saves, store final work, and pause the runtime session. */
  async pause(work?: Record<string, unknown>): Promise<void> {
    clearTimeout(this.positionTimer);
    if (!this.study) return;
    if (work) this.saveWork(work);
    await this.flush();
    await api.pauseClassLesson(this.sessionId);
  }

  dispose() {
    clearTimeout(this.positionTimer);
  }
}
