// Interpreter for the descriptor's declarative form rules. ALL conditional form
// behavior (required-when, dynamic hints) is backend-authored data in the
// descriptor; this is the generic evaluator the UI runs — no provider logic.

import type { AuthField, AuthMethod, FieldHint, FieldRule } from '$lib/types/corvus/providers';

type Values = Record<string, string>;

/** Evaluate a `FieldRule` against the current field values. */
export function matchRule(rule: FieldRule, values: Values): boolean {
  const v = (values[rule.field] ?? '').trim();
  const m = rule.matches;
  switch (m.op) {
    case 'nonEmpty': return v.length > 0;
    case 'endsWith': return v.endsWith(m.value);
    case 'equals':   return v === m.value;
    case 'contains': return v.includes(m.value);
    default:         return false;
  }
}

/** Whether a field must be filled given the current values (static `required`
 *  OR a matching `requiredWhen`). */
export function isFieldRequired(field: AuthField, values: Values): boolean {
  if (field.required) return true;
  return field.requiredWhen ? matchRule(field.requiredWhen, values) : false;
}

/** The hint to show under a fields form: the first conditional hint whose rule
 *  matches, else the unconditional (`when` absent) fallback, else `null`. */
export function resolveHint(hints: FieldHint[], values: Values): string | null {
  const conditional = hints.find((h) => h.when && matchRule(h.when, values));
  if (conditional) return conditional.text;
  const fallback = hints.find((h) => !h.when);
  return fallback ? fallback.text : null;
}

/** The fields of a method (empty for an OAuth method). */
export function fieldsOf(method: AuthMethod | undefined): AuthField[] {
  return method && method.kind.type === 'fields' ? method.kind.fields : [];
}

/** Whether a fields form is complete enough to submit (every effectively-required
 *  field has a non-empty value). */
export function canSubmitFields(method: AuthMethod | undefined, values: Values): boolean {
  const fields = fieldsOf(method);
  if (fields.length === 0) return false;
  return fields.every((f) => !isFieldRequired(f, values) || (values[f.key] ?? '').trim().length > 0);
}
