/**
 * Language-server IPC — the calls that exist only for a server-backed language.
 *
 * Everything the editor already had a call for — completion, go-to, find-usages, hover,
 * diagnostics, rename — is **not** here. Those route through the same
 * `bennu_completion` / `bennu_declaration` / `bennu_references` / `bennu_hover` /
 * `bennu_diagnostics` the Java engine answers, and the backend decides which engine replies.
 * That is the whole point of the provider seam: opening a `.rs` file does not send the editor
 * down a second code path.
 *
 * What lives here is the surface that had no Java equivalent to inherit: semantic tokens,
 * outline, format, code actions, signature help, and the server lifecycle the settings panel
 * drives.
 *
 * Same conventions as the rest of `ipc/bennu`: every call wraps its fields under `{ args: … }`
 * and every offset is a **UTF-8 byte offset**.
 */

import { bennu } from '../rpc';
import type { CompletionItem, FileDiagnostics, SourceEdit } from '$lib/types/bennu';

/** One occurrence of the symbol under the caret. */
export interface LspHighlight {
  start: number;
  end: number;
  /** `read` · `write` · `text`. `text` is what a server that did not distinguish gives, and it is the
   *  majority — so it must not be rendered as a lesser kind of occurrence. */
  kind: string;
}

/** A foldable region, in byte offsets. */
export interface LspFold {
  /** Where the fold begins — the end of the header line, so what names the region stays visible. */
  start: number;
  end: number;
  /** `comment` · `imports` · `region`, or empty for an ordinary block. */
  kind: string;
  /** What to show in place of the folded text; empty for the editor's default. */
  placeholder: string;
}

/** One code lens: where it goes, what it says, and what pressing it does. */
export interface LspLens {
  /** Byte offset of the item it belongs to. */
  start: number;
  /** 1-based line, so the editor can place it without a second conversion. */
  line: number;
  title: string;
  /** The command pressing it runs. `null` for a lens that is only a label. */
  command?: string | null;
  arguments: unknown[];
}

/** One call site inside a hierarchy node. */
export interface LspCallSite {
  file: string;
  start: number;
  end: number;
  line: number;
  preview: string;
}

/** One node of a call or type hierarchy.
 *
 *  The two hierarchies share a shape because the protocol gives them one, and because the panel
 *  that draws them is one panel: a tree whose children are fetched a level at a time. */
export interface LspHierarchyNode {
  name: string;
  /** A lowercase kind name (`function`, `struct`, `trait`). */
  kind: string;
  detail?: string | null;
  /** Where the declaration is — the name token, so go-to lands on it. */
  file: string;
  start: number;
  end: number;
  line: number;
  col: number;
  /** The trimmed source line, for a preview. */
  preview: string;
  /** The call sites inside this node that reach the item asked about; empty for a type hierarchy. */
  call_sites: LspCallSite[];
  /** The server's own handle on this item, opaque. Sent back **verbatim** to fetch its children. */
  handle: unknown;
}

/** Which way a hierarchy is walked. `incoming`/`outgoing` are calls, `supertypes`/`subtypes` types. */
export type LspHierarchyDirection = 'incoming' | 'outgoing' | 'supertypes' | 'subtypes';

/** The expansion of a macro.
 *
 *  `expansion` is Rust source as **text** — not a file the server knows about. That is why it cannot
 *  be navigated, and why a nested macro has to be expanded from the original file rather than from
 *  inside this result. */
export interface LspMacroExpansion {
  /** The macro's name. */
  name: string;
  /** Rust source. */
  expansion: string;
}

/** A server's lifecycle state. */
export type LspState = 'starting' | 'ready' | 'failed' | 'exited';

/** One language server's live state — mirrors the BE `LspStatus`. */
export interface LspStatus {
  /** The catalogue / config id (`rust-analyzer`). */
  id: string;
  name: string;
  /** The language it serves (`rust`). */
  language: string;
  /** The workspace root it was started for. */
  root: string;
  /** The executable that was resolved and run. */
  command: string;
  /** The server's self-reported name + version, when it gave one. */
  version?: string | null;
  state: LspState;
  /** Why it failed, or the last error it chose to show. Empty when healthy. */
  message: string;
  /** The long-running operation it is reporting (`"Indexing 43%"`), or empty. */
  progress: string;
  /** Which editor features it can serve — so the UI never offers one that answers nothing. */
  features: string[];
  /** The tail of its stderr. Usually the only place a failed start explains itself. */
  log_tail: string[];
}

