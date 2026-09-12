import { beforeEach, describe, expect, it } from 'vitest';
import { mockApi } from '../../mock';

async function acceptFoundations() {
  const options = await mockApi.getEnrollmentOptions('typescript');
  const draft = await mockApi.saveEnrollmentDraft({ id: null, expected_revision: null, course: options.course, configuration: options.default_configuration });
  const recommendation = await mockApi.getPathRecommendation(draft.id, draft.revision);
  return mockApi.acceptClassPath({ draft_id: draft.id, expected_revision: draft.revision, recommendation_id: recommendation.id });
}

describe('unit challenges in the browser preview', () => {
  beforeEach(() => { if (typeof localStorage !== 'undefined') localStorage.clear(); });

  it('checks out only the demonstrated samples and leaves failed ones on the route', async () => {
    const accepted = await acceptFoundations();
    await expect(mockApi.startUnitChallenge('typescript', 'foundations')).resolves.toMatchObject({ unit: 'foundations', submitted: false, path_revision: accepted.revision });
    const challenge = (await mockApi.getUnitChallenge('typescript'))!;
    expect(challenge.questions).toHaveLength(3);
    const [first, second, third] = challenge.questions;
    // First sample correct, the other two skipped.
    const correct = (await import('../../../../src-tauri/seed/diagnostics.json')).default.courses.find((c) => c.course_id === 'typescript')!.questions.find((q) => q.id === first.id)!.answer;
    let revision = await mockApi.saveUnitChallengeResponse('typescript', challenge.round_id, challenge.revision, first.id, { answer: correct, status: 'answered' });
    revision = await mockApi.saveUnitChallengeResponse('typescript', challenge.round_id, revision, second.id, { answer: '', status: 'skipped' });
    revision = await mockApi.saveUnitChallengeResponse('typescript', challenge.round_id, revision, third.id, { answer: '', status: 'skipped' });
    const submitted = await mockApi.submitUnitChallengeRound('typescript', challenge.round_id, revision);
    expect(submitted.submitted).toBe(true);
    expect(submitted.demonstrated.map((t) => t.id)).toEqual([submitted.criteria[0].competency]);
    expect(submitted.needs_practice.map((t) => t.id)).toEqual([submitted.criteria[1].competency, submitted.criteria[2].competency]);
    const path = await mockApi.applyUnitChallenge('typescript', submitted.attempt_id, accepted.revision);
    expect(path.revision).toBe(accepted.revision + 1);
    expect(path.recommendation.checked?.map((t) => t.id)).toEqual(submitted.demonstrated.map((t) => t.id));
    const map = await mockApi.getCurriculumMap('typescript');
    expect(map.concepts.find((c) => c.slug === submitted.demonstrated[0].id)).toMatchObject({ path_status: 'prior_knowledge_checked', required: false });
    expect(map.concepts.find((c) => c.slug === submitted.needs_practice[0].id)?.path_status).toBe('upcoming');
    expect(map.path?.checked).toBe(1);
    expect((await mockApi.getUnitChallenge('typescript'))?.applied_revision).toBe(path.revision);
    // Applying again is idempotent.
    expect((await mockApi.applyUnitChallenge('typescript', submitted.attempt_id, accepted.revision)).revision).toBe(path.revision);
  });
});
