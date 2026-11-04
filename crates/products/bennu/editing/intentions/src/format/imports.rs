//! Import organisation for the formatter: one edit over the whole import block.
//!
//! ## Design
//!
//! A plain text scanner, no parse tree. The *block* is every `import ...;` line from the first one
//! to the last one, blank lines between them allowed. It ends at the first line that is neither an
//! import, a blank line, nor a comment; a comment (or a trailing comment on an import line, or two
//! imports on one line) **inside** the block makes the whole function bail with `None`, exactly like
//! `be/src/optimize_imports.rs`: a comment cannot follow "its" import once the order changes, and
//! moving it somewhere meaningless is worse than not formatting.
//!
//! ## What it does and does not do
//!
//! * **Never removes an unused import** and never adds one — only exact duplicates collapse.
//! * Static imports are placed per `style.static_imports` (`First` / `Last` as one block, `Mixed`
//!   sorted in among the ordinary ones).
//! * Ordinary imports are grouped by `style.import_groups`. A prefix matches on a package boundary
//!   (`java` matches `java.util.X`, not `javax.x` nor `javafoo.x`); the longest matching prefix wins;
//!   `*` is "everything else" (when absent, unmatched imports form a last group). The `static`
//!   entry is ignored: the static block position is `style.static_imports`' job.
//! * Inside a group the order is segment by segment, bytewise (`java.util.List` before
//!   `java.util.concurrent.X`: uppercase sorts before lowercase, and `*` before both).
//! * Wildcard collapse (`wildcard_threshold` / `static_wildcard_threshold`): enough single-level
//!   imports from one package (or one class's static members) become `pkg.*`; an existing `pkg.*`
//!   simply absorbs them.
//!
//! The edit starts at the first import line's first byte and ends at the last import line's last
//! content byte (its line terminator is **not** part of the edit), so applying it twice is a no-op:
//! the second run renders the same text and returns `None`. The file's own line ending is kept.

use super::style::{FormatStyle, StaticImports};
use crate::Edit;
use std::collections::{BTreeSet, HashMap, HashSet};

/// One parsed `import` statement.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Import {
    is_static: bool,
    /// Dotted path without the keyword, whitespace or `;` (`java.util.*`, `org.x.Util.helper`).
    path: String,
}

impl Import {
    fn is_wildcard(&self) -> bool {
        self.path.ends_with(".*")
    }

    /// The package (or class, for statics) the import comes from; `""` when it has no dot.
    fn owner(&self) -> &str {
        let base = self.path.strip_suffix(".*").unwrap_or(&self.path);
        if self.is_wildcard() {
            base
        } else {
            base.rsplit_once('.').map(|(owner, _)| owner).unwrap_or("")
        }
    }

    fn render(&self) -> String {
        if self.is_static {
            format!("import static {};", self.path)
        } else {
            format!("import {};", self.path)
        }
    }

    fn segments(&self) -> Vec<&str> {
        self.path.split('.').collect()
    }
}

/// The import block of a file: its byte range and the statements in it.
struct Block {
    start: usize,
    end: usize,
    imports: Vec<Import>,
}

/// Sort, group, dedupe and (optionally) collapse the imports of `source`.
///
/// `None` when `style.organize_imports` is off, when there is no import block, when something
/// unsafe to move sits inside it, or when it already reads exactly as it would be written.
pub fn organize_imports(source: &str, style: &FormatStyle) -> Option<Edit> {
    if !style.organize_imports {
        return None;
    }
    let block = find_block(source)?;
    let newline = if source.contains("\r\n") { "\r\n" } else { "\n" };
    let text = render(block.imports, style, newline);
    if source.get(block.start..block.end) == Some(text.as_str()) {
        return None;
    }
    Some(Edit { start: block.start, end: block.end, replacement: text })
}

// ── Locating the block ──────────────────────────────────────────────────────────────────────

