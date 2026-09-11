/**
 * Bennu DTO Lab IPC — a class tried out as JSON, validated on the project's own JVM, and turned into
 * tests from a template the user owns.
 *
 * Wire shapes mirror `crates/products/bennu/be/src/dtolab.rs`; the JVM replies (`read`, `validate`,
 * `default`) pass through from the harness as they are, so their fields are snake_case too.
 */

import { bennu } from '../rpc';

export interface DtoLabConstraint {
  /** Simple name (`Size`). */
  name: string;
  fqn: string;
  /** As written in source; a string literal without its quotes. */
  attributes: Record<string, string>;
  message: string | null;
}

export interface DtoLabField {
  name: string;
  /** What Jackson calls it. */
  json_name: string;
  type_name: string;
  ignored: boolean;
  setter: string | null;
  getter: string | null;
  constraints: DtoLabConstraint[];
}

export interface DtoLabClass {
  name: string;
  package: string;
  fqn: string;
  /** `com.example.Order$Line` — what the JVM is asked for. */
  binary: string;
  record: boolean;
  fields: DtoLabField[];
}

export interface DtoLabClassView {
  class: DtoLabClass;
  /** A payload sketched from source. */
  skeleton: unknown;
  /** `jakarta.validation`, `javax.validation`, or empty. */
  validation: string;
  /** Where a new test for the class goes. */
  test_file: string;
  /** The template the project generates with. */
  template: string;
}

/** A value as the JVM found it, with what it holds. */
export interface DtoLabObjectNode {
  name?: string;
  type: string | null;
  /** The value's text for a leaf; `null` for something with children. */
  text: string | null;
  children: DtoLabObjectNode[];
}

export interface DtoLabReadResult {
  ok: boolean;
  /** `jackson` when the project has it; `fields` when the payload was bound by field names. */
  binder: 'jackson' | 'fields';
  object: DtoLabObjectNode | null;
  /** What that object writes back — `null` without Jackson. */
  json: string | null;
  binding_error?: string;
  writing_error?: string;
}

export interface DtoLabViolation {
  path: string;
  template: string;
  message: string;
  constraint: string;
  attributes: Record<string, string>;
  invalid_value: string | null;
  constant?: string | null;
}

export interface DtoLabValidateResult {
  ok: boolean;
  binder: 'jackson' | 'fields';
  violations: DtoLabViolation[];
  binding_error?: string;
  /** Why the project's own message bundles could not be applied, when they could not. */
  note?: string;
}

export interface DtoLabTemplateInfo {
  name: string;
  origin: 'builtin' | 'global';
  path: string | null;
}

export interface DtoLabTemplates {
  templates: DtoLabTemplateInfo[];
  /** The one the project generates with. */
  project: string;
  /** Where global templates live. */
  dir: string;
}

export interface DtoLabTypeInFile {
  name: string;
  /** `Outer.Inner`. */
  path: string;
  depth: number;
}

export interface DtoLabPreview {
  file: string;
  exists: boolean;
  /** The whole resulting file. */
  text: string;
  /** Just what is added. */
  inserted: string;
  /** For an existing file: the text the insertion was computed against, and its byte offset. */
  base: string | null;
  offset: number | null;
  template: string;
  cases: number;
  /** The expectations came from the project's validator rather than from the source. */
  verified: boolean;
  warnings: string[];
  classes: DtoLabTypeInFile[];
}

export interface DtoLabTarget {
  file: string;
  /** `Outer.Inner`; the file's first class when absent. */
  class?: string | null;
}

export interface DtoLabGenerateRequest {
  root: string;
  file: string;
  source: string;
  offset: number | null;
  template?: string | null;
  target?: DtoLabTarget | null;
  fields?: string[] | null;
}

/** The class at the caret, read from source. `null` when there is none. Wire: `bennu_dtolab_class`. */
export function dtoLabClass(root: string, file: string, source: string, offset: number | null): Promise<DtoLabClassView | null> {
  return bennu('bennu_dtolab_class', { args: { root, file, source, offset } });
}

/** Bind a payload with the project's Jackson and write it back. Wire: `bennu_dtolab_read`. */
export function dtoLabRead(root: string, cls: string, json: string): Promise<DtoLabReadResult> {
  return bennu('bennu_dtolab_read', { args: { root, class: cls, json, locale: null, validation: null } });
}

/** The JSON the project's `ObjectMapper` writes for a new instance. Wire: `bennu_dtolab_default_json`. */
export function dtoLabDefaultJson(root: string, cls: string): Promise<{ ok: boolean; json: string | null }> {
  return bennu('bennu_dtolab_default_json', { args: { root, class: cls } });
}

/** Bind and validate with the project's own validator and bundles. Wire: `bennu_dtolab_validate`. */
export function dtoLabValidate(
  root: string,
  cls: string,
  json: string,
  locale: string | null,
  validation: string | null,
): Promise<DtoLabValidateResult> {
  return bennu('bennu_dtolab_validate', { args: { root, class: cls, json, locale, validation } });
}

/** The templates, and the one the project uses. Wire: `bennu_dtolab_templates`. */
export function dtoLabTemplates(root: string): Promise<DtoLabTemplates> {
  return bennu('bennu_dtolab_templates', { args: { root } });
}

/** Wire: `bennu_dtolab_set_project_template`. */
export function dtoLabSetProjectTemplate(root: string, name: string): Promise<void> {
  return bennu('bennu_dtolab_set_project_template', { args: { root, name } });
}

/** Create a global template as a copy of `from` (the built-in one when null); returns its path.
 *  Wire: `bennu_dtolab_new_template`. */
export function dtoLabNewTemplate(name: string, from: string | null): Promise<string> {
  return bennu('bennu_dtolab_new_template', { args: { name, from } });
}

/** What a generation would write. Nothing is written. Wire: `bennu_dtolab_generate`. */
export function dtoLabGenerate(request: DtoLabGenerateRequest): Promise<DtoLabPreview> {
  return bennu('bennu_dtolab_generate', {
    args: {
      ...request,
      template: request.template ?? null,
      target: request.target ? { file: request.target.file, class: request.target.class ?? null, source: null } : null,
      fields: request.fields ?? null,
    },
  });
}

/** Write a generated test that is a new file. Wire: `bennu_dtolab_create_file`. */
export function dtoLabCreateFile(root: string, file: string, text: string): Promise<void> {
  return bennu('bennu_dtolab_create_file', { args: { root, file, text } });
}
