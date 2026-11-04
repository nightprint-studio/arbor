//! `.editorconfig` -> formatter style keys.
//!
//! ## Design
//!
//! The caller (the backend) walks up from the Java file collecting `.editorconfig` contents until
//! one says `root = true`, and hands them over **farthest first, nearest last**. Each one is treated
//! as applying to the same project-relative path: sections are matched against it, later sections
//! and later files override earlier ones, `unset` cancels a key.
//!
//! The output is a JSON object naming **only** keys that exist in `FormatStyle`, with values of the
//! kind the style expects, so it can be laid over the defaults with `FormatStyle::layered` as is.
//! Everything this does not understand (an unknown property, a brace style such as
//! `next_line_if_wrapped`, a non-numeric size) is dropped rather than guessed at.
//!
//! ## Globs
//!
//! Implemented here, std only: `*` (no `/`), `**` (anything), `?`, `[abc]` / `[a-z]` / `[!abc]`,
//! `{a,b}` alternatives (nested too), `{1..3}` numeric ranges and `\` escapes. A section without a
//! `/` matches the file name at any depth (`*.java`); one with a `/` is anchored at the project
//! root (`src/**/*.java`).
//!
//! ## Mapping notes
//!
//! * `indent_size = tab` takes `tab_width`; with `indent_style = tab` and no `indent_size`, a numeric
//!   `tab_width` is the indent width too (the EditorConfig default).
//! * `max_line_length = off` is `line_width = 0` (never wrap for width).
//! * IntelliJ's `ij_java_class_count_to_use_import_on_demand` is the threshold for *classes* (the
//!   style's `wildcard_threshold`); `ij_java_names_count_to_use_import_on_demand` is for static
//!   *names* (`static_wildcard_threshold`).

use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// The style keys the `.editorconfig` files in `configs` set for `file_rel_path`.
pub fn editorconfig_style(file_rel_path: &str, configs: &[&str]) -> Map<String, Value> {
    let path = normalize_path(file_rel_path);
    let mut props: BTreeMap<String, String> = BTreeMap::new();
    for config in configs {
        apply_config(config, &path, &mut props);
    }
    props_to_style(&props)
}

fn normalize_path(path: &str) -> String {
    let unified = path.replace('\\', "/");
    let mut rest = unified.as_str();
    while let Some(stripped) = rest.strip_prefix("./") {
        rest = stripped;
    }
    rest.trim_start_matches('/').to_string()
}

// ── Parsing ─────────────────────────────────────────────────────────────────────────────────

/// Lay the matching sections of one file over `props` (keys and values lower-cased).
fn apply_config(config: &str, path: &str, props: &mut BTreeMap<String, String>) {
    let config = config.strip_prefix('\u{feff}').unwrap_or(config);
    let mut section_matches: Option<bool> = None; // None: before the first section
    for raw in config.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') && line.len() >= 2 {
            let glob = &line[1..line.len() - 1];
            section_matches = Some(glob_matches(glob, path));
            continue;
        }
        if section_matches != Some(true) {
            continue;
        }
        let split = line.find('=').or_else(|| line.find(':'));
        let Some(at) = split else { continue };
        let key = line[..at].trim().to_lowercase();
        let value = line[at + 1..].trim().to_lowercase();
        if !key.is_empty() {
            props.insert(key, value);
        }
    }
}

// ── Globs ───────────────────────────────────────────────────────────────────────────────────

/// Whether the section glob `glob` selects `path` (project-relative, `/`-separated).
pub(crate) fn glob_matches(glob: &str, path: &str) -> bool {
    let glob = glob.trim();
    if glob.is_empty() {
        return false;
    }
    let pattern = if has_slash_outside_class(glob) {
        glob.trim_start_matches('/').to_string()
    } else {
        format!("**/{glob}")
    };
    let text: Vec<char> = path.chars().collect();
    expand_braces(&pattern.chars().collect::<Vec<_>>())
        .iter()
        .any(|alt| match_here(&alt.chars().collect::<Vec<_>>(), &text))
}

