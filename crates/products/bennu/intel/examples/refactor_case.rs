//! Applies every refactoring this crate offers, at every site of a real project, and compiles each
//! result with `javac`. An error javac reports that the file did not have before is a refactoring
//! that produced code the user could not have written.
//!
//! ## Why this and not more unit tests
//!
//! The unit tests here check a transformation against a source string somebody wrote by hand, which
//! means they check the shapes that were thought of. The shapes that break a refactoring are the
//! ones nobody thinks of — a lambda body, a `switch` expression, an anonymous class, a comment
//! sitting between two statements of the selection, a generic whose `<` the reindent walks past.
//! Real code has all of them and does not need to be invented.
//!
//! javac is the oracle because the question has an exact answer: the file compiled before and must
//! compile after. Every one of these refactorings is behaviour-preserving **by construction**, so
//! "still compiles" is not the whole of correctness — but every failure it reports is real, and a
//! refactoring that does not compile is the only kind users never forgive.
//!
//! ## Why the compile is per file
//!
//! Every refactoring in this crate is file-local: *extract method* adds a method to the class it
//! took the statements from, *extract constant* a field, and both inlines rewrite one body. So the
//! unit of judgement is one file, compiled against the rest of the project on `-sourcepath`, and
//! that is what makes the sweep affordable — a whole-project compile per plan would be minutes
//! apiece.
//!
//! It also means the corpus does not have to build. A file that already fails to compile is
//! **skipped**, not charged to the refactoring, so any pile of Java can be pointed at: the ones
//! whose dependencies are missing drop out on their own, and `cp=` brings them back if you have the
//! jars.
//!
//! ## The type a plan cannot name
//!
//! *Extract variable* and *extract constant* leave a [`TypeSlot`] for the caller's resolver. Here
//! there is no resolver, deliberately — this measures the transformation, not the inference, which
//! is measured where it lives. So the slot is filled the two honest ways:
//!
//! * a local gets `var`, which is what javac itself would infer, so the compile still judges the
//!   placement and the replacement rather than a guess about the type;
//! * a constant gets a type read off the literal, because *extract constant* only ever fires on a
//!   compile-time constant and `private static final var` is not a thing.
//!
//! A `var` that does not compile is a finding and not noise: it is exactly the expression whose type
//! cannot be written from the expression alone, and the refactoring should have said so.
//!
//! ```sh
//! cargo run -p bennu-refactor --release --example refactor_case -- <project-dir> \
//!     [files=N] [stride=N] [only=<refactoring-id>] [workers=N] [cp=<classpath>] [show]
//! ```

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use bennu_check::prelude::checked_exceptions_in;
use bennu_classpath::prelude::resolve_jdk_classpath;
use bennu_index::prelude::PersistedIndex;
use bennu_intel::prelude::{build_project_index_from_sources, declarable_type_detail, Declarable};
use bennu_java::prelude::{parse_java, TypeResolver};
use bennu_query::prelude::{IndexResolver, JdkMemberIndex};
use bennu_intentions::prelude::insert_import_edit;
use bennu_refactor::prelude::{
    merge_throws, refactorings_at, transfer_into, Plan, RefactorEdit, TypeNeed,
};
use std::sync::Arc;
use tree_sitter::{Node, Tree};

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(root) = args.next() else {
        eprintln!(
            "usage: refactor_case <project-dir> [files=N] [stride=N] [only=<id>] [workers=N] [level=N] [cp=<classpath>] [show]"
        );
        std::process::exit(2);
    };
    let rest: Vec<String> = args.collect();
    let opt = |k: &str| {
        rest.iter()
            .find_map(|a| a.strip_prefix(k).map(String::from))
    };
    let files_limit: usize = opt("files=")
        .and_then(|s| s.parse().ok())
        .unwrap_or(usize::MAX);
    let stride: usize = opt("stride=").and_then(|s| s.parse().ok()).unwrap_or(1);
    let only = opt("only=");
    // What this run compiles at, so a plan that needs a newer Java is refused here exactly as the
    // backend refuses it against the project's own level. `level=N` for a corpus judged older.
    let level: u16 = opt("level=").and_then(|s| s.parse().ok()).unwrap_or(21);
    let classpath = opt("cp=");
    let show = rest.iter().any(|a| a == "show");

    let root = PathBuf::from(&root);
    let mut javas = Vec::new();
    collect(&root, &mut javas);
    javas.sort();
    eprintln!("java files       : {}", javas.len());

    // A mirror, so nothing is ever written inside the corpus itself — this gets pointed at working
    // trees, and a harness that edits one in place is a harness nobody runs twice.
    let work = std::env::temp_dir().join(format!("bennu-refactor-{}", std::process::id()));
    let _ = fs::remove_dir_all(&work);

    // ONE MIRROR PER WORKER, and that is the whole reason this can run in parallel. A worker
    // rewrites the file it is judging and compiles it against its siblings on the `-sourcepath`;
    // share the tree and the siblings a second worker reads are whatever a third has half-applied.
    // A few megabytes of copies buys the machine's other nine cores.
    // One JVM per worker, each holding a compiler open — on this machine about a gigabyte apiece.
    // `available_parallelism()` is the right default on a machine nobody is using and the wrong one
    // on the machine somebody is working at: eighteen of them took the desktop with them, and the
    // workers the system reclaimed fell back to a fresh `javac` per compile, which is correct and
    // several times slower. `workers=N` is the way to leave the machine usable.
    let workers = opt("workers=")
        .and_then(|s| s.parse::<usize>().ok())
        .or_else(|| std::thread::available_parallelism().map(|n| n.get()).ok())
        .unwrap_or(4)
        .max(1);
    let mut mirrors: Vec<(PathBuf, Vec<PathBuf>)> = Vec::new();
    for w in 0..workers {
        let dir = work.join(format!("w{w}"));
        let files = mirror(&javas, &root, &dir);
        mirrors.push((dir, files));
    }
    let relative: Vec<PathBuf> = javas
        .iter()
        .map(|p| p.strip_prefix(&root).unwrap_or(p).to_path_buf())
        .take(files_limit)
        .collect();
    eprintln!("source roots     : {}", source_roots(&mirrors[0].1).len());
    // The resolver the product uses, built once and shared: the project's own index plus the JDK's
    // members. Without it this harness measures a refactoring nobody ships — see `fill`.
    let index_dir = work.join("_index");
    fs::create_dir_all(&index_dir).ok();
    let indexed: Vec<(PathBuf, String)> = javas
        .iter()
        .filter_map(|p| fs::read_to_string(p).ok().map(|t| (p.clone(), t)))
        .collect();
    let built = build_project_index_from_sources(&indexed, &index_dir);
    built.builder.persist().expect("persist the index");
    let jdk = resolve_jdk_classpath("21").expect("a JDK to resolve against");
    let persisted = PersistedIndex::open(built.builder.blob_path(), built.builder.fst_path())
        .expect("open the index");
    let mut index_resolver = IndexResolver::new(persisted, JdkMemberIndex::new(Box::new(jdk)));
    for (simple, binary) in built.type_map.iter() {
        index_resolver.add_simple_hint(simple, binary);
    }
    let resolver: Arc<dyn TypeResolver + Send + Sync> = Arc::new(index_resolver);
    eprintln!("resolver         : project index + JDK");

    let helper = build_compile_server(&work);
    eprintln!(
        "workers          : {workers} ({})",
        if helper.is_some() { "one JVM each, kept warm" } else { "a javac per compile" }
    );

    let shared = Mutex::new(Aggregate::default());
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);

    std::thread::scope(|scope| {
        for (dir, files) in &mirrors {
            let (shared, next, done) = (&shared, &next, &done);
            let (relative, only, classpath, helper) = (&relative, &only, &classpath, &helper);
            let resolver = Arc::clone(&resolver);
            scope.spawn(move || {
                let classes = dir.join("_classes");
                fs::create_dir_all(&classes).ok();
                // Simple type name → the file that declares it, which for a top-level type IS its
                // file name. Per mirror, because a worker may only ever touch its own copies.
                let types: BTreeMap<String, PathBuf> = files
                    .iter()
                    .filter_map(|p| {
                        p.file_stem().and_then(|s| s.to_str()).map(|s| (s.to_string(), p.clone()))
                    })
                    .collect();
                let cp = Compiler::new(
                    join_paths(&source_roots(files)),
                    classpath.clone(),
                    classes,
                    helper.as_deref(),
                );
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(rel) = relative.get(i) else { break };
                    let mut local = Aggregate::default();
                    sweep_file(
                        &dir.join(rel),
                        rel,
                        &cp,
                        stride,
                        only.as_deref(),
                        show,
                        resolver.as_ref(),
                        &types,
                        level,
                        &mut local,
                    );
                    shared.lock().expect("aggregate").absorb(local);
                    let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                    if n % 100 == 0 {
                        eprintln!("… {n}/{} files", relative.len());
                    }
                }
            });
        }
    });

    let Aggregate {
        stats,
        refused,
        reasons,
        failures,
        compiled,
        skipped,
    } = shared.into_inner().expect("aggregate");

    println!("\n=== refactorings over {} ===", root.display());
    println!("files compiled clean  : {compiled}");
    println!("files skipped (broken): {skipped}");
    println!();
    println!(
        "{:<20} {:>8} {:>8} {:>8} {:>9} {:>9}",
        "refactoring", "applied", "clean", "BROKEN", "untypable", "refused"
    );
    for (id, t) in &stats {
        let rate = if t.applied > 0 {
            100.0 * t.clean as f64 / t.applied as f64
        } else {
            100.0
        };
        println!(
            "{id:<20} {:>8} {:>8} {:>8} {:>9} {:>9}   {rate:.1}%",
            t.applied,
            t.clean,
            t.broken,
            t.untypable,
            refused.get(id).copied().unwrap_or(0)
        );
    }
    for (id, n) in &refused {
        if !stats.contains_key(id) {
            println!("{id:<20} {:>8} {:>8} {:>8} {:>9} {n:>9}", 0, 0, 0, 0);
        }
    }

    // The member moves carry their target in the id — `move-member:Builder` — which is what makes
    // the menu a choice and what turns this table into eighty rows of four applications each. The
    // roll-up is the number a person is actually asking for.
    let mut rolled: BTreeMap<String, Tally> = BTreeMap::new();
    for (id, t) in &stats {
        let family = id.split(':').next().unwrap_or(id).to_string();
        let mine = rolled.entry(family).or_default();
        mine.applied += t.applied;
        mine.clean += t.clean;
        mine.broken += t.broken;
        mine.untypable += t.untypable;
    }
    println!("\n=== by refactoring ===");
    for (family, t) in &rolled {
        let rate = if t.applied > 0 { 100.0 * t.clean as f64 / t.applied as f64 } else { 100.0 };
        println!(
            "{family:<22} {:>8} {:>8} {:>8} {:>9}   {rate:.1}%",
            t.applied, t.clean, t.broken, t.untypable
        );
    }

    if !reasons.is_empty() {
        println!("\n=== why they refused ===");
        // By family, for the same reason the roll-up above exists: one refusal seen from eighty
        // targets is one refusal.
        let mut byfamily: BTreeMap<(String, String), usize> = BTreeMap::new();
        for ((id, reason), n) in &reasons {
            let family = id.split(':').next().unwrap_or(id).to_string();
            *byfamily.entry((family, reason.clone())).or_default() += n;
        }
        let mut rows: Vec<_> = byfamily.iter().collect();
        rows.sort_by_key(|((id, _), n)| (id.clone(), std::cmp::Reverse(**n)));
        for ((id, reason), n) in rows {
            println!("  {n:>4}  {id:<22} {reason}");
        }
    }

    if !failures.is_empty() {
        // The shape of the failures before any of them: a hundred instances of one defect and a
        // hundred different defects are the same number and not remotely the same news, and a
        // printout capped at forty cannot tell you which you have.
        println!("\n=== what broke, by kind ===");
        let mut kinds: BTreeMap<(String, String), usize> = BTreeMap::new();
        for f in &failures {
            let first = f.javac.split(" | ").next().unwrap_or(&f.javac);
            // The variable's name is in the message and is not part of the kind.
            let kind = first.split(" for local variable ").next().unwrap_or(first);
            let family = f.id.split(':').next().unwrap_or(&f.id).to_string();
            *kinds.entry((family, kind.to_string())).or_default() += 1;
        }
        let mut rows: Vec<_> = kinds.iter().collect();
        rows.sort_by_key(|((id, _), n)| (id.clone(), std::cmp::Reverse(**n)));
        for ((id, kind), n) in rows {
            println!("  {n:>4}  {id:<18} {kind}");
        }

        println!(
            "\n=== {} refactorings that broke the file ===",
            failures.len()
        );
        for f in failures.iter().take(40) {
            println!("\n■ {}  {}:{}", f.id, f.file, f.line);
            println!("  selection: {}", f.selection);
            println!("  javac    : {}", f.javac);
            println!("  produced :");
            for l in f.applied.lines() {
                println!("      {l}");
            }
        }
        if failures.len() > 40 {
            println!("\n… and {} more", failures.len() - 40);
        }
    }

    let _ = fs::remove_dir_all(&work);
}

