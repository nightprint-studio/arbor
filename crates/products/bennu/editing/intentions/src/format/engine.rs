//! The formatter's entry points and the order its passes run in.
//!
//! text → imports → tokens → classification → structure (forced braces, brace placement, annotations,
//! blank lines) → spacing → wrapping and layout → text, and then a check that the result still
//! contains exactly the tokens the input did. The check is the safety net under everything else: a
//! formatter bug becomes "the file is only re-indented", never "the file's code changed".

use super::classify::classify;
use super::imports::organize_imports;
use super::lex::{lex, signature, Kind};
use super::reindent::{diff_lines, reindent_source, IndentStyle};
use super::render::render;
use super::spacing::gaps;
use super::structure::{apply_annotations, apply_blank_lines, apply_braces, force_braces, units};
use super::style::{FormatStyle, LineEnding};
use crate::Edit;

/// `source`, formatted.
pub fn format_source(source: &str, style: &FormatStyle) -> String {
    let crlf = source.contains("\r\n");
    let src = source.replace("\r\n", "\n");

    let mut text = match pipeline(&src, style, 0, true) {
        Some(t) => t,
        None => fallback(&src, style),
    };

    let had_final = src.ends_with('\n');
    while text.ends_with('\n') {
        text.pop();
    }
    if !text.is_empty() && (style.ensure_final_newline || had_final) {
        text.push('\n');
    }
    let crlf_out = match style.line_ending {
        LineEnding::Auto => crlf,
        LineEnding::Lf => false,
        LineEnding::Crlf => true,
    };
    if crlf_out {
        text = text.replace('\n', "\r\n");
    }
    text
}

/// The line-range edits that reformat `source`; empty when it is already formatted.
pub fn format_edits(source: &str, style: &FormatStyle) -> Vec<Edit> {
    let formatted = format_source(source, style);
    if formatted == source {
        return Vec::new();
    }
    edits_between(source, &formatted)
}

/// Reformat only the lines touched by `start..end` (UTF-8 byte offsets). The selection is widened to
/// whole lines and formatted as a fragment indented like its first line, so a statement pasted
/// mid-method comes out at the method's indent.
pub fn format_range(source: &str, start: usize, end: usize, style: &FormatStyle) -> Vec<Edit> {
    let crlf = source.contains("\r\n");
    let src = source.replace("\r\n", "\n");
    let to_norm = |off: usize| -> usize {
        let off = off.min(source.len());
        if crlf { off - source[..off].matches("\r\n").count() } else { off }
    };
    let to_orig = |off: usize| -> usize { if crlf { off + src[..off].matches('\n').count() } else { off } };

    let (s, e) = (to_norm(start.min(end)), to_norm(end.max(start)));
    let ls = src[..s.min(src.len())].rfind('\n').map_or(0, |p| p + 1);
    let mut le = e.min(src.len());
    if le > ls && src.as_bytes()[le - 1] == b'\n' {
        le -= 1;
    }
    le = src[le..].find('\n').map_or(src.len(), |p| le + p);
    if le <= ls {
        return Vec::new();
    }
    let snippet = &src[ls..le];
    let iw = style.indent_width.max(1);
    let width_of = |l: &str| -> usize {
        l[..l.len() - l.trim_start().len()].chars().map(|c| if c == '\t' { iw } else { 1 }).sum()
    };
    // The fragment sits where the code before it says: one level in after a line that opens a block,
    // level with it otherwise; with nothing before it, as indented as it already is.
    let base = match src[..ls].lines().rev().find(|l| !l.trim().is_empty()) {
        Some(prev) => width_of(prev) + if prev.trim_end().ends_with('{') { iw } else { 0 },
        None => snippet.lines().find(|l| !l.trim().is_empty()).map(width_of).unwrap_or(0),
    };

    let Some(mut formatted) = pipeline(snippet, style, base, false) else { return Vec::new() };
    while formatted.ends_with('\n') {
        formatted.pop();
    }
    if formatted == snippet {
        return Vec::new();
    }
    let crlf_out = match style.line_ending {
        LineEnding::Auto => crlf,
        LineEnding::Lf => false,
        LineEnding::Crlf => true,
    };
    if crlf_out {
        formatted = formatted.replace('\n', "\r\n");
    }
    vec![Edit { start: to_orig(ls), end: to_orig(le), replacement: formatted }]
}

/// The previous formatter — re-indentation — as the answer when the engine's output fails its check.
fn fallback(src: &str, style: &FormatStyle) -> String {
    let indent = IndentStyle {
        indent_width: style.indent_width.max(1),
        use_tabs: style.use_tabs,
        max_blank_lines: style.max_blank_lines,
        indent_case_body: style.indent_case_body,
    };
    reindent_source(src, indent)
}

