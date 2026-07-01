import type { IconComponent } from '$lib/types/icon';
import { appearanceStore } from '$lib/stores/appearance.svelte';

/**
 * The accent palette a parked chip can be tinted with. These map 1:1 to the
 * Arbor state tokens (`--accent`, `--success`, …) — the dock resolves the
 * variant to its CSS custom property. Accent is used for TYPE/identity here
 * (so distinct dialogs are visually separable), not as random decoration.
 */
export type ParkedAccent = 'accent' | 'success' | 'warning' | 'danger' | 'info';

/** Ordered palette used by {@link accentForTitle} to derive a stable accent
 *  from a chip's title when the caller doesn't specify one. */
export const PARKED_ACCENTS: readonly ParkedAccent[] = [
  'accent',
  'info',
  'success',
  'warning',
  'danger',
] as const;

/**
 * Derive a stable, well-distributed accent from an arbitrary string (usually
 * the chip title, or an explicit `kind` key). Same input → same accent across
 * sessions, so a given dialog always parks under the same colour even without
 * explicit metadata. FNV-1a keeps the hash cheap and dependency-free.
 */
export function accentForTitle(key: string): ParkedAccent {
  let hash = 0x811c9dc5;
  for (let i = 0; i < key.length; i++) {
    hash ^= key.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193);
  }
  const idx = (hash >>> 0) % PARKED_ACCENTS.length;
  return PARKED_ACCENTS[idx];
}

/**
 * One entry in the parked-modals dock.
 *
 * Unlike the original "modal stays mounted, display:none" approach, parked
 * entries are now ACTION RECORDS: the modal is actually closed at minimize
 * time, and `execute` knows how to re-open it from scratch (switching to
 * the right tab first, then dispatching the open-detail flow). This is
 * what makes the entries survive workspace / tab switches — the modal
 * lifecycle is decoupled from the chip lifecycle.
 *
 * Tradeoff: state local to the modal (scroll position, unsubmitted
 * comments, fetched detail data) is lost across minimize → restore.
 * Workflow continuity is preserved; ephemeral input is not.
 */
export interface ParkedModalEntry {
  id:       string;
  title:    string;
  /** Optional Lucide icon component shown to the left of the title. */
  icon?:    IconComponent;
  /** Optional short subtitle rendered under the title (e.g. source tab
   *  name, repo, "Merge request"). Purely informational. */
  subtitle?: string;
  /** Optional type/category key. When `accent` is not given, the accent is
   *  derived deterministically from this (or, failing that, the title) so
   *  entries of the same kind share a colour. Callers that want an explicit
   *  colour set {@link accent} directly. */
  kind?:    string;
  /** Explicit accent colour for the chip. Overrides the title/kind-derived
   *  accent. Use for type identity, never for random decoration. */
  accent?:  ParkedAccent;
  /** Re-open the modal from scratch. May switch tabs / open closed
   *  projects from the registry; returns a Promise so the chip can show
   *  a spinner while async work runs. May throw — the dock catches and
   *  toasts. */
  execute:  () => void | Promise<void>;
}

function createParkedModalsStore() {
  let entries = $state<ParkedModalEntry[]>([]);

  return {
    get entries() { return entries; },
    get count()   { return entries.length; },

    /** Attempt to park a modal. Returns `false` when the user-configured
     *  cap has been reached and the entry was rejected — the caller is
     *  expected to show a toast and leave the modal open. Re-parking
     *  an existing id (same modal minimized twice) always succeeds. */
    park(entry: ParkedModalEntry): boolean {
      const alreadyParked = entries.some(e => e.id === entry.id);
      if (!alreadyParked && entries.length >= appearanceStore.parkedModalsMax) {
        return false;
      }
      // Replace any prior entry with the same id so a re-park after a
      // remount doesn't leave a duplicate chip behind. Push to the end so
      // newly-parked items appear on the right (LRU at the head).
      entries = [...entries.filter(e => e.id !== entry.id), entry];
      return true;
    },

    /** Drop an entry without running its action. Used by the chip's ✕
     *  button and by the restore path (after `execute` succeeds, the
     *  modal is up — the chip can go). */
    unpark(id: string) {
      entries = entries.filter(e => e.id !== id);
    },
  };
}

export const parkedModalsStore = createParkedModalsStore();

/** Resolve the accent a chip should render with: explicit `accent` wins,
 *  otherwise a stable hash of `kind` (preferred) or `title`. */
export function resolveParkedAccent(entry: ParkedModalEntry): ParkedAccent {
  return entry.accent ?? accentForTitle(entry.kind ?? entry.title);
}
