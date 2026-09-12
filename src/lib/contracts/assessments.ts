export type AssessmentRoundId = string & { readonly __entity: 'assessment_round' };
export interface AssessmentResponse { answer: string; status: 'draft' | 'answered' | 'skipped' }
export interface AssessmentWork {
  roundId: AssessmentRoundId;
  revision: number;
  responses: Record<string, AssessmentResponse>;
}