/// Judge one file: everything offered at every site, applied, compiled, and put back.
///
/// One file per call because that is the unit the compile judges, and because it is what makes the
/// sweep parallel — a worker owns its mirror and touches nothing another worker reads.
#[allow(clippy::too_many_arguments)]
fn sweep_file(
    file: &Path,
    rel: &Path,
    cp: &Compiler,
    stride: usize,
    only: Option<&str>,
    show: bool,
    resolver: &dyn TypeResolver,
    types: &BTreeMap<String, PathBuf>,
    level: u16,
    out: &mut Aggregate,
) {
    let Ok(source) = fs::read_to_string(file) else {
        return;
    };
    if cp.errors(file) > 0 {
        out.skipped += 1;
        return;
    }
    out.compiled += 1;

    let Some(tree) = parse_java(&source) else {
        return;
    };
    let sites = sites(&tree, &source, stride);

    for (start, end) in sites {
        for outcome in refactorings_at(&source, start, end) {
            let mut plan = match outcome {
                Ok(plan) => plan,
                Err(refusal) => {
                    *out.reasons
                        .entry((refusal.id.clone(), refusal.reason))
                        .or_default() += 1;
                    *out.refused.entry(refusal.id).or_default() += 1;
                    continue;
                }
            };
            if only.is_some_and(|id| id != plan.id) {
                continue;
            }
            if !fill(&mut plan, &source, resolver) {
                out.stats.entry(plan.id.clone()).or_default().untypable += 1;
                continue;
            }
            // A plan that reaches a SECOND file — a member pulled into a superclass, a nested
            // type given its own — is applied to both and both are judged. Skipping them would
            // score the half that stayed behind, which is the half that always compiles.
            let elsewhere = match spread(&mut plan, file, types, level) {
                Ok(spread) => spread,
                Err(reason) => {
                    let tally = out.stats.entry(plan.id.clone()).or_default();
                    tally.untypable += 1;
                    *out.reasons.entry((plan.id.clone(), reason)).or_default() += 1;
                    continue;
                }
            };
            let tally = out.stats.entry(plan.id.clone()).or_default();
            tally.applied += 1;

            let applied = plan.apply(&source);
            fs::write(file, &applied).ok();
            for other in &elsewhere {
                fs::write(&other.path, &other.after).ok();
            }
            let mut errors = cp.errors(file);
            let mut transcript = (errors > 0 || show).then(|| cp.transcript(file));
            // Each file the plan touched is compiled on its own: an error introduced in the
            // superclass is the whole point of measuring a move, and `-sourcepath` reports it only
            // when something drags that file in.
            for other in &elsewhere {
                let theirs = cp.errors(&other.path);
                if theirs > 0 && errors == 0 {
                    transcript = Some(cp.transcript(&other.path));
                }
                errors += theirs;
            }
            fs::write(file, &source).ok();
            for other in &elsewhere {
                match &other.before {
                    Some(text) => {
                        fs::write(&other.path, text).ok();
                    }
                    None => {
                        fs::remove_file(&other.path).ok();
                    }
                }
            }

            if errors > 0 {
                tally.broken += 1;
                out.failures.push(Failure {
                    id: plan.id.clone(),
                    file: rel.display().to_string(),
                    line: line_of(&source, start),
                    selection: excerpt(&source, start, end),
                    javac: first_errors(transcript.as_deref().unwrap_or("")),
                    applied: window(&applied, &plan),
                });
            } else {
                tally.clean += 1;
                if show {
                    println!(
                        "--- {} {}:{} ---\n{}",
                        plan.id,
                        rel.display().to_string(),
                        line_of(&source, start),
                        window(&applied, &plan)
                    );
                }
            }
        }
    }
}