fn has_slash_outside_class(glob: &str) -> bool {
    let mut in_class = false;
    let mut escaped = false;
    for c in glob.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' => escaped = true,
            '[' => in_class = true,
            ']' => in_class = false,
            '/' if !in_class => return true,
            _ => {}
        }
    }
    false
}

/// Expand every `{a,b}` / `{1..3}` into plain alternatives. A lone `{x}` stays literal (escaped).
fn expand_braces(chars: &[char]) -> Vec<String> {
    let mut open = None;
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '\\' => i += 1,
            '{' => {
                open = Some(i);
                break;
            }
            _ => {}
        }
        i += 1;
    }
    let Some(open) = open else { return vec![chars.iter().collect()] };

    let mut depth = 0i32;
    let mut close = None;
    let mut j = open;
    while j < chars.len() {
        match chars[j] {
            '\\' => j += 1,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(j);
                    break;
                }
            }
            _ => {}
        }
        j += 1;
    }
    let prefix: String = chars[..open].iter().collect();
    let Some(close) = close else {
        // Unbalanced: the brace is an ordinary character.
        return expand_braces(&chars[open + 1..]).into_iter().map(|rest| format!("{prefix}\\{{{rest}")).collect();
    };

    let inner = &chars[open + 1..close];
    let alternatives = split_top_level(inner);
    let mut options: Vec<String> = Vec::new();
    if alternatives.len() == 1 {
        match parse_range(inner) {
            Some(numbers) => options.extend(numbers),
            None => {
                for expanded in expand_braces(inner) {
                    options.push(format!("\\{{{expanded}\\}}"));
                }
            }
        }
    } else {
        for alternative in alternatives {
            options.extend(expand_braces(&alternative));
        }
    }

    let suffixes = expand_braces(&chars[close + 1..]);
    let mut out = Vec::with_capacity(options.len() * suffixes.len());
    for option in &options {
        for suffix in &suffixes {
            out.push(format!("{prefix}{option}{suffix}"));
        }
    }
    out
}

fn split_top_level(inner: &[char]) -> Vec<Vec<char>> {
    let mut parts = vec![Vec::new()];
    let mut depth = 0i32;
    let mut i = 0;
    while i < inner.len() {
        let c = inner[i];
        match c {
            '\\' => {
                parts.last_mut().unwrap().push(c);
                if let Some(&next) = inner.get(i + 1) {
                    parts.last_mut().unwrap().push(next);
                }
                i += 2;
                continue;
            }
            '{' => depth += 1,
            '}' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(Vec::new());
                i += 1;
                continue;
            }
            _ => {}
        }
        parts.last_mut().unwrap().push(c);
        i += 1;
    }
    parts
}

fn parse_range(inner: &[char]) -> Option<Vec<String>> {
    let text: String = inner.iter().collect();
    let (from, to) = text.split_once("..")?;
    let (from, to): (i64, i64) = (from.trim().parse().ok()?, to.trim().parse().ok()?);
    if to < from || to - from > 1000 {
        return None;
    }
    Some((from..=to).map(|n| n.to_string()).collect())
}

