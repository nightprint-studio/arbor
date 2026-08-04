/**
 * Bennu framework-extension IPC — the one surface every framework plugin speaks through.
 *
 * Deliberately **not** named `spring.ts`. The backend routes these calls through a
 * capability-gated registry (`bennu-ext`), and Spring is simply the first extension
 * registered in it: a second framework adds catalog kinds and highlight kinds, not a
 * second IPC file. The two `bennu_spring_*` calls at the bottom are the exception that
 * proves it — they are settings, and settings are framework-specific by nature.
 *
 * Kept in its own file (not `index.ts`) so concurrent edits to the main bennu IPC surface
 * don't race. Import directly where used:
 *   `import { extGutter, extCatalog } from '$lib/ipc/bennu/ext';`
 *
 * Every offset here is a **UTF-8 byte offset**, like the rest of the bennu contract; the
 * frontend maps them against the buffer it already has.
 */

import { bennu } from '../rpc';

/** A span of framework syntax to colour. `kind` is namespaced (`spring.placeholder.key`);
 *  an unknown kind renders neutrally rather than being dropped, so the backend can add one
 *  without a frontend change. */
export interface ExtHighlight {
  start: number;
  end: number;
  kind: string;
}

/** A place to jump to — a go-to result, a gutter arrow's destination, a catalog row. */
export interface ExtTarget {
  /** Absolute path, forward-slashed. */
  file: string;
  /** Byte offset to place the caret at. */
  offset: number;
  /** What the user picks from when there is more than one. */
  label: string;
  /** Secondary line under the label. May be empty. */
  detail: string;
}

/** A mark in the editor's left gutter. */
export interface ExtGutterMark {
  /** 1-based line. */
  line: number;
  /** Icon key (`bean` | `inject` | `endpoint`); unknown keys render as a neutral dot. */
  kind: string;
  tooltip: string;
  /** Empty = decorative; one = jump; several = a picker. */
  targets: ExtTarget[];
}

/** A hover card contributed by an extension. */
export interface ExtHover {
  title: string;
  signature: string;
  doc: string;
}

/** One row of a catalog — the uniform shape behind every framework list panel. */
export interface ExtEntry {
  id: string;
  primary: string;
  secondary: string;
  /** Short classifier rendered as a badge (`@Service`, `GET`, `<bean>`). */
  kind: string;
  file: string | null;
  offset: number | null;
  line: number | null;
  tags: string[];
  /** Sub-rows the panel can expand — a handler's parameters under its route. Generic, so any
   *  catalog grows detail rows without needing a panel of its own. */
  children: ExtEntry[];
}

/** Something an extension offers to write into the file in front of you.
 *
 *  Contributed rather than enumerated here: which buttons belong on a repository is the
 *  extension's knowledge, and **an action is only ever returned when it applies** — so the
 *  toolbar's contents are the answer to "what kind of file is this", and there is no
 *  disabled-button state to explain. */
export interface ExtAction {
  /** Namespaced by extension id (`jpa.query.count`); sent back when chosen. */
  id: string;
  label: string;
  /** Tooltip — what it will write, in one line. */
  detail: string;
  /** Icon key; unknown keys render without one rather than breaking the row. */
  icon: string;
  /** Empty = a plain button. Non-empty = a dropdown, and the parent is not itself an action. */
  children: ExtAction[];
}

/** A headline number, optionally drilling into a catalog. */
export interface ExtStat {
  label: string;
  value: number;
  catalog: string | null;
}

/** One `application*.yml` / `.properties` the project declares. */
export interface PropertyFileInfo {
  path: string;
  name: string;
  /** Profile from the file name (`dev`), empty for the base file. */
  profile: string;
  keys: number;
}

/** What the frontend needs to decide whether to offer the framework tooling at all. */
export interface ExtOverview {
  /** Ids of the active extensions (`['spring']`); empty → no framework tooling here. */
  extensions: string[];
  ready: boolean;
  stats: ExtStat[];
  property_files: PropertyFileInfo[];
  active_property_file: string | null;
}

/** Spans of framework syntax in a buffer (property placeholders, SpEL, path variables).
 *  Wire: `bennu_ext_highlights`. */
export function extHighlights(file: string, source: string): Promise<ExtHighlight[]> {
  return bennu('bennu_ext_highlights', { args: { file, source } });
}