/// A file the plan touches besides the one it was invoked in, with the text to put back after.
struct Elsewhere {
    path: PathBuf,
    /// What was there before, or `None` when the plan created the file.
    before: Option<String>,
    after: String,
}

/// Resolve the halves of a plan that live in another file, and fold their edits into it.
///
/// This is where the harness has to do **exactly** what the backend does: the transfer is resolved
/// with [`transfer_into`], which is the same function the backend calls, and the created file goes
/// beside the one that lost the type, which is the same rule. A harness that wrote its own version
/// of either would report on a product nobody ships.
///
/// `Err` is a refusal to measure, not a failure: a target this corpus does not hold is not
/// something the refactoring got wrong.
fn spread(
    plan: &mut Plan,
    file: &Path,
    types: &BTreeMap<String, PathBuf>,
    level: u16,
) -> Result<Vec<Elsewhere>, String> {
    let mut out = Vec::new();
    // The same check the backend makes, against the level this harness actually **compiles** at —
    // which is the honest level to judge against here, since that compile is the oracle. A plan
    // needing more than that would be refused in the editor too.
    if let Some(needs) = &plan.needs_level {
        if level < needs.at_least {
            return Err(format!("needs Java {}, this run compiles at {level}", needs.at_least));
        }
    }
    if let Some(transfer) = plan.transfer.clone() {
        let path = types
            .get(&transfer.target)
            .cloned()
            .ok_or_else(|| format!("`{}` is not a type of this corpus", transfer.target))?;
        let before = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let edits = transfer_into(plan, &before, &path.display().to_string())?;
        let mut after = before.clone();
        let mut edits = edits;
        edits.sort_by(|a, b| b.start.cmp(&a.start).then(b.end.cmp(&a.end)));
        for edit in &edits {
            let (s, e) = (edit.start.min(after.len()), edit.end.min(after.len()));
            after.replace_range(s..e, &edit.text);
        }
        out.push(Elsewhere { path, before: Some(before), after });
    }
    if let Some(created) = plan.new_source.clone() {
        let path = file
            .parent()
            .ok_or("the file has no folder")?
            .join(format!("{}.java", created.name));
        if path.exists() {
            return Err(format!("{}.java already exists in this package", created.name));
        }
        // The backend refuses a NESTED type that another file uses — it asks the reference index,
        // which knows that `import a.b.Outer.Inner;` two packages away is a use. There is no index
        // here, so the same question is asked of the text: does any other file write that name at
        // all. Stricter than the backend, never looser, so what this measures is a subset of what
        // ships rather than something it does not do. See `NewSource::was_nested`.
        if created.was_nested {
            if let Some(other) = mentions_elsewhere(&created.name, file, types) {
                return Err(format!("`{}` is used by {other}", created.name));
            }
        }
        out.push(Elsewhere { path, before: None, after: created.text });
    }
    Ok(out)
}

