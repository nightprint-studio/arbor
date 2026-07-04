/**
 * Bennu navigation IPC — go-to-definition + live re-index wrappers.
 *
 * Kept in its own file (not `index.ts`) so concurrent edits to the main bennu IPC
 * surface don't race. Import these directly where used:
 *   `import { definition, didChange } from '$lib/ipc/bennu/nav';`
 *
 * Both route through the generic `bennu(...)` rpc bridge to `bennu-be`, wrapping
 * their fields under `{ args: … }` (the proven convention — the seam keys params by
 * the handler's single `args` parameter, and the inner fields are the handler
 * struct's fields in snake_case). Wire shapes below match the BE handlers verbatim
 * (`crates/products/bennu/be/src/intel.rs`).
 */

import { bennu } from '../rpc';

/** A resolved go-to-definition target for a JSP form/link action reference — mirrors
 *  the BE `DefinitionResult` (intel.rs). `bennu_definition` returns `null` when no
 *  project owns the file, the config graph isn't built yet, or the action is unknown. */
export interface DefinitionResult {
  /** Absolute path to the struts config fragment the `<action>` is declared in
   *  (the primary openable go-to-definition target). */
  config_file: string;
  /** Byte offset of the `<action>` element in `config_file` — jump here so go-to lands on
   *  the declaration line, not the top of the file. */
  config_offset: number;
  /** The resolved implementation class FQCN (the C1 chain), if resolvable. A name,
   *  not a path — shown for context, not directly openable. */
  class_fqcn: string | null;
  /** The resolved view JSP (the Tiles chain), if resolvable — a JSP path. */
  view_jsp: string | null;
}

/** Resolve a Spring **bean id** (a struts `<action class="beanId">` value under the caret in
 *  a config XML) to its implementation class FQCN — the FE then opens that class from the
 *  index. `null` when no project owns the file, the config isn't built, or the id names no
 *  known bean (caller then treats the value as an FQCN directly).
 *  Wire: `bennu_bean_class` — `{ file, name }`. */
export function beanClass(file: string, name: string): Promise<string | null> {
  return bennu('bennu_bean_class', { args: { file, name } });
}

/** Resolve a JSP form/link **action reference** (`/do/Category/viewTree`, or a bare
 *  `bando-search`) to its definition target. `file` is any file inside the owning
 *  project (used to pick the project's config graph); `action` is the action
 *  qualified name / reference under the caret. Resolves to `null` gracefully when no
 *  target exists (no project, config still building, unknown action).
 *  Wire: `bennu_definition` — `DefinitionArgs { file, action }`. */
export function definition(file: string, action: string): Promise<DefinitionResult | null> {
  return bennu('bennu_definition', { args: { file, action } });
}

/** A resolved go-to-declaration target for a Java symbol under the caret — mirrors the
 *  BE `DeclarationTarget`. Byte offsets of the declaration NAME token, plus a 1-based
 *  line/col (computed BE-side so the FE just opens `file` and jumps to `line`). */
export interface DeclarationTarget {
  /** Absolute path (forward slashes) of the declaring `.java` file. */
  file: string;
  /** Start byte offset of the declaration name token. */
  start: number;
  /** End byte offset (exclusive). */
  end: number;
  /** 1-based line of the declaration. */
  line: number;
  /** 1-based column of the declaration name. */
  col: number;
  /** Human label of the target (`"method com.x.Foo.bar()"`, `"field count"`, …). */
  label: string;
}

/** Resolve the Java symbol at `file`:`offset` (UTF-8 byte offset) to its declaration
 *  site. `source` is the current (possibly-unsaved) buffer — the caret is classified
 *  against it. Resolves to `null` gracefully when the caret isn't on a resolvable symbol,
 *  the declaration lives in a JDK / dependency jar (no project source), or the index is
 *  still building. Wire: `bennu_declaration` — `DeclarationArgs { file, source, offset }`. */
export function declaration(
  file: string,
  source: string,
  offset: number,
): Promise<DeclarationTarget | null> {
  return bennu('bennu_declaration', { args: { file, source, offset } });
}

/** Live-edit re-index: hand the BE the edited file's full text so it patches the
 *  persisted index (completion / definition then reflect the edit without reopening
 *  the project). `text === null` signals the file was deleted. Returns `true` when a
 *  project owns the file (the patch ran), `false` otherwise. Runs on the BE blocking
 *  pool — never blocks typing on this side (call it debounced / on save).
 *  Wire: `bennu_did_change` — `DidChangeArgs { file, text? }`. */
export function didChange(file: string, text: string | null): Promise<boolean> {
  return bennu('bennu_did_change', { args: { file, text } });
}

