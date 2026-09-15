/**
 * How one completion row is drawn beyond CodeMirror's label and detail: the kind icon, the
 * parameter list beside the name, the origin on the right edge, and the row classes the theme reads.
 *
 *   [icon]  name(Type param, …)   ………   ReturnType   Origin
 *
 * ## The row, IntelliJ's way
 *
 * - A member the receiver **declares** is drawn in bold and names no origin: it is the thing you are
 *   most likely looking for, and "declared by the class you are completing on" goes without saying.
 * - An **inherited** member keeps the normal weight and names where it comes from.
 * - An **implicit** member — what every object has, `java.lang.Object`'s — is dimmed as well.
 *
 * A provider that says nothing about standing gets none of this, and the origin is shown whenever
 * it names one, as before.
 *
 * App-agnostic: the fields are optional properties on CodeMirror's own `Completion`, and a product's
 * converter (Bennu's `completion-item`) is what fills them in.
 */

import type { Completion } from '@codemirror/autocomplete';
import { completionIconMarkup, describeCompletionIcon, type IconModifier } from './completion-icons';

/** Where a member stands relative to the receiver it was offered on. */
export type CompletionStanding = 'own' | 'inherited' | 'implicit';

/** The optional fields a rich completion carries on top of CodeMirror's. */
export interface RichCompletionFields {
  /** The simple name of the declaring type, or nothing when the item has no owner. */
  origin?: string;
  /** Struck through in the list. Still offered — it exists, and you may be reading old code. */
  isDeprecated?: boolean;
  /** The parameter list drawn right after the name, `(Object obj)`. */
  signature?: string;
  /** See {@link CompletionStanding}. */
  standing?: CompletionStanding;
  /** Modifiers marked on the icon. */
  modifiers?: readonly IconModifier[];
}

/** A CodeMirror completion with the rich fields. */
export type RichOption = Completion & RichCompletionFields;

/**
 * The origin column's text, or nothing.
 *
 * Nothing for a member the receiver declares — the popup would otherwise repeat the receiver's name
 * on most of its rows — and nothing when the item names no owner.
 */
export function originText(option: RichOption): string | undefined {
  if (option.standing === 'own') return undefined;
  return option.origin || undefined;
}

/** The classes the theme reads off a row: its standing and whether it is deprecated. */
export function rowClass(option: RichOption): string {
  const classes: string[] = [];
  if (option.standing) classes.push(`cm-completion-${option.standing}`);
  if (option.isDeprecated) classes.push('cm-completion-deprecated');
  return classes.join(' ');
}

function span(className: string, text: string): HTMLElement {
  const el = document.createElement('span');
  el.className = className;
  el.textContent = text;
  return el;
}

/** The kind icon, in the slot CodeMirror's own icon occupies. */
function renderKindIcon(completion: Completion): HTMLElement {
  const option = completion as RichOption;
  const icon = describeCompletionIcon(option.type, option.modifiers, option.isDeprecated);
  const el = document.createElement('span');
  el.className = 'cm-completionKindIcon';
  el.dataset.kind = icon.kind.type;
  // Built from a fixed table with no provider text in it — see `completion-icons`.
  el.innerHTML = completionIconMarkup(icon);
  return el;
}

/** The parameter list, straight after the label. */
function renderSignature(completion: Completion): HTMLElement | null {
  const signature = (completion as RichOption).signature;
  return signature ? span('cm-completionSignature', signature) : null;
}

/** The right-hand column: where a candidate comes from. */
function renderOrigin(completion: Completion): HTMLElement | null {
  const origin = originText(completion as RichOption);
  return origin ? span('cm-completionOrigin', origin) : null;
}

/**
 * The parts CodeMirror adds to every row (`addToOptions`), with CodeMirror's own icons turned off.
 *
 * Positions against CodeMirror's slots: its icon is 20, the label 50, the detail 80. The signature
 * sits between label and detail so it reads as part of the name; the origin goes last, and the
 * theme pushes it to the right edge.
 */
export const completionRowParts = [
  { render: renderKindIcon, position: 20 },
  { render: renderSignature, position: 60 },
  { render: renderOrigin, position: 90 },
];