fn find_block(source: &str) -> Option<Block> {
    let mut offset = 0usize;
    let mut in_block_comment = false;
    let mut first_start: Option<usize> = None;
    let mut last_end = 0usize;
    let mut saw_comment_in_block = false;
    let mut imports: Vec<Import> = Vec::new();

    for raw in source.split_inclusive('\n') {
        let line_start = offset;
        offset += raw.len();
        let content = raw.strip_suffix('\n').unwrap_or(raw);
        let content = content.strip_suffix('\r').unwrap_or(content);
        let line_end = line_start + content.len();
        let trimmed = content.trim();

        if in_block_comment {
            if trimmed.contains("*/") {
                in_block_comment = false;
            }
            if first_start.is_some() {
                saw_comment_in_block = true;
            }
            continue;
        }
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("//") {
            if first_start.is_some() {
                saw_comment_in_block = true;
            }
            continue;
        }
        if trimmed.starts_with("/*") {
            in_block_comment = !trimmed[2..].contains("*/");
            if first_start.is_some() {
                saw_comment_in_block = true;
            }
            continue;
        }
        if first_start.is_none() && is_package_line(trimmed) {
            continue;
        }
        match parse_import(trimmed) {
            None => break, // a type, an annotation: the header is over
            Some(None) => return None, // an import we cannot safely move
            Some(Some(import)) => {
                if first_start.is_none() {
                    first_start = Some(line_start);
                } else if saw_comment_in_block {
                    return None;
                }
                last_end = line_end;
                imports.push(import);
            }
        }
    }

    let start = first_start?;
    Some(Block { start, end: last_end, imports })
}

fn is_package_line(trimmed: &str) -> bool {
    trimmed
        .strip_prefix("package")
        .is_some_and(|rest| rest.chars().next().map_or(true, |c| c.is_whitespace() || c == ';'))
}

/// `None`: not an import line. `Some(None)`: an import line we refuse to rewrite (no `;`, a second
/// statement or a trailing comment after it). `Some(Some(_))`: parsed.
fn parse_import(trimmed: &str) -> Option<Option<Import>> {
    let rest = trimmed.strip_prefix("import")?;
    if !rest.chars().next().is_some_and(|c| c.is_whitespace()) {
        return None;
    }
    let mut rest = rest.trim();
    let mut is_static = false;
    if let Some(after) = rest.strip_prefix("static") {
        if after.chars().next().is_some_and(|c| c.is_whitespace()) {
            is_static = true;
            rest = after.trim();
        }
    }
    let Some(semi) = rest.find(';') else { return Some(None) };
    if !rest[semi + 1..].trim().is_empty() {
        return Some(None);
    }
    let path: String = rest[..semi].split_whitespace().collect();
    if path.is_empty() {
        return Some(None);
    }
    Some(Some(Import { is_static, path }))
}

// ── Rendering ───────────────────────────────────────────────────────────────────────────────

fn render(imports: Vec<Import>, style: &FormatStyle, newline: &str) -> String {
    let mut imports = dedupe(imports);
    imports = collapse(imports, false, style.wildcard_threshold);
    imports = collapse(imports, true, style.static_wildcard_threshold);
    imports = dedupe(imports);

    let mixed = style.static_imports == StaticImports::Mixed;
    let (statics, ordinary): (Vec<Import>, Vec<Import>) = if mixed {
        (Vec::new(), imports)
    } else {
        imports.into_iter().partition(|i| i.is_static)
    };

    let groups = group_by_prefix(ordinary, &style.import_groups);
    let static_block = sorted(statics);

    let mut blocks: Vec<Vec<Import>> = Vec::new();
    match style.static_imports {
        StaticImports::First => {
            blocks.push(static_block);
            blocks.extend(groups);
        }
        StaticImports::Last => {
            blocks.extend(groups);
            blocks.push(static_block);
        }
        StaticImports::Mixed => blocks.extend(groups),
    }
    blocks.retain(|b| !b.is_empty());

    let separator = if style.blank_line_between_import_groups {
        format!("{newline}{newline}")
    } else {
        newline.to_string()
    };
    blocks
        .iter()
        .map(|block| block.iter().map(Import::render).collect::<Vec<_>>().join(newline))
        .collect::<Vec<_>>()
        .join(&separator)
}

fn dedupe(imports: Vec<Import>) -> Vec<Import> {
    let mut seen = HashSet::new();
    imports.into_iter().filter(|i| seen.insert(i.clone())).collect()
}

