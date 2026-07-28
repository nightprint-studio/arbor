/**
 * Picus project IPC — agreeing with the backend on what a repository *is*.
 *
 * Reading a repository (`ipc/picus/scripts.ts`) answers "what is in this folder".
 * This file answers the other half: "and which of it is Oracle, which is an
 * update script, and who says so". Discovery proposes; **nothing is written into
 * someone's repository until this call is made**, and when it is, the backend
 * says where it wrote (`configPath`).
 *
 * ## Why an edit is not a folder
 *
 * A correction travels as `{ path, dialect?, role? }` rather than as a whole
 * folder, because the three states of each field carry the whole meaning:
 *
 *  | wire            | means                                   |
 *  |-----------------|-----------------------------------------|
 *  | field absent    | leave it exactly as it is               |
 *  | field `null`    | clear the declaration — inherit again    |
 *  | field set       | declare it here                          |
 *
 * `undefined` and `null` are therefore **not interchangeable** here, and
 * `JSON.stringify` drops the former while keeping the latter — which is exactly
 * the distinction the backend deserialises. Build edits with {@link folderEdit}
 * rather than by hand, so an accidental `undefined` never becomes a silent no-op.
 */

import type {
  Dialect,
  FolderAlias,
  FolderEngine,
  FolderRole,
  ForeignEngine,
  Project,
} from '$lib/types/picus';
import { picus } from '../rpc';

/**
 * One correction to one folder, keyed by its project-relative path.
 *
 * Keyed by path rather than by an id because a path is what the user sees and
 * what survives a rescan.
 */
export interface ProjectEdit {
  path: string;
  /**
   * Absent = leave alone · `null` = back to inherited · set = declare here.
   *
   * May name an engine Picus does not read (`'sqlserver'`): a folder has one
   * engine, so it travels in one key.
   */
  dialect?: FolderEngine | null;
  /** Absent = leave alone · `null` = back to inherited · set = declare here. */
  role?: FolderRole | null;
}

/** What the user asked to change about a folder. Absent keys are not sent. */
export interface FolderClassification {
  dialect?: FolderEngine | null;
  role?: FolderRole | null;
}

/**
 * Build an edit that carries exactly the fields that were asked for.
 *
 * Written out rather than spread from the caller's object so an `undefined`
 * value can never travel as a present-but-empty key: `{ dialect: undefined }`
 * survives object spread and would read as "leave alone" only by accident.
 */
export function folderEdit(path: string, change: FolderClassification): ProjectEdit {
  const edit: ProjectEdit = { path };
  if ('dialect' in change) edit.dialect = change.dialect ?? null;
  if ('role' in change) edit.role = change.role ?? null;
  return edit;
}

export interface ConfirmedProject {
  /** Absolute path of the file that was written — a tool writing into your
   *  repository should say where. */
  configPath: string;
  /** The tree as it stands after the corrections, ready to replace the old one. */
  project: Project;
  /** The project's folder-name vocabulary as it stands after them. */
  aliases: FolderAlias[];
  /** What is wrong with the configuration now — reported, never fatal. */
  problems: string[];
}

/**
 * Apply corrections and write `.arbor/picus/project.toml`.
 *
 * The backend re-reads the folder rather than trusting a client-held snapshot,
 * and answers with the tree as it will look from now on — so the caller replaces
 * its project with this reply instead of patching its own copy.
 */
export function confirmProject(root: string, edits: ProjectEdit[]): Promise<ConfirmedProject> {
  return picus('picus_confirm_project', { root, edits });
}

/**
 * Declare — or forget — what a folder **name** means in this repository.
 *
 * The other half of classification, and the half that scales: an edit answers for
 * one path, this answers for every folder called `POS` **including the ones that
 * do not exist yet**. A repository shipping a folder set per delivered version
 * cannot be described any other way without re-describing it every release.
 *
 * Both fields are **replaced**, not merged — an alias has exactly two, so "set it
 * to this" needs none of the three-valued machinery {@link ProjectEdit} needs.
 * Passing neither removes the alias.
 */
export function setFolderAlias(
  root: string,
  name: string,
  engine: FolderEngine | null,
  role: FolderRole | null,
): Promise<ConfirmedProject> {
  return picus('picus_set_folder_alias', { root, name, engine, role });
}

/**
 * Every folder an alias of this name would apply to, in tree order.
 *
 * Asked **before** the alias is offered, so the offer can say "and the other ten
 * folders called POS" with a number that is true. The matching rule is not
 * reimplemented here on purpose: `POS` matching `01_POS` but not `POSIZIONI` is
 * load-bearing, and a second copy of a load-bearing rule is a copy that drifts.
 */
export function foldersNamed(root: string, name: string): Promise<string[]> {
  return picus('picus_folders_named', { root, name });
}
