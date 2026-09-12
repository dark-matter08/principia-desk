import { afterEach, describe, expect, it, vi } from 'vitest';
import { createAssessmentEditor } from './work-editor';
import type { AssessmentWork } from '../../contracts/assessments';

const initial = (): AssessmentWork => ({ roundId: 'round-one' as AssessmentWork['roundId'], revision: 0, responses: { a: { answer: '', status: 'draft' }, b: { answer: '', status: 'draft' } } });
function storage() {
  const values = new Map<string, string>();
  return { values, getItem: (key: string) => values.get(key) ?? null, setItem: (key: string, value: string) => { values.set(key, value); }, removeItem: (key: string) => { values.delete(key); } };
}
afterEach(() => vi.useRealTimers());

describe('shared assessment answer recovery', () => {
  it('serializes rapid edits and confirmation behind the in-flight draft using the returned native revision', async () => {
    vi.useFakeTimers();
    const local = storage();
    let release!: () => void;
    const save = vi.fn().mockImplementationOnce(async () => { await new Promise<void>((resolve) => { release = resolve; }); return 1; }).mockResolvedValueOnce(2).mockResolvedValueOnce(3);
    const editor = createAssessmentEditor(initial(), save, local);
    editor.edit('a', { answer: 'first', status: 'draft' });
    const pending = editor.flush();
    editor.edit('a', { answer: 'final', status: 'answered' });
    editor.edit('b', { answer: 'another answer', status: 'draft' });
    expect(save).toHaveBeenCalledTimes(1);
    release(); await pending;
    expect(save.mock.calls.map((call) => call[2])).toEqual([0, 1, 2]);
    expect(editor.snapshot().responses.a).toEqual({ answer: 'final', status: 'answered' });
    expect(local.values.size).toBe(0);
  });

  it('retains failed work across restart and flushes it against the original revision', async () => {
    const local = storage();
    const first = createAssessmentEditor(initial(), async () => { throw new Error('offline'); }, local);
    first.edit('b', { answer: 'draft with an explanation', status: 'draft' });
    expect(await first.flush()).toBe(false);
    const save = vi.fn().mockResolvedValue(1);
    const restored = createAssessmentEditor(initial(), save, local);
    expect(restored.snapshot().responses.b.answer).toBe('draft with an explanation');
    expect(await restored.flush()).toBe(true);
    expect(save).toHaveBeenCalledWith('b', { answer: 'draft with an explanation', status: 'draft' }, 0);
    expect(local.values.size).toBe(0);
  });

  it('requires an explicit choice before replacing a newer saved answer and refuses to move work to another round', async () => {
    const local = storage();
    const first = createAssessmentEditor(initial(), async () => { throw new Error('offline'); }, local);
    first.edit('a', { answer: 'my unsent work', status: 'draft' }); await first.flush();
    const newer = initial(); newer.revision = 2; newer.responses.a = { answer: 'saved elsewhere', status: 'answered' };
    const save = vi.fn().mockResolvedValue(3);
    const restored = createAssessmentEditor(newer, save, local);
    expect(restored.snapshot().status).toBe('conflict');
    expect(await restored.flush()).toBe(false);
    expect(save).not.toHaveBeenCalled();
    await expect(restored.resolve({ ...newer, roundId: 'another' as AssessmentWork['roundId'] }, true)).rejects.toThrow('active round changed');
    expect(local.values.size).toBe(1);
    await restored.resolve(newer, true);
    expect(save).toHaveBeenCalledWith('a', { answer: 'my unsent work', status: 'draft' }, 2);
  });

  it('recognizes a lost save response after restart without resubmitting or creating another revision', async () => {
    const local = storage();
    const first = createAssessmentEditor(initial(), async () => { throw new Error('response lost'); }, local);
    first.edit('a', { answer: 'already saved', status: 'answered' }); await first.flush();
    const newer = initial(); newer.revision = 1; newer.responses.a = { answer: 'already saved', status: 'answered' };
    const save = vi.fn();
    const restored = createAssessmentEditor(newer, save, local);
    expect(restored.snapshot().status).toBe('saved');
    expect(await restored.flush()).toBe(true);
    expect(save).not.toHaveBeenCalled();
    expect(local.values.size).toBe(0);
  });

  it('retains an unreadable recovery record without saving only its valid answers', async () => {
    const local = storage();
    const key = 'principia:assessment-work:round-one';
    const raw = JSON.stringify({ roundId: 'round-one', baseRevision: 0, pending: { a: { answer: 'valid local work', status: 'draft' }, b: { answer: 12, status: 'draft' } } });
    local.setItem(key, raw);
    const save = vi.fn();
    const editor = createAssessmentEditor(initial(), save, local);
    expect(editor.snapshot().status).toBe('conflict');
    expect(editor.hasLocalChanges()).toBe(false);
    await expect(editor.resolve(initial(), true)).rejects.toThrow('unreadable');
    expect(await editor.flush()).toBe(false);
    expect(local.getItem(key)).toBe(raw);
    expect(save).not.toHaveBeenCalled();
    await editor.resolve(initial(), false);
    expect(local.values.size).toBe(0);
  });

  it('continues native saves when browser recovery storage cannot be read', async () => {
    const save = vi.fn().mockResolvedValue(1);
    const local = { getItem() { throw new Error('storage disabled'); }, setItem: vi.fn(), removeItem: vi.fn() };
    const editor = createAssessmentEditor(initial(), save, local);
    editor.edit('a', { answer: 'native draft', status: 'draft' });
    expect(await editor.flush()).toBe(true);
    expect(save).toHaveBeenCalledWith('a', { answer: 'native draft', status: 'draft' }, 0);
    expect(local.setItem).not.toHaveBeenCalled();
  });
});
