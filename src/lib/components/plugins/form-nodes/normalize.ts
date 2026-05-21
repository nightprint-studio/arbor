/*
 * Tree-walk utilities — pure functions that traverse a FormNode tree.
 * Shared between FormNodeRenderer (initial setup + `form.replace`) and
 * any sub-renderer that needs to enumerate descendants (e.g. file picker
 * and menu portal lookups in the dispatcher).
 */
import type { FormNode } from '$lib/types/plugin';

// ── Node normalisation ────────────────────────────────────────────────────
// Assign stable IDs to every node that lacks one so sections and keyed
// iterators always have something to reference.

let _nid = 0;

export function normalizeNode(n: FormNode): FormNode {
  const withId: FormNode = n.id ? n : { ...n, id: `_n${_nid++}` } as FormNode;

  if (withId.type === 'switch') {
    const src = withId as any;
    const cases: Record<string, FormNode[]> = {};
    for (const [k, arr] of Object.entries(src.cases ?? {})) {
      cases[k] = (arr as FormNode[]).map(normalizeNode);
    }
    return {
      ...withId,
      cases,
      default: src.default ? src.default.map(normalizeNode) : undefined,
    } as FormNode;
  }

  if (withId.type === 'tabs') {
    const src = withId as any;
    return {
      ...withId,
      tabs: (src.tabs ?? []).map((t: any) => ({
        ...t,
        children: (t.children ?? []).map(normalizeNode),
      })),
    } as FormNode;
  }

  if (withId.type === 'wizard') {
    const src = withId as any;
    return {
      ...withId,
      steps: (src.steps ?? []).map((s: any) => ({
        ...s,
        children: (s.children ?? []).map(normalizeNode),
      })),
    } as FormNode;
  }

  if (withId.type === 'tree_layout') {
    const src = withId as any;
    return {
      ...withId,
      nav_children:        (src.nav_children        ?? []).map(normalizeNode),
      nav_footer_children: (src.nav_footer_children ?? []).map(normalizeNode),
      content_children:    (src.content_children    ?? []).map(normalizeNode),
    } as FormNode;
  }

  // pipeline_editor: `stages` + `operations` are opaque data for the
  // dedicated editor; only `step_detail_form` contains form fields.
  if (withId.type === 'pipeline_editor') {
    const src = withId as any;
    return {
      ...withId,
      step_detail_form: (src.step_detail_form ?? []).map(normalizeNode),
    } as FormNode;
  }

  if (withId.type === 'form_field') {
    const src = withId as any;
    return {
      ...withId,
      children: (src.children ?? []).map(normalizeNode),
      actions:  Array.isArray(src.actions) ? src.actions.map(normalizeNode) : undefined,
    } as FormNode;
  }

  if ('children' in withId && Array.isArray((withId as any).children)) {
    return {
      ...withId,
      children: (withId as any).children.map(normalizeNode),
    } as FormNode;
  }
  return withId;
}

// ── Field collection ──────────────────────────────────────────────────────
// Recursively walk the tree and extract every field's initial value.

export function collectFields(ns: FormNode[]): [string, any][] {
  const acc: [string, any][] = [];
  for (const n of ns) {
    if ('name' in n && n.name) {
      const def = (n as any).default;
      const fallback: any =
        n.type === 'checkbox' || n.type === 'toggle'              ? false
        : n.type === 'number' || n.type === 'range'               ? 0
        : n.type === 'kv_list'                                    ? {}
        : n.type === 'tags'                                       ? []
        : n.type === 'table'                                      ? []
        : n.type === 'tree' && (n as any).multi                   ? []
        : n.type === 'date' || n.type === 'datetime' || n.type === 'time' ? ''
        : n.type === 'filter_bar'                                 ? { search: '', filters: {} }
        : n.type === 'chip_bar' && (n as any).multi               ? []
        : '';
      const seeded = n.type === 'filter_bar'
        ? { search: (def?.search ?? ''), filters: { ...(def?.filters ?? {}) } }
        : (def !== undefined ? def : fallback);
      acc.push([n.name, seeded]);
    }
    if (n.type === 'switch') {
      const s = n as any;
      for (const arr of Object.values(s.cases ?? {})) {
        acc.push(...collectFields(arr as FormNode[]));
      }
      if (s.default) acc.push(...collectFields(s.default));
      continue;
    }
    if (n.type === 'tabs') {
      for (const t of (n as any).tabs ?? []) {
        acc.push(...collectFields(t.children ?? []));
      }
      continue;
    }
    if (n.type === 'wizard') {
      for (const s of (n as any).steps ?? []) {
        acc.push(...collectFields(s.children ?? []));
      }
      continue;
    }
    if (n.type === 'tree_layout') {
      const t = n as any;
      acc.push(...collectFields(t.nav_children     ?? []));
      acc.push(...collectFields(t.content_children ?? []));
      continue;
    }
    if (n.type === 'pipeline_editor') {
      acc.push(...collectFields((n as any).step_detail_form ?? []));
      continue;
    }
    if ('children' in n && Array.isArray((n as any).children)) {
      acc.push(...collectFields((n as any).children));
    }
  }
  return acc;
}

