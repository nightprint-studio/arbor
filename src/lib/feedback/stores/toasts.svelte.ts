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
  /** Per-toast auto-dismiss timers, cancelled on manual dismiss so a late timer
   *  can't remove a different toast (ids never repeat, but tracking keeps the
   *  store leak-free and lets `duration <= 0` mean "sticky, no auto-dismiss"). */
  const timers = new Map<string, ReturnType<typeof setTimeout>>();

  function cancelTimer(id: string): void {
    const t = timers.get(id);
    if (t !== undefined) {
      clearTimeout(t);
      timers.delete(id);
    }
  }

  function show(
    message: string,
    kind: ToastKind = 'info',
    duration = 3500,
    action?: ToastAction,
  ): string {
    const id = `toast-${++counter}`;
    toasts.push({ id, kind, message, duration, addedAt: Date.now(), action });
    // A non-positive duration is a sticky toast (dismissed only by the user or a
    // click action) — don't schedule an immediate auto-dismiss.
    if (duration > 0) {
      timers.set(id, setTimeout(() => {
        timers.delete(id);
        dismiss(id);
      }, duration));
    }
    return id;
  }

  function dismiss(id: string) {
    cancelTimer(id);
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
