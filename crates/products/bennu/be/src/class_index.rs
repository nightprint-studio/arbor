//! `class_index` domain — `bennu_class_index`, powering the "Go to Class" navigator.
//!
//! **Cache-first**: when a project is open at `root`, the entries were already captured
//! during the background index build (the same parse that built the symbol index — no
//! separate whole-project scan), so this handler returns them instantly. The **fresh**
//! source scan below is the fallback for when no open project owns `root` or its build
//! hasn't landed yet.
//!
//! The fresh scan walks the project's `.java` sources under `root` with [`collect_java`],
//! runs [`extract_symbols`] on each, and emits one [`ClassEntry`] per declared type (class
//! / interface / enum, including nested types).
//!
//! `extract_symbols`' [`TypeDecl`] carries no byte offset / line, so the declaration
//! line is recovered here by locating the `class`/`interface`/`enum <Name>` token in
//! the source and counting newlines up to it; a type that can't be located that way
//! falls back to line 1. Sources are decoded in the project's declared encoding (Maven
//! `sourceEncoding`) via `read_source_for_index`, recovering a mislabelled file rather than
//! dropping it, so a non-UTF-8 file still surfaces its classes; only a genuine IO error skips
//! a file (and that's logged, not silent).

use std::path::Path;

use bennu_core::prelude::BennuState;
use bennu_intel::prelude::{collect_java, read_source_for_index};
use bennu_java::prelude::extract_symbols;
use bennu_proto::prelude::ClassEntry;
use serde::Deserialize;

use crate::index_service::IndexService;

/// Args for [`bennu_class_index`].
#[derive(Deserialize)]
pub struct ClassIndexArgs {
    /// Absolute path to the project root to scan.
    pub root: String,
}

/// Return one [`ClassEntry`] per declared type in the project at `root`. Serves the cache
/// the index build populated when a project is open (instant, no re-parse); falls back to a
/// fresh `.java` scan otherwise.
#[arbor_rpc::handler]
fn bennu_class_index(_ctx: &BennuState, args: ClassIndexArgs) -> Result<Vec<ClassEntry>, String> {
    // Cache-first: the open project's build already captured every declared type.
    if let Some(cached) = IndexService::global().class_index(&args.root) {
        return Ok(cached);
    }

    let mut paths = Vec::new();
    collect_java(Path::new(&args.root), &mut paths);

    // Decode in the project's declared encoding (Maven `sourceEncoding`), recovering a
    // mislabelled file rather than dropping it — same policy as the open-project build.
    let encoding_label = crate::index_service::resolve_index_encoding(&args.root);
    let mut out = Vec::new();
    for path in paths {
        // Only a true IO error skips (logged inside the helper); a non-UTF-8 source is
        // decoded + recovered so its classes still surface.
        let Some(decoded) = read_source_for_index(&path, &encoding_label) else {
            continue;
        };
        let source = decoded.text;
        let file = path.to_string_lossy().replace('\\', "/");
        let symbols = extract_symbols(&source);
        for td in &symbols.types {
            let line = decl_line(&source, &td.name).unwrap_or(1);
            out.push(ClassEntry {
                fqcn: td.fqn.clone(),
                simple: td.name.clone(),
                file: file.clone(),
                line,
            });
        }
    }
    Ok(out)
}

/// Find the 1-based line of a type declaration by locating the first
/// `class`/`interface`/`enum` keyword immediately followed (ignoring whitespace) by
/// the type `name`. Returns `None` when no such site is found (caller defaults to 1).
fn decl_line(source: &str, name: &str) -> Option<usize> {
    for (idx, line) in source.lines().enumerate() {
        if line_declares_type(line, name) {
            return Some(idx + 1);
        }
    }
    None
}

/// Whether `line` contains a `class|interface|enum <name>` declaration token — a
/// keyword whose next non-space word is exactly `name` (bounded so `Foo` doesn't
/// match `FooBar`).
fn line_declares_type(line: &str, name: &str) -> bool {
    for kw in ["class", "interface", "enum"] {
        let mut rest = line;
        while let Some(pos) = rest.find(kw) {
            let after = &rest[pos + kw.len()..];
            // The keyword must be a standalone word (space/tab before the name).
            let before_ok = pos == 0
                || !rest.as_bytes()[pos - 1].is_ascii_alphanumeric()
                    && rest.as_bytes()[pos - 1] != b'_';
            let name_after = after.trim_start();
            if before_ok
                && after.len() != name_after.len() // there WAS whitespace after the kw
                && name_after.starts_with(name)
            {
                let tail = &name_after[name.len()..];
                // The name must end here (bound: `{`, `<`, whitespace, `extends`…).
                let bounded = tail
                    .chars()
                    .next()
                    .map(|c| !c.is_alphanumeric() && c != '_')
                    .unwrap_or(true);
                if bounded {
                    return true;
                }
            }
            rest = &rest[pos + kw.len()..];
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locates_class_and_interface_lines() {
        let src = "package a.b;\n\npublic class Order {\n  int x;\n}\n";
        assert_eq!(decl_line(src, "Order"), Some(3));

        let src2 = "interface Repo {}\n";
        assert_eq!(decl_line(src2, "Repo"), Some(1));
    }

    #[test]
    fn name_is_word_bounded() {
        // `Foo` must not match the declaration of `FooBar`.
        let src = "class FooBar {}\n";
        assert_eq!(decl_line(src, "Foo"), None);
        assert_eq!(decl_line(src, "FooBar"), Some(1));
    }
}
