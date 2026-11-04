//! The formatter's preferences — one struct, one meaning per field, serialised as-is to TOML / JSON.
//!
//! Every field has a serde default, so a file (or a project override) that names three of them is a
//! complete style: the rest is [`FormatStyle::default`]. That is also what makes the **layers**
//! cheap — the backend resolves `defaults ← global ← detected ← .editorconfig ← project` as JSON
//! objects laid over one another ([`FormatStyle::layered`]) and decodes the result once.
//!
//! ## The default is the cautious one
//!
//! `normalize_spacing` is off and every wrap / brace / annotation policy is `Preserve`, so a fresh
//! install formats exactly as the previous formatter did: re-indentation, trailing whitespace and
//! blank lines. Anything that rewrites an expression is something the user opted into.
//!
//! The wire names are the field names; the frontend mirrors this file in
//! `src/lib/types/bennu-format.ts` and the settings page is built from the same list.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Where a `{` goes relative to the construct that opens it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BraceStyle {
    /// Leave each brace where the author put it.
    #[default]
    Preserve,
    /// `if (x) {` — K&R / Java convention.
    SameLine,
    /// The brace alone on the next line, at the construct's indent — Allman.
    NextLine,
}

/// What to do with the line breaks inside one kind of list or expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WrapPolicy {
    /// Keep the author's line breaks inside it.
    #[default]
    Preserve,
    /// Join it onto one line, however long that gets.
    Never,
    /// Join it, and when the line is too long break only where needed, filling each line.
    Fill,
    /// Join it, and when the line is too long put **every** element on its own line.
    Chop,
    /// Every element on its own line, even if it would fit.
    Always,
}

/// Where the annotations of a declaration sit relative to the declaration itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationPlacement {
    #[default]
    Preserve,
    /// Each annotation on a line of its own, above the declaration.
    OwnLine,
    /// Marker annotations (no arguments) share the declaration's line.
    SameLine,
}

/// Where `static` imports go in the import block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StaticImports {
    /// Before every ordinary import (IntelliJ's default).
    #[default]
    First,
    /// After every ordinary import.
    Last,
    /// Sorted in among the others, as if `static` were not there.
    Mixed,
}

/// The line ending written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LineEnding {
    /// Whatever the file already uses.
    #[default]
    Auto,
    Lf,
    Crlf,
}

