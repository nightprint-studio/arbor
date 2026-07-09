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
    extract_symbols, ClassFlags, ClassMembers, FileSymbols, Import, Member, MemberKind, MethodDecl,
    TypeDecl, TypeKind,
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
    /// The type-kind slug (`"class"` / `"interface"` / `"enum"` / `"record"` / `"annotation"`) — for
    /// the file-tree / navigator kind icons.
    pub kind: String,
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
    // reference resolves at ingest time (before the type's own file is processed). Also the FULL,
    // non-lossy set of project binaries — the simple→binary map keeps only ONE binary per simple name,
    // so it can't answer "is `com/x/Foo` a project type?" when several packages declare a `Foo`; the
    // set can, which is what lets a wildcard import (`import com.x.*;`) resolve to the right package.
    let mut project_types: BTreeMap<String, String> = BTreeMap::new();
    let mut project_binaries: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for (_p, fs) in &parsed {
        for td in &fs.types {
            let binary = td.fqn.replace('.', "/");
            project_types.insert(td.name.clone(), binary.clone());
            project_binaries.insert(binary);
        }
    }
    let is_project = |b: &str| project_binaries.contains(b);

    let mut builder = IndexBuilder::new(index_dir);
    let mut next_id: u32 = 0;
    let mut type_count = 0usize;
    let mut member_count = 0usize;
    let mut classes: Vec<ClassDecl> = Vec::new();

    for ((path, fs), (_p2, source)) in parsed.iter().zip(sources.iter()) {
        let (records, ids, types, members) =
            file_records(path, fs, &project_types, next_id, &is_project);
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
                kind: td.kind.slug().to_string(),
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
        Ok(bytes) => {
            // Normalize to LF so every index / validation byte offset agrees with the editor's LF
            // document (the interactive read `bennu_read_file` normalizes identically). Without this,
            // go-to targets and whole-project diagnostics on a CRLF file drift down by one position
            // per preceding line.
            let mut d = decode_for_index(&bytes, encoding_label);
            d.text = bennu_project::prelude::normalize_newlines(&d.text);
            Some(d)
        }
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
/// `pub` so the reference-index walk ([`crate::refs`]) and the be layer's parallel whole-project
/// validation can share this one work-stealing primitive (it deliberately leaves ~2 cores free for
/// the interactive path — the right "don't peg the machine" default for a background sweep).
pub fn parallel_map<T, R, F>(items: &[T], f: F) -> Vec<R>
where
    T: Sync,
    R: Send,
    F: Fn(&T) -> R + Sync,
{
    // `0` = the default background budget (leave ~2 cores for the foreground).
    parallel_map_capped(items, 0, f)
}

/// Like [`parallel_map`] but with an explicit worker cap. `max_workers == 0` uses the default
/// background budget (`available_parallelism − 2`); any other value is clamped to `[1, cores]`.
///
/// The whole-project validation passes a *gentler* cap (a user setting, default ≈ half the cores) so a
/// big sweep can't saturate every core and starve the interactive path (go-to / completion) and the UI
/// shell — the "don't peg the machine so hard the editor freezes" knob the user controls.
pub fn parallel_map_capped<T, R, F>(items: &[T], max_workers: usize, f: F) -> Vec<R>
where
    T: Sync,
    R: Send,
    F: Fn(&T) -> R + Sync,
{
    let n = items.len();
    let cores = std::thread::available_parallelism().map(|p| p.get()).unwrap_or(1);
    let workers = if max_workers == 0 {
        cores.saturating_sub(2).max(1)
    } else {
        max_workers.min(cores).max(1)
    };
    // Serial for a small project / single worker — the parse of a handful of files is faster
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
/// project-wide simple→binary map so cross-type references still resolve. `is_project` answers
/// "is this binary a project type?" so a wildcard import (`import pkg.*;`) can resolve a supertype /
/// parameter / field to the exact package — the caller wires it to the live resolver's project view.
pub fn file_records_from_source(
    path: &Path,
    source: &str,
    project_types: &BTreeMap<String, String>,
    start_id: u32,
    is_project: &dyn Fn(&str) -> bool,
) -> Vec<IndexRecord> {
    let fs = extract_symbols(source);
    file_records(path, &fs, project_types, start_id, is_project).0
}

/// Build one file's records; returns `(records, next_id, type_count, member_count)`.
/// `project_types` is the project-wide simple→binary type map so cross-type references
/// (a field of another project type) resolve to a binary name at ingest time. `is_project` tests a
/// candidate binary for project membership (wildcard-import resolution — see [`resolve_binary`]).
fn file_records(
    path: &Path,
    fs: &FileSymbols,
    project_types: &BTreeMap<String, String>,
    start_id: u32,
    is_project: &dyn Fn(&str) -> bool,
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

        let members = build_class_members(td, &fs.imports, project_types, is_project);
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
    is_project: &dyn Fn(&str) -> bool,
) -> ClassMembers {
    let superclass = td.extends.as_ref().map(|s| resolve_binary(s, imports, project_types, is_project));
    let interfaces = td
        .implements
        .iter()
        .map(|i| resolve_binary(i, imports, project_types, is_project))
        .collect();

    let mut methods: Vec<Member> = td
        .methods
        .iter()
        .map(|m| Member {
            name: m.name.clone(),
            kind: MemberKind::Method,
            return_type: type_text_to_ref(&m.return_type_text, imports, project_types, is_project),
            params: m
                .params
                .iter()
                .map(|p| type_text_to_ref(&p.type_text, imports, project_types, is_project))
                .collect(),
            is_static: m.is_static,
            // Carried from the source symbol model (an explicit `abstract` modifier or a bodyless
            // interface method → abstract; an interface `default` method → default). This lets the
            // extend-final / implement-abstract / functional-interface checks fire against
            // **project** supertypes too, not just bytecode ones.
            is_abstract: m.is_abstract,
            is_default: m.is_default,
            is_final: m.is_final,
            visibility: m.visibility,
            raw_signature: render_method(m),
            // Resolve each written `throws` type to a binary name (imports + project types), so a
            // call site can check an unhandled/undeclared checked exception against a project method.
            throws: m
                .throws
                .iter()
                .map(|t| resolve_binary(t, imports, project_types, is_project))
                .collect(),
        })
        .collect();

    let mut fields: Vec<Member> = td
        .fields
        .iter()
        .map(|f| Member {
            name: f.name.clone(),
            kind: MemberKind::Field,
            return_type: type_text_to_ref(&f.type_text, imports, project_types, is_project),
            params: Vec::new(),
            is_static: f.is_static,
            is_abstract: false,
            is_default: false,
            is_final: f.is_final,
            visibility: f.visibility,
            raw_signature: format!("{} {}", f.type_text, f.name),
            throws: Vec::new(),
        })
        .collect();

    // Lombok generated members: append the getters/setters/`log` its annotations would generate,
    // so they resolve like real declarations. A user-declared method of the same name suppresses
    // the synthetic one (the synth checks against the names already collected above).
    let existing_methods: std::collections::HashSet<String> =
        methods.iter().map(|m| m.name.clone()).collect();
    let synth = crate::lombok::synthesize(td, imports, project_types, &existing_methods, is_project);
    methods.extend(synth.methods);
    fields.extend(synth.fields);

    ClassMembers {
        superclass,
        interfaces,
        methods,
        fields,
        flags: class_flags(td),
        type_params: td.type_params.clone(),
    }
}

/// Map a source [`TypeDecl`] to the seam [`ClassFlags`] the inheritance / implement-abstract checks
/// read — the project-source counterpart to the bytecode-decoded flags. Interfaces + annotation
/// types are `is_interface` (and implicitly abstract); enums / records set their own bit (the
/// checks treat those as un-extendable directly, so no need to also force `is_final`).
fn class_flags(td: &TypeDecl) -> ClassFlags {
    let is_interface = matches!(td.kind, TypeKind::Interface | TypeKind::Annotation);
    ClassFlags {
        is_interface,
        is_abstract: is_interface || td.is_abstract,
        is_final: td.is_final,
        is_enum: matches!(td.kind, TypeKind::Enum),
        is_annotation: matches!(td.kind, TypeKind::Annotation),
        is_record: matches!(td.kind, TypeKind::Record),
        is_sealed: td.is_sealed,
    }
}

/// Resolve a supertype / throws simple name to a binary name (generics stripped).
///
/// Resolution order mirrors Java name lookup (JLS §6.5.5 / §7.5.1):
///   1. a fully-qualified name (`a.b.C`) wins;
///   2. an explicit single-type `import` of that simple name — it is authoritative and, unlike the
///      flat project map below, CANNOT be shadowed by a same-simple-name type in another package.
///      This precedence fix matters in large legacy code full of duplicate simple names (generated
///      `*Type` classes): a supertype/param/field whose name collides elsewhere resolved to whichever
///      binary the global map happened to keep — so a class's `extends`/`implements`, or a method's
///      parameter, could bind to the WRONG same-named type (its members then "missing", or its binary
///      unequal to the caller's) even though the file imports the right one;
///   3. the project-wide simple→binary map — a same-package type used without an import, or a
///      cross-file reference. Collision-prone for duplicate simple names, so it comes AFTER imports;
///   4. the **implicit `java.lang.*` import** (JLS §7.3), so a bare `Exception` / `RuntimeException` /
///      `Runnable` resolves to its `java/lang/…` binary instead of an unresolved bare word (without
///      it a project method's `throws Exception` stayed the raw `"Exception"`, never equal to the
///      resolved `java/lang/Exception` a call site sees → a false "does not permit `Exception`"). Only
///      a KNOWN java.lang name is mapped, so a genuine typo stays raw rather than a fake `java/lang/Typo`.
fn resolve_binary(
    simple: &str,
    imports: &[Import],
    project_types: &BTreeMap<String, String>,
    is_project: &dyn Fn(&str) -> bool,
) -> String {
    let base = simple.split('<').next().unwrap_or(simple).trim();
    if base.contains('.') {
        return base.replace('.', "/");
    }
    // An explicit single-type import wins over the collision-prone global project map.
    for imp in imports {
        if imp.simple_name() == Some(base) {
            return imp.path.replace('.', "/");
        }
    }
    // A non-static wildcard import that brings in a PROJECT type of this simple name pins its exact
    // package — the fix for a supertype (`extends`/`implements`) or a `throws` whose simple name
    // collides across packages (JAXB `*Type`) and would otherwise bind whichever binary the collapsed
    // map kept, mis-walking the hierarchy (a false `@Override`-overrides-nothing on an inherited method).
    for imp in imports {
        if imp.star && !imp.static_ {
            let candidate = format!("{}/{base}", imp.path.replace('.', "/"));
            if is_project(&candidate) {
                return candidate;
            }
        }
    }
    if let Some(b) = project_types.get(base) {
        return b.clone();
    }
    // `java.lang` is implicitly imported: a bare name that's a known java.lang type resolves there.
    if is_java_lang_implicit(base) {
        return format!("java/lang/{base}");
    }
    base.to_string()
}

/// Whether `name` is a public top-level `java.lang` type — implicitly imported (JLS §7.3), so a bare
/// use of it in a project's `extends` / `implements` / `throws` resolves to `java/lang/<name>`. Kept
/// to the ubiquitous exception hierarchy + the common interfaces/classes a project subclasses,
/// implements, or throws (the cases `resolve_binary` feeds). Deliberately a curated set, not every
/// java.lang class: a name NOT here stays raw (unresolvable) rather than risk mapping a typo /
/// same-package type to a fake `java/lang/…` binary. The resolver-backed checks additionally SKIP on
/// any still-unresolved throws entry, so a name this set happens to miss can never cause a false
/// positive — it only means that one check stays silent for it.
fn is_java_lang_implicit(name: &str) -> bool {
    matches!(
        name,
        // Throwable roots + the exception/error hierarchy (the `throws` case).
        "Throwable"
            | "Exception"
            | "Error"
            | "RuntimeException"
            | "InterruptedException"
            | "ClassNotFoundException"
            | "CloneNotSupportedException"
            | "NoSuchMethodException"
            | "NoSuchFieldException"
            | "IllegalAccessException"
            | "InstantiationException"
            | "ReflectiveOperationException"
            | "NullPointerException"
            | "IllegalArgumentException"
            | "IllegalStateException"
            | "IndexOutOfBoundsException"
            | "ArrayIndexOutOfBoundsException"
            | "StringIndexOutOfBoundsException"
            | "ClassCastException"
            | "NumberFormatException"
            | "UnsupportedOperationException"
            | "ArithmeticException"
            | "NegativeArraySizeException"
            | "ArrayStoreException"
            | "SecurityException"
            | "IllegalMonitorStateException"
            | "IllegalThreadStateException"
            | "EnumConstantNotPresentException"
            | "TypeNotPresentException"
            | "AssertionError"
            | "StackOverflowError"
            | "OutOfMemoryError"
            | "NoClassDefFoundError"
            | "ExceptionInInitializerError"
            | "VirtualMachineError"
            | "LinkageError"
            // Common interfaces/classes a project extends / implements.
            | "Object"
            | "Runnable"
            | "Comparable"
            | "Iterable"
            | "Cloneable"
            | "CharSequence"
            | "Appendable"
            | "AutoCloseable"
            | "Readable"
            | "Thread"
            | "Number"
            | "Enum"
            | "Record"
            | "String"
            | "ThreadLocal"
    )
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
    fn project_type_flags_and_abstract_methods_are_carried() {
        // An abstract project interface with a bodyless method → the resolved ClassMembers must
        // carry `is_interface` and an abstract method, so the implement-abstract check fires against
        // project supertypes (not only bytecode ones).
        let fs = extract_symbols("package p;\npublic interface Repo { void save(); }\n");
        let td = fs.types.iter().find(|t| t.name == "Repo").unwrap();
        let cm = build_class_members(td, &fs.imports, &BTreeMap::new(), &|_: &str| false);
        assert!(cm.flags.is_interface, "interface flag carried");
        assert!(cm.flags.is_abstract, "interface is implicitly abstract");
        let save = cm.methods.iter().find(|m| m.name == "save").unwrap();
        assert!(save.is_abstract, "bodyless interface method carried as abstract");

        // A `final` class → is_final; a plain class → no flags.
        let ff = extract_symbols("package p;\npublic final class Utils {}\n");
        let ftd = ff.types.iter().find(|t| t.name == "Utils").unwrap();
        assert!(build_class_members(ftd, &ff.imports, &BTreeMap::new(), &|_: &str| false).flags.is_final);
    }

    #[test]
    fn implicit_java_lang_throws_and_super_resolve_to_binaries() {
        // A project method's bare `throws Exception` must resolve to `java/lang/Exception` (java.lang
        // is implicitly imported) — NOT stay the raw word `Exception`. Otherwise the override-widening
        // / checked-exception checks compare a resolved `java/lang/Exception` against `Exception` and
        // misfire ("overridden method does not permit Exception") on perfectly legal code.
        let fs = extract_symbols(
            "package p;\npublic class Base { public void init() throws Exception {} }\n",
        );
        let td = fs.types.iter().find(|t| t.name == "Base").unwrap();
        let cm = build_class_members(td, &fs.imports, &BTreeMap::new(), &|_: &str| false);
        let init = cm.methods.iter().find(|m| m.name == "init").unwrap();
        assert_eq!(init.throws, vec!["java/lang/Exception".to_string()], "{:?}", init.throws);

        // A bare `extends RuntimeException` / `implements Runnable` resolve to their java.lang binary.
        let ex = extract_symbols("package p;\npublic class Boom extends RuntimeException {}\n");
        let etd = ex.types.iter().find(|t| t.name == "Boom").unwrap();
        let ecm = build_class_members(etd, &ex.imports, &BTreeMap::new(), &|_: &str| false);
        assert_eq!(ecm.superclass.as_deref(), Some("java/lang/RuntimeException"));

        let rn = extract_symbols("package p;\npublic class Job implements Runnable { public void run() {} }\n");
        let rtd = rn.types.iter().find(|t| t.name == "Job").unwrap();
        let rcm = build_class_members(rtd, &rn.imports, &BTreeMap::new(), &|_: &str| false);
        assert!(rcm.interfaces.iter().any(|i| i == "java/lang/Runnable"), "{:?}", rcm.interfaces);

        // A genuinely unknown bare name is NOT coerced to a fake `java/lang/…` — it stays raw so the
        // checks treat it as unresolved (SKIP), never as a real java.lang type.
        let tp = extract_symbols("package p;\npublic class C { public void m() throws Wibble {} }\n");
        let ctd = tp.types.iter().find(|t| t.name == "C").unwrap();
        let ccm = build_class_members(ctd, &tp.imports, &BTreeMap::new(), &|_: &str| false);
        assert_eq!(ccm.methods[0].throws, vec!["Wibble".to_string()], "unknown stays raw");
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
