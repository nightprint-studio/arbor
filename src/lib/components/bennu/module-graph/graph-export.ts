/**
 * The graph, as text you can take somewhere else.
 *
 * ## Who this is for
 *
 * Chiefly a language model: "here is my workspace's shape, now help me reason about it" is a question
 * an LLM answers well and a picture cannot be pasted into. So the formats are chosen for *readers*
 * rather than for round-tripping — there is deliberately **no import**. Nothing here is a save format;
 * the manifests are the truth and this is a description of them.
 *
 * Three, because they are read by three different things:
 *
 * | Format | For |
 * |---|---|
 * | **Markdown** | an LLM, or a person. Prose-shaped, the cheapest in tokens, and it *says* what the numbers mean instead of leaving them to be inferred from field names |
 * | **JSON** | a script. Every field the backend computed, exactly as computed |
 * | **CSV** | a spreadsheet. One row per edge — the part you cannot get by looking |
 *
 * RON would be a fourth spelling of the JSON with nothing to read it: fulcrum's engine consumes RON
 * *content*, not tool output, and no LLM or spreadsheet prefers it. Left out rather than added for
 * symmetry.
 *
 * ## Node **ids**, never indices
 *
 * The wire format numbers its edges' endpoints, which is right for a renderer and useless in a file:
 * an index is meaningless once it leaves the array it indexes, and doubly so for an export that may
 * describe a *subset*. Everything below is written in terms of the module's own name.
 *
 * ## It exports what is on screen
 *
 * Including the filters, and it **says so** in the header. An export that silently described the whole
 * project while the window showed one crate's neighbourhood would be the worst of both: a reader — human
 * or model — has no way to tell, and would draw conclusions about modules that were never in view.
 */

import type { GraphEdge, GraphNode, ModuleGraph } from '$lib/ipc/bennu/deps';
import { moduleWord } from '$lib/ipc/bennu/deps';

export type ExportFormat = 'markdown' | 'json' | 'csv';

/** What was on screen when the export was taken. */
export interface ExportScope {
  /** The project's own name, for the header. */
  project: string;
  /** The node indices that were drawn, or `null` for all of them. */
  only: Set<number> | null;
  /** Whether the edges that do not order a build were included. */
  includesSoft: boolean;
  /** How the solo scope was set, when solo was on. */
  soloScope?: 'both' | 'deps' | 'users';
  /** The module solo was centred on, when it was on. */
  soloOf?: string;
}

/** The file extension each format is written with. */
export const EXPORT_EXT: Record<ExportFormat, string> = {
  markdown: 'md',
  json: 'json',
  csv: 'csv',
};

/** One row of the flattened graph — the shape every format is built from. */
interface Row {
  node: GraphNode;
  /** Ids of the modules it depends on, with the scope each dependency was declared in. */
  out: { id: string; scope: string; optional: boolean; condition: string }[];
  /** Ids of the modules that depend on it. */
  in: string[];
}

/** Flatten the drawn graph into id-keyed rows, in the graph's own (manifest) order. */
function rows(graph: ModuleGraph, edges: GraphEdge[], only: Set<number> | null): Row[] {
  const shown = (i: number) => !only || only.has(i);
  const id = (i: number) => graph.nodes[i]?.id ?? `#${i}`;
  const live = edges.filter((e) => shown(e.from) && shown(e.to));
  return graph.nodes
    .map((node, i) => ({ node, i }))
    .filter(({ i }) => shown(i))
    .map(({ node, i }) => ({
      node,
      out: live
        .filter((e) => e.from === i)
        .map((e) => ({ id: id(e.to), scope: e.scope, optional: e.optional, condition: e.condition })),
      in: [...new Set(live.filter((e) => e.to === i).map((e) => id(e.from)))].sort(),
    }));
}

/** The `## Scope` note every format opens with — what this describes, and what it leaves out. */
function preamble(graph: ModuleGraph, scope: ExportScope, shownCount: number): string[] {
  const words = moduleWord(graph.ecosystem, true);
  const lines = [
    `project: ${scope.project || '(unnamed)'}`,
    `ecosystem: ${graph.ecosystem || 'unknown'}`,
    `${words}: ${shownCount}${scope.only ? ` of ${graph.nodes.length} (filtered)` : ''}`,
  ];
  if (scope.only && scope.soloOf) {
    const which =
      scope.soloScope === 'deps'
        ? 'what it is built on'
        : scope.soloScope === 'users'
          ? 'what depends on it'
          : 'everything connected to it';
    lines.push(`filter: solo on "${scope.soloOf}" — ${which}`);
  }
  if (!scope.includesSoft) {
    lines.push(
      `filter: ${graph.ecosystem === 'cargo' ? 'dev-dependencies' : 'test-scope dependencies'} excluded`,
    );
  }
  if (graph.truncated) {
    lines.push('note: the project has more modules than the graph was built for; this is a prefix');
  }
  lines.push('source: the project manifests, unresolved — see the notes at the end');
  return lines;
}

