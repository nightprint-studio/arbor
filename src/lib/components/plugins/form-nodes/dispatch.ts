/*
 * Dispatch seam — the single point every actionable node slot routes through.
 *
 * Every slot (button `action`, menu option, header action, select `change`,
 * future per-node events) resolves to a `DispatchTarget`. A bare `action`
 * string is sugar for `{ kind: 'action', name }`; richer slots may carry a
 * `DispatchTarget` object directly. `toDispatchTarget` normalises both shapes
 * so callers never branch on the legacy string form themselves.
 *
 * The executor lives in `FormNodeRenderer` (it needs the live plugin name +
 * payload), exposed on the rendering ctx as `dispatch`. Only `kind: 'action'`
 * is wired today; `kind: 'command'` lands with command invocation.
 */
import type { DispatchTarget } from '$lib/types/plugin';

/**
 * Desugar a slot value into a `DispatchTarget`.
 *   - `undefined` / `null` / `''` → `null` (no-op slot)
 *   - a non-empty string         → `{ kind: 'action', name }`
 *   - an object                  → returned unchanged
 */
export function toDispatchTarget(
  slot: string | DispatchTarget | undefined | null,
): DispatchTarget | null {
  if (slot == null) return null;
  if (typeof slot === 'string') return slot ? { kind: 'action', name: slot } : null;
  return slot;
}
