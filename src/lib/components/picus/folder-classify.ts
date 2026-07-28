/**
 * Saying what a folder is — the menu and the write, in one place.
 *
 * Classifying a folder is reachable from three places (the tree row's menu, the
 * tree row's keyboard, the command palette's dialog) and it must mean the same
 * thing in all three: the same options, the same wording, the same confirmation,
 * the same "clear it back to inherited". So the menu and the call live here and
 * the three call sites only decide how they are presented.
 *
 * The **answers** themselves are not here — they are in `engine-choices.ts`,
 * because a file and a name are classified with exactly the same list and three
 * copies of one vocabulary is three chances for "portable" to go missing from
 * one of them. `file-classify.ts` is this file's sibling, one level down.
 *
 * `null` is a first-class choice throughout, not an absence: **clearing** a wrong
 * guess is a thing users need to do, and it is a different act from never having
 * said anything. `ipc/picus/project.ts` documents the three-valued wire.
 *
 * ## And the offer that turns one decision into eleven
 *
 * Classifying `AGGIORNAMENTO/2024/POS` says something about one folder. In a
 * repository with a folder set per delivered version there are ten more called
 * `POS` and there will be another next month. {@link offerAliasFor} raises the
 * second question at the moment the user has the answer — as a **separate**
 * decision, never as a side effect of the first. What that offer then says, and
 * what accepting it writes, is `alias-offer.ts`.
 */

import { Database, Eraser, FileCog, FolderCog, Layers } from 'lucide-svelte';
import type { MenuItem } from '$lib/components/shared/ContextMenu.svelte';
import { toastStore } from '$lib/feedback/stores/toasts.svelte';
import { picusProjectStore, type FolderEntry } from '$lib/stores/picus/project.svelte';
import { picusUiStore } from '$lib/stores/picus/ui.svelte';
import {
  CLEAR_ID,
  ROLE_CHOICES,
  engineMenuItems,
  roleMenuId,
} from './engine-choices';
import {
  FOLDER_ROLE_LABELS,
  aliasScope,
  declaredEngine,
  engineLabel,
  folderEngine,
  scopeCoversFolders,
  type FolderEngine,
  type FolderRole,
} from '$lib/types/picus';

/**
 * The two submenus a folder row offers, with the current answer marked.
 */
export function folderClassifyItems(entry: FolderEntry): MenuItem[] {
  const { node } = entry;
  const declaredRole = node.role;

  const dialectItems = engineMenuItems({
    declared: declaredEngine(node),
    effective: folderEngine(node),
    from: entry.dialectFrom && entry.dialectFrom !== node.path ? entry.dialectFrom : '',
  });

  const roleItems: MenuItem[] = [
    ...ROLE_CHOICES.map((r): MenuItem => ({
      id: roleMenuId(r),
      label: FOLDER_ROLE_LABELS[r],
      icon: FileCog,
      badge:
        declaredRole === r ? 'set here'
          : node.effectiveRole === r && declaredRole === null ? 'inherited'
            : undefined,
      badgeAccent: declaredRole === r,
    })),
    { id: 'role-sep', label: '', separator: true },
    {
      id: `role:${CLEAR_ID}`,
      label: 'Inherit from above',
      icon: Eraser,
      disabled: declaredRole === null,
      subtitle: entry.roleFrom && entry.roleFrom !== node.path ? entry.roleFrom : undefined,
    },
  ];

  return [
    { id: 'engine', label: 'Engine', icon: Database, children: dialectItems },
    { id: 'role', label: 'Role', icon: Layers, children: roleItems },
    { id: 'classify-sep', label: '', separator: true },
    { id: 'classify', label: 'Classify this folder…', icon: FolderCog },
  ];
}

/**
 * Turn a menu id into a write. Returns `true` when the id was one of ours.
 *
 * The toast quotes the file the backend says it wrote, because this is the one
 * action in Picus that puts something into the user's own repository — and a
 * tool that does that says where.
 */
