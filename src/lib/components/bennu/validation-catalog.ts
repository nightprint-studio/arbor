/**
 * Struts 2 / XWork built-in field-validator catalog — the static knowledge the
 * "New validator" modal (BennuValidationModal) offers. Each entry names a validator
 * `type` (what goes in `<field-validator type="…">`) plus its typed parameters, so the
 * modal can render a param form and generate the `<param name=…>` block.
 *
 * This is pure Struts domain data (not project-specific), so it lives on the FE — no BE
 * round-trip. The action-class property list (the `<field name>` candidates) DOES come
 * from the backend (it needs the index); the validator set does not.
 *
 * Reference: the Apache Struts 2 bundled validators. Only the field-level ones are
 * offered here (the action-level `expression` validator isn't attached to a `<field>`).
 */

/** One typed parameter of a validator (`<param name=…>value</param>`). */
export interface ValidatorParam {
  name: string;
  label: string;
  kind: 'text' | 'number' | 'bool';
  placeholder?: string;
  /** Short hint shown under the field. */
  hint?: string;
}

/** One `<field-validator type="…">` the modal can generate. */
export interface ValidatorType {
  type: string;
  label: string;
  description: string;
  params: ValidatorParam[];
}

const TRIM: ValidatorParam = {
  name: 'trim',
  label: 'Trim',
  kind: 'bool',
  hint: 'Trim whitespace before validating.',
};

/** The bundled Struts 2 field validators, ordered by how often they appear in legacy
 *  Struts/Entando trees. */
export const STRUTS_VALIDATORS: ValidatorType[] = [
  {
    type: 'requiredstring',
    label: 'Required string',
    description: 'The field must be present and not blank.',
    params: [TRIM],
  },
  {
    type: 'required',
    label: 'Required',
    description: 'The field must be non-null (any type).',
    params: [],
  },
  {
    type: 'stringlength',
    label: 'String length',
    description: 'The string length must fall within a range.',
    params: [
      { name: 'minLength', label: 'Min length', kind: 'number', placeholder: 'e.g. 2' },
      { name: 'maxLength', label: 'Max length', kind: 'number', placeholder: 'e.g. 50' },
      TRIM,
    ],
  },
  {
    type: 'email',
    label: 'Email',
    description: 'The string must be a well-formed email address.',
    params: [],
  },
  {
    type: 'url',
    label: 'URL',
    description: 'The string must be a well-formed URL.',
    params: [],
  },
  {
    type: 'int',
    label: 'Integer range',
    description: 'The integer value must fall within a range.',
    params: [
      { name: 'min', label: 'Min', kind: 'number', placeholder: 'e.g. 0' },
      { name: 'max', label: 'Max', kind: 'number', placeholder: 'e.g. 100' },
    ],
  },
  {
    type: 'double',
    label: 'Decimal range',
    description: 'The decimal value must fall within a range.',
    params: [
      { name: 'minInclusive', label: 'Min (inclusive)', kind: 'number' },
      { name: 'maxInclusive', label: 'Max (inclusive)', kind: 'number' },
    ],
  },
  {
    type: 'date',
    label: 'Date range',
    description: 'The date must fall within a range (uses the action locale format).',
    params: [
      { name: 'min', label: 'Min date', kind: 'text', placeholder: 'e.g. 01/01/2020' },
      { name: 'max', label: 'Max date', kind: 'text', placeholder: 'e.g. 31/12/2030' },
    ],
  },
  {
    type: 'regex',
    label: 'Regular expression',
    description: 'The string must match a regular expression.',
    params: [
      { name: 'regexExpression', label: 'Pattern', kind: 'text', placeholder: 'e.g. [A-Z]{2}\\d{3}' },
      { name: 'caseSensitive', label: 'Case sensitive', kind: 'bool' },
      TRIM,
    ],
  },
  {
    type: 'fieldexpression',
    label: 'Field expression (OGNL)',
    description: 'An OGNL expression that must evaluate to true for this field.',
    params: [
      { name: 'expression', label: 'OGNL expression', kind: 'text', placeholder: 'e.g. password == password2' },
    ],
  },
];

/** Look a validator up by its `type`. */
export function validatorByType(type: string): ValidatorType | undefined {
  return STRUTS_VALIDATORS.find((v) => v.type === type);
}
