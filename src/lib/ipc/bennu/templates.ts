/**
 * Bennu code templates IPC — the templates the user owns, by kind, and what one renders to.
 *
 * Wire shapes mirror `crates/products/bennu/be/src/templates.rs`. Rendering writes nothing; the one
 * call that does is `createFileFromTemplate`, and it refuses a file that is already there.
 */

import { bennu } from '../rpc';

/** What a template generates, which decides what it is rendered with and where its output goes. */
export type TemplateKindId = 'new-file' | 'class' | 'config-properties' | 'config-class' | 'validation-tests' | 'live';

export const TEMPLATE_KINDS: readonly TemplateKindId[] = ['new-file', 'class', 'config-properties', 'config-class', 'validation-tests', 'live'];

export interface TemplateInfo {
  name: string;
  /** The extension of what it writes (`java`, `yml`). */
  extension: string;
  origin: 'builtin' | 'global';
  /** Built in only as a starting point: copied from, never generated with. */
  starter: boolean;
  description: string | null;
  /** Where the file is — `null` for a built-in. */
  path: string | null;
  /** Its `bennu.requires`: what a project needs for it to be offered. */
  requires: string[];
  /** An abbreviation's `bennu.abbrev` — the word that expands it when that is not its file name. */
  abbrev: string | null;
  /** The first of them the open project does not meet (`Needs Java 16 or later`), or `null`. */
  unmet: string | null;
}

export interface KindTemplates {
  kind: TemplateKindId;
  title: string;
  description: string;
  /** The extension a new template of this kind gets. */
  extension: string;
  /** Where this kind's templates are kept. */
  directory: string;
  templates: TemplateInfo[];
  /** The one the open project generates with. */
  project: string | null;
  /** The kind only runs on a Java project — it reads a class, the Spring model or Bean Validation. */
  java_only: boolean;
  /** A new template of this kind may be written for any language, so the dialog asks which. */
  picks_language: boolean;
}

export interface TemplateInsertion {
  /** Byte offset in the class's file. */
  offset: number;
  /** The members, re-indented to the class. */
  text: string;
}

/** One tab stop of an insertion, as a byte range into its text. Stops sharing a `group` are one
 *  value in several places — typing in one writes the others. */
export interface TemplateStop {
  start: number;
  end: number;
  group: number;
}

/** A byte range of a file and its replacement — an import, beside members inserted into a class. */
export interface TemplateEdit {
  start: number;
  end: number;
  replacement: string;
}

export interface RenderedTemplate {
  template: string;
  text: string;
  /** `file` — a new file at `file`; `members` — inside the class in `file`; `text` — to copy. */
  output: 'file' | 'members' | 'text';
  file: string | null;
  exists: boolean;
  insertion: TemplateInsertion | null;
  /** The insertion's tab stops, for a template that asked for them with `bennu.stops: true`. Empty
   *  otherwise: snippet syntax and generated code share `$`, so it is never assumed. */
  insertion_stops: TemplateStop[];
  /** For `members`: the imports they need, as edits to the class's file — applied with the insertion. */
  import_edits: TemplateEdit[];
  notes: string[];
  /** With `traceLines`: for each line of what is shown — the insertion's text when there is one — the
   *  1-based template line that wrote it, `0` for none. `null` when it cannot be told. */
  source_lines: number[] | null;
}

export interface RenderTemplateRequest {
  root: string;
  kind: TemplateKindId;
  /** The project's template for the kind when absent. */
  template?: string | null;
  /** The Java file holding the class — every kind but `new-file`. */
  file?: string | null;
  /** That file's unsaved text. */
  source?: string | null;
  /** A byte offset inside the class; the file's first class when absent. */
  offset?: number | null;
  /** The simple name of the class in `file`, nested ones included; `offset` wins. */
  class?: string | null;
  /** For `config-class`: the key whose group of keys the class binds; the one at `offset` when absent. */
  prefix?: string | null;
  /** For `config-class`: read the profile files beside `file` too. `true` when absent. */
  profiles?: boolean | null;
  /** For `config-class`: the keys the class gets, relative to `prefix`; every key when absent. */
  keys?: string[] | null;
  /** For `config-class`: a group's shape by its key below `prefix` — `true` a Map, `false` a type per key. */
  maps?: Record<string, boolean> | null;
  /** For `new-file`: the directory, and the name typed. */
  directory?: string | null;
  name?: string | null;
  /** A template's unsaved text, rendered instead of the saved one — and its output's extension. */
  text?: string | null;
  extension?: string | null;
  /** Values laid over what the template renders with — objects merge key by key. */
  parameters?: Record<string, unknown> | null;
  /** Also say which template line wrote each output line — a second render, for a preview. */
  traceLines?: boolean | null;
}

export interface TemplateSchema {
  /** JSON Schema of what a template of the kind is rendered with. */
  context: Record<string, unknown>;
  filters: { name: string; doc: string }[];
}

/** Every kind's templates, or one kind's; with the project's choice when a root is given.
 *  Wire: `bennu_list_templates`. */
export function listTemplates(root: string | null, kind?: TemplateKindId): Promise<KindTemplates[]> {
  return bennu('bennu_list_templates', { args: { root, kind: kind ?? null } });
}

