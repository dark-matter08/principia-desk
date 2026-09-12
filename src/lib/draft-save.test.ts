import { afterEach, describe, expect, it, vi } from 'vitest';
import { createDraftSaver } from './draft-save';

function storage(): Storage {
  const values = new Map<string, string>();
  return {
    get length() { return values.size; },
    clear: () => values.clear(),
    getItem: (key) => values.get(key) ?? null,
    key: (index) => [...values.keys()][index] ?? null,
    removeItem: (key) => { values.delete(key); },
    setItem: (key, value) => { values.set(key, value); },
  };
}

afterEach(() => vi.useRealTimers());

describe('exercise draft persistence', () => {
  it('flushes before the debounce expires and keeps independently allocated owners isolated', async () => {
    vi.useFakeTimers();
    const cache = storage();
    const courseSave = vi.fn().mockResolvedValue(undefined);
    const classSave = vi.fn().mockResolvedValue(undefined);
    const course = createDraftSaver('course:1', courseSave, vi.fn(), cache);
    const classroom = createDraftSaver('classroom:1', classSave, vi.fn(), cache);
    course.schedule('course work');
    classroom.schedule('class work');
    await course.flush();
    expect(courseSave).toHaveBeenCalledWith('course work');
    expect(classSave).not.toHaveBeenCalled();
    expect(classroom.recoveredDraft()).toBe('class work');
    await classroom.flush();
  });

  it('serializes writes across navigation and remount so an old request cannot win', async () => {
    const cache = storage();
    const saved: string[] = [];
    let release!: () => void;
    const first = createDraftSaver('course:2', async (text) => {
      await new Promise<void>((resolve) => { release = resolve; });
      saved.push(text);
    }, vi.fn(), cache);
    first.schedule('old');
    const firstFlush = first.flush();
    await Promise.resolve();
    const next = createDraftSaver('course:2', async (text) => { saved.push(text); }, vi.fn(), cache);
    next.schedule('new');
    const nextFlush = next.flush();
    expect(saved).toEqual([]);
    release();
    await Promise.all([firstFlush, nextFlush]);
    expect(saved).toEqual(['old', 'new']);
    expect(next.recoveredDraft()).toBeNull();
  });

  it('recovers a failed native write after remount and removes the backup only on success', async () => {
    const cache = storage();
    const status = vi.fn();
    const failed = createDraftSaver('course:3', async () => { throw new Error('disk full'); }, status, cache);
    failed.schedule('valuable work');
    await failed.flush();
    expect(status).toHaveBeenLastCalledWith('error');
    const save = vi.fn().mockResolvedValue(undefined);
    const remount = createDraftSaver('course:3', save, vi.fn(), cache);
    expect(remount.recoveredDraft()).toBe('valuable work');
    remount.schedule(remount.recoveredDraft()!);
    await remount.flush();
    expect(save).toHaveBeenCalledWith('valuable work');
    expect(remount.recoveredDraft()).toBeNull();
  });
});