/// A file other than `mine` that writes `name` as a word, if any.
fn mentions_elsewhere(
    name: &str,
    mine: &Path,
    types: &BTreeMap<String, PathBuf>,
) -> Option<String> {
    let is_word = |line: &str| {
        line.match_indices(name).any(|(at, _)| {
            let before = line[..at].chars().next_back();
            let after = line[at + name.len()..].chars().next();
            !before.is_some_and(|c| c.is_alphanumeric() || c == '_')
                && !after.is_some_and(|c| c.is_alphanumeric() || c == '_')
        })
    };
    types
        .values()
        .filter(|p| p.as_path() != mine)
        .find(|p| fs::read_to_string(p).is_ok_and(|text| text.lines().any(is_word)))
        .map(|p| p.display().to_string())
}

/// What one worker found, and what the run's totals are made of.
#[derive(Default)]
struct Aggregate {
    stats: BTreeMap<String, Tally>,
    refused: BTreeMap<String, usize>,
    reasons: BTreeMap<(String, String), usize>,
    failures: Vec<Failure>,
    compiled: usize,
    skipped: usize,
}

impl Aggregate {
    fn absorb(&mut self, other: Aggregate) {
        for (id, t) in other.stats {
            let mine = self.stats.entry(id).or_default();
            mine.applied += t.applied;
            mine.clean += t.clean;
            mine.broken += t.broken;
            mine.untypable += t.untypable;
        }
        for (id, n) in other.refused {
            *self.refused.entry(id).or_default() += n;
        }
        for (k, n) in other.reasons {
            *self.reasons.entry(k).or_default() += n;
        }
        self.failures.extend(other.failures);
        self.compiled += other.compiled;
        self.skipped += other.skipped;
    }
}

