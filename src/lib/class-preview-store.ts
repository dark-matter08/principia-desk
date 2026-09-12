import type { AcceptedPath } from './contracts/classes';
import type { EnrollmentDraft } from './contracts/enrollment';

export interface PreviewClassRecord { draft: EnrollmentDraft; path: AcceptedPath }
const key = 'principia:preview:accepted-paths';
let memory: PreviewClassRecord[] = [];
export function previewClassRecords(): PreviewClassRecord[] {
  const raw = typeof localStorage !== 'undefined' ? localStorage.getItem(key) : null;
  const records = raw ? JSON.parse(raw) : memory;
  if (!Array.isArray(records)) throw new Error('Saved preview paths could not be read.');
  return structuredClone(records);
}
export function savePreviewClassRecords(records: PreviewClassRecord[]) {
  // The accepted draft and its immutable revision share one durable write.
  if (typeof localStorage !== 'undefined') localStorage.setItem(key, JSON.stringify(records));
  memory = structuredClone(records);
}
