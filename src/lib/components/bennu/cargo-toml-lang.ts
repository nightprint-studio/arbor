/**
 * The editor language for a **`Cargo.toml`**.
 *
 * Colouring is the stock TOML mode; what is added is everything a generic TOML mode cannot know,
 * because it comes from outside the file:
 *
 * - **completion** over the manifest schema (table headers, the keys of the table you are in, the
 *   values of a closed set like `edition`) *and* over the crates this machine actually has — names
 *   and versions from the workspace, `Cargo.lock` and the local registry cache;
 * - **diagnostics** for the things Cargo either ignores silently (a key typo, which is why the
 *   dependency you added is not there) or refuses outright (a feature referring to something that
 *   does not exist).
 *
 * Both are backend calls through the ordinary `bennu_completion` / `bennu_diagnostics` handlers —
 * the backend routes a `Cargo.toml` to its own engine, exactly as it routes a `.rs` file to a
 * language server. Nothing about the manifest schema lives on this side.
 *
 * ## Why not rust-analyzer
 *
 * It does read manifests, and it says almost nothing about them: it reports a handful of errors, and
 * only after a workspace reload. A key typo is worth a squiggle the moment you type it, which means
 * a validator that runs on the buffer.
 *
 * ## The one thing this file decides
 *
 * Whether to *ask*. The backend decides the candidates and their replace range — it has the spans —
 * so the only local judgement is not opening a popup on every newline in a manifest.
 */

import type { LanguageDescriptor } from '$lib/components/shared/ui/code-editor';
import { makeByteToU16, makeU16ToByte } from '$lib/components/shared/ui/code-editor';
import { StreamLanguage, type StreamParser } from '@codemirror/language';
import type { CompletionContext, CompletionResult } from '@codemirror/autocomplete';
import { completion as ipcCompletion } from '$lib/ipc/bennu';
import { projectStore } from '$lib/stores/bennu/project.svelte';

/** Whether a path is a Cargo manifest — the same test the backend applies, and the gate for handing
 *  out this descriptor at all.
 *
 *  The **name**, not the extension: a Rust project has plenty of `.toml` files that are not
 *  manifests (`rustfmt.toml`, `.cargo/config.toml`), and applying the manifest schema to one of them
 *  would flag every key in it. */
export function isCargoManifest(path: string | null | undefined): boolean {
  if (!path) return false;
  return (path.split(/[\\/]/).pop() ?? '').toLowerCase() === 'cargo.toml';
}

/**
 * Where the token under the caret begins — used only to decide whether anything is being typed.
 *
 * The backend returns a `replace_start` / `replace_end` with every candidate (it parsed the file and
 * knows the spans), so this is deliberately not trying to be the same answer. It exists so the popup
 * does not open unprompted on a blank line.
 */
function typedSomething(ctx: CompletionContext): boolean {
  const line = ctx.state.doc.lineAt(ctx.pos);
  const before = line.text.slice(0, ctx.pos - line.from);
  const trimmed = before.trimStart();
  if (trimmed.startsWith('#')) return false;
  // A `[` opens a table header, and offering the tables there with nothing else typed is exactly
  // right — it is the one position where an empty prefix is a question.
  if (trimmed.startsWith('[')) return true;
  // Otherwise: something word-like, a quote just opened, or an `=` just typed.
  return /[\w.\-/:]$|["'=,[]\s*$/.test(before);
}

const manifestCompletionSource = async (
  ctx: CompletionContext,
): Promise<CompletionResult | null> => {
  const path = projectStore.activeFilePath;
  if (!path || !isCargoManifest(path)) return null;
  if (!ctx.explicit && !typedSomething(ctx)) return null;

  const src = ctx.state.doc.toString();
  const byteOffset = makeU16ToByte(src)(ctx.pos);
  const items = await ipcCompletion(path, byteOffset, src).catch(() => []);
  if (items.length === 0) return null;

  // Every candidate carries its own range, so `from` is taken from the first — they all share it.
  // One mapper for the whole popup: building it per candidate would be a pass over the document
  // per row.
  const toU16 = makeByteToU16(src);
  const from = items[0].replace_start != null ? toU16(items[0].replace_start) : ctx.pos;
  const to = items[0].replace_end != null ? toU16(items[0].replace_end) : ctx.pos;

  return {
    from,
    to,
    options: items.map((it) => ({
      label: it.label,
      type: completionType(it.kind),
      detail: it.detail ?? undefined,
      apply: it.insert_text ?? undefined,
      // The backend already ordered them (newest version first); its `sort_text` decides.
      boost: it.sort_text ? Math.max(-50, 50 - Number(it.sort_text)) : undefined,
    })),
    // The backend filtered against what was typed. Letting CodeMirror re-filter would drop a
    // candidate whose label starts before the caret — a `dep:serde` completed after `dep:`, or a
    // path member completed after `crates/`.
    filter: false,
  };
};

/** Map a backend completion kind onto a CodeMirror one, for the icon. */
function completionType(kind: string): string {
  switch (kind) {
    case 'table':
      return 'namespace';
    case 'module':
      return 'class';
    case 'folder':
      return 'text';
    case 'value':
      return 'constant';
    default:
      return 'property';
  }
}

const intel = {
  completion: manifestCompletionSource,
};

/**
 * Build the descriptor. Called once at module load — the identity has to be stable, because
 * `CodeEditor` rebuilds its extensions when the descriptor changes and a fresh object per read
 * would remount the editor on every keystroke.
 */
export function cargoTomlLang(parser: StreamParser<unknown>): LanguageDescriptor {
  return {
    id: 'cargo-toml',
    createParser: () => Promise.reject(new Error('cm-language:cargo-toml has no tree-sitter parser')),
    classify: () => null,
    cmExtension: StreamLanguage.define(parser),
    intel,
  };
}
