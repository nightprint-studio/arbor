/**
 * Bennu (Java editor) IPC — thin `bennu(...)` rpc wrappers over the Model-D bridge.
 *
 * Types only + wrappers — no UI, no state. Every command routes through the generic
 * `rpc` bridge to **`bennu-be`** via the bound {@link bennu} helper: `bennu('<handler>',
 * params)`, where `<handler>` is the exact backend handler name (snake_case = the Rust
 * fn name).
 *
 * ⚠️ Arg convention: the RPC seam keys params by the handler's **parameter name**, not
 * by the struct field. Every `bennu-be` handler takes a single struct parameter named
 * `args`, so each call wraps its fields under `{ args: … }` (the proven tyto/studio
 * convention) — NOT a bare/flat object. The inner field names are the struct's fields
 * in snake_case (forwarded verbatim inside the opaque `params`).
 *
 * TS function names are camelCase; wire method names are the exact snake_case strings.
 */

import { bennu } from '../rpc';
import type {
  ProjectInfo, TreeNode, ReadFileResult, CapabilitySet, CompletionItem, Diagnostic,
} from '$lib/types/bennu';

/** Open a Java project folder: resolve the build model (modules / JDK) + capabilities.
 *  Wire: `bennu_open_project` — `OpenProjectArgs { root }`. */
export function openProject(root: string): Promise<ProjectInfo> {
  return bennu('bennu_open_project', { args: { root } });
}

/** Read the project file tree (directories + files) rooted at `root`. Wire:
 *  `bennu_project_tree` — `ProjectTreeArgs { root, depth? }`. */
export function projectTree(root: string): Promise<TreeNode> {
  return bennu('bennu_project_tree', { args: { root } });
}

/** Read a file's text + the encoding it was decoded from. `root` (the project root)
 *  is needed so the backend can resolve the pom-declared encoding. Wire:
 *  `bennu_read_file` — `ReadFileArgs { root, file }`. */
export function readFile(root: string, file: string): Promise<ReadFileResult> {
  return bennu('bennu_read_file', { args: { root, file } });
}

/** Re-detect the domain capabilities (Spike-D bitset) for the open project. Wire:
 *  `bennu_capabilities` — `CapabilitiesArgs { root }`. */
export function capabilities(root: string): Promise<CapabilitySet> {
  return bennu('bennu_capabilities', { args: { root } });
}

/** Completion candidates at a source offset (UTF-8 byte offset). Wire:
 *  `bennu_completion` — `CompletionArgs { file, offset }`. Returns `[]` until the
 *  language service is ready. */
export function completion(file: string, offset: number): Promise<CompletionItem[]> {
  return bennu('bennu_completion', { args: { file, offset } });
}

/** Diagnostics for a file (Phase 0 backend returns `[]`). Wire: `bennu_diagnostics`
 *  — `DiagnosticsArgs { file }`. */
export function diagnostics(file: string): Promise<Diagnostic[]> {
  return bennu('bennu_diagnostics', { args: { file } });
}
