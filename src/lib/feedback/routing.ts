// ---------------------------------------------------------------------------
// Feedback routing.
//
// Backend events (notifications, jobs, plugin operations) broadcast to every
// window. Each window mounts a <FeedbackHost> with an `id`; the host filters
// incoming items by an optional `target` field so an item only lands in the
// window it was addressed to. The `main` host additionally accepts untagged
// items (`target == null`), so existing emitters that pass no target keep
// their original behavior with no changes.
//
// Filtering happens at INGEST (inside each listener), parameterized by the
// host's predicate — so each window's stores only ever hold its own items and
// the downstream widgets (JobsOverlay, StatusBar, OperationsOverlay, the bell)
// need no routing logic of their own.
// ---------------------------------------------------------------------------

export type TargetAccepts = (itemTarget?: string | null) => boolean;

/** Build the accept-predicate for a host identified by `hostId`. The main host
 *  also accepts untagged items. */
export function makeAccepts(hostId: string, isMain: boolean): TargetAccepts {
  return (itemTarget) =>
    itemTarget == null || itemTarget === ''
      ? isMain
      : itemTarget === hostId;
}

/** Default predicate when no host has configured routing — accepts everything
 *  (preserves pre-routing behavior for any direct caller). */
export const acceptAll: TargetAccepts = () => true;