/// Everything the Java formatter can be told. See the module doc for the layering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FormatStyle {
    // ── Indentation ────────────────────────────────────────────────────────────────────────
    /// Width of one indent level, in columns (and the tab's display width).
    pub indent_width: usize,
    /// Indent with tabs rather than spaces.
    pub use_tabs: bool,
    /// Extra indent, in columns, of a wrapped line relative to the line that started the statement.
    pub continuation_indent: usize,
    /// Indent the statements under a `case` label one level in from it.
    pub indent_case_body: bool,
    /// Align a wrapped argument / parameter list under the character after its `(` rather than
    /// indenting it by `continuation_indent`.
    pub align_wrapped_to_paren: bool,

    // ── Blank lines ────────────────────────────────────────────────────────────────────────
    /// The most consecutive blank lines to keep anywhere. `0` removes them all.
    pub max_blank_lines: usize,
    /// Blank lines forced after the `package` line and after the import block (`0` leaves them).
    pub blank_lines_after_header: usize,
    /// Blank lines forced before a method, constructor, initialiser or nested type that follows
    /// another member (`0` leaves them as written).
    pub blank_lines_before_member: usize,

    // ── Braces ─────────────────────────────────────────────────────────────────────────────
    /// `class` / `interface` / `enum` / `record` bodies.
    pub brace_type: BraceStyle,
    /// Method, constructor and initialiser bodies.
    pub brace_method: BraceStyle,
    /// `if` / `for` / `while` / `do` / `try` / `switch` / `synchronized` blocks.
    pub brace_block: BraceStyle,
    /// `else`, `catch`, `finally` and the `while` of a `do`: on the line of the closing `}` (`SameLine`)
    /// or on the next (`NextLine`).
    pub brace_continuation: BraceStyle,
    /// Add braces around the single-statement body of an `if` / `else` / `for` / `while`.
    pub force_braces: bool,

    // ── Spacing — applied only when `normalize_spacing` is on ───────────────────────────────
    /// Master switch for the spacing rules below. Off: the author's spacing is untouched.
    pub normalize_spacing: bool,
    /// `if (` rather than `if(` — also `for`, `while`, `switch`, `catch`, `synchronized`.
    pub space_before_control_paren: bool,
    /// `foo (x)` rather than `foo(x)` in a call or declaration.
    pub space_before_call_paren: bool,
    /// `) {` rather than `){`.
    pub space_before_lbrace: bool,
    /// Spaces around `+ - * / % && || == != < > <= >= & | ^ << >> >>>` (the binary ones).
    pub space_around_binary: bool,
    /// Spaces around `=` and the compound assignments.
    pub space_around_assign: bool,
    /// Spaces around `? :` of a ternary.
    pub space_around_ternary: bool,
    /// Spaces around `->` of a lambda or an arrow `case`.
    pub space_around_arrow: bool,
    /// A space after `,` and `;` in a `for`.
    pub space_after_comma: bool,
    /// A space before `,`.
    pub space_before_comma: bool,
    /// `(Type) value` rather than `(Type)value`.
    pub space_after_cast: bool,
    /// `( x )` rather than `(x)`.
    pub space_within_parens: bool,
    /// `[ i ]` rather than `[i]`.
    pub space_within_brackets: bool,
    /// `{ 1, 2 }` rather than `{1, 2}` in an array initialiser.
    pub space_within_array_braces: bool,
    /// `List< String >` rather than `List<String>`.
    pub space_within_angles: bool,
    /// `for (a : b)` rather than `for (a: b)`.
    pub space_around_foreach_colon: bool,

    // ── Wrapping ───────────────────────────────────────────────────────────────────────────
    /// The right margin, in columns. `0` never wraps for width.
    pub line_width: usize,
    /// Parameters of a method / constructor declaration.
    pub wrap_parameters: WrapPolicy,
    /// Arguments of a call or `new`.
    pub wrap_arguments: WrapPolicy,
    /// A chain of `.call()`s.
    pub wrap_chain: WrapPolicy,
    /// A chain wraps only from this many calls on.
    pub chain_min_calls: usize,
    /// Keep `receiver.firstCall()` together on the first line of a wrapped chain.
    pub chain_keep_first_call: bool,
    /// Operands of `+ - * / && ||` …
    pub wrap_binary: WrapPolicy,
    /// `? :` of a ternary.
    pub wrap_ternary: WrapPolicy,
    /// The `extends` / `implements` / `throws` lists.
    pub wrap_clauses: WrapPolicy,
    /// The elements of an array initialiser.
    pub wrap_array_init: WrapPolicy,
    /// A wrapped binary / ternary operator starts the new line instead of ending the old one.
    pub operator_on_next_line: bool,
    /// The `)` closing a **chopped** list goes on a line of its own.
    pub closing_paren_on_new_line: bool,
    /// The `(` of a chopped list is followed by a break (`foo(\n    a,\n    b)`) rather than the first
    /// element staying beside it.
    pub break_after_open_paren: bool,

    // ── Imports ────────────────────────────────────────────────────────────────────────────
    /// Sort and group the imports as part of formatting.
    pub organize_imports: bool,
    /// The groups, in order. Each entry is a package prefix (`java`, `javax`, `org.springframework`),
    /// `*` for everything else, or `static` for the static block when `static_imports` is `mixed`.
    pub import_groups: Vec<String>,
    /// A blank line between two groups.
    pub blank_line_between_import_groups: bool,
    /// Where the `static` imports go.
    pub static_imports: StaticImports,
    /// Collapse this many imports from one package into `package.*`. `0` never collapses.
    pub wildcard_threshold: usize,
    /// The same for static imports of one class's members.
    pub static_wildcard_threshold: usize,

    // ── Annotations ────────────────────────────────────────────────────────────────────────
    pub annotations_type: AnnotationPlacement,
    pub annotations_method: AnnotationPlacement,
    pub annotations_field: AnnotationPlacement,

    // ── Comments ───────────────────────────────────────────────────────────────────────────
    /// Re-align the `*` of a block comment / Javadoc continuation lines under the opening `/*`.
    pub align_block_comments: bool,
    /// `// text` rather than `//text`.
    pub space_after_line_comment: bool,
    /// Re-flow the prose of a Javadoc paragraph to `line_width`.
    pub wrap_javadoc: bool,

    // ── File ───────────────────────────────────────────────────────────────────────────────
    pub ensure_final_newline: bool,
    pub line_ending: LineEnding,

    // ── Behaviour (not part of the output) ─────────────────────────────────────────────────
    /// Format a Java buffer when it is saved.
    pub format_on_save: bool,
    /// Read `.editorconfig` (indent, tabs, width, line ending, final newline) and let it win over the
    /// global style.
    pub use_editorconfig: bool,
    /// Measure the project's existing code and let what it does nearly always win over the global
    /// style. Off by default; the settings page can also detect once and store the result as the
    /// project's override.
    pub auto_detect: bool,
}

