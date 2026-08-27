//! Runs Bennu's validation over the JDK's own javac test corpus and scores what it agrees with.
//!
//! `test/langtools/tools/javac` is ~1500 negative tests: a Java file that must NOT compile, beside a
//! golden `.out` recording the diagnostics javac raises for it, in `-XDrawDiagnostics` form:
//!
//! ```text
//! Parens2.java:13:9: compiler.err.not.stmt
//! ```
//!
//! A line, a javac diagnostic key, and the assertion that the two belong together. That is the
//! oracle a validation cannot produce for itself — a corpus of *correct* code only proves the
//! absence of false positives, and says nothing about what we fail to see.
//!
//! Two modes, because the corpus answers two different questions:
//!
//! * `pairs` — one row per javac key: how often Bennu was silent on its line, and which checks it
//!   raised when it wasn't. This is the EVIDENCE the [`bennu_check::javac`] table is built from: a
//!   key that lands on `not-a-statement` four hundred times is a mapping, not a guess.
//! * `report` (default) — the same run scored through that table: agreed, mapped-but-silent, or
//!   declared-not-covered.
//!
//! Each jtreg test directory is treated as its own small project, which is what jtreg itself does:
//! the corpus reuses class names across tests freely, so one index over all of it would have every
//! type declared a dozen times.
//!
//! Not a test — it needs a corpus that isn't in this repo:
//!
//! ```sh
//! git clone --depth 1 --filter=blob:none --sparse https://github.com/openjdk/jdk.git
//! cd jdk && git sparse-checkout set test/langtools/tools/javac
//! ```
//!
//! then `cargo run -p bennu-intel --release --example langtools -- <that path> [report|pairs]`.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use bennu_check::prelude::{check_file_resolved, FileContext};
use bennu_classpath::prelude::{
    resolve_jdk_classpath, ClassMembers, MemberIndex, SourceMemberIndex,
};
use bennu_index::prelude::PersistedIndex;
use bennu_intel::prelude::build_project_index_from_sources;
use bennu_query::prelude::IndexResolver;

/// One diagnostic a golden file asserts: which source, which line, which javac key.
struct Expected {
    source: PathBuf,
    /// 1-based, as javac reports it.
    line: usize,
    key: String,
}

/// What one scored expectation turned into: the javac key, and the Bennu codes on that line
/// (`-` when Bennu was silent there).
type Row = (String, String);

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(root) = args.next() else {
        eprintln!("usage: langtools <corpus-dir> [report|pairs]");
        std::process::exit(2);
    };
    let mode = args.next().unwrap_or_else(|| "report".to_string());

    let mut goldens = Vec::new();
    collect(Path::new(&root), "out", &mut goldens);
    eprintln!("golden files : {}", goldens.len());

    // Group by test DIRECTORY: one index per jtreg test, because the corpus reuses class names
    // across tests and a single index over all of it would declare every type many times over.
    let mut by_dir: HashMap<PathBuf, Vec<Expected>> = HashMap::new();
    for golden in &goldens {
        for e in parse_golden(golden) {
            let dir = golden.parent().unwrap_or(Path::new(".")).to_path_buf();
            by_dir.entry(dir).or_default().push(e);
        }
    }
    let total: usize = by_dir.values().map(Vec::len).sum();
    eprintln!("test dirs    : {}", by_dir.len());
    eprintln!("expectations : {total}");

    let dirs: Vec<(PathBuf, Vec<Expected>)> = by_dir.into_iter().collect();
    let rows = run_all(dirs);

    match mode.as_str() {
        "pairs" => emit_pairs(&rows),
        "report" => emit_report(&rows),
        other => {
            eprintln!("unknown mode `{other}` — expected `report` or `pairs`");
            std::process::exit(2);
        }
    }
}

