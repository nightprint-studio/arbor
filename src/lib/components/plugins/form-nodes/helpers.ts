/*
 * Pure helpers shared between FormNodeRenderer and its sub-renderers.
 * No state, no Svelte runes — just functions.
 */
import type { DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
import type {
  FormFieldRange, FormSelectOption,
} from '$lib/types/plugin';
import { PLUGIN_ICONS } from '$lib/utils/plugin-icons';

// ── Style helpers ────────────────────────────────────────────────────────

export function gapStr(gap: number | string): string {
  return typeof gap === 'number' ? `${gap}px` : gap;
}

export function containerStyle(n: {
  style?: string; columns?: number | string; gap?: number | string;
}): string {
  const parts: string[] = [];
  if (n.columns !== undefined) {
    const cols =
      typeof n.columns === 'number' ? `repeat(${n.columns}, 1fr)` : n.columns;
    parts.push(`display:grid`, `grid-template-columns:${cols}`);
  }
  if (n.gap !== undefined) parts.push(`gap:${gapStr(n.gap)}`);
  if (n.style) parts.push(n.style);
  return parts.join(';');
}

export function rowStyle(n: {
  style?: string; gap?: number | string; align?: string; wrap?: boolean;
}): string {
  const parts = ['display:flex', 'flex-direction:row'];
  if (n.gap  !== undefined) parts.push(`gap:${gapStr(n.gap)}`);
  if (n.align)              parts.push(`align-items:${n.align}`);
  if (n.wrap)               parts.push('flex-wrap:wrap');
  if (n.style)              parts.push(n.style);
  return parts.join(';');
}

// ── Section body ──────────────────────────────────────────────────────────
// Sections coming from the panel-DSL (PluginSidebarPanel) use `nodes` for
// their body; sections built for the form-DSL use `children`. Plugins that
// share builders between both renderers would otherwise crash. Treat either
// field as the section body.
export function sectionBody(n: any): any[] {
  if (Array.isArray(n.children)) return n.children;
  if (Array.isArray(n.nodes))    return n.nodes;
  return [];
}

// ── Misc primitives ──────────────────────────────────────────────────────

/** Capitalise a short value for display when a bare-string option was supplied. */
export function prettify(s: string): string {
  if (!s) return s;
  return s.charAt(0).toUpperCase() + s.slice(1);
}

/** Lua's empty `{}` is ambiguous; mlua serialises it as a JSON object,
 *  which crashes anything that expects `T[]`. Coerce to an array at the
 *  boundary so consumers can always `.map(...)`. */
export function toArr<T>(v: unknown): T[] {
  return Array.isArray(v) ? (v as T[]) : [];
}

export function treeKey(field: string, value: string): string {
  return `${field}::${value}`;
}

export function fmtRange(n: FormFieldRange, v: number): string {
  return n.value_format ? n.value_format.replace('{v}', String(v)) : String(v);
}

// ── Select / multiselect option helpers ──────────────────────────────────
// Plugins can pass a flat `string[]` / `{value,label}[]` list (legacy) OR
// the richer shape with groups, separators, icons, subtitle and meta.
// These accept both.

export type RawOption =
  | string
  | { value: string; label: string; disabled?: boolean; description?: string };

export function normalizeOptions(raw: RawOption[] | undefined) {
  return (raw ?? []).map(o =>
    typeof o === 'string' ? { value: o, label: prettify(o) } : o
  );
}

function _isSelectGroup(o: any): boolean {
  return o && typeof o === 'object' && typeof o.group === 'string' && Array.isArray(o.items);
}
function _isSelectSeparator(o: any): boolean {
  return o && typeof o === 'object' && o.separator === true;
}

/** Walk a (possibly nested) option list and run `fn` on each leaf item. */
function _walkSelectItems(
  raw: FormSelectOption[] | undefined,
  fn: (it: { value: string; label: string }) => void,
) {
  for (const o of (raw ?? [])) {
    if (o == null) continue;
    if (typeof o === 'string') {
      fn({ value: o, label: prettify(o) });
    } else if (_isSelectSeparator(o)) {
      continue;
    } else if (_isSelectGroup(o)) {
      _walkSelectItems((o as any).items, fn);
    } else {
      const it = o as any;
      fn({ value: it.value, label: it.label ?? prettify(it.value) });
    }
  }
}

/** Flat lookup of `value → label` over the (possibly nested) list. */
export function selectLabelOf(raw: FormSelectOption[] | undefined, value: string): string | undefined {
  let out: string | undefined;
  _walkSelectItems(raw, it => { if (out === undefined && it.value === value) out = it.label; });
  return out;
}

/** Total item count, ignoring groups/separators. */
export function selectItemCount(raw: FormSelectOption[] | undefined): number {
  let n = 0;
  _walkSelectItems(raw, () => { n++; });
  return n;
}

/** Build DropdownItem[] for a `select` or `multiselect` field.
 *  `setValue` is invoked when an item is picked — the dispatcher passes a
 *  function that mutates `values[fieldName]` (and notifies on change for
 *  single-select callers). */
export function buildSelectDropdownItems(
  raw: FormSelectOption[] | undefined,
  fieldName: string,
  multiple: boolean,
  current: unknown,
  setValue: (fieldName: string, multiple: boolean, value: string) => void,
): DropdownItem[] {
  const out: DropdownItem[] = [];
  for (const o of (raw ?? [])) {
    if (o == null) continue;

    if (_isSelectSeparator(o)) {
      out.push({ kind: 'separator', label: (o as any).label });
      continue;
    }
    if (_isSelectGroup(o)) {
      const g = o as any;
      out.push({
        kind: 'group',
        id: `g:${g.group}`,
        label: g.group,
        items: buildSelectDropdownItems(g.items, fieldName, multiple, current, setValue),
        collapsible: !!g.collapsible,
        defaultCollapsed: !!g.default_collapsed,
      });
      continue;
    }

    let value: string;
    let label: string;
    let iconName: string | undefined;
    let subtitle: string | undefined;
    let meta:     string | undefined;
    let disabled = false;

    if (typeof o === 'string') {
      value = o;
      label = prettify(o);
    } else {
      const it = o as any;
      value    = it.value;
      label    = it.label ?? prettify(it.value);
      iconName = it.icon;
      subtitle = it.description;
      meta     = it.meta;
      disabled = !!it.disabled;
    }

    out.push({
      kind:  'item',
      id:    value,
      label,
      icon:  iconName ? PLUGIN_ICONS[iconName] : undefined,
      subtitle,
      meta,
      disabled,
      active: multiple
        ? Array.isArray(current) && (current as string[]).includes(value)
        : current === value,
      onclick: () => setValue(fieldName, multiple, value),
    });
  }
  return out;
}

/** Wrap each leaf item's onclick so it also fires `actions.change` with
 *  `{value}` payload. Recurses into groups. Items unchanged when `action`
 *  is falsy. */
export function wrapSelectChange(
  items:  DropdownItem[],
  action: string | undefined,
  fire:   (action: string, extra: Record<string, unknown>) => void,
): DropdownItem[] {
  if (!action) return items;
  return items.map((it) => {
    if (it.kind === 'group') {
      return { ...it, items: wrapSelectChange(it.items, action, fire) };
    }
    if (it.kind !== 'item') return it;
    const orig = it.onclick;
    return {
      ...it,
      onclick: () => {
        orig?.();
        fire(action, { value: it.id });
      },
    };
  });
}

/** Trigger label for a multiselect field. */
export function multiselectSummary(
  raw: FormSelectOption[] | undefined,
  selected: string[],
  placeholder: string,
): string {
  if (!selected || selected.length === 0) return placeholder;
  if (selected.length === 1) return selectLabelOf(raw, selected[0]) ?? selected[0];
  return `${selected.length} selected`;
}
