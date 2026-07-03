//! Java → index ingestion: turn a project's `.java` sources into
//! [`bennu_index::prelude::IndexRecord`]s and drive the [`IndexBuilder`].
//!
//! Each declared type becomes a `Class` [`Symbol`] whose `members_json` is the
//! JSON-encoded resolved [`ClassMembers`](bennu_java::prelude::ClassMembers)
//! (supertypes + methods + fields, with binary-name types). It's reachable under two
//! fst keys — its simple name (bare-`Foo` completion) and its binary name
//! (`members_of(binary)` from the resolver). Each method/field also emits a
//! lightweight symbol under its simple name (the search-everywhere axis).
//!
//! JDK members are **not** copied into the index — they're resolved live off the
//! mmap'd rt.jar / jimage by the resolver. The index owns only the mutable project
//! surface, so a per-file edit patches just that file.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use bennu_index::prelude::{IndexBuilder, IndexRecord, Source, Symbol, SymbolKind};
use bennu_java::prelude::{
    extract_symbols, ClassMembers, FileSymbols, Import, Member, MemberKind, MethodDecl, TypeDecl,
    Visibility,
};

use crate::typemap::type_text_to_ref;

/// Build (or rebuild) the whole index for a project rooted at `root`, into `index_dir`.
/// Returns the builder (holding the per-file record set) so the caller can persist and
/// later [`patch`](IndexBuilder::patch_file) single files, plus `(type_count,
/// member_count)` for logging.
///
/// Reads each `.java` off disk once. When the caller has already read the sources (the be
/// layer reads them once and shares the text with the rename engine — no second disk pass),
/// prefer [`build_project_index_from_sources`].
pub fn build_project_index(
    root: &Path,
    index_dir: &Path,
) -> (IndexBuilder, usize, usize) {
    let sources = read_java_sources(root);
    let built = build_project_index_from_sources(&sources, index_dir);
    (built.builder, built.type_count, built.member_count)
}

/// One captured type declaration for the "Go to Class" navigator, built alongside the
/// symbol index (no separate whole-project re-parse). The be layer maps this to its wire
/// `ClassEntry`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassDecl {
    /// Fully-qualified, dotted class name (`com.acme.Order`).
    pub fqcn: String,
    /// The simple (unqualified) type name (`Order`).
    pub simple: String,
    /// Absolute path (forward slashes) of the source file declaring the type.
    pub file: String,
    /// 1-based line of the type declaration (recovered from the source token position).
    pub line: usize,
}

/// The whole-project build result: the persistable builder + symbol counts + the derived
/// caches the be layer keeps on the project slot (class navigator entries + the
/// simple→binary type map), so nothing downstream re-walks or re-parses the project.
pub struct ProjectBuild {
    /// The builder holding the per-file record set (persist + single-file patch).
    pub builder: IndexBuilder,
    pub type_count: usize,
    pub member_count: usize,
    /// One entry per declared type (class / interface / enum, incl. nested).
    pub classes: Vec<ClassDecl>,
    /// simple name → binary name for the project's own declared types.
    pub type_map: BTreeMap<String, String>,
}

/// Build the whole index from **already-read** `(path, source)` pairs — the single-read
/// path the be layer uses to share source text between the symbol index and the rename
/// engine (avoiding a second full disk pass). Parses the files in parallel and derives the
/// class navigator + type map in the same pass, so the caller needs no further re-walk.
pub fn build_project_index_from_sources(
    sources: &[(PathBuf, String)],
    index_dir: &Path,
) -> ProjectBuild {
    // Parse every file (in parallel across a bounded pool). Order is preserved so the
    // persist order stays deterministic.
    let parsed: Vec<(PathBuf, FileSymbols)> = parse_sources_parallel(sources);

    // First pass: every declared simple name → binary name, so a same-project type
    // reference resolves at ingest time (before the type's own file is processed).
    let mut project_types: BTreeMap<String, String> = BTreeMap::new();
    for (_p, fs) in &parsed {
        for td in &fs.types {
            project_types.insert(td.name.clone(), td.fqn.replace('.', "/"));
        }
    }

    let mut builder = IndexBuilder::new(index_dir);
    let mut next_id: u32 = 0;
    let mut type_count = 0usize;
    let mut member_count = 0usize;
    let mut classes: Vec<ClassDecl> = Vec::new();

    for ((path, fs), (_p2, source)) in parsed.iter().zip(sources.iter()) {
        let (records, ids, types, members) = file_records(path, fs, &project_types, next_id);
        next_id = ids;
        type_count += types;
        member_count += members;
        // Capture the class navigator entries from the SAME parse (the decl line is
        // recovered from the source token, mirroring the fresh-scan navigator).
        let file_key = path.to_string_lossy().replace('\\', "/");
        for td in &fs.types {
            classes.push(ClassDecl {
                fqcn: td.fqn.clone(),
                simple: td.name.clone(),
                file: file_key.clone(),
                line: decl_line(source, &td.name).unwrap_or(1),
            });
        }
        builder.set_file(path.clone(), records);
    }

    ProjectBuild { builder, type_count, member_count, classes, type_map: project_types }
}

