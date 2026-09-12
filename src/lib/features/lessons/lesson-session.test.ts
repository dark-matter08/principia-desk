import { afterEach, describe, expect, it, vi } from 'vitest';
import { api } from '../../ipc';
import { LessonSession, restoreAnswers, type LessonLike } from './lesson-session.svelte';

function lesson(overrides: Partial<LessonLike> = {}): LessonLike {
  return {
    session_id: 'study-1',
    runtime: 'study',
    checkpoint: { revision: 2, body: { stage: 'practice', reading: { anchor: null, offset: 420 }, work: { reflection: 'notes' } }, updated_at: '' },
    check: {
      round_id: 'round-1',
      revision: 3,
      submitted: false,
      responses: {
        '1': { status: 'answered', answer: '2', revision: 1, saved_at: '' },
        '2': { status: 'draft', answer: '0', revision: 1, saved_at: '' },
      },
    } as unknown as LessonLike['check'],
    questions: [{ id: 1 }, { id: 2 }, { id: 3 }],
    ...overrides,
  };
}

afterEach(() => vi.restoreAllMocks());

describe('restoreAnswers', () => {
  it('restores only confirmed answers from the frozen round', () => {
    expect(restoreAnswers(lesson())).toEqual([2, -1, -1]);
    expect(restoreAnswers(lesson({ check: null }))).toEqual([-1, -1, -1]);
  });
});

describe('LessonSession', () => {
  it('starts from the saved stage, offset and answers', () => {
    const session = new LessonSession(lesson());
    expect(session.stage).toBe('practice');
    expect(session.restoreOffset).toBe(420);
    expect(session.answers).toEqual([2, -1, -1]);
    expect(session.complete).toBe(false);
    expect(session.unanswered).toBe(2);
    expect(session.saveMessage).toBe('answers saved with this lesson');
  });

  it('serializes answer saves and carries the round revision forward', async () => {
    const save = vi.spyOn(api, 'saveClassCheckAnswer').mockImplementation(async (_s, _r, revision) => ({ revision: revision + 1 }) as never);
    const session = new LessonSession(lesson());
    session.choose(1, 1);
    session.choose(2, 0);
    expect(session.answerStatus).toBe('saving');
    await session.flush();
    expect(save).toHaveBeenNthCalledWith(1, 'study-1', 'round-1', 3, 2, 1);
    expect(save).toHaveBeenNthCalledWith(2, 'study-1', 'round-1', 4, 3, 0);
    expect(session.complete).toBe(true);
    expect(session.round).toEqual({ roundId: 'round-1', revision: 5 });
    expect(session.answerStatus).toBe('saved');
  });

  it('surfaces a failed save and refuses to flush silently', async () => {
    vi.spyOn(api, 'saveClassCheckAnswer').mockRejectedValue(new Error('disk full'));
    const session = new LessonSession(lesson());
    session.choose(1, 1);
    await expect(session.flush()).rejects.toThrow('disk full');
    expect(session.answerStatus).toBe('error');
    expect(session.saveMessage).toContain('disk full');
    expect(session.answers[1]).toBe(1);
  });

  it('keeps legacy lessons in memory without native calls', async () => {
    const save = vi.spyOn(api, 'saveClassCheckAnswer');
    const work = vi.spyOn(api, 'saveClassLessonWork');
    const pause = vi.spyOn(api, 'pauseClassLesson');
    const session = new LessonSession(lesson({ runtime: 'legacy', session_id: '17', checkpoint: null, check: null }));
    session.choose(0, 1);
    session.saveWork({ reflection: 'x' });
    await session.pause({ reflection: 'x' });
    expect(session.answers).toEqual([1, -1, -1]);
    expect(session.restoreOffset).toBe(0);
    expect(session.saveMessage).toBe('answers kept until you submit');
    expect(save).not.toHaveBeenCalled();
    expect(work).not.toHaveBeenCalled();
    expect(pause).not.toHaveBeenCalled();
  });

  it('saves the reading position with its stage once per distinct position', async () => {
    const work = vi.spyOn(api, 'saveClassLessonWork').mockResolvedValue({} as never);
    const session = new LessonSession(lesson());
    const scroller = { scrollTop: 900, clientHeight: 600 } as HTMLElement;
    const anchors = { practice: { offsetTop: 1000 } as HTMLElement, check: { offsetTop: 2000 } as HTMLElement };
    await session.savePosition(scroller, anchors);
    await session.savePosition(scroller, anchors);
    expect(work).toHaveBeenCalledTimes(1);
    expect(work).toHaveBeenCalledWith({ session_id: 'study-1', expected_revision: null, stage: 'practice', reading: { anchor: null, offset: 900 } });
    scroller.scrollTop = 1800;
    await session.savePosition(scroller, anchors);
    expect(work).toHaveBeenLastCalledWith(expect.objectContaining({ stage: 'check', reading: { anchor: null, offset: 1800 } }));
  });

  it('pauses after flushing work and answers', async () => {
    vi.spyOn(api, 'saveClassCheckAnswer').mockResolvedValue({ revision: 4 } as never);
    const work = vi.spyOn(api, 'saveClassLessonWork').mockResolvedValue({} as never);
    const pause = vi.spyOn(api, 'pauseClassLesson').mockResolvedValue(undefined);
    const session = new LessonSession(lesson());
    session.choose(1, 0);
    await session.pause({ writing_response: 'Hallo' });
    expect(work).toHaveBeenCalledWith({ session_id: 'study-1', expected_revision: null, work: { writing_response: 'Hallo' } });
    expect(pause).toHaveBeenCalledWith('study-1');
  });
});
