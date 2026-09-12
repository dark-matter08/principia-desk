/**
 * The desk's own confirm and prompt: a question the interface asks with its
 * own controls rather than the browser's. `ask` resolves when the learner
 * answers; `DialogHost` draws whatever is being asked.
 */
export interface DialogField {
  label: string;
  placeholder?: string;
  value?: string;
  /** The answer needs some text before it can be confirmed. */
  required?: boolean;
}

export interface DialogRequest {
  title: string;
  message?: string;
  field?: DialogField;
  confirm?: string;
  cancel?: string;
  /** The confirming key does something drastic and is coloured for it. */
  danger?: boolean;
}

export interface DialogAnswer {
  ok: boolean;
  value: string;
}

interface Pending {
  request: DialogRequest;
  resolve: (answer: DialogAnswer) => void;
}

let pending = $state<Pending | null>(null);

export const dialog = {
  get current(): DialogRequest | null {
    return pending?.request ?? null;
  },
  /** Ask, and get the answer once the learner has given it. One at a time. */
  ask(request: DialogRequest): Promise<DialogAnswer> {
    pending?.resolve({ ok: false, value: '' });
    return new Promise((resolve) => {
      pending = { request, resolve };
    });
  },
  answer(ok: boolean, value = '') {
    const current = pending;
    pending = null;
    current?.resolve({ ok, value: value.trim() });
  },
};

/** A yes-or-no question. */
export const confirmDialog = (title: string, message?: string, options: Partial<DialogRequest> = {}) =>
  dialog.ask({ title, message, confirm: 'Confirm', cancel: 'Cancel', ...options }).then((answer) => answer.ok);

/** A question answered with a line of text; null when cancelled. */
export const promptDialog = (title: string, field: DialogField, options: Partial<DialogRequest> = {}) =>
  dialog.ask({ title, field, confirm: 'Save', cancel: 'Cancel', ...options }).then((answer) => (answer.ok ? answer.value : null));
