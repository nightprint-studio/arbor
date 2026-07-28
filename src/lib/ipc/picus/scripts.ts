/**
 * Picus script-repository IPC — the half of the product that reads, checks and
 * rewrites the SQL files on disk.
 *
 * Everything here goes through the generic `picus(...)` rpc bridge to `picus-be`,
 * exactly like `db.ts`. The payloads are the shapes `$lib/types/picus` already
 * describes (`Project`, `Finding`, `InventoryObject`) — the Rust side serialises
 * camelCase precisely so there is no translation layer.
 *
 * ## Reading is two calls, not one
 *
 * `picus_open_scripts` answers "what is in this folder" — the tree, plus the
 * questions the reader could not settle on its own. `picus_analyze_scripts`
 * answers "and is it coherent" — the inventory and the findings. They are apart
 * because the first is fast enough to block a panel on and the second is not:
 * the tree appears, the verdict arrives.
 *
 * ## Writing is two calls, and that is the point
 *
 * `picus_preview_apply` returns **the exact bytes that would land**, each file
 * with a digest of what it looked like when they were computed.
 * `picus_apply` sends those digests back, and refuses — naming the file — if
 * anything moved underneath in the meantime. A write that cannot prove it is
 * writing over what the user reviewed is a write that must not happen.
 */

import type {
  Finding,
  FolderAlias,
  InventoryObject,
  LineEnding,
  ObjectKind,
  Project,
  RuleId,
  Target,
} from '$lib/types/picus';
import type { DmlModel } from './db';
import { picus } from '../rpc';

// ── Notices ──────────────────────────────────────────────────────────────────

/**
 * One thing the reader wants to tell the user about a file or a folder.
 *
 * `needsAttention` is the difference between "worth knowing" (this folder was
 * classified as data because of its name) and "answer me" (this folder's engine
 * could not be decided). The second kind is a question, and the panel treats it
 * as one.
 */
export interface ProjectNote {
  path: string;
  message: string;
  needsAttention: boolean;
}

/**
 * The wire form of a notice, read tolerantly.
 *
 * `notes` is pinned by the contract; `problems`, `orphans` and
 * `rejectedSuppressions` are lists whose element shape is not, so they are read
 * through {@link toNote} rather than assumed. A backend that sends bare strings
 * and one that sends objects both render.
 */
export type RawNotice =
  | string
  | {
      path?: string;
      file?: string;
      message?: string;
      reason?: string;
      rule?: string;
      needsAttention?: boolean;
    };

/** Normalise a wire notice to the one shape the panels render. */
export function toNote(raw: RawNotice, needsAttention = true): ProjectNote {
  if (typeof raw === 'string') return { path: '', message: raw, needsAttention };
  const path = raw.path ?? raw.file ?? '';
  const message = raw.message ?? raw.reason ?? '';
  const prefix = raw.rule ? `${raw.rule}: ` : '';
  return {
    path,
    message: `${prefix}${message || path || 'Unspecified'}`,
    needsAttention: raw.needsAttention ?? needsAttention,
  };
}

/** Normalise a list of wire notices, dropping nothing. */
export function toNotes(raw: RawNotice[] | null | undefined, needsAttention = true): ProjectNote[] {
  return (raw ?? []).map((n) => toNote(n, needsAttention));
}

// ── Reading the repository ───────────────────────────────────────────────────

export interface OpenScriptsResult {
  project: Project;
  /** What the reader inferred and wants stated — folder roles, encodings, layout. */
  notes: ProjectNote[];
  /**
   * The repository has no `.arbor/picus/project.toml` yet: everything above was
   * inferred from the tree and is **not** saved with the repository.
   */
  isNew: boolean;
  /** What the reader could not settle. A question to the user, never a footnote. */
  problems: RawNotice[];
  /**
   * The folder names this repository has declared a meaning for.
   *
   * Sent with the tree because it *explains* the tree: a `POS` folder reading as
   * PostgreSQL when nothing about `POS` says PostgreSQL is a mystery until the
   * vocabulary is on screen next to it.
   */
  aliases: FolderAlias[];
}

/** Read a script repository. Cheap enough to await before drawing the tree. */
export function openScripts(root: string): Promise<OpenScriptsResult> {
  return picus('picus_open_scripts', { root });
}

/** Same, forcing a re-read from disk — the explicit "I changed files outside". */
export function refreshScripts(root: string): Promise<OpenScriptsResult> {
  return picus('picus_refresh_scripts', { root });
}

// ── Analysing it ─────────────────────────────────────────────────────────────

/**
 * A rule that could not run, and why.
 *
 * A rule that did not run is not a rule that passed — `VER003` skipped because
 * the naming pattern yields no version bounds means the version chain is
 * *unchecked*, not sound. Reporting the silence is the whole difference between
 * a clean report and an empty one.
 */
export interface SkippedRule {
  rule: RuleId;
  /** Where the rule stood down — a folder, a file, or the project. */
  scope: string;
  reason: string;
}

