// checkoutResultHandler.ts — single source of truth for reacting to a CheckoutResult.
//
// All `checkout_*_safe` backend commands return Ok(CheckoutResult) even when
// the operation ended in a state that needs follow-up (stash apply error,
// stash conflicts, checkout outright failed after a successful stash). Route
// every result through `handleCheckoutResult` so the UX is identical regardless
// of which call site triggered the checkout (sidebar, palette, graph, reflog,
// bisect…).
//
// Mirrors the shape and intent of `pullResultHandler.ts`.

import type { CheckoutResult, StashEntry } from '$lib/types/git';
import { uiStore } from '$lib/stores/ui.svelte';

export interface CheckoutResultContext {
  /** Human-readable label for the target ('main', 'origin/foo', '7a3b9c2', …).
   *  Used verbatim in toasts. */
  targetLabel: string;
  /** Optional override for the success toast ('Checked out main' by default). */
  successMessage?: string;
}

/**
 * Interpret a CheckoutResult, routing the user to the right recovery UI.
 * Returns true on a fully clean checkout — the caller typically shows a
 * success toast (or relies on `successMessage` below).
 */
export function handleCheckoutResult(
  result: CheckoutResult,
  ctx: CheckoutResultContext,
): boolean {
  const { targetLabel } = ctx;

  // ── stash apply failed for a non-conflict reason, OR the checkout itself
  //    failed after the pre-stash was already saved (backend folds both into
  //    `stash_apply_error` — the message prefix tells them apart). ─────────
  if (result.stash_apply_error) {
    const msg = result.stash_apply_error;
    if (msg.startsWith('checkout failed')) {
      uiStore.showToast(
        `Could not switch to '${targetLabel}': ${msg.replace(/^checkout failed:\s*/, '')}. `
        + `Your changes are preserved in the Stash panel.`,
        'error',
        8000,
      );
    } else {
      uiStore.showToast(
        `Checked out '${targetLabel}' — stash re-apply failed: ${msg}. `
        + `Your changes are safe in the Stash panel.`,
        'warning',
        8000,
      );
    }
    return false;
  }

  // ── stash re-apply produced conflicts — open the resolution modal. ──────
  if (result.stash_conflicts.length > 0 && result.pre_checkout_stash) {
    const stash: StashEntry = result.pre_checkout_stash;
    uiStore.openStashConflictModal(stash, result.stash_conflicts);
    uiStore.showToast(
      `Checked out '${targetLabel}' — stash re-applied with ${result.stash_conflicts.length} `
      + `conflict${result.stash_conflicts.length === 1 ? '' : 's'}`,
      'warning',
    );
    return false;
  }

  // ── clean checkout ──────────────────────────────────────────────────────
  // `did_stash` carries the round-trip signal even after the backend drops the
  // stash entry on clean apply, so the toast tells the user their working
  // changes were preserved.
  const base = ctx.successMessage ?? `Checked out '${targetLabel}'`;
  uiStore.showToast(
    result.did_stash ? `${base} (local changes stashed and restored)` : base,
    'success',
  );
  return true;
}
