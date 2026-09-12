import { describe, expect, it, vi } from 'vitest';
import { api, type AppStateView } from './ipc';
import { mockEmit } from './mock';
import { app, resolveRoute, shouldShowEscapeHatch } from './stores.svelte';

function state(overrides: Partial<AppStateView> = {}): AppStateView {
  return {
    onboarded: true,
    selected_focus: 'javascript',
    schedule_hour: 9,
    schedule_minute: 0,
    debug_day: false,
    enforcement_disarmed: false,
    schedule_paused: false,
    kiosk_level: 'hard',
    model: 'opus',
    agent: 'claude',
    custom_agent_bin: '',
    deepseek_key_configured: false,
    alarm: null,
    blocks: [],
    classroom_programs: [],
    classroom_slots: [],
    classroom_due_count: 0,
    active_classroom_sessions: [],
    appointments: [],
    focus: null,
    ...overrides,
  };
}

describe('authoritative screen routing', () => {
  it('keeps the current screen once onboarded', () => {
    expect(resolveRoute(state(), 'loading')).toEqual({ screen: 'idle' });
    expect(resolveRoute(state(), 'classroom')).toEqual({ screen: 'classroom' });
  });

  it('leaves the setup wizard only once onboarding is complete', () => {
    expect(resolveRoute(state({ onboarded: false }), 'idle')).toEqual({ screen: 'setup' });
    expect(resolveRoute(state(), 'setup')).toEqual({ screen: 'idle' });
  });
});

describe('escape hatch visibility', () => {
  it('stays hidden without a focused holder', () => {
    expect(shouldShowEscapeHatch(null)).toBe(false);
    expect(shouldShowEscapeHatch(state())).toBe(false);
  });
});

it('keeps the latest app state when refreshes resolve out of order', async () => {
  const first = state();
  const second = state({ debug_day: true });
  const getState = vi.spyOn(api, 'getAppState').mockResolvedValue(first);
  vi.spyOn(api, 'markFrontendReady').mockResolvedValue(undefined);
  try {
    await app.init();
    let releaseOld!: (value: AppStateView) => void;
    getState.mockImplementationOnce(() => new Promise((resolve) => { releaseOld = resolve; }));
    const oldRefresh = app.refresh();
    getState.mockResolvedValueOnce(second);
    await app.refresh();
    expect(app.state?.debug_day).toBe(true);

    releaseOld(first);
    await oldRefresh;
    expect(app.state?.debug_day).toBe(true);
    expect(app.screen).toBe('idle');
  } finally {
    vi.restoreAllMocks();
  }
});

it('keeps preparation visible across navigation and prevents duplicate starts until failure releases it', async () => {
  app.state = state();
  app.screen = 'idle';
  let rejectStart!: (cause: Error) => void;
  const start = vi.spyOn(api, 'startClassroomSession').mockImplementationOnce(() => new Promise((_, reject) => { rejectStart = reject; }));
  try {
    const pending = app.startClass('linux-bash', 12);
    expect(app.preparingClass?.subjectId).toBe('linux-bash');
    app.navigate('classes');
    await app.startClass('german', 13);
    expect(start).toHaveBeenCalledTimes(1);
    expect(app.preparingClass?.subjectId).toBe('linux-bash');
    rejectStart(new Error('Provider unavailable'));
    await pending;
    expect(app.preparingClass).toBeNull();
    expect(app.error).toContain('Provider unavailable');
    start.mockRejectedValueOnce(new Error('Authentication required'));
    await app.startClass('german', 13);
    expect(start).toHaveBeenCalledTimes(2);
    expect(app.preparingClass).toBeNull();
    expect(app.error).toContain('Authentication required');
  } finally {
    vi.restoreAllMocks();
    app.error = '';
  }
});

describe('focused class sessions', () => {
  const focus = { session_id: 'study-1', course_id: 'typescript', policy: 'focused' as const, locked: true };

  it('shows the escape hatch only once the kiosk is engaged for the holder', () => {
    expect(shouldShowEscapeHatch(state({ focus }))).toBe(true);
    expect(shouldShowEscapeHatch(state({ focus: { ...focus, locked: false } }))).toBe(false);
  });

  it('locks navigation and reports the holder to other classes', () => {
    app.state = state({ focus });
    app.screen = 'idle';
    expect(app.locked).toBe(true);
    expect(app.isFocusLocked('study-1')).toBe(true);
    expect(app.isFocusLocked('study-2')).toBe(false);
    expect(app.heldByOtherClass('javascript')).toBe('typescript');
    expect(app.heldByOtherClass('typescript')).toBeNull();
    app.navigate('progress');
    expect(app.screen).toBe('idle');
    app.state = state();
    expect(app.locked).toBe(false);
    app.navigate('progress');
    expect(app.screen).toBe('dashboard');
    app.navigate('today');
  });
});
