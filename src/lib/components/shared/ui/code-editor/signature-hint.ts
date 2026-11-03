/**
 * Parameter hints — the signature of the call the caret is inside, with the argument you are on
 * marked.
 *
 * You type `transfer(` and a strip appears above the line reading
 * `transfer(String source, String target, long amount)` with `String source` picked out; a comma
 * moves the mark along. It answers the question that makes people stop typing and go and read the
 * method: *what does the third one want?*
 *
 * ## A tooltip above the caret, not a completion
 *
 * It is not a list and nothing is chosen from it, so it must not behave like completion: it never
 * takes the keyboard and never intercepts Enter. What it is, is a tooltip pinned above the call,
 * styled as the editor's other tooltips. The host decides when to ask for it (Bennu: on demand, by
 * shortcut) and closes it on Escape — {@link signatureHintActive} is there for that keymap.
 *
 * ## The provider decides what is shown; this decides when
 *
 * Resolving overloads is language work and lives on the other side of {@link setSignature}. Here
 * lives the part every language shares: keeping the strip up while the caret is in the argument
 * list, taking it down when it leaves, and re-asking after each keystroke that could have moved
 * which argument you are on.
 */

import { EditorView, showTooltip, type Tooltip } from '@codemirror/view';
import { StateEffect, StateField, type Extension } from '@codemirror/state';

/** The signature to show, and where the caret is inside it. */
export interface SignatureInfo {
  /** The rendered signature — `transfer(String source, String target, long amount)`. */
  label: string;
  /**
   * The `[start, end)` ranges, within `label`, of each parameter.
   *
   * Ranges rather than a parameter list, for the same reason the snippet stops are ranges: the
   * provider has already rendered the label, and asking it to render the parts a second time so
   * this can reassemble them invites the two renderings to disagree.
   */
  params: readonly { start: number; end: number }[];
  /** Index into `params` of the argument the caret is on; out-of-range marks nothing. */
  active: number;
  /** The documentation line shown under the signature, if the provider has one. */
  doc?: string | null;
  /** `2 of 3` overload counters, when the call is ambiguous and the provider picked one. */
  overload?: { index: number; count: number } | null;
  /** Document offset the strip is anchored to — normally the call's opening paren. */
  pos: number;
}

/** Show (or replace) the parameter hint. */
export const setSignature = StateEffect.define<SignatureInfo>();
/** Take it down. */
export const clearSignature = StateEffect.define<null>();

/** Build the strip's DOM: the signature with the active parameter marked, and its doc line. */
function render(info: SignatureInfo): HTMLElement {
  const dom = document.createElement('div');
  dom.className = 'cm-sig-hint';

  const line = document.createElement('div');
  line.className = 'cm-sig-line';

  const active = info.params[info.active];
  if (active && active.start >= 0 && active.end <= info.label.length && active.start < active.end) {
    // Three spans rather than innerHTML: the label is provider text and goes in as text, always.
    line.appendChild(document.createTextNode(info.label.slice(0, active.start)));
    const mark = document.createElement('span');
    mark.className = 'cm-sig-active';
    mark.textContent = info.label.slice(active.start, active.end);
    line.appendChild(mark);
    line.appendChild(document.createTextNode(info.label.slice(active.end)));
  } else {
    line.textContent = info.label;
  }
  dom.appendChild(line);

  if (info.overload && info.overload.count > 1) {
    const badge = document.createElement('span');
    badge.className = 'cm-sig-overload';
    badge.textContent = `${info.overload.index + 1} of ${info.overload.count}`;
    line.appendChild(badge);
  }

  if (info.doc) {
    const doc = document.createElement('div');
    doc.className = 'cm-sig-doc';
    doc.textContent = info.doc;
    dom.appendChild(doc);
  }
  return dom;
}

const signatureField = StateField.define<Tooltip | null>({
  create() {
    return null;
  },
  update(tooltip, tr) {
    for (const effect of tr.effects) {
      if (effect.is(clearSignature)) return null;
      if (effect.is(setSignature)) {
        const info = effect.value;
        return {
          pos: Math.min(Math.max(info.pos, 0), tr.state.doc.length),
          above: true,
          // No arrow, and never taking focus: it is a label, not a thing to interact with.
          arrow: false,
          create: () => ({ dom: render(info) }),
        };
      }
    }
    if (!tooltip) return null;
    // An edit that isn't followed by a fresh signature would leave the strip anchored to a position
    // that has moved. Ride the changes until the provider answers again.
    if (tr.docChanged) {
      return { ...tooltip, pos: tr.changes.mapPos(tooltip.pos, -1) };
    }
    return tooltip;
  },
  provide: (f) => showTooltip.from(f),
});

const signatureTheme = EditorView.baseTheme({
  '.cm-sig-hint': {
    // Surface, border and radius come from the editor's `.cm-tooltip`, so this card is the same
    // object as the hover card; only its contents are styled here, with the hover card's tokens.
    padding: '8px 11px',
    maxWidth: '640px',
    fontFamily: 'var(--font-ui-sans)',
    color: 'var(--text-secondary)',
  },
  '.cm-sig-line': {
    whiteSpace: 'pre-wrap', wordBreak: 'break-word', lineHeight: '1.5',
    fontFamily: 'var(--font-code)', fontSize: 'var(--font-size-sm)',
  },
  '.cm-sig-active': {
    color: 'var(--text-primary)',
    fontWeight: '600',
    // Underlined as well as bold: on a signature of several same-typed parameters, weight alone is
    // easy to lose track of at a glance.
    textDecoration: 'underline',
    textUnderlineOffset: '2px',
  },
  '.cm-sig-overload': {
    marginLeft: '8px',
    fontFamily: 'var(--font-ui-sans)',
    fontSize: 'var(--font-size-2xs)',
    color: 'var(--text-muted)',
  },
  '.cm-sig-doc': {
    marginTop: '5px',
    paddingTop: '5px',
    borderTop: '1px solid var(--border-subtle)',
    fontSize: 'var(--font-size-xs)',
    color: 'var(--text-secondary)',
    whiteSpace: 'pre-wrap',
  },
});

/** Whether a parameter hint is currently up — for a keymap that wants to close it with Escape. */
export function signatureHintActive(view: EditorView): boolean {
  return view.state.field(signatureField, false) != null;
}

/** Ask for the hint at the caret right now, whatever the trigger was. */
export function showSignatureHint(view: EditorView, info: SignatureInfo) {
  view.dispatch({ effects: setSignature.of(info) });
}

/** The parameter-hint extension. */
export function signatureHints(): Extension {
  return [signatureField, signatureTheme];
}
