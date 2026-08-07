/**
 * The call / type hierarchy — one tree, fetched a level at a time.
 *
 * Both hierarchies live in one store for the same reason they share a wire type: the protocol gives
 * them the same shape, and the question they answer differs only in direction. A call hierarchy walks
 * *who calls this* or *what this calls*; a type hierarchy walks *what this is built on* or *what is
 * built on this*. Four directions, one tree, one panel.
 *
 * ## Why it is lazy, and not a depth limit
 *
 * A level is a request. Fetching the whole tree eagerly is not a slow version of this — it does not
 * terminate: mutual recursion, a trait implemented by a type that uses it, or simply a widely-called
 * helper turns "expand everything" into a sweep of the workspace. So a node's children are fetched
 * the first time it is expanded and then kept, which also means a recursive call chain is
 * *displayable* — you walk into it as far as you care to, and no further.
 *
 * ## The handle is the identity
 *
 * A node's `handle` is the server's own opaque item, sent back verbatim to ask for its children.
 * Re-deriving one from a node's name and position would be asking about something the server never
 * offered — see the wire type. This store therefore never constructs a node; it only ever passes
 * back what it was given.
 */

import { SvelteSet } from 'svelte/reactivity';
import {
  lspPrepareHierarchy, lspHierarchyStep,
  type LspHierarchyDirection, type LspHierarchyNode,
} from '$lib/ipc/bennu/lsp';

/** Which hierarchy is on screen. */
export type HierarchyKind = 'calls' | 'types';

/** One row of the tree. */
export interface HierarchyRow {
  /** Position in the tree (`0/2/1`), stable while the tree stands — the Tree widget's node id. */
  id: string;
  node: LspHierarchyNode;
  /** `null` until this level has been fetched. */
  children: HierarchyRow[] | null;
  loading: boolean;
  /** The server answered, and there was nothing — a leaf we *know* is a leaf, drawn without a
   *  chevron. Distinct from `children === null`, which is "not asked yet". */
  exhausted: boolean;
}

/** The directions each hierarchy offers, in the order the panel shows them. */
export const DIRECTIONS: Record<HierarchyKind, { id: LspHierarchyDirection; label: string }[]> = {
  calls: [
    { id: 'incoming', label: 'Callers' },
    { id: 'outgoing', label: 'Callees' },
  ],
  types: [
    { id: 'subtypes', label: 'Implementors' },
    { id: 'supertypes', label: 'Supertypes' },
  ],
};

