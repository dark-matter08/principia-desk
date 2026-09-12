import { beforeEach, describe, expect, it, vi } from 'vitest';
import { COURSES } from './catalog';
import { previewEnrollmentOptions, savePreviewEnrollmentDraft } from './enrollment-preview';
import { getPreviewPlacement, startPreviewPlacement, savePreviewPlacement, submitPreviewPlacement, continuePreviewPlacement, finishPreviewPlacement, recommendPreviewPath } from './placement-preview';
import banks from '../../src-tauri/seed/diagnostics.json';
import type { ClassroomSubjectId } from './catalog.generated';
import type { DiagnosticView } from './contracts/placement';
import type { EnrollmentDraft } from './contracts/enrollment';
beforeEach(() => { const values = new Map<string, string>(); vi.stubGlobal('localStorage', { getItem: (key: string) => values.get(key) ?? null, setItem: (key: string, value: string) => values.set(key, value), removeItem: (key: string) => values.delete(key) }); });
function fixture(course: ClassroomSubjectId) { const options = previewEnrollmentOptions(course); const configuration = options.default_configuration; configuration.entry = { route: 'diagnostic' }; return savePreviewEnrollmentDraft({ id: null, expected_revision: null, course: options.course, configuration }); }
async function answer(draft: EnrollmentDraft, check: DiagnosticView, wrong: number[] = [], skip = false) {
  const bank = banks.courses.find((b) => b.course_id === draft.course.course_id)!;
  let revision = check.revision;
  for (const [index, q] of check.questions.entries()) {
    const key = bank.questions.find((item) => item.id === q.id)!;
    const value = wrong.includes(index) ? key.choices.find((c) => c.id !== key.answer)!.id : key.answer;
    revision = await savePreviewPlacement(draft.id, check.round_id, revision, q.id, { answer: skip ? '' : value, status: skip ? 'skipped' : 'answered' });
  }
  return submitPreviewPlacement(draft.id, check.round_id, revision);
}
describe('preview placement contracts', () => {
  it('uses the same authored banks and all-course expected recommendations as native', async () => {
    for (const course of COURSES) {
      const draft = fixture(course.id); const check = await startPreviewPlacement(draft.id, draft.revision, true);
      expect(check.questions[0]).not.toHaveProperty('answer');
      const done = await answer(draft, check); await finishPreviewPlacement(draft.id, done.round_id);
      const path = await recommendPreviewPath(draft.id, draft.revision);
      expect(path.entry_point).toBe(course.kind === 'language' ? 'A2' : 'synthesis');
      expect(path.criteria.every((r) => r.verdict === 'passed')).toBe(true);
      expect(path.required_outcome).toBe(course.outcome);
      expect(check.stages.length).toBe(course.kind === 'language' ? 2 : 4);
      expect(check.stages.every((stage) => stage.questions === 3 && stage.samples.length === 3)).toBe(true);
    }
  });
  it('restores drafts, rejects stale answers and wrong owners, and preserves skipped samples as unknown', async () => {
    const draft = fixture('linux-bash'); const check = await startPreviewPlacement(draft.id, draft.revision, true);
    const response = { answer: check.questions[0].choices[1].id, status: 'draft' as const };
    await savePreviewPlacement(draft.id, check.round_id, 0, check.questions[0].id, response);
    await expect(savePreviewPlacement(draft.id, check.round_id, 0, check.questions[1].id, response)).rejects.toThrow('Answers changed');
    const other = fixture('german'); await startPreviewPlacement(other.id, other.revision, true);
    await expect(savePreviewPlacement(other.id, check.round_id, 1, check.questions[0].id, response)).rejects.toThrow('changed');
    const restored = (await getPreviewPlacement(draft.id))!;
    expect(restored.responses[check.questions[0].id]).toEqual(response);
    const done = await answer(draft, restored, [], true); await finishPreviewPlacement(draft.id, done.round_id);
    const path = await recommendPreviewPath(draft.id, draft.revision);
    expect(path.entry_point).toBe('foundations'); expect(path.criteria.every((r) => r.verdict === 'unknown')).toBe(true);
  });
  it('keeps initial evidence across bounded follow-ups and rejects a stale goal recommendation', async () => {
    const draft = fixture('bash-scripting');
    const first = await startPreviewPlacement(draft.id, draft.revision, true); const done = await answer(draft, first, [0, 1]);
    const followup = await continuePreviewPlacement(draft.id, done.round_id);
    expect((await continuePreviewPlacement(draft.id, done.round_id)).round_id).toBe(followup.round_id);
    const finished = await answer(draft, followup);
    expect(finished.completed).toBe(true); expect(finished.criteria[0].evidence).toHaveLength(2);
    expect(finished.criteria[0].evidence[0].verdict).toBe('needs_practice');
    const config = structuredClone(draft.configuration); config.goal.note = 'New goal';
    const updated = savePreviewEnrollmentDraft({ id: draft.id, expected_revision: draft.revision, course: draft.course, configuration: config });
    await expect(recommendPreviewPath(updated.id, updated.revision)).rejects.toThrow('goal');
    expect((await getPreviewPlacement(updated.id))!.matches_draft).toBe(false);
  });
});
