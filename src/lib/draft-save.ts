export type SaveStatus = 'idle' | 'saving' | 'saved' | 'error';

// Serialize writes across component instances, including a remount while an
// earlier instance is flushing. Owners are namespaced by the caller.
const writes = new Map<string, Promise<void>>();

function browserStorage(): Storage | undefined {
  try {
    return typeof localStorage === 'undefined' ? undefined : localStorage;
  } catch {
    return undefined;
  }
}

export function createDraftSaver(
  ownerKey: string,
  persist: (text: string) => Promise<unknown>,
  status: (value: SaveStatus) => void,
  storage = browserStorage(),
) {
  const cacheKey = `principia:unsaved-exercise:${ownerKey}`;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let pending: string | undefined;
  let revision = 0;

  function recoveredDraft(): string | null {
    try {
      return storage?.getItem(cacheKey) ?? null;
    } catch {
      return null;
    }
  }

  function flush(): Promise<void> {
    clearTimeout(timer);
    if (pending === undefined) return writes.get(ownerKey) ?? Promise.resolve();
    const text = pending;
    const version = revision;
    pending = undefined;
    const operation = (writes.get(ownerKey) ?? Promise.resolve()).then(async () => {
      try {
        await persist(text);
        if (recoveredDraft() === text) {
          try { storage?.removeItem(cacheKey); } catch { /* Native save succeeded. */ }
        }
        if (version === revision) status('saved');
      } catch {
        if (version === revision) {
          pending = text;
          status('error');
        }
      }
    });
    writes.set(ownerKey, operation);
    void operation.then(() => {
      if (writes.get(ownerKey) === operation) writes.delete(ownerKey);
    });
    return operation;
  }

  function schedule(text: string) {
    revision += 1;
    pending = text;
    // Keep failed or interrupted writes recoverable after navigation/restart.
    try { storage?.setItem(cacheKey, text); } catch { /* Still save natively. */ }
    status('saving');
    clearTimeout(timer);
    timer = setTimeout(() => { void flush(); }, 700);
  }

  return { schedule, flush, recoveredDraft };
}
