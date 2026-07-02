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
pub fn build_project_index(
    root: &Path,
    index_dir: &Path,
) -> (IndexBuilder, usize, usize) {
    let mut paths = Vec::new();
    collect_java(root, &mut paths);

    // First pass: every declared simple name → binary name, so a same-project type
    // reference resolves at ingest time (before the type's own file is processed).
    let mut project_types: BTreeMap<String, String> = BTreeMap::new();
    let mut parsed: Vec<(PathBuf, FileSymbols)> = Vec::with_capacity(paths.len());
    for p in paths {
        if let Ok(src) = std::fs::read_to_string(&p) {
            let fs = extract_symbols(&src);
            for td in &fs.types {
                project_types.insert(td.name.clone(), td.fqn.replace('.', "/"));
            }
            parsed.push((p, fs));
        }
    }

    let mut builder = IndexBuilder::new(index_dir);
    let mut next_id: u32 = 0;
    let mut type_count = 0usize;
    let mut member_count = 0usize;

    for (path, fs) in &parsed {
        let (records, ids, types, members) = file_records(path, fs, &project_types, next_id);
        next_id = ids;
        type_count += types;
        member_count += members;
        builder.set_file(path.clone(), records);
    }

    (builder, type_count, member_count)
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
