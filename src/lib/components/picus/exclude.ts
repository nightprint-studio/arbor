/**
 * Taking something out of the project — and putting it back.
 *
 * The third sibling of `folder-classify.ts` and `file-classify.ts`, and the one
 * that answers a different question. Those two say *what* something is; this one
 * says whether it is here at all. There are migration scripts nobody wants
 * counted, and a repository that keeps them in a folder of its own has no way to
 * say so with an engine and a role.
 *
 * ## Not the `ignored` role, and the wording must never blur them
 *
 * `role = "ignored"` says **this is not an installation folder**: nothing is
 * generated into it and it takes part in no cross-dialect comparison, but it is
 * still read, its objects still appear in the inventory, and its files are still
 * checked. That is deliberate — knowing that `MIGRAZIONE_2019` creates a table is
 * worth having.
 *
 * Exclusion says **pretend this is not in the repository**: not parsed, not
 * indexed, no coverage column, no findings, never a destination.
 *
 * They cannot be merged, because `ignored` is also the *fallback* for a folder
 * nobody has classified — making the fallback mean "excluded" would silently drop
 * from the report exactly the folders that need attention. So nothing here is
 * ever labelled "ignore": the verb is **exclude**, and its opposite is **put
 * back**.
 *
 * ## One module for a folder and a script
 *
 * Like the verb behind it: it is one decision taken about one row, and the path
 * names whichever it is. What differs is only the sentence — and the one case
 * where the sentence has to differ, which is the whole reason this file exists as
 * more than a call:
 *
 *  • a row that **declares** its own exclusion is *undoing its own decision*;
 *  • a row excluded **because something above it is** is being *rescued*, and the
 *    write is not the same one — it puts `excluded = false` on that row, which is
 *    the only way to keep the single migration that does matter without moving it
 *    on disk.
 *
 * An excluded row is never hidden, here or in the tree. Hiding it would leave no
 * way to change one's mind.
 */

import { PackageMinus, PackagePlus } from 'lucide-svelte';
import type { MenuItem } from '$lib/components/shared/ContextMenu.svelte';
import { toastStore } from '$lib/feedback/stores/toasts.svelte';
import { picusProjectStore } from '$lib/stores/picus/project.svelte';
import {
  declaresExclusion,
  excludedByAncestor,
  isExcluded,
  type Excludable,
  type FolderNode,
  type ScriptFile,
} from '$lib/types/picus';

/** Menu ids. Two, because the action has two directions and no third state. */
export const EXCLUDE_ID = 'exclusion:out';
export const INCLUDE_ID = 'exclusion:in';

/** The row the decision is about, whichever kind of row it is. */
export interface ExclusionTarget {
  kind: 'folder' | 'file';
  /** Project-relative path — the identity the backend answers to. */
  path: string;
  /** What the row shows, and what the confirmation names. */
  name: string;
  /** Where it stands: declared here, inherited, or not at all. */
  subject: Excludable;
}

export function folderTarget(node: FolderNode): ExclusionTarget {
  return { kind: 'folder', path: node.path, name: node.name || node.path, subject: node };
}

export function fileTarget(file: ScriptFile): ExclusionTarget {
  return { kind: 'file', path: file.path, name: file.name, subject: file };
}

/** `folder` or `script` — the word every sentence below is built on. */
function noun(target: ExclusionTarget): string {
  return target.kind === 'folder' ? 'folder' : 'script';
}

/**
 * The nearest folder **above** this row that excluded itself, if any.
 *
 * Named in the menu so "put this back" is never a mystery: the decision is not on
 * the row the user is looking at, and without the path they would have to guess
 * which ancestor to go and correct.
 */
function exclusionSource(target: ExclusionTarget): string {
  let cursor: string | null =
    target.kind === 'file'
      ? picusProjectStore.folderOfFile(target.path)?.node.path ?? null
      : picusProjectStore.entryFor(target.path)?.parent ?? null;
  while (cursor !== null) {
    const entry = picusProjectStore.entryFor(cursor);
    if (!entry) return '';
    if (entry.node.excluded === true) return entry.node.path;
    cursor = entry.parent;
  }
  return '';
}