/// Match a brace-free pattern against `text`.
fn match_here(pattern: &[char], text: &[char]) -> bool {
    let Some(&head) = pattern.first() else { return text.is_empty() };
    match head {
        '*' if pattern.get(1) == Some(&'*') => {
            let rest = &pattern[2..];
            for k in 0..=text.len() {
                if match_here(rest, &text[k..]) {
                    return true;
                }
                // `**/` also stands for "no directory at all".
                let at_dir_start = k == 0 || text[k - 1] == '/';
                if at_dir_start && rest.first() == Some(&'/') && match_here(&rest[1..], &text[k..]) {
                    return true;
                }
            }
            false
        }
        '*' => {
            let rest = &pattern[1..];
            for k in 0..=text.len() {
                if match_here(rest, &text[k..]) {
                    return true;
                }
                if k < text.len() && text[k] == '/' {
                    break;
                }
            }
            false
        }
        '?' => !text.is_empty() && text[0] != '/' && match_here(&pattern[1..], &text[1..]),
        '\\' => match pattern.get(1) {
            Some(&escaped) => !text.is_empty() && text[0] == escaped && match_here(&pattern[2..], &text[1..]),
            None => !text.is_empty() && text[0] == '\\' && match_here(&pattern[1..], &text[1..]),
        },
        '[' => match parse_class(pattern) {
            Some((ranges, negate, len)) => {
                !text.is_empty()
                    && text[0] != '/'
                    && (ranges.iter().any(|&(lo, hi)| text[0] >= lo && text[0] <= hi) != negate)
                    && match_here(&pattern[len..], &text[1..])
            }
            None => !text.is_empty() && text[0] == '[' && match_here(&pattern[1..], &text[1..]),
        },
        literal => !text.is_empty() && text[0] == literal && match_here(&pattern[1..], &text[1..]),
    }
}

/// `[abc]`, `[a-z]`, `[!abc]` at the start of `pattern`: the ranges, whether it is negated, and how
/// many pattern characters it spans. `None` when there is no closing `]`.
fn parse_class(pattern: &[char]) -> Option<(Vec<(char, char)>, bool, usize)> {
    let mut i = 1;
    let negate = matches!(pattern.get(i), Some(&'!') | Some(&'^'));
    if negate {
        i += 1;
    }
    let body_start = i;
    let mut ranges = Vec::new();
    while i < pattern.len() {
        let c = pattern[i];
        if c == ']' && i > body_start {
            return Some((ranges, negate, i + 1));
        }
        if pattern.get(i + 1) == Some(&'-') && pattern.get(i + 2).is_some_and(|&hi| hi != ']') {
            ranges.push((c, pattern[i + 2]));
            i += 3;
        } else {
            ranges.push((c, c));
            i += 1;
        }
    }
    None
}

// ── Mapping ─────────────────────────────────────────────────────────────────────────────────

