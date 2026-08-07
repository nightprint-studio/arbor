/**
 * A {@link LanguageDescriptor} for any language served by a **language server**.
 *
 * One factory, not one file per language: everything a server-backed language needs from the
 * editor is identical — the same completion call, the same hover call, the same signature help
 * — so what varies is only the base highlighter and the comment syntax. Adding Go or Zig here is
 * a call to {@link lspLanguage}, not a new module.
 *
 * ## Two highlight layers
 *
 * The descriptor still carries a `cmExtension` (a CodeMirror legacy mode for Rust, TOML, …).
 * That is deliberate: it colours the file the instant it opens, with no round-trip, and keeps it
 * coloured while it is edited and while the server is down. The server's semantic tokens are
 * pushed **on top** by the editor host (`BennuEditor`), refining what the mode could only guess
 * at — a struct from a trait, a macro from a function, a `mut` binding from an immutable one.
 *
 * ## What is NOT here
 *
 * Go-to, find-usages, diagnostics and rename. Those are not language-descriptor concerns: they
 * are host gestures, and the host already routes them through the shared `bennu_declaration` /
 * `bennu_references` / `bennu_diagnostics` / `bennu_rename_plan` calls that the backend answers
 * with whichever engine owns the file. Adding them here would be a second, parallel path to the
 * same handlers.
 */

import type { LanguageDescriptor, CompletionSource } from '$lib/components/shared/ui/code-editor';
import {
  hoverCardDom, insertWithStops, makeByteToU16, makeU16ToByte,
} from '$lib/components/shared/ui/code-editor';
import { StreamLanguage, type StreamParser } from '@codemirror/language';
import { insertCompletionText, type Completion, type CompletionContext, type CompletionResult }
  from '@codemirror/autocomplete';
import type { EditorView, Tooltip } from '@codemirror/view';
import { projectStore } from '$lib/stores/bennu/project.svelte';
import { completion as ipcCompletion } from '$lib/ipc/bennu';
import { hover as ipcHover } from '$lib/ipc/bennu/nav';
import {
  lspResolveCompletion, lspSignatureHelp, type LspSignature,
} from '$lib/ipc/bennu/lsp';
import type { CompletionItem, SourceEdit } from '$lib/types/bennu';

/** Map a provider `kind` to a CodeMirror completion `type` (drives the popup's kind icon).
 *
 *  Broader than the Java map because a language server's vocabulary is: it distinguishes a
 *  struct from a class, an enum member from a constant, a module from a namespace. */
function kindToType(kind: string): string {
  switch (kind) {
    case 'method':
    case 'function':
    case 'constructor':   return 'method';
    case 'field':
    case 'property':      return 'property';
    case 'class':
    case 'struct':
    case 'interface':
    case 'enum':
    case 'event':         return 'class';
    case 'type-parameter': return 'type';
    case 'variable':       return 'variable';
    case 'keyword':        return 'keyword';
    case 'constant':
    case 'enum-member':    return 'constant';
    case 'module':         return 'namespace';
    case 'snippet':        return 'text';
    default:               return 'text';
  }
}

/**
 * Only the latest keystroke's answer is allowed to open a popup.
 *
 * CodeMirror coalesces its own requests, but the IPC round-trip can still race: two keystrokes
 * in flight can resolve out of order, and the older answer would replace the newer list with a
 * stale one.
 */
let completionSeq = 0;

/** Apply a provider's extra edits (for Rust, the `use` line an auto-imported item needs).
 *
 *  Dispatched as a **second** transaction, after the insertion. Both are byte-offset edits
 *  computed against the pre-insertion buffer, and an import sits above the caret — so applying
 *  the insertion first leaves the import's offsets untouched, whereas one combined transaction
 *  would have to reason about which of the two shifts the other. */
function applyAdditionalEdits(view: EditorView, edits: SourceEdit[], preInsertSource: string) {
  if (!edits.length) return;
  const b2u = makeByteToU16(preInsertSource);
  // Descending, so an earlier edit's offsets are still valid after a later one is applied.
  const mapped = edits
    .map((e) => ({ from: b2u(e.start), to: b2u(e.end), insert: e.new_text }))
    .sort((a, b) => b.from - a.from);
  view.dispatch({ changes: mapped });
}

/**
 * Turn one provider item into a CodeMirror completion.
 *
 * `rank` is the item's position in the provider's own ordering, and it becomes a `boost` because
 * that is the only lever CodeMirror offers: it scores options by fuzzy-matching the label and
 * adds `boost`, so a string sort key has nowhere to go. Without the nudge, typing `it` in Rust
 * puts `zip` above `iter` — while rust-analyzer's own ordering already knew which one you meant.
 *
 * The mapping is a nudge, not an override: the top of the server's list gets a strong boost, the
 * tail a small penalty, and CodeMirror's match quality still decides between neighbours. A hard
 * override would break the thing CodeMirror is genuinely better at — ranking by what was typed.
 */
