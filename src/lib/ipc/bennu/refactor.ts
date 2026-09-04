/**
 * Bennu refactoring IPC — extract method, extract variable and constant, inline variable, inline
 * method.
 *
 * ## Two calls, and why not one
 *
 * {@link refactorings} asks *what can be done here* and is what fills the Alt+Enter list;
 * {@link refactorPlan} asks for the edits of the one that was chosen. They are separate because a
 * keystroke can happen between the menu opening and a row being picked, and a plan computed against
 * text that has since changed is a plan that corrupts the buffer. The second call re-reads the
 * buffer, so the edits always belong to the text they are applied to.
 *
 * ## Refusals are rows
 *
 * An offer with a `reason` is one that cannot be done here, and it is shown greyed **with** the
 * reason rather than hidden: "the selection produces `total` and `count`, and a method can only
 * return one" says what to change. This is also how a language server's disabled code actions are
 * rendered, so the two read the same in one menu.
 */

import { bennu } from '../rpc';

/** One row of the refactoring list. */
export interface RefactorOffer {
  /** Stable id, sent back to {@link refactorPlan}. */
  id: string;
  label: string;
  /** Empty when the refactoring applies; otherwise why it does not. */
  reason: string;
  /** The name it would introduce, when it introduces one. */
  name: string;
}

/** One byte-range replacement. */
export interface RefactorEdit {
  start: number;
  end: number;
  text: string;
  /** `call` · `declaration` · `body` · `use` · `import` — what the edit is for. */
  reason: string;
}

/** A planned refactoring. */
export interface RefactorPlan {
  id: string;
  label: string;
  /** **Descending by start.** Applied in this order they need no offset re-mapping — which is why
   *  the backend sorts them and the editor must not. */
  edits: RefactorEdit[];
  /** The introduced name, for offering a rename straight after. */
  name: string;
  /** Where the caret should land. */
  caret: number | null;
  /** True when no type could be resolved and the declaration still says `var`. Worth telling the
   *  user on a project targeting Java 8, where `var` does not compile. */
  unresolved_type: boolean;
}

/** What can be refactored at the caret (`start === end`) or over the selection. Byte offsets.
 *  Wire: `bennu_refactorings`. */
export function refactorings(
  file: string,
  source: string,
  start: number,
  end: number,
): Promise<RefactorOffer[]> {
  return bennu('bennu_refactorings', { args: { file, source, start, end } });
}

/** The edits for one refactoring, computed against the buffer as it is now. Wire:
 *  `bennu_refactor_plan`. */
export function refactorPlan(
  file: string,
  source: string,
  start: number,
  end: number,
  id: string,
): Promise<RefactorPlan> {
  return bennu('bennu_refactor_plan', { args: { file, source, start, end, id } });
}

/** Create the file for a type that does not resolve, beside the file that names it, and answer with
 *  its path. Wire: `bennu_create_class`. */
export function createClass(
  file: string,
  source: string,
  start: number,
  end: number,
): Promise<string> {
  return bennu('bennu_create_class', { args: { file, source, start, end } });
}
