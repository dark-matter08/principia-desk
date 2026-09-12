import type { ClassroomProgramView, ClassroomSlotView, ScheduleConflict } from '../../ipc';

/** Minutes in one recurring week; study times occupy a circular interval. */
const WEEK_MINUTES = 7 * 24 * 60;
export const WEEKDAY_NAMES = ['Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday', 'Sunday'];

export interface ScheduleCandidate {
  subject_id: string;
  hour: number;
  minute: number;
  weekdays: number[];
  session_minutes: number;
  /** Minutes for particular weekdays; others use `session_minutes`. */
  durations?: Record<string, number>;
  /** "HH:MM" for particular weekdays; others start at `hour:minute`. */
  starts?: Record<string, string>;
}

/** The minutes a study time lasts on a weekday: its own figure, or the class default. */
export function dayMinutes(durations: Record<string, number> | undefined, weekday: number, fallback: number): number {
  return durations?.[String(weekday)] ?? fallback;
}

/** When a study time starts on a weekday: its own time, or the rule's. */
export function dayStart(starts: Record<string, string> | undefined, weekday: number, hour: number, minute: number): [number, number] {
  const own = starts?.[String(weekday)];
  if (!own) return [hour, minute];
  const [h, m] = own.split(':').map(Number);
  return Number.isInteger(h) && Number.isInteger(m) && h >= 0 && h <= 23 && m >= 0 && m <= 59 ? [h, m] : [hour, minute];
}

export interface ConflictScope {
  /** Rules being edited or replaced never conflict with themselves. */
  excludedSlotIds?: number[];
  /** Planning replaces this class's planned rules; ignore them. */
  excludePlannedFor?: string | null;
}

function interval(weekday: number, hour: number, minute: number, minutes: number): [number, number] {
  const start = (weekday - 1) * 1440 + hour * 60 + minute;
  return [start, start + Math.max(1, minutes)];
}

/** Half-open intervals compared against neighbouring week copies for wraparound. */
function overlaps(a: [number, number], b: [number, number]): boolean {
  return [-WEEK_MINUTES, 0, WEEK_MINUTES].some((shift) => a[0] < b[1] + shift && b[0] + shift < a[1]);
}

/**
 * Mirror of the native `classroom::schedule_conflicts` check so the editor and
 * planner can surface overlaps before a save. The native result stays authoritative.
 */
export function scheduleConflicts(
  candidates: ScheduleCandidate[],
  slots: ClassroomSlotView[],
  programs: ClassroomProgramView[],
  scope: ConflictScope = {},
): ScheduleConflict[] {
  const first = candidates[0];
  if (!first) return [];
  const program = (id: string) => programs.find((p) => p.subject_id === id);
  const excluded = new Set(scope.excludedSlotIds ?? []);
  const existing = slots.filter((slot) => {
    const own = slot.subject_id === first.subject_id;
    const owner = program(slot.subject_id);
    if (!slot.enabled || excluded.has(slot.id)) return false;
    if (!own && !owner?.enabled) return false;
    return !(own && slot.source === 'planned' && scope.excludePlannedFor === first.subject_id);
  });
  const label = program(first.subject_id)?.label ?? first.subject_id;
  const conflicts: ScheduleConflict[] = [];
  for (const candidate of candidates) {
    for (const weekday of candidate.weekdays) {
      const [hour, minute] = dayStart(candidate.starts, weekday, candidate.hour, candidate.minute);
      const mine = interval(weekday, hour, minute, dayMinutes(candidate.durations, weekday, candidate.session_minutes));
      for (const slot of existing) {
        const fallback = program(slot.subject_id)?.session_minutes ?? 30;
        for (const theirs of slot.weekdays) {
          const minutes = dayMinutes(slot.durations, theirs, fallback);
          const [theirHour, theirMinute] = dayStart(slot.starts, theirs, slot.hour, slot.minute);
          if (!overlaps(mine, interval(theirs, theirHour, theirMinute, minutes))) continue;
          conflicts.push({
            weekday, hour, minute, subject_id: candidate.subject_id, label,
            with_slot_id: slot.id, with_subject_id: slot.subject_id, with_label: slot.label,
            with_weekday: theirs, with_hour: theirHour, with_minute: theirMinute, with_session_minutes: minutes,
          });
        }
      }
    }
  }
  const key = (c: ScheduleConflict) => [c.weekday, c.hour, c.minute, c.with_slot_id, c.with_weekday].join(':');
  const seen = new Set<string>();
  return conflicts
    .sort((a, b) => a.weekday - b.weekday || a.hour - b.hour || a.minute - b.minute || a.with_slot_id - b.with_slot_id || a.with_weekday - b.with_weekday)
    .filter((c) => !seen.has(key(c)) && seen.add(key(c)));
}

const pad = (n: number) => String(n).padStart(2, '0');

export function describeConflict(conflict: ScheduleConflict): string {
  return `${conflict.with_label} on ${WEEKDAY_NAMES[conflict.with_weekday - 1]} at ${pad(conflict.with_hour)}:${pad(conflict.with_minute)} (${conflict.with_session_minutes} min)`;
}

/** Same wording as the native error so the browser preview matches the desktop. */
export function conflictMessage(conflicts: ScheduleConflict[]): string {
  const first = conflicts[0];
  if (!first) return '';
  const more = conflicts.length === 1 ? '' : ` and ${conflicts.length - 1} other overlap${conflicts.length === 2 ? '' : 's'}`;
  return `${first.label} on ${WEEKDAY_NAMES[first.weekday - 1]} at ${pad(first.hour)}:${pad(first.minute)} overlaps ${describeConflict(first)}${more}. Choose another time or shorten a session.`;
}
