import { describe, expect, it } from 'vitest';
import { conflictMessage, dayStart, scheduleConflicts } from './schedule-conflicts';
import type { ClassroomProgramView, ClassroomSlotView } from '../../ipc';

function program(subject_id: string, label: string, enabled: boolean, session_minutes = 30): ClassroomProgramView {
  return { subject_id: subject_id as ClassroomProgramView['subject_id'], kind: 'engineering', label, native_label: '', short_code: 'X', enabled, agent: 'claude', model: 'sonnet', custom_agent_bin: '', prompt_profile: '', prompt_version: 'v1', session_minutes, learning_goal: '', target_weekly_minutes: 0, progress: 0, progress_label: '', completed: false, focus_policy: 'advisory' as const, route: null, review_due: 0, language_progress: null };
}
function slot(id: number, subject_id: string, label: string, hour: number, minute: number, weekdays: number[], source: 'manual' | 'planned' = 'manual', enabled = true): ClassroomSlotView {
  return { id, subject_id: subject_id as ClassroomSlotView['subject_id'], label, short_code: 'X', kind: 'engineering', hour, minute, weekdays, durations: {}, starts: {}, enabled, owed: false, next_fire_at: '', in_progress: false, source, occurrence_id: null, disposition: null };
}
const programs = [program('linux-bash', 'Linux Bash', true), program('typescript', 'TypeScript', false, 45)];
const slots = [slot(1, 'linux-bash', 'Linux Bash', 9, 0, [1, 3, 5]), slot(2, 'linux-bash', 'Linux Bash', 0, 10, [1]), slot(3, 'typescript', 'TypeScript', 14, 0, [2], 'planned')];

describe('scheduleConflicts', () => {
  it('reports overlaps with active classes and names the weekday and time', () => {
    const conflicts = scheduleConflicts([{ subject_id: 'typescript', hour: 9, minute: 15, weekdays: [3, 4], durations: {}, session_minutes: 45 }], slots, programs);
    expect(conflicts).toHaveLength(1);
    expect(conflicts[0]).toMatchObject({ weekday: 3, with_label: 'Linux Bash', with_hour: 9, with_minute: 0, with_session_minutes: 30 });
    expect(conflictMessage(conflicts)).toBe('TypeScript on Wednesday at 09:15 overlaps Linux Bash on Wednesday at 09:00 (30 min). Choose another time or shorten a session.');
  });
  it('treats adjacent sessions as free and wraps across the end of the week', () => {
    expect(scheduleConflicts([{ subject_id: 'typescript', hour: 9, minute: 30, weekdays: [3], durations: {}, session_minutes: 45 }], slots, programs)).toHaveLength(0);
    const wrapped = scheduleConflicts([{ subject_id: 'typescript', hour: 23, minute: 50, weekdays: [7], durations: {}, session_minutes: 30 }], slots, programs);
    expect(wrapped.map((c) => [c.weekday, c.with_weekday, c.with_hour, c.with_minute])).toEqual([[7, 1, 0, 10]]);
    expect(scheduleConflicts([{ subject_id: 'typescript', hour: 23, minute: 50, weekdays: [6], durations: {}, session_minutes: 30 }], slots, programs)).toHaveLength(0);
  });
  it('ignores paused classes, an edited rule and the planner’s own replaced rules', () => {
    // Linux Bash may overlap the paused TypeScript planned time.
    expect(scheduleConflicts([{ subject_id: 'linux-bash', hour: 14, minute: 15, weekdays: [2], durations: {}, session_minutes: 30 }], slots, programs)).toHaveLength(0);
    // TypeScript's own other times still count, unless they are being edited or replaced by planning.
    expect(scheduleConflicts([{ subject_id: 'typescript', hour: 14, minute: 30, weekdays: [2], durations: {}, session_minutes: 45 }], slots, programs)).toHaveLength(1);
    expect(scheduleConflicts([{ subject_id: 'typescript', hour: 14, minute: 30, weekdays: [2], durations: {}, session_minutes: 45 }], slots, programs, { excludedSlotIds: [3] })).toHaveLength(0);
    expect(scheduleConflicts([{ subject_id: 'typescript', hour: 14, minute: 30, weekdays: [2], durations: {}, session_minutes: 45 }], slots, programs, { excludePlannedFor: 'typescript' })).toHaveLength(0);
  });
  it('takes each day at its own start time, on both sides', () => {
    // Linux Bash at 09:00 Mon/Wed/Fri; a TypeScript rule at 07:30 that starts Wednesday at 09:10 runs into it only on Wednesday.
    const conflicts = scheduleConflicts([{ subject_id: 'typescript', hour: 7, minute: 30, weekdays: [1, 3], durations: {}, starts: { '3': '09:10' }, session_minutes: 45 }], slots, programs);
    expect(conflicts.map((c) => [c.weekday, c.hour, c.minute])).toEqual([[3, 9, 10]]);
    // An existing rule's own Friday start moves its interval: Linux Bash Friday at 06:00 is free at 09:00.
    const early = slots.map((s) => (s.id === 1 ? { ...s, starts: { '5': '06:00' } } : s));
    expect(scheduleConflicts([{ subject_id: 'typescript', hour: 9, minute: 0, weekdays: [5], durations: {}, session_minutes: 30 }], early, programs)).toHaveLength(0);
    expect(scheduleConflicts([{ subject_id: 'typescript', hour: 6, minute: 15, weekdays: [5], durations: {}, session_minutes: 30 }], early, programs).map((c) => [c.with_hour, c.with_minute])).toEqual([[6, 0]]);
    expect(dayStart({ '2': '25:00', '3': '18:05' }, 2, 7, 30)).toEqual([7, 30]);
    expect(dayStart({ '2': '25:00', '3': '18:05' }, 3, 7, 30)).toEqual([18, 5]);
  });
});