#[derive(Default)]
struct Tally {
    applied: usize,
    clean: usize,
    broken: usize,
    /// Planned, but no type could be named for the slot the plan asked for.
    ///
    /// Two different things land here and both are correct outcomes. Where the slot is optional the
    /// plan kept `var` and was applied — these are the ones the harness could not type, and they
    /// measure the harness rather than the refactoring. Where it is REQUIRED the product itself
    /// declined, because `var` would have re-inferred the expression against nothing; a refusal is
    /// the answer there, so counting it as a failure would punish the fix for the failure.
    untypable: usize,
}

struct Failure {
    id: String,
    file: String,
    line: usize,
    selection: String,
    javac: String,
    applied: String,
}

/// One `javac` invocation, configured once.
struct Compiler {
    sourcepath: String,
    classpath: Option<String>,
    out: PathBuf,
    /// A JVM kept alive, compiling on request — see [`Compiler::server`].
    server: RefCell<Option<Server>>,
}

/// The long-lived compiler process and its pipes.
struct Server {
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
}

impl Drop for Compiler {
    fn drop(&mut self) {
        if let Some(mut server) = self.server.borrow_mut().take() {
            drop(server.stdin);
            let _ = server.child.wait();
        }
    }
}

impl Compiler {
    fn new(sourcepath: String, classpath: Option<String>, out: PathBuf, helper: Option<&Path>) -> Self {
        let me = Self { sourcepath, classpath, out, server: RefCell::new(None) };
        if let Some(helper) = helper {
            *me.server.borrow_mut() = me.start(helper);
        }
        me
    }