/** Invalidate + rebuild the whole semantic index for the project at `root` (BE
 *  `bennu_reindex`): drops the class cache / symbol index / config resolver / rename engine
 *  / completion provider and rebuilds them from a fresh source scan off-thread, emitting
 *  `arbor://bennu/index-progress` like an open. No compilation happens (that's
 *  `bennu_build`). A no-op on the BE when no open project owns `root`.
 *  Wire: `bennu_reindex` — `ReindexArgs { root }`. */
export function reindex(root: string): Promise<void> {
  return bennu('bennu_reindex', { args: { root } });
}

// ── rename (docs §5 #10-12) ─────────────────────────────────────────────────────

/** Reason an edit was planned — drives the preview grouping + the review nudge.
 *  Mirrors the BE `RenameEdit.reason` string (rename.rs). */
export type RenameReason = 'declaration' | 'reference' | 'import' | 'spring-bean' | 'local';

/** One concrete rename edit — mirrors the BE `RenameEdit` (proto contract). Byte
 *  offsets; the FE applies through CodeMirror (undo works) and may guard on `old`. */
export interface RenameEdit {
  /** Absolute path (forward slashes) of the file to edit. */
  file: string;
  /** Start byte offset. */
  start: number;
  /** End byte offset (exclusive). */
  end: number;
  /** Replacement text. */
  new_text: string;
  /** The exact text currently at `[start, end)` — a stale-buffer guard. */
  old: string;
  /** Why this edit exists. */
  reason: RenameReason;
  /** True when inferred/heuristic (a method use-site where an overload could collapse) —
   *  surface for review, never auto-apply as if exact. */
  inferred: boolean;
}

/** The edits for one file, in offset order (a preview group). Mirrors BE
 *  `RenameFileEdits`. */
export interface RenameFileEdits {
  file: string;
  edits: RenameEdit[];
}

/** The rename PREVIEW — mirrors the BE `RenamePreview` (proto contract).
 *  `bennu_rename_plan` returns `null` when the caret isn't renameable or the rename
 *  engine is still building. */
export interface RenamePreview {
  /** The old identifier under the caret. */
  old_name: string;
  /** The requested new name. */
  new_name: string;
  /** A short human label of the target (`"method com.x.Foo.bar()"`, `"local `x`"`, …). */
  target_label: string;
  /** The edits, grouped by file. */
  files: RenameFileEdits[];
  /** Total number of edit sites. */
  total_edits: number;
  /** Whether any edit is `inferred` (the FE nudges review before applying). */
  has_inferred: boolean;
}

/** Plan a rename for the symbol at `file`:`offset` → `newName`, returning the PREVIEW
 *  the user confirms before anything is written. `source` is the current (possibly
 *  unsaved) buffer — the caret is classified against it. Resolves to `null` gracefully
 *  when no project owns the file, its rename engine is still building, or the caret
 *  isn't on a renameable identifier.
 *  Wire: `bennu_rename_plan` — `RenameArgs { file, source, offset, new_name }`. */
export function renamePlan(
  file: string,
  source: string,
  offset: number,
  newName: string,
): Promise<RenamePreview | null> {
  return bennu('bennu_rename_plan', { args: { file, source, offset, new_name: newName } });
}

/** The concrete edits to apply for a rename (the flattened plan). The FE applies each
 *  through CodeMirror so undo works — the backend never writes buffers. Returns `[]`
 *  when there's nothing to do (unrenameable / still building). Prefer previewing with
 *  {@link renamePlan} first; call this on confirm.
 *  Wire: `bennu_rename_apply` — `RenameArgs { file, source, offset, new_name }`. */
export function renameApply(
  file: string,
  source: string,
  offset: number,
  newName: string,
): Promise<RenameEdit[]> {
  return bennu('bennu_rename_apply', { args: { file, source, offset, new_name: newName } });
}

// ── find usages (docs §5 #7) ──────────────────────────────────────────────────────

/** One resolved use site — mirrors the BE `UsageHit` (proto contract). Byte offsets
 *  plus a 1-based line/col and the trimmed source line for the results preview. */
export interface UsageHit {
  /** Absolute path (forward slashes) of the file the use is in. */
  file: string;
  /** Start byte offset of the referencing identifier. */
  start: number;
  /** End byte offset (exclusive). */
  end: number;
  /** 1-based line of the reference. */
  line: number;
  /** 1-based column of the reference. */
  col: number;
  /** The trimmed source line, for the results list. */
  preview: string;
}

/** The find-usages result — mirrors the BE `UsagesResult`. `bennu_references` returns
 *  `null` when the caret isn't on a resolvable declaration or the index is still
 *  building. */
