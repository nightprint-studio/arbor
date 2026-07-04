/**
 * Bennu form-analysis IPC — the form → parameters inspector.
 *
 * Kept in its own file (not `index.ts`) so concurrent edits to the main bennu IPC
 * surface don't race. Routes through the generic `bennu(...)` rpc bridge to `bennu-be`,
 * wrapping fields under `{ args: … }` (the proven convention). Wire shapes below mirror
 * the BE `FormAnalysis` / `FormInfo` / `FormFieldInfo` (proto contract) field-for-field.
 *
 * The analysis is **include-aware**: a legacy JSP form is split across `<jsp:include>`s, so a
 * form's `fields` are the COMPLETE parameter set it posts — the form's own inputs plus every
 * input a fragment inside it contributes, each tagged with the `source_file` it lives in. The
 * walk is two-way: a page shows its children's parameters, and an included fragment shows the
 * parent form it feeds (its siblings + the page's own fields). `host_file` names the JSP that
 * declares the `<form>` — it may differ from the analysed file.
 */

import { bennu } from '../rpc';

/** One input field of a JSP `<form>`, correlated against its action class — mirrors the BE
 *  `FormFieldInfo`. The inspector shows the name, the value it posts, the control kind, two
 *  badges (bound / validated), and — for a field pulled in from an include — its source file. */
export interface FormFieldInfo {
  /** The raw form-field name (the `name=` / legacy `property=` attribute value). */
  name: string;
  /** The control kind label (`text` / `password` / `hidden` / `checkbox` / `radio` /
   *  `select` / `textarea` / `submit` / `file` / `other`). */
  control: string;
  /** The field's submitted `value=` as written (a fixed value or an `${…}`/`%{…}`
   *  expression) — the "hypothetical value" the form posts. `null` when no `value=`. */
  value: string | null;
  /** True when the field is submitted only under a condition (inside `<c:if>` / `<s:if>` /
   *  `<c:when>` / `<c:otherwise>`) — the inspector flags it as not always sent. */
  conditional: boolean;
  /** The nearest enclosing condition (`<c:if test>` value, or `"else"`), when `conditional`. */
  condition: string | null;
  /** True when `name` is a writable property (a setter) of the resolved action class. */
  bound: boolean;
  /** True when `name` carries a validation rule for the resolved action class. */
  validated: boolean;
  /** Forward-slashed path of the JSP this field's tag lives in — the form's `host_file` for its
   *  own fields, or an included fragment for a spliced-in parameter. */
  source_file: string;
  /** Start byte offset of the field name value inside the quotes, in `source_file`. */
  start: number;
  /** End byte offset (exclusive), in `source_file`. */
  end: number;
}

/** One JSP `<form>` correlated with its action — mirrors the BE `FormInfo`. A form whose
 *  action doesn't resolve is still listed (all fields unbound/unvalidated, `action_class`
 *  null) so the inspector always shows every form. `fields` is the complete include-expanded
 *  parameter set; `host_file` is the JSP that declares the `<form>`. */
export interface FormInfo {
  /** The normalized action reference (`null` when the form has no `action=` or it is a
   *  computed OGNL expression). For `action="<wp:action path=…/>"` it is the nested path. */
  action: string | null;
  /** The resolved implementation class FQCN (the C1 chain), if resolvable. */
  action_class: string | null;
  /** The struts config fragment the `<action>` is declared in (an openable go-to site),
   *  if the action resolved. */
  config_file: string | null;
  /** The form's `method=` (`get`/`post`), if present. */
  method: string | null;
  /** Forward-slashed path of the JSP that declares the `<form>` (may differ from the analysed
   *  file — a form on a page that includes the fragment you're viewing). */
  host_file: string;
  /** Start byte offset of the `<form>` open tag, in `host_file`. */
  start: number;
  /** End byte offset (exclusive), in `host_file`. */
  end: number;
  /** The form's parameters — own fields + every included-fragment input, source-tagged. */
  fields: FormFieldInfo[];
}

/** The result of `bennu_form_analysis` for one JSP — mirrors the BE `FormAnalysis`. Every
 *  `<form>` relevant to the file (its own, a fragment it includes, or a page that includes it),
 *  each with its aggregated parameters. Empty (`forms: []`) for a non-JSP / project-less /
 *  non-participating file (never rejects). */
export interface FormAnalysis {
  /** Every relevant `<form>`, each with its complete parameter set. */
  forms: FormInfo[];
  /** True when the include walk hit its node cap and left related files unvisited (the FE
   *  shows a hint so a huge include graph never silently drops coverage). */
  truncated: boolean;
}

/** Analyse the forms relevant to the JSP at `file` (any absolute path inside the owning
 *  project), **include-aware**: each `<form>` on the file, on a fragment it includes, or on a
 *  page that includes it, with its complete include-expanded parameter set and each field's
 *  bind/validate correlation. Never rejects — a non-JSP / project-less file resolves to
 *  `{ forms: [], truncated: false }`.
 *
 *  The include graph is served from an incremental per-project cache: the reactive per-tab
 *  fetch (`full = false`) refreshes only the open file; pass `full = true` (the Refresh button)
 *  to force a full re-walk so a newly-added file or parent include is picked up.
 *  Wire: `bennu_form_analysis` — `{ file, full }`. */
export function formAnalysis(file: string, full = false): Promise<FormAnalysis> {
  return bennu('bennu_form_analysis', { args: { file, full } });
}
