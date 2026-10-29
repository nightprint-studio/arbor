/**
 * New Project wizard IPC — the generator catalog, each generator's form, a dry-run preview, and
 * the creation run.
 *
 * Creation is fire-and-subscribe: {@link createProject} returns at once and the run narrates itself
 * through `arbor://bennu/new-project` events keyed by the caller's run id. Subscribe with
 * {@link onProjectRun} **before** calling it, so no event can be missed.
 *
 * Same convention as the rest of the bennu IPC: one `args` object, snake_case fields.
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { bennu } from '../rpc';

export type NameRule = 'crate' | 'artifact' | 'npm';

export interface NewProjectCategory {
  id: string;
  label: string;
}

export interface NewProjectGenerator {
  id: string;
  category: string;
  label: string;
  description: string;
  /** A key the UI maps to an icon. */
  icon: string;
  name_rule: NameRule;
  requires: string[];
  /** The options come from a remote service, so the form loads asynchronously. */
  remote: boolean;
  available: boolean;
  /** Why it isn't available. */
  reason?: string;
}

export interface ToolStatus {
  name: string;
  label: string;
  found: boolean;
  path: string | null;
}

export interface NewProjectCatalog {
  categories: NewProjectCategory[];
  generators: NewProjectGenerator[];
  tools: ToolStatus[];
  /** The user's home folder, for a first-run default location. */
  home: string;
}

export interface FieldOption {
  value: string;
  label: string;
  description?: string;
  group?: string;
}

export type FieldKind = 'text' | 'select' | 'toggle' | 'multi';

export type FieldValue = string | boolean | string[];

export interface GeneratorField {
  key: string;
  label: string;
  kind: FieldKind;
  default: FieldValue;
  options: FieldOption[];
  hint?: string;
  placeholder?: string;
  advanced: boolean;
  /** Shown only while another field has this value (`"true"` / `"false"` for toggles). */
  show_when?: { key: string; value: string };
}

export interface FieldsResult {
  fields: GeneratorField[];
  /** Built from an old copy because the service couldn't be reached. */
  stale: boolean;
}

export interface ProjectRequest {
  generator: string;
  name: string;
  location: string;
  git: boolean;
  options: Record<string, FieldValue>;
}

export interface Problem {
  field: string;
  message: string;
}

export type TargetState =
  | 'free' | 'free_new_parents' | 'empty_dir' | 'non_empty_dir' | 'is_file' | 'bad_location' | 'incomplete';

export interface TargetCheck {
  path: string;
  state: TargetState;
  message: string | null;
}

export interface ProjectPreview {
  problems: Problem[];
  target: TargetCheck;
  /** Project-relative paths of a locally generated project. */
  files: string[];
  summary: string[];
  /** The command (or download) a remote generator will run, for display. */
  command: string | null;
  ok: boolean;
}

/** What the wizard remembers — mirrors the BE `NewProjectConfig`. */
export interface NewProjectPrefs {
  last_location: string;
  last_generator: string;
  git_init: boolean | null;
  package_manager: string;
  last_group: string;
}

export type ProjectRunEvent =
  | { id: string; kind: 'stage'; text: string }
  | { id: string; kind: 'log'; text: string }
  | { id: string; kind: 'done'; path: string; open: string | null; warnings: string[] }
  | { id: string; kind: 'error'; message: string }
  | { id: string; kind: 'cancelled' };

/** The generators, the installed tools, and which generators that leaves usable. Wire: `bennu_np_catalog`. */
export function newProjectCatalog(): Promise<NewProjectCatalog> {
  return bennu('bennu_np_catalog', {});
}

/** A generator's form. Async for Spring / Quarkus / Bevy, whose options come from the network.
 *  Wire: `bennu_np_fields`. */
export function newProjectFields(generator: string, refresh = false): Promise<FieldsResult> {
  return bennu('bennu_np_fields', { args: { generator, refresh } });
}

/** What a request would do, without doing it. Wire: `bennu_np_preview`. */
export function previewProject(request: ProjectRequest): Promise<ProjectPreview> {
  return bennu('bennu_np_preview', { args: request });
}

/** Start creating. Returns at once; follow the run with {@link onProjectRun}. Wire: `bennu_np_create`. */
export function createProject(runId: string, request: ProjectRequest): Promise<{ path: string }> {
  return bennu('bennu_np_create', { args: { run_id: runId, request } });
}

/** Cancel a run: kills its process tree and removes what it made. Wire: `bennu_np_cancel`. */
export function cancelProject(runId: string): Promise<boolean> {
  return bennu('bennu_np_cancel', { args: { run_id: runId } });
}

export function getNewProjectPrefs(): Promise<NewProjectPrefs> {
  return bennu('get_bennu_new_project', {});
}

export function setNewProjectPrefs(config: NewProjectPrefs): Promise<void> {
  return bennu('set_bennu_new_project', { config });
}

/** Subscribe to the progress of one run. Resolves once the listener is attached. */
export async function onProjectRun(runId: string, handler: (e: ProjectRunEvent) => void): Promise<UnlistenFn> {
  return listen<ProjectRunEvent>('arbor://bennu/new-project', (e) => {
    if (e.payload.id === runId) handler(e.payload);
  });
}
