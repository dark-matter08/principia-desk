import type { AssessmentResponse, AssessmentWork } from '../../contracts/assessments';

export interface AssessmentEditorState extends AssessmentWork {
  status: 'saved' | 'saving' | 'error' | 'conflict';
  error: string;
}
type StorageAccess = Pick<Storage, 'getItem' | 'setItem' | 'removeItem'>;
type Persist = (item: string, response: AssessmentResponse, expectedRevision: number) => Promise<number>;
const same = (a: AssessmentResponse | undefined, b: AssessmentResponse) => a?.answer === b.answer && a?.status === b.status;
function browserStorage(): StorageAccess | undefined { try { return globalThis.localStorage; } catch { return undefined; } }

/** One serialized answer queue per frozen round, independent of its screen. */
export function createAssessmentEditor(initial: AssessmentWork, persist: Persist, storage = browserStorage()) {
  const key = `principia:assessment-work:${initial.roundId}`;
  let state: AssessmentEditorState = { ...structuredClone(initial), status: 'saved', error: '' };
  let pending: Record<string, AssessmentResponse> = Object.create(null);
  let recoveryStorage = storage;
  let running: Promise<boolean> | undefined;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const listeners = new Set<(state: AssessmentEditorState) => void>();
  const emit = () => listeners.forEach((listener) => listener(structuredClone(state)));
  function cache() {
    try {
      if (Object.keys(pending).length) recoveryStorage?.setItem(key, JSON.stringify({ roundId: state.roundId, baseRevision: state.revision, pending }));
      else recoveryStorage?.removeItem(key);
    } catch { /* Native persistence still runs when browser recovery storage is unavailable. */ }
  }
  let raw: string | null | undefined;
  try { raw = recoveryStorage?.getItem(key); } catch { recoveryStorage = undefined; }
  try {
    if (raw) {
      const recovered = JSON.parse(raw);
      if (recovered.roundId !== initial.roundId || !Number.isInteger(recovered.baseRevision) || recovered.baseRevision < 0 || !recovered.pending || typeof recovered.pending !== 'object' || Array.isArray(recovered.pending)) throw new Error('Invalid recovery record');
      for (const [id, response] of Object.entries(recovered.pending) as [string, AssessmentResponse][]) {
        if (!Object.hasOwn(initial.responses, id) || !response || typeof response.answer !== 'string' || !['draft', 'answered', 'skipped'].includes(response.status)) throw new Error('Invalid recovery answer');
        if (!same(initial.responses[id], response)) pending[id] = response;
      }
      if (Object.keys(pending).length && recovered.baseRevision !== initial.revision) {
        state.status = 'conflict'; state.error = 'A newer version of this round was saved. Choose which answers to keep.';
      } else {
        Object.assign(state.responses, pending);
        if (Object.keys(pending).length) state.status = 'saving';
        else cache();
      }
    }
  } catch {
    // Do not offer a partial recovery after one malformed answer. Retain the
    // original bytes until the learner explicitly chooses the saved round.
    pending = Object.create(null);
    state.status = 'conflict'; state.error = 'The local recovery record could not be read. It is retained until you choose the saved answers.';
  }

  async function run(): Promise<boolean> {
    while (Object.keys(pending).length) {
      const [id, response] = Object.entries(pending)[0];
      state.status = 'saving'; state.error = ''; emit();
      try {
        state.revision = await persist(id, structuredClone(response), state.revision);
        if (same(pending[id], response)) delete pending[id];
        cache();
      } catch (error) {
        state.status = 'error'; state.error = String(error); emit(); return false;
      }
    }
    state.status = 'saved'; state.error = ''; emit(); return true;
  }
  function flush(): Promise<boolean> {
    clearTimeout(timer);
    if (state.status === 'conflict') return Promise.resolve(false);
    if (running) return running;
    running = run().finally(() => { running = undefined; });
    return running;
  }
  function edit(id: string, response: AssessmentResponse) {
    if (!Object.hasOwn(state.responses, id)) throw new Error('Question does not belong to this frozen round.');
    if (state.status === 'conflict') return;
    state.responses[id] = structuredClone(response);
    pending[id] = structuredClone(response);
    state.status = 'saving'; state.error = ''; cache(); emit();
    clearTimeout(timer); timer = setTimeout(() => { void flush(); }, 400);
  }
  /** Explicit recovery choice after fetching the current native revision. */
  async function resolve(saved: AssessmentWork, keepLocal: boolean) {
    if (saved.roundId !== state.roundId) throw new Error('The active round changed. The old local answers remain retained.');
    if (keepLocal && state.status === 'conflict' && !Object.keys(pending).length) throw new Error('This recovery record is unreadable; choose the saved answers to discard it.');
    if (running) await running;
    clearTimeout(timer);
    if (!keepLocal) pending = Object.create(null);
    state = { ...structuredClone(saved), status: 'saved', error: '' };
    Object.assign(state.responses, pending);
    cache(); emit();
    return flush();
  }
  function refresh(saved: AssessmentWork) {
    if (saved.roundId === state.roundId && !running && !Object.keys(pending).length && state.status === 'saved') {
      state = { ...structuredClone(saved), status: 'saved', error: '' }; emit();
    }
  }
  return {
    edit, flush, resolve, refresh,
    hasLocalChanges: () => Object.keys(pending).length > 0,
    snapshot: () => structuredClone(state),
    subscribe(listener: (state: AssessmentEditorState) => void) { listeners.add(listener); listener(structuredClone(state)); return () => { listeners.delete(listener); }; },
  };
}

const editors = new Map<string, ReturnType<typeof createAssessmentEditor>>();
export function assessmentEditor(work: AssessmentWork, persist: Persist) {
  const existing = editors.get(work.roundId);
  if (existing) { existing.refresh(work); return existing; }
  const editor = createAssessmentEditor(work, persist);
  editors.set(work.roundId, editor);
  return editor;
}
