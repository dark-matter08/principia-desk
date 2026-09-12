import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { COURSES } from './catalog';
import type { SaveEnrollmentDraft } from './contracts/enrollment';

let storage: Map<string, string>;
beforeEach(() => {
  vi.resetModules();
  storage = new Map();
  vi.stubGlobal('localStorage', {
    getItem: (key: string) => storage.get(key) ?? null,
    setItem: (key: string, value: string) => storage.set(key, value),
  });
});
afterEach(() => vi.unstubAllGlobals());

describe('enrollment draft preview contract', () => {
  it('restores all nine manual-entry drafts after a browser restart without accepting classes', async () => {
    const preview = await import('./enrollment-preview');
    const saved = COURSES.map((course) => {
      const options = preview.previewEnrollmentOptions(course.id);
      const configuration = structuredClone(options.default_configuration);
      configuration.entry = { route: 'manual', entry_point: options.entry_points.at(-1)!.id, familiar_competencies: [options.familiarity_options[0].id] };
      if (configuration.goal.kind === 'language_level') configuration.goal.target_level = 'B2';
      configuration.tutor = { provider: 'gemini', model: 'provider-specific-model', custom_agent_bin: null };
      return preview.savePreviewEnrollmentDraft({ id: null, expected_revision: null, course: options.course, configuration });
    });
    vi.resetModules();
    const reopened = await import('./enrollment-preview');
    for (const draft of saved) {
      expect(reopened.previewEnrollmentDraft(draft.course.course_id)).toEqual(draft);
      expect(draft.status).toBe('draft');
      expect(draft.accepted_class_id).toBeNull();
    }
    expect(storage.size).toBe(9);
  });

  it('makes saves idempotent and rejects stale changes without replacing the stored choices', async () => {
    const preview = await import('./enrollment-preview');
    const options = preview.previewEnrollmentOptions('linux-bash');
    const request: SaveEnrollmentDraft = { id: null, expected_revision: null, course: options.course, configuration: options.default_configuration };
    const first = preview.savePreviewEnrollmentDraft(request);
    expect(preview.savePreviewEnrollmentDraft(request)).toEqual(first);
    const changed = { ...request, id: first.id, expected_revision: first.revision, configuration: { ...request.configuration, entry: { route: 'diagnostic' as const } } };
    const second = preview.savePreviewEnrollmentDraft(changed);
    expect(second.revision).toBe(2);
    expect(preview.savePreviewEnrollmentDraft(changed)).toEqual(second);
    expect(() => preview.savePreviewEnrollmentDraft({ ...request, id: first.id, expected_revision: first.revision })).toThrow('changed');
    expect(preview.previewEnrollmentDraft('linux-bash')).toEqual(second);
  });

  it('rejects wrong curriculum versions and unrelated familiarity before creating a draft', async () => {
    const preview = await import('./enrollment-preview');
    const options = preview.previewEnrollmentOptions('bash-scripting');
    const request: SaveEnrollmentDraft = { id: null, expected_revision: null, course: options.course, configuration: options.default_configuration };
    expect(() => preview.savePreviewEnrollmentDraft({ ...request, course: { ...request.course, fingerprint: 'old' } })).toThrow('Curriculum changed');
    expect(() => preview.savePreviewEnrollmentDraft({ ...request, configuration: { ...request.configuration, entry: { route: 'manual', entry_point: options.entry_points[0].id, familiar_competencies: ['typescript-types'] } } })).toThrow('competencies');
    expect(storage.size).toBe(0);
  });

  it('keeps unsaved choices out of the cache when durable storage fails', async () => {
    const preview = await import('./enrollment-preview');
    const options = preview.previewEnrollmentOptions('german');
    vi.stubGlobal('localStorage', { getItem: () => null, setItem: () => { throw new Error('Storage full'); } });
    expect(() => preview.savePreviewEnrollmentDraft({ id: null, expected_revision: null, course: options.course, configuration: options.default_configuration })).toThrow('Storage full');
    expect(preview.previewEnrollmentDraft('german')).toBeNull();
  });
});
