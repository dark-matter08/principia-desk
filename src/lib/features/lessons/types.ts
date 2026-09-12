import type { LessonStage } from '../../ipc';

/** One entry of the shell's stage rail. */
export interface StageLink { id: LessonStage; label: string; available: boolean }

/** A frozen multiple-choice question as the shared check renders it. */
export interface CheckQuestion { id: number; prompt: string; choices: string[]; meta: string }

/** A graded answer returned with the session result. */
export interface CheckCorrection { question_id: number; correct: boolean; correct_answer: string; explanation: string }