/** The one thing this row can be asked, in the direction it would go. */
export interface ExclusionAction {
  /** What the write would set. */
  excluded: boolean;
  /**
   * The sentence, given whatever the caller calls the row.
   *
   * A menu row is looking at the thing and says `this script`; a palette entry is
   * not and says `4_12_POS.sql`. Both take the same sentence around it, which is
   * the point of building it here rather than twice.
   */
  label: (subject: string) => string;
  /** `folder` or `script` — what a menu puts after "this". */
  noun: string;
  /** Why, or where the decision was taken. Empty when the label says it all. */
  detail: string;
  icon: typeof PackageMinus;
}

/**
 * What this row offers about being in the project — in whichever of its three
 * senses applies.
 *
 * One action and never two: "exclude" and "put back" are the same switch, and
 * offering both would make the user work out which one is the current state.
 * Computed here so the row menu and the command palette say the same sentence
 * about the same row — including the sentence that is *not* interchangeable,
 * the rescue.
 */
export function exclusionAction(target: ExclusionTarget): ExclusionAction {
  const { subject } = target;
  const thing = noun(target);

  if (!isExcluded(subject)) {
    // Declaring anything while not excluded can only mean `excluded = false`:
    // this row was rescued from an excluded folder, and saying so is the
    // difference between "nothing has been decided here" and "you decided this".
    const rescuedFrom = declaresExclusion(subject) ? exclusionSource(target) : '';
    return {
      excluded: true,
      label: (s) => `Exclude ${s} from the project`,
      noun: thing,
      // The consequence, not the mechanism: this is the one action in Picus that
      // makes findings disappear, and it should say so before it is taken.
      detail: rescuedFrom
        ? `Kept in the excluded ${rescuedFrom}`
        : 'Not read, not indexed, never a destination',
      icon: PackageMinus,
    };
  }

  // Excluded because an ancestor is: this is a rescue, and it reads as one. The
  // write lands on THIS row (`excluded = false`), which is what lets one script
  // stay in a folder full of ones that do not.
  if (excludedByAncestor(subject)) {
    const source = exclusionSource(target);
    return {
      excluded: false,
      label: (s) => `Keep ${s} in the project`,
      noun: thing,
      detail: source ? `Excluded by ${source}` : 'Excluded from above',
      icon: PackagePlus,
    };
  }

  return {
    excluded: false,
    label: (s) => `Put ${s} back into the project`,
    noun: thing,
    detail: '',
    icon: PackagePlus,
  };
}

/** The one entry a row's context menu offers about being in the project. */
export function exclusionItems(target: ExclusionTarget): MenuItem[] {
  const action = exclusionAction(target);
  return [
    {
      id: action.excluded ? EXCLUDE_ID : INCLUDE_ID,
      label: action.label(`this ${action.noun}`),
      icon: action.icon,
      subtitle: action.detail || undefined,
    },
  ];
}

/** Turn a menu id into a write. Returns `true` when the id was one of ours. */
export async function runExclusionId(target: ExclusionTarget, id: string): Promise<boolean> {
  if (id !== EXCLUDE_ID && id !== INCLUDE_ID) return false;
  await setExcluded(target, id === EXCLUDE_ID);
  return true;
}

/**
 * Take it out, or put it back, and say so.
 *
 * Returns `true` on success. The toast quotes the file the backend says it wrote,
 * for the same reason every other write in Picus does: this puts something into
 * the user's own repository, and a tool that does that says where.
 */
export async function setExcluded(
  target: ExclusionTarget,
  excluded: boolean,
): Promise<boolean> {
  const message = await picusProjectStore.setExcluded(target.path, excluded);
  if (message) {
    toastStore.show(`${target.path} could not be changed — ${message}`, 'error');
    return false;
  }
  reveal(target, excluded);
  const where = picusProjectStore.configPath;
  const said = excluded
    ? `${target.name} is outside the project — it stays in the tree, marked`
    : `${target.name} is part of the project again`;
  toastStore.show(`${said}${where ? `. Saved in ${where}` : ''}`, 'success');
  return true;
}

/**
 * Put the row that just changed on screen.
 *
 * Excluding a folder **closes** it, so what has to be visible afterwards is the
 * row itself and not its contents — opening the folder we just took out of the
 * project would contradict the decision on screen. Everything else is revealed
 * as usual, because a confirmation nobody can see is a claim.
 */
function reveal(target: ExclusionTarget, excluded: boolean): void {
  if (target.kind === 'file') {
    picusProjectStore.revealFile(target.path);
    return;
  }
  if (!excluded) {
    picusProjectStore.revealFolder(target.path);
    return;
  }
  const parent = picusProjectStore.entryFor(target.path)?.parent;
  if (parent) picusProjectStore.revealFolder(parent);
}
