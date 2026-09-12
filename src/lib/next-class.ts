import type { AppStateView } from './ipc';

export interface NextClass {
  at: Date;
  label: string;
  due: boolean;
}

interface Candidate {
  at: Date;
  label: string;
  due: boolean;
}

function localDateKey(date: Date): string {
  return [
    date.getFullYear(),
    String(date.getMonth() + 1).padStart(2, '0'),
    String(date.getDate()).padStart(2, '0'),
  ].join('-');
}

export function nextScheduledClass(state: AppStateView | null, now: Date): NextClass | null {
  if (!state || state.schedule_paused) return null;

  const candidates: Candidate[] = [];
  const enabledSubjects = new Set(
    state.classroom_programs
      .filter((program) => program.enabled)
      .map((program) => program.subject_id),
  );

  for (const slot of state.classroom_slots) {
    if (!slot.enabled || !enabledSubjects.has(slot.subject_id)) continue;
    const at = slot.owed ? new Date(now) : new Date(slot.next_fire_at);
    if (Number.isNaN(at.getTime()) || (!slot.owed && at < now)) continue;
    candidates.push({ at, label: slot.label, due: slot.owed });
  }

  candidates.sort((left, right) => left.at.getTime() - right.at.getTime());
  const first = candidates[0];
  if (!first) return null;
  const labels = [
    ...new Set(
      candidates
        .filter((candidate) => Math.abs(candidate.at.getTime() - first.at.getTime()) < 1_000)
        .map((candidate) => candidate.label),
    ),
  ];
  return {
    at: first.at,
    label: labels.join(' + '),
    due: candidates.some(
      (candidate) =>
        candidate.due && Math.abs(candidate.at.getTime() - first.at.getTime()) < 1_000,
    ),
  };
}

export function formatClassCountdown(nextClass: NextClass | null, now: Date): string {
  if (!nextClass) return '—';
  if (nextClass.due) return 'due now';
  const total = Math.max(0, Math.floor((nextClass.at.getTime() - now.getTime()) / 1_000));
  const days = Math.floor(total / 86_400);
  const hours = Math.floor((total % 86_400) / 3_600);
  const minutes = Math.floor((total % 3_600) / 60);
  const seconds = total % 60;
  const clock = `${String(hours).padStart(2, '0')}:${String(minutes).padStart(2, '0')}:${String(seconds).padStart(2, '0')}`;
  return days > 0 ? `${days}d ${clock}` : clock;
}

export function formatClassTime(nextClass: NextClass | null, now: Date): string {
  if (!nextClass) return 'No scheduled classes';
  if (nextClass.due) return 'scheduled time has arrived';

  const tomorrow = new Date(now);
  tomorrow.setDate(tomorrow.getDate() + 1);
  const date = localDateKey(nextClass.at);
  const day =
    date === localDateKey(now)
      ? 'today'
      : date === localDateKey(tomorrow)
        ? 'tomorrow'
        : nextClass.at.toLocaleDateString(undefined, { weekday: 'short' });
  const time = `${String(nextClass.at.getHours()).padStart(2, '0')}:${String(nextClass.at.getMinutes()).padStart(2, '0')}`;
  return `${day} · ${time}`;
}
