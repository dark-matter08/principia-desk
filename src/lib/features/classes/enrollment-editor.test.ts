import { afterEach, describe, expect, it, vi } from 'vitest';
import { createEnrollmentEditor, type EnrollmentEditorState } from './enrollment-editor';
import { previewEnrollmentOptions } from '../../enrollment-preview';
import type { EnrollmentDraft, EnrollmentDraftId, SaveEnrollmentDraft } from '../../contracts/enrollment';

function fixture() {
  const values = new Map<string, string>();
  const storage = { getItem: (key: string) => values.get(key) ?? null, setItem: (key: string, value: string) => { values.set(key, value); }, removeItem: (key: string) => { values.delete(key); } };
  const options = previewEnrollmentOptions('bash-scripting');
  let stored: EnrollmentDraft | null = null;
  const save = (input: SaveEnrollmentDraft) => {
    if ((stored?.revision ?? null) !== input.expected_revision) throw new Error('Draft changed');
    stored = { ...input, id: 'draft-test' as EnrollmentDraftId, revision: (stored?.revision ?? 0) + 1, status: 'draft', accepted_class_id: null, created_at: 'today', updated_at: 'today' };
    return structuredClone(stored);
  };
  const api = {
    getEnrollmentOptions: vi.fn(async () => structuredClone(options)),
    getEnrollmentDraft: vi.fn(async () => structuredClone(stored)),
    saveEnrollmentDraft: vi.fn(async (input: SaveEnrollmentDraft) => save(input)),
  };
  return { storage, values, options, api, save, stored: () => stored };
}
afterEach(() => vi.useRealTimers());

describe('starting-preference editor persistence', () => {
  it('opens a fresh draft after acceptance without saving into the frozen draft on unmount', async () => {
    const { api, storage, options } = fixture();
    const editor = createEnrollmentEditor('bash-scripting', api, storage);
    let state!: EnrollmentEditorState;
    editor.subscribe(value => { state = value; });
    await editor.load(); await editor.flush();
    editor.accepted();
    expect(await editor.flush()).toBe(false);
    expect(api.saveEnrollmentDraft).toHaveBeenCalledTimes(1);
    api.getEnrollmentDraft.mockResolvedValueOnce(null);
    options.default_configuration.entry = { route: 'manual', entry_point: 'production', familiar_competencies: [] };
    await editor.load();
    expect(state.draft).toBeNull();
    expect(state.configuration?.entry).toEqual(options.default_configuration.entry);
  });
  it('serializes edits made during a save and retains the newest values after leaving the screen', async () => {
    vi.useFakeTimers();
    const fixtureData = fixture();
    const { api, options, save, storage, values } = fixtureData;
    let release!: () => void;
    api.saveEnrollmentDraft.mockImplementationOnce(async (input) => {
      await new Promise<void>((resolve) => { release = resolve; });
      return save(input);
    });
    const editor = createEnrollmentEditor('bash-scripting', api, storage);
    await editor.load();
    editor.edit({ ...options.default_configuration, goal: { kind: 'course_outcome', note: 'first' } });
    const pending = editor.flush();
    editor.edit({ ...options.default_configuration, goal: { kind: 'course_outcome', note: 'newest' } });
    expect(api.saveEnrollmentDraft).toHaveBeenCalledTimes(1);
    release();
    expect(await pending).toBe(true);
    expect(api.saveEnrollmentDraft).toHaveBeenCalledTimes(2);
    expect(fixtureData.stored()?.configuration.goal.note).toBe('newest');
    expect(fixtureData.stored()?.revision).toBe(2);
    expect(values.size).toBe(0);
  });

  it('recovers a failed save after restart and retries against the original native revision', async () => {
    const { api, options, storage, stored, values } = fixture();
    const first = createEnrollmentEditor('bash-scripting', api, storage);
    await first.load();
    api.saveEnrollmentDraft.mockRejectedValueOnce(new Error('Disk full'));
    first.edit({ ...options.default_configuration, entry: { route: 'diagnostic' } });
    expect(await first.flush()).toBe(false);
    expect(values.size).toBe(1);
    const reopened = createEnrollmentEditor('bash-scripting', api, storage);
    await reopened.load();
    await reopened.flush();
    expect(stored()?.configuration.entry.route).toBe('diagnostic');
    expect(values.size).toBe(0);
  });

  it('requires an explicit choice when a newer saved draft conflicts with recovered edits', async () => {
    const { api, options, save, storage, stored } = fixture();
    const first = createEnrollmentEditor('bash-scripting', api, storage);
    await first.load();
    await first.flush();
    first.edit({ ...options.default_configuration, goal: { kind: 'course_outcome', note: 'my pending choices' } });
    const other = stored()!;
    save({ id: other.id, expected_revision: other.revision, course: options.course, configuration: { ...options.default_configuration, goal: { kind: 'course_outcome', note: 'saved elsewhere' } } });
    expect(await first.flush()).toBe(false);
    const reopened = createEnrollmentEditor('bash-scripting', api, storage);
    let state!: EnrollmentEditorState;
    reopened.subscribe((value) => { state = value; });
    await reopened.load();
    expect(state.status).toBe('conflict');
    expect(await reopened.flush()).toBe(false);
    expect(stored()?.configuration.goal.note).toBe('saved elsewhere');
    // Editing an unresolved conflict must keep the old revision in recovery.
    reopened.edit({ ...state.configuration!, goal: { kind: 'course_outcome', note: 'edited recovery' } });
    const restartedAgain = createEnrollmentEditor('bash-scripting', api, storage);
    restartedAgain.subscribe((value) => { state = value; });
    await restartedAgain.load();
    expect(state.status).toBe('conflict');
    await restartedAgain.resolve(true);
    expect(stored()?.configuration.goal.note).toBe('edited recovery');
    expect(stored()?.revision).toBe(3);
  });

  it('keeps ordinary loading and save failures retryable without affecting another course', async () => {
    const { api, storage, options } = fixture();
    api.getEnrollmentOptions.mockRejectedValueOnce(new Error('Offline'));
    const editor = createEnrollmentEditor('bash-scripting', api, storage);
    let state!: EnrollmentEditorState;
    editor.subscribe((value) => { state = value; });
    await editor.load();
    expect(state.status).toBe('error');
    await editor.load();
    expect(state.options?.course).toEqual(options.course);
    expect(await editor.flush()).toBe(true);
    expect(api.saveEnrollmentDraft.mock.calls[0][0].course.course_id).toBe('bash-scripting');
  });

  it('retains an unreadable recovery copy until the learner explicitly chooses saved choices', async () => {
    const { api, storage, values } = fixture();
    storage.setItem('principia:enrollment-recovery:bash-scripting', '{ interrupted recovery data');
    const editor = createEnrollmentEditor('bash-scripting', api, storage);
    let state!: EnrollmentEditorState;
    editor.subscribe((value) => { state = value; });
    await editor.load();
    expect(state.status).toBe('error');
    expect(await editor.flush()).toBe(false);
    expect(api.saveEnrollmentDraft).not.toHaveBeenCalled();
    expect(values.size).toBe(1);
    await editor.resolve(false);
    expect(values.size).toBe(0);
    expect(await editor.flush()).toBe(true);
  });
});