export interface UsagesResult {
  /** A short human label of the target (`"method com.x.Foo.bar()"`, …). */
  target_label: string;
  /** Every use site across the project (empty when the symbol has none). */
  usages: UsageHit[];
}

/** Find all usages of the symbol at `file`:`offset` (UTF-8 byte offset). `source` is
 *  the current (possibly-unsaved) buffer — the caret is classified against it.
 *  Resolves to `null` gracefully when the caret isn't on a resolvable declaration or
 *  the reference index is still building.
 *  Wire: `bennu_references` — `ReferencesArgs { file, source, offset }`. */
export function references(
  file: string,
  source: string,
  offset: number,
): Promise<UsagesResult | null> {
  return bennu('bennu_references', { args: { file, source, offset } });
}

/** Find-usages for a Struts **action** reference (a JSP `action="…"` value under the
 *  caret): every JSP across the project that references `action`. `file` is any file in
 *  the owning project. Absolute action names only (a relative ref isn't resolvable). The
 *  usages share the {@link UsageHit} shape, so the same results popover renders them.
 *  Wire: `bennu_action_usages` — `{ file, action }`. */
export function actionUsages(file: string, action: string): Promise<UsagesResult> {
  return bennu('bennu_action_usages', { args: { file, action } });
}

// ── JSP page-scoped variable navigation ──────────────────────────────────────────

/** Go-to-declaration + find-usages for a JSP **page-scoped variable** — mirrors the BE
 *  `JspNav`. Everything is single-file (a JSP variable is page-scoped), so `declaration`
 *  (when present) and every `usages` hit live in the SAME file as the caret. A default
 *  (empty label, `declaration: null`, `usages: []`) when the caret isn't on a JSP variable. */
export interface JspNav {
  /** A short human label of the variable (`"JSP variable `total`"`). */
  label: string;
  /** The declaring `<c:set>`/`<s:set>`/… site in this file, or `null` when the name is
   *  referenced but not declared in the page. */
  declaration: DeclarationTarget | null;
  /** Every EL/OGNL reference to the variable in this file, in document order. */
  usages: UsageHit[];
}

/** Resolve the JSP page-scoped variable at `file`:`offset` (a `<c:set var>`/`<s:set var>`/…
 *  declaration or an `${var}`/`%{var}` reference under the caret) to its in-page declaration
 *  + all references. `source` is the current (possibly-unsaved) buffer. Single-file and
 *  index-free — always answers (empty when the caret isn't on a JSP variable).
 *  Wire: `bennu_jsp_nav` — `JspNavArgs { file, source, offset }`. */
export function jspNav(file: string, source: string, offset: number): Promise<JspNav> {
  return bennu('bennu_jsp_nav', { args: { file, source, offset } });
}

/** Resolve a JSP **include / view reference** under the caret (`<%@ include file>` /
 *  `<jsp:include page>` / `<s:include value>` / `<c:import url>`) to the absolute
 *  (forward-slashed) path of the referenced JSP, for cross-file Ctrl+B / Ctrl+click go-to.
 *  `file` is the JSP being edited (the resolution base — absolute paths resolve against the
 *  webapp root, relative ones against the JSP's own dir); `path` is the reference token
 *  (the FE's `refAtCaret()`). Resolves to `null` gracefully when the reference is a computed
 *  expression, an external `http(s)://` URL, or doesn't point at an existing file.
 *  Wire: `bennu_jsp_include_target` — `JspIncludeTargetArgs { file, path }`. */
export function jspIncludeTarget(file: string, path: string): Promise<string | null> {
  return bennu('bennu_jsp_include_target', { args: { file, path } });
}

// ── hover (docs §5) ─────────────────────────────────────────────────────────────

/** Hover card for the symbol under the caret — mirrors the BE `HoverInfo`. */
export interface HoverInfo {
  /** The member/type signature (a `raw_signature` when known, else a synthesized one). */
  signature: string;
  /** `method` | `field` | `class` (types aren't distinguished into interface/enum yet). */
  kind: string;
  /** Owning type's dotted FQCN for a member; `null` for a type. */
  container: string | null;
  /** Javadoc / leading comment — `null` for now (extraction deferred on the BE). */
  doc: string | null;
}

/** Resolve the hover info for the symbol at `file`:`offset` (UTF-8 byte offset).
 *  `null` when the caret isn't on a resolvable symbol or the index is still building.
 *  Wire: `bennu_hover` — `HoverArgs { file, source, offset }`. */
export function hover(file: string, source: string, offset: number): Promise<HoverInfo | null> {
  return bennu('bennu_hover', { args: { file, source, offset } });
}
