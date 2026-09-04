//! Differential run against javac: same sources, both compilers, three ways to disagree.
//!
//! The langtools corpus scores what we SEE on a line javac complains about. It cannot score the
//! other half, because every file in it is meant to fail: a Bennu diagnostic anywhere else is
//! indistinguishable from javac's own cascade. This corpus is built the other way round — one
//! defect per directory, javac run over it to record the truth — so all three disagreements are
//! nameable:
//!
//! * `MISSED`   — javac errors on a line, Bennu says nothing there.
//! * `WRONG`    — Bennu marks the line, but with a check the [`bennu_check::javac`] table does not
//!   map that javac key to. A red squiggle in the right place with the wrong sentence on it.
//! * `FALSE+`   — Bennu errors on a line javac had no complaint about. The expensive kind: it is
//!   what makes an editor's validation something a user turns off.
//!
//! Generate the corpus first — `corpus/javac_diff_corpus.py` writes the cases and runs javac over
//! them — then:
//!
//! ```sh
//! python3 crates/products/bennu/intel/corpus/javac_diff_corpus.py /tmp/diffcorpus
//! cargo run -p bennu-intel --release --example javac_diff -- /tmp/diffcorpus [detail]
//! ```
//!
//! `dump <case>` prints one case in full instead — javac's transcript, then every Bennu diagnostic
//! with its severity — which is what to reach for once the report says a case disagrees.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use bennu_check::prelude::{check_file_resolved, coverage, Coverage, FileContext};
use bennu_classpath::prelude::{
    resolve_jdk_classpath, ClassMembers, MemberIndex, SourceMemberIndex,
};
use bennu_index::prelude::PersistedIndex;
use bennu_intel::prelude::build_project_index_from_sources;
use bennu_query::prelude::IndexResolver;

/// One disagreement, in the form the report groups by.
struct Finding {
    case: String,
    file: String,
    line: usize,
    /// The javac key, or `-` when javac said nothing here.
    key: String,
    /// The Bennu codes on that line, or `-` when Bennu said nothing.
    codes: String,
    kind: Kind,
    /// The source line itself — a report you can read without opening the corpus.
    text: String,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum Kind {
    Agreed,
    Missed,
    Wrong,
    FalsePositive,
    /// javac errors here and the table already declares the key unimplemented — a known gap, not
    /// a regression, and worth separating so the actionable list stays short.
    MissedDeclared,
}

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(root) = args.next() else {
        eprintln!("usage: javac_diff <corpus-dir> [detail]");
        std::process::exit(2);
    };
    let mode = args.next().unwrap_or_default();
    let detail = mode == "detail";
    // `dump <case>` prints every diagnostic of one case, whatever its severity, beside javac's own
    // transcript. The report says a case disagrees; this says what each side actually emitted.
    let dump = (mode == "dump").then(|| args.next().unwrap_or_default());

    let mut cases: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = fs::read_dir(&root) {
        for e in entries.flatten() {
            if e.path().join("expected.out").is_file() {
                cases.push(e.path());
            }
        }
    }
    cases.sort();
    eprintln!("cases: {}", cases.len());
    if cases.is_empty() {
        std::process::exit(1);
    }

    if let Err(why) = resolve_jdk_classpath("21") {
        eprintln!("no JDK ({why}) — a JDK-less run would score every library type as unresolved.");
        std::process::exit(1);
    }

    if let Some(want) = dump {
        let Ok(source) = resolve_jdk_classpath("21") else { return };
        let jdk = SourceMemberIndex::new(source);
        for dir in cases.iter().filter(|d| d.ends_with(&want)) {
            dump_case(dir, &jdk);
        }
        return;
    }

    let findings = run_all(cases);
    report(&findings, detail);
}

fn run_all(cases: Vec<PathBuf>) -> Vec<Finding> {
    let next = Arc::new(AtomicUsize::new(0));
    let cases = Arc::new(cases);
    let out: Arc<Mutex<Vec<Finding>>> = Arc::new(Mutex::new(Vec::new()));
    let threads = std::env::var("BENNU_LT_THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4));

    std::thread::scope(|scope| {
        for _ in 0..threads {
            let (next, cases, out) = (next.clone(), cases.clone(), out.clone());
            scope.spawn(move || {
                let Ok(source) = resolve_jdk_classpath("21") else { return };
                let jdk = SourceMemberIndex::new(source);
                let mut local = Vec::new();
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(dir) = cases.get(i) else { break };
                    local.extend(run_case(dir, &jdk));
                }
                out.lock().unwrap_or_else(|p| p.into_inner()).extend(local);
            });
        }
    });
    Arc::try_unwrap(out).map(|m| m.into_inner().unwrap()).unwrap_or_default()
}

struct BorrowedJdk<'a>(&'a dyn MemberIndex);

