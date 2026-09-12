import { api } from '../../ipc';
import type { ClassroomSubjectId } from '../../catalog.generated';
import { createEnrollmentEditor } from './enrollment-editor';

const controllers = new Map<ClassroomSubjectId, ReturnType<typeof createEnrollmentEditor>>();
export function enrollmentEditor(courseId: ClassroomSubjectId) {
  let controller = controllers.get(courseId);
  if (!controller) {
    let storage: Storage | undefined;
    try { storage = localStorage; } catch { /* Native save is still available. */ }
    controller = createEnrollmentEditor(courseId, api, storage);
    controllers.set(courseId, controller);
  }
  return controller;
}