/// Validate every test directory, across a thread per core.
///
/// Each directory is independent — its own index, its own temp dir — so this is a scatter with no
/// shared state but the output. Worth the threads: the corpus is ~1500 index builds, and a serial
/// run turns a coffee break into a lunch.
///
/// The JDK is opened once **per thread** rather than once overall: a `ClassSource` is not `Sync`,
/// and re-opening it per test directory would put a jimage open in the inner loop.
fn run_all(dirs: Vec<(PathBuf, Vec<Expected>)>) -> Vec<Row> {
    // Fail before the scatter rather than inside it: without a JDK every library type looks
    // unresolvable, which scores as agreement for entirely the wrong reason.
    if let Err(why) = resolve_jdk_classpath("21") {
        eprintln!("no JDK ({why}) — install one and re-run; a JDK-less score would be a lie.");
        std::process::exit(1);
    }
    let next = Arc::new(AtomicUsize::new(0));
    let dirs = Arc::new(dirs);
    let out: Arc<Mutex<Vec<Row>>> = Arc::new(Mutex::new(Vec::new()));
    let done = Arc::new(AtomicUsize::new(0));
    // A crash inside a check aborts the whole process, so when hunting one down `BENNU_LT_THREADS=1`
    // plus `BENNU_LT_TRACE=1` makes the last line printed the file that did it.
    let threads = std::env::var("BENNU_LT_THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4));
    let trace = std::env::var("BENNU_LT_TRACE").is_ok();

    std::thread::scope(|scope| {
        for _ in 0..threads {
            let (next, dirs, out, done) = (next.clone(), dirs.clone(), out.clone(), done.clone());
            let trace = trace;
            scope.spawn(move || {
                let Ok(source) = resolve_jdk_classpath("21") else { return };
                let jdk = SourceMemberIndex::new(source);
                let mut local = Vec::new();
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some((dir, wants)) = dirs.get(i) else { break };
                    if trace {
                        eprintln!("dir {i}: {}", dir.display());
                    }
                    local.extend(run_dir(dir, wants, &jdk, trace));
                    let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                    if n % 200 == 0 {
                        eprintln!("  {n}/{} dirs", dirs.len());
                    }
                }
                out.lock().unwrap_or_else(|p| p.into_inner()).extend(local);
            });
        }
    });
    Arc::try_unwrap(out).map(|m| m.into_inner().unwrap()).unwrap_or_default()
}

/// Lends one thread's JDK index to the per-directory resolver, which wants to own its member
/// source. Opening a JDK per test directory instead would put a jimage open in the inner loop.
struct BorrowedJdk<'a>(&'a dyn MemberIndex);

impl MemberIndex for BorrowedJdk<'_> {
    fn members_of(&self, binary_name: &str) -> Option<ClassMembers> {
        self.0.members_of(binary_name)
    }
}