    /// Start the compile server, or `None` — in which case every compile falls back to `javac`.
    ///
    /// ## Why a server at all
    ///
    /// A compile here is milliseconds of work behind half a second of JVM startup, and the sweep
    /// does one per plan: on a real project that is hours of starting Java and minutes of compiling
    /// Java. The JDK's own `javax.tools.JavaCompiler` is the same compiler `javac` wraps, so a
    /// process that holds one open and takes file names on stdin does identical work without paying
    /// for the JVM again — and gets faster as it runs, because the JIT finally has something to warm
    /// up on.
    fn start(&self, helper: &Path) -> Option<Server> {
        let mut cmd = Command::new("java");
        cmd.arg("-cp")
            .arg(helper)
            .arg("CompileServer")
            .arg(&self.sourcepath)
            .arg(self.classpath.clone().unwrap_or_default())
            .arg(&self.out)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());
        let mut child = cmd.spawn().ok()?;
        let stdin = child.stdin.take()?;
        let stdout = BufReader::new(child.stdout.take()?);
        Some(Server { child, stdin, stdout })
    }

    fn run(&self, file: &Path) -> String {
        if let Some(server) = self.server.borrow_mut().as_mut() {
            if let Some(transcript) = Self::ask(server, file) {
                return transcript;
            }
        }
        self.run_javac(file)
    }

    /// One request/response with the server: a path in, diagnostics out, terminated by the sentinel.
    ///
    /// `None` on any protocol trouble, and the caller then falls back to a real `javac` — a harness
    /// that silently reported "no errors" because a pipe closed would be worse than a slow one.
    fn ask(server: &mut Server, file: &Path) -> Option<String> {
        writeln!(server.stdin, "{}", file.display()).ok()?;
        server.stdin.flush().ok()?;
        let mut out = String::new();
        loop {
            let mut line = String::new();
            if server.stdout.read_line(&mut line).ok()? == 0 {
                return None; // the server died
            }
            if line.starts_with("##END") {
                return Some(out);
            }
            out.push_str(&line);
        }
    }

    fn run_javac(&self, file: &Path) -> String {
        // `-implicit:none` so the siblings javac reads off the sourcepath are resolved but not
        // compiled: without it one file's compile drags in the transitive closure of the project.
        let mut cmd = Command::new("javac");
        cmd.arg("-nowarn")
            .arg("-proc:none")
            .arg("-implicit:none")
            .arg("-sourcepath")
            .arg(&self.sourcepath)
            .arg("-d")
            .arg(&self.out);
        if let Some(cp) = &self.classpath {
            cmd.arg("-cp").arg(cp);
        }
        cmd.arg(file);
        cmd.output()
            .map(|o| String::from_utf8_lossy(&o.stderr).into_owned())
            .unwrap_or_default()
    }

    fn errors(&self, file: &Path) -> usize {
        self.run(file).matches("error:").count()
    }

    fn transcript(&self, file: &Path) -> String {
        self.run(file)
    }
}

/// Write and compile the helper the compile server runs, once, and answer with its classpath dir.
///
/// `None` means the sweep runs on plain `javac` — slower, identical answers.
fn build_compile_server(work: &Path) -> Option<PathBuf> {
    let dir = work.join("_server");
    fs::create_dir_all(&dir).ok()?;
    let source = dir.join("CompileServer.java");
    fs::write(&source, COMPILE_SERVER_JAVA).ok()?;
    let out = Command::new("javac").arg("-d").arg(&dir).arg(&source).output().ok()?;
    out.status.success().then_some(dir)
}

/// The helper: one JVM, one compiler, a file name per line, diagnostics and a sentinel back.
const COMPILE_SERVER_JAVA: &str = r###"
import javax.tools.*;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.util.*;

public class CompileServer {
    public static void main(String[] args) throws Exception {
        String sourcepath = args[0], classpath = args[1], out = args[2];
        JavaCompiler javac = ToolProvider.getSystemJavaCompiler();
        if (javac == null) { System.exit(2); }
        BufferedReader in = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8));
        PrintStream reply = new PrintStream(new FileOutputStream(FileDescriptor.out), true, "UTF-8");
        String line;
        while ((line = in.readLine()) != null) {
            if (line.isEmpty()) continue;
            int errors = 0;
            StringBuilder sb = new StringBuilder();
            try {
                // A FRESH file manager each time: the one under test has just been rewritten on
                // disk, and a cached file object would compile the previous version.
                StandardJavaFileManager fm =
                    javac.getStandardFileManager(null, null, StandardCharsets.UTF_8);
                List<String> opts = new ArrayList<>(Arrays.asList(
                    "-nowarn", "-proc:none", "-implicit:none", "-sourcepath", sourcepath, "-d", out));
                if (!classpath.isEmpty()) { opts.add("-cp"); opts.add(classpath); }
                DiagnosticCollector<JavaFileObject> diags = new DiagnosticCollector<>();
                Iterable<? extends JavaFileObject> units = fm.getJavaFileObjects(new File(line));
                javac.getTask(new StringWriter(), fm, diags, opts, null, units).call();
                for (Diagnostic<? extends JavaFileObject> d : diags.getDiagnostics()) {
                    if (d.getKind() == Diagnostic.Kind.ERROR) {
                        errors++;
                        String where = d.getSource() == null ? "" : d.getSource().getName() + ":" + d.getLineNumber() + ": ";
                        sb.append(where).append("error: ")
                          .append(String.valueOf(d.getMessage(null)).replace('\n', ' ')).append('\n');
                    }
                }
                fm.close();
            } catch (Throwable t) {
                sb.append("error: compile server: ").append(t).append('\n');
                errors++;
            }
            reply.print(sb);
            reply.println("##END " + errors);
        }
    }
}
"###;

