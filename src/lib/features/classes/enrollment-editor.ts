import type { ClassroomSubjectId } from '../../catalog.generated';
import type { EnrollmentConfiguration, EnrollmentDraft, EnrollmentOptions, SaveEnrollmentDraft } from '../../contracts/enrollment';

type EnrollmentApi = {
  getEnrollmentOptions(id: ClassroomSubjectId): Promise<EnrollmentOptions>;
  getEnrollmentDraft(id: ClassroomSubjectId): Promise<EnrollmentDraft | null>;
  saveEnrollmentDraft(input: SaveEnrollmentDraft): Promise<EnrollmentDraft>;
};
type RecoveryStorage = Pick<Storage, 'getItem' | 'setItem' | 'removeItem'>;
export interface EnrollmentEditorState {
  options: EnrollmentOptions | null;
  draft: EnrollmentDraft | null;
  configuration: EnrollmentConfiguration | null;
  status: 'loading' | 'saved' | 'dirty' | 'saving' | 'error' | 'conflict';
  error: string;
}
const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);

/** One controller per course, retained across navigation. Saves are serialized;
 * recovery retains the expected native revision instead of overwriting a newer
 * draft after restart. Storage failure never masquerades as a successful save. */
export function createEnrollmentEditor(courseId: ClassroomSubjectId, api: EnrollmentApi, storage?: RecoveryStorage) {
  let state: EnrollmentEditorState = { options: null, draft: null, configuration: null, status: 'loading', error: '' };
  const listeners = new Set<(state: EnrollmentEditorState) => void>();
  const cacheKey = `principia:enrollment-recovery:${courseId}`;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let running: Promise<boolean> | null = null;
  let loading: Promise<void> | null = null;
  let editVersion = 0;
  let initialized = false;
  let conflict = false;
  let conflictBase: Pick<SaveEnrollmentDraft, 'id' | 'expected_revision' | 'course'> | null = null;
  let recoveryWarning = '';
  const emit = () => { for (const listener of listeners) listener(structuredClone(state)); };
  const request = (): SaveEnrollmentDraft => ({
    id: state.draft?.id ?? null, expected_revision: state.draft?.revision ?? null,
    course: structuredClone(state.options!.course), ...(conflictBase ?? {}), configuration: structuredClone(state.configuration!),
  });
  function cache() {
    if (!storage) { recoveryWarning = 'A recovery copy is unavailable. Keep this screen open until the draft is saved.'; return; }
    try { storage?.setItem(cacheKey, JSON.stringify(request())); recoveryWarning = ''; }
    catch { recoveryWarning = 'The recovery copy could not be stored. Keep this screen open until the draft is saved.'; }
  }
  function clearCache() { try { storage?.removeItem(cacheKey); } catch { /* Native record remains authoritative. */ } }

  async function load() {
    if (initialized) return;
    if (loading) return loading;
    loading = (async () => {
      try {
        const [options, draft] = await Promise.all([api.getEnrollmentOptions(courseId), api.getEnrollmentDraft(courseId)]);
        state = { options, draft, configuration: structuredClone(draft?.configuration ?? options.default_configuration), status: 'saved', error: '' };
        let recovered: SaveEnrollmentDraft | null = null;
        try {
          const raw = storage?.getItem(cacheKey);
          if (raw) recovered = JSON.parse(raw);
        } catch { throw new Error('The local recovery copy could not be read. Choose saved choices explicitly to discard it.'); }
        if (recovered) {
          if (recovered.course.course_id !== courseId || !recovered.configuration) throw new Error('Invalid recovery copy for this course.');
          if (same(recovered.configuration, draft?.configuration) && same(recovered.course, draft?.course)) clearCache();
          else {
            state.configuration = recovered.configuration;
            conflict = recovered.id !== (draft?.id ?? null) || recovered.expected_revision !== (draft?.revision ?? null) || !same(recovered.course, options.course);
            if (conflict) conflictBase = { id: recovered.id, expected_revision: recovered.expected_revision, course: recovered.course };
            state.status = conflict ? 'conflict' : 'dirty';
            state.error = conflict ? 'Saved choices or the curriculum changed while this edit was unsaved. Choose which version to keep.' : '';
          }
        } else if (draft && !same(draft.course, options.course)) {
          conflict = true;
          conflictBase = { id: draft.id, expected_revision: draft.revision, course: draft.course };
          state.status = 'conflict';
          state.error = 'This draft uses an earlier curriculum. Review your choices before saving them against the current course.';
        }
        initialized = true;
        emit();
        if (state.status === 'dirty') void flush();
      } catch (error) { state.status = 'error'; state.error = String(error); emit(); }
    })().finally(() => { loading = null; });
    return loading;
  }

  function edit(configuration: EnrollmentConfiguration) {
    if (!state.options || !initialized) return;
    state.configuration = structuredClone(configuration);
    editVersion += 1;
    cache();
    state.status = conflict ? 'conflict' : 'dirty';
    if (!conflict) state.error = recoveryWarning;
    emit();
    clearTimeout(timer);
    if (!conflict) timer = setTimeout(() => { void flush(); }, 600);
  }

  function flush(): Promise<boolean> {
    clearTimeout(timer);
    if (running) return running;
    if (!initialized || !state.configuration || conflict) return Promise.resolve(false);
    if (state.status === 'saved' && state.draft) return Promise.resolve(true);
    running = (async () => {
      while (true) {
        const version = editVersion;
        const input = request();
        state.status = 'saving'; emit();
        try {
          const saved = await api.saveEnrollmentDraft(input);
          state.draft = saved;
          if (version === editVersion) {
            state.status = 'saved'; state.error = ''; clearCache(); emit(); return true;
          }
          // An edit arrived in flight. Advance its base revision only after our
          // own write succeeded, then persist the newer choices in order.
          cache();
        } catch (error) {
          state.status = 'error'; state.error = `${String(error)}${recoveryWarning ? ` ${recoveryWarning}` : ''}`;
          emit(); return false;
        }
      }
    })().finally(() => { running = null; });
    return running;
  }

  async function resolve(keepLocal: boolean) {
    if (running) await running;
    try {
      const [options, draft] = await Promise.all([api.getEnrollmentOptions(courseId), api.getEnrollmentDraft(courseId)]);
      state.options = options; state.draft = draft; conflict = false; conflictBase = null; initialized = true;
      if (!keepLocal) {
        state.configuration = structuredClone(draft?.configuration ?? options.default_configuration);
        clearCache();
      }
      // Explicit conflict resolution refreshes the optimistic revision. Native
      // validation still checks course-specific entry points and familiarity.
      state.status = keepLocal ? 'dirty' : 'saved'; state.error = ''; emit();
      if (!keepLocal && draft && !same(draft.course, options.course)) {
        conflict = true; conflictBase = { id: draft.id, expected_revision: draft.revision, course: draft.course };
        state.status = 'conflict'; state.error = 'This saved draft uses an earlier curriculum. Review it before keeping these choices for the current course.'; emit();
      }
      if (keepLocal) { cache(); await flush(); }
    } catch (error) { state.status = 'error'; state.error = String(error); emit(); }
  }

  return {
    load, edit, flush, resolve,
    accepted() {
      clearTimeout(timer); clearCache(); initialized = false; conflict = false; conflictBase = null;
      state = { options: null, draft: null, configuration: null, status: 'loading', error: '' }; emit();
    },
    subscribe(listener: (state: EnrollmentEditorState) => void) {
      listeners.add(listener); listener(structuredClone(state));
      return () => { listeners.delete(listener); };
    },
  };
}
