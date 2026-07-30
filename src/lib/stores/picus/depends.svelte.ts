/**
 * Picus dependencies — the object graph of the connected schema, held per
 * connection.
 *
 * Read **once** per connection and kept until something says otherwise, for the
 * same reason the schema snapshot is: a graph that silently reloads under you while
 * you are reading an install order off it is worse than a stale one you know is
 * stale. `invalidate` is the only way it goes away, and a re-read of the catalogue
 * is the obvious caller.
 *
 * The walk itself is local. Every question the panel asks — what does this need,
 * what needs this, in what order would these be created — is answered from the one
 * graph already in hand, so expanding a branch is a lookup rather than a round trip.
 *
 * ## The gate
 *
 * `supported` is the engine's own answer (`capabilities.dependencyGraph`), not a
 * guess from the dialect name. The panel does not exist for an engine that has no
 * graph — an absent button is an honest interface, a button that reports "not
 * supported" is a maze.
 */

import {
  dependencies,
  type DependencyEdge,
  type DependencyGraph,
  type DependencyKind,
  type DependencyNode,
} from '$lib/ipc/picus/depends';
import { connectionsStore } from './connections.svelte';
import { picusProvidersStore } from './providers.svelte';

/** The edges of one graph, indexed both ways plus the nodes by name. */
interface GraphIndex {
  byName: Map<string, DependencyNode>;
  /** Keyed by `from`: what that object needs. */
  out: Map<string, DependencyEdge[]>;
  /** Keyed by `to`: what needs that object. */
  in: Map<string, DependencyEdge[]>;
}

const EMPTY_INDEX: GraphIndex = { byName: new Map(), out: new Map(), in: new Map() };

function bucket(map: Map<string, DependencyEdge[]>, key: string, edge: DependencyEdge) {
  const held = map.get(key);
  if (held) held.push(edge);
  else map.set(key, [edge]);
}

function indexOf(graph: DependencyGraph | null): GraphIndex {
  if (!graph) return EMPTY_INDEX;
  const byName = new Map(graph.nodes.map((n) => [n.name, n]));
  const out = new Map<string, DependencyEdge[]>();
  const inbound = new Map<string, DependencyEdge[]>();
  for (const edge of graph.edges) {
    bucket(out, edge.from, edge);
    bucket(inbound, edge.to, edge);
  }
  return { byName, out, in: inbound };
}

/** What a node with no row of its own still is — the far side of an edge into a
 *  schema that was not read. Named rather than dropped, like everything else here. */
function unknownNode(name: string): DependencyNode {
  return { name, kind: 'unknown', schema: '' };
}

/**
 * Why one object needs another, as a sentence rather than an enum value.
 *
 * Exported because two places say it — the edge label in the trees and the reason
 * column of the creation order — and a second phrasing of the same fact is how two
 * panels come to disagree about what an edge means.
 */
export function edgeReason(edge: DependencyEdge): string {
  const via = edge.via ? ` ${edge.via}` : '';
  switch (edge.kind) {
    case 'foreignKey':      return `foreign key${via}`;
    case 'viewSource':      return 'read by the view';
    case 'triggerTable':    return 'trigger on it';
    case 'triggerRoutine':  return 'fires it';
    case 'sequenceDefault': return `default of${via}`;
    case 'routineBody':     return 'used in the body';
    default:                return edge.kind;
  }
}

/** The tone an edge's badge takes. A foreign key is the one that also orders rows,
 *  not just objects, so it is the one that gets the accent. */
export function edgeTone(kind: DependencyKind): 'accent' | 'info' | 'neutral' {
  if (kind === 'foreignKey') return 'accent';
  if (kind === 'viewSource' || kind === 'routineBody') return 'info';
  return 'neutral';
}

/** A creation order, and the part of it that has none. */
export interface CreationOrder {
  /** Dependencies first: every node appears after everything it needs. */
  order: DependencyNode[];
  /**
   * Nodes on a cycle. They are listed after the order and marked, never dropped —
   * a cycle is a real thing to have (two tables referencing each other) and the
   * answer is "these four have to be created in two steps", not silence.
   */
  cyclic: DependencyNode[];
}

/**
 * Everything `root` needs, transitively, in an order that works.
 *
 * Kahn's algorithm over the reachable sub-graph, taking ties alphabetically so two
 * runs on an unchanged schema produce the same list — an order you cannot compare
 * with the last one is an order you cannot review.
 */
function topologicalOrder(index: GraphIndex, root: string): CreationOrder {
  // Reachable set first: the order is about what this object drags in, not about
  // the whole schema.
  const reachable = new Set<string>();
  const stack = [root];
  while (stack.length) {
    const name = stack.pop()!;
    if (reachable.has(name)) continue;
    reachable.add(name);
    for (const edge of index.out.get(name) ?? []) stack.push(edge.to);
  }

  const pending = new Set(reachable);
  const emitted = new Set<string>();
  const order: string[] = [];

  while (pending.size) {
    const ready = [...pending]
      .filter((name) =>
        (index.out.get(name) ?? []).every((e) => !reachable.has(e.to) || emitted.has(e.to)),
      )
      .sort();
    // Nothing is ready and something is left: what remains is a cycle, or hangs off
    // one. Both are the same answer to the user — this part cannot be ordered.
    if (!ready.length) break;
    for (const name of ready) {
      order.push(name);
      emitted.add(name);
      pending.delete(name);
    }
  }

  const resolve = (name: string) => index.byName.get(name) ?? unknownNode(name);
  return {
    order: order.map(resolve),
    cyclic: [...pending].sort().map(resolve),
  };
}