/// Read every project `.java` source once, in parallel — the single disk pass the be
/// layer shares between the symbol index build and the rename-engine build. Paths are
/// normalized on read; an unreadable / non-UTF-8 file is skipped (robust scan).
pub fn read_java_sources(root: &Path) -> Vec<(PathBuf, String)> {
    let mut paths = Vec::new();
    collect_java(root, &mut paths);
    read_sources_parallel(&paths)
}

/// Read `paths` off disk in parallel across a bounded std-thread pool, preserving order.
/// Unreadable / non-UTF-8 files are dropped. (No rayon: not a workspace dependency.)
fn read_sources_parallel(paths: &[PathBuf]) -> Vec<(PathBuf, String)> {
    parallel_map(paths, |p| std::fs::read_to_string(p).ok().map(|s| (p.clone(), s)))
        .into_iter()
        .flatten()
        .collect()
}

/// Parse `sources` in parallel across a bounded std-thread pool, preserving order.
fn parse_sources_parallel(sources: &[(PathBuf, String)]) -> Vec<(PathBuf, FileSymbols)> {
    parallel_map(sources, |(p, src)| (p.clone(), extract_symbols(src)))
}

/// Map `f` over `items` across at most `available_parallelism` worker threads, returning
/// the results in input order. A tiny, dependency-free work-stealing-by-chunks split: each
/// worker owns a contiguous slice, so no per-item synchronization. Falls back to a serial
/// map for a small input (the thread spawn isn't worth it) or a single core.
fn parallel_map<T, R, F>(items: &[T], f: F) -> Vec<R>
where
    T: Sync,
    R: Send,
    F: Fn(&T) -> R + Sync,
{
    let n = items.len();
    let workers = std::thread::available_parallelism().map(|p| p.get()).unwrap_or(1);
    // Serial for a small project / single core — the parse of a handful of files is faster
    // than spinning threads up.
    if workers <= 1 || n <= 32 {
        return items.iter().map(&f).collect();
    }
    let chunks = workers.min(n);
    let chunk_size = n.div_ceil(chunks);
    let f = &f;
    let mut out: Vec<Option<R>> = (0..n).map(|_| None).collect();

    std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for (ci, item_chunk) in items.chunks(chunk_size).enumerate() {
            let base = ci * chunk_size;
            handles.push(scope.spawn(move || {
                let mut local = Vec::with_capacity(item_chunk.len());
                for it in item_chunk {
                    local.push(f(it));
                }
                (base, local)
            }));
        }
        for h in handles {
            let (base, local) = h.join().expect("parse worker panicked");
            for (i, r) in local.into_iter().enumerate() {
                out[base + i] = Some(r);
            }
        }
    });

    out.into_iter().map(|o| o.expect("every slot filled")).collect()
}

/// Find the 1-based line of a type declaration by locating the first
/// `class`/`interface`/`enum <name>` token in `source`. Mirrors the fresh-scan navigator's
/// recovery (the extractor's [`TypeDecl`] carries no offset). `None` → caller defaults to 1.
fn decl_line(source: &str, name: &str) -> Option<usize> {
    for (idx, line) in source.lines().enumerate() {
        if line_declares_type(line, name) {
            return Some(idx + 1);
        }
    }
    None
}

