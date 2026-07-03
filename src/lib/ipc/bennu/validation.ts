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