function createDependsStore() {
  /** One graph per connection. Never evicted on its own — see the module note. */
  let graphs = $state<Record<string, DependencyGraph>>({});
  let loading = $state(false);
  let error = $state('');
  /**
   * The connection being read right now.
   *
   * Deliberately not `$state`, exactly as in the schema store: it guards against a
   * second call, and a reactive read of it inside an effect that calls `load` would
   * be one more dependency able to re-trigger it.
   */
  let reading = '';

  /** `engine → can this engine answer a dependency graph`, read once per session. */

  const graph = $derived<DependencyGraph | null>(graphs[connectionsStore.activeId] ?? null);
  const index = $derived(indexOf(graph));

  return {
    get graph() { return graph; },
    get loading() { return loading; },
    get error() { return error; },
    get nodes() { return graph?.nodes ?? []; },
    get unresolved() { return graph?.unresolved ?? []; },
    /** True once the active connection's graph is in hand. */
    get loaded() { return graph !== null; },

    /**
     * Whether this engine has the concept at all.
     *
     * False until the descriptors have been read, so the panel is absent rather
     * than flickering into existence — which is the right way round: a tool window
     * that appears a moment after the window opens is one the user has already
     * decided is not there.
     */
    get supported() {
      // Primes the read itself. The rail button is gated on this getter, and the
      // panel — the only other place that would have asked — cannot render until
      // the button exists, so waiting for it would be waiting for something this
      // answer is the precondition of.
      void picusProvidersStore.load();
      return picusProvidersStore.capabilities(connectionsStore.activeDialect)
        ?.dependencyGraph ?? false;
    },

    /**
     * Make sure the descriptors are in — what `supported` answers from.
     *
     * Delegated to `picusProvidersStore`, which is the single reader. Three stores
     * had grown their own copy of this document along with three different retry
     * rules; the descriptors say what the build supports, so there is one answer
     * and there should be one place holding it.
     */
    loadCapabilities(): Promise<void> {
      return picusProvidersStore.load();
    },

    /**
     * Read the graph of the active connection, once.
     *
     * A second ask while one is in flight is the same question, so it is dropped
     * rather than queued — the schema read's lesson, and the same shape of bug
     * (an effect that re-fires because the condition that started it is still true
     * until the answer lands).
     */
    async load(force = false) {
      const id = connectionsStore.activeId;
      if (!id) return;
      if (!force && graphs[id]) return;
      if (reading === id) return;
      reading = id;
      loading = true;
      error = '';
      try {
        const read = await dependencies(id);
        // The active connection can change while a large catalogue is walked.
        // Filing one database's graph under another's name is the kind of quiet
        // wrongness that gets a script run against the wrong server.
        if (reading !== id) return;
        graphs = { ...graphs, [id]: read };
      } catch (e) {
        if (reading !== id) return;
        error = String(e);
      } finally {
        if (reading === id) {
          reading = '';
          loading = false;
        }
      }
    },

    /** Forget a connection's graph — after a re-read of the catalogue, or a DDL run.
     *  Without an id, forgets the active connection's. */
    invalidate(id?: string) {
      const key = id ?? connectionsStore.activeId;
      if (!key || !(key in graphs)) return;
      const { [key]: _gone, ...rest } = graphs;
      graphs = rest;
    },

    /** Forget everything — on disconnect, so no panel can show a dead schema. */
    clear() {
      graphs = {};
      error = '';
      reading = '';
      loading = false;
    },

    node(name: string): DependencyNode | null {
      return index.byName.get(name) ?? null;
    },

    /**
     * The graph's own spelling of a name, or `''` when it holds no such object.
     *
     * Exact first, then case-insensitively — the same fold every other lookup in
     * Picus applies (PostgreSQL folds, Oracle shouts). Without it, an object
     * reached from a tab that spelled it differently from the catalogue looks like
     * an object with no dependencies at all, which is a wrong answer wearing an
     * empty state.
     */
    matchName(name: string): string {
      if (!name) return '';
      if (index.byName.has(name)) return name;
      const folded = name.toUpperCase();
      for (const key of index.byName.keys()) {
        if (key.toUpperCase() === folded) return key;
      }
      return '';
    },

    /** What `name` needs. */
    dependsOn(name: string): DependencyEdge[] {
      return index.out.get(name) ?? [];
    },

    /** What needs `name`. */
    usedBy(name: string): DependencyEdge[] {
      return index.in.get(name) ?? [];
    },

    /** The node an edge points at, even when it was never listed as an object. */
    resolve(name: string): DependencyNode {
      return index.byName.get(name) ?? unknownNode(name);
    },

    /** Everything `name` drags in, in an order that would create it. */
    creationOrder(name: string): CreationOrder {
      return topologicalOrder(index, name);
    },
  };
}

export const dependsStore = createDependsStore();