impl MemberIndex for BorrowedJdk<'_> {
    fn members_of(&self, binary_name: &str) -> Option<ClassMembers> {
        self.0.members_of(binary_name)
    }
}

fn run_case(dir: &Path, jdk: &dyn MemberIndex) -> Vec<Finding> {
    let case = dir.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let mut javas = Vec::new();
    collect(dir, "java", &mut javas);
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
    let (blob, fst) = (temp.path().join("symbols.blob"), temp.path().join("names.fst"));
    let Ok(persisted) = PersistedIndex::open(&blob, &fst) else { return Vec::new() };
    let mut resolver = IndexResolver::new(persisted, BorrowedJdk(jdk));
    for (simple, binary) in built.type_map.iter() {
        resolver.add_simple_hint(simple, binary);
    }

    // What javac said, by file and line.
    let expected = parse_golden(&dir.join("expected.out"), dir);
    let mut want: HashMap<(PathBuf, usize), Vec<String>> = HashMap::new();
    for (path, line, key) in expected {
        want.entry((path, line)).or_default().push(key);
    }

    let mut findings = Vec::new();
    for (path, text) in &sources {
        let ctx = FileContext {
            file_stem: path.file_stem().map(|s| s.to_string_lossy().into_owned()),
            expected_package: None,
            java_major: None,
            // Each case is a whole self-contained project compiled against a real JDK — the same
            // condition the gate exists to detect. Leaving it false silences the import checks and
            // scores their misses as gaps that are really the harness lying about the classpath.
            classpath_complete: true,
        };
        let diags = check_file_resolved(text, &ctx, &resolver, true);
        let starts = line_starts(text);
        let lines: Vec<&str> = text.lines().collect();
        let src = |line: usize| lines.get(line.saturating_sub(1)).map(|s| s.trim()).unwrap_or("").to_string();

        // Bennu's errors, by line. Warnings and weaker are excluded on purpose: a lint that javac
        // has no opinion about is not a false positive, it is the point of having an editor.
        let mut got: BTreeMap<usize, Vec<String>> = BTreeMap::new();
        for d in &diags {
            if d.severity != "error" {
                continue;
            }
            let code = if d.code.is_empty() { "<uncoded>".to_string() } else { d.code.clone() };
            got.entry(line_of(&starts, d.start)).or_default().push(code);
        }
        for codes in got.values_mut() {
            codes.sort();
            codes.dedup();
        }

        let file = path.strip_prefix(dir).unwrap_or(path).to_string_lossy().into_owned();
        let mut claimed: HashSet<usize> = HashSet::new();

        for ((wpath, line), keys) in want.iter() {
            if wpath != path {
                continue;
            }
            claimed.insert(*line);
            let errs: Vec<&String> = keys.iter().filter(|k| k.starts_with("compiler.err.")).collect();
            if errs.is_empty() {
                continue; // javac only warned here — Bennu marking it is legitimate.
            }
            let codes = got.get(line).cloned().unwrap_or_default();
            let joined = if codes.is_empty() { "-".to_string() } else { codes.join(",") };
            // Agreement is per-line, not per-key: any expected key the marks satisfy is enough.
            // A key the table declares un-covered is a known gap whether or not something else
            // happened to mark the line — calling that "the wrong check" would put a row in the
            // actionable list that no edit to the table could ever remove.
            let claimed = errs.iter().any(|k| matches!(coverage(k), Some(Coverage::Check(_))));
            let kind;
            if !claimed {
                kind = Kind::MissedDeclared;
            } else if codes.is_empty() {
                kind = Kind::Missed;
            } else if errs.iter().any(|k| match coverage(k) {
                Some(Coverage::Check(ids)) => codes.iter().any(|c| ids.iter().any(|id| id.code() == c)),
                _ => false,
            }) {
                kind = Kind::Agreed;
            } else {
                kind = Kind::Wrong;
            }
            findings.push(Finding {
                case: case.clone(),
                file: file.clone(),
                line: *line,
                key: errs.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("|"),
                codes: joined,
                kind,
                text: src(*line),
            });
        }

        for (line, codes) in got {
            if claimed.contains(&line) {
                continue;
            }
            findings.push(Finding {
                case: case.clone(),
                file: file.clone(),
                line,
                key: "-".to_string(),
                codes: codes.join(","),
                kind: Kind::FalsePositive,
                text: src(line),
            });
        }
    }
    findings
}