/// One run of the passes over `\n`-normalised text. `None` when the check fails.
fn pipeline(src: &str, style: &FormatStyle, base: usize, whole_file: bool) -> Option<String> {
    let mut text = src.to_string();
    if whole_file && style.organize_imports {
        if let Some(edit) = organize_imports(&text, style) {
            if edit.start <= edit.end && edit.end <= text.len() {
                text.replace_range(edit.start..edit.end, &edit.replacement);
            }
        }
    }

    let mut toks = lex(&text);
    if toks.is_empty() {
        return Some(String::new());
    }
    let mut info = classify(&toks);
    if style.force_braces && force_braces(&mut toks, &info) {
        info = classify(&toks);
    }
    if style.space_after_line_comment {
        for t in toks.iter_mut().filter(|t| t.kind == Kind::LineComment) {
            let rest = &t.text[2..];
            if rest.chars().next().is_some_and(char::is_alphanumeric) {
                t.text = format!("// {rest}");
            }
        }
    }

    let us = units(&toks, &info);
    apply_braces(&mut toks, &info, style);
    apply_annotations(&mut toks, &info, &us, style);
    apply_blank_lines(&mut toks, &info, &us, style);
    let gaps = gaps(&toks, &info, style);

    let expected = signature(&toks);
    let out = render(&mut toks, &info, &gaps, &us, style, base);
    if signature(&lex(&out)) != expected {
        return None;
    }
    Some(out)
}

