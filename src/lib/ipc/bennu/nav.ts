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
import type { Diagnostic } from '$lib/types/bennu';

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
 *  `source` + `offset` (the live JSP buffer + caret) are optional: when the bare `action`
 *  string is ambiguous, the BE re-scans the buffer to fold an enclosing `<s:url namespace="…">`
 *  onto a relative `action`, resolving the qualified name.
 *  Wire: `bennu_definition` — `DefinitionArgs { file, action, source?, offset? }`. */
export function definition(
  file: string,
  action: string,
  source?: string,
  offset?: number,
): Promise<DefinitionResult | null> {
  return bennu('bennu_definition', { args: { file, action, source, offset } });
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

/** Go-to from a JSP form field / OGNL root (a `<s:textfield name="…">` etc.), or a
 *  `*-validation.xml` `<field name="…">`, under the caret to the **action class's** `get`/`set`/`is`
 *  accessor for that property. `source` is the live buffer. `null` when the caret isn't on a
 *  resolvable field, or the action / property doesn't resolve to a project class.
 *  Wire: `bennu_action_property_target` — `{ file, source, offset }`. */
export function actionPropertyTarget(
  file: string,
  source: string,
  offset: number,
): Promise<DeclarationTarget | null> {
  return bennu('bennu_action_property_target', { args: { file, source, offset } });
}

/** Hover on a JSP form field / OGNL root, or a `*-validation.xml` `<field>`, under the caret →
 *  the action property's **type** (`String customer`, `List<Item> items`) and its owning action
 *  class, as a {@link HoverInfo}. `null` when the caret isn't on a resolvable field.
 *  Wire: `bennu_action_property_hover` — `{ file, source, offset }`. */
export function actionPropertyHover(
  file: string,
  source: string,
  offset: number,
): Promise<HoverInfo | null> {
  return bennu('bennu_action_property_hover', { args: { file, source, offset } });
}

/** Go-to on a Struts `<result>` body under the caret in a config XML: a JSP path opens the JSP; an
 *  OGNL/EL root (`${prop}`) jumps to the owning action's property accessor. `null` when the caret
 *  isn't on a resolvable result target.
 *  Wire: `bennu_struts_result_target` — `{ file, source, offset }`. */
export function strutsResultTarget(
  file: string,
  source: string,
  offset: number,
): Promise<DeclarationTarget | null> {
  return bennu('bennu_struts_result_target', { args: { file, source, offset } });
}

/** Lint a Struts config XML's `<result>` targets: a JSP path that resolves to no file under the web
 *  app → "JSP not found"; an OGNL/EL root that isn't a property of the owning action → warning.
 *  Byte-offset diagnostics against the live buffer. Empty when nothing (never a false positive).
 *  Wire: `bennu_struts_result_lint` — `{ file, source }`. */
export function strutsResultLint(file: string, source: string): Promise<Diagnostic[]> {
  return bennu('bennu_struts_result_lint', { args: { file, source } });
}

/** A source-view location: the on-disk `.java` path + a byte offset to jump to, plus whether the
 *  tab should offer "Download sources" (a stub served for a third-party dependency). */
export interface DecompiledLocation {
  file: string;
  offset: number;
  can_download: boolean;
}

/** One member of a type — a declared field, or the property a getter exposes. */
export interface TypeMember {
  name: string;
  /** As it reads: `List<Order>`, `String`. */
  type_text: string;
  /** `field` | `property`. A property is what an interface (or a bytecode-only class) exposes
   *  instead of fields, and saying which keeps the reading honest. */
  kind: string;
  /** The qualified type to ask about next when this member holds something worth opening.
   *  `null` for a `String`, an `int`, an enum — what makes a row expandable or not. */
  expand: string | null;
  inherited: boolean;
}

/** What a type is and what it holds. */
export interface TypeShape {
  /** The qualified name it resolved to. */
  name: string;
  simple: string;
  /** `class` | `interface` | `enum` | `record` | `annotation`. */
  kind: string;
  /** Where the project declares it — absent for a library type. */
  file: string | null;
  line: number | null;
  members: TypeMember[];
}

/**
 * What is inside the type `typeText` names, resolved against `file`'s imports.
 *
 * **One level per call, and only when asked.** A catalog is hundreds of rows naming hundreds of
 * types; resolving them to build the list would make the panel pay, on open, for the two you were
 * going to look at. And a DTO graph can be deep and cyclic, so the recursion is the caller's —
 * it stops when the user stops clicking.
 *
 * Wrappers are unwrapped first: `ResponseEntity<QFormDto>` is a `QFormDto`, and the envelope is
 * never the thing you wanted to open.
 *
 * `null` is the ordinary answer for most types — a scalar, a type variable, a class the classpath
 * cannot reach — and means "offer no expansion", not "something failed".
 * Wire: `bennu_type_shape` — `{ root, file, type_text }`.
 */
export function typeShape(
  root: string,
  file: string,
  typeText: string,
): Promise<TypeShape | null> {
  return bennu('bennu_type_shape', { args: { root, file, type_text: typeText } });
}

/** Resolve a **library/JDK type** `name` (a simple name resolved via `source`'s imports, or a dotted
 *  FQCN) to a source view on disk — the real `.java` (JDK `src.zip` / a downloaded dependency
 *  `-sources.jar`) when available, else a decompiled-from-bytecode stub. `null` when it doesn't
 *  resolve, is a project type (real source), or can't be decoded.
 *  Wire: `bennu_decompiled_source` — `{ file, source, name }`. */
export function decompiledSource(
  file: string,
  source: string,
  name: string,
): Promise<DecompiledLocation | null> {
  return bennu('bennu_decompiled_source', { args: { file, source, name } });
}

/** Resolve a **stack-trace frame** in a library / JDK class (one the console could not resolve
 *  from the project's class index) to a source view: the real `.java` from the JDK's `src.zip`
 *  or a downloaded `-sources.jar` when there is one, else a stub decompiled from the bytecode.
 *
 *  Where it lands depends on which of the two it got: against real source the frame's `line` is
 *  a fact and is used; against a stub the line numbers are fiction, so it lands on `method`.
 *  `root` is the open project (it picks the classpath resolver). Resolves to `null` when
 *  nothing resolves — the caller then leaves the click alone.
 *  Wire: `bennu_frame_source` — `{ root, class, method?, line? }`. */
export function frameSource(
  root: string,
  cls: string,
  method?: string,
  line?: number,
): Promise<DecompiledLocation | null> {
  return bennu('bennu_frame_source', { args: { root, class: cls, method, line } });
}

/** Download the `-sources.jar` for the dependency that owns the library type `name` (resolved via
 *  `file`'s buffer `source`), via `mvn dependency:get`, as a tracked background job. `viewPath` is
 *  the open decompiled tab's path, echoed back in `arbor://bennu/sources-ready { path, ok }` so the
 *  FE reloads the right tab (and clears its spinner on failure). Rejects fast only when the type
 *  isn't a resolvable library type. Wire: `bennu_download_sources` — `{ file, source, name, view_path }`. */
export function downloadSources(
  file: string,
  source: string,
  name: string,
  viewPath: string,
): Promise<string> {
  return bennu('bennu_download_sources', { args: { file, source, name, view_path: viewPath } });
}

/** A single "unknown property on action" lint hit — a JSP field / validation `<field>` whose root
 *  property name matches no bean property of the resolved action class. Byte offsets into the buffer. */
export interface PropertyLintHit {
  start: number;
  end: number;
  /** The offending property name. */
  name: string;
  /** The action class simple-name it was checked against. */
  action: string;
}

/** Lint the JSP form fields / validation `<field>`s in `source` against the resolved action class's
 *  bean properties — a warning per field whose name exists nowhere on the action. Empty when the
 *  action / its properties can't be resolved (never a false positive).
 *  Wire: `bennu_action_property_lint` — `{ file, source }`. */
export function actionPropertyLint(file: string, source: string): Promise<PropertyLintHit[]> {
  return bennu('bennu_action_property_lint', { args: { file, source } });
}

/** One candidate Struts action a JSP view could be bound to (from the reverse view→action lookup). */
export interface JspActionOption {
  /** Action qualified-name (`/do/Cat/viewTree`) — the stored binding value. */
  qname: string;
  class_fqcn: string | null;
  /** Class simple-name (or qname tail) for the dropdown label. */
  simple: string;
}

/** The action-binding state for a JSP view — candidates, the pinned action, and the effective one
 *  actually used for OGNL go-to / linting. */
export interface JspActionBinding {
  candidates: JspActionOption[];
  bound: string | null;
  effective: string | null;
}

/** The reverse view→action candidates + current binding for the JSP `file` (the action picker).
 *  Wire: `bennu_jsp_actions` — `{ file }`. */
export function jspActions(file: string): Promise<JspActionBinding> {
  return bennu('bennu_jsp_actions', { args: { file } });
}

/** Pin (or, with `action === null`, clear) which action a JSP view's OGNL is checked/navigated
 *  against; persisted in the bennu config. Wire: `bennu_set_jsp_action` — `{ file, action? }`. */
export function setJspAction(file: string, action: string | null): Promise<boolean> {
  return bennu('bennu_set_jsp_action', { args: { file, action } });
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
 *  `bennu_reindex`): drops the class cache / symbol index / config resolver / semantic engine
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
  /** Why this rename must NOT be applied, or `null`. The edits are still listed — seeing what it
   *  would do is how the reason makes sense — but this is a refusal, not a warning to click past.
   *  Today: a method that overrides a member of a library type, which cannot be renamed with it. */
  blocked: string | null;
  /** The file this rename also has to move, or `null`. Applied AFTER the edits — they are
   *  addressed to the old path. */
  file_rename: RenameFileMove | null;
}

