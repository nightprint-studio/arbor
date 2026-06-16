// ---------------------------------------------------------------------------
// Toast store — transient, per-window pop notifications.
//
// Extracted out of the `uiStore` god-store so the feedback subsystem owns it.
// Toasts are frontend-only and live in the JS context of the window that
// created them, so they need no backend routing: each window has its own
// `toastStore` instance. `uiStore.showToast` / `uiStore.toasts` delegate here,
// keeping the ~600 existing call sites unchanged.
// ---------------------------------------------------------------------------

export type ToastKind = 'info' | 'success' | 'warning' | 'error';

export interface ToastAction {
  label: string;
  /** Side-effect to run when the user clicks the action button. The toast
   *  is dismissed automatically afterwards. Kept as a closure (not data)
   *  because toasts don't survive a reload — for persisted click actions
   *  use `notificationsStore.add(..., action)` instead. */
  onClick: () => void;
}

export interface Toast {
  id: string;
  kind: ToastKind;
  message: string;
  duration: number;
  /** Wall-clock ms when the toast was added.  Used by the unified
   *  bottom-right stack to interleave toasts with notifications in
   *  chronological order. */
  addedAt: number;
  /** Optional clickable action rendered as a button on the right side
   *  of the toast (e.g. "Open" → deep-links to a pipeline run). */
  action?: ToastAction;
}

function createToastStore() {
  let toasts = $state<Toast[]>([]);
  let counter = 0;

  function show(
    message: string,
    kind: ToastKind = 'info',
    duration = 3500,
    action?: ToastAction,
  ): string {
    const id = `toast-${++counter}`;
    toasts.push({ id, kind, message, duration, addedAt: Date.now(), action });
    setTimeout(() => dismiss(id), duration);
    return id;
  }

  function dismiss(id: string) {
    const idx = toasts.findIndex(t => t.id === id);
    if (idx !== -1) toasts.splice(idx, 1);
  }

  return {
    get toasts() { return toasts; },
    show,
    dismiss,
  };
}

export const toastStore = createToastStore();