/// Index one test directory and validate the sources its golden files have something to say about.
fn run_dir(dir: &Path, wants: &[Expected], jdk: &dyn MemberIndex, trace: bool) -> Vec<Row> {
    // NON-recursive: a jtreg test's sources are the files beside its golden, and its subdirectories
    // are OTHER tests. Recursing here made the corpus root a single "test" over four thousand files.
    let mut javas = siblings(dir, "java");
    javas.sort();
    if javas.is_empty() {
        return Vec::new();
    }
    let sources: Vec<(PathBuf, String)> = javas
        .iter()
        .filter_map(|p| fs::read_to_string(p).ok().map(|t| (p.clone(), t)))
        .collect();

    let Some(temp) = TempDir::new(dir) else { return Vec::new() };
    let built = build_project_index_from_sources(&sources, temp.path());
    if built.builder.persist().is_err() {
        return Vec::new();
    }
    let blob = temp.path().join("symbols.blob");
    let fst = temp.path().join("names.fst");
    let Ok(persisted) = PersistedIndex::open(&blob, &fst) else { return Vec::new() };
    let mut resolver = IndexResolver::new(persisted, BorrowedJdk(jdk));
    for (simple, binary) in built.type_map.iter() {
        resolver.add_simple_hint(simple, binary);
    }

    // One parse per source rather than one per expectation: a golden asserting forty diagnostics
    // on one file would otherwise validate it forty times.
    let mut per_source: HashMap<&PathBuf, Vec<&Expected>> = HashMap::new();
    for want in wants {
        per_source.entry(&want.source).or_default().push(want);
    }

    let mut rows = Vec::new();
    for (path, wants) in per_source {
        let Some((_, text)) = sources.iter().find(|(p, _)| p == path) else { continue };
        if trace {
            eprintln!("  file {}", path.display());
        }
        let ctx = FileContext {
            file_stem: path.file_stem().map(|s| s.to_string_lossy().into_owned()),
            // The corpus has no source roots and no pom — asking for a package match or a
            // classpath verdict would invent findings the golden file never claimed.
            expected_package: None,
            java_major: None,
            classpath_complete: false,
        };
        let diags = check_file_resolved(text, &ctx, &resolver, true);
        let starts = line_starts(text);
        let mut got: HashMap<usize, Vec<&str>> = HashMap::new();
        for d in &diags {
            let code = if d.code.is_empty() { "<uncoded>" } else { d.code.as_str() };
            got.entry(line_of(&starts, d.start)).or_default().push(code);
        }
        for want in wants {
            let mut codes = got.get(&want.line).cloned().unwrap_or_default();
            codes.sort_unstable();
            codes.dedup();
            let joined = if codes.is_empty() { "-".to_string() } else { codes.join(",") };
            rows.push((want.key.clone(), joined));
        }
    }
    rows
}

/// Aggregate the run per javac key — the evidence the mapping table is written from.
fn emit_pairs(rows: &[Row]) {
    let mut per_key: HashMap<&str, HashMap<&str, usize>> = HashMap::new();
    for (key, codes) in rows {
        *per_key.entry(key).or_default().entry(codes).or_insert(0) += 1;
    }
    let mut keys: Vec<_> = per_key.iter().map(|(k, v)| (*k, v)).collect();
    keys.sort_by(|a, b| {
        let (ca, cb): (usize, usize) = (a.1.values().sum(), b.1.values().sum());
        cb.cmp(&ca).then(a.0.cmp(b.0))
    });
    println!("javac_key\ttotal\tsilent\tbennu_codes_by_frequency");
    for (key, seen) in keys {
        let total: usize = seen.values().sum();
        let silent = seen.get("-").copied().unwrap_or(0);
        let mut hits: Vec<_> = seen.iter().filter(|(c, _)| **c != "-").collect();
        hits.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        let shown: Vec<String> = hits.iter().take(5).map(|(c, n)| format!("{c}×{n}")).collect();
        println!("{key}\t{total}\t{silent}\t{}", shown.join(" "));
    }
}

/// Score the run through the committed mapping.
fn emit_report(rows: &[Row]) {
    use bennu_check::prelude::{coverage, Coverage};
    let (mut agreed, mut silent, mut out_of_scope, mut not_yet, mut undeclared) = (0, 0, 0, 0, 0);
    let mut gaps: HashMap<&str, usize> = HashMap::new();
    for (key, codes) in rows {
        match coverage(key) {
            None => undeclared += 1,
            Some(Coverage::OutOfScope(_)) => out_of_scope += 1,
            Some(Coverage::Missing) => {
                not_yet += 1;
                *gaps.entry(key).or_insert(0) += 1;
            }
            Some(Coverage::Check(ids)) => {
                if codes.split(',').any(|c| ids.iter().any(|id| id.code() == c)) {
                    agreed += 1;
                } else {
                    silent += 1;
                    *gaps.entry(key).or_insert(0) += 1;
                }
            }
        }
    }
    // The number that answers "would I have been sent back to IntelliJ": did Bennu put ANY mark on
    // the line javac complains about? Naming the right check matters for quick-fixes and for
    // suppression, but a red squiggle in the right place is what stops a build from being the first
    // time you hear about the problem.
    let marked = rows.iter().filter(|(_, codes)| codes != "-").count();
    println!("expectations                      : {}", rows.len());
    println!("  Bennu marks that line at all    : {marked} ({:.1}%)", 100.0 * marked as f64 / rows.len().max(1) as f64);
    println!();
    let scored = agreed + silent;
    println!("scored (key is mapped to a check) : {scored}");
    println!("  agreed                          : {agreed}");
    println!("  mapped but Bennu was silent     : {silent}");
    println!("declared not covered yet          : {not_yet}");
    println!("declared out of an editor's reach : {out_of_scope}");
    println!("UNDECLARED — a gap in the table   : {undeclared}");
    if scored > 0 {
        println!("agreement where mapped            : {:.1}%", 100.0 * agreed as f64 / scored as f64);
    }
    let mut top: Vec<_> = gaps.into_iter().collect();
    top.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    println!("\nlargest gaps:");
    for (key, n) in top.iter().take(40) {
        println!("  {n:5}  {key}");
    }
}

