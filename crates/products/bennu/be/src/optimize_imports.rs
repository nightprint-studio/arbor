//! `optimize_imports` domain — `bennu_optimize_imports`: drop what the file does not use, and put
//! what is left in order. One edit over the whole import block.
//!
//! ## Why this reads the checker rather than deciding for itself
//!
//! "Is this import unused" already has an answer in the product — `bennu-check`'s `unused-import`
//! warning, which is deliberately conservative (a name mentioned in a Javadoc counts as used; a
//! `static` or wildcard import is never judged). A command that removed an import the squiggle does
//! not flag, or left one it does, would be two engines disagreeing about the same file, and each
//! would look right on its own. So the inventory comes from there and this only assembles text.
//!
//! ## What it will not do
//!
//! * **Never collapse to a wildcard.** IntelliJ folds five imports from one package into `a.b.*` by
//!   default; that can change what a simple name resolves to when two packages export it, and a
//!   reformat command has no business changing meaning.
//! * **Never add an import.** Optimize is about what is there. The missing one is Alt+Enter's job.
//! * **Never touch a `static` or wildcard import's presence** — only its position.
//!
//! The layout is IntelliJ's Java default: everything else, then `javax` and `java`, then the static
//! imports, alphabetical within each group and a blank line between the groups.

use bennu_core::prelude::BennuState;
use bennu_proto::prelude::SourceEdit;
use serde::Deserialize;

/// Args for [`bennu_optimize_imports`].
#[derive(Deserialize)]
pub struct OptimizeArgs {
    pub file: String,
    pub source: String,
}

/// Rewrite the import block. Empty when the file is not Java, has no imports, or is already in
/// order — "already optimized" and "nothing to do" are the same answer, and the editor says so.
#[arbor_rpc::handler]
fn bennu_optimize_imports(
    _ctx: &BennuState,
    args: OptimizeArgs,
) -> Result<Vec<SourceEdit>, String> {
    if !crate::intel::is_java_file(&args.file) {
        return Ok(Vec::new());
    }
    Ok(optimize(&args.source)
        .map(|(start, end, text)| {
            vec![SourceEdit {
                file: args.file.clone(),
                start,
                end,
                new_text: text,
            }]
        })
        .unwrap_or_default())
}

/// The replacement for the import block: `(start, end, text)`, or `None` when there is nothing to
/// change.
fn optimize(source: &str) -> Option<(usize, usize, String)> {
    let tree = bennu_java::prelude::parse_java(source)?;
    let inventory = bennu_check::prelude::import_inventory(tree.root_node(), source);
    if inventory.is_empty() {
        return None;
    }
    // The block is everything from the first import to the last, INCLUDING whatever sits between
    // them — blank lines, and any comment written among the imports. That is the honest cost of
    // reordering: a comment cannot follow the import it was written above once the order changes,
    // and silently moving it somewhere it no longer means anything is worse than saying so. A file
    // with a comment inside its import block is therefore left alone.
    let start = inventory.first()?.start;
    let end = inventory.last()?.end;
    if has_comment_between(source, start, end) {
        return None;
    }

    let kept: Vec<&bennu_check::prelude::ImportEntry> =
        inventory.iter().filter(|i| i.used).collect();
    // Two spellings of one import are one import — the checker flags the second as a duplicate, and
    // removing it here is the same judgement.
    let mut rendered: Vec<(u8, String)> = Vec::new();
    for entry in kept {
        let line = if entry.static_ {
            format!("import static {};", entry.path)
        } else {
            format!("import {};", entry.path)
        };
        if rendered.iter().any(|(_, l)| *l == line) {
            continue;
        }
        rendered.push((group_of(entry), line));
    }
    if rendered.is_empty() {
        // Every import went. Removing the block leaves the blank line that followed it, which the
        // caller's own formatting pass collapses.
        return changed(source, start, end, String::new());
    }

    // Sorted by group then by the line itself: within a group the line and the path order the same
    // way, and comparing the rendered line keeps `import static` beside its own kind.
    rendered.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

    let newline = if source.contains("\r\n") { "\r\n" } else { "\n" };
    let mut out = String::new();
    let mut previous: Option<u8> = None;
    for (group, line) in &rendered {
        if let Some(before) = previous {
            out.push_str(newline);
            // A blank line between the third-party block, the platform block, and the statics —
            // `javax` and `java` are one block, which is why the boundary is tested on the group's
            // BAND rather than on the group number.
            if band(before) != band(*group) {
                out.push_str(newline);
            }
        }
        out.push_str(line);
        previous = Some(*group);
    }
    changed(source, start, end, out)
}

/// The edit, unless it would replace the block with exactly what is already there.
fn changed(source: &str, start: usize, end: usize, text: String) -> Option<(usize, usize, String)> {
    (source.get(start..end) != Some(text.as_str())).then_some((start, end, text))
}