impl Default for FormatStyle {
    fn default() -> Self {
        Self {
            indent_width: 4,
            use_tabs: false,
            continuation_indent: 8,
            indent_case_body: true,
            align_wrapped_to_paren: false,

            max_blank_lines: 1,
            blank_lines_after_header: 0,
            blank_lines_before_member: 0,

            brace_type: BraceStyle::Preserve,
            brace_method: BraceStyle::Preserve,
            brace_block: BraceStyle::Preserve,
            brace_continuation: BraceStyle::Preserve,
            force_braces: false,

            normalize_spacing: false,
            space_before_control_paren: true,
            space_before_call_paren: false,
            space_before_lbrace: true,
            space_around_binary: true,
            space_around_assign: true,
            space_around_ternary: true,
            space_around_arrow: true,
            space_after_comma: true,
            space_before_comma: false,
            space_after_cast: true,
            space_within_parens: false,
            space_within_brackets: false,
            space_within_array_braces: false,
            space_within_angles: false,
            space_around_foreach_colon: true,

            line_width: 120,
            wrap_parameters: WrapPolicy::Preserve,
            wrap_arguments: WrapPolicy::Preserve,
            wrap_chain: WrapPolicy::Preserve,
            chain_min_calls: 3,
            chain_keep_first_call: true,
            wrap_binary: WrapPolicy::Preserve,
            wrap_ternary: WrapPolicy::Preserve,
            wrap_clauses: WrapPolicy::Preserve,
            wrap_array_init: WrapPolicy::Preserve,
            operator_on_next_line: true,
            closing_paren_on_new_line: false,
            break_after_open_paren: false,

            organize_imports: false,
            import_groups: vec!["java".into(), "javax".into(), "*".into()],
            blank_line_between_import_groups: true,
            static_imports: StaticImports::First,
            wildcard_threshold: 0,
            static_wildcard_threshold: 0,

            annotations_type: AnnotationPlacement::Preserve,
            annotations_method: AnnotationPlacement::Preserve,
            annotations_field: AnnotationPlacement::Preserve,

            align_block_comments: true,
            space_after_line_comment: false,
            wrap_javadoc: false,

            ensure_final_newline: true,
            line_ending: LineEnding::Auto,

            format_on_save: false,
            use_editorconfig: true,
            auto_detect: false,
        }
    }
}

