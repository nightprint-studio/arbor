/**
 * Struts validation XML generation — turns a {@link ValidatorSpec} (chosen in
 * BennuValidationModal) into the `<field>` / `<field-validator>` block inserted into a
 * `*-validation.xml`. Pure string building (no editor / DOM), so it's trivially testable
 * and reusable.
 */

/** The user's choice in the modal: which field, which validator, its params + message. */
export interface ValidatorSpec {
  /** The action property the validator guards (`<field name="…">`). */
  field: string;
  /** The validator type (`<field-validator type="…">`). */
  type: string;
  /** Non-empty params only (`name → value`); empties are dropped by the caller. */
  params: Record<string, string>;
  /** The error message text. Ignored when `key` is set. */
  message: string;
  /** An i18n message key (`<message key="…"/>`) — wins over `message` when set. */
  key?: string;
  /** Stop further validators on this field once this one fails. */
  shortCircuit?: boolean;
}

/** Escape a string for use inside an XML attribute value (double-quoted). */
function escAttr(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/"/g, '&quot;');
}

/** Escape a string for use as XML text content. */
function escText(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

/** Render just the `<field-validator>` element (params + message), indented by `pad`. */
export function renderFieldValidator(spec: ValidatorSpec, pad = '    '): string {
  const inner = pad + '    ';
  const open = spec.shortCircuit
    ? `<field-validator type="${escAttr(spec.type)}" short-circuit="true">`
    : `<field-validator type="${escAttr(spec.type)}">`;
  const lines = [pad + open];
  for (const [name, value] of Object.entries(spec.params)) {
    if (value === '') continue;
    lines.push(`${inner}<param name="${escAttr(name)}">${escText(value)}</param>`);
  }
  lines.push(
    spec.key
      ? `${inner}<message key="${escAttr(spec.key)}"/>`
      : `${inner}<message>${escText(spec.message)}</message>`,
  );
  lines.push(pad + '</field-validator>');
  return lines.join('\n');
}

/** Render a whole `<field name="…">` block wrapping the validator, indented by `pad`. */
export function renderFieldBlock(spec: ValidatorSpec, pad = '    '): string {
  return [
    `${pad}<field name="${escAttr(spec.field)}">`,
    renderFieldValidator(spec, pad + '    '),
    `${pad}</field>`,
  ].join('\n');
}