/** `dev`/`test`-style scopes: real dependencies that do not order the build. */
function soft(ecosystem: string, scope: string): boolean {
  return ecosystem === 'cargo' ? scope === 'dev' : scope === 'test';
}

/**
 * The LLM-shaped export.
 *
 * Deliberately *narrated*. A model handed `{"impact": 9}` has to guess what impact means; handed
 * "changing it rebuilds 9 crates" it does not, and the sentence costs fewer tokens than the schema it
 * replaces. The three lists at the end are the questions people actually ask of a workspace, answered
 * once so a reader does not have to derive them from the edge list.
 */
function toMarkdown(graph: ModuleGraph, edges: GraphEdge[], scope: ExportScope): string {
  const list = rows(graph, edges, scope.only);
  const words = moduleWord(graph.ecosystem, true);
  const word = moduleWord(graph.ecosystem);
  const out: string[] = [];

  out.push(`# Dependency structure — ${scope.project || 'project'}`, '');
  out.push(...preamble(graph, scope, list.length).map((l) => `- ${l}`), '');

  out.push('## How to read this', '');
  out.push(
    `Each ${word} lists what it depends on and what depends on it, both **inside this project only** —`,
    'third-party dependencies are counted, not listed. `layer` is how far above the foundation it sits:',
    `layer 0 depends on no other ${word} here, and a ${word} is one layer above the deepest thing it`,
    'depends on. `rebuilds` is how many modules a change to it reaches transitively — the cost of',
    'touching it. A dependency marked `(dev)` or `(test)` does not order the build.',
    '',
  );

  out.push(`## ${words[0].toUpperCase()}${words.slice(1)}`, '');
  for (const row of list) {
    const n = row.node;
    const facts = [
      n.kind || 'unknown kind',
      `layer ${n.layer}`,
      `rebuilds ${n.impact}`,
      `built on ${n.reach}`,
      `${n.external} third-party`,
    ];
    if (n.in_cycle) facts.push('**in a cycle**');
    out.push(`### ${n.id}`, '');
    out.push(`${facts.join(' · ')}  `);
    if (n.name && n.name !== n.id) out.push(`name: ${n.name}  `);
    out.push(`manifest: ${n.manifest}`, '');
    if (row.out.length) {
      // Merged by target, unlike the machine formats. A crate declared as both a normal and a dev
      // dependency is two rows in the CSV — which is the truth about the manifest — and reads as
      // `nd-render, nd-render (dev)` in a sentence, which is the same crate said twice.
      const merged = new Map<string, { soft: boolean; hard: boolean; marks: Set<string> }>();
      for (const d of row.out) {
        const at = merged.get(d.id) ?? { soft: false, hard: false, marks: new Set<string>() };
        if (soft(graph.ecosystem, d.scope)) at.soft = true;
        else at.hard = true;
        if (d.optional) at.marks.add('optional');
        if (d.condition) at.marks.add(d.condition);
        merged.set(d.id, at);
      }
      const deps = [...merged.entries()]
        .map(([dep, at]) => {
          const marks = [...at.marks];
          // Only worth saying when it is the *only* way the dependency exists, or when it is an
          // addition to an ordinary one — both of which change what the edge means.
          const softName = graph.ecosystem === 'cargo' ? 'dev' : 'test';
          if (at.soft) marks.unshift(at.hard ? `also ${softName}` : `${softName} only`);
          return marks.length ? `${dep} (${marks.join(', ')})` : dep;
        })
        .join(', ');
      out.push(`- depends on: ${deps}`);
    } else {
      out.push(`- depends on: nothing in this project`);
    }
    out.push(row.in.length ? `- used by: ${row.in.join(', ')}` : '- used by: nothing in this project');
    out.push('');
  }

  // The three questions, answered rather than left to be derived.
  const byImpact = [...list].sort((a, b) => b.node.impact - a.node.impact).filter((r) => r.node.impact > 0);
  if (byImpact.length) {
    out.push('## Most expensive to change', '');
    for (const r of byImpact.slice(0, 12)) {
      out.push(`- ${r.node.id} — rebuilds ${r.node.impact}`);
    }
    out.push('');
  }

  const unused = list.filter((r) => !r.in.length && !(r.node.kind || '').includes('bin'));
  if (unused.length) {
    out.push('## Nothing here depends on these', '');
    out.push(
      `Not a verdict: a library published to a registry, and a deployable, both legitimately have no`,
      'internal dependents. In a private workspace it is usually dead code.',
      '',
    );
    for (const r of unused) out.push(`- ${r.node.id} (${r.node.kind || 'unknown kind'})`);
    out.push('');
  }

  if (graph.cycles.length) {
    out.push('## Cycles', '');
    out.push(
      `${graph.ecosystem === 'cargo' ? 'cargo' : 'Maven'} refuses to build these. Each group is a set of`,
      `${words} that all reach each other, which is usually more than the pair the build tool names.`,
      '',
    );
    // Comma-separated and not arrow-joined: the backend reports a strongly-connected *set*, and
    // writing `a → b → c` would invent a specific ring out of a group that may contain several.
    graph.cycles.forEach((ring, at) => {
      const members = ring.map((i) => graph.nodes[i]?.id ?? `#${i}`);
      out.push(`${at + 1}. ${members.join(', ')} — these all reach each other`);
    });
    out.push('');
  }

  out.push('## What this does not say', '');
  out.push(
    'Read from the manifests, so nothing is *resolved*: feature unification, which `cfg(…)`',
    'dependencies a given platform admits, and whether a Maven profile is active are the build tool’s',
    'answers rather than the manifests’. Conditional and optional edges are therefore included and',
    'labelled rather than evaluated.',
    '',
  );
  return out.join('\n');
}