function toCompletion(item: CompletionItem, rank: number, file: string): Completion {
  const completion: Completion = {
    label: item.label,
    detail: item.detail ?? undefined,
    type: kindToType(item.kind),
    boost: item.preselect ? 99 : Math.max(-50, 60 - rank),
  };

  // `label` is a display string — a server may send `push(…)`. What goes in the buffer is
  // `insert_text`, and inserting the label verbatim is how an accepted completion produces
  // code that does not compile.
  const insert = item.insert_text ?? item.label;
  const extras = item.edits ?? [];
  const needsCustomApply =
    insert !== item.label || extras.length > 0 || (item.snippet_stops?.length ?? 0) > 0;

  if (needsCustomApply) {
    completion.apply = (view, _c, from, to) => {
      const pre = view.state.doc.toString();
      // `insert_text` is plain text either way — the backend parsed the placeholder syntax away and
      // left the stops as byte ranges into it (see `bennu-lsp`'s `snippet.rs`). So a snippet differs
      // from a plain completion only in what happens *after* the text lands.
      const stops = item.snippet_stops ?? [];
      if (stops.length > 0) {
        insertWithStops(view, from, to, insert, stops, makeByteToU16(insert));
      } else {
        view.dispatch(insertCompletionText(view.state, insert, from, to));
      }
      applyAdditionalEdits(view, extras, pre);
    };
  }

  // Documentation, fetched only for the item the user actually highlights — a server answers a
  // completion list without docs, and resolving all four hundred eagerly would be four hundred
  // round-trips.
  if (item.doc) {
    completion.info = () => docDom(item.doc!);
  } else if (item.resolve_id != null) {
    const id = item.resolve_id;
    completion.info = async () => {
      const resolved = await lspResolveCompletion(file, id).catch(() => null);
      const text = resolved?.doc ?? resolved?.detail;
      return text ? docDom(text) : null;
    };
  }
  return completion;
}

/** A completion item's documentation, as the popup's info panel. */
function docDom(text: string): HTMLElement {
  const dom = document.createElement('div');
  dom.className = 'cm-completionInfo-doc';
  dom.textContent = text;
  return dom;
}


/** The completion source shared by every server-backed language.
 *
 *  Fires on an in-progress identifier, and on the trigger characters a server cares about. The
 *  backend works out *which* trigger the caret follows from the buffer itself, so this does not
 *  need to know that Rust triggers on `::` and not only on `.`. */
