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
  /** The resolved implementation class FQCN (the C1 chain), if resolvable. A name,
   *  not a path — shown for context, not directly openable. */
  class_fqcn: string | null;
  /** The resolved view JSP (the Tiles chain), if resolvable — a JSP path. */
  view_jsp: string | null;
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

/** Live-edit re-index: hand the BE the edited file's full text so it patches the
 *  persisted index (completion / definition then reflect the edit without reopening
 *  the project). `text === null` signals the file was deleted. Returns `true` when a
 *  project owns the file (the patch ran), `false` otherwise. Runs on the BE blocking
 *  pool — never blocks typing on this side (call it debounced / on save).
 *  Wire: `bennu_did_change` — `DidChangeArgs { file, text? }`. */
export function didChange(file: string, text: string | null): Promise<boolean> {
  return bennu('bennu_did_change', { args: { file, text } });
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