/// Read one golden file into the diagnostics it asserts.
fn parse_golden(out: &Path) -> Vec<Expected> {
    let Ok(text) = fs::read_to_string(out) else { return Vec::new() };
    let dir = out.parent().unwrap_or(Path::new("."));
    let mut found = Vec::new();
    for line in text.lines() {
        // `Foo.java:13:9: compiler.err.not.stmt: args…`. Anything else — the `1 error` tally, a
        // `compiler.note.…` — is not a located diagnostic.
        let Some((locator, rest)) = line.split_once(": compiler.") else { continue };
        let Some(key) = rest.split(':').next() else { continue };
        if !key.starts_with("err.") && !key.starts_with("warn.") {
            continue;
        }
        let mut parts = locator.rsplitn(3, ':');
        let (Some(_col), Some(line_no), Some(name)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let Ok(line_no) = line_no.trim().parse::<usize>() else { continue };
        let Some(source) = resolve(dir, name.trim()) else { continue };
        found.push(Expected { source, line: line_no, key: format!("compiler.{key}") });
    }
    found
}

/// Find the `.java` a golden row names. Its path is written relative to wherever javac ran, which
/// is not the golden file's own directory for every test — so try the direct join, then the subtree.
fn resolve(dir: &Path, name: &str) -> Option<PathBuf> {
    let direct = dir.join(name);
    if direct.is_file() {
        return Some(direct);
    }
    // Only beside the golden: a same-named file in a sibling test is a different file, and
    // scoring against it would be scoring against the wrong source.
    let base = Path::new(name).file_name()?;
    siblings(dir, "java").into_iter().find(|p| p.file_name() == Some(base))
}

/// The files directly in `dir` with `ext` — no recursion.
fn siblings(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == ext))
        .collect()
}

/// Every file under `dir` with `ext`, recursively — for finding the golden files themselves.
fn collect(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, ext, out);
        } else if path.extension().is_some_and(|e| e == ext) {
            out.push(path);
        }
    }
}

/// Byte offset of each line start, to put a diagnostic's offset on javac's line.
fn line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0usize];
    starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
    starts
}

/// The 1-based line holding `offset`.
fn line_of(starts: &[usize], offset: usize) -> usize {
    match starts.binary_search(&offset) {
        Ok(i) => i + 1,
        Err(i) => i,
    }
}

/// A self-cleaning temp dir, named after the test it indexes so a leftover is traceable.
struct TempDir(PathBuf);

impl TempDir {
    fn new(from: &Path) -> Option<Self> {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let tag = from.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let base = std::env::temp_dir().join(format!("bennu-langtools-{}-{tag}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).ok()?;
        Some(TempDir(base))
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        // Best-effort: the index mmap may still hold the dir for a moment after the resolver drops.
        let _ = fs::remove_dir_all(&self.0);
    }
}