const lspCompletionSource: CompletionSource = async (
  ctx: CompletionContext,
): Promise<CompletionResult | null> => {
  // A conservative trigger set that covers the languages in the catalogue: a word being typed,
  // or a member/path/attribute punctuation right before the caret.
  const word = ctx.matchBefore(/[\w$]*$/);
  const punct = ctx.matchBefore(/[.:>@#/\\-]$/) != null;
  if (!ctx.explicit && !punct && (!word || word.from === word.to)) return null;

  const path = projectStore.activeFilePath;
  if (!path) return null;

  const src = ctx.state.doc.toString();
  const byteOffset = makeU16ToByte(src)(ctx.pos);

  const seq = ++completionSeq;
  let items: CompletionItem[];
  try {
    items = await ipcCompletion(path, byteOffset, src);
  } catch {
    return null; // the backend is absent — no popup, no error
  }
  if (seq !== completionSeq) return null; // superseded by a newer keystroke
  if (!items.length) return null;

  // Sort by the server's own key before handing the list over. CodeMirror's own scoring is a
  // fuzzy match on the label, which for Rust puts `zip` above `iter` when you typed `it` —
  // rust-analyzer's `sortText` already encodes relevance (locals before trait methods before
  // the rest) and is the better order.
  const sorted = [...items].sort(compareBySortText);

  // The token CodeMirror should replace. Preferring the provider's own range would be more
  // precise, but it is expressed in byte offsets per item and CodeMirror wants one `from` for
  // the whole list — so the identifier under the caret it is, which is what every item's range
  // agrees on in practice.
  const from = word ? word.from : ctx.pos;

  return {
    from,
    options: sorted.map((it, rank) => toCompletion(it, rank, path)),
    // Keep the popup open while the user keeps typing identifier characters. A server that
    // marked its list incomplete would want a re-request per keystroke; treating every list as
    // filterable is the cheaper default and is right for the languages here.
    validFor: /^[\w$]*$/,
  };
};

/** Order by the provider's `sort_text` when both have one, else by label. */
function compareBySortText(a: CompletionItem, b: CompletionItem): number {
  const sa = a.sort_text;
  const sb = b.sort_text;
  if (sa && sb && sa !== sb) return sa < sb ? -1 : 1;
  if (sa && !sb) return -1;
  if (!sa && sb) return 1;
  return a.label.localeCompare(b.label);
}

/**
 * The hover source.
 *
 * Goes through the **shared** `bennu_hover`, which the backend answers from whichever engine
 * owns the file. The card's three slots (signature / container / doc) are filled by the backend
 * from the server's markdown, so there is nothing language-specific here — which is why this is
 * not a per-language function.
 */
function lspHoverSource(
  view: EditorView,
  pos: number,
  _side: -1 | 1,
): Promise<Tooltip | null> | null {
  const path = projectStore.activeFilePath;
  if (!path) return null;

  // Expand the identifier around the pointer. Same word shape as the Java hover, plus `!` so a
  // Rust macro (`println!`) hovers as one token rather than as `println`.
  const line = view.state.doc.lineAt(pos);
  const rel = pos - line.from;
  const text = line.text;
  const isWord = (c: string) => /[\w$]/.test(c);
  let s = rel;
  let e = rel;
  while (s > 0 && isWord(text[s - 1])) s--;
  while (e < text.length && isWord(text[e])) e++;
  if (s === e) return null;
  if (e < text.length && text[e] === '!') e++;

  const from = line.from + s;
  const to = line.from + e;
  const src = view.state.doc.toString();
  const byteOffset = makeU16ToByte(src)(from + Math.floor((e - s) / 2));

  return ipcHover(path, src, byteOffset)
    .then((info) => {
      if (!info) return null;
      return {
        pos: from,
        end: to,
        above: true,
        create: () => ({ dom: hoverCardDom(info) }),
      } as Tooltip;
    })
    .catch(() => null);
}

/**
 * Signature help, rendered as a hover-style tooltip at the caret.
 *
 * Not wired as `intel.hover` (that is pointer-driven); the host calls
 * {@link fetchSignatureHelp} on a caret move inside an argument list. Kept here so the
 * formatting of the active parameter lives beside the rest of the language's presentation.
 */
export async function fetchSignatureHelp(
  file: string,
  source: string,
  byteOffset: number,
): Promise<LspSignature | null> {
  return lspSignatureHelp(file, source, byteOffset).catch(() => null);
}

/** Render a signature into a hover card, with the active parameter emphasised. */
export function signatureCardDom(sig: LspSignature): HTMLElement {
  const dom = document.createElement('div');
  dom.className = 'cm-hover-card';

  const head = document.createElement('div');
  head.className = 'cm-hc-head';
  const title = document.createElement('span');
  title.className = 'cm-hc-title';

  // The server gave the active parameter's span inside the label, so it can be bolded exactly
  // rather than by searching the label for the parameter's text — which goes wrong the moment
  // two parameters share a name or a type.
  const start = sig.active_start;
  const end = sig.active_end;
  if (start != null && end != null && end > start && end <= sig.label.length) {
    title.appendChild(document.createTextNode(sig.label.slice(0, start)));
    const active = document.createElement('strong');
    active.textContent = sig.label.slice(start, end);
    title.appendChild(active);
    title.appendChild(document.createTextNode(sig.label.slice(end)));
  } else {
    title.textContent = sig.label;
  }
  head.appendChild(title);
  dom.appendChild(head);

  if (sig.doc) {
    const doc = document.createElement('div');
    doc.className = 'cm-hc-doc';
    doc.textContent = sig.doc;
    dom.appendChild(doc);
  }
  return dom;
}

/**
 * Build a descriptor for a server-backed language.
 *
 * @param id           stable id, for parser caches and debugging (`rust`)
 * @param baseMode     the CodeMirror stream mode that provides the instant, local highlight
 * @param commentTokens the language's comment syntax, for `Ctrl+/`
 */
export function lspLanguage(
  id: string,
  baseMode: StreamParser<unknown>,
  commentTokens: { line?: string; block?: { open: string; close: string } },
): LanguageDescriptor {
  return {
    id,
    createParser: () =>
      Promise.reject(new Error(`lsp-language:${id} highlights from a CodeMirror mode`)),
    classify: () => null,
    cmExtension: StreamLanguage.define(baseMode),
    // A stream mode carries no fold information of its own, so the Lezer path stays off — and the
    // server's `textDocument/foldingRange` answers instead. It folds by *item*: a `use` block, a doc
    // comment, a `#[cfg]`-gated module, a match arm. The ranges are pushed by the editor host.
    cmFold: false,
    serverFold: true,
    commentTokens,
    intel: { completion: lspCompletionSource, hover: lspHoverSource },
  };
}
