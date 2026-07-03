/**
 * Bennu form-analysis IPC — the form → action → fields inspector.
 *
 * Kept in its own file (not `index.ts`) so concurrent edits to the main bennu IPC
 * surface don't race. Routes through the generic `bennu(...)` rpc bridge to `bennu-be`,
 * wrapping fields under `{ args: … }` (the proven convention). Wire shapes below mirror
 * the BE `FormAnalysis` / `FormInfo` / `FormFieldInfo` (proto contract) field-for-field.
 */

import { bennu } from '../rpc';

/** One input field of a JSP `<form>`, correlated against its action class — mirrors the BE
 *  `FormFieldInfo`. The inspector shows the name, the control kind, and two badges: whether
 *  it binds (a writable property of the action class) and whether it is validated. */
export interface FormFieldInfo {
  /** The raw form-field name (the `name=` / legacy `property=` attribute value). */
  name: string;
  /** The control kind label (`text` / `password` / `hidden` / `checkbox` / `radio` /
   *  `select` / `textarea` / `submit` / `file` / `other`). */
  control: string;
  /** True when `name` is a writable property (a setter) of the resolved action class. */
  bound: boolean;
  /** True when `name` carries a validation rule for the resolved action class. */
  validated: boolean;
  /** Start byte offset of the field name value inside the quotes. */
  start: number;
  /** End byte offset (exclusive). */
  end: number;
}

/** One JSP `<form>` correlated with its action — mirrors the BE `FormInfo`. A form whose
 *  action doesn't resolve is still listed (all fields unbound/unvalidated, `action_class`
 *  null) so the inspector always shows every form. */
export interface FormInfo {
  /** The normalized action reference (`null` when the form has no `action=` or it is a
   *  computed OGNL expression). */
  action: string | null;
  /** The resolved implementation class FQCN (the C1 chain), if resolvable. */
  action_class: string | null;
  /** The struts config fragment the `<action>` is declared in (an openable go-to site),
   *  if the action resolved. */
  config_file: string | null;
  /** The form's `method=` (`get`/`post`), if present. */
  method: string | null;
  /** Start byte offset of the `<form>` open tag. */
  start: number;
  /** End byte offset (exclusive). */
  end: number;
  /** The form's input fields, each correlated against the action class. */
  fields: FormFieldInfo[];
}

/** The result of `bennu_form_analysis` for one JSP — mirrors the BE `FormAnalysis`. Empty
 *  (`forms: []`) for a non-JSP / project-less file (never rejects). */
export interface FormAnalysis {
  /** Every `<form>` found in the JSP, in source order. */
  forms: FormInfo[];
}

/** Analyse the forms of the JSP at `file` (any absolute path inside the owning project):
 *  each `<form>`, its action's resolved class + declaring config fragment, and each field's
 *  bind/validate correlation. Never rejects — a non-JSP / project-less file resolves to
 *  `{ forms: [] }`. Wire: `bennu_form_analysis` — `{ file }`. */
export function formAnalysis(file: string): Promise<FormAnalysis> {
  return bennu('bennu_form_analysis', { args: { file } });
}
