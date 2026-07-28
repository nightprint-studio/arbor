/**
 * The engine question, asked once — whatever it is being asked *about*.
 *
 * A folder has an engine, a file has an engine, and a name can mean an engine.
 * Three different things to classify, one list of answers: the two dialects
 * Picus reads, **portable**, the engines it can only name, and "inherit". They
 * have to be the same list with the same wording and the same order in all
 * three places, or the product teaches three slightly different vocabularies for
 * one concept — which is how "portable" quietly stops being offered somewhere.
 *
 * So the choices, their labels, their icons and the menu they build live here,
 * and `folder-classify.ts` / `file-classify.ts` only decide what to do with the
 * answer.
 *
 * ## The four answers, and why "inherit" is one of them
 *
 * Oracle and PostgreSQL are what Picus reads. **Portable** is plain SQL meant to
 * run on both — a promise only a person can make, never inferred, and the one
 * that turns two files into one. SQL Server, DB2, MySQL, MariaDB and SQLite are
 * engines Picus can only *name*: saying so is a real answer with real
 * consequences, because it is what stops the folder generating a question on
 * every scan forever. And "inherit" is the last: clearing a wrong guess is
 * something users need to do, and it is a different act from never having said
 * anything.
 */

import { Ban, Blend, Database, Eraser } from 'lucide-svelte';
import type { MenuItem } from '$lib/components/shared/ContextMenu.svelte';
import {
  DIALECTS,
  FOREIGN_ENGINES,
  FOREIGN_ENGINE_CHOICES,
  GENERIC_ENGINE,
  isDialect,
  isGenericEngine,
  type Dialect,
  type FolderEngine,
  type ForeignEngine,
  type FolderRole,
} from '$lib/types/picus';

/** Engines that can be declared and that Picus can read, in display order. */
export const DIALECT_CHOICES: Dialect[] = ['oracle', 'postgres'];

/**
 * Roles a folder can be declared as, in the order they run.
 *
 * `ignored` is last and is a genuine choice: a folder of one-off fixes that must
 * never be indexed or written into is exactly what it is for.
 *
 * Folders only — a role is what a *directory of scripts* is for, and the file
 * beside another in the same directory is for the same thing. Nothing about a
 * file offers this list.
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

/** The same, short enough for a menu row rather than a `<Select>` option. */
export function engineMenuLabel(engine: FolderEngine): string {
  if (isDialect(engine)) return DIALECTS[engine].short;
  if (isGenericEngine(engine)) return 'Portable · both engines';
  return FOREIGN_ENGINES[engine as ForeignEngine];
}

/** The colour an engine carries everywhere — always a theme token. */
export function engineTint(engine: FolderEngine): string {
  if (isDialect(engine)) return `var(${DIALECTS[engine].colorVar})`;
  if (isGenericEngine(engine)) return 'var(--accent)';
  return 'var(--text-muted)';
}

/** Where the thing being classified stands right now. */
export interface EngineStanding {
  /** What it declares itself; `null` = it says nothing and inherits. */
  declared: FolderEngine | null;
  /** What applies after inheritance. */
  effective: FolderEngine | null;
  /** Where an inherited answer came from — named under "Inherit". */
  from?: string;
  /** What "inherit" reads as here: `above` for a folder, `the folder` for a file. */
  inheritLabel?: string;
}

function engineItem(e: FolderEngine, at: EngineStanding): MenuItem {
  return {
    id: engineMenuId(e),
    label: engineMenuLabel(e),
    icon: isDialect(e) ? Database : isGenericEngine(e) ? Blend : Ban,
    iconColor: engineTint(e),
    // "set here" and "inherited" ride on the badge rather than the label so the
    // list still reads as a list of choices — the mark says where the thing
    // stands, it does not rename the option.
    badge:
      at.declared === e ? 'set here'
        : at.effective === e && at.declared === null ? 'inherited'
          : undefined,
    badgeAccent: at.declared === e,
  };
}

/**
 * The engine submenu, with the current answer marked and "inherit" at the end.
 *
 * One builder for folders and files alike: the answers are identical, only the
 * word for where an inherited one came from differs.
 */
export function engineMenuItems(at: EngineStanding): MenuItem[] {
  return [
    ...DIALECT_CHOICES.map((e) => engineItem(e, at)),
    // Portable sits beside the dialects, not below the fold: it is a first-class
    // answer and the one that turns two files into one.
    engineItem(GENERIC_ENGINE, at),
    // The engines Picus cannot read are a group of their own: picking one is
    // saying "this is not mine", which is a different kind of answer from
    // picking Oracle, and a flat list would blur that.
    { id: 'engine-foreign-sep', label: '', separator: true },
    { id: 'engine-foreign-head', label: 'Recognised, not supported', header: true },
    ...FOREIGN_ENGINE_CHOICES.map((e) => engineItem(e, at)),
    { id: 'dialect-sep', label: '', separator: true },
    {
      id: `dialect:${CLEAR_ID}`,
      label: at.inheritLabel ?? 'Inherit from above',
      icon: Eraser,
      // Nothing to clear when it never declared one — the option would do
      // nothing, and an option that does nothing is worse than an absent one.
      disabled: at.declared === null,
      subtitle: at.from || undefined,
    },
  ];
}

/** The engine list as `<Select>` options, with "inherit" last. */
export function engineSelectOptions(inheritLabel = 'Inherit from above') {
  return [
    ...ENGINE_CHOICES.map((e) => ({ value: e as string, label: engineChoiceLabel(e) })),
    { value: CLEAR_ID, label: inheritLabel },
  ];
}

/** Read a picker's value back: `CLEAR_ID` is `null`, everything else is itself. */
export function engineFromChoice(value: string): FolderEngine | null {
  return value === CLEAR_ID ? null : (value as FolderEngine);
}
