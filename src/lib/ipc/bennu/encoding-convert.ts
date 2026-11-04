/**
 * Bennu encoding-conversion IPC — rewrite files from one encoding to another.
 *
 * Its own file so concurrent edits to the main bennu IPC surface don't race. Round-trips through
 * the generic `bennu(...)` rpc bridge to `bennu_encoding_convert_{plan,apply,undo}`.
 *
 * Two different verbs share this vocabulary and must not be confused: *reinterpret* changes how a
 * file is READ (a config override, no write — see `BennuReinterpretModal`), *convert* rewrites its
 * bytes (this file).
 */

import { bennu } from '../rpc';

export type BomPolicy = 'keep' | 'add' | 'remove';
export type Fallback = 'abort' | 'replace' | 'escape';

/** What a conversion is asked to do — the preview and the real run take the same request. */
export interface ConvertRequest {
  /** Absolute path of the project root. */
  root: string;
  /** Files and/or directories, absolute and inside the project. The root itself = whole project. */
  targets: string[];
  /** The encoding the files are in now; omit to read each file as Bennu does. */
  from?: string;
  /** The encoding to write. */
  to: string;
  bom: BomPolicy;
  fallback: Fallback;
  /** Rewrite the encoding a file declares about itself (XML prolog, JSP directive). */
  rewrite_declarations: boolean;
}

export type FileStatus =
  | 'convert' | 'unchanged' | 'binary' | 'invalid' | 'blocked' | 'unreadable' | 'unknown-encoding';

/** One character the target cannot hold — mirrors the BE `Sample`. */
export interface Unrepresentable {
  line: number;
  column: number;
  ch: string;
  /** `U+20AC`. */
  code: string;
}

/** What would happen to one file — mirrors the BE `FilePlan`. */
export interface FilePlan {
  file: string;
  rel: string;
  status: FileStatus;
  from: string;
  detected: string;
  size_before: number;
  size_after: number;
  bom_before: boolean;
  bom_after: boolean;
  unrepresentable_total: number;
  samples: Unrepresentable[];
  declaration: string | null;
  declaration_rewritten: boolean;
  message: string | null;
}

/** The dialog's "declare it in the pom" facts — mirrors the BE `ProjectFacts`. */
export interface ProjectFacts {
  declared: string;
  can_declare: boolean;
  whole_project: boolean;
  to: string;
}

/** The dry run — mirrors the BE `ConvertPlan`. `files` lists everything that is not `unchanged`. */
export interface ConvertPlan {
  files: FilePlan[];
  truncated: boolean;
  total_files: number;
  to_convert: number;
  unchanged: number;
  skipped: number;
  blocked: number;
  unrepresentable_total: number;
  project: ProjectFacts;
}

export interface ConvertFailure {
  file: string;
  error: string;
}

/** What a conversion did — mirrors the BE `ApplyResult`. */
export interface ApplyResult {
  run_id: string;
  converted: number;
  unchanged: number;
  skipped: number;
  failed: ConvertFailure[];
  cancelled: boolean;
  converted_files: string[];
  poms_declared: number;
  declare_note: string | null;
  backup_dir: string;
}

/** What an undo did — mirrors the BE `UndoResult`. */
export interface UndoResult {
  run_id: string;
  restored: string[];
  /** Files changed after the conversion, left as they are. */
  skipped: string[];
}

/** Dry-run a conversion: nothing is written. Wire: `bennu_encoding_convert_plan`. */
export function encodingConvertPlan(request: ConvertRequest): Promise<ConvertPlan> {
  return bennu('bennu_encoding_convert_plan', { args: request });
}

/** Run a conversion (a Job). Backs every file up first. Wire: `bennu_encoding_convert_apply`. */
export function encodingConvertApply(request: ConvertRequest, declareProject: boolean): Promise<ApplyResult> {
  return bennu('bennu_encoding_convert_apply', { args: { ...request, declare_project: declareProject } });
}

/** Undo the latest conversion (or `runId`) in one step. Wire: `bennu_encoding_convert_undo`. */
export function encodingConvertUndo(root: string, runId?: string): Promise<UndoResult> {
  return bennu('bennu_encoding_convert_undo', { args: { root, run_id: runId ?? null } });
}
