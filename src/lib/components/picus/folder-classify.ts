/**
 * Saying what a folder is — the vocabulary, the menu and the write, in one place.
 *
 * Classifying a folder is reachable from three places (the tree row's menu, the
 * tree row's keyboard, the command palette's dialog) and it must mean the same
 * thing in all three: the same options, the same wording, the same confirmation,
 * the same "clear it back to inherited". So the options and the call live here
 * and the three call sites only decide how they are presented.
 *
 * `null` is a first-class choice throughout, not an absence: **clearing** a wrong
 * guess is a thing users need to do, and it is a different act from never having
 * said anything. `ipc/picus/project.ts` documents the three-valued wire.
 *
 * ## Four engine answers
 *
 * Oracle and PostgreSQL are what Picus reads. **Portable** is the third: plain
 * SQL meant to run on both, which is a promise only a person can make — nothing
 * infers it — and which pays for itself immediately, because one portable file
 * satisfies both engines where two used to be needed. SQL Server, DB2, MySQL,
 * MariaDB and SQLite are engines Picus can only **name** — offered because "these
 * are SQL Server scripts" is a true and useful thing to say, and because saying
 * it is what stops the folder generating a question on every scan forever. And
 * "Inherit from above" is the last: not knowing, on purpose.
 *
 * ## And the offer that turns one decision into eleven
 *
 * Classifying `AGGIORNAMENTO/2024/POS` says something about one folder. In a
 * repository with a folder set per delivered version there are ten more called
 * `POS` and there will be another next month. {@link offerAliasFor} raises the
 * second question at the moment the user has the answer — as a **separate**
 * decision, never as a side effect of the first.
 */

import { Ban, Blend, Database, Eraser, FileCog, FolderCog, Layers } from 'lucide-svelte';
import type { MenuItem } from '$lib/components/shared/ContextMenu.svelte';
import { toastStore } from '$lib/feedback/stores/toasts.svelte';
import { picusProjectStore, type FolderEntry } from '$lib/stores/picus/project.svelte';
import { picusUiStore } from '$lib/stores/picus/ui.svelte';
import {
  DIALECTS,
  FOLDER_ROLE_LABELS,
  FOREIGN_ENGINES,
  FOREIGN_ENGINE_CHOICES,
  GENERIC_ENGINE,
  declaredEngine,
  engineLabel,
  folderEngine,
  isDialect,
  isGenericEngine,
  type Dialect,
  type ForeignEngine,
  type FolderEngine,
  type FolderRole,
} from '$lib/types/picus';

/** Engines a folder can be declared as and Picus can read, in display order. */
export const DIALECT_CHOICES: Dialect[] = ['oracle', 'postgres'];

/**
 * Roles a folder can be declared as, in the order they run.
 *
 * `ignored` is last and is a genuine choice: a folder of one-off fixes that must
 * never be indexed or written into is exactly what it is for.
 */
export const ROLE_CHOICES: FolderRole[] = ['init', 'update', 'routines', 'data', 'ignored'];

/**
 * Every engine offered, readable ones first.
 *
 * The unsupported half is deliberately part of the same list rather than tucked
 * behind a second control: a folder has one engine, so choosing it is one act.
 */
export const ENGINE_CHOICES: FolderEngine[] = [
  ...DIALECT_CHOICES,
  GENERIC_ENGINE,
  ...FOREIGN_ENGINE_CHOICES,
];

/** Menu id → what it means. Kept as strings because `ContextMenu` speaks ids. */
export const CLEAR_ID = 'inherit';

export function engineMenuId(e: FolderEngine): string { return `dialect:${e}`; }
export function roleMenuId(r: FolderRole): string { return `role:${r}`; }

/** How an engine reads in a picker: unsupported ones say so, every time. */
export function engineChoiceLabel(engine: FolderEngine): string {
  if (isDialect(engine)) return DIALECTS[engine].short;
  if (isGenericEngine(engine)) return 'Portable — runs on both engines';
  return `${FOREIGN_ENGINES[engine as ForeignEngine]} — not supported`;
}

/**
 * The two submenus a folder row offers, with the current answer marked.
 *
 * "set here" and "inherited" are on the badge rather than the label so the list
 * still reads as a list of choices — the mark says where the folder stands, it
 * does not rename the option.
 */
export function folderClassifyItems(entry: FolderEntry): MenuItem[] {
  const { node } = entry;
  const declared = declaredEngine(node);
  const effective = folderEngine(node);
  const declaredRole = node.role;

  const engineItem = (e: FolderEngine): MenuItem => ({
    id: engineMenuId(e),
    label: isDialect(e)
      ? DIALECTS[e].short
      : isGenericEngine(e)
        ? 'Portable · both engines'
        : FOREIGN_ENGINES[e as ForeignEngine],
    icon: isDialect(e) ? Database : isGenericEngine(e) ? Blend : Ban,
    iconColor: isDialect(e)
      ? `var(${DIALECTS[e].colorVar})`
      : isGenericEngine(e)
        ? 'var(--accent)'
        : 'var(--text-muted)',
    badge:
      declared === e ? 'set here'
        : effective === e && declared === null ? 'inherited'
          : undefined,
    badgeAccent: declared === e,
  });

  const dialectItems: MenuItem[] = [
    ...DIALECT_CHOICES.map(engineItem),
    // Portable sits beside the dialects, not below the fold: it is a first-class
    // answer and the one that turns two files into one.
    engineItem(GENERIC_ENGINE),
    // The engines Picus cannot read are a group of their own: picking one is
    // saying "this is not mine", which is a different kind of answer from
    // picking Oracle, and a flat list would blur that.
    { id: 'engine-foreign-sep', label: '', separator: true },
    { id: 'engine-foreign-head', label: 'Recognised, not supported', header: true },
    ...FOREIGN_ENGINE_CHOICES.map(engineItem),
    { id: 'dialect-sep', label: '', separator: true },
    {
      id: `dialect:${CLEAR_ID}`,
      label: 'Inherit from above',
      icon: Eraser,
      // Nothing to clear when this folder never declared one — the option would
      // do nothing, and an option that does nothing is worse than an absent one.
      disabled: declared === null,
      subtitle: entry.dialectFrom && entry.dialectFrom !== node.path ? entry.dialectFrom : undefined,
    },
  ];

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
 *  • the project already says exactly this about the name;
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
  if (existing && (existing.engine ?? null) === engine && (existing.role ?? null) === role) return;

  const paths = await picusProjectStore.foldersNamed(name);
  // One folder is not a rule, it is the thing that was just done.
  if (paths.length < 2) return;

  picusUiStore.offerFolderAlias({ name, engine, role, paths, origin: entry.node.path });
}

/**
 * What the offer would do, in the user's words — the confirmation's body.
 *
 * Exported so the shell renders it and this file decides what it says: the
 * wording of "this reaches eleven folders and every one added later" is the part
 * that makes the offer safe to accept, and it belongs next to the rule.
 */
export function aliasOfferDetail(
  paths: string[],
  origin: string,
  configPath: string,
): string {
  const others = paths.filter((p) => p !== origin);
  const shown = others.slice(0, 8);
  const lines = [
    shown.join('\n') + (others.length > shown.length ? `\n…and ${others.length - shown.length} more` : ''),
    'Any folder of this name added later is classified the same way, without touching the configuration again.',
    'A folder that declares its own engine keeps it — a specific answer still beats the rule.',
  ];
  if (configPath) lines.push(`Saved with the repository, in ${configPath}.`);
  return lines.join('\n\n');
}
