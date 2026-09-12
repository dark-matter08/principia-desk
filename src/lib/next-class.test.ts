import { describe, expect, it } from 'vitest';
import type { AppStateView, ClassroomSlotView } from './ipc';
import {
  nextScheduledClass,
} from './next-class';

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

function classroomSlot(
  at: Date,
  overrides: Partial<ClassroomSlotView> = {},
): ClassroomSlotView {
  return {
    id: 1,
    subject_id: 'typescript',
    label: 'TypeScript',
    short_code: 'TS',
    kind: 'engineering',
    hour: at.getHours(),
    minute: at.getMinutes(),
    weekdays: [at.getDay() === 0 ? 7 : at.getDay()], durations: {}, starts: {},
    enabled: true,
    owed: false,
    next_fire_at: at.toISOString(),
    in_progress: false,
    source: 'manual',
    occurrence_id: null,
    disposition: null,
    ...overrides,
  };
}

describe('next scheduled class', () => {
  const now = new Date(2026, 6, 28, 8, 0, 0);

  it('returns no countdown while the schedule is paused', () => {
    expect(nextScheduledClass(state({ schedule_paused: true }), now)).toBeNull();
  });

  it('does not turn an old daily time or obligation into a class appointment', () => {
    expect(nextScheduledClass(state({ schedule_hour: 9, schedule_minute: 0 }), now)).toBeNull();
    expect(nextScheduledClass(state(), now)).toBeNull();
  });

  it('uses a class time without adding the retired daily routine', () => {
    const at = new Date(2026, 6, 28, 9, 0, 0);
    const next = nextScheduledClass(
      state({
        classroom_programs: [
          {
            subject_id: 'typescript',
            kind: 'engineering',
            label: 'TypeScript',
            native_label: 'TypeScript',
            short_code: 'TS',
            enabled: true,
            agent: 'claude',
            model: 'opus',
            custom_agent_bin: '',
            prompt_profile: 'classroom.typescript',
            prompt_version: 'v1',
            session_minutes: 30,
            focus_policy: 'advisory' as const, route: null, review_due: 0,
            learning_goal: '',
            target_weekly_minutes: 90,
            progress: 0,
            progress_label: '',
            completed: false,
            language_progress: null,
          },
        ],
        classroom_slots: [classroomSlot(at)],
      }),
      now,
    );
    expect(next?.label).toBe('TypeScript');
  });

  it('uses saved class weekdays independently of the retired daily time', () => {
    const friday = new Date(2026, 6, 31, 18, 30, 0);
    const next = nextScheduledClass(
      state({
        schedule_hour: 25,
        classroom_programs: [
          {
            subject_id: 'typescript',
            kind: 'engineering',
            label: 'TypeScript',
            native_label: 'TypeScript',
            short_code: 'TS',
            enabled: true,
            agent: 'claude',
            model: 'opus',
            custom_agent_bin: '',
            prompt_profile: 'classroom.typescript',
            prompt_version: 'v1',
            session_minutes: 30,
            focus_policy: 'advisory' as const, route: null, review_due: 0,
            learning_goal: '',
            target_weekly_minutes: 90,
            progress: 0,
            progress_label: '',
            completed: false,
            language_progress: null,
          },
        ],
        classroom_slots: [classroomSlot(friday)],
      }),
      now,
    );
    expect(next?.label).toBe('TypeScript');
    expect(next?.at.getDay()).toBe(5);
  });

  it('chooses the earliest enabled class and ignores stale or disabled slots', () => {
    const first = new Date(2026, 6, 28, 8, 30, 0);
    const stale = new Date(2026, 6, 28, 7, 30, 0);
    const programs = [
      {
        subject_id: 'typescript' as const,
        kind: 'engineering' as const,
        label: 'TypeScript',
        native_label: 'TypeScript',
        short_code: 'TS',
        enabled: true,
        agent: 'claude' as const,
        model: 'opus' as const,
        custom_agent_bin: '',
        prompt_profile: 'classroom.typescript',
        prompt_version: 'v1',
        session_minutes: 30,
        focus_policy: 'advisory' as const, route: null, review_due: 0,
        learning_goal: '',
        target_weekly_minutes: 90,
        progress: 0,
        progress_label: '',
        completed: false,
        language_progress: null,
      },
    ];
    const next = nextScheduledClass(
      state({
        schedule_hour: 20,
        classroom_programs: programs,
        classroom_slots: [
          classroomSlot(stale),
          classroomSlot(first, { id: 2 }),
          classroomSlot(new Date(2026, 6, 28, 8, 15, 0), { id: 3, enabled: false }),
        ],
      }),
      now,
    );
    expect(next?.at.getHours()).toBe(8);
    expect(next?.at.getMinutes()).toBe(30);
  });
});