impl FormatStyle {
    /// The style that results from laying `layers` over the defaults, first to last (later wins).
    /// Each layer is a JSON object naming only the keys it sets; keys this build does not know, or
    /// whose value has the wrong shape, are dropped rather than failing the whole style — a
    /// hand-edited config must degrade to a slightly different format, not to none.
    pub fn layered(layers: &[&Map<String, Value>]) -> FormatStyle {
        let mut merged = match serde_json::to_value(FormatStyle::default()) {
            Ok(Value::Object(m)) => m,
            _ => Map::new(),
        };
        for layer in layers {
            for (key, value) in layer.iter() {
                let Some(slot) = merged.get_mut(key) else { continue };
                // Same JSON kind as the default, or it is not this field.
                if std::mem::discriminant(slot) == std::mem::discriminant(value) {
                    *slot = value.clone();
                }
            }
        }
        // A value of the right kind can still be a wrong enum string; decode field by field is not
        // worth it — fall back to the previous layer's result by retrying without the offenders.
        match serde_json::from_value::<FormatStyle>(Value::Object(merged.clone())) {
            Ok(style) => style,
            Err(_) => {
                let mut safe = match serde_json::to_value(FormatStyle::default()) {
                    Ok(Value::Object(m)) => m,
                    _ => Map::new(),
                };
                for (key, value) in merged {
                    let mut probe = safe.clone();
                    probe.insert(key.clone(), value.clone());
                    if serde_json::from_value::<FormatStyle>(Value::Object(probe)).is_ok() {
                        safe.insert(key, value);
                    }
                }
                serde_json::from_value(Value::Object(safe)).unwrap_or_default()
            }
        }
    }

    /// This style as a JSON object (every key present) — what the settings page edits.
    pub fn to_map(&self) -> Map<String, Value> {
        match serde_json::to_value(self) {
            Ok(Value::Object(m)) => m,
            _ => Map::new(),
        }
    }

    /// The keys on which `self` differs from the defaults — the minimal override that reproduces it.
    pub fn diff_from_default(&self) -> Map<String, Value> {
        let base = FormatStyle::default().to_map();
        self.to_map().into_iter().filter(|(k, v)| base.get(k) != Some(v)).collect()
    }

    /// One indent level as text.
    pub fn indent_unit(&self) -> String {
        if self.use_tabs { "\t".to_string() } else { " ".repeat(self.indent_width.max(1)) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn obj(v: Value) -> Map<String, Value> {
        v.as_object().cloned().unwrap()
    }

    #[test]
    fn a_style_naming_three_keys_is_complete() {
        let style: FormatStyle =
            serde_json::from_value(json!({ "indent_width": 2, "use_tabs": true, "brace_method": "next_line" })).unwrap();
        assert_eq!(style.indent_width, 2);
        assert!(style.use_tabs);
        assert_eq!(style.brace_method, BraceStyle::NextLine);
        assert_eq!(style.line_width, 120);
    }

    #[test]
    fn later_layers_win_and_unknown_keys_are_dropped() {
        let global = obj(json!({ "indent_width": 2, "line_width": 100 }));
        let project = obj(json!({ "indent_width": 8, "no_such_key": true }));
        let style = FormatStyle::layered(&[&global, &project]);
        assert_eq!(style.indent_width, 8);
        assert_eq!(style.line_width, 100);
    }

    #[test]
    fn a_wrongly_shaped_value_is_ignored_not_fatal() {
        let layer = obj(json!({ "indent_width": "four", "brace_type": "sideways", "use_tabs": true }));
        let style = FormatStyle::layered(&[&layer]);
        assert_eq!(style.indent_width, 4);
        assert_eq!(style.brace_type, BraceStyle::Preserve);
        assert!(style.use_tabs);
    }

    #[test]
    fn the_diff_round_trips() {
        let style = FormatStyle { indent_width: 2, wrap_chain: WrapPolicy::Chop, ..FormatStyle::default() };
        let patch = style.diff_from_default();
        assert_eq!(patch.len(), 2);
        assert_eq!(FormatStyle::layered(&[&patch]), style);
    }
}
