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
use bennu_project::prelude::{decode_for_index, source_encoding_label, IndexDecode};

use crate::typemap::type_text_to_ref;

/// Build (or rebuild) the whole index for a project rooted at `root`, into `index_dir`.
/// Returns the builder (holding the per-file record set) so the caller can persist and
/// later [`patch`](IndexBuilder::patch_file) single files, plus `(type_count,
/// member_count)` for logging.
///
/// Reads each `.java` off disk once, in the project's declared encoding (resolved from the
/// pom's `sourceEncoding`, else UTF-8). When the caller has already read the sources (the be
/// layer reads them once and shares the text with the rename engine — no second disk pass),
/// prefer [`build_project_index_from_sources`].
pub fn build_project_index(
    root: &Path,
    index_dir: &Path,
) -> (IndexBuilder, usize, usize) {
    let label = source_encoding_label(root, "UTF-8");
    let ProjectSources { sources, .. } = read_java_sources(root, &label);
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

/// A source file whose bytes weren't valid in the project's declared (Maven) encoding — it
/// was recovered + indexed anyway, and flagged here for the "non-compliant files" report the
/// be layer surfaces (a future UI). See [`read_java_sources`] / `decode_for_index`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NonCompliantSource {
    /// Absolute path of the offending source.
    pub file: PathBuf,
    /// The encoding the project declared (and that didn't fit the bytes).
    pub declared_encoding: String,
    /// The encoding actually used to recover the text (`UTF-8` / `windows-1252`).
    pub decoded_as: String,
}

/// The read + decoded project sources, plus the files that weren't valid in the declared
/// encoding (recovered, not dropped). Returned by [`read_java_sources`].
pub struct ProjectSources {
    /// One `(path, decoded text)` per readable `.java` source, in walk order.
    pub sources: Vec<(PathBuf, String)>,
    /// The non-compliant subset (a diagnostic list; the sources above still include them).
    pub non_compliant: Vec<NonCompliantSource>,
}

/// Read every project `.java` source once, in parallel — the single disk pass the be layer
/// shares between the symbol index build and the rename-engine build. Each is decoded in the
/// project's declared `encoding_label` (the Maven `sourceEncoding`); a file whose bytes don't
/// fit is recovered and reported (never silently dropped), and only a genuinely unreadable
/// file (IO error) is skipped (and logged).
pub fn read_java_sources(root: &Path, encoding_label: &str) -> ProjectSources {
    let mut paths = Vec::new();
    collect_java(root, &mut paths);
    read_sources_parallel(&paths, encoding_label)
}

/// Read `paths` off disk in parallel across a bounded std-thread pool, preserving order.
/// Each is decoded via [`read_source_for_index`] in `encoding_label`; non-compliant files are
/// collected, IO-unreadable files dropped (and logged). (No rayon: not a workspace dep.)
fn read_sources_parallel(paths: &[PathBuf], encoding_label: &str) -> ProjectSources {
    let decoded = parallel_map(paths, |p| {
        read_source_for_index(p, encoding_label).map(|d| (p.clone(), d))
    });
    let mut sources = Vec::with_capacity(decoded.len());
    let mut non_compliant = Vec::new();
    for (p, d) in decoded.into_iter().flatten() {
        if d.non_compliant {
            non_compliant.push(NonCompliantSource {
                file: p.clone(),
                declared_encoding: encoding_label.to_string(),
                decoded_as: d.encoding.clone(),
            });
        }
        sources.push((p, d.text));
    }
    ProjectSources { sources, non_compliant }
}

/// Read a `.java` file for INDEXING, decoded in the project's declared `encoding_label`.
///
/// The whole index build hangs off this: Java's structure (keywords, identifiers, braces) is
/// ASCII, so a source that isn't valid UTF-8 — Cp1252 / ISO-8859-1, the norm in legacy
/// Struts/Entando trees — MUST still be indexed, or its classes vanish silently from
/// completion, Go-to-Class and navigation. Decoding goes through `bennu-project`'s
/// `decode_for_index`: it tries the declared (Maven) encoding first, and recovers via
/// `encoding_rs` (UTF-8, else Windows-1252) when the bytes don't fit — flagging the file
/// non-compliant rather than dropping it. Returns `None` only on a genuine IO error, which is
/// logged, so the sole remaining skip is visible rather than silent.
pub fn read_source_for_index(path: &Path, encoding_label: &str) -> Option<IndexDecode> {
    match std::fs::read(path) {
        Ok(bytes) => Some(decode_for_index(&bytes, encoding_label)),
        Err(e) => {
            eprintln!("bennu-intel: skipping unreadable source {}: {e}", path.display());
            None
        }
    }
}