/// Every place worth asking "what can you do here", as a `(start, end)` pair.
///
/// A caret (`start == end`) where the user would put one — on an expression, on a name — and a
/// selection over whole statements, which is the only gesture *extract method* answers.
fn sites(tree: &Tree, source: &str, stride: usize) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    let mut stack = vec![tree.root_node()];
    while let Some(n) = stack.pop() {
        match n.kind() {
            // A caret in an expression: extract variable / extract constant.
            "method_invocation"
            | "binary_expression"
            | "object_creation_expression"
            | "string_literal"
            | "decimal_integer_literal"
            | "field_access"
            | "cast_expression"
            | "array_access"
            | "ternary_expression" => {
                out.push((n.start_byte(), n.start_byte()));
                // And the same expression SELECTED, which is the other way a user asks for it.
                out.push((n.start_byte(), n.end_byte()));
                // A call's name is where the caret sits for inline method.
                if let Some(name) = n.child_by_field_name("name") {
                    out.push((name.start_byte(), name.start_byte()));
                }
            }
            // A caret on a local's name: inline variable.
            "variable_declarator" => {
                if let Some(name) = n.child_by_field_name("name") {
                    out.push((name.start_byte(), name.start_byte()));
                }
            }
            // A caret on a member's or a type's own header: the member moves, and move class.
            "method_declaration"
            | "field_declaration"
            | "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration" => {
                out.push((n.start_byte(), n.start_byte()));
            }
            // Runs of whole statements: extract method.
            "block" => {
                let mut c = n.walk();
                let stmts: Vec<Node> = n
                    .named_children(&mut c)
                    .filter(|s| is_statement(s.kind()))
                    .collect();
                for i in 0..stmts.len() {
                    for len in [1usize, 2, 3] {
                        let Some(last) = stmts.get(i + len - 1) else {
                            continue;
                        };
                        out.push((stmts[i].start_byte(), last.end_byte()));
                    }
                }
            }
            _ => {}
        }
        let mut c = n.walk();
        for ch in n.named_children(&mut c) {
            stack.push(ch);
        }
    }
    out.sort_unstable();
    out.dedup();
    let _ = source;
    out.into_iter().step_by(stride.max(1)).collect()
}

fn is_statement(kind: &str) -> bool {
    kind.ends_with("_statement") || kind == "local_variable_declaration"
}

/// Fill the plan's type slot, or say the harness cannot — see the module docs on why the two cases
/// are filled differently.
/// Fill the slots a plan left for its caller — **exactly the way the backend does**.
///
/// This is the whole reason the harness lives in this crate. Without a resolver it filled a local's
/// type with `var` and a constant's by reading the literal, which measured neither the product nor
/// anything else: 706 of the failures on one library were `var` written where the backend would
/// have written a type or declined. With one, a run says what a user would see.
///
/// `false` means the plan would not be applied at all — which is a real answer, not a skipped case:
/// a slot the resolver cannot fill acceptably is a refusal in the product too. The three-way answer
/// below has to match `bennu-be`'s exactly, or the run measures a product that does not exist.
fn fill(plan: &mut Plan, source: &str, resolver: &dyn TypeResolver) -> bool {
    if let Some(slot) = plan.type_slot.clone() {
        match declarable_type_detail(source, slot.start, slot.end, resolver) {
            Declarable::Writable(written, needed) => {
                plan.fill_type(&written);
                // The backend adds the import as one more edit, so the measurement must too — a
                // type written without its import is a file that does not compile for a reason the
                // refactoring did not have.
                for fqn in needed {
                    if let Some(edit) = insert_import_edit(source, &fqn) {
                        plan.edits.push(RefactorEdit::new(
                            edit.start,
                            edit.end,
                            edit.replacement,
                            "import",
                        ));
                    }
                }
                plan.reorder();
            }
            // The backend keeps `var` where it is acceptable and refuses where it is not.
            Declarable::Unwritable
                if matches!(slot.need, TypeNeed::Required | TypeNeed::RequiredOnceInferred) =>
            {
                return false
            }
            Declarable::Unknown if matches!(slot.need, TypeNeed::Required) => return false,
            _ => plan.type_slot = None,
        }
    }
    if let Some(guard) = plan.type_guard.clone() {
        if let Declarable::Writable(inferred, _) =
            declarable_type_detail(source, guard.start, guard.end, resolver)
        {
            if inferred != guard.written {
                return false; // the backend refuses here; the harness has to do the same
            }
        }
    }
    if let Some(slot) = plan.throws_slot.clone() {
        let proven = checked_exceptions_in(source, slot.start, slot.end, resolver);
        plan.fill_throws(&merge_throws(&slot.placeholder, &proven.kinds, proven.complete, source));
    }
    true
}