// ── Recursive flatten — yields every descendant, walking through every
// branching node type. Used for global lookups (kv_list rows, autocomplete
// id → node, menu_button id → node, file picker name → node). ────────────

export function flattenAll(n: FormNode): FormNode[] {
  const out: FormNode[] = [n];
  if (n.type === 'switch') {
    const s = n as any;
    for (const arr of Object.values(s.cases ?? {})) {
      for (const c of arr as FormNode[]) out.push(...flattenAll(c));
    }
    if (s.default) for (const c of s.default) out.push(...flattenAll(c));
    return out;
  }
  if (n.type === 'tabs') {
    for (const t of (n as any).tabs ?? []) {
      for (const c of t.children ?? []) out.push(...flattenAll(c));
    }
    return out;
  }
  if (n.type === 'wizard') {
    for (const s of (n as any).steps ?? []) {
      for (const c of s.children ?? []) out.push(...flattenAll(c));
    }
    return out;
  }
  if (n.type === 'tree_layout') {
    const t = n as any;
    for (const c of t.nav_children     ?? []) out.push(...flattenAll(c));
    for (const c of t.content_children ?? []) out.push(...flattenAll(c));
    return out;
  }
  if (n.type === 'pipeline_editor') {
    for (const c of (n as any).step_detail_form ?? []) out.push(...flattenAll(c));
    return out;
  }
  if ('children' in n && Array.isArray((n as any).children)) {
    for (const c of (n as any).children) out.push(...flattenAll(c));
  }
  return out;
}

// ── Per-container maps ────────────────────────────────────────────────────

export function buildCollapseMap(
  ns: FormNode[],
  sectionBody: (n: any) => any[],
): Record<string, boolean> {
  const m: Record<string, boolean> = {};
  const walk = (ns: FormNode[]) => {
    if (!Array.isArray(ns)) return;
    for (const n of ns) {
      if (n.type === 'section') {
        m[n.id!] = (n as any).collapsed ?? false;
        walk(sectionBody(n));
      } else if (n.type === 'switch') {
        const s = n as any;
        for (const arr of Object.values(s.cases ?? {})) walk(arr as FormNode[]);
        if (s.default) walk(s.default);
      } else if (n.type === 'tabs') {
        for (const t of (n as any).tabs ?? []) walk(t.children ?? []);
      } else if (n.type === 'wizard') {
        for (const s of (n as any).steps ?? []) walk(s.children ?? []);
      } else if (n.type === 'tree_layout') {
        const t = n as any;
        walk(t.nav_children     ?? []);
        walk(t.content_children ?? []);
      } else if (n.type === 'pipeline_editor') {
        walk((n as any).step_detail_form ?? []);
      } else if ('children' in n && Array.isArray((n as any).children)) {
        walk((n as any).children);
      }
    }
  };
  walk(ns);
  return m;
}

export function buildActiveTabMap(ns: FormNode[]): Record<string, string> {
  const m: Record<string, string> = {};
  const walk = (ns: FormNode[]) => {
    if (!Array.isArray(ns)) return;
    for (const n of ns) {
      if (n.type === 'tabs') {
        const t = n as any;
        const first = t.tabs?.[0]?.id ?? '';
        m[n.id!] = t.default_tab ?? first;
        for (const tab of t.tabs ?? []) walk(tab.children ?? []);
      } else if (n.type === 'switch') {
        const s = n as any;
        for (const arr of Object.values(s.cases ?? {})) walk(arr as FormNode[]);
        if (s.default) walk(s.default);
      } else if (n.type === 'wizard') {
        for (const s of (n as any).steps ?? []) walk(s.children ?? []);
      } else if (n.type === 'tree_layout') {
        const t = n as any;
        walk(t.nav_children     ?? []);
        walk(t.content_children ?? []);
      } else if (n.type === 'pipeline_editor') {
        walk((n as any).step_detail_form ?? []);
      } else if ('children' in n && Array.isArray((n as any).children)) {
        walk((n as any).children);
      }
    }
  };
  walk(ns);
  return m;
}

export function buildWizardStepMap(ns: FormNode[]): Record<string, string> {
  const m: Record<string, string> = {};
  const walk = (ns: FormNode[]) => {
    if (!Array.isArray(ns)) return;
    for (const n of ns) {
      if (n.type === 'wizard') {
        const w = n as any;
        const first = w.steps?.[0]?.id ?? '';
        m[n.id!] = w.start_step ?? first;
        for (const s of w.steps ?? []) walk(s.children ?? []);
      } else if (n.type === 'tabs') {
        for (const t of (n as any).tabs ?? []) walk(t.children ?? []);
      } else if (n.type === 'switch') {
        const s = n as any;
        for (const arr of Object.values(s.cases ?? {})) walk(arr as FormNode[]);
        if (s.default) walk(s.default);
      } else if ('children' in n && Array.isArray((n as any).children)) {
        walk((n as any).children);
      }
    }
  };
  walk(ns);
  return m;
}
