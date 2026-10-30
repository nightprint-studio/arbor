/**
 * Filtering a flame graph, which is not the same as filtering a table.
 *
 * A row can simply be left out; a box cannot, because the boxes nest and the ones below it would be left hanging.
 * So the filter works on the **stacks**: every frame that does not pass is removed from the stack it is in, and the
 * frames that remain close up around the gap. `Tomcat → Spring → MyService → HikariPool → JDBC` filtered to
 * `MyService` becomes `MyService`, with the time of everything it called kept as its own self time — which is what
 * "show me my code" means, and why the graph then reads as a graph of the project rather than of the framework.
 *
 * The root keeps the whole recording's weight, so what the filter hides shows as the root's own remainder and the
 * widths stay honest: a box that is 10% wide is 10% of the recording, not 10% of what survived.
 */

export interface FlameLike {
  name: string;
  value: number;
  self?: number;
  meta?: unknown;
  children?: FlameLike[];
}

const classOf = (n: FlameLike): string => (n.meta as { class?: string } | undefined)?.class ?? '';
const kindOf = (n: FlameLike): string => (n.meta as { kind?: string } | undefined)?.kind ?? '';
const keyOf = (n: FlameLike): string => `${n.name}\u0000${kindOf(n)}`;

/** Merge siblings that became equal once the frames between them and their parent were removed. */
function mergeSiblings(nodes: FlameLike[]): FlameLike[] {
  const byKey = new Map<string, FlameLike>();
  for (const n of nodes) {
    const prev = byKey.get(keyOf(n));
    if (!prev) {
      byKey.set(keyOf(n), { ...n, children: [...(n.children ?? [])] });
      continue;
    }
    prev.value += n.value;
    prev.children = mergeSiblings([...(prev.children ?? []), ...(n.children ?? [])]);
  }
  return [...byKey.values()].sort((a, b) => b.value - a.value);
}

/** What a node becomes: itself with its lifted children when it passes, otherwise just those children. */
function lift(n: FlameLike, keep: (cls: string) => boolean): FlameLike[] {
  const kids = mergeSiblings((n.children ?? []).flatMap((c) => lift(c, keep)));
  const cls = classOf(n);
  // A node with no class (a thread, the root) is structure, not code: it is never the reason to hide anything.
  if (cls !== '' && !keep(cls)) return kids;
  const below = kids.reduce((s, k) => s + k.value, 0);
  return [{ ...n, children: kids, self: Math.max(0, n.value - below) }];
}

/** The graph with only the frames `keep` accepts. The root keeps its full value; its remainder is what was hidden. */
export function filterFlame(root: FlameLike, keep: (cls: string) => boolean): FlameLike {
  const kids = mergeSiblings((root.children ?? []).flatMap((c) => lift(c, keep)));
  const below = kids.reduce((s, k) => s + k.value, 0);
  return { ...root, children: kids, self: Math.max(0, root.value - below) };
}

/** The share of the recording the filter hid, 0..100 — the root's remainder against its value. */
export function hiddenShare(filtered: FlameLike): number {
  return filtered.value > 0 ? ((filtered.self ?? 0) * 100) / filtered.value : 0;
}