/// Whether `line` contains a `class|interface|enum <name>` declaration token — a keyword
/// whose next non-space word is exactly `name` (bounded so `Foo` doesn't match `FooBar`).
fn line_declares_type(line: &str, name: &str) -> bool {
    for kw in ["class", "interface", "enum"] {
        let mut rest = line;
        while let Some(pos) = rest.find(kw) {
            let after = &rest[pos + kw.len()..];
            let before_ok = pos == 0
                || !rest.as_bytes()[pos - 1].is_ascii_alphanumeric()
                    && rest.as_bytes()[pos - 1] != b'_';
            let name_after = after.trim_start();
            if before_ok
                && after.len() != name_after.len()
                && name_after.starts_with(name)
            {
                let tail = &name_after[name.len()..];
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

/// Re-extract a single file's source into fresh [`IndexRecord`]s (for an incremental
/// [`patch`](IndexBuilder::patch_file)). `project_types` should be the current
/// project-wide simple→binary map so cross-type references still resolve.
pub fn file_records_from_source(
    path: &Path,
    source: &str,
    project_types: &BTreeMap<String, String>,
    start_id: u32,
) -> Vec<IndexRecord> {
    let fs = extract_symbols(source);
    file_records(path, &fs, project_types, start_id).0
}

/// Build one file's records; returns `(records, next_id, type_count, member_count)`.
/// `project_types` is the project-wide simple→binary type map so cross-type references
/// (a field of another project type) resolve to a binary name at ingest time.
fn file_records(
    path: &Path,
    fs: &FileSymbols,
    project_types: &BTreeMap<String, String>,
    start_id: u32,
) -> (Vec<IndexRecord>, u32, usize, usize) {
    let mut records = Vec::new();
    let mut next_id = start_id;
    let mut type_count = 0usize;
    let mut member_count = 0usize;
    let file_str = path.to_string_lossy().into_owned();

    for td in &fs.types {
        let binary = td.fqn.replace('.', "/");
        let class_id = next_id;
        next_id += 1;
        type_count += 1;

        let members = build_class_members(td, &fs.imports, project_types);
        let members_json = serde_json::to_string(&members).unwrap_or_default();

        let class_sym = Symbol {
            id: class_id,
            kind: SymbolKind::Class,
            simple_name: td.name.clone(),
            fqn: binary.clone(),
            owner_id: u32::MAX,
            source: Source::ProjectSource,
            signature: format!("class {}", td.fqn),
            modifiers: String::new(),
            loc_file: file_str.clone(),
            loc_start: 0,
            loc_end: 0,
            loc_container: String::new(),
            loc_class: String::new(),
            members_json,
        };
        // Reachable by BOTH its simple name and its binary name → one payload.
        records.push(IndexRecord::new(class_sym, td.name.clone()).with_key(binary.clone()));

        for m in &td.methods {
            records.push(IndexRecord::new(
                member_symbol(next_id, class_id, &binary, &m.name, SymbolKind::Method,
                    &render_method(m), &file_str, m.is_static),
                m.name.clone(),
            ));
            next_id += 1;
            member_count += 1;
        }
        for f in &td.fields {
            records.push(IndexRecord::new(
                member_symbol(next_id, class_id, &binary, &f.name, SymbolKind::Field,
                    &format!("{} {}", f.type_text, f.name), &file_str, f.is_static),
                f.name.clone(),
            ));
            next_id += 1;
            member_count += 1;
        }
    }

    (records, next_id, type_count, member_count)
}

/// Build a resolved [`ClassMembers`] (java seam shape) from a source [`TypeDecl`],
/// resolving each written type text to a binary name via imports + project types.
fn build_class_members(
    td: &TypeDecl,
    imports: &[Import],
    project_types: &BTreeMap<String, String>,
) -> ClassMembers {
    let superclass = td.extends.as_ref().map(|s| resolve_binary(s, imports, project_types));
    let interfaces =
        td.implements.iter().map(|i| resolve_binary(i, imports, project_types)).collect();

    let methods = td
        .methods
        .iter()
        .map(|m| Member {
            name: m.name.clone(),
            kind: MemberKind::Method,
            return_type: type_text_to_ref(&m.return_type_text, imports, project_types),
            params: m
                .params
                .iter()
                .map(|p| type_text_to_ref(&p.type_text, imports, project_types))
                .collect(),
            is_static: m.is_static,
            visibility: Visibility::Public,
            raw_signature: render_method(m),
        })
        .collect();

    let fields = td
        .fields
        .iter()
        .map(|f| Member {
            name: f.name.clone(),
            kind: MemberKind::Field,
            return_type: type_text_to_ref(&f.type_text, imports, project_types),
            params: Vec::new(),
            is_static: f.is_static,
            visibility: Visibility::Public,
            raw_signature: format!("{} {}", f.type_text, f.name),
        })
        .collect();

    ClassMembers { superclass, interfaces, methods, fields }
}

/// Resolve a supertype simple name to a binary name (generics stripped).
fn resolve_binary(
    simple: &str,
    imports: &[Import],
    project_types: &BTreeMap<String, String>,
) -> String {
    let base = simple.split('<').next().unwrap_or(simple).trim();
    if base.contains('.') {
        return base.replace('.', "/");
    }
    if let Some(b) = project_types.get(base) {
        return b.clone();
    }
    for imp in imports {
        if imp.simple_name() == Some(base) {
            return imp.path.replace('.', "/");
        }
    }
    base.to_string()
}

fn render_method(m: &MethodDecl) -> String {
    let params: Vec<String> =
        m.params.iter().map(|p| format!("{} {}", p.type_text, p.name)).collect();
    format!("{} {}({})", m.return_type_text, m.name, params.join(", "))
}

#[allow(clippy::too_many_arguments)]
fn member_symbol(
    id: u32,
    owner: u32,
    owner_binary: &str,
    name: &str,
    kind: SymbolKind,
    signature: &str,
    file: &str,
    is_static: bool,
) -> Symbol {
    Symbol {
        id,
        kind,
        simple_name: name.to_string(),
        fqn: owner_binary.to_string(),
        owner_id: owner,
        source: Source::ProjectSource,
        signature: signature.to_string(),
        modifiers: if is_static { "static".into() } else { String::new() },
        loc_file: file.to_string(),
        loc_start: 0,
        loc_end: 0,
        loc_container: String::new(),
        loc_class: String::new(),
        members_json: String::new(),
    }
}

/// The project-wide simple→binary type map for a set of `.java` files (used to seed a
/// patch so cross-file references still resolve). Cheap re-scan of just the type decls.
pub fn project_type_map(root: &Path) -> BTreeMap<String, String> {
    let mut paths = Vec::new();
    collect_java(root, &mut paths);
    let mut map = BTreeMap::new();
    for p in paths {
        if let Ok(src) = std::fs::read_to_string(&p) {
            for td in extract_symbols(&src).types {
                map.insert(td.name, td.fqn.replace('.', "/"));
            }
        }
    }
    map
}

/// Recursively collect `.java` files under `dir`, skipping `target` / hidden dirs.
pub fn collect_java(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name == "target" || name.starts_with('.') {
                continue;
            }
            collect_java(&p, out);
        } else if p.extension().and_then(|x| x.to_str()) == Some("java") {
            out.push(p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decl_line_locates_type_and_is_word_bounded() {
        let src = "package a.b;\n\npublic class Order {\n  int x;\n}\n";
        assert_eq!(decl_line(src, "Order"), Some(3));
        assert_eq!(decl_line("interface Repo {}\n", "Repo"), Some(1));
        // `Foo` must not match the declaration of `FooBar`.
        assert_eq!(decl_line("class FooBar {}\n", "Foo"), None);
        assert_eq!(decl_line("class FooBar {}\n", "FooBar"), Some(1));
    }

    #[test]
    fn parallel_map_preserves_order_for_large_input() {
        // Above the serial cutoff so the threaded path runs; the doubled values must come
        // back in input order.
        let items: Vec<usize> = (0..1000).collect();
        let out = parallel_map(&items, |x| x * 2);
        assert_eq!(out.len(), 1000);
        assert!(out.iter().enumerate().all(|(i, v)| *v == i * 2));
    }

    #[test]
    fn parallel_map_serial_path_for_small_input() {
        let items: Vec<usize> = (0..5).collect();
        let out = parallel_map(&items, |x| x + 1);
        assert_eq!(out, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn build_from_sources_captures_class_navigator_and_type_map() {
        let dir = std::env::temp_dir().join(format!("bennu-jidx-src-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let sources = vec![
            (
                PathBuf::from("/proj/src/a/Order.java"),
                "package a;\npublic class Order { int total() { return 1; } }\n".to_string(),
            ),
            (
                PathBuf::from("/proj/src/a/Repo.java"),
                "package a;\npublic interface Repo { }\n".to_string(),
            ),
        ];
        let built = build_project_index_from_sources(&sources, &dir);
        assert_eq!(built.type_count, 2);
        // Class navigator: both types captured, with a recovered decl line (line 2 each).
        assert_eq!(built.classes.len(), 2);
        let order = built.classes.iter().find(|c| c.simple == "Order").expect("Order");
        assert_eq!(order.fqcn, "a.Order");
        assert_eq!(order.line, 2);
        assert!(order.file.contains("a/Order.java")); // forward-slash normalized
        // Type map: simple → binary for both project types.
        assert_eq!(built.type_map.get("Order").map(String::as_str), Some("a/Order"));
        assert_eq!(built.type_map.get("Repo").map(String::as_str), Some("a/Repo"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
