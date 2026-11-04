/**
 * The formatter's options as data — ONE list, in the order and groups of `style.rs`, from which the
 * settings page renders every row. Adding a field to the Rust struct means adding one entry here
 * (the unit test fails until the two agree), not writing a row in a component.
 *
 * Pure on purpose (no icons, no components): the page maps a group id to its glyph.
 */
import type { FormatKey } from '$lib/types/bennu-format';

export type FormatGroupId =
  | 'indentation' | 'blank_lines' | 'braces' | 'spacing' | 'wrapping'
  | 'imports' | 'annotations' | 'comments' | 'file' | 'behaviour';

export interface FormatGroup {
  id: FormatGroupId;
  label: string;
  /** One sentence under the group's title. */
  description: string;
}

export interface EnumChoice {
  value: string;
  label: string;
}

interface Common {
  key: FormatKey;
  group: FormatGroupId;
  label: string;
  description: string;
  /** A boolean option that must be on for this one to mean anything; the row stays visible but inert. */
  dependsOn?: FormatKey;
}

export interface BoolOption extends Common { kind: 'bool' }
export interface IntOption extends Common { kind: 'int'; min: number; max: number; step?: number; suffix?: string }
export interface EnumOption extends Common { kind: 'enum'; choices: EnumChoice[] }
export interface StringListOption extends Common { kind: 'stringlist'; placeholder: string }
export type FormatOption = BoolOption | IntOption | EnumOption | StringListOption;

export const FORMAT_GROUPS: FormatGroup[] = [
  { id: 'indentation', label: 'Indentation', description: 'How wide a level is and how a wrapped line is indented.' },
  { id: 'blank_lines', label: 'Blank lines', description: 'How many blank lines are kept or forced.' },
  { id: 'braces', label: 'Braces', description: 'Where each kind of opening brace goes.' },
  { id: 'spacing', label: 'Spacing', description: 'Spaces around operators, commas and brackets. Applied only when the master switch is on.' },
  { id: 'wrapping', label: 'Wrapping', description: 'The right margin and how each kind of list or expression is broken across lines.' },
  { id: 'imports', label: 'Imports', description: 'Sorting and grouping of the import block.' },
  { id: 'annotations', label: 'Annotations', description: 'Where annotations sit relative to what they annotate.' },
  { id: 'comments', label: 'Comments', description: 'Comment alignment and Javadoc.' },
  { id: 'file', label: 'File', description: 'What the file ends with and how its lines end.' },
  { id: 'behaviour', label: 'Behaviour', description: 'When the formatter runs and which sources of style it listens to.' },
];

// ── Factories — they exist so the list below reads as a table, not as 80 object literals ──────────

type Meta = [key: FormatKey, label: string, description: string];

const bool = (group: FormatGroupId, [key, label, description]: Meta, dependsOn?: FormatKey): BoolOption =>
  ({ kind: 'bool', group, key, label, description, dependsOn });

const int = (
  group: FormatGroupId, [key, label, description]: Meta,
  min: number, max: number, extra: { step?: number; suffix?: string; dependsOn?: FormatKey } = {},
): IntOption => ({ kind: 'int', group, key, label, description, min, max, ...extra });

const choice = (
  group: FormatGroupId, [key, label, description]: Meta, choices: EnumChoice[], dependsOn?: FormatKey,
): EnumOption => ({ kind: 'enum', group, key, label, description, choices, dependsOn });

const BRACE_CHOICES: EnumChoice[] = [
  { value: 'preserve', label: 'Keep' },
  { value: 'same_line', label: 'Same line' },
  { value: 'next_line', label: 'Next line' },
];

const WRAP_CHOICES: EnumChoice[] = [
  { value: 'preserve', label: 'Keep as written' },
  { value: 'never', label: 'Never wrap' },
  { value: 'fill', label: 'Wrap if needed' },
  { value: 'chop', label: 'Chop if needed' },
  { value: 'always', label: 'Always one per line' },
];

const ANNOTATION_CHOICES: EnumChoice[] = [
  { value: 'preserve', label: 'Keep' },
  { value: 'own_line', label: 'Own line' },
  { value: 'same_line', label: 'Same line' },
];

const SPACING = 'normalize_spacing' as const;
const IMPORTS = 'organize_imports' as const;