/** A server Bennu knows how to run, resolved against this machine. */
export interface LspServerInfo {
  id: string;
  name: string;
  language: string;
  /** The file extensions it serves, without dots. */
  extensions: string[];
  /** The resolved absolute path, or null when nothing was found. */
  path?: string | null;
  /** The bare command name it was looked up under. */
  command: string;
  /** How to install it — shown instead of a bare "not found". */
  install_hint: string;
  enabled: boolean;
  /** True when it comes from the user's own `[[lsp.servers]]`. */
  custom: boolean;
  /** The command that installs it, argv-style. Empty when there is none Bennu will run —
   *  a system package (clangd is LLVM), or a server the user defined themselves. The
   *  settings page offers an Install button exactly when this is non-empty.
   *
   *  **Optional on purpose.** The frontend and `bennu-be` are separate binaries with
   *  separate build times, so a field added to the wire is absent until the backend is
   *  rebuilt — and a consumer that assumes it is there crashes the settings page on the
   *  version of the backend that was running when it was added. Read it with `?.`. */
  install?: string[];
}

/** What an install attempt did. */
export interface LspInstallResult {
  ok: boolean;
  /** The command that ran, as it would be typed — so a failure leaves something to try by
   *  hand rather than only the news that it failed. */
  command: string;
  /** Where the server resolved to afterwards, when it is now there. */
  path: string | null;
  /** The last lines of output, for the failure message. The whole log went to the Build panel. */
  tail: string;
  /** A one-line diagnosis, when the failure is one with a known fix (a toolchain too old, a
   *  package manager that is not there). Shown instead of {@link tail} — the raw output is
   *  still in the Build panel, and a toast has room for the answer or for the evidence but
   *  not for both. */
  hint?: string;
}

/** One semantically-classified span, in **UTF-8 byte offsets**. */
export interface LspToken {
  start: number;
  end: number;
  /** The token class → `cm-tok-<class>`. */
  class: string;
  /** Extra modifier classes → `cm-tokmod-<name>`. */
  modifiers?: string[];
}

/** A quick fix or refactoring offered at the caret. */
export interface LspAction {
  title: string;
  /** `quickfix`, `refactor.extract`, … or empty. */
  kind: string;
  /** The server's own pick — the UI puts it first. */
  preferred: boolean;
  /** Why it cannot be applied right now. Shown greyed rather than hidden. */
  disabled?: string | null;
  /** The edits to apply, through CodeMirror so undo works. */
  edits: SourceEdit[];
  /** File creations / renames / deletions it also wants, as human descriptions. Bennu does not
   *  perform these, so a non-empty list means the action cannot be fully carried out. */
  file_ops: string[];
  /** A server command to run instead of (or after) the edits. */
  command?: string | null;
  arguments: unknown[];
}

/** Signature help for the call the caret is inside. */
export interface LspSignature {
  label: string;
  doc?: string | null;
  params: string[];
  /** Which parameter the caret is in — the UI bolds it. */
  active_param?: number | null;
  /** The active parameter's span within `label`, in UTF-16 units so it can be sliced directly. */
  active_start?: number | null;
  active_end?: number | null;
}

/** One node of a server outline, or one hit of a workspace-symbol search. */
export interface LspSymbol {
  name: string;
  /** A lowercase kind (`struct`, `function`, `field`). */
  kind: string;
  detail?: string | null;
  /** Byte range of the whole declaration. */
  start: number;
  end: number;
  /** Byte range of the NAME — where go-to lands. */
  name_start: number;
  name_end: number;
  /** 1-based line, and 1-based column in UTF-16 units (CodeMirror's own coordinate). */
  line: number;
  col: number;
  file: string;
  deprecated: boolean;
  children: LspSymbol[];
}

// ── Lifecycle ────────────────────────────────────────────────────────────────