/// Edits turning `before` into `after`: one per changed line when the line counts agree, else a
/// single edit over the lines between the common head and the common tail — which keeps the caret
/// where it was for every change but the one in the middle.
fn edits_between(before: &str, after: &str) -> Vec<Edit> {
    let b: Vec<&str> = before.split_inclusive('\n').collect();
    let a: Vec<&str> = after.split_inclusive('\n').collect();
    if b.len() == a.len() {
        return diff_lines(before, after);
    }
    let head = b.iter().zip(a.iter()).take_while(|(x, y)| x == y).count();
    let tail = b[head..].iter().rev().zip(a[head..].iter().rev()).take_while(|(x, y)| x == y).count();
    let start: usize = b[..head].iter().map(|l| l.len()).sum();
    let end: usize = before.len() - b[b.len() - tail..].iter().map(|l| l.len()).sum::<usize>();
    let replacement: String = a[head..a.len() - tail].concat();
    vec![Edit { start, end, replacement }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::style::{BraceStyle, WrapPolicy};

    fn fmt(src: &str, style: &FormatStyle) -> String {
        format_source(src, style)
    }

    fn d() -> FormatStyle {
        FormatStyle::default()
    }

    // ── The default style is the old formatter ───────────────────────────────────────────

    #[test]
    fn the_default_style_only_reindents() {
        let src = "class C {\nvoid m() {\nint  x=1;\n}\n}\n";
        assert_eq!(fmt(src, &d()), "class C {\n    void m() {\n        int  x=1;\n    }\n}\n");
    }

    #[test]
    fn trailing_whitespace_goes_and_blank_lines_collapse() {
        assert_eq!(fmt("class C {   \n\n\n\n    int x;\n}\n", &d()), "class C {\n\n    int x;\n}\n");
    }

    #[test]
    fn crlf_is_preserved() {
        assert_eq!(fmt("class C {\r\nint x;\r\n}\r\n", &d()), "class C {\r\n    int x;\r\n}\r\n");
    }

    #[test]
    fn a_brace_in_a_string_is_not_a_brace() {
        let out = fmt("class C {\nString s = \"{\";\nint x = 1;\n}\n", &d());
        assert!(out.contains("\n    int x = 1;"), "{out}");
    }

    #[test]
    fn broken_code_still_formats() {
        let out = fmt("class C {\nvoid m( {\nint x =\n", &d());
        assert!(out.starts_with("class C {\n    void m( {"), "{out}");
    }

    // ── Indentation ──────────────────────────────────────────────────────────────────────

    #[test]
    fn tabs_and_widths_follow_the_style() {
        let tabs = FormatStyle { use_tabs: true, ..d() };
        assert_eq!(fmt("class C {\nint x;\n}\n", &tabs), "class C {\n\tint x;\n}\n");
        let two = FormatStyle { indent_width: 2, ..d() };
        assert_eq!(fmt("class C {\nint x;\n}\n", &two), "class C {\n  int x;\n}\n");
    }

    #[test]
    fn a_continuation_line_is_indented_by_the_continuation_indent() {
        let out = fmt("class C {\nint x = a\n+ b;\n}\n", &FormatStyle { continuation_indent: 6, ..d() });
        assert_eq!(out, "class C {\n    int x = a\n          + b;\n}\n");
    }

    // ── Braces ───────────────────────────────────────────────────────────────────────────

    #[test]
    fn allman_methods() {
        let style = FormatStyle { brace_method: BraceStyle::NextLine, ..d() };
        let out = fmt("class A {\n    void m() {\n        x();\n    }\n}\n", &style);
        assert_eq!(out, "class A {\n    void m()\n    {\n        x();\n    }\n}\n");
    }

    #[test]
    fn same_line_braces_and_cuddled_else() {
        let style = FormatStyle { brace_block: BraceStyle::SameLine, brace_continuation: BraceStyle::SameLine, ..d() };
        let out = fmt("class A {\n    void m() {\n        if (a)\n        {\n            x();\n        }\n        else\n        {\n            y();\n        }\n    }\n}\n", &style);
        assert!(out.contains("        if (a) {\n            x();\n        } else {\n            y();\n        }\n"), "{out}");
    }

    #[test]
    fn forced_braces() {
        let style = FormatStyle { force_braces: true, ..d() };
        let out = fmt("class A {\n    void m() {\n        if (a) x();\n    }\n}\n", &style);
        assert!(out.contains("        if (a) {\n            x();\n        }\n"), "{out}");
    }

    // ── Spacing ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn spacing_is_normalised_when_asked() {
        let style = FormatStyle { normalize_spacing: true, ..d() };
        let out = fmt("class A {\nvoid m(int a,int b){\nif(a>b){\nint x=a+b;\n}\n}\n}\n", &style);
        assert!(out.contains("void m(int a, int b) {"), "{out}");
        assert!(out.contains("if (a > b) {"), "{out}");
        assert!(out.contains("int x = a + b;"), "{out}");
    }

    // ── Blank lines ──────────────────────────────────────────────────────────────────────

    #[test]
    fn blank_lines_are_forced_before_members() {
        let style = FormatStyle { blank_lines_before_member: 1, ..d() };
        let out = fmt("class A {\n    int x;\n    void a() {\n    }\n    void b() {\n    }\n}\n", &style);
        assert_eq!(out, "class A {\n    int x;\n\n    void a() {\n    }\n\n    void b() {\n    }\n}\n");
    }

    // ── Wrapping ─────────────────────────────────────────────────────────────────────────

    fn wrap(width: usize) -> FormatStyle {
        FormatStyle { line_width: width, ..d() }
    }

    #[test]
    fn chopping_arguments_puts_each_on_its_own_line() {
        let style = FormatStyle { wrap_arguments: WrapPolicy::Chop, ..wrap(40) };
        let out = fmt("class A {\n    void m() {\n        foo(alpha, beta, gamma, delta, epsilon, zeta);\n    }\n}\n", &style);
        assert!(
            out.contains("        foo(alpha,\n                beta,\n                gamma,\n                delta,\n                epsilon,\n                zeta);\n"),
            "{out}"
        );
    }

    #[test]
    fn filling_arguments_breaks_only_where_needed() {
        let style = FormatStyle { wrap_arguments: WrapPolicy::Fill, ..wrap(40) };
        let out = fmt("class A {\n    void m() {\n        foo(alpha, beta, gamma, delta, epsilon, zeta);\n    }\n}\n", &style);
        assert!(out.contains("        foo(alpha, beta, gamma, delta,\n                epsilon, zeta);\n"), "{out}");
    }

    #[test]
    fn a_short_line_is_joined_not_wrapped() {
        let style = FormatStyle { wrap_arguments: WrapPolicy::Chop, ..wrap(80) };
        let out = fmt("class A {\n    void m() {\n        foo(a,\n            b);\n    }\n}\n", &style);
        assert!(out.contains("        foo(a, b);\n") || out.contains("        foo(a,b);\n"), "{out}");
    }

    #[test]
    fn a_call_chain_breaks_before_its_dots() {
        let style = FormatStyle { wrap_chain: WrapPolicy::Chop, ..wrap(40) };
        let out = fmt("class A {\n    void m() {\n        String s = list.stream().filter(x).map(y).collect(z);\n    }\n}\n", &style);
        assert!(
            out.contains("        String s = list.stream()\n                .filter(x)\n                .map(y)\n                .collect(z);\n"),
            "{out}"
        );
    }

    #[test]
    fn binary_operators_wrap_before_the_operator_by_default() {
        let style = FormatStyle { wrap_binary: WrapPolicy::Chop, ..wrap(30) };
        let out = fmt("class A {\n    boolean b = aaaaaaaa && bbbbbbbb && cccccccc;\n}\n", &style);
        assert!(out.contains("    boolean b = aaaaaaaa\n            && bbbbbbbb\n            && cccccccc;\n"), "{out}");
    }

    #[test]
    fn parameters_can_be_chopped_with_the_closing_paren_alone() {
        let style = FormatStyle {
            wrap_parameters: WrapPolicy::Chop,
            break_after_open_paren: true,
            closing_paren_on_new_line: true,
            ..wrap(20)
        };
        let out = fmt("class A {\n    void m(int a, int b, int c) {\n        x();\n    }\n}\n", &style);
        assert_eq!(
            out,
            "class A {\n    void m(\n            int a,\n            int b,\n            int c\n    ) {\n        x();\n    }\n}\n"
        );
    }

    #[test]
    fn a_lambda_block_in_the_arguments_is_left_alone() {
        let style = FormatStyle { wrap_arguments: WrapPolicy::Chop, ..wrap(20) };
        let src = "class A {\n    void m() {\n        list.forEach(x -> {\n            f(x);\n        });\n    }\n}\n";
        assert_eq!(fmt(src, &style), src);
    }

    // ── Comments ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn javadoc_lines_follow_the_indent() {
        let out = fmt("class A {\n/**\n * Doc.\n */\nvoid m() {}\n}\n", &d());
        assert_eq!(out, "class A {\n    /**\n     * Doc.\n     */\n    void m() {}\n}\n");
    }

    // ── Idempotence ──────────────────────────────────────────────────────────────────────

    const SAMPLE: &str = r#"package a.b;