/** Gutter marks (bean / injection / endpoint) for a buffer. Wire: `bennu_ext_gutter`. */
export function extGutter(file: string, source: string): Promise<ExtGutterMark[]> {
  return bennu('bennu_ext_gutter', { args: { file, source } });
}

/** Go-to targets at a caret. Empty when the caret is on nothing a framework knows about —
 *  which is most of a file, so this is safe to chain after the language's own go-to.
 *  Wire: `bennu_ext_navigate`. */
export function extNavigate(file: string, source: string, offset: number): Promise<ExtTarget[]> {
  return bennu('bennu_ext_navigate', { args: { file, source, offset } });
}

/** Hover card at a caret, or `null`. Wire: `bennu_ext_hover`. */
export function extHover(file: string, source: string, offset: number): Promise<ExtHover | null> {
  return bennu('bennu_ext_hover', { args: { file, source, offset } });
}

/** Framework completion candidates at a caret. Wire: `bennu_ext_completion`. */
export function extCompletion(file: string, source: string, offset: number) {
  return bennu<{ label: string; kind: string; detail: string | null }[]>('bennu_ext_completion', {
    args: { file, source, offset },
  });
}

/** The text that **certainly** follows the caret, drawn as ghost text and accepted with Tab —
 *  a documented default for a key left empty, a prefix exactly one known key can continue.
 *  `null` is the normal answer, and the only alternative to a guess. Wire:
 *  `bennu_ext_inline_hint`. */
export function extInlineHint(
  file: string,
  source: string,
  offset: number,
): Promise<string | null> {
  return bennu('bennu_ext_inline_hint', { args: { file, source, offset } });
}

/** What the active frameworks offer to write into this buffer. Empty on most files, which is
 *  the correct and common answer. Wire: `bennu_ext_actions`. */
export function extActions(file: string, source: string): Promise<ExtAction[]> {
  return bennu('bennu_ext_actions', { args: { file, source } });
}

/** Download the schema an XML document names and cache it; resolves to the local path.
 *
 *  Better than opening the address in a browser for a reason that is not convenience: a
 *  downloaded schema **joins the catalog**, so a `pom.xml` whose grammar was the built-in table
 *  starts being answered by the real Maven schema instead. Fetched only when the user asks —
 *  this is the far end of a ctrl+click, never something a scan does.
 *  Wire: `bennu_xml_fetch_schema`. */
export function xmlFetchSchema(url: string): Promise<string> {
  return bennu('bennu_xml_fetch_schema', { args: { url } });
}

/** A configuration key rendered as the environment variable that overrides it. */
export interface EnvVarView {
  key: string;
  value: string;
  /** The variable name (`SPRING_JPA_SHOWSQL`). */
  name: string;
  /** `[label, text]` pairs — `.env`, shell, `docker run`, compose. */
  forms: [string, string][];
}

/** The environment override for the property on the line at `offset`, or `null` when that line
 *  declares no key. Read-only: nothing is written to the file. Wire: `bennu_spring_env_var`. */
export function springEnvVar(
  file: string,
  source: string,
  offset: number,
): Promise<EnvVarView | null> {
  return bennu('bennu_spring_env_var', { args: { file, source, offset } });
}

/** The rows of one catalog. `kind` may be namespaced by extension id (`spring.beans`) or
 *  bare (`beans`, answered by the first extension that has it). Wire: `bennu_ext_catalog`. */
export function extCatalog(root: string, kind: string): Promise<ExtEntry[]> {
  return bennu('bennu_ext_catalog', { args: { root, kind } });
}

/** Which extensions are active for a project, their headline counts, and the property-file
 *  picker's contents. Wire: `bennu_ext_overview`. */
export function extOverview(root: string): Promise<ExtOverview> {
  return bennu('bennu_ext_overview', { args: { root } });
}

/** Rebuild a project's framework model — after the semantic index lands, or after saving a
 *  file that could change the wiring. Wire: `bennu_spring_refresh`. */
export function extRefresh(root: string): Promise<boolean> {
  return bennu('bennu_spring_refresh', { args: { root } });
}

/** Pin which `application*.yml` the project's `${…}` placeholders resolve against; `null`
 *  clears the pin and falls back to the profile-less files. Persisted per project in the
 *  bennu config. Wire: `bennu_spring_set_property_file`. */
export function setSpringPropertyFile(root: string, file: string | null): Promise<boolean> {
  return bennu('bennu_spring_set_property_file', { args: { root, file } });
}
