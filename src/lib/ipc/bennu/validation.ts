/**
 * Bennu Struts-validation IPC — the context for the "New validator" modal.
 *
 * Kept in its own file (not `index.ts`) so concurrent edits to the main bennu IPC
 * surface don't race. Routes through the generic `bennu(...)` rpc bridge to `bennu-be`,
 * wrapping fields under `{ args: … }` (the proven convention). Wire shape mirrors the BE
 * `ValidationContext` (proto contract) verbatim.
 */

import { bennu } from '../rpc';

/** Everything the "New validator" modal needs for a `<Action>-validation.xml` — mirrors
 *  the BE `ValidationContext`. Empty lists / `null` FQCN when the action class isn't
 *  indexed yet (the modal degrades to a free-text field name). */
export interface ValidationContext {
  /** The action class simple-name derived from the file name (`FooAction`). */
  action_simple: string;
  /** The resolved action class FQCN, when the project index knows it. */
  action_fqcn: string | null;
  /** The action's writable bean properties (from its setters) — the `<field name>`
   *  candidates the modal offers. */
  properties: string[];
  /** Field names already carrying a validator in this file (dedupe / reuse hints). */
  existing_fields: string[];
}

/** Resolve the modal context for the `<Action>-validation.xml` at `file` (any absolute
 *  path inside the owning project). Never rejects for an unresolved action — it returns a
 *  context with empty lists. Wire: `bennu_validation_context` — `{ file }`. */
export function validationContext(file: string): Promise<ValidationContext> {
  return bennu('bennu_validation_context', { args: { file } });
}

/** One `<param>` a validator accepts — mirrors the BE `ParamDefWire`. `kind` drives the FE control
 *  (`bool`→toggle, `int`/`long`/`double`→number, `date`/`text`/`ognl`/`regex`→text). */
export interface ValidatorParamDef {
  name: string;
  kind: 'bool' | 'int' | 'long' | 'double' | 'date' | 'text' | 'ognl' | 'regex';
  required: boolean;
}

/** A built-in validator definition — mirrors the BE `ValidatorDefWire`. Single source of truth,
 *  shared with the authoring layer, so the chain-builder never drifts from what the BE emits. */
export interface ValidatorDef {
  type_name: string;
  label: string;
  is_field: boolean;
  params: ValidatorParamDef[];
}

/** The built-in Struts2 validator vocabulary. Wire: `bennu_validator_catalog`. */
export function validatorCatalog(): Promise<ValidatorDef[]> {
  return bennu('bennu_validator_catalog', { args: {} });
}

/** One validator to author (the FE chain item) — mirrors the BE `AuthoredValidatorWire`. */
export interface AuthoredValidator {
  type_name: string;
  params: { name: string; value: string }[];
  message: { key: string | null; text: string } | null;
  short_circuit: boolean;
}

/** Append the ordered validator chain to `field` in `existingXml`, returning the new full
 *  document (pure BE authoring — the FE writes the result). Wire: `bennu_validation_author`. */
export function validationAuthor(
  existingXml: string,
  field: string,
  validators: AuthoredValidator[],
): Promise<string> {
  return bennu('bennu_validation_author', { args: { existing_xml: existingXml, field, validators } });
}

/** The `<Class>-validation.xml` bound to a Java action class + existence + content to open —
 *  mirrors the BE `ValidationTargetResult`. `null` when `file` isn't a `.java` path. */
export interface ValidationTarget {
  path: string;
  exists: boolean;
  /** Existing file text, or a fresh skeleton to write when it doesn't exist yet. */
  content: string;
}

/** Resolve the validation file for the Java action class `file`. Wire: `bennu_validation_target`. */
export function validationTarget(file: string): Promise<ValidationTarget | null> {
  return bennu('bennu_validation_target', { args: { file } });
}
