/**
 * transfers store — a small, **window-local** registry of long-running
 * download / export operations that carry *progress* (a percentage or an
 * indeterminate spinner), distinct from the generic background-`jobs` list
 * (which only has running/done state, no percent).
 *
 * It's the model behind the shared `TransfersOverlay`: any feature pushes a
 * transfer with `start()`, streams progress via `update()`, and resolves it
 * with `finish()` / `fail()` / `cancelled()`. Nemus is the first consumer
 * (sample-pack downloads + offline WAV exports), but nothing here is
 * nemus-specific — the store lives in the shared feedback module so any window
 * can adopt it.
 *
 * Unlike `jobs`, transfers are registered client-side (the caller already knows
 * the label + progress source), so there's no backend event stream or
 * cross-window routing — a plain per-window singleton.
 *
 * A transfer reaching a *terminal* state (done / failed) also fires a one-shot
 * notification (bell + transient toast) so the user is told even when the
 * overlay is closed — done centrally here so every consumer (downloads,
 * exports, future kinds) notifies the same way. Cancellations stay silent
 * (user-initiated).
 */

import { notificationsStore } from './notifications.svelte';

export type TransferKind = 'download' | 'export' | 'import';
export type TransferState = 'active' | 'done' | 'failed' | 'cancelled';

export interface Transfer {
  /** Caller-stable id (e.g. a pack id or a render job id). */
  id: string;
  kind: TransferKind;
  /** Primary label (pack name / output filename). */
  label: string;
  /** Optional secondary line (phase text / category). */
  sublabel?: string;
  /** 0..100, or `null` for an indeterminate bar. */
  progress: number | null;
  state: TransferState;
  /** Wall-clock ms when the transfer started (set by the store) — for elapsed
   *  time + a rate-based ETA in the overlay. */
  startedAt: number;
  /** Wall-clock ms when it reached a terminal state — freezes the elapsed time. */
  endedAt?: number;
  /** Failure reason (when `state === 'failed'`). */
  error?: string;
  /** Filesystem target (the installed folder / the exported file). When present
   *  on a finished transfer, the overlay offers a "reveal in file explorer". */
  path?: string;
  /** Cancel handler — when present and active, the overlay shows a Stop button. */
  cancel?: () => void;
}

/** How long a finished (done/cancelled) transfer lingers before self-removing. */
const AUTO_DISMISS_MS = 6000;

function createTransfersStore() {
  let transfers = $state<Transfer[]>([]);
  const timers = new Map<string, ReturnType<typeof setTimeout>>();

  function clearTimer(id: string) {
    const t = timers.get(id);
    if (t) { clearTimeout(t); timers.delete(id); }
  }
  function scheduleDismiss(id: string) {
    clearTimer(id);
    timers.set(id, setTimeout(() => { remove(id); }, AUTO_DISMISS_MS));
  }
  function patch(id: string, fn: (t: Transfer) => Transfer) {
    transfers = transfers.map((t) => (t.id === id ? fn(t) : t));
  }
  function remove(id: string) {
    clearTimer(id);
    transfers = transfers.filter((t) => t.id !== id);
  }
  /** One-shot terminal notification for a transfer reaching done/failed — so the
   *  user learns the outcome even with the overlay closed. */
  function notifyTerminal(t: Transfer, state: 'done' | 'failed') {
    const noun = t.kind === 'download' ? 'Download' : t.kind === 'import' ? 'Import' : 'Export';
    if (state === 'done') {
      const verb = t.kind === 'download' ? 'installed' : t.kind === 'import' ? 'imported' : 'exported';
      notificationsStore.add(`${noun} complete`, `${t.label} ${verb}.`, 'success');
    } else {
      notificationsStore.add(`${noun} failed`, t.error ? `${t.label} — ${t.error}` : t.label, 'error');
    }
  }

  return {
    get transfers() { return transfers; },
    get activeCount() { return transfers.filter((t) => t.state === 'active').length; },
    get finishedCount() { return transfers.filter((t) => t.state !== 'active').length; },
    get hasAny() { return transfers.length > 0; },

    /** Register (or restart) a transfer. A same-id entry is replaced. */
    start(t: Omit<Transfer, 'state' | 'startedAt' | 'endedAt'> & { state?: TransferState }) {
      clearTimer(t.id);
      const entry: Transfer = {
        progress: null,
        ...t,
        state: t.state ?? 'active',
        startedAt: Date.now(),
      };
      transfers = [...transfers.filter((x) => x.id !== t.id), entry];
    },

    /** Stream progress / relabel / attach a path to a transfer. */
    update(id: string, p: Partial<Pick<Transfer, 'progress' | 'sublabel' | 'label' | 'path'>>) {
      patch(id, (t) => ({ ...t, ...p }));
    },

    /** Mark a transfer complete (full bar). A transfer with a revealable `path`
     *  lingers until the user dismisses it (so the "reveal" action stays
     *  available); a pathless one auto-dismisses. */
    finish(id: string, sublabel?: string) {
      // Only the first active→done transition notifies: a pack install can fire
      // both a final 100%-extract event and a terminal job-done, but the second
      // finds the transfer already `done` and stays a no-op.
      const prev = transfers.find((t) => t.id === id);
      if (!prev) return;
      patch(id, (t) => ({ ...t, state: 'done', progress: 100, sublabel: sublabel ?? t.sublabel, endedAt: Date.now() }));
      if (!prev.path) scheduleDismiss(id);
      if (prev.state === 'active') notifyTerminal({ ...prev, state: 'done' }, 'done');
    },
    /** Mark a transfer failed (stays until dismissed). */
    fail(id: string, error?: string) {
      const prev = transfers.find((t) => t.id === id);
      if (!prev) return;
      patch(id, (t) => ({ ...t, state: 'failed', error, endedAt: Date.now() }));
      if (prev.state === 'active') notifyTerminal({ ...prev, state: 'failed', error }, 'failed');
    },
    /** Mark a transfer cancelled (then auto-dismiss). */
    cancelled(id: string) {
      patch(id, (t) => ({ ...t, state: 'cancelled', endedAt: Date.now() }));
      scheduleDismiss(id);
    },

    /** Invoke a transfer's cancel handler (the overlay's Stop button). */
    requestCancel(id: string) {
      transfers.find((t) => t.id === id)?.cancel?.();
    },

    remove,
    /** Drop every non-active transfer. */
    clearFinished() {
      for (const t of transfers) if (t.state !== 'active') clearTimer(t.id);
      transfers = transfers.filter((t) => t.state === 'active');
    },
  };
}

export const transfersStore = createTransfersStore();