/** Every language server this session has a slot for, with its state.
 *
 *  Also what wires the backend's event sink, so calling it once on startup is what enables the
 *  pushed `arbor://bennu/lsp-status` updates. Wire: `bennu_lsp_status`. */
export function lspStatus(): Promise<LspStatus[]> {
  return bennu('bennu_lsp_status', { args: {} });
}

/** The servers Bennu knows how to run, resolved against this machine — including the ones that
 *  are NOT installed, with their install hints. Wire: `bennu_lsp_servers`. */
export function lspServers(): Promise<LspServerInfo[]> {
  return bennu('bennu_lsp_servers', { args: {} });
}

/** Install a language server by running the command its own ecosystem ships it through —
 *  `rustup component add`, `cargo install --git`, `npm install -g`. Streams into the Build
 *  panel while it runs, which for a `cargo install` is minutes. Rejects for a server whose
 *  install is a system package. Wire: `bennu_lsp_install`. */
export function lspInstall(id: string): Promise<LspInstallResult> {
  return bennu('bennu_lsp_install', { args: { id } });
}

/** Restart a server. The only way out of a failed slot (failures are sticky on purpose), and
 *  therefore the fix for "I just installed it". Wire: `bennu_lsp_restart`. */
export function lspRestart(root: string, language: string): Promise<boolean> {
  return bennu('bennu_lsp_restart', { args: { root, language } });
}

/** Stop a server and forget its slot. Wire: `bennu_lsp_stop`. */
export function lspStop(root: string, language: string): Promise<boolean> {
  return bennu('bennu_lsp_stop', { args: { root, language } });
}

// ── Highlighting ─────────────────────────────────────────────────────────────

/** Semantic tokens for a buffer — the colouring only something that knows the types can do.
 *  Empty for a file no server serves, or while one is starting.
 *  Wire: `bennu_lsp_semantic_tokens` — `{ file, source }`. */
export function lspSemanticTokens(file: string, source: string): Promise<LspToken[]> {
  return bennu('bennu_lsp_semantic_tokens', { args: { file, source } });
}

// ── Structure ────────────────────────────────────────────────────────────────

/** The document outline. Wire: `bennu_lsp_document_symbols` — `{ file, source }`. */
export function lspDocumentSymbols(file: string, source: string): Promise<LspSymbol[]> {
  return bennu('bennu_lsp_document_symbols', { args: { file, source } });
}

/** Every occurrence of the symbol at `offset`, in this buffer.
 *
 *  Not find-usages: that is a workspace sweep with a results panel. This is what the editor paints
 *  while the caret rests somewhere, so it is cheap and its failure is silence.
 *  Wire: `bennu_lsp_highlights` — `{ file, source, offset }`. */
export function lspHighlights(
  file: string,
  source: string,
  offset: number,
): Promise<LspHighlight[]> {
  return bennu('bennu_lsp_highlights', { args: { file, source, offset } });
}

/** The chain of syntactic ranges enclosing `offset`, innermost first, as `[start, end]` byte pairs.
 *
 *  The **whole chain** in one answer: expanding then walks a list the editor already holds instead of
 *  asking again, and shrinking walks back down it.
 *  Wire: `bennu_lsp_selection_ranges` — `{ file, source, offset }`. */
export function lspSelectionRanges(
  file: string,
  source: string,
  offset: number,
): Promise<[number, number][]> {
  return bennu('bennu_lsp_selection_ranges', { args: { file, source, offset } });
}

/** The foldable regions of a buffer. Wire: `bennu_lsp_folding` — `{ file, source }`. */
export function lspFolding(file: string, source: string): Promise<LspFold[]> {
  return bennu('bennu_lsp_folding', { args: { file, source } });
}

/** The code lenses for a buffer — the counts and actions drawn above an item.
 *
 *  Already resolved: a server answers `textDocument/codeLens` with positions and no titles, filling
 *  them in one `codeLens/resolve` at a time, so the backend does that pass and this returns lenses
 *  that have something to draw. Wire: `bennu_lsp_code_lenses` — `{ file, source }`. */
export function lspCodeLenses(file: string, source: string): Promise<LspLens[]> {
  return bennu('bennu_lsp_code_lenses', { args: { file, source } });
}