/// The source roots of a set of files, read off each file's own `package` line — so `javac` can
/// resolve a sibling by name whatever the project's layout is.
fn source_roots(files: &[PathBuf]) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    for f in files {
        let Ok(text) = fs::read_to_string(f) else {
            continue;
        };
        let package = text.lines().find_map(|l| {
            l.trim()
                .strip_prefix("package ")
                .map(|p| p.trim_end_matches(';').trim().to_string())
        });
        let mut dir = f.parent().map(Path::to_path_buf).unwrap_or_default();
        if let Some(pkg) = package {
            for _ in pkg.split('.') {
                let Some(up) = dir.parent().map(Path::to_path_buf) else {
                    break;
                };
                dir = up;
            }
        }
        if !roots.contains(&dir) {
            roots.push(dir);
        }
    }
    roots
}

/// Copy every source into `dest`, keeping its path relative to `root`, and answer with the copies.
fn mirror(javas: &[PathBuf], root: &Path, dest: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for p in javas {
        let Ok(text) = fs::read_to_string(p) else {
            continue;
        };
        let target = dest.join(p.strip_prefix(root).unwrap_or(p));
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).ok();
        }
        if fs::write(&target, &text).is_ok() {
            out.push(target);
        }
    }
    out
}

fn join_paths(roots: &[PathBuf]) -> String {
    roots
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(":")
}

/// The first errors javac reported, on one line — enough to say what kind of wrong it is.
fn first_errors(transcript: &str) -> String {
    let lines: Vec<&str> = transcript
        .lines()
        .filter(|l| l.contains("error:"))
        .map(|l| l.rsplit("error:").next().unwrap_or(l).trim())
        .collect();
    let mut joined = lines
        .iter()
        .take(2)
        .cloned()
        .collect::<Vec<_>>()
        .join(" | ");
    if lines.len() > 2 {
        joined.push_str(&format!(" (+{} more)", lines.len() - 2));
    }
    joined
}

/// The region of the applied source the plan touched, with a little around it — what a unit test
/// would be written from.
fn window(applied: &str, plan: &Plan) -> String {
    let lo = plan.edits.iter().map(|e| e.start).min().unwrap_or(0);
    // The edits carry ORIGINAL offsets and this reads the APPLIED text, so the end of the touched
    // region has moved by everything the plan inserted. Without this the window cuts mid-line and
    // the printout — the thing a fix gets written from — lies about what was produced.
    let grew: usize = plan
        .edits
        .iter()
        .map(|e| e.text.len().saturating_sub(e.end - e.start))
        .sum();
    let hi = plan
        .edits
        .iter()
        .map(|e| e.start + e.text.len())
        .max()
        .unwrap_or(applied.len())
        + grew;
    let from = applied[..lo.min(applied.len())]
        .rfind('\n')
        .map(|i| i + 1)
        .unwrap_or(0);
    let to = applied[hi.min(applied.len())..]
        .find('\n')
        .map(|i| hi + i)
        .unwrap_or(applied.len())
        .min(applied.len());
    applied[from..to].to_string()
}

fn excerpt(source: &str, start: usize, end: usize) -> String {
    let text = if start == end {
        let to = source[start..]
            .find('\n')
            .map(|i| start + i)
            .unwrap_or(source.len());
        &source[start..to]
    } else {
        &source[start..end.min(source.len())]
    };
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.len() > 90 {
        format!("{}…", &flat[..90])
    } else {
        flat
    }
}

fn line_of(source: &str, offset: usize) -> usize {
    source[..offset.min(source.len())].matches('\n').count() + 1
}

fn short(p: &Path, root: &Path) -> String {
    p.strip_prefix(root).unwrap_or(p).display().to_string()
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        let name = e.file_name();
        let name = name.to_string_lossy();
        if p.is_dir() {
            if matches!(
                name.as_ref(),
                "target" | ".git" | "build" | "_classes" | "node_modules"
            ) {
                continue;
            }
            collect(&p, out);
        } else if p.extension().is_some_and(|x| x == "java") {
            out.push(p);
        }
    }
}
