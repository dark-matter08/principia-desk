import { courseDefinition } from '../../catalog';
import type { ClassroomProgramView } from '../../ipc';

export type ClassTab = 'overview' | 'settings' | 'entry' | 'curriculum' | 'schedule';
export type ClassFilter = 'all' | 'active' | 'paused' | 'completed';

export function filterClasses(programs: ClassroomProgramView[], query: string, filter: ClassFilter, hasSavedWork: (id: ClassroomProgramView['subject_id']) => boolean) {
  const terms = query.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean);
  return programs.filter((program) => {
    const course = courseDefinition(program.subject_id);
    const searchable = `${program.label} ${program.native_label} ${program.short_code} ${course?.summary ?? ''}`.toLocaleLowerCase();
    if (!terms.every((term) => searchable.includes(term))) return false;
    if (filter === 'active') return program.enabled && !program.completed;
    if (filter === 'paused') return !program.enabled && !program.completed && (program.progress > 0 || !!program.accepted_path || hasSavedWork(program.subject_id));
    if (filter === 'completed') return program.completed;
    return true;
  });
}