/// Replace `threshold` or more single imports of one owner by `owner.*`.
fn collapse(imports: Vec<Import>, statics: bool, threshold: usize) -> Vec<Import> {
    if threshold == 0 {
        return imports;
    }
    let mut counts: HashMap<String, usize> = HashMap::new();
    for i in imports.iter().filter(|i| i.is_static == statics && !i.is_wildcard()) {
        if !i.owner().is_empty() {
            *counts.entry(i.owner().to_string()).or_insert(0) += 1;
        }
    }
    let collapsing: BTreeSet<String> =
        counts.into_iter().filter(|(_, n)| *n >= threshold).map(|(owner, _)| owner).collect();
    if collapsing.is_empty() {
        return imports;
    }
    let mut out: Vec<Import> = imports
        .into_iter()
        .filter(|i| i.is_static != statics || i.is_wildcard() || !collapsing.contains(i.owner()))
        .collect();
    out.extend(collapsing.into_iter().map(|owner| Import { is_static: statics, path: format!("{owner}.*") }));
    out
}

fn sorted(mut imports: Vec<Import>) -> Vec<Import> {
    imports.sort_by(|a, b| a.segments().cmp(&b.segments()).then(a.is_static.cmp(&b.is_static)));
    imports
}

/// Distribute `imports` over the configured groups, each sorted. One `Vec` per group, in order,
/// with a trailing one for imports no group claims (only used when there is no `*` entry).
fn group_by_prefix(imports: Vec<Import>, groups: &[String]) -> Vec<Vec<Import>> {
    let mut buckets: Vec<Vec<Import>> = vec![Vec::new(); groups.len() + 1];
    for import in imports {
        let index = group_index(&import.path, groups);
        buckets[index].push(import);
    }
    buckets.into_iter().map(sorted).collect()
}

