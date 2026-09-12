/**
 * One provider [`CompletionItem`] → one CodeMirror `Completion`.
 *
 * ## Why this is shared
 *
 * The wire shape is one shape for every language — that is the point of the provider seam — and
 * the conversion was written twice anyway: once for the native Java engine and once for the
 * language-server path. They drifted exactly where you would expect. The server path honoured
 * `insert_text`, the snippet stops, the extra edits and the documentation panel; the Java path
 * honoured none of them, so a field the backend had always been able to send simply did nothing
 * when Java sent it. Adding the parentheses to a method call would have had to be written a
 * second time, in the copy that did not know how.
 *
 * So: one converter, and what varies between the two is passed in — how a documentation panel is
 * built, and what happens after the text lands.
 *
 * ## What it deliberately does not decide
 *
 * The **order**. `boost` carries the provider's own ranking across, because CodeMirror re-scores
 * by fuzzy match and would throw away everything the index knew (see `completion-rank`). Which
 * band an item belongs in is the caller's, since only it knows whether the item was resolved or
 * guessed at.
 */

import { insertCompletionText, type Completion } from '@codemirror/autocomplete';
import type { EditorView } from '@codemirror/view';
// The two modules directly rather than the editor's barrel: the barrel re-exports `.svelte`
// components, and importing it pulls a Svelte compile into anything that only wants these
// functions — the unit tests included.
import { insertWithStops } from '$lib/components/shared/ui/code-editor/snippet-stops';
import { makeByteToU16 } from '$lib/components/shared/ui/code-editor/highlight';
import type { CompletionItem, SourceEdit } from '$lib/types/bennu';

/**
 * A completion carrying the **origin** the popup draws on the right of the row.
 *
 * CodeMirror's own `Completion` has a label and a detail and no third slot, and the third slot is
 * what tells `List.of` from `Set.of` — and an inherited method from one of your own. Declared as
 * an extension of the library's type rather than smuggled through `any`, so the renderer that
 * reads it and the converters that write it agree about the name.
 */
export interface RichCompletion extends Completion {
  /** The simple name of the declaring type, or nothing when the item has no owner. */
  origin?: string;
  /** Struck through in the list. Still offered — it exists, and you may be reading old code. */
  isDeprecated?: boolean;
  /**
   * Drawn in grey at the caret while this row is selected.
   *
   * For a row that WRITES something — a member the class does not have yet — the label says
   * `getCustomer` and the insertion is four lines, so the list alone cannot tell you what Enter
   * will do. This is that outcome, previewed where it will land. It is the popup's own selection
   * rendered, not a second proposal, so it costs the popup nothing and competes for no key.
   */
  ghost?: string;
}

/** What varies between one provider and the next. */
export interface ItemHooks {
  /**
   * Build the documentation panel for this item, or `null` when there is nothing to show.
   *
   * Called by CodeMirror only for the row the user actually highlights, which is why a lazy
   * fetch belongs here rather than in the list: a member list for a busy receiver is hundreds of
   * candidates, and documentation for a library one is read out of an archive on disk.
   */
  info?: (item: CompletionItem) => Promise<Node | null> | Node | null;
  /** Runs once the inserted text has landed — Java's auto-import, and the acceptance the
   *  ranking memory learns from. */
  after?: (view: EditorView, item: CompletionItem) => void;
}

/**
 * Apply a provider's extra edits (for Rust, the `use` line an auto-imported item needs).
 *
 * Dispatched as a **second** transaction, after the insertion. Both are byte-offset edits
 * computed against the pre-insertion buffer, and an import sits above the caret — so applying the
 * insertion first leaves the import's offsets untouched, whereas one combined transaction would
 * have to reason about which of the two shifts the other.
 */
export function applyAdditionalEdits(
  view: EditorView,
  edits: SourceEdit[],
  preInsertSource: string,
): void {
  if (!edits.length) return;
  const b2u = makeByteToU16(preInsertSource);
  // Descending, so an earlier edit's offsets are still valid after a later one is applied.
  const mapped = edits
    .map((e) => ({ from: b2u(e.start), to: b2u(e.end), insert: e.new_text }))
    .sort((a, b) => b.from - a.from);
  view.dispatch({ changes: mapped });
}

/**
 * Map a provider `kind` to a CodeMirror completion `type` — what drives the popup's kind icon.
 *
 * One map for every provider. A language server's vocabulary is the broader of the two (it tells
 * a struct from a class, an enum member from a constant), and the native Java engine adds the two
 * it needs that no server sends: `annotation`, which gets its own glyph because after an `@` the
 * whole list is annotations and the glyph is what says the filter took, and `parameter`, which is
 * a variable you did not declare.
 */