import java.util.List;

/** Doc. */
@Deprecated
public class Sample<T extends Comparable<T>> extends Base implements Runnable, Comparable<Sample<T>> {
    private final Map<String, List<Integer>> map = new HashMap<>();
    private static final int[] NUMS = {1, 2, 3};

    public Sample(String name, int count, List<String> items, Map<String, Object> options, boolean flag) throws IOException, IllegalStateException {
        super(name);
        if (count > 0 && !items.isEmpty() || flag) { run(); } else { stop(); }
        for (String s : items) System.out.println(s);
        String text = items.stream().filter(s -> s.length() > 3).map(String::trim).collect(Collectors.joining(", "));
        int v = flag ? count + 1 : count - 1;
        switch (count) {
            case 1:
                run();
                break;
            default:
                stop();
        }
        list.forEach(x -> {
            f(x);
        });
    }

    @Override
    public void run() { }
}
"#;

    #[test]
    fn formatting_twice_changes_nothing() {
        let styles = vec![
            d(),
            FormatStyle { normalize_spacing: true, ..d() },
            FormatStyle {
                normalize_spacing: true,
                wrap_parameters: WrapPolicy::Chop,
                wrap_arguments: WrapPolicy::Fill,
                wrap_chain: WrapPolicy::Chop,
                wrap_binary: WrapPolicy::Fill,
                wrap_ternary: WrapPolicy::Chop,
                wrap_clauses: WrapPolicy::Chop,
                brace_type: BraceStyle::SameLine,
                brace_method: BraceStyle::NextLine,
                brace_block: BraceStyle::SameLine,
                brace_continuation: BraceStyle::SameLine,
                force_braces: true,
                blank_lines_before_member: 1,
                ..wrap(60)
            },
            FormatStyle { use_tabs: true, continuation_indent: 4, wrap_arguments: WrapPolicy::Always, ..wrap(100) },
        ];
        for style in styles {
            let once = fmt(SAMPLE, &style);
            let twice = fmt(&once, &style);
            assert_eq!(once, twice, "style {style:?}\n--- once ---\n{once}");
        }
    }

    #[test]
    fn formatting_never_changes_the_tokens() {
        let style = FormatStyle {
            normalize_spacing: true,
            wrap_arguments: WrapPolicy::Chop,
            wrap_chain: WrapPolicy::Chop,
            ..wrap(50)
        };
        let out = fmt(SAMPLE, &style);
        let strip = |s: &str| s.split_whitespace().collect::<Vec<_>>().join("");
        // Braces were not forced here, so apart from whitespace the two texts are the same.
        assert_eq!(strip(&out), strip(SAMPLE));
    }

    // ── Ranges ───────────────────────────────────────────────────────────────────────────

    #[test]
    fn a_selection_is_formatted_alone() {
        let src = "class A {\n    void m() {\n  int a=1;\n        int b=2;\n    }\n}\n";
        let start = src.find("  int a").unwrap();
        let edits = format_range(src, start, start + 4, &d());
        assert_eq!(edits.len(), 1);
        let applied = format!("{}{}{}", &src[..edits[0].start], edits[0].replacement, &src[edits[0].end..]);
        assert!(applied.contains("\n  int a=1;\n") == false, "{applied}");
        assert!(applied.contains("int b=2;"), "{applied}");
    }
}
