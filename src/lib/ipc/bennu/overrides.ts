/**
 * Bennu "Implement / override methods" IPC.
 *
 * Two calls, because the dialog is a conversation: ask what the class under the caret could
 * override, let the user tick some, then ask for the edits that write them.
 *
 * The selection travels back **whole** rather than as indices into the first answer. The buffer can
 * change between the two calls — a keystroke, a save, a formatter — and an index into a list
 * computed against different text is how a generator writes the wrong method.
 */

import { bennu } from '../rpc';

/** One method the caret's class could override. Mirrors the BE `OverridableWire`. */
export interface OverridableMember {
  name: string;
  /** `[type, name]` pairs in declaration order. */
  params: [string, string][];
  return_type: string;
  /** `"public"` | `"protected"` | `"package"`. */
  visibility: string;
  /** The compiler will demand it — these are ticked when the dialog opens. */
  is_abstract: boolean;
  throws: string[];
  /** Dotted FQCN of the declaring type — what the dialog groups by. */
  declaring_type: string;
  /** A readable one-line signature for the row. */
  signature: string;
  /** Binary names of every type the generated method mentions, so the answer can carry its
   *  imports. Opaque to the frontend: it is handed straight back. */
  types: string[];
}

/** A byte range in the requested buffer to replace. */
export interface GeneratedEdit {
  start: number;
  end: number;
  replacement: string;
}

/** Every method the class at `offset` could override, abstract ones first. Empty when the caret is
 *  not inside a class or the index is still building.
 *  Wire: `bennu_overridable_members` — `{ file, source, offset }`. */
export function overridableMembers(
  file: string,
  source: string,
  offset: number,
): Promise<OverridableMember[]> {
  return bennu('bennu_overridable_members', { args: { file, source, offset } });
}

/**
 * The edits that write `selected` into the class at `offset`.
 *
 * Returned **highest offset first**, so applying them in order needs no remapping: every edit is
 * above the ones already applied. There is usually more than one — the methods go inside the class
 * brace, and each type they mention that the file does not import gets an `import` line, without
 * which the generated code does not compile.
 *
 * Wire: `bennu_generate_overrides` — `{ source, offset, selected }`.
 */
export function generateOverrides(
  source: string,
  offset: number,
  selected: OverridableMember[],
): Promise<GeneratedEdit[]> {
  return bennu('bennu_generate_overrides', { args: { source, offset, selected } });
}
