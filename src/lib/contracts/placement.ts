import type { AssessmentResponse, AssessmentRoundId } from './assessments';
import type { CourseReference, EnrollmentDraftId } from './enrollment';
export type AssessmentAttemptId = string & { readonly __entity: 'assessment_attempt' };
export type PlacementVerdict = 'passed' | 'needs_practice' | 'unknown';
export interface CriterionResult {
  id: string; competency: string; entry_point: string; label: string; verdict: PlacementVerdict;
  evidence: { question_id: string; response: AssessmentResponse; verdict: PlacementVerdict; explanation: string; expected_answer: string }[];
}
export interface DiagnosticView {
  attempt_id: AssessmentAttemptId; round_id: AssessmentRoundId; ordinal: number; revision: number;
  questions: { id: string; label: string; prompt: string; choices: { id: string; text: string }[] }[];
  responses: Record<string, AssessmentResponse>; criteria: CriterionResult[];
  submitted: boolean; completed: boolean; can_follow_up: boolean; matches_draft: boolean;
  course: CourseReference; scope_note: string; estimated_minutes: number;
  /** What the first round samples, stage by stage, so the learner is briefed before the first question. */
  stages: StageSample[];
}
export interface StageSample { id: string; label: string; questions: number; samples: string[] }
export interface PathTopic { id: string; label: string; reason: string }
export interface PathRecommendation {
  id: string; draft_id: EnrollmentDraftId; draft_revision: number; course: CourseReference;
  route: 'foundations' | 'manual' | 'diagnostic'; entry_point: string; entry_label: string; explanation: string;
  earlier_topics: PathTopic[]; refreshers: PathTopic[]; criteria: CriterionResult[];
  required_outcome: string; assessment_attempt_id: AssessmentAttemptId | null;
  /** Topics left by choice after acceptance: not assessed, no credit. */
  bypassed?: PathTopic[];
  /** Topics whose prior knowledge a unit challenge demonstrated. */
  checked?: PathTopic[];
  /** Accepted bridge lessons, taken before dependent work. */
  bridges?: PathTopic[];
  /** Declined bridge proposals, recorded as `topic` before `before`. */
  declined_bridges?: { topic: string; before: string }[];
}