fn props_to_style(props: &BTreeMap<String, String>) -> Map<String, Value> {
    let get = |key: &str| props.get(key).map(String::as_str).filter(|v| *v != "unset");
    let number = |key: &str| get(key).and_then(|v| v.parse::<u64>().ok());
    let flag = |key: &str| match get(key) {
        Some("true") => Some(true),
        Some("false") => Some(false),
        _ => None,
    };
    let mut style = Map::new();
    let mut set = |key: &str, value: Value| {
        style.insert(key.to_string(), value);
    };

    match get("indent_style") {
        Some("tab") => set("use_tabs", Value::Bool(true)),
        Some("space") => set("use_tabs", Value::Bool(false)),
        _ => {}
    }

    let tab_width = number("tab_width").filter(|n| *n > 0);
    match get("indent_size") {
        Some("tab") => {
            if let Some(width) = tab_width {
                set("indent_width", Value::from(width));
            }
        }
        Some(other) => {
            if let Some(width) = other.parse::<u64>().ok().filter(|n| *n > 0) {
                set("indent_width", Value::from(width));
            }
        }
        None => {
            if get("indent_style") == Some("tab") {
                if let Some(width) = tab_width {
                    set("indent_width", Value::from(width));
                }
            }
        }
    }

    match get("max_line_length") {
        Some("off") => set("line_width", Value::from(0u64)),
        Some(other) => {
            if let Ok(width) = other.parse::<u64>() {
                set("line_width", Value::from(width));
            }
        }
        None => {}
    }

    match get("end_of_line") {
        Some("lf") => set("line_ending", Value::from("lf")),
        Some("crlf") => set("line_ending", Value::from("crlf")),
        _ => {}
    }
    if let Some(final_newline) = flag("insert_final_newline") {
        set("ensure_final_newline", Value::Bool(final_newline));
    }

    if let Some(n) = number("ij_java_continuation_indent_size") {
        set("continuation_indent", Value::from(n));
    }

    for (property, key) in [
        ("ij_java_class_brace_style", "brace_type"),
        ("ij_java_method_brace_style", "brace_method"),
        ("ij_java_brace_style", "brace_block"),
    ] {
        match get(property) {
            Some("end_of_line") => set(key, Value::from("same_line")),
            Some("next_line") => set(key, Value::from("next_line")),
            _ => {}
        }
    }

    if let Some(n) = number("ij_java_class_count_to_use_import_on_demand") {
        set("wildcard_threshold", Value::from(n));
    }
    if let Some(n) = number("ij_java_names_count_to_use_import_on_demand") {
        set("static_wildcard_threshold", Value::from(n));
    }
    if let Some(n) = number("ij_java_blank_lines_around_method") {
        set("blank_lines_before_member", Value::from(n));
    }
    if let Some(n) = number("ij_java_keep_blank_lines_in_code") {
        set("max_blank_lines", Value::from(n));
    }

    // The control keywords share one style key; the first one the file mentions decides.
    let control = [
        "ij_java_space_before_if_parentheses",
        "ij_java_space_before_for_parentheses",
        "ij_java_space_before_while_parentheses",
        "ij_java_space_before_switch_parentheses",
        "ij_java_space_before_catch_parentheses",
        "ij_java_space_before_synchronized_parentheses",
    ]
    .iter()
    .find_map(|key| flag(*key));
    if let Some(space) = control {
        set("space_before_control_paren", Value::Bool(space));
    }
    if let Some(space) = flag("ij_java_space_after_comma") {
        set("space_after_comma", Value::Bool(space));
    }
    if let Some(space) = flag("ij_java_space_after_type_cast") {
        set("space_after_cast", Value::Bool(space));
    }

    style
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::style::{BraceStyle, FormatStyle};
    use serde_json::json;

    const BASIC: &str = "root = true\n\n[*]\nindent_style = space\nindent_size = 2\nmax_line_length = 100\nend_of_line = lf\ninsert_final_newline = true\n\n[*.java]\nindent_size = 4\n";

    #[test]
    fn basic_mapping_and_section_override() {
        let m = editorconfig_style("src/main/Foo.java", &[BASIC]);
        assert_eq!(m.get("use_tabs"), Some(&json!(false)));
        assert_eq!(m.get("indent_width"), Some(&json!(4)));
        assert_eq!(m.get("line_width"), Some(&json!(100)));
        assert_eq!(m.get("line_ending"), Some(&json!("lf")));
        assert_eq!(m.get("ensure_final_newline"), Some(&json!(true)));
    }

    #[test]
    fn a_section_that_does_not_match_contributes_nothing() {
        let m = editorconfig_style("Foo.xml", &["[*.java]\nindent_size = 8\n"]);
        assert!(m.is_empty());
    }

    #[test]
    fn nearer_files_override_farther_ones() {
        let far = "[*]\nindent_size = 2\nindent_style = space\n";
        let near = "[*.java]\nindent_size = 8\nindent_style = tab\n";
        let m = editorconfig_style("a/B.java", &[far, near]);
        assert_eq!(m.get("indent_width"), Some(&json!(8)));
        assert_eq!(m.get("use_tabs"), Some(&json!(true)));
        let swapped = editorconfig_style("a/B.java", &[near, far]);
        assert_eq!(swapped.get("indent_width"), Some(&json!(2)));
    }

    #[test]
    fn tab_indent_uses_tab_width() {
        let m = editorconfig_style("B.java", &["[*]\nindent_style = tab\nindent_size = tab\ntab_width = 4\n"]);
        assert_eq!(m.get("use_tabs"), Some(&json!(true)));
        assert_eq!(m.get("indent_width"), Some(&json!(4)));
        let implicit = editorconfig_style("B.java", &["[*]\nindent_style = tab\ntab_width = 8\n"]);
        assert_eq!(implicit.get("indent_width"), Some(&json!(8)));
        let no_width = editorconfig_style("B.java", &["[*]\nindent_size = tab\n"]);
        assert!(!no_width.contains_key("indent_width"));
    }

    #[test]
    fn unset_removes_a_key() {
        let cfg = "[*]\nindent_size = 2\nmax_line_length = 80\n[*.java]\nindent_size = unset\nmax_line_length = off\n";
        let m = editorconfig_style("A.java", &[cfg]);
        assert!(!m.contains_key("indent_width"));
        assert_eq!(m.get("line_width"), Some(&json!(0)));
    }

    #[test]
    fn values_and_keys_are_case_insensitive() {
        let m = editorconfig_style("A.java", &["[*]\nIndent_Style = TAB\nEND_OF_LINE = CRLF\n"]);
        assert_eq!(m.get("use_tabs"), Some(&json!(true)));
        assert_eq!(m.get("line_ending"), Some(&json!("crlf")));
    }

    #[test]
    fn unsupported_values_are_ignored() {
        let cfg = "[*]\nend_of_line = cr\nindent_size = lots\nij_java_class_brace_style = next_line_if_wrapped\nmax_line_length = wide\n";
        assert!(editorconfig_style("A.java", &[cfg]).is_empty());
    }

    #[test]
    fn comments_and_blank_lines_are_skipped() {
        let cfg = "# top\n; also a comment\n\n[*]\n# inner\nindent_size = 3\n";
        assert_eq!(editorconfig_style("A.java", &[cfg]).get("indent_width"), Some(&json!(3)));
    }

    #[test]
    fn intellij_keys() {
        let cfg = "[*.java]\n\
                   ij_java_continuation_indent_size = 4\n\
                   ij_java_class_brace_style = next_line\n\
                   ij_java_method_brace_style = end_of_line\n\
                   ij_java_brace_style = end_of_line\n\
                   ij_java_class_count_to_use_import_on_demand = 5\n\
                   ij_java_names_count_to_use_import_on_demand = 3\n\
                   ij_java_blank_lines_around_method = 2\n\
                   ij_java_keep_blank_lines_in_code = 1\n\
                   ij_java_space_before_if_parentheses = false\n\
                   ij_java_space_after_comma = true\n\
                   ij_java_space_after_type_cast = false\n\
                   ij_java_wrap_long_lines = true\n";
        let m = editorconfig_style("x/Y.java", &[cfg]);
        assert_eq!(m.get("continuation_indent"), Some(&json!(4)));
        assert_eq!(m.get("brace_type"), Some(&json!("next_line")));
        assert_eq!(m.get("brace_method"), Some(&json!("same_line")));
        assert_eq!(m.get("brace_block"), Some(&json!("same_line")));
        assert_eq!(m.get("wildcard_threshold"), Some(&json!(5)));
        assert_eq!(m.get("static_wildcard_threshold"), Some(&json!(3)));
        assert_eq!(m.get("blank_lines_before_member"), Some(&json!(2)));
        assert_eq!(m.get("max_blank_lines"), Some(&json!(1)));
        assert_eq!(m.get("space_before_control_paren"), Some(&json!(false)));
        assert_eq!(m.get("space_after_comma"), Some(&json!(true)));
        assert_eq!(m.get("space_after_cast"), Some(&json!(false)));
        assert_eq!(m.len(), 11, "wrap_long_lines must be ignored: {m:?}");
    }

    #[test]
    fn every_emitted_key_is_a_real_style_key_of_the_right_kind() {
        let cfg = "[*]\nindent_style=tab\ntab_width=4\nindent_size=tab\nmax_line_length=120\nend_of_line=crlf\n\
                   insert_final_newline=false\nij_java_continuation_indent_size=8\nij_java_class_brace_style=next_line\n\
                   ij_java_method_brace_style=next_line\nij_java_brace_style=end_of_line\n\
                   ij_java_class_count_to_use_import_on_demand=99\nij_java_names_count_to_use_import_on_demand=99\n\
                   ij_java_blank_lines_around_method=1\nij_java_keep_blank_lines_in_code=2\n\
                   ij_java_space_before_while_parentheses=true\nij_java_space_after_comma=false\nij_java_space_after_type_cast=true\n";
        let m = editorconfig_style("A.java", &[cfg]);
        let known = FormatStyle::default().to_map();
        for (key, value) in &m {
            let default = known.get(key).unwrap_or_else(|| panic!("unknown key {key}"));
            assert_eq!(
                std::mem::discriminant(default),
                std::mem::discriminant(value),
                "wrong kind for {key}: {value:?}"
            );
        }
        let style = FormatStyle::layered(&[&m]);
        assert!(style.use_tabs);
        assert_eq!(style.indent_width, 4);
        assert_eq!(style.brace_type, BraceStyle::NextLine);
        assert_eq!(style.brace_block, BraceStyle::SameLine);
        assert!(!style.ensure_final_newline);
    }

    #[test]
    fn globs_without_a_slash_match_at_any_depth() {
        assert!(glob_matches("*.java", "Foo.java"));
        assert!(glob_matches("*.java", "a/b/Foo.java"));
        assert!(!glob_matches("*.java", "Foo.kt"));
        assert!(glob_matches("Foo.java", "src/Foo.java"));
        assert!(!glob_matches("Foo.java", "src/XFoo.java"));
    }

    #[test]
    fn globs_with_a_slash_are_anchored() {
        assert!(glob_matches("src/**/*.java", "src/Foo.java"));
        assert!(glob_matches("src/**/*.java", "src/a/b/Foo.java"));
        assert!(!glob_matches("src/**/*.java", "test/Foo.java"));
        assert!(!glob_matches("src/**/*.java", "x/src/Foo.java"));
        assert!(glob_matches("/src/*.java", "src/Foo.java"));
        assert!(!glob_matches("src/*.java", "src/a/Foo.java"));
        assert!(glob_matches("src/**", "src/a/b/c.txt"));
    }

    #[test]
    fn globs_support_alternatives_classes_and_wildcards() {
        assert!(glob_matches("*.{java,kt}", "A.java"));
        assert!(glob_matches("*.{java,kt}", "A.kt"));
        assert!(!glob_matches("*.{java,kt}", "A.xml"));
        assert!(glob_matches("{src,test}/**/*.java", "test/a/B.java"));
        assert!(glob_matches("Foo?.java", "Foo1.java"));
        assert!(!glob_matches("Foo?.java", "Foo.java"));
        assert!(glob_matches("[AB]*.java", "Axx.java"));
        assert!(!glob_matches("[AB]*.java", "Cxx.java"));
        assert!(glob_matches("[!AB]*.java", "Cxx.java"));
        assert!(glob_matches("[a-c]1.java", "b1.java"));
        assert!(glob_matches("File{1..3}.java", "File2.java"));
        assert!(!glob_matches("File{1..3}.java", "File4.java"));
        assert!(glob_matches("{a,{b,c}}.java", "c.java"));
        assert!(glob_matches("*", "anything/at/all.java"));
    }

    #[test]
    fn the_path_is_normalised() {
        let m = editorconfig_style(".\\src\\Foo.java", &["[src/*.java]\nindent_size = 5\n"]);
        assert_eq!(m.get("indent_width"), Some(&json!(5)));
    }

    #[test]
    fn a_bom_and_properties_before_any_section_are_harmless() {
        let cfg = "\u{feff}root = true\nindent_size = 9\n[*]\nindent_size = 2\n";
        assert_eq!(editorconfig_style("A.java", &[cfg]).get("indent_width"), Some(&json!(2)));
    }
}
