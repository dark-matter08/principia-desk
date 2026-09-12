import manifest from '../../src-tauri/seed/catalog.json';
import type { ClassroomSubjectId, FocusArea } from './catalog.generated';

export interface CourseDefinition {
  id: ClassroomSubjectId;
  course_id: string;
  kind: 'engineering' | 'language';
  label: string;
  native_label: string;
  short_code: string;
  title: string;
  summary: string;
  version: string;
  context: string;
  outcome: string;
  environment: string;
  prompt_profile: string;
  prerequisite_courses: readonly ClassroomSubjectId[];
  source_hosts: readonly string[];
  reference_lessons: readonly string[];
  capabilities: readonly string[];
  entry_points: readonly { id: string; label: string }[];
}

// Both native and preview builds derive this metadata from the same manifest.
export const COURSES: readonly CourseDefinition[] = manifest.courses.map(({ prompt_path: _promptPath, ...course }) => ({
  ...course,
  prompt_profile: `classroom.${course.id}`,
})) as CourseDefinition[];

export const ENGINEERING_COURSES = COURSES.filter((course): course is CourseDefinition & { id: FocusArea } => course.kind === 'engineering');

/** Courses the desk reported at runtime: the bundled ones again, and the
 *  learner's own. Filled on every state refresh from `get_catalog`. */
const runtime = new Map<string, CourseDefinition>();

export function registerCourses(courses: readonly CourseDefinition[]) {
  for (const course of courses) runtime.set(course.id, course);
}

export function courseDefinition(id: string): CourseDefinition | undefined {
  return runtime.get(id) ?? COURSES.find((course) => course.id === id);
}

/** Whether a course is one the learner made. */
export function isCustomCourse(id: string): boolean {
  return id.startsWith('custom-');
}

/** Every course known right now, bundled first. */
export function allCourses(): CourseDefinition[] {
  const known = new Map<string, CourseDefinition>();
  for (const course of COURSES) known.set(course.id, course);
  for (const [id, course] of runtime) known.set(id, course);
  return [...known.values()];
}