function createBennuHierarchyStore() {
  let kind = $state<HierarchyKind>('calls');
  // The default per kind is the question you actually have when you ask. For calls that is "who
  // calls this" — you are reading a function and want to know what depends on it. For types it is
  // "what implements this", which on a Rust trait is the list a reader is looking for; supertypes of
  // a concrete type is a shorter and rarer answer.
  let direction = $state<LspHierarchyDirection>('incoming');
  let roots = $state<HierarchyRow[]>([]);
  /** What the tree is about — the item the caret was on. Shown in the header. */
  let subject = $state<string | null>(null);
  /** Any path inside the workspace: which server answers a step. The file the hierarchy was opened
   *  from, which is also correct when that file is a dependency's source (see `session_covering`). */
  let scope = $state<string | null>(null);
  let loading = $state(false);
  /** Why the tree is empty, when it is empty for a reason worth saying. */
  let message = $state<string | null>(null);
  /** Bumped every time a hierarchy is built, so the panel can take the keyboard even when the same
   *  subject is asked about twice — which a name alone cannot tell apart. */
  let openNonce = $state(0);
  const expanded = new SvelteSet<string>();

  /** Wrap the server's nodes as unfetched rows under `parentId`. */
  function rowsOf(nodes: LspHierarchyNode[], parentId: string): HierarchyRow[] {
    return nodes.map((node, i) => ({
      id: parentId ? `${parentId}/${i}` : String(i),
      node,
      children: null,
      loading: false,
      exhausted: false,
    }));
  }

  /** The row at `id`, or null. Walks the id's own segments — the tree is small and the path is in
   *  the id, so there is nothing to index. */
  function rowAt(id: string): HierarchyRow | null {
    let level = roots;
    let row: HierarchyRow | null = null;
    for (const segment of id.split('/')) {
      const i = Number(segment);
      row = level[i] ?? null;
      if (!row) return null;
      level = row.children ?? [];
    }
    return row;
  }

  /** Discard every fetched level, keeping the roots — what a direction change means. */
  function collapseAll() {
    expanded.clear();
    roots = roots.map((r) => ({ ...r, children: null, loading: false, exhausted: false }));
  }

  /** Fetch `id`'s children if they are not in hand, and mark it expanded. */
  async function expand(id: string) {
    const row = rowAt(id);
    if (!row || !scope) return;
    expanded.add(id);
    if (row.children !== null || row.loading) return;
    row.loading = true;
    try {
      const found = await lspHierarchyStep(scope, row.node.handle, direction);
      row.children = rowsOf(found, id);
      row.exhausted = found.length === 0;
    } catch {
      // A step that failed is not a leaf: leaving `children` null lets it be tried again, which is
      // the right outcome for a server that was busy or restarting.
      row.children = null;
      row.exhausted = false;
    } finally {
      row.loading = false;
    }
  }

  /** How deep an id sits — the sort key that fetches a parent before its children. */
  function depthOf(id: string): number {
    return id.split('/').length;
  }

  return {
    get kind() { return kind; },
    get direction() { return direction; },
    get roots() { return roots; },
    get subject() { return subject; },
    get loading() { return loading; },
    get message() { return message; },
    get expanded() { return expanded; },
    get openNonce() { return openNonce; },
    /** The directions the current hierarchy offers. */
    get directions() { return DIRECTIONS[kind]; },

    /**
     * Build a hierarchy from the caret.
     *
     * `file`/`source`/`offset` are the live buffer and a byte offset in it. An empty answer is not an
     * error — the caret is simply not on something this hierarchy can be built from — and it says so
     * rather than opening an empty tree with no explanation.
     */
    async open(k: HierarchyKind, file: string, source: string, offset: number) {
      kind = k;
      direction = DIRECTIONS[k][0].id;
      scope = file;
      roots = [];
      expanded.clear();
      subject = null;
      message = null;
      loading = true;
      try {
        const found = await lspPrepareHierarchy(file, source, offset, k === 'calls');
        if (found.length === 0) {
          message = k === 'calls'
            ? 'No function or method at the caret.'
            : 'No type or trait at the caret.';
          return;
        }
        roots = rowsOf(found, '');
        subject = found[0].name;
        openNonce += 1;
        // The root's own level is what was asked for, so it is fetched without waiting for a click:
        // a panel that opens showing one row and a chevron has answered nothing yet.
        await expand(roots[0].id);
      } catch {
        message = 'The language server could not build this hierarchy.';
      } finally {
        loading = false;
      }
    },

    /** Expansion, as the Tree widget drives it. */
    toggle(id: string, next: boolean) {
      if (next) void expand(id);
      else expanded.delete(id);
    },

    /** Walk the other way. The roots stay — it is the same subject, asked the opposite question —
     *  and every fetched level is discarded, because those answers were about the old direction. */
    setDirection(d: LspHierarchyDirection) {
      if (d === direction) return;
      direction = d;
      collapseAll();
      const first = roots[0];
      if (first) void expand(first.id);
    },

    /** Re-ask every level currently open. What "the code changed under me" needs.
     *
     *  Shallowest first, because a level can only be attached to a parent that has already been
     *  fetched — asking for `0/2/1` before `0/2` exists would find no row to hang it on. */
    async refresh() {
      const open = [...expanded].sort((a, b) => depthOf(a) - depthOf(b) || a.localeCompare(b));
      collapseAll();
      // Awaited one at a time rather than fired together, which is the same ordering argument: a
      // parent's level has to have LANDED, not merely been requested.
      for (const id of open) await expand(id);
    },

    clear() {
      roots = [];
      expanded.clear();
      subject = null;
      message = null;
      scope = null;
    },
  };
}

export const bennuHierarchyStore = createBennuHierarchyStore();