/// Print one case in full: javac's transcript, then every Bennu diagnostic with its severity.
fn dump_case(dir: &Path, jdk: &dyn MemberIndex) {
    println!("=== {} ===", dir.display());
    if let Ok(g) = fs::read_to_string(dir.join("expected.out")) {
        println!("--- javac ---\n{}", g.trim_end());
    }
    let mut javas = Vec::new();
    collect(dir, "java", &mut javas);
    javas.sort();
    let sources: Vec<(PathBuf, String)> = javas
        .iter()
        .filter_map(|p| fs::read_to_string(p).ok().map(|t| (p.clone(), t)))
        .collect();
    let Some(temp) = TempDir::new(dir) else { return };
    let built = build_project_index_from_sources(&sources, temp.path());
    if built.builder.persist().is_err() {
        return;
    }
    let (blob, fst) = (temp.path().join("symbols.blob"), temp.path().join("names.fst"));
    let Ok(persisted) = PersistedIndex::open(&blob, &fst) else { return };
    let mut resolver = IndexResolver::new(persisted, BorrowedJdk(jdk));
    for (simple, binary) in built.type_map.iter() {
        resolver.add_simple_hint(simple, binary);
    }
    println!("--- bennu ---");
    for (path, text) in &sources {
        let ctx = FileContext {
            file_stem: path.file_stem().map(|s| s.to_string_lossy().into_owned()),
            expected_package: None,
            java_major: None,
            classpath_complete: true,
        };
        let starts = line_starts(text);
        for d in check_file_resolved(text, &ctx, &resolver, true) {
            println!(
                "{}:{}: [{}] {} — {}",
                path.file_name().unwrap_or_default().to_string_lossy(),
                line_of(&starts, d.start),
                d.severity,
                if d.code.is_empty() { "<uncoded>" } else { &d.code },
                d.message
            );
        }
    }
}

fn report(findings: &[Finding], detail: bool) {
    let count = |k: Kind| findings.iter().filter(|f| f.kind == k).count();
    let (agreed, missed, declared, wrong, fp) = (
        count(Kind::Agreed),
        count(Kind::Missed),
        count(Kind::MissedDeclared),
        count(Kind::Wrong),
        count(Kind::FalsePositive),
    );
    let asserted = agreed + missed + declared + wrong;
    println!("javac error lines           : {asserted}");
    println!("  agreed (right check)      : {agreed}");
    println!("  marked, wrong check       : {wrong}");
    println!("  MISSED (table claims it)  : {missed}");
    println!("  missed (declared no-cover): {declared}");
    println!("FALSE POSITIVES             : {fp}");
    if asserted > 0 {
        let marked = findings
            .iter()
            .filter(|f| f.kind != Kind::FalsePositive && f.codes != "-")
            .count();
        println!("lines marked at all         : {marked} ({:.1}%)", 100.0 * marked as f64 / asserted as f64);
    }

    for (title, kind) in [
        ("FALSE POSITIVES — Bennu errors where javac is happy", Kind::FalsePositive),
        ("MISSED — javac errors, Bennu silent, table says we cover it", Kind::Missed),
        ("WRONG CHECK — marked, but not the check the key maps to", Kind::Wrong),
        ("MISSED (declared not covered yet)", Kind::MissedDeclared),
    ] {
        let mut rows: Vec<&Finding> = findings.iter().filter(|f| f.kind == kind).collect();
        if rows.is_empty() {
            continue;
        }
        rows.sort_by(|a, b| a.case.cmp(&b.case).then(a.line.cmp(&b.line)));
        println!("\n=== {title} ({}) ===", rows.len());
        if kind == Kind::MissedDeclared && !detail {
            let mut per_key: BTreeMap<&str, usize> = BTreeMap::new();
            for r in &rows {
                *per_key.entry(r.key.as_str()).or_insert(0) += 1;
            }
            let mut v: Vec<_> = per_key.into_iter().collect();
            v.sort_by(|a, b| b.1.cmp(&a.1));
            for (k, n) in v {
                println!("  {n:3}  {k}");
            }
            continue;
        }
        for r in rows {
            println!("  {:28} {}:{:<4} {} | bennu={} javac={}", r.case, r.file, r.line, r.text, r.codes, r.key);
        }
    }
}

/// Read a `-XDrawDiagnostics` transcript into (source, line, key).
fn parse_golden(out: &Path, dir: &Path) -> Vec<(PathBuf, usize, String)> {
    let Ok(text) = fs::read_to_string(out) else { return Vec::new() };
    let mut found = Vec::new();
    for line in text.lines() {
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
        let path = dir.join(name.trim());
        if !path.is_file() {
            continue;
        }
        found.push((path, line_no, format!("compiler.{key}")));
    }
    found
}

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

fn line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0usize];
    starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
    starts
}

fn line_of(starts: &[usize], offset: usize) -> usize {
    match starts.binary_search(&offset) {
        Ok(i) => i + 1,
        Err(i) => i,
    }
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(from: &Path) -> Option<Self> {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let tag = from.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let base = std::env::temp_dir().join(format!("bennu-diff-{}-{tag}-{n}", std::process::id()));
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
        let _ = fs::remove_dir_all(&self.0);
    }
}
