/**
 * Bennu index-inspector IPC — per-kind entry listing for the Index inspector modal.
 *
 * Kept in its own file (not `index.ts`) so concurrent edits to the main bennu IPC
 * surface don't race. Import directly where used:
 *   `import { indexEntries } from '$lib/ipc/bennu/inspect';`
 *
 * Routes through the generic `bennu(...)` rpc bridge to `bennu-be`, wrapping its
 * fields under `{ args: … }` (the proven convention — the RPC seam keys params by the
 * handler's single `args` parameter; the inner field names are the handler struct's
 * fields in snake_case, forwarded verbatim inside the opaque `params`).
 *
 * ── FE SEAM (BE contract not implemented yet) ────────────────────────────────────
 * The `Types` kind already has a real endpoint (`bennu_class_index`), consumed
 * directly by the modal. The OTHER kinds (members / jars / jdk / beans / actions /
 * relations) need ONE generic entry endpoint the BE must add:
 *
 *   handler:  bennu_index_entries
 *   args:     IndexEntriesArgs { root: String, kind: String }   // kind = one of
 *             "members" | "jars" | "jdk" | "beans" | "actions" | "relations"
 *   returns:  Vec<IndexEntry>   (the snake_case struct below)
 *
 * Until that handler exists the call rejects and the modal degrades to a clear
 * "not available yet / building" state per kind (never blocks, never throws to the
 * user). A `MOCK_INDEX_ENTRIES` seam flag (below) can flip this file to serve
 * in-memory fixtures for FE-only iteration without the BE.
 */

import { bennu } from '../rpc';

/** The inspectable index kinds (the same set shown in the headline stat cards).
 *  `types` is served by `bennu_class_index`; the rest by `bennu_index_entries`. */
export type IndexKind =
  | 'types'
  | 'members'
  | 'jars'
  | 'jdk'
  | 'beans'
  | 'actions'
  | 'relations';

/** One generic index entry (`bennu_index_entries`) — a uniform row shape across all
 *  non-`types` kinds. Mirrors the BE `IndexEntry` (to be added to `bennu-proto`);
 *  fields are snake_case to match the wire contract field-for-field.
 *
 *  The shape is deliberately generic so a single virtualized+filterable list renders
 *  every kind: a required `primary` label + `secondary` detail (both searched), plus
 *  an OPTIONAL openable location (`file` + 1-based `line`) present only for entries
 *  that map to a source site (a bean class, a struts action's config fragment, a
 *  member's declaring file). Entries without a `file` render as non-openable rows. */
export interface IndexEntry {
  /** Primary label — the entry's name. Searched. Examples per kind:
   *   members → simple member name (`getOrder`); jars → jar filename (`struts2-core-2.5.30.jar`);
   *   jdk → the classpath/module label (`java.base`); beans → bean id (`orderService`);
   *   actions → action qualified name (`/do/Category/viewTree`); relations → the edge
   *   label (`orderService → OrderDao`). */
  primary: string;
  /** Secondary detail — fqcn / path / owner / target. Also searched. Examples:
   *   members → owning type fqcn + signature; jars → absolute jar path; jdk → version /
   *   source; beans → bean class fqcn; actions → resolved class fqcn; relations → the
   *   relation kind (`bean-ref` | `action-view` | …). May be empty. */
  secondary: string;
  /** Absolute path (forward slashes) of an openable source site, or `null` when the
   *  entry has no navigable location (a jar, a JDK module, a member with no source). */
  file: string | null;
  /** 1-based line to jump to when `file` is set; `null` otherwise. */
  line: number | null;
}

// ── MOCK seam ────────────────────────────────────────────────────────────────────
// Flip to `true` to render in-memory fixtures instead of hitting `bennu_index_entries`
// (FE-only iteration before the BE handler lands). Keep `false` for the real seam:
// when the handler is missing the RPC rejects and the modal shows the graceful
// "not available yet" state — which is the intended pre-BE behaviour.
const MOCK_INDEX_ENTRIES = false;

const MOCK_FIXTURES: Partial<Record<IndexKind, IndexEntry[]>> = {
  jdk: [{ primary: 'Language level', secondary: '1.8 · maven.compiler.source', file: null, line: null }],
};

/** List every index entry of `kind` for the project at `root`. `kind` is one of the
 *  non-`types` {@link IndexKind}s (`types` is served by `bennu_class_index`). Resolves
 *  to `[]` for an empty set; REJECTS when the BE handler isn't present yet — the caller
 *  catches and shows the "not available yet / building" state, so this never blocks.
 *  Wire: `bennu_index_entries` — `IndexEntriesArgs { root, kind }`. */
export function indexEntries(root: string, kind: IndexKind): Promise<IndexEntry[]> {
  if (MOCK_INDEX_ENTRIES) {
    return Promise.resolve(MOCK_FIXTURES[kind] ?? []);
  }
  return bennu('bennu_index_entries', { args: { root, kind } });
}

// ── encoding report (non-compliant source files) ─────────────────────────────────

/** One source file whose bytes weren't valid in the project's declared (Maven
 *  `sourceEncoding`) encoding — recovered + indexed anyway, but flagged. Mirrors the BE
 *  `EncodingIssue` (snake_case, field-for-field). The seam for a future "non-compliant
 *  files" UI: Bennu never silently drops such a file, and this lists the ones that need
 *  their real encoding sorted out. */
export interface EncodingIssue {
  /** Absolute path (forward slashes) of the non-compliant source. */
  file: string;
  /** The encoding the project declared (and that didn't fit the bytes), e.g. `"Cp1252"`. */
  declared_encoding: string;
  /** The encoding actually used to recover the text (`"UTF-8"` / `"windows-1252"`). */
  decoded_as: string;
}

/** List the source files whose bytes weren't valid in the project's declared encoding for
 *  the project at `root` (recovered + indexed, but flagged). Resolves to `[]` when every
 *  file was compliant, the index hasn't built, or no project owns `root`.
 *  Wire: `bennu_encoding_report` — `EncodingReportArgs { root }`. */
export function encodingReport(root: string): Promise<EncodingIssue[]> {
  return bennu('bennu_encoding_report', { args: { root } });
}

// ── JDK status (titlebar / Problems diagnostics) ─────────────────────────────────

/** How the project's JDK resolved — mirrors the BE `JdkStatus`. Drives the JDK diagnostics:
 *  a titlebar warning when `!any_installed`, a Problems entry when installed but `!exact`
 *  (a fallback JDK is standing in for the level the project targets). */
export interface JdkStatus {
  /** The Java language level the project targets (`null` if unparseable). */
  requested_major: number | null;
  /** Absolute path (forward slashes) of the JDK home that would be used, or `null` when none
   *  is installed. */
  resolved_home: string | null;
  /** The language level of the resolved JDK, if any. */
  resolved_major: number | null;
  /** True when a JDK of the exact requested level was found (no fallback). */
  exact: boolean;
  /** True when at least one JDK is installed. */
  any_installed: boolean;
}

/** Resolve the JDK status for the project at `root`. `null` when no project owns `root`.
 *  Wire: `bennu_jdk_status` — `JdkStatusArgs { root }`. */
export function jdkStatus(root: string): Promise<JdkStatus | null> {
  return bennu('bennu_jdk_status', { args: { root } });
}
