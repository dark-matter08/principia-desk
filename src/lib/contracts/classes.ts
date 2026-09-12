import type { EnrollmentConfiguration, EnrollmentDraftId } from './enrollment';
import type { PathRecommendation, PathTopic } from './placement';
export interface PathReference { class_id: string; path_revision_id: string; course_snapshot_fingerprint: string }
export interface AcceptedPath { reference: PathReference; revision: number; configuration: EnrollmentConfiguration; recommendation: PathRecommendation; accepted_at: string }
export interface PathSummary { class_id: string; path_revision_id: string; revision: number; entry_point: string; entry_label: string; route: string; earlier_topics: number; refreshers: number }
export interface AcceptPath { draft_id: EnrollmentDraftId; expected_revision: number; recommendation_id: string }
/** A deliberate route change; never a grade or completion evidence. */
export type PathChange =
  | { kind: 'bypass'; topics: string[] }
  | { kind: 'include'; topics: string[] }
  /** Prior knowledge demonstrated by a unit challenge attempt. */
  | { kind: 'check_out'; topics: string[]; attempt_id: string }
  /** Take a short bridge lesson on `topic` before more work on `before`. */
  | { kind: 'accept_bridge'; topic: string; before: string }
  /** Decline a proposed bridge; it is not proposed again for that pair. */
  | { kind: 'decline_bridge'; topic: string; before: string };
/** A gap that appeared in practice: a failed check on a topic whose prerequisite was set aside. */
export interface BridgeProposal { topic: PathTopic; before: PathTopic; session_id: string; score: number }
export interface RevisePath { course_id: string; expected_revision: number; change: PathChange }
