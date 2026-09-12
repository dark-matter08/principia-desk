import type { AssessmentResponse, AssessmentRoundId } from './assessments';
import type { AssessmentAttemptId, CriterionResult, PathTopic } from './placement';

/** A unit challenge: a class-owned assessment attempt that samples one unit's
 *  entry questions. Passing samples can check the learner out of those topics;
 *  nothing here consumes a study session, an appointment or a focus lock. */
export interface UnitChallengeView {
  attempt_id: AssessmentAttemptId;
  round_id: AssessmentRoundId;
  revision: number;
  course_id: string;
  unit: string;
  unit_label: string;
  /** Path revision the challenge was started against. */
  path_revision: number;
  questions: { id: string; label: string; prompt: string; choices: { id: string; text: string }[] }[];
  responses: Record<string, AssessmentResponse>;
  criteria: CriterionResult[];
  submitted: boolean;
  /** Topics the passed samples demonstrated; they can be checked out of. */
  demonstrated: PathTopic[];
  /** Topics whose samples failed or were skipped; they stay on the route. */
  needs_practice: PathTopic[];
  /** Path revision that recorded this challenge, once applied. */
  applied_revision: number | null;
  estimated_minutes: number;
}