export function kindToType(kind: string): string {
  switch (kind) {
    case 'method':
    case 'function':
    case 'constructor':    return 'method';
    case 'field':
    case 'property':       return 'property';
    case 'class':
    case 'struct':
    case 'interface':
    case 'enum':
    case 'event':
    case 'type':           return 'class';
    case 'type-parameter': return 'type';
    case 'variable':       return 'variable';
    case 'parameter':      return 'variable';
    case 'keyword':        return 'keyword';
    case 'constant':
    case 'enum-member':    return 'constant';
    case 'annotation':     return 'annotation';
    // A member that does not exist yet. Its own glyph because the row is not naming something
    // that is there: drawing it identically to one that is would be claiming it was.
    case 'generate':       return 'generate';
    case 'module':
    case 'package':        return 'namespace';
    case 'snippet':        return 'text';
    default:               return 'text';
  }
}

/**
 * A member collapsed to the one line that fits **beside an open popup**.
 *
 * The popup opens directly under the caret, so a four-line preview drawn at the caret is a four-line
 * preview with a list on top of it. The line that carries the information is the first one — the
 * signature — and the body of a generated member is implied by its name anyway: an accessor returns
 * its field, a stub throws. So the body becomes `{ … }` and the whole thing fits where it is.
 *
 * The full text is not lost: it is what accepting writes, and what the popup's own documentation
 * panel shows beside the list. And with the popup CLOSED the standalone proposal is drawn in full,
 * because there nothing is competing for the space.
 */
function headlineOf(text: string | undefined): string | undefined {
  if (!text) return undefined;
  const flat = text.replace(/\s*\r?\n\s*/g, ' ').trim();
  const brace = flat.indexOf('{');
  return brace < 0 ? flat : `${flat.slice(0, brace).trimEnd()} { … }`;
}

/** The simple name of a binary name — `org/springframework/web/cors/CorsConfiguration` →
 *  `CorsConfiguration`, `com/x/Outer$Inner` → `Inner`. What the popup has room to show. */
export function simpleOwner(binary: string): string {
  const parts = binary.split(/[/$]/);
  return parts[parts.length - 1] || binary;
}

/**
 * Convert one item, with `boost` already decided by the caller.
 *
 * `label` is a **display** string and `insert_text` is what goes in the buffer; inserting the
 * label verbatim is how accepting a completion produces code that does not compile. For the Java
 * engine that difference is the parentheses of a call.
 */
export function toCompletion(item: CompletionItem, boost: number, hooks: ItemHooks = {}): RichCompletion {
  const completion: RichCompletion = {
    label: item.label,
    detail: item.detail ?? undefined,
    type: kindToType(item.kind),
    boost,
    origin: item.owner ? simpleOwner(item.owner) : undefined,
    isDeprecated: item.deprecated || undefined,
    // Only where the label cannot stand for the insertion: a generated member is several lines of
    // code behind a one-word row. An ordinary completion inserts what it says.
    ghost: item.kind === 'generate' ? headlineOf(item.insert_text) : undefined,
  };

  const insert = item.insert_text ?? item.label;
  const extras = item.edits ?? [];
  const stops = item.snippet_stops ?? [];
  // A snippet is written as code at column 0 and lands wherever the caret is, so every line after the
  // first needs the caret line's indentation — `insertWithStops` is what adds it, and a template with
  // no tab stops at all needs it just as much as one with three. Anything else is inserted as it came:
  // a generated member arrives already indented to the class it goes into, and indenting it twice is
  // the same defect the other way round.
  const reindents = !!item.snippet && insert.includes('\n');
  const needsCustomApply =
    insert !== item.label || extras.length > 0 || stops.length > 0 || reindents || !!hooks.after;

  if (needsCustomApply) {
    completion.apply = (view, _c, from, to) => {
      const pre = view.state.doc.toString();
      // `insert_text` is plain text either way — the placeholder syntax is parsed away in the
      // backend and what is left of it is the stops, as byte ranges into it. So a snippet differs
      // from a plain completion only in what happens *after* the text lands.
      if (stops.length > 0 || reindents) {
        insertWithStops(view, from, to, insert, stops, makeByteToU16(insert));
      } else {
        view.dispatch(insertCompletionText(view.state, insert, from, to));
      }
      applyAdditionalEdits(view, extras, pre);
      hooks.after?.(view, item);
    };
  }

  if (hooks.info) {
    const build = hooks.info;
    completion.info = () => build(item);
  }
  return completion;
}
