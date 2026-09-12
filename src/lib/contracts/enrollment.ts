import type { AgentId, ClassroomSubjectId } from '../ipc';

export type EnrollmentDraftId = string & { readonly __entity: 'enrollment_draft' };
export interface CourseReference { course_id: ClassroomSubjectId; version: string; fingerprint: string }
export type EntryChoice =
  | { route: 'foundations' }
  | { route: 'diagnostic' }
  | { route: 'manual'; entry_point: string; familiar_competencies: string[] };
export type LearningGoal =
  | { kind: 'course_outcome'; note: string }
  | { kind: 'language_level'; target_level: string; note: string };
export interface EnrollmentConfiguration {
  goal: LearningGoal;
  entry: EntryChoice;
  pace: { session_minutes: number; weekly_minutes: number | null };
  tutor: { provider: AgentId; model: string; custom_agent_bin: string | null };
  focus_policy: 'advisory' | 'focused' | 'strict';
}
export interface EnrollmentOptions {
  course: CourseReference;
  entry_points: { id: string; label: string }[];
  familiarity_options: { id: string; label: string; group: string | null }[];
  default_configuration: EnrollmentConfiguration;
  /** A placement check exists for the course. A learner's own class has none until its question bank is written. */
  diagnostic_available?: boolean;
}
export interface EnrollmentDraft {
  id: EnrollmentDraftId;
  course: CourseReference;
  configuration: EnrollmentConfiguration;
  status: 'draft' | 'accepted' | 'cancelled';
  revision: number;
  accepted_class_id: string | null;
  created_at: string;
  updated_at: string;
}
export interface SaveEnrollmentDraft {
  id: EnrollmentDraftId | null;
  expected_revision: number | null;
  course: CourseReference;
  configuration: EnrollmentConfiguration;
}