export const FORMAT_OPTIONS: FormatOption[] = [
  // ── Indentation
  int('indentation', ['indent_width', 'Indent width', 'Width of one indent level, in columns — and the display width of a tab.'], 1, 16, { suffix: 'col' }),
  bool('indentation', ['use_tabs', 'Indent with tabs', 'Use tab characters instead of spaces for each indent level.']),
  int('indentation', ['continuation_indent', 'Continuation indent', 'Extra indent of a wrapped line, relative to the line that started the statement.'], 0, 32, { suffix: 'col' }),
  bool('indentation', ['indent_case_body', 'Indent case bodies', 'Indent the statements under a case label one level in from it.']),
  bool('indentation', ['align_wrapped_to_paren', 'Align wrapped lists to the parenthesis', 'Align a wrapped argument or parameter list under the character after its ( instead of using the continuation indent.']),

  // ── Blank lines
  int('blank_lines', ['max_blank_lines', 'Keep at most', 'The most consecutive blank lines kept anywhere. 0 removes them all.'], 0, 10),
  int('blank_lines', ['blank_lines_after_header', 'After package and imports', 'Blank lines forced after the package line and after the import block. 0 leaves them as written.'], 0, 5),
  int('blank_lines', ['blank_lines_before_member', 'Before a member', 'Blank lines forced before a method, constructor, initializer or nested type that follows another member. 0 leaves them as written.'], 0, 5),

  // ── Braces
  choice('braces', ['brace_type', 'Type declarations', 'class, interface, enum and record bodies.'], BRACE_CHOICES),
  choice('braces', ['brace_method', 'Methods', 'Method, constructor and initializer bodies.'], BRACE_CHOICES),
  choice('braces', ['brace_block', 'Blocks', 'if, for, while, do, try, switch and synchronized blocks.'], BRACE_CHOICES),
  choice('braces', ['brace_continuation', 'else, catch, finally', 'Where else, catch, finally and the while of a do go relative to the closing brace.'], BRACE_CHOICES),
  bool('braces', ['force_braces', 'Always use braces', 'Add braces around the single-statement body of an if, else, for or while.']),

  // ── Spacing
  bool('spacing', ['normalize_spacing', 'Normalize spacing', 'Master switch for the rules below. Off, the spacing you wrote is left alone.']),
  bool('spacing', ['space_before_control_paren', 'Before control parenthesis', 'if (x) rather than if(x) — also for, while, switch, catch and synchronized.'], SPACING),
  bool('spacing', ['space_before_call_paren', 'Before call parenthesis', 'foo (x) rather than foo(x) in a call or a declaration.'], SPACING),
  bool('spacing', ['space_before_lbrace', 'Before opening brace', ') { rather than ){.'], SPACING),
  bool('spacing', ['space_around_binary', 'Around binary operators', 'Spaces around + - * / % && || == != < > <= >= & | ^ << >> >>>.'], SPACING),
  bool('spacing', ['space_around_assign', 'Around assignment', 'Spaces around = and the compound assignments.'], SPACING),
  bool('spacing', ['space_around_ternary', 'Around ternary', 'Spaces around ? and : of a conditional expression.'], SPACING),
  bool('spacing', ['space_around_arrow', 'Around arrows', 'Spaces around -> of a lambda or an arrow case.'], SPACING),
  bool('spacing', ['space_after_comma', 'After comma', 'A space after , and after ; in a for.'], SPACING),
  bool('spacing', ['space_before_comma', 'Before comma', 'A space before ,.'], SPACING),
  bool('spacing', ['space_after_cast', 'After cast', '(Type) value rather than (Type)value.'], SPACING),
  bool('spacing', ['space_within_parens', 'Within parentheses', '( x ) rather than (x).'], SPACING),
  bool('spacing', ['space_within_brackets', 'Within brackets', '[ i ] rather than [i].'], SPACING),
  bool('spacing', ['space_within_array_braces', 'Within array braces', '{ 1, 2 } rather than {1, 2} in an array initializer.'], SPACING),
  bool('spacing', ['space_within_angles', 'Within angle brackets', 'List< String > rather than List<String>.'], SPACING),
  bool('spacing', ['space_around_foreach_colon', 'Around for-each colon', 'for (a : b) rather than for (a: b).'], SPACING),

  // ── Wrapping
  int('wrapping', ['line_width', 'Right margin', 'The width lines are wrapped to. 0 never wraps for width.'], 0, 400, { suffix: 'col' }),
  choice('wrapping', ['wrap_parameters', 'Parameters', 'The parameters of a method or constructor declaration.'], WRAP_CHOICES),
  choice('wrapping', ['wrap_arguments', 'Arguments', 'The arguments of a call or new.'], WRAP_CHOICES),
  choice('wrapping', ['wrap_chain', 'Call chains', 'A chain of .call()s, such as a stream pipeline.'], WRAP_CHOICES),
  int('wrapping', ['chain_min_calls', 'Chain wraps from', 'A chain is wrapped only from this many calls on.'], 1, 20, { suffix: 'calls' }),
  bool('wrapping', ['chain_keep_first_call', 'Keep the first call with its receiver', 'Keep receiver.firstCall() together on the first line of a wrapped chain.']),
  choice('wrapping', ['wrap_binary', 'Binary expressions', 'The operands of + - * / && || and the like.'], WRAP_CHOICES),
  choice('wrapping', ['wrap_ternary', 'Ternary', 'The ? and : parts of a conditional expression.'], WRAP_CHOICES),
  choice('wrapping', ['wrap_clauses', 'extends, implements, throws', 'The extends, implements and throws lists.'], WRAP_CHOICES),
  choice('wrapping', ['wrap_array_init', 'Array initializers', 'The elements of an array initializer.'], WRAP_CHOICES),
  bool('wrapping', ['operator_on_next_line', 'Operator starts the new line', 'A wrapped binary or ternary operator begins the new line instead of ending the old one.']),
  bool('wrapping', ['closing_paren_on_new_line', 'Closing parenthesis on its own line', 'The ) closing a chopped list goes on a line of its own.']),
  bool('wrapping', ['break_after_open_paren', 'Break after opening parenthesis', 'The ( of a chopped list is followed by a break instead of the first element staying beside it.']),

  // ── Imports
  bool('imports', ['organize_imports', 'Organize imports on format', 'Sort and group the imports as part of formatting.']),
  { kind: 'stringlist', group: 'imports', key: 'import_groups', label: 'Import groups', dependsOn: IMPORTS, placeholder: 'Package prefix, * or static',
    description: 'The groups, in order. Each is a package prefix (java, javax, org.springframework), * for everything else, or static for the static block when static imports are mixed.' },
  bool('imports', ['blank_line_between_import_groups', 'Blank line between groups', 'Separate two import groups with a blank line.'], IMPORTS),
  choice('imports', ['static_imports', 'Static imports', 'Where the static imports go in the block.'], [
    { value: 'first', label: 'First' },
    { value: 'last', label: 'Last' },
    { value: 'mixed', label: 'Mixed in' },
  ], IMPORTS),
  int('imports', ['wildcard_threshold', 'Collapse to a wildcard from', 'Collapse this many imports from one package into package.*. 0 never collapses. A wildcard can make a simple name ambiguous.'], 0, 99, { dependsOn: IMPORTS }),
  int('imports', ['static_wildcard_threshold', 'Collapse static imports from', 'The same for the static imports of one class. 0 never collapses.'], 0, 99, { dependsOn: IMPORTS }),

  // ── Annotations
  choice('annotations', ['annotations_type', 'On types', 'Annotations of a class, interface, enum or record.'], ANNOTATION_CHOICES),
  choice('annotations', ['annotations_method', 'On methods', 'Annotations of a method or constructor.'], ANNOTATION_CHOICES),
  choice('annotations', ['annotations_field', 'On fields', 'Annotations of a field.'], ANNOTATION_CHOICES),

  // ── Comments
  bool('comments', ['align_block_comments', 'Align block comments', 'Re-align the * of a block comment or Javadoc under the opening /*.']),
  bool('comments', ['space_after_line_comment', 'Space after //', '// text rather than //text.']),
  bool('comments', ['wrap_javadoc', 'Re-flow Javadoc', 'Re-flow the prose of a Javadoc paragraph to the right margin.']),

  // ── File
  bool('file', ['ensure_final_newline', 'End with a newline', 'Make sure the file ends with a line break.']),
  choice('file', ['line_ending', 'Line ending', 'The line ending written. Auto keeps whatever the file already uses.'], [
    { value: 'auto', label: 'Keep' },
    { value: 'lf', label: 'LF' },
    { value: 'crlf', label: 'CRLF' },
  ]),

  // ── Behaviour
  bool('behaviour', ['format_on_save', 'Format on save', 'Format a Java file when it is saved. Other languages are not touched.']),
  bool('behaviour', ['use_editorconfig', 'Read .editorconfig', 'Let the project’s .editorconfig (indent, tabs, width, line ending, final newline) win over the global style.']),
  bool('behaviour', ['auto_detect', 'Detect from existing code', 'Measure the project’s code and let what it does nearly always win over the global style. You can also detect once and store the result as the project style.']),
];

/** The options of one group, in catalogue order. */
export function optionsOf(group: FormatGroupId): FormatOption[] {
  return FORMAT_OPTIONS.filter((o) => o.group === group);
}

/** The option for a key, if the catalogue knows it. */
export function optionFor(key: string): FormatOption | undefined {
  return FORMAT_OPTIONS.find((o) => o.key === key);
}

/** A readable form of a style value, for badges and the detection list: `Next line`, `on`, `120`. */
export function describeValue(key: string, value: unknown): string {
  const option = optionFor(key);
  if (option?.kind === 'enum') return option.choices.find((c) => c.value === value)?.label ?? String(value);
  if (typeof value === 'boolean') return value ? 'on' : 'off';
  if (Array.isArray(value)) return value.join(', ');
  return String(value);
}
