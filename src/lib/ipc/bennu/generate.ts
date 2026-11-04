/**
 * Bennu code generators IPC — Spring Security, converters, HTTP clients, builders, Tauri/Bevy skeletons.
 *
 * Wire shapes mirror `crates/products/bennu/runtime/be/src/code_generate.rs`. Listing, opening and
 * previewing write nothing; new files are written with `createFileFromTemplate` (which refuses an existing
 * one) and the edits to the open file are applied by the editor as one undo step.
 */

import { bennu } from '../rpc';

export type GeneratorTarget = 'class' | 'java' | 'rust' | 'folder' | 'endpoint' | 'project';

export interface GeneratorMeta {
  id: string;
  title: string;
  description: string;
  /** A lucide icon name. */
  icon: string;
  group: string;
  language: 'java' | 'rust';
  targets: GeneratorTarget[];
  related: string[];
}

export interface GeneratorListing extends GeneratorMeta {
  /** The project has a dependency this generator is about. */
  suggested: boolean;
}

/** Where the user is: the project, the file under the caret and the caret in it (bytes). */
export interface GenSite {
  root: string;
  file?: string | null;
  /** The buffer, possibly unsaved. */
  source?: string | null;
  offset?: number | null;
  /** A folder of the project tree the generator was opened on: new classes go into its package. */
  directory?: string | null;
}

export interface FieldInfo {
  name: string;
  type_name: string;
  is_final: boolean;
  id: boolean;
  annotations: string[];
}

export interface ClassInfo {
  name: string;
  package: string;
  kind: 'class' | 'record' | 'interface' | 'enum' | 'annotation';
  annotations: string[];
  fields: FieldInfo[];
  constants: string[];
  methods: string[];
}

export interface EndpointParam {
  name: string;
  type_text: string;
  binding: string;
  required: boolean;
}

export interface EndpointInfo {
  id: string;
  methods: string[];
  path: string;
  controller: string;
  handler: string;
  return_type: string;
  produces: string;
  params: EndpointParam[];
}

export interface OpenedGenerator {
  meta: GeneratorMeta;
  /** The options the dialog opens with — typed by each generator's form. */
  defaults: Record<string, unknown>;
  /** What was detected, as short chips: `Spring Boot 3.2.1`, `jakarta`, `Lombok`. */
  detected: string[];
  class: ClassInfo | null;
  package: string;
  endpoints: EndpointInfo[];
}

export interface PlannedFile {
  path: string;
  name: string;
  text: string;
  language: string;
  /** Already on disk: it cannot be written. */
  exists: boolean;
}

export interface TextEdit {
  start: number;
  end: number;
  replacement: string;
}

export interface GeneratedExtra {
  label: string;
  language: string;
  text: string;
}

export interface GeneratorPreview {
  files: PlannedFile[];
  /** What lands in the open file: members, annotations, imports. */
  edits: TextEdit[];
  members: { text: string; file: string | null; class: string | null } | null;
  /** One line per annotation added to existing code. */
  wiring: string[];
  extras: GeneratedExtra[];
  notes: string[];
  /** `group:artifact` the output needs and the project lacks. */
  dependencies: string[];
}

/** Every generator, for the command palette and the menus. Wire: `bennu_generate_list`. */
export function listGenerators(root: string | null): Promise<GeneratorListing[]> {
  return bennu('bennu_generate_list', { args: { root } });
}

/** A generator's defaults for the caret, and what the project was detected to be. Wire: `bennu_generate_open`. */
export function openGenerator(id: string, site: GenSite): Promise<OpenedGenerator> {
  return bennu('bennu_generate_open', { args: { id, ...site } });
}

/** Run a generator with `options`; nothing is written. Wire: `bennu_generate_preview`. */
export function previewGenerator(id: string, site: GenSite, options: unknown): Promise<GeneratorPreview> {
  return bennu('bennu_generate_preview', { args: { id, ...site, options } });
}