fn group_index(path: &str, groups: &[String]) -> usize {
    let mut best: Option<(usize, usize)> = None; // (prefix length, group index)
    let mut star: Option<usize> = None;
    for (index, group) in groups.iter().enumerate() {
        let prefix = group.trim().trim_end_matches('.');
        match prefix {
            "*" => {
                star.get_or_insert(index);
            }
            "" | "static" => {}
            _ => {
                let matches = path == prefix
                    || (path.starts_with(prefix) && path[prefix.len()..].starts_with('.'));
                if matches && best.map_or(true, |(len, _)| prefix.len() > len) {
                    best = Some((prefix.len(), index));
                }
            }
        }
    }
    best.map(|(_, i)| i).or(star).unwrap_or(groups.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style() -> FormatStyle {
        FormatStyle { organize_imports: true, ..FormatStyle::default() }
    }

    fn run(src: &str, st: &FormatStyle) -> Option<String> {
        let edit = organize_imports(src, st)?;
        let mut out = src.to_string();
        out.replace_range(edit.start..edit.end, &edit.replacement);
        Some(out)
    }

    fn assert_idempotent(src: &str, st: &FormatStyle) {
        if let Some(once) = run(src, st) {
            assert_eq!(run(&once, st), None, "second run changed:\n{once:?}");
        }
    }

    #[test]
    fn groups_statics_first_and_sorts_inside_groups() {
        let src = "package p;\n\nimport org.x.B;\nimport java.util.List;\nimport static a.B.c;\nimport javax.x.Y;\nimport java.io.File;\n\nclass C {}\n";
        let out = run(src, &style()).expect("changed");
        assert_eq!(
            out,
            "package p;\n\nimport static a.B.c;\n\nimport java.io.File;\nimport java.util.List;\n\nimport javax.x.Y;\n\nimport org.x.B;\n\nclass C {}\n"
        );
        assert_idempotent(src, &style());
    }

    #[test]
    fn the_edit_range_is_exact() {
        let src = "package p;\n\nimport org.x.B;\nimport java.util.List;\n\nclass C {}\n";
        let edit = organize_imports(src, &style()).unwrap();
        assert_eq!(&src[edit.start..edit.end], "import org.x.B;\nimport java.util.List;");
        assert_eq!(edit.replacement, "import java.util.List;\n\nimport org.x.B;");
    }

    #[test]
    fn statics_last() {
        let st = FormatStyle { static_imports: StaticImports::Last, ..style() };
        let src = "import static a.B.c;\nimport org.x.B;\nimport java.util.List;\n\nclass C {}\n";
        assert_eq!(
            run(src, &st).unwrap(),
            "import java.util.List;\n\nimport org.x.B;\n\nimport static a.B.c;\n\nclass C {}\n"
        );
        assert_idempotent(src, &st);
    }

    #[test]
    fn statics_mixed_are_sorted_among_the_others() {
        let st = FormatStyle { static_imports: StaticImports::Mixed, ..style() };
        let src = "import org.x.B;\nimport static a.B.c;\nimport java.util.List;\n\nclass C {}\n";
        assert_eq!(
            run(src, &st).unwrap(),
            "import java.util.List;\n\nimport static a.B.c;\nimport org.x.B;\n\nclass C {}\n"
        );
        assert_idempotent(src, &st);
    }

    #[test]
    fn no_blank_line_between_groups_when_disabled() {
        let st = FormatStyle { blank_line_between_import_groups: false, ..style() };
        let src = "import org.x.B;\nimport java.io.File;\n\nclass C {}\n";
        assert_eq!(run(src, &st).unwrap(), "import java.io.File;\nimport org.x.B;\n\nclass C {}\n");
    }

    #[test]
    fn a_prefix_matches_on_a_package_boundary() {
        let st = FormatStyle { import_groups: vec!["java".into(), "*".into()], ..style() };
        let src = "import javax.x.Y;\nimport java.util.L;\nimport javafoo.Z;\n\nclass C {}\n";
        assert_eq!(
            run(src, &st).unwrap(),
            "import java.util.L;\n\nimport javafoo.Z;\nimport javax.x.Y;\n\nclass C {}\n"
        );
    }

    #[test]
    fn the_longest_prefix_wins() {
        let st = FormatStyle {
            import_groups: vec!["org".into(), "org.springframework".into(), "*".into()],
            ..style()
        };
        let src = "import org.springframework.A;\nimport org.apache.B;\nimport com.x.C;\n\nclass C {}\n";
        assert_eq!(
            run(src, &st).unwrap(),
            "import org.apache.B;\n\nimport org.springframework.A;\n\nimport com.x.C;\n\nclass C {}\n"
        );
    }

    #[test]
    fn without_a_star_group_the_unmatched_go_last() {
        let st = FormatStyle { import_groups: vec!["java".into()], ..style() };
        let src = "import org.x.B;\nimport java.io.File;\n\nclass C {}\n";
        assert_eq!(run(src, &st).unwrap(), "import java.io.File;\n\nimport org.x.B;\n\nclass C {}\n");
    }

    #[test]
    fn wildcards_collapse_at_the_threshold_but_not_across_packages() {
        let st = FormatStyle { wildcard_threshold: 3, ..style() };
        let src = "import java.util.List;\nimport java.util.Map;\nimport java.util.Set;\nimport java.util.concurrent.Callable;\nimport org.x.B;\n\nclass C {}\n";
        assert_eq!(
            run(src, &st).unwrap(),
            "import java.util.*;\nimport java.util.concurrent.Callable;\n\nimport org.x.B;\n\nclass C {}\n"
        );
        assert_idempotent(src, &st);
    }

    #[test]
    fn below_the_threshold_nothing_collapses() {
        let st = FormatStyle { wildcard_threshold: 3, ..style() };
        let src = "import java.util.Map;\nimport java.util.List;\n\nclass C {}\n";
        assert_eq!(run(src, &st).unwrap(), "import java.util.List;\nimport java.util.Map;\n\nclass C {}\n");
    }

    #[test]
    fn an_existing_wildcard_absorbs_the_specifics() {
        let st = FormatStyle { wildcard_threshold: 2, ..style() };
        let src = "package p;\n\nimport java.util.*;\nimport java.util.List;\nimport java.util.Map;\n\nclass C {}\n";
        assert_eq!(run(src, &st).unwrap(), "package p;\n\nimport java.util.*;\n\nclass C {}\n");
    }

    #[test]
    fn static_members_of_one_class_collapse() {
        let st = FormatStyle { static_wildcard_threshold: 3, ..style() };
        let src = "import static org.junit.Assert.assertTrue;\nimport static org.junit.Assert.assertEquals;\nimport static org.junit.Assert.fail;\nimport static a.B.c;\n\nclass C {}\n";
        assert_eq!(
            run(src, &st).unwrap(),
            "import static a.B.c;\nimport static org.junit.Assert.*;\n\nclass C {}\n"
        );
        assert_idempotent(src, &st);
    }

    #[test]
    fn exact_duplicates_collapse_but_nothing_else_is_removed() {
        let src = "import java.util.List;\nimport java.util.List;\nimport org.never.Used;\n\nclass C {}\n";
        assert_eq!(
            run(src, &style()).unwrap(),
            "import java.util.List;\n\nimport org.never.Used;\n\nclass C {}\n"
        );
    }

    #[test]
    fn an_already_organised_block_is_left_alone() {
        let src = "package p;\n\nimport java.util.List;\n\nimport org.x.B;\n\nclass C {}\n";
        assert_eq!(organize_imports(src, &style()), None);
    }

    #[test]
    fn off_means_off() {
        let src = "import org.x.B;\nimport java.util.List;\n\nclass C {}\n";
        assert_eq!(organize_imports(src, &FormatStyle::default()), None);
    }

    #[test]
    fn no_imports_means_none() {
        assert_eq!(organize_imports("package p;\n\nclass C {}\n", &style()), None);
        assert_eq!(organize_imports("", &style()), None);
    }

    #[test]
    fn a_comment_inside_the_block_bails() {
        let line = "import org.x.B;\n// keep me\nimport java.util.List;\n\nclass C {}\n";
        assert_eq!(organize_imports(line, &style()), None);
        let block = "import org.x.B;\n/* keep me */\nimport java.util.List;\n\nclass C {}\n";
        assert_eq!(organize_imports(block, &style()), None);
    }

    #[test]
    fn a_trailing_comment_on_an_import_bails() {
        let src = "import org.x.B; // why\nimport java.util.List;\n\nclass C {}\n";
        assert_eq!(organize_imports(src, &style()), None);
    }

    #[test]
    fn comments_around_the_block_are_not_inside_it() {
        let src = "/* license */\npackage p;\n// first\nimport org.x.B;\nimport java.util.List;\n\n/** Doc. */\nclass C {}\n";
        let out = run(src, &style()).unwrap();
        assert_eq!(
            out,
            "/* license */\npackage p;\n// first\nimport java.util.List;\n\nimport org.x.B;\n\n/** Doc. */\nclass C {}\n"
        );
    }

    #[test]
    fn blank_lines_inside_the_block_are_normalised() {
        let src = "import org.x.B;\n\n\nimport java.util.List;\n\nclass C {}\n";
        assert_eq!(run(src, &style()).unwrap(), "import java.util.List;\n\nimport org.x.B;\n\nclass C {}\n");
    }

    #[test]
    fn crlf_is_preserved_and_idempotent() {
        let src = "package p;\r\n\r\nimport org.x.B;\r\nimport java.util.List;\r\nimport static a.B.c;\r\n\r\nclass C {}\r\n";
        let out = run(src, &style()).unwrap();
        assert_eq!(
            out,
            "package p;\r\n\r\nimport static a.B.c;\r\n\r\nimport java.util.List;\r\n\r\nimport org.x.B;\r\n\r\nclass C {}\r\n"
        );
        assert!(!out.replace("\r\n", "").contains('\n'), "a bare LF crept in");
        assert_eq!(run(&out, &style()), None);
    }

    #[test]
    fn a_file_without_a_trailing_newline_after_the_imports_works() {
        let src = "import org.x.B;\nimport java.util.List;";
        assert_eq!(run(src, &style()).unwrap(), "import java.util.List;\n\nimport org.x.B;");
    }

    #[test]
    fn idempotent_on_a_mixed_bag() {
        let st = FormatStyle { wildcard_threshold: 2, static_wildcard_threshold: 2, ..style() };
        let src = "package p;\n\nimport static org.junit.Assert.a;\nimport static org.junit.Assert.b;\nimport java.util.List;\nimport java.util.Map;\nimport javax.x.Y;\nimport com.z.Q;\nimport java.util.List;\n\npublic class C {}\n";
        assert_idempotent(src, &st);
        for static_imports in [StaticImports::First, StaticImports::Last, StaticImports::Mixed] {
            assert_idempotent(src, &FormatStyle { static_imports, ..st.clone() });
        }
    }

    #[test]
    fn import_lookalikes_end_the_header() {
        // `important` is not an import; it is code, so the block ends before it.
        let src = "import org.x.B;\nimport java.util.List;\nimportant();\n";
        let out = run(src, &style()).unwrap();
        assert_eq!(out, "import java.util.List;\n\nimport org.x.B;\nimportant();\n");
    }
}