/// IntelliJ's Java default layout, as a sort key.
fn group_of(entry: &bennu_check::prelude::ImportEntry) -> u8 {
    if entry.static_ {
        return 3;
    }
    if entry.path.starts_with("javax.") || entry.path == "javax" {
        return 1;
    }
    if entry.path.starts_with("java.") || entry.path == "java" {
        return 2;
    }
    0
}

/// Which blank-line-separated band a group belongs to. `javax` and `java` share one.
fn band(group: u8) -> u8 {
    match group {
        1 | 2 => 1,
        3 => 2,
        _ => 0,
    }
}

/// Whether a `//` or `/* */` comment is written between two offsets.
///
/// A scan rather than a tree walk because it has to see a comment ANYWHERE in the gap, including
/// inside the whitespace tree-sitter attaches to neither import. String literals cannot occur here
/// — the region is import statements and the space between them — so `//` and `/*` are unambiguous.
fn has_comment_between(source: &str, start: usize, end: usize) -> bool {
    let Some(text) = source.get(start..end) else {
        return true;
    };
    text.contains("//") || text.contains("/*")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Apply the optimization and return the whole file, or `None` when there was nothing to do.
    fn run(src: &str) -> Option<String> {
        let (start, end, text) = optimize(src)?;
        let mut out = src.to_string();
        out.replace_range(start..end, &text);
        Some(out)
    }

    #[test]
    fn unused_imports_go_and_the_rest_are_grouped() {
        let src = "package p;\n\
                   import java.util.List;\n\
                   import static org.junit.Assert.assertTrue;\n\
                   import javax.servlet.ServletException;\n\
                   import org.acme.Thing;\n\
                   import java.io.File;\n\
                   \n\
                   class C { List<Thing> f() { assertTrue(true); return null; } }\n";
        let out = run(src).expect("something changed");
        assert!(out.contains(
            "import org.acme.Thing;\n\
             \n\
             import java.util.List;\n\
             \n\
             import static org.junit.Assert.assertTrue;"
        ), "got:\n{out}");
        // `ServletException` and `File` are named nowhere else.
        assert!(!out.contains("ServletException"));
        assert!(!out.contains("java.io.File"));
    }

    #[test]
    fn javax_and_java_share_a_band() {
        let src = "package p;\n\
                   import java.util.List;\n\
                   import javax.sql.DataSource;\n\
                   class C { List<DataSource> f() { return null; } }\n";
        let out = run(src).expect("the order changed");
        assert!(
            out.contains("import javax.sql.DataSource;\nimport java.util.List;"),
            "got:\n{out}"
        );
    }

    #[test]
    fn an_already_ordered_block_is_left_alone() {
        let src = "package p;\n\
                   import org.acme.Thing;\n\
                   \n\
                   import java.util.List;\n\
                   class C { List<Thing> f() { return null; } }\n";
        assert_eq!(run(src), None);
    }

    #[test]
    fn a_comment_inside_the_block_stops_the_rewrite() {
        let src = "package p;\n\
                   import java.util.List;\n\
                   // kept on purpose\n\
                   import org.acme.Thing;\n\
                   class C { List<Thing> f() { return null; } }\n";
        assert_eq!(run(src), None);
    }

    #[test]
    fn a_static_import_is_never_removed_even_when_nothing_names_it() {
        let src = "package p;\n\
                   import static org.acme.Util.helper;\n\
                   class C {}\n";
        // Nothing changes: the one import stays where it is. What matters is that it SURVIVES.
        let out = run(src).unwrap_or_else(|| src.to_string());
        assert!(out.contains("import static org.acme.Util.helper;"), "got:\n{out}");
    }

    #[test]
    fn a_wildcard_import_is_never_removed_and_keeps_its_star() {
        let src = "package p;\n\
                   import org.acme.*;\n\
                   import java.util.List;\n\
                   class C { List<Object> f() { return null; } }\n";
        let out = run(src).expect("the order changed");
        assert!(out.contains("import org.acme.*;"), "got:\n{out}");
    }

    #[test]
    fn a_duplicate_import_collapses_to_one() {
        let src = "package p;\n\
                   import java.util.List;\n\
                   import java.util.List;\n\
                   class C { List<Object> f() { return null; } }\n";
        let out = run(src).expect("the duplicate went");
        assert_eq!(out.matches("import java.util.List;").count(), 1, "got:\n{out}");
    }

    #[test]
    fn crlf_source_keeps_its_line_endings() {
        let src = "package p;\r\nimport org.acme.Thing;\r\nimport java.util.List;\r\nclass C { List<Thing> f() { return null; } }\r\n";
        let out = run(src).expect("the order changed");
        assert!(!out.contains("Thing;\nimport"), "a bare LF crept in:\n{out:?}");
    }

    #[test]
    fn a_file_with_no_imports_has_nothing_to_do() {
        assert_eq!(run("package p;\nclass C {}\n"), None);
    }
}