/**
 * A suppression comment the analysis refused.
 *
 * Someone believes that line is silencing something and it is not — because it
 * names no rule, is malformed, or names a rule that never fired where it sits.
 * The comment is quoted back (`text`) so the fix is obvious without opening the
 * file, though the location makes it one click away anyway.
 */
export interface RejectedSuppression {
  /** Project-relative path. */
  file: string;
  /** 1-based line of the comment. */
  line: number;
  /** The comment as written. */
  text: string;
  /** What is wrong with it, in the words of whoever has to fix it. */
  problem: string;
}

export interface AnalyzeScriptsResult {
  inventory: InventoryObject[];
  findings: Finding[];
  skipped: SkippedRule[];
  /** Suppression comments that named nothing, or named a rule that never fired. */
  rejectedSuppressions: RejectedSuppression[];
  /** Objects and files no classified folder claims — indexed, outside the model. */
  orphans: RawNotice[];
}

/** A refused suppression as the notice list renders it. */
export function suppressionNote(s: RejectedSuppression): ProjectNote {
  return {
    path: `${s.file}:${s.line}`,
    // The comment itself, then what is wrong with it: the user recognises the line
    // they wrote before they read the diagnosis.
    message: `${s.problem}${s.text ? ` — ${s.text.trim()}` : ''}`,
    needsAttention: true,
  };
}

/** Run the rules over the repository. Slow by nature — never awaited on a paint. */
export function analyzeScripts(root: string): Promise<AnalyzeScriptsResult> {
  return picus('picus_analyze_scripts', { root });
}

// ── Where one object is named ────────────────────────────────────────────────

/** One place an object appears — a row of the drill-down behind a coverage cell. */
export interface ObjectUsage {
  /** Project-relative path. */
  path: string;
  /** The folder whose coverage column this counts under. */
  folder: string;
  line: number;
  /** The statement creates or redefines the object, rather than merely using it. */
  defining: boolean;
  /** …and it is a CREATE, not an ALTER. */
  creating: boolean;
  /** `select`, `insert`, `create`, … — what the statement holding it does. */
  statement: string;
}

/**
 * Every place one object is named, optionally restricted to one folder.
 *
 * A separate call rather than a field on the inventory, because this has a row
 * per *mention* where the inventory has one per object — one or two orders of
 * magnitude more in a real repository. Asked when a cell is clicked, and not
 * before: it answers the question the matrix raises and could not settle, which
 * is that the cell says three and the only useful next thought is *which three*.
 */
export function objectUsages(
  root: string,
  kind: ObjectKind,
  name: string,
  folder?: string,
): Promise<ObjectUsage[]> {
  return picus('picus_object_usages', { root, kind, name, folder });
}

// ── One file's text ──────────────────────────────────────────────────────────

/**
 * A file as the backend decoded it. The encoding and line ending come back with
 * the text because they are what a later write has to preserve — reading them
 * off the tree entry instead would let the two drift apart.
 */
export interface ScriptText {
  text: string;
  encoding: string;
  eol: LineEnding;
}

export function scriptText(root: string, path: string): Promise<ScriptText> {
  return picus('picus_script_text', { root, path });
}

// ── Writing ──────────────────────────────────────────────────────────────────

/** One destination file, before and after, exactly as the write would leave it. */
export interface PreviewFile {
  /** Project-relative path, POSIX separators. */
  path: string;
  /** The file as it is on disk right now. Empty when `createsFile`. */
  before: string;
  /** The file as the write would leave it. */
  after: string;
  encoding: string;
  eol: LineEnding;
  /** Why the block lands where it lands — the insertion rule, in words. */
  reasons: string[];
  /** The file does not exist yet and the write would create it. */
  createsFile: boolean;
  /**
   * Digest of `before`. Handed back to {@link applyScripts}, which refuses when
   * it no longer matches what is on disk.
   */
  digest: string;
}

export interface PreviewApplyResult {
  files: PreviewFile[];
}

/**
 * Compute the write without performing it. Reads disk; never touches it.
 *
 * ⚠ `targets` must already be **only the ones the user armed**. Like
 * `picus_emit`, both write handlers prepare every target they are handed and do
 * not consult `enabled` — passing the whole list would write into destinations
 * the user deliberately unchecked. The store's `enabledTargets` is what goes
 * across, never `targets`.
 */
export function previewApply(
  root: string,
  model: DmlModel,
  targets: Target[],
): Promise<PreviewApplyResult> {
  return picus('picus_preview_apply', { root, model, targets });
}

/** What the write did, as project-relative paths rather than counts. */
export interface ApplyResult {
  written: string[];
  created: string[];
  unchanged: string[];
}

/**
 * Perform the write, proving it is the one that was reviewed.
 *
 * `digests` maps each previewed path to the digest that preview reported. If a
 * file changed since, the call **fails naming that file** — and that message is
 * the useful part, so it must reach the user as it arrived rather than being
 * flattened into "the write failed". The same holds for two destinations naming
 * one file: the backend refuses by name, because only the user can say which of
 * the two was meant.
 */
export function applyScripts(
  root: string,
  model: DmlModel,
  targets: Target[],
  digests: Record<string, string>,
): Promise<ApplyResult> {
  return picus('picus_apply', { root, model, targets, digests });
}