export async function runFolderClassifyId(entry: FolderEntry, id: string): Promise<boolean> {
  const [kind, value] = id.split(':');
  if (kind === 'dialect') {
    const next = value === CLEAR_ID ? null : (value as FolderEngine);
    await classifyFolder(entry, { dialect: next });
    return true;
  }
  if (kind === 'role') {
    const next = value === CLEAR_ID ? null : (value as FolderRole);
    await classifyFolder(entry, { role: next });
    return true;
  }
  return false;
}

/** What the write says once it landed — the same sentence from every entry point. */
function describe(entry: FolderEntry, change: { dialect?: FolderEngine | null; role?: FolderRole | null }): string {
  const parts: string[] = [];
  if ('dialect' in change) {
    parts.push(change.dialect ? engineLabel(change.dialect) : 'inherited engine');
  }
  if ('role' in change) {
    parts.push(change.role ? FOLDER_ROLE_LABELS[change.role] : 'inherited role');
  }
  return `${entry.node.name} → ${parts.join(' · ')}`;
}

/**
 * Declare (or clear) a folder's engine and/or role, and say so.
 *
 * Returns `true` on success. Every caller wants the same feedback, so the toast
 * is here rather than repeated three times with three wordings.
 *
 * On success it also raises the "…and every folder named X" question — see
 * {@link offerAliasFor}. Raising it here rather than at the three call sites is
 * the same reasoning as the toast: the offer must be the same offer whether the
 * classification came from a row, a dialog or the palette.
 */
export async function classifyFolder(
  entry: FolderEntry,
  change: { dialect?: FolderEngine | null; role?: FolderRole | null },
): Promise<boolean> {
  const message = await picusProjectStore.classify(entry.node.path, change);
  if (message) {
    toastStore.show(`${entry.node.path} could not be classified — ${message}`, 'error');
    return false;
  }
  // Classifying from the dialog or the palette leaves the tree wherever it was.
  // Opening the path makes the confirmation checkable rather than a claim.
  picusProjectStore.revealFolder(entry.node.path);
  const where = picusProjectStore.configPath;
  toastStore.show(
    `${describe(entry, change)}${where ? `. Saved in ${where}` : ''}`,
    'success',
  );
  void offerAliasFor(entry, change);
  return true;
}

/**
 * Ask whether what was just said about one folder should hold for its **name**.
 *
 * Deliberately fire-and-forget and deliberately after the fact: the
 * classification has already landed, so the offer can be declined, ignored, or
 * fail to load its count without costing the user the decision they made. It is
 * a second question, and a second question is allowed to be optional.
 *
 * Not raised when:
 *  • the change cleared a declaration rather than making one — "inherit" is not a
 *    meaning a name can carry;
 *  • the name reaches only the folder just classified — there is no work saved,
 *    and an offer that saves nothing is an interruption;
 *  • the project already says exactly this about the name **and already looks for
 *    it in folder names** — widening an existing alias to files is still worth
 *    asking about, narrowing it is not;
 *  • the user declined this name earlier in the session.
 */
export async function offerAliasFor(
  entry: FolderEntry,
  change: { dialect?: FolderEngine | null; role?: FolderRole | null },
): Promise<void> {
  const engine = change.dialect ?? null;
  const role = change.role ?? null;
  if (!engine && !role) return;

  const name = entry.node.name;
  if (!name) return;

  const existing = picusProjectStore.aliasFor(name);
  if (
    existing
    && (existing.engine ?? null) === engine
    && (existing.role ?? null) === role
    && scopeCoversFolders(aliasScope(existing))
  ) return;

  const paths = await picusProjectStore.foldersNamed(name);
  // One folder is not a rule, it is the thing that was just done.
  if (paths.length < 2) return;

  picusUiStore.offerAlias({
    kind: 'folder',
    name,
    engine,
    role,
    folderPaths: paths,
    origin: entry.node.path,
  });
}
