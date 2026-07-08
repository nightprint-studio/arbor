//! `import_class` — the pure edit half of the "Import class" intention.
//!
//! Given the source and a fully-qualified class name, compute WHERE to insert the `import …;` line
//! (byte offset) and the exact text — respecting the file's package declaration, its existing imports
//! (inserting alphabetically), and its newline style (LF / CRLF). The DETECTION half (which simple
//! type name under the caret needs an import) and the CANDIDATE lookup (which FQNs a simple name maps
//! to) live in the resolver-backed layer; this stays a pure, exhaustively-testable string transform.

use crate::Edit;

/// Compute the edit that adds `import <fqn>;` to `source`, or `None` when it's already imported.
///
/// Placement, in order of preference:
///   1. **Alphabetically** among the existing non-static imports (before the first whose path sorts
///      after `fqn`, else after the last import).
///   2. After the last import when only `static` imports exist.
///   3. After the `package …;` line (with a blank line between it and the new import).
///   4. At the very top of the file (no package, no imports).
///
/// The inserted line uses the file's prevailing newline (`\r\n` if the file has any, else `\n`).
pub fn insert_import_edit(source: &str, fqn: &str) -> Option<Edit> {
    let import_stmt = format!("import {fqn};");
    let nl = if source.contains("\r\n") { "\r\n" } else { "\n" };

    let mut package_end: Option<usize> = None; // byte offset just past the package line (incl. newline)
    let mut last_import_end: Option<usize> = None; // just past the last import line (any kind)
    let mut plain_imports: Vec<(usize, String)> = Vec::new(); // (line_start, imported path) — non-static

    let mut offset = 0usize;
    for line in source.split_inclusive('\n') {
        let line_start = offset;
        offset += line.len();
        let trimmed = line.trim();
        if trimmed == import_stmt {
            return None; // already imported — nothing to do
        }
        if trimmed.starts_with("package ") {
            package_end = Some(offset);
        } else if trimmed.starts_with("import ") {
            last_import_end = Some(offset);
            if !trimmed.starts_with("import static ") {
                let path = trimmed
                    .trim_start_matches("import ")
                    .trim_end_matches(';')
                    .trim()
                    .to_string();
                plain_imports.push((line_start, path));
            }
        }
    }

    let (at, text) = if !plain_imports.is_empty() {
        // Alphabetical: before the first existing import that sorts AFTER `fqn`, else after them all.
        let insert_before = plain_imports.iter().find(|(_, path)| path.as_str() > fqn).map(|(s, _)| *s);
        match insert_before {
            Some(pos) => (pos, format!("{import_stmt}{nl}")),
            None => (last_import_end.unwrap_or(0), format!("{import_stmt}{nl}")),
        }
    } else if let Some(end) = last_import_end {
        // Only static imports — put the new plain import after them.
        (end, format!("{import_stmt}{nl}"))
    } else if let Some(pkg_end) = package_end {
        // No imports yet: a blank line separates the package declaration from the first import.
        (pkg_end, format!("{nl}{import_stmt}{nl}"))
    } else {
        // No package, no imports: top of the file.
        (0, format!("{import_stmt}{nl}"))
    };

    Some(Edit { start: at, end: at, replacement: text })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Apply the edit and return the resulting source (for readable assertions).
    fn applied(source: &str, fqn: &str) -> String {
        let e = insert_import_edit(source, fqn).expect("an edit");
        format!("{}{}{}", &source[..e.start], e.replacement, &source[e.end..])
    }

    #[test]
    fn inserts_after_package_when_no_imports() {
        let src = "package com.acme;\n\npublic class C {}\n";
        assert_eq!(
            applied(src, "java.util.List"),
            "package com.acme;\n\nimport java.util.List;\n\npublic class C {}\n"
        );
    }

    #[test]
    fn inserts_at_top_when_no_package_and_no_imports() {
        let src = "class C {}\n";
        assert_eq!(applied(src, "java.util.List"), "import java.util.List;\nclass C {}\n");
    }

    #[test]
    fn inserts_alphabetically_among_existing_imports() {
        let src = "package a;\n\nimport java.util.List;\nimport java.util.Set;\n\nclass C {}\n";
        // `java.util.Map` sorts between List and Set.
        assert_eq!(
            applied(src, "java.util.Map"),
            "package a;\n\nimport java.util.List;\nimport java.util.Map;\nimport java.util.Set;\n\nclass C {}\n"
        );
    }

    #[test]
    fn appends_after_last_import_when_alphabetically_greatest() {
        let src = "import java.util.List;\nimport java.util.Map;\n\nclass C {}\n";
        assert_eq!(
            applied(src, "java.util.Set"),
            "import java.util.List;\nimport java.util.Map;\nimport java.util.Set;\n\nclass C {}\n"
        );
    }

    #[test]
    fn already_imported_yields_no_edit() {
        let src = "package a;\n\nimport java.util.List;\n\nclass C {}\n";
        assert!(insert_import_edit(src, "java.util.List").is_none());
    }

    #[test]
    fn goes_after_static_only_imports() {
        let src = "package a;\n\nimport static org.junit.Assert.assertTrue;\n\nclass C {}\n";
        assert_eq!(
            applied(src, "java.util.List"),
            "package a;\n\nimport static org.junit.Assert.assertTrue;\nimport java.util.List;\n\nclass C {}\n"
        );
    }

    #[test]
    fn preserves_crlf_newlines() {
        let src = "package a;\r\n\r\nclass C {}\r\n";
        let out = applied(src, "java.util.List");
        assert!(out.contains("\r\nimport java.util.List;\r\n"), "{out:?}");
    }
}