/** The locations a pressed lens carries in its own arguments.
 *
 *  A lens that shows a count is a **client** command: the server sends the whole location list with
 *  it, because it had to run the query to count them. So pressing one costs no second query — this
 *  reads what already arrived. `null` when the command is something else (a runnable), which the
 *  caller reads as "nothing to list".
 *  Wire: `bennu_lsp_lens_locations` — `{ file, source, title, arguments }`. */
export function lspLensLocations(
  file: string,
  source: string,
  title: string,
  args: unknown[],
): Promise<import('$lib/ipc/bennu/nav').UsagesResult | null> {
  return bennu('bennu_lsp_lens_locations', { args: { file, source, title, arguments: args } });
}

/** The item at `offset` a hierarchy can be built from — the root of the tree the panel draws.
 *
 *  `calls` picks which hierarchy: `true` for the call hierarchy, `false` for the type hierarchy. An
 *  empty list means the caret is not on something either can be built from.
 *  Wire: `bennu_lsp_prepare_hierarchy` — `{ file, source, offset, calls }`. */
export function lspPrepareHierarchy(
  file: string,
  source: string,
  offset: number,
  calls: boolean,
): Promise<LspHierarchyNode[]> {
  return bennu('bennu_lsp_prepare_hierarchy', { args: { file, source, offset, calls } });
}

/** One level of a hierarchy, expanded from a node's handle.
 *
 *  `scope` is any path inside the workspace — which server answers. Not the node's own file: a
 *  caller can live in a dependency's source, which is deliberately not a workspace of its own.
 *  `item` is the node's `handle`, passed back verbatim.
 *  Wire: `bennu_lsp_hierarchy_step` — `{ scope, item, direction }`. */
export function lspHierarchyStep(
  scope: string,
  item: unknown,
  direction: LspHierarchyDirection,
): Promise<LspHierarchyNode[]> {
  return bennu('bennu_lsp_hierarchy_step', { args: { scope, item, direction } });
}

/** The edits a file rename implies — for Rust, the `mod` declaration that names it and every `use`
 *  path through the module it declares.
 *
 *  Asked **before** the move, which is what the protocol method is for: the server answers about the
 *  tree as it stands. An empty list is the honest answer both for "nothing refers to it by name" and
 *  for a server without the capability, which is why it is safe to ask on a keystroke to preview a
 *  rename. Wire: `bennu_lsp_will_rename` — `{ file, new_path }`. */
export function lspWillRename(file: string, newPath: string): Promise<SourceEdit[]> {
  return bennu('bennu_lsp_will_rename', { args: { file, new_path: newPath } });
}

/** Re-read the project's manifests and rebuild the crate graph.
 *
 *  `scope` is any path inside the workspace — the project root is the natural one. `false` when no
 *  server covers it or the server has no such method; both are states, not failures.
 *  Wire: `bennu_lsp_reload_workspace` — `{ scope }`. */
export function lspReloadWorkspace(scope: string): Promise<boolean> {
  return bennu('bennu_lsp_reload_workspace', { args: { scope } });
}

/** Expand the macro at `offset`.
 *
 *  `null` when the caret is not in a macro call. The expansion is Rust **text** the server produced,
 *  not a document it knows — so it cannot be navigated, and a macro inside it has to be expanded by
 *  pointing at it in the original file.
 *  Wire: `bennu_lsp_expand_macro` — `{ file, source, offset }`. */
export function lspExpandMacro(
  file: string,
  source: string,
  offset: number,
): Promise<LspMacroExpansion | null> {
  return bennu('bennu_lsp_expand_macro', { args: { file, source, offset } });
}

/** Search symbols across the workspace `file` belongs to.
 *  Wire: `bennu_lsp_workspace_symbols` — `{ file, query }`. */
export function lspWorkspaceSymbols(file: string, query: string): Promise<LspSymbol[]> {
  return bennu('bennu_lsp_workspace_symbols', { args: { file, query } });
}

// ── Navigation extras ────────────────────────────────────────────────────────

/** Go to the **type** of the expression under the caret — a distinct gesture from go-to
 *  definition, and the one that answers "what is this thing" on a `let` binding.
 *  Wire: `bennu_lsp_type_definition` — `{ file, source, offset }`. */