/** A source file that has to be renamed along with the type it declares. Java ties a public
 *  top-level type to its filename, so renaming the type without the file leaves code that does
 *  not compile. Only ever set for a type whose file is named after it — never a nested one. */
export interface RenameFileMove {
  /** The file's current path. */
  from: string;
  /** The path it must take — same directory, new basename. */
  to: string;
}

/** Plan a rename for the symbol at `file`:`offset` → `newName`, returning the PREVIEW
 *  the user confirms before anything is written. `source` is the current (possibly
 *  unsaved) buffer — the caret is classified against it. Resolves to `null` gracefully
 *  when no project owns the file, its semantic engine is still building, or the caret
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
  /**
   * The member this use was found **through**, when it is not the one asked about — `getName()`.
   *
   * A field whose accessors Lombok generates is used as `order.getName()` and never as `name`, so
   * its uses are real uses spelled as something else. Absent for an ordinary hit.
   */
  via?: string;
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
 *  `originFile` is for a caret inside a **library source view**: that file lives under no
 *  project root, so its own path cannot pick the index the use sites are in — a file from
 *  the project it was opened from does. Omit it for an ordinary project buffer.
 *  Wire: `bennu_references` — `ReferencesArgs { file, source, offset, origin_file }`. */
export function references(
  file: string,
  source: string,
  offset: number,
  originFile?: string,
): Promise<UsagesResult | null> {
  return bennu('bennu_references', { args: { file, source, offset, origin_file: originFile } });
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

// ── MyBatis mapper-XML navigation ─────────────────────────────────────────────────

/** A resolved go-to target from inside a mapper XML — mirrors the BE `MybatisNavResult`.
 *  Either an intra-file byte `offset` (a `<sql>`/`<resultMap>` in the same mapper) or a
 *  cross-file `line` in a `.java` (the interface method/type); the unused one is `0`. */
export interface MybatisNav {
  /** Absolute path (forward slashes) of the file to open. */
  file: string;
  /** Byte offset to jump to (intra-file); `0` when `line` is used instead. */
  offset: number;
  /** 1-based line to jump to (cross-file into a `.java`); `0` when `offset` is used. */
  line: number;
}

/** Resolve the mapper-XML token at `file`:`offset` — a statement `id` → its Java interface
 *  method, `namespace` → the interface, `<include refid>` → its `<sql>`, a `resultMap="…"`
 *  → its `<resultMap>`. `source` is the current (possibly-unsaved) buffer, so the offset is
 *  classified against what the user sees. `null` when the caret isn't on a navigable
 *  reference or it can't be resolved (no index yet, an as-yet-unsupported cross-namespace
 *  fragment). Wire: `bennu_mybatis_nav` — `MybatisNavArgs { file, source, offset }`. */
export function mybatisNav(file: string, source: string, offset: number): Promise<MybatisNav | null> {
  return bennu('bennu_mybatis_nav', { args: { file, source, offset } });
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

/** Go-to-declaration from a caret INSIDE a library/JDK source view — resolves the target against the
 *  ORIGIN project (`originFile`, which picks the classpath resolver; a library view's own path is
 *  under no project) and returns the target source view (member-precise). Chains library → library.
 *  `null` when the caret isn't a resolvable type / member access.
 *  Wire: `bennu_library_declaration` — `{ origin_file, source, offset }`. */
export function libraryDeclaration(
  originFile: string,
  source: string,
  offset: number,
): Promise<DecompiledLocation | null> {
  return bennu('bennu_library_declaration', { args: { origin_file: originFile, source, offset } });
}

/** Hover INSIDE a library/JDK source view — the inferred type of the local/expression at the caret,
 *  via the origin project's resolver. `null` when the caret isn't a typeable local.
 *  Wire: `bennu_library_hover` — `{ origin_file, source, offset }`. */
export function libraryHover(
  originFile: string,
  source: string,
  offset: number,
): Promise<HoverInfo | null> {
  return bennu('bennu_library_hover', { args: { origin_file: originFile, source, offset } });
}
