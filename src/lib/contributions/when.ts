import type { WhenClause } from '$lib/types/contribution';

/**
 * when-clause matcher for contribution points.
 *
 * Contributions targeting tree-kind sidebars carry an optional `when` object
 * (now a top-level field on PluginContribution after Phase 5 — previously a
 * payload-level convention) that narrows which tree nodes they apply to.
 *
 *   { kind: "module" }
 *   { kind: ["module", "runnable"] }
 *   { kind: "module", data_field: { key: "template_id", value: "maven" } }
 *   { multi: true }   — multi-select context menus only (selectedIds.size > 1)
 *   { multi: false }  — single-select context menus only (default semantics)
 *
 * `multi` not set means the contribution is visible in both single and multi
 * mode (backward-compat default). The host passes `isMulti` through `ctx`
 * when building the multi-mode context menu.
 *
 * The `ctx` argument is typically a TreeNode — only `kind` and `data` fields
 * are accessed. Any unknown/non-object ctx is treated as "always matches".
 */
export type WhenContext =
  | { kind?: unknown; data?: Record<string, unknown> | null; __isMulti?: boolean }
  | unknown;

export function whenMatches(when: WhenClause | undefined, ctx: WhenContext): boolean {
  if (!when || typeof when !== 'object') return true;
  if (!ctx  || typeof ctx  !== 'object') return true;
  const c = ctx as Record<string, unknown>;
  if (when.kind !== undefined) {
    const k        = when.kind;
    const nodeKind = c.kind;
    const ok = Array.isArray(k) ? k.includes(nodeKind as string) : k === nodeKind;
    if (!ok) return false;
  }
  if (when.data_field) {
    const df   = when.data_field;
    const data = c.data as Record<string, unknown> | null | undefined;
    if (data?.[df.key] !== df.value) return false;
  }
  if (typeof (when as { multi?: boolean }).multi === 'boolean') {
    const wantMulti = !!(when as { multi?: boolean }).multi;
    const isMulti   = !!c.__isMulti;
    if (wantMulti !== isMulti) return false;
  }
  return true;
}