/** The whole graph, every field, ids instead of indices. */
function toJson(graph: ModuleGraph, edges: GraphEdge[], scope: ExportScope): string {
  const list = rows(graph, edges, scope.only);
  const shown = (i: number) => !scope.only || scope.only.has(i);
  const id = (i: number) => graph.nodes[i]?.id ?? `#${i}`;
  const payload = {
    // A one-line explanation inside the file, because an export ends up in a chat window with no
    // context around it and a reader should not have to guess what `impact` counts.
    about:
      'Internal dependency graph of one project, read from its manifests by Bennu. ' +
      'Edges are between the project\'s own modules only; third-party dependencies are counted in ' +
      '`external`. `layer` 0 depends on nothing internal. `impact` is how many modules rebuild when ' +
      'this one changes; `reach` is how many it is built on. Nothing here is resolved by the build tool.',
    project: scope.project,
    ecosystem: graph.ecosystem,
    filtered: !!scope.only,
    includes_non_ordering_dependencies: scope.includesSoft,
    truncated: graph.truncated,
    layers: Math.max(0, ...list.map((r) => r.node.layer + 1)),
    external_total: graph.external_total,
    modules: list.map((r) => ({
      id: r.node.id,
      name: r.node.name,
      kind: r.node.kind,
      manifest: r.node.manifest,
      layer: r.node.layer,
      impact: r.node.impact,
      reach: r.node.reach,
      external: r.node.external,
      in_cycle: r.node.in_cycle,
      depends_on: r.out,
      used_by: r.in,
    })),
    cycles: graph.cycles
      .map((ring) => ring.filter(shown).map(id))
      .filter((ring) => ring.length > 1),
  };
  return `${JSON.stringify(payload, null, 2)}\n`;
}

/** Wrap a CSV field only when it needs it, and double any quote inside it. */
function cell(value: string): string {
  return /[",\n]/.test(value) ? `"${value.replace(/"/g, '""')}"` : value;
}

/**
 * One row per edge — the graph as a table.
 *
 * The edges rather than the modules, because the modules' numbers are all on screen already and this is
 * the part you cannot read off the window. Ends with the isolated modules as rows with an empty `to`, so
 * a `lib` nothing depends on does not vanish from a file that claims to describe the project.
 */
function toCsv(graph: ModuleGraph, edges: GraphEdge[], scope: ExportScope): string {
  const list = rows(graph, edges, scope.only);
  const lines = ['from,to,scope,orders_build,optional,condition,in_cycle'];
  for (const row of list) {
    if (!row.out.length && !row.in.length) {
      lines.push([cell(row.node.id), '', '', '', '', '', ''].join(','));
      continue;
    }
    for (const d of row.out) {
      lines.push(
        [
          cell(row.node.id),
          cell(d.id),
          cell(d.scope),
          soft(graph.ecosystem, d.scope) ? 'false' : 'true',
          d.optional ? 'true' : 'false',
          cell(d.condition),
          row.node.in_cycle && graph.nodes.find((n) => n.id === d.id)?.in_cycle ? 'true' : 'false',
        ].join(','),
      );
    }
  }
  return `${lines.join('\n')}\n`;
}

/** Render the drawn graph in `format`. */
export function exportGraph(
  format: ExportFormat,
  graph: ModuleGraph,
  edges: GraphEdge[],
  scope: ExportScope,
): string {
  if (format === 'json') return toJson(graph, edges, scope);
  if (format === 'csv') return toCsv(graph, edges, scope);
  return toMarkdown(graph, edges, scope);
}

/** A filename for the export: the project, what it is, and the extension. */
export function exportFilename(format: ExportFormat, project: string, filtered: boolean): string {
  const base = (project || 'project').replace(/[^\w.-]+/g, '-').replace(/^-+|-+$/g, '') || 'project';
  return `${base}-graph${filtered ? '-solo' : ''}.${EXPORT_EXT[format]}`;
}