export function lspTypeDefinition(
  file: string,
  source: string,
  offset: number,
): Promise<import('$lib/ipc/bennu/nav').DeclarationTarget | null> {
  return bennu('bennu_lsp_type_definition', { args: { file, source, offset } });
}

/** Go to implementations — for a Rust trait method, every `impl` of it.
 *  Wire: `bennu_lsp_implementations` — `{ file, source, offset }`. */
export function lspImplementations(
  file: string,
  source: string,
  offset: number,
): Promise<import('$lib/ipc/bennu/nav').UsagesResult | null> {
  return bennu('bennu_lsp_implementations', { args: { file, source, offset } });
}

/** Signature help at the caret. Wire: `bennu_lsp_signature_help` — `{ file, source, offset }`. */
export function lspSignatureHelp(
  file: string,
  source: string,
  offset: number,
): Promise<LspSignature | null> {
  return bennu('bennu_lsp_signature_help', { args: { file, source, offset } });
}

/** Fill in one completion candidate's documentation.
 *
 *  Servers answer a completion list without docs and fill them in one item at a time — asking
 *  for four hundred eagerly would be four hundred round-trips. `null` when the list has been
 *  superseded, which is the normal outcome of the user carrying on typing.
 *  Wire: `bennu_lsp_resolve_completion` — `{ file, id }`. */
export function lspResolveCompletion(file: string, id: number): Promise<CompletionItem | null> {
  return bennu('bennu_lsp_resolve_completion', { args: { file, id } });
}

// ── Editing ──────────────────────────────────────────────────────────────────

/** Format the whole buffer (`rustfmt`, for Rust).
 *
 *  Returns **edits**, not the formatted text: applying them through CodeMirror keeps the format
 *  in the undo history and the caret in place.
 *  Wire: `bennu_lsp_format` — `{ file, source, tab_size?, insert_spaces? }`. */
export function lspFormat(
  file: string,
  source: string,
  tabSize?: number,
  insertSpaces?: boolean,
): Promise<SourceEdit[]> {
  return bennu('bennu_lsp_format', {
    args: { file, source, tab_size: tabSize, insert_spaces: insertSpaces },
  });
}

/** Quick fixes and refactorings for the caret / selection — the Alt+Enter list.
 *  Wire: `bennu_lsp_code_actions` — `{ file, source, start, end }`. */
export function lspCodeActions(
  file: string,
  source: string,
  start: number,
  end: number,
): Promise<LspAction[]> {
  return bennu('bennu_lsp_code_actions', { args: { file, source, start, end } });
}

/** Run a server command — how an action whose edit is computed lazily is applied. The resulting
 *  edits arrive on `arbor://bennu/lsp-apply-edit`.
 *  Wire: `bennu_lsp_execute_command` — `{ file, command, arguments }`. */
export function lspExecuteCommand(
  file: string,
  command: string,
  args: unknown[] = [],
): Promise<boolean> {
  return bennu('bennu_lsp_execute_command', { args: { file, command, arguments: args } });
}

// ── Document lifecycle ───────────────────────────────────────────────────────

/** Tell the server a buffer was saved.
 *
 *  The backend already does this from `bennu_write_file` (so autosave counts); this exists for a
 *  host that saves by another route. Wire: `bennu_lsp_did_save` — `{ file, source }`. */
export function lspDidSave(file: string, source: string): Promise<boolean> {
  return bennu('bennu_lsp_did_save', { args: { file, source } });
}

/** Tell the server a tab was closed. Never starts a server.
 *  Wire: `bennu_lsp_did_close` — `{ file }`. */
export function lspDidClose(file: string): Promise<boolean> {
  return bennu('bennu_lsp_did_close', { args: { file } });
}

/** Every file the server reports problems in, for the project-wide Problems panel.
 *
 *  Cannot be assembled from the open buffers: a server publishes for files nobody has opened,
 *  which is the whole value of having `cargo check` behind it.
 *  Wire: `bennu_lsp_problems` — `{ file }`. */
export function lspProblems(file: string): Promise<FileDiagnostics[]> {
  return bennu('bennu_lsp_problems', { args: { file } });
}
