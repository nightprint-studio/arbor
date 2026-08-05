//! `main_classes` domain — `bennu_main_classes`, powering the run-config editor's
//! main-class picker.
//!
//! A fresh source scan (no persisted index required): it walks the project's `.java`
//! sources under `root` with [`collect_java`] (the same walk `class_index` reuses),
//! runs [`extract_symbols`] on each, and returns one [`MainClassEntry`] per type that
//! declares a `public static void main(String[] …)` entry point.
//!
//! The `main`-method *detection* ([`is_main_method`]) is the pure, unit-tested core;
//! the FS walk + module attribution is the glue around it. `module` is the enclosing
//! Maven module (the nearest ancestor dir holding a `pom.xml`, relative to `root`) —
//! `None` for the root module — so a multi-module project's picker can group by module.

use std::path::{Path, PathBuf};

use bennu_core::prelude::BennuState;
use bennu_intel::prelude::{collect_java, read_source_for_index};
use bennu_java::prelude::{extract_symbols, MethodDecl};
use bennu_proto::prelude::MainClassEntry;
use serde::Deserialize;

/// Args for [`bennu_main_classes`].
#[derive(Deserialize)]
pub struct MainClassesArgs {
    /// Absolute path to the project root to scan.
    pub root: String,
}

/// Return one [`MainClassEntry`] per class declaring `public static void main(String[])`
/// in the project at `root`. A fresh `.java` scan — sources are decoded tolerantly (UTF-8
/// or legacy Cp1252/Latin-1), so a non-UTF-8 file's `main` still shows up; only a true IO
/// error skips a file. Never errors just because a project has no main class (yields `[]`).
#[arbor_rpc::handler]
fn bennu_main_classes(
    _ctx: &BennuState,
    args: MainClassesArgs,
) -> Result<Vec<MainClassEntry>, String> {
    let root = PathBuf::from(&args.root);
    let mut paths = Vec::new();
    collect_java(&root, &mut paths);

    // Decode in the project's declared encoding (Maven `sourceEncoding`), same as the build.
    let encoding_label = crate::index_service::resolve_index_encoding(&args.root);
    let mut out = Vec::new();
    for path in paths {
        let Some(decoded) = read_source_for_index(&path, &encoding_label) else {
            continue; // true IO error only — non-UTF-8 is decoded + recovered, not dropped
        };
        let source = decoded.text;
        let symbols = extract_symbols(&source);
        let has_main = symbols
            .types
            .iter()
            .filter(|td| td.methods.iter().any(is_main_method))
            .map(|td| td.fqn.clone())
            .collect::<Vec<_>>();
        if has_main.is_empty() {
            continue;
        }
        let file = path.to_string_lossy().replace('\\', "/");
        let module = module_of(&root, &path);
        for fqcn in has_main {
            out.push(MainClassEntry {
                fqcn,
                source_file: Some(file.clone()),
                module: module.clone(),
            });
        }
    }
    Ok(out)
}

/// Whether `m` is a Java entry point: a `static` method named `main` returning `void`
/// with a single `String[]` / `String...` parameter. (Access modifier isn't captured by
/// [`MethodDecl`]; the JVM requires `public`, but in practice a `static void main(String[])`
/// is unambiguously an entry point — being lenient on visibility never mis-fires here.)
fn is_main_method(m: &MethodDecl) -> bool {
    m.name == "main"
        && m.is_static
        && normalize_type(&m.return_type_text) == "void"
        && m.params.len() == 1
        && is_string_array(&m.params[0].type_text)
}

/// Whether a parameter type is `String[]` / `String...` (bare or `java.lang.`-qualified),
/// tolerant of whitespace between the name and the brackets.
///
/// An EMPTY type text is accepted as the varargs (`String... args`) case: tree-sitter-java
/// models `String...` as a `spread_parameter` whose type child is NOT under the `type`
/// field, so [`extract_symbols`] records its `type_text` empty. On a single-param
/// `static void main` this is unambiguously the varargs entry point (a `main(int x)` still
/// carries `"int"` and is rejected), so accepting empty catches `main(String...)` without
/// mis-firing.
fn is_string_array(type_text: &str) -> bool {
    let t = normalize_type(type_text);
    t.is_empty()
        || matches!(
            t.as_str(),
            "String[]" | "String..." | "java.lang.String[]" | "java.lang.String..."
        )
}

/// Collapse internal whitespace so `String []` / `String  ...` compare equal to their
/// canonical forms.
fn normalize_type(type_text: &str) -> String {
    type_text.split_whitespace().collect::<Vec<_>>().join("")
}

/// The Maven module `file` belongs to: the nearest ancestor dir (between `file` and
/// `root`, inclusive of intermediate dirs) that holds a `pom.xml`, expressed relative to
/// `root` with forward slashes. `None` when that dir is `root` itself (the root module)
/// or nothing matches.
pub(crate) fn module_of(root: &Path, file: &Path) -> Option<String> {
    let mut dir = file.parent()?;
    loop {
        // Stop once we've climbed above the project root.
        if !dir.starts_with(root) {
            return None;
        }
        if dir.join("pom.xml").is_file() {
            let rel = dir.strip_prefix(root).ok()?;
            let rel = rel.to_string_lossy().replace('\\', "/");
            return if rel.is_empty() { None } else { Some(rel) };
        }
        dir = dir.parent()?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::extract_symbols;

    /// `extract_symbols` on a canonical `public static void main(String[] args)` yields a
    /// method our detector flags — the end-to-end pure path the handler relies on.
    #[test]
    fn detects_canonical_main() {
        let src = "package it.foo;\n\
                   public class App {\n\
                   \x20 public static void main(String[] args) {}\n\
                   }\n";
        let syms = extract_symbols(src);
        let td = &syms.types[0];
        assert_eq!(td.fqn, "it.foo.App");
        assert!(td.methods.iter().any(is_main_method), "should detect main(String[])");
    }

    /// Varargs `main(String... args)` is a valid entry point too.
    #[test]
    fn detects_varargs_main() {
        let src = "public class V { public static void main(String... a) {} }\n";
        let syms = extract_symbols(src);
        assert!(syms.types[0].methods.iter().any(is_main_method));
    }

    /// Non-entry-point look-alikes must NOT be flagged: an instance `main`, a `main`
    /// with the wrong parameter, and a differently-named static `void(String[])`.
    #[test]
    fn rejects_non_main_methods() {
        let src = "public class N {\n\
                   \x20 public void main(String[] a) {}\n\
                   \x20 public static void main(int x) {}\n\
                   \x20 public static void run(String[] a) {}\n\
                   }\n";
        let syms = extract_symbols(src);
        assert!(!syms.types[0].methods.iter().any(is_main_method), "no method here is an entry point");
    }

    #[test]
    fn is_string_array_tolerates_qualified_and_spacing() {
        assert!(is_string_array("String[]"));
        assert!(is_string_array("String..."));
        assert!(is_string_array("java.lang.String[]"));
        assert!(is_string_array("String []"));
        assert!(is_string_array("")); // varargs spread_parameter (type not exposed)
        assert!(!is_string_array("String"));
        assert!(!is_string_array("int[]"));
    }
}