/// Parse `sources` in parallel across a bounded std-thread pool, preserving order.
fn parse_sources_parallel(sources: &[(PathBuf, String)]) -> Vec<(PathBuf, FileSymbols)> {
    parallel_map(sources, |(p, src)| (p.clone(), extract_symbols(src)))
}

/// Map `f` over `items` across at most `available_parallelism` worker threads, returning
/// the results in input order. Dependency-free **work-stealing**: workers pull the next free
/// index off a shared atomic cursor, so a few heavy items (a big / deeply-generic source
/// file costs 20×+ a small one) can't strand a whole static chunk of light items behind them
/// on one straggler thread while the others sit idle. Falls back to a serial map for a small
/// input (the thread spawn isn't worth it) or a single core.
///
/// `pub(crate)` so the reference-index walk ([`crate::refs`]) can parallelize over files too.
pub(crate) fn parallel_map<T, R, F>(items: &[T], f: F) -> Vec<R>
where
    T: Sync,
    R: Send,
    F: Fn(&T) -> R + Sync,
{
    let n = items.len();
    let cores = std::thread::available_parallelism().map(|p| p.get()).unwrap_or(1);
    // Leave ~2 cores for the foreground: this runs on the background index thread, and
    // saturating every core starves bennu-be's RPC thread (go-to / completion stall) and the
    // UI shell — the walk must stay a background citizen, not peg the machine.
    let workers = cores.saturating_sub(2).max(1);
    // Serial for a small project / single core — the parse of a handful of files is faster
    // than spinning threads up.
    if workers <= 1 || n <= 32 {
        return items.iter().map(&f).collect();
    }
    let next = std::sync::atomic::AtomicUsize::new(0);
    let f = &f;
    let next = &next;
    let mut out: Vec<Option<R>> = (0..n).map(|_| None).collect();

    std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for _ in 0..workers.min(n) {
            handles.push(scope.spawn(move || {
                // Grab the next unclaimed index until the queue drains. `local` keeps each
                // result paired with its index so input order is restored on merge.
                let mut local: Vec<(usize, R)> = Vec::new();
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if i >= n {
                        break;
                    }
                    local.push((i, f(&items[i])));
                }
                local
            }));
        }
        for h in handles {
            for (i, r) in h.join().expect("parse worker panicked") {
                out[i] = Some(r);
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
/// patch so cross-file references still resolve). Cheap re-scan of just the type decls,
/// decoded in the project's declared `encoding_label` so a non-UTF-8 file's types still seed.
pub fn project_type_map(root: &Path, encoding_label: &str) -> BTreeMap<String, String> {
    let mut paths = Vec::new();
    collect_java(root, &mut paths);
    let mut map = BTreeMap::new();
    for p in paths {
        if let Some(decoded) = read_source_for_index(&p, encoding_label) {
            for td in extract_symbols(&decoded.text).types {
                map.insert(td.name, td.fqn.replace('.', "/"));
            }
        }
    }
    map
}

/// Recursively collect `.java` files under `dir`, skipping `target` / hidden dirs.
pub fn collect_java(dir: &Path, out: &mut Vec<PathBuf>) {
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) => {
            // A directory we can't read (permissions) would otherwise drop its whole
            // subtree of classes silently — log it so the gap is visible.
            eprintln!("bennu-intel: skipping unreadable directory {}: {e}", dir.display());
            return;
        }
    };
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
    fn read_source_for_index_recovers_non_utf8_and_flags_it() {
        let dir = std::env::temp_dir().join(format!("bennu-jidx-enc-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);

        // A file declared UTF-8 but written with a lone 0xE0 (Latin-1 'à') in a comment: not
        // valid UTF-8, so a plain `read_to_string` would drop it and hide `class Foo`. The
        // decode recovers it and flags it non-compliant — the class stays discoverable.
        let bad = dir.join("Foo.java");
        std::fs::write(&bad, b"// caff\xE0\nclass Foo {}\n").expect("write");
        let decoded = read_source_for_index(&bad, "UTF-8").expect("readable");
        assert!(decoded.non_compliant);
        assert_eq!(decl_line(&decoded.text, "Foo"), Some(2));

        let _ = std::fs::remove_file(&bad);
        let _ = std::fs::remove_dir_all(&dir);
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
