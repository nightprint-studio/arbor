/**
 * Module-level reactive store for `tabs` widgets keyed by `persist_key`.
 *
 * Why: multiple `tabs` FormNodes inside the same modal — typically one in
 * `header.centre` with `strip_only = true` (the visual switcher) and one in
 * `nodes` with the actual panel content — live in DIFFERENT `FormNodeRenderer`
 * instances. Each renderer keeps its own `ctx.activeTabMap[node.id]`, so
 * without a shared channel they'd drift the moment the user clicks a tab.
 *
 * This store solves it: every tabs widget whose `persist_key` is set reads
 * its active id from this map; the first call falls back to localStorage,
 * and the first call to `setPersistedActiveTab` populates the reactive slot.
 * From that point on, every consumer of the same key re-renders when one
 * widget writes — strip ↔ body stay in lock-step within the same window,
 * and the choice survives a close + reopen via localStorage.
 *
 * Important: reads (`getPersistedActiveTab`) do NOT mutate the `$state` map.
 * Mutating during another component's render would be either a no-op (with
 * a warning) or a re-render loop in Svelte 5. The first write happens in
 * `setPersistedActiveTab`, which is only called from user interaction
 * (`onSelect`) — safely outside the render phase.
 *
 * Keys without a `persist_key` keep the legacy per-renderer behaviour
 * (`ctx.activeTabMap`); this store is opt-in by setting `persist_key`.
 */

let persistKeyToActiveId = $state<Record<string, string>>({});

/**
 * Read the active tab id for a given `persist_key`. Reactive: subscribers
 * to this slot re-render when `setPersistedActiveTab` writes to it.
 *
 * Resolution order on FIRST access (when the slot is empty):
 *   1. `localStorage[persist_key]`, if present.
 *   2. The `fallback` arg (typically `default_tab` or first tab id).
 *   3. `null`.
 *
 * No side-effects: this never writes to the `$state` map or localStorage,
 * so it's safe to call during render.
 */
export function getPersistedActiveTab(
  persistKey: string,
  fallback: string | null = null,
): string | null {
  // Reactive read — once a write lands here, all consumers re-render.
  const live = persistKeyToActiveId[persistKey];
  if (typeof live === 'string' && live !== '') return live;
  // First-touch fallback path: read localStorage directly, no $state write.
  if (typeof window !== 'undefined') {
    try {
      const stored = window.localStorage.getItem(persistKey);
      if (stored != null && stored !== '') return stored;
    } catch { /* ignore */ }
  }
  return fallback;
}

/**
 * Write the active tab id for a given `persist_key`. Both the in-memory
 * `$state` slot and `localStorage` are updated, so:
 *   - sibling widgets in the same window receive the change reactively;
 *   - re-opening the modal (or another tabs widget mounting later) finds
 *     the user's pick in localStorage and initialises to the same value.
 *
 * Safe to call from event handlers (`onSelect`), `$effect`, or any code
 * path that runs OUTSIDE render. Must not be called from `{@const ...}` /
 * template expressions.
 */
export function setPersistedActiveTab(persistKey: string, id: string): void {
  persistKeyToActiveId[persistKey] = id;
  if (typeof window !== 'undefined') {
    try { window.localStorage.setItem(persistKey, id); } catch { /* ignore */ }
  }
}
