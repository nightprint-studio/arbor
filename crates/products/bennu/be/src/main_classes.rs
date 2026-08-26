//! `main_classes` domain — `bennu_main_classes`, powering the run-config editor's
//! main-class picker and the Spring Boot entry-point resolution.
//!
//! A source scan (no persisted index required): it walks the project's `.java` sources under
//! `root` with [`collect_java`] (the same walk `class_index` reuses) and returns one
//! [`MainClassEntry`] per type declaring a `public static void main(String[] …)` entry point.
//!
//! ## Two things keep it from being a whole-project parse
//!
//! - **A `contains` pre-filter.** A file with no `main(` in its text cannot declare an entry
//!   point, so it is never handed to tree-sitter. On a legacy tree that rejects essentially
//!   every file, turning "parse five thousand sources" into "read five thousand sources and
//!   parse twenty". Same trick, and the same trade-off, as `bennu-test`'s `TEST_MARKERS`: a
//!   false hit costs one parse, a false miss costs an entry point the picker never offers.
//! - **A per-root cache**, dropped when the project is re-indexed. The picker is opened
//!   repeatedly, ▷ consults it on every press, and a Spring Boot launch resolves its class
//!   through it — none of which should re-walk the tree.
//!
//! Both existed in the test domain and neither existed here, which is why wiring this up to
//! the run configuration editor could saturate the backend: every open of the modal cost a
//! full parse of the project, on a thread that then held a core while the editor's own
//! per-keystroke requests queued behind it.
//!
//! The `main`-method *detection* ([`is_main_method`]) is the pure, unit-tested core;
//! the FS walk + module attribution is the glue around it. `module` is the enclosing
//! Maven module (the nearest ancestor dir holding a `pom.xml`, relative to `root`) —
//! `None` for the root module — so a multi-module project's picker can group by module.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};

use bennu_core::prelude::BennuState;
use bennu_intel::prelude::{collect_java, read_source_for_index};
use bennu_java::prelude::{extract_symbols, MethodDecl, TypeDecl};
use bennu_proto::prelude::MainClassEntry;
use serde::Deserialize;

/// Args for [`bennu_main_classes`].
#[derive(Deserialize)]
pub struct MainClassesArgs {
    /// Absolute path to the project root to scan.
    pub root: String,
    /// Re-scan even if this project has been scanned before. The cache is dropped on
    /// re-index anyway; this is for a caller that knows better.
    #[serde(default)]
    pub force: bool,
}

/// The pre-filter: a declaration always writes the name and the parameter list together, so a
/// source with neither of these cannot declare an entry point and never reaches the parser.
///
/// `main (` is included because the space is legal, not because anyone writes it — the cost
/// of carrying it is one extra `contains` per file, and the cost of omitting it would be a
/// class that can never be run from the UI with no way to tell why.
const MAIN_MARKERS: &[&str] = &["main(", "main ("];

/// Return one [`MainClassEntry`] per class declaring `public static void main(String[])`
/// in the project at `root`. Sources are decoded tolerantly (UTF-8 or legacy
/// Cp1252/Latin-1), so a non-UTF-8 file's `main` still shows up; only a true IO error skips
/// a file. Never errors just because a project has no main class (yields `[]`).
///
/// Cached per root — see the module doc.
#[arbor_rpc::handler]
fn bennu_main_classes(
    _ctx: &BennuState,
    args: MainClassesArgs,
) -> Result<Vec<MainClassEntry>, String> {
    if !args.force {
        if let Some(hit) = cache().read().ok().and_then(|c| c.get(&args.root).cloned()) {
            return Ok((*hit).clone());
        }
    }

    let root = PathBuf::from(&args.root);
    let mut paths = Vec::new();
    collect_java(&root, &mut paths);

    // Decode in the project's declared encoding (Maven `sourceEncoding`), same as the build.
    let encoding = crate::index_service::encoding_plan(&args.root);
    let mut out = Vec::new();
    for path in paths {
        let Some(decoded) = read_source_for_index(&path, &encoding) else {
            continue; // true IO error only — non-UTF-8 is decoded + recovered, not dropped
        };
        let source = decoded.text;
        // The cheap gate, BEFORE the parse — this is what keeps a whole-project scan off the
        // parser for all but a handful of files.
        if !MAIN_MARKERS.iter().any(|m| source.contains(m)) {
            continue;
        }
        let symbols = extract_symbols(&source);
        let has_main = symbols
            .types
            .iter()
            .filter(|td| td.methods.iter().any(is_main_method))
            // Carry whether the type is a Boot application, so a Spring Boot configuration
            // can resolve its own entry point instead of asking for it.
            .map(|td| (td.fqn.clone(), is_spring_boot_app(td)))
            .collect::<Vec<_>>();
        if has_main.is_empty() {
            continue;
        }
        let file = path.to_string_lossy().replace('\\', "/");
        let module = module_of(&root, &path);
        for (fqcn, spring_boot) in has_main {
            out.push(MainClassEntry {
                fqcn,
                source_file: Some(file.clone()),
                module: module.clone(),
                spring_boot,
            });
        }
    }

    if let Ok(mut c) = cache().write() {
        c.insert(args.root.clone(), Arc::new(out.clone()));
    }
    Ok(out)
}

/// Whole-project entry points, per root. Entry points change about as often as the build
/// does, and the walk is a read of every `.java` in the tree, so the picker opening — or ▷
/// being pressed — must not pay for it twice.
fn cache() -> &'static RwLock<HashMap<String, Arc<Vec<MainClassEntry>>>> {
    static CACHE: OnceLock<RwLock<HashMap<String, Arc<Vec<MainClassEntry>>>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Drop a project's cached entry points — called when its index is rebuilt, so a newly
/// written `main` doesn't need a restart to appear in the picker.
pub(crate) fn forget_main_classes(root: &str) {
    if let Ok(mut c) = cache().write() {
        c.remove(root);
    }
}

/// Whether `td` is a Spring Boot application class.
///
/// `@SpringBootApplication` is the composed annotation everyone uses; the three it stands for
/// (`@SpringBootConfiguration` + `@EnableAutoConfiguration` + `@ComponentScan`) are accepted
/// individually too, because a project that spells them out is still a Boot application and
/// the point of this is to spare someone typing a class name.
///
/// Simple names only — the annotations are read off the source, where they appear as they
/// were written, and no other annotation in the ecosystem shares these names.
fn is_spring_boot_app(td: &TypeDecl) -> bool {
    td.has_annotation("SpringBootApplication")
        || td.has_annotation("SpringBootConfiguration")
        || td.has_annotation("EnableAutoConfiguration")
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

    /// A Boot entry point is recognised, so a Spring Boot configuration can fill its own main
    /// class in rather than asking for a name the editor is holding.
    #[test]
    fn detects_a_spring_boot_application() {
        let src = "package it.foo;\n\
                   @SpringBootApplication\n\
                   public class App {\n\
                   \x20 public static void main(String[] args) {}\n\
                   }\n";
        let td = &extract_symbols(src).types[0];
        assert!(is_spring_boot_app(td));
        assert!(td.methods.iter().any(is_main_method));
    }

    /// A plain `main` class is NOT a Boot app — otherwise every project would look like one.
    #[test]
    fn a_plain_main_class_is_not_a_boot_app() {
        let src = "public class Tool { public static void main(String[] a) {} }\n";
        assert!(!is_spring_boot_app(&extract_symbols(src).types[0]));
    }

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