/** Create a template as a copy of `from` (the kind's first built-in when null); returns its path.
 *  `extension` is what it writes (`java`, `rs`, or empty for plain text) — honoured only by the kinds
 *  that pick their language; the copied template's own otherwise. Wire: `bennu_template_new`. */
export function newTemplate(
  kind: TemplateKindId,
  name: string,
  from: string | null,
  extension?: string | null,
): Promise<string> {
  return bennu('bennu_template_new', { args: { kind, name, from, extension: extension ?? null } });
}

/** Rename one of your templates, keeping what it writes; returns its new path. A project generating
 *  with it follows the new name. Wire: `bennu_template_rename`. Built-ins are refused. */
export function renameTemplate(
  kind: TemplateKindId,
  name: string,
  newName: string,
  root: string | null,
): Promise<string> {
  return bennu('bennu_template_rename', { args: { kind, name, new_name: newName, root } });
}

/** A template's text, for a preview of what it writes. Wire: `bennu_template_text`. */
export function templateText(kind: TemplateKindId, name: string): Promise<string> {
  return bennu('bennu_template_text', { args: { kind, name } });
}

/** Wire: `bennu_template_delete`. Built-ins are refused. */
export function deleteTemplate(kind: TemplateKindId, name: string): Promise<void> {
  return bennu('bennu_template_delete', { args: { kind, name } });
}

/** The file to open a template from — a built-in's written out first, which the editor shows read-only.
 *  Wire: `bennu_template_open`. */
export function openTemplate(kind: TemplateKindId, name: string): Promise<string> {
  return bennu('bennu_template_open', { args: { kind, name } });
}

/** Set the word that expands one of your abbreviations, independently of its file name. Empty puts
 *  it back to being named by its file. Wire: `bennu_template_set_abbrev`. */
export function setTemplateAbbrev(name: string, abbrev: string): Promise<void> {
  return bennu('bennu_template_set_abbrev', { args: { name, abbrev } });
}

/** Wire: `bennu_template_set_project`. */
export function setProjectTemplate(root: string, kind: TemplateKindId, name: string): Promise<void> {
  return bennu('bennu_template_set_project', { args: { root, kind, name } });
}

/** What a template of `kind` can read, and Bennu's filters. Wire: `bennu_template_schema`. */
export function templateSchema(kind: TemplateKindId): Promise<TemplateSchema> {
  return bennu('bennu_template_schema', { args: { kind } });
}

/** Render a template — a saved one, or unsaved text — and say where the result goes. Nothing is
 *  written. Wire: `bennu_render_template`. */
export function renderTemplate(request: RenderTemplateRequest): Promise<RenderedTemplate> {
  return bennu('bennu_render_template', {
    args: {
      root: request.root,
      kind: request.kind,
      template: request.template ?? null,
      file: request.file ?? null,
      source: request.source ?? null,
      offset: request.offset ?? null,
      class: request.class ?? null,
      prefix: request.prefix ?? null,
      profiles: request.profiles ?? null,
      keys: request.keys ?? null,
      maps: request.maps ?? null,
      directory: request.directory ?? null,
      name: request.name ?? null,
      text: request.text ?? null,
      extension: request.extension ?? null,
      parameters: request.parameters ?? null,
      trace_lines: request.traceLines ?? false,
    },
  });
}

/** Write generated text that is a new file. Wire: `bennu_template_create_file`. */
export function createFileFromTemplate(root: string, file: string, text: string): Promise<void> {
  return bennu('bennu_template_create_file', { args: { root, file, text } });
}

/** What a configuration class can be written for, at the caret of a configuration file. */
export interface ConfigClassOrigin {
  /** The key at the caret. */
  key: string | null;
  /** Every group of keys the caret is in, outermost first — or the groups at the top of the file. */
  prefixes: string[];
  /** The innermost group the caret is in — or, for a selection, the group its keys share. */
  prefix: string | null;
  /** The keys a selection named, relative to `prefix`; `null` without a selection. */
  selected: string[] | null;
  class_name: string;
  /** Where the class would go, and the package that folder is. */
  directory: string;
  package: string;
  /** The profile files beside the open one. */
  profiles: string[];
}

/** Wire: `bennu_config_class_origin`. */
export function configClassOrigin(
  root: string,
  file: string,
  source: string,
  offset: number | null,
  selectionEnd: number | null = null,
): Promise<ConfigClassOrigin> {
  return bennu('bennu_config_class_origin', { args: { root, file, source, offset, selection_end: selectionEnd } });
}

/** One key of a group, for choosing which of them the class gets. */
export interface ConfigKeyNode {
  /** Relative to the prefix: `smtp.auth`. */
  key: string;
  name: string;
  depth: number;
  /** Has keys under it. */
  group: boolean;
  /** A group whose entries look like names of one thing — bound as a `Map` unless chosen otherwise. */
  map: boolean;
  /** The value of a single one, `[3]` for a list of three. */
  sample: string;
}

/** The keys under a prefix, parents first. Wire: `bennu_config_class_keys`. */
export function configClassKeys(file: string, source: string, prefix: string, profiles: boolean): Promise<ConfigKeyNode[]> {
  return bennu('bennu_config_class_keys', { args: { file, source, prefix, profiles } });
}
