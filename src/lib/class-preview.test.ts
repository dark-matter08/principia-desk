import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { COURSES } from './catalog';
let storage: Map<string, string>;
beforeEach(() => {
  vi.resetModules(); storage = new Map();
  vi.stubGlobal('localStorage', { getItem: (key: string) => storage.get(key) ?? null, setItem: (key: string, value: string) => storage.set(key, value) });
});
afterEach(() => vi.unstubAllGlobals());
async function setup(courseId: typeof COURSES[number]['id'], entry = 'mechanisms') {
  const enrollment = await import('./enrollment-preview');
  const options = enrollment.previewEnrollmentOptions(courseId);
  const configuration = options.default_configuration;
  configuration.entry = { route: 'manual', entry_point: entry, familiar_competencies: [] };
  if (configuration.goal.kind === 'language_level') configuration.goal.target_level = 'B2';
  const draft = enrollment.savePreviewEnrollmentDraft({ id: null, expected_revision: null, course: options.course, configuration });
  const recommendation = await (await import('./placement-preview')).recommendPreviewPath(draft.id, draft.revision);
  return { draft, input: { draft_id: draft.id, expected_revision: draft.revision, recommendation_id: recommendation.id } };
}
describe('accepted preview paths', () => {
  it('accepts every course once, restores after restart, and keeps old revisions addressable', async () => {
    const classes = await import('./class-preview');
    const enrollment = await import('./enrollment-preview');
    for (const course of COURSES) {
      const { draft, input } = await setup(course.id, course.entry_points[1].id);
      const [first, duplicate] = await Promise.all([classes.acceptPreviewClassPath(input), classes.acceptPreviewClassPath(input)]);
      expect(first).toEqual(duplicate);
      expect(first.revision).toBe(1);
      expect(first.recommendation.required_outcome).toBe(course.outcome);
      expect(enrollment.previewEnrollmentDraft(course.id)).toBeNull();
      expect(enrollment.findPreviewEnrollmentDraft(draft.id)?.status).toBe('accepted');
      const next = await setup(course.id, course.entry_points[0].id);
      const second = await classes.acceptPreviewClassPath(next.input);
      expect(second.reference.class_id).toBe(first.reference.class_id);
      expect(second.revision).toBe(2);
      expect(await classes.acceptPreviewClassPath(input)).toEqual(first);
      expect(classes.previewClassPath(course.id)).toEqual(second);
    }
    vi.resetModules(); const reopened = await import('./class-preview');
    for (const course of COURSES) expect(reopened.previewClassPath(course.id)?.revision).toBe(2);
  });
  it('does not activate stale recommendations or lose drafts after storage failure', async () => {
    const classes = await import('./class-preview'); const enrollment = await import('./enrollment-preview');
    const { input, draft } = await setup('linux-bash');
    await expect(classes.acceptPreviewClassPath({ ...input, recommendation_id: 'stale' })).rejects.toThrow('changed');
    expect(classes.previewClassPath('linux-bash')).toBeNull();
    vi.stubGlobal('localStorage', { getItem: (key: string) => storage.get(key) ?? null, setItem: () => { throw new Error('Storage full'); } });
    await expect(classes.acceptPreviewClassPath(input)).rejects.toThrow('Storage full');
    expect(classes.previewClassPath('linux-bash')).toBeNull();
    expect(enrollment.previewEnrollmentDraft('linux-bash')).toEqual(draft);
  });
});
