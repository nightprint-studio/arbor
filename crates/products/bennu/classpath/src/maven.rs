//! [`resolve_maven_classpath`] — source a Maven project's dependency-jar bytecode
//! from `~/.m2` so member-access completion reaches framework/library types
//! (Spring, servlet, Hibernate, Struts…), not just the JDK + project sources.
//!
//! This layers in **behind the same** [`ClassSource`](crate::source::ClassSource) /
//! [`MultiSource`](crate::jdk::MultiSource) / member-index API as the JDK
//! bootclasspath (docs §10: "dep jars layer in behind `ClassSource`"). The JDK path in
//! [`crate::jdk`] is untouched — a project with no resolvable deps degrades exactly to
//! the JDK-only behavior.
//!
//! ## How a project becomes a dep-augmented member index
//!
//! ```no_run
//! use bennu_classpath::prelude::*;
//! use std::path::Path;
//!
//! // 1. The JDK bootclasspath for the project's language level (Phase 1).
//! let jdk = resolve_jdk_classpath("1.8").unwrap();
//!
//! // 2. The project's dependency jars, resolved via Maven and cached by pom mtime.
//! let mut cache = MavenClasspathCache::new();
//! let deps = cache
//!     .get(Path::new("/path/to/project"), &MavenResolveOpts::default())
//!     .unwrap();
//!
//! // 3. Layer deps behind the JDK into one source, then a member index.
//! let source = deps.augment(jdk);                 // JDK probed first, then dep jars
//! let index = SourceMemberIndex::new(source);
//! let members = index.members_of("javax/servlet/http/HttpServletRequest"); // now Some(_)
//! ```
//!
//! ## Partial failure is non-fatal (docs §8)
//!
//! Some deps live on a private repo and won't resolve; `dependency:build-classpath`
//! may exit non-zero yet still write the classpath it *could* resolve. We always read
//! the output file, collect existing jars as sources, and record non-existent entries
//! as [`MavenClasspath::unresolved`] — a normal "unresolved" state, never a hard error
//! (only a total absence of any output file on a failed run is surfaced as `Err`).
//!
//! ## Cost & caching
//!
//! `build-classpath` shells out to Maven (seconds). [`MavenClasspathCache`] keys the
//! resolved classpath on the pom's mtime, so a re-resolve within a session is free
//! until the pom changes.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

use crate::jdk::MultiSource;
use crate::source::{ClassSource, JarSource};

/// Options for running Maven's `dependency:build-classpath`.
#[derive(Debug, Clone)]
pub struct MavenResolveOpts {
    /// The Maven launcher: `"mvn"` (found on `PATH`) or an absolute path to
    /// `mvn`/`mvn.cmd`.
    pub mvn_path: String,
    /// `JAVA_HOME` to export for the Maven child, so the project's JDK (e.g. JDK 8) is
    /// used regardless of the ambient one. `None` inherits the environment.
    pub java_home: Option<PathBuf>,
    /// Run Maven **offline** (`-o`): resolve only from the local `~/.m2` cache — fast
    /// and deterministic, no network. Defaults to `true`; set `false` for a first-time
    /// resolve that may need to download.
    pub offline: bool,
    /// Maven scope to resolve (`-Dmdep.includeScope`), or `None` for **every** scope.
    ///
    /// `None` is the default and is what the *index* wants: completion and navigation should
    /// reach into test and provided classes, because you edit those too.
    ///
    /// A **launch** wants something narrower. `dependency:build-classpath` with no scope
    /// resolves test and provided along with the rest, so a run started from here gets a
    /// classpath the JVM would never see under `mvn spring-boot:run` — and that is not a
    /// cosmetic difference: a `@ConditionalOnClass` guarding a bean on the presence of a
    /// test-scoped library then fires in the IDE and not in production, which is the kind of
    /// divergence that costs an afternoon to find.
    pub scope: Option<String>,
    /// How long the `mvn` child may take before it is killed — see [`run_bounded`] for why there
    /// has to be a limit at all. Generous on purpose: a first resolve that really is downloading a
    /// large reactor is doing the work it was asked to do, and cutting that short would report a
    /// shortfall that is only "not finished yet".
    pub timeout: Duration,
    /// Resolve ONE module of the reactor rooted at `project_dir` (relative path, `services/core`),
    /// from inside the reactor — `-pl <module> -am` — and read only that module's classpath.
    ///
    /// `None` resolves `project_dir` as it stands and takes the union of every module's file (the
    /// index's view). A **launch** wants `Some`: resolved from inside the reactor, a dependency on a
    /// sibling module is answered with that sibling's `target/classes` rather than with a jar in
    /// `~/.m2` — which is stale after every edit, and absent until someone runs `mvn install`, in
    /// which case Maven resolves nothing at all for the module. See [`resolve_maven_classpath`].
    pub reactor_module: Option<String>,
}

impl Default for MavenResolveOpts {
    fn default() -> Self {
        Self {
            mvn_path: "mvn".to_string(),
            java_home: None,
            offline: true,
            scope: None,
            timeout: Duration::from_secs(300),
            reactor_module: None,
        }
    }
}

impl MavenResolveOpts {
    /// Start from defaults with an explicit Maven launcher.
    pub fn with_mvn(mvn_path: impl Into<String>) -> Self {
        Self { mvn_path: mvn_path.into(), ..Self::default() }
    }

    /// Set the `JAVA_HOME` exported to the Maven child (builder-style).
    pub fn java_home(mut self, home: impl Into<PathBuf>) -> Self {
        self.java_home = Some(home.into());
        self
    }

    /// Toggle offline (`-o`) resolution (builder-style).
    pub fn offline(mut self, offline: bool) -> Self {
        self.offline = offline;
        self
    }
}

/// The outcome of resolving a project's dependency classpath.
#[derive(Debug, Clone)]
pub struct MavenClasspath {
    /// Absolute paths to dep jars that exist on disk (openable as [`JarSource`]).
    pub jars: Vec<PathBuf>,
    /// Classpath entries Maven emitted that do NOT exist on disk (private-repo /
    /// unresolved deps). Non-fatal — kept for reporting.
    pub unresolved: Vec<PathBuf>,
    /// Whether `mvn` exited 0. `false` means partial (see [`unresolved`](Self::unresolved))
    /// — the resolved jars are still usable.
    pub mvn_ok: bool,
    /// Every entry that exists on disk — jars **and directories** — in Maven's classpath order.
    ///
    /// [`jars`](Self::jars) is the index's view and holds files only; a directory there is not a
    /// jar to open. A launch needs the directories too: resolved from inside a reactor, a sibling
    /// module arrives as its `target/classes`, and dropping it would drop the sibling. Directories
    /// also still appear in [`unresolved`](Self::unresolved), as they always have.
    pub entries: Vec<PathBuf>,
}

impl MavenClasspath {
    /// Count of dep jars that exist on disk and can be sourced.
    pub fn resolved_count(&self) -> usize {
        self.jars.len()
    }

    /// Count of classpath entries Maven emitted that are missing from disk.
    pub fn unresolved_count(&self) -> usize {
        self.unresolved.len()
    }

    /// Open each resolved jar as a [`JarSource`], skipping any that fail to open (a
    /// corrupt/unsupported jar must not sink the whole classpath — same policy as the
    /// JDK ext jars in [`crate::jdk`]). Returns the sources plus the count that failed
    /// to open.
    pub fn jar_sources(&self) -> (Vec<Box<dyn ClassSource>>, usize) {
        let mut sources: Vec<Box<dyn ClassSource>> = Vec::new();
        let mut open_failures = 0usize;
        for jar in &self.jars {
            match JarSource::open(jar) {
                Ok(src) => sources.push(Box::new(src)),
                Err(_) => open_failures += 1,
            }
        }
        (sources, open_failures)
    }

    /// Layer the dep jars **behind** an existing base source (typically the JDK
    /// bootclasspath from [`resolve_jdk_classpath`](crate::jdk::resolve_jdk_classpath))
    /// into one [`MultiSource`]. The base is probed **first** (the real JDK core wins
    /// over any shaded copy bundled in a dependency), then the dep jars in classpath
    /// order. Jars that fail to open are skipped.
    pub fn augment(&self, base: Box<dyn ClassSource>) -> MultiSource {
        let (dep_sources, _) = self.jar_sources();
        let mut all: Vec<Box<dyn ClassSource>> = Vec::with_capacity(1 + dep_sources.len());
        all.push(base);
        all.extend(dep_sources);
        MultiSource::new(all)
    }

    /// The dep jars alone as a [`MultiSource`], with no JDK base — for callers that
    /// chain the JDK elsewhere or want to inspect only dependency types.
    pub fn into_source(&self) -> MultiSource {
        let (dep_sources, _) = self.jar_sources();
        MultiSource::new(dep_sources)
    }
}

/// The per-module file `dependency:build-classpath` writes its classpath into. **Relative** on
/// purpose — see [`resolve_maven_classpath`].
const OUTPUT_FILE_NAME: &str = "bennu-classpath.txt";

/// The file a reactor-scoped resolve ([`MavenResolveOpts::reactor_module`]) writes. A name of its
/// own, so a launch resolving at `runtime` scope can never overwrite — or be read back as — the
/// every-scope file the index is reading at the same moment.
const REACTOR_OUTPUT_FILE_NAME: &str = "bennu-run-classpath.txt";

/// Run a child to completion, or kill it after `timeout`.
///
/// `Command::output()` waits for as long as the child feels like taking, and a `mvn` that is
/// waiting on a repository it cannot reach feels like taking forever. That is not one slow call:
/// the project's whole index build runs on one thread, in phases, and this is one of them — so a
/// hung resolve takes go-to-declaration, find-usages and rename down with it for the rest of the
/// session, while the class list, built in an earlier phase, keeps working. The symptom is
/// "navigation stopped working" and nothing anywhere names a Maven process as the reason.
///
/// stdout and stderr are drained by their own threads rather than read after the wait, because a
/// child that fills a pipe nobody is reading blocks in `write` — and a build log fills 64 KB long
/// before a reactor finishes.
fn run_bounded(mut cmd: Command, timeout: Duration) -> std::io::Result<std::process::Output> {
    use std::io::Read;
    use std::process::Stdio;

    let mut child = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    let drain = |mut pipe: Option<std::process::ChildStdout>| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(p) = pipe.as_mut() {
                let _ = p.read_to_end(&mut buf);
            }
            buf
        })
    };
    let out_thread = drain(child.stdout.take());
    let err_pipe = child.stderr.take();
    let err_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut p) = err_pipe {
            let _ = p.read_to_end(&mut buf);
        }
        buf
    });

    let started = std::time::Instant::now();
    let status = loop {
        match child.try_wait()? {
            Some(status) => break status,
            None if started.elapsed() >= timeout => {
                let _ = child.kill();
                let status = child.wait()?;
                eprintln!(
                    "bennu-classpath: mvn exceeded {}s and was stopped — the dependency classpath \
                     is whatever it had written by then",
                    timeout.as_secs()
                );
                break status;
            }
            // Long enough not to spin a core on a resolve that takes minutes, short enough that a
            // fast one is not noticeably delayed by the poll itself.
            None => std::thread::sleep(Duration::from_millis(50)),
        }
    };
    Ok(std::process::Output {
        status,
        stdout: out_thread.join().unwrap_or_default(),
        stderr: err_thread.join().unwrap_or_default(),
    })
}

/// Run `mvn dependency:build-classpath` for the project rooted at `project_dir`
/// (must contain a `pom.xml`) and collect the resolved dependency classpath.
///
/// Maven writes the classpath (an OS-separated list of jar paths) to a file under each module's
/// `target/`; we read those files rather than parse `-q` stdout, so log noise is irrelevant. A
/// non-zero Maven exit is **not** fatal on its own: as long as an output file was written (partial
/// resolution), it is read and its entries split into existing jars vs
/// [`MavenClasspath::unresolved`].
///
/// ## Multi-module: one file per module, then the union
///
/// `mdep.outputFile` used to be passed as an **absolute** path, and that quietly broke every
/// multi-module project. `build-classpath` runs once per module of the reactor, and every module
/// wrote to the *same* absolute file — so each overwrote the previous one and what survived was
/// whichever module Maven happened to build last. Worse, a reactor root is usually `<packaging>pom`
/// with no dependencies of its own, so the "resolved classpath" could end up essentially empty.
/// Opening a class in any other module then found none of its dependencies, and every library type
/// in it was reported unresolvable — thousands of errors on a project that compiles.
///
/// Passing a **relative** name makes Maven resolve it per-module, so each writes into its own
/// `target/`. We then read every file the run produced and take the **union**, deduplicated: the
/// index serves one project, and a type is either on some module's classpath or nowhere.
///
/// Stale files are removed across the whole tree first, so a module Maven fails on can't contribute
/// last session's answer.
///
/// A sibling module's own artifact may appear on another module's classpath — as `target/classes` (a
/// directory) or as its jar in `~/.m2`. Either is harmless: a directory fails to open as a jar and is
/// skipped, and the module's types are indexed from source anyway, which is the better tier.
///
/// ## One module, from inside the reactor
///
/// With [`MavenResolveOpts::reactor_module`] set, the goal runs at the reactor root as
/// `compile dependency:build-classpath -pl <module> -am`, with the compiler and resources skipped
/// (the launch has just compiled). The `compile` phase is there for Maven's reactor resolution:
/// before Maven 3.9 a sibling is answered with its `target/classes` only when the session includes
/// that phase, and otherwise falls through to `~/.m2`. Only the module's own file is read, so a
/// sibling's `<optional>` dependencies do not leak onto its classpath.
pub fn resolve_maven_classpath(
    project_dir: &Path,
    opts: &MavenResolveOpts,
) -> Result<MavenClasspath, String> {
    let pom = project_dir.join("pom.xml");
    if !pom.is_file() {
        return Err(format!("no pom.xml in {}", project_dir.display()));
    }
    if let Some(module) = opts.reactor_module.as_deref().filter(|m| !m.trim().is_empty()) {
        return resolve_reactor_module(project_dir, module.trim(), opts);
    }

    // Best-effort: clear every stale output under the tree so a module whose resolve fails this run
    // can't have last run's file read as a success.
    for stale in find_output_files(project_dir) {
        let _ = fs::remove_file(stale);
    }

    let mut cmd = Command::new(&opts.mvn_path);
    cmd.current_dir(project_dir)
        .arg("-q")
        .arg("dependency:build-classpath")
        // RELATIVE: resolved against each module's own basedir, so a reactor writes one file per
        // module instead of N modules racing to overwrite one path.
        .arg(format!("-Dmdep.outputFile=target/{OUTPUT_FILE_NAME}"))
        // Don't let one unresolvable artifact abort the reactor before writing.
        .arg("-Dmdep.ignoreMissing=true")
        .arg("--fail-never")
        .arg("--batch-mode");
    if let Some(scope) = &opts.scope {
        // Absent by default — the goal then resolves every scope, which is what the index wants.
        cmd.arg(format!("-Dmdep.includeScope={scope}"));
    }
    if opts.offline {
        cmd.arg("-o");
    }
    if let Some(jh) = &opts.java_home {
        cmd.env("JAVA_HOME", jh);
    }

    let output = run_bounded(cmd, opts.timeout)
        .map_err(|e| format!("spawn mvn ({}): {e}", opts.mvn_path))?;
    let mvn_ok = output.status.success();

    // Read every file the run produced, even on a non-zero exit: build-classpath commonly writes
    // the deps it *could* resolve before failing on a private-repo one.
    let produced = find_output_files(project_dir);
    if produced.is_empty() {
        // Nothing written anywhere → say what Maven actually said. The whole run goes to this
        // process's stderr as well: this path is rare, and it is exactly the moment somebody
        // wants the full log rather than three lines of it.
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!(
            "bennu-classpath: mvn dependency:build-classpath wrote nothing in {} (exit {:?})\n\
             ----- mvn stdout -----\n{stdout}\n----- mvn stderr -----\n{stderr}",
            project_dir.display(),
            output.status.code(),
        );
        return Err(format!(
            "mvn dependency:build-classpath wrote no classpath. {}",
            maven_failure_reason(&stdout, &stderr)
        ));
    }

    Ok(classpath_from(union_entries(&produced), mvn_ok))
}

/// [`resolve_maven_classpath`] for one module of a reactor — see "One module, from inside the
/// reactor" there.
fn resolve_reactor_module(
    root: &Path,
    module: &str,
    opts: &MavenResolveOpts,
) -> Result<MavenClasspath, String> {
    let module = module.replace('\\', "/").trim_matches('/').to_string();
    let output = root.join(&module).join("target").join(REACTOR_OUTPUT_FILE_NAME);
    // Last run's answer must not be read as this run's.
    let _ = fs::remove_file(&output);

    let mut cmd = Command::new(&opts.mvn_path);
    cmd.current_dir(root)
        .arg("-q")
        .arg("compile")
        .arg("dependency:build-classpath")
        .arg("-pl")
        .arg(&module)
        .arg("-am")
        // The launch compiled a moment ago: the phase is wanted, the work is not.
        .arg("-Dmaven.main.skip=true")
        .arg("-Dmaven.resources.skip=true")
        .arg(format!("-Dmdep.outputFile=target/{REACTOR_OUTPUT_FILE_NAME}"))
        .arg("-Dmdep.ignoreMissing=true")
        .arg("--fail-never")
        .arg("--batch-mode");
    if let Some(scope) = &opts.scope {
        cmd.arg(format!("-Dmdep.includeScope={scope}"));
    }
    if opts.offline {
        cmd.arg("-o");
    }
    if let Some(jh) = &opts.java_home {
        cmd.env("JAVA_HOME", jh);
    }

    let run = run_bounded(cmd, opts.timeout)
        .map_err(|e| format!("spawn mvn ({}): {e}", opts.mvn_path))?;
    let Ok(raw) = fs::read_to_string(&output) else {
        let stdout = String::from_utf8_lossy(&run.stdout);
        let stderr = String::from_utf8_lossy(&run.stderr);
        eprintln!(
            "bennu-classpath: reactor resolve of {module} wrote nothing in {} (exit {:?})\n\
             ----- mvn stdout -----\n{stdout}\n----- mvn stderr -----\n{stderr}",
            root.display(),
            run.status.code(),
        );
        return Err(format!(
            "mvn dependency:build-classpath -pl {module} -am wrote no classpath. {}",
            maven_failure_reason(&stdout, &stderr)
        ));
    };
    Ok(classpath_from(split_entries(&raw), run.status.success()))
}

/// Classify raw classpath entries into a [`MavenClasspath`].
fn classpath_from(raw_entries: Vec<String>, mvn_ok: bool) -> MavenClasspath {
    let entries: Vec<PathBuf> = raw_entries
        .iter()
        .map(|e| e.trim())
        .filter(|e| !e.is_empty())
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .collect();
    let (jars, unresolved) = classify_entries(raw_entries);
    MavenClasspath { jars, unresolved, mvn_ok, entries }
}

/// What Maven said went wrong, in one line fit for a notification.
///
/// ## Why stdout
///
/// Maven logs **everything** — `[ERROR]` included — to **stdout**; stderr is very nearly always
/// empty. Reading only stderr is why this used to report `exit Some(0)` with an empty tail on a
/// project whose `mvn clean package` was broken: the run really did exit 0 (`--fail-never` is
/// passed, so one module's failure must not abort the reactor), and the sentence naming the
/// failure was sitting in the stream nobody read.
///
/// ## Why the first error lines and not the last
///
/// A Maven failure ends with four lines of boilerplate — `-> [Help 1]`, the `-X` suggestion, the
/// wiki URL — so a *tail* is reliably the part that says nothing. The first `[ERROR]` line is the
/// one that names the goal and the project that failed.
fn maven_failure_reason(stdout: &str, stderr: &str) -> String {
    /// How many error lines make it into the notification. Enough for "Failed to execute goal X
    /// on project Y" plus the cause under it; past that it stops being readable in a toast.
    const KEEP: usize = 3;

    let errors: Vec<&str> = stdout
        .lines()
        .chain(stderr.lines())
        .filter_map(|l| {
            let l = l.trim();
            l.strip_prefix("[ERROR]").or_else(|| l.strip_prefix("[FATAL]")).map(str::trim)
        })
        .filter(|l| !l.is_empty() && !is_maven_boilerplate(l))
        .take(KEEP)
        .collect();
    if !errors.is_empty() {
        let said = errors.join(" | ");
        // A model-building failure is not a missing jar, and reporting it as one sends the reader
        // to look for an artifact that was never the problem. Maven never got as far as resolving
        // anything: it could not read the POMs, so NOTHING resolved, and the remedy is about the
        // parent it could not find rather than about the dependency the message happens to name.
        if is_model_failure(&said) {
            return format!(
                "Maven could not read this project's POMs, so it resolved nothing at all — this is                  not about a single artifact. {said}"
            );
        }
        return format!("Maven said: {said}");
    }

    // No tagged error at all — a launcher that printed a shell error, a JVM that refused to
    // start. Whatever came out is better than silence.
    let loose: Vec<&str> = stderr
        .lines()
        .chain(stdout.lines())
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .take(KEEP)
        .collect();
    if !loose.is_empty() {
        return format!("Maven said: {}", loose.join(" | "));
    }
    "Maven printed nothing — run `mvn dependency:build-classpath` in the project to see why."
        .to_string()
}

/// Whether Maven failed while **building the project model** rather than while resolving.
///
/// The distinction is the whole of what a reader needs: a resolution failure names an artifact and
/// is fixed by fetching it, while a model failure means Maven never read the project at all. The
/// commonest cause by far is a parent POM that is not in the local repository and cannot be
/// downloaded — which is exactly what an offline resolve produces on a project whose parent has
/// only ever been fetched by another tool.
fn is_model_failure(said: &str) -> bool {
    const MARKERS: [&str; 4] = [
        "while processing the POMs",
        "Non-resolvable parent POM",
        "Non-readable POM",
        "Non-resolvable import POM",
    ];
    MARKERS.iter().any(|m| said.contains(m))
}

/// The lines every Maven failure ends with, which say nothing about this one.
fn is_maven_boilerplate(line: &str) -> bool {
    line.starts_with("->")
        || line.starts_with("Re-run Maven")
        || line.starts_with("To see the full stack trace")
        || line.starts_with("For more information about the errors")
        || line.starts_with("http://")
        || line.starts_with("https://")
}

/// The deduplicated union of the classpath entries written in `files`, in first-seen order.
///
/// One `dependency:build-classpath` run over a reactor writes one file per module, and the same
/// third-party jar appears in most of them. Deduplicating here — *before* [`classify_entries`] pays
/// one `stat` per entry — keeps a wide reactor from restat'ing the same jar dozens of times. The order
/// is first-seen-stable because the resolver's decode memo is keyed on the jar set.
///
/// An unreadable file is skipped rather than failing the union: a module whose file we can't read
/// costs its own deps, not everybody else's.
fn union_entries(files: &[PathBuf]) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<String> = Vec::new();
    for file in files {
        let Ok(raw) = fs::read_to_string(file) else { continue };
        for entry in split_entries(&raw) {
            let entry = entry.trim().to_string();
            if !entry.is_empty() && seen.insert(entry.clone()) {
                out.push(entry);
            }
        }
    }
    out
}

/// Every `*/target/bennu-classpath.txt` under `root` (the root's own included), for a reactor of any
/// nesting depth.
///
/// A bounded walk that only ever descends into a directory that could hold a module: a `target/` is
/// entered just to read the file, and the usual noise dirs are skipped. Depth-capped because a
/// module tree is shallow by construction and an unbounded walk of a large repo to find a handful of
/// files would be the wrong trade.
fn find_output_files(root: &Path) -> Vec<PathBuf> {
    /// Deep enough for `root/group/subgroup/module/target/file`; deeper reactors are vanishingly rare.
    const MAX_DEPTH: usize = 6;
    let mut out = Vec::new();
    collect_output_files(root, MAX_DEPTH, &mut out);
    out.sort(); // deterministic union order regardless of filesystem enumeration
    out
}

fn collect_output_files(dir: &Path, depth_left: usize, out: &mut Vec<PathBuf>) {
    let candidate = dir.join("target").join(OUTPUT_FILE_NAME);
    if candidate.is_file() {
        out.push(candidate);
    }
    if depth_left == 0 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        if !ft.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // `target` is handled by the `candidate` probe above; the rest is noise a module never hides in.
        if matches!(name.as_ref(), "target" | ".git" | "node_modules" | ".idea" | "src") {
            continue;
        }
        collect_output_files(&entry.path(), depth_left - 1, out);
    }
}

/// Open a list of dependency jar paths as one [`MultiSource`], skipping any that fail to open (a
/// corrupt/unsupported jar must not sink the whole tier — same policy as [`MavenClasspath::jar_sources`]).
/// For a caller (e.g. the index service) that already knows the resolved jar paths — from a persisted
/// classpath cache — and wants a ready [`ClassSource`](crate::source::ClassSource) without re-running
/// Maven.
pub fn source_from_jars(jars: &[PathBuf]) -> MultiSource {
    let mut sources: Vec<Box<dyn ClassSource>> = Vec::new();
    for jar in jars {
        if let Ok(src) = JarSource::open(jar) {
            sources.push(Box::new(src));
        }
    }
    MultiSource::new(sources)
}

/// Split a build-classpath string into existing jars vs non-existent entries.
///
/// Test-only since the multi-module union took over the production path: real callers dedup across
/// modules first ([`union_entries`]) and then classify, so this composition survives only as the
/// single-string shorthand the classification tests are written against.
#[cfg(test)]
fn split_classpath(raw: &str) -> (Vec<PathBuf>, Vec<PathBuf>) {
    classify_entries(split_entries(raw))
}

/// Partition classpath entries into "exists on disk" (openable as a [`JarSource`]) and "doesn't"
/// (private-repo / unresolved deps, kept for reporting). Blank entries are dropped.
///
/// Split out from [`split_classpath`] so the multi-module union can dedup entries before paying one
/// `stat` each, while both paths still classify identically.
fn classify_entries<I: IntoIterator<Item = String>>(entries: I) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut jars = Vec::new();
    let mut unresolved = Vec::new();
    for entry in entries {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let p = PathBuf::from(entry);
        if p.is_file() {
            jars.push(p);
        } else {
            unresolved.push(p);
        }
    }
    (jars, unresolved)
}

/// Split a classpath string into entries. Windows uses `;` (unambiguous). A `:`
/// classpath (Unix) must NOT split a drive-letter colon (`C:\...`): a `:` is only a
/// separator when it is not the second char of a `<letter>:\` / `<letter>:/` drive
/// prefix at an entry boundary.
fn split_entries(raw: &str) -> Vec<String> {
    let raw = raw.trim();
    if raw.contains(';') {
        return raw.split(';').map(|s| s.to_string()).collect();
    }

    let bytes = raw.as_bytes();
    let mut out = Vec::new();
    let mut start = 0usize;
    for (i, &b) in bytes.iter().enumerate() {
        if b != b':' {
            continue;
        }
        let prev_is_letter = i >= 1 && bytes[i - 1].is_ascii_alphabetic();
        let letter_at_entry_start = i == start + 1; // entry begins "X:"
        let next_is_slash =
            i + 1 < bytes.len() && (bytes[i + 1] == b'\\' || bytes[i + 1] == b'/');
        if prev_is_letter && letter_at_entry_start && next_is_slash {
            continue; // drive letter, not a separator
        }
        out.push(raw[start..i].to_string());
        start = i + 1;
    }
    out.push(raw[start..].to_string());
    out
}

// ── caching by pom mtime ─────────────────────────────────────────────────────

/// A per-session cache of resolved Maven classpaths, keyed by the project's pom path **and the
/// scope**, invalidated when the pom's mtime changes. `build-classpath` costs seconds; this
/// makes a re-resolve within a session free until the pom is edited.
///
/// The scope is part of the key and that is not a detail: the same pom resolves to a *different*
/// classpath per scope, and one cache slot would mean whichever caller ran first decides what
/// everyone else gets — a launch resolving `runtime` would hand the index a classpath with the
/// test dependencies missing, and completion would stop working inside every test in the project.
#[derive(Default)]
pub struct MavenClasspathCache {
    entries: HashMap<(PathBuf, String), CacheEntry>,
}

struct CacheEntry {
    pom_mtime: SystemTime,
    classpath: MavenClasspath,
}

impl MavenClasspathCache {
    pub fn new() -> Self {
        Self { entries: HashMap::new() }
    }

    /// Resolve (or return the cached) dependency classpath for `project_dir`.
    /// Recomputes only when the pom's mtime differs from the cached one.
    pub fn get(
        &mut self,
        project_dir: &Path,
        opts: &MavenResolveOpts,
    ) -> Result<MavenClasspath, String> {
        let pom = project_dir.join("pom.xml");
        let mtime = fs::metadata(&pom)
            .and_then(|m| m.modified())
            .map_err(|e| format!("stat {}: {e}", pom.display()))?;
        let key = (pom.clone(), cache_slot(opts.scope.as_deref(), opts.reactor_module.as_deref()));

        if let Some(hit) = self.entries.get(&key) {
            if hit.pom_mtime == mtime {
                return Ok(hit.classpath.clone());
            }
        }

        let classpath = resolve_maven_classpath(project_dir, opts)?;
        self.entries
            .insert(key, CacheEntry { pom_mtime: mtime, classpath: classpath.clone() });
        Ok(classpath)
    }

    /// Whether a fresh (mtime-valid) entry is cached for this project **at `scope`** — `None`
    /// meaning the every-scope resolve, the same key [`Self::get`] uses.
    pub fn is_cached_at(&self, project_dir: &Path, scope: Option<&str>) -> bool {
        let pom = project_dir.join("pom.xml");
        let key = (pom.clone(), cache_slot(scope, None));
        match (self.entries.get(&key), fs::metadata(&pom).and_then(|m| m.modified())) {
            (Some(hit), Ok(mtime)) => hit.pom_mtime == mtime,
            _ => false,
        }
    }

    /// Whether the every-scope resolve is cached — the common question, and what every caller
    /// before scopes existed was asking.
    pub fn is_cached(&self, project_dir: &Path) -> bool {
        self.is_cached_at(project_dir, None)
    }
}

/// The cache slot for a (scope, reactor module) pair. A module resolved from inside the reactor is
/// a different classpath than the reactor's union, so it must never share a slot with it.
fn cache_slot(scope: Option<&str>, reactor_module: Option<&str>) -> String {
    match reactor_module.map(str::trim).filter(|m| !m.is_empty()) {
        Some(m) => format!("{}@{m}", scope.unwrap_or_default()),
        None => scope.unwrap_or_default().to_string(),
    }
}

#[cfg(test)]
mod tests {
    /// The reported case: an offline resolve on a project whose parent POM is not in `~/.m2`.
    /// Maven never reached resolution, so reporting it as a missing dependency sent the reader
    /// after an artifact that was never the problem.
    #[test]
    fn a_parent_pom_failure_says_nothing_resolved_rather_than_naming_a_jar() {
        let stderr = "[ERROR] Some problems were encountered while processing the POMs:\n                      [ERROR] Non-resolvable parent POM for it.acme:service:1.0: Could not find artifact\n                      [ERROR] -> [Help 2]\n";
        let reason = super::maven_failure_reason("", stderr);
        assert!(reason.contains("could not read this project's POMs"), "{reason}");
        assert!(reason.contains("resolved nothing at all"), "{reason}");
        // And it still carries Maven's own words — the parent's coordinates are the fix.
        assert!(reason.contains("Non-resolvable parent POM"), "{reason}");
    }

    /// An ordinary goal failure is untouched: it really is about the thing it names.
    #[test]
    fn a_goal_failure_is_reported_as_maven_said_it() {
        let stderr = "[ERROR] Failed to execute goal on project api: Could not resolve dependencies\n";
        let reason = super::maven_failure_reason("", stderr);
        assert!(reason.starts_with("Maven said:"), "{reason}");
    }

    use super::*;
    use crate::members::MemberIndex;

    // ── Pure classpath-splitting (no mvn / no deps needed) ───────────────────

    #[test]
    fn split_windows_classpath_keeps_drive_letters() {
        let raw = r"C:\a\x.jar;C:\b\y.jar";
        let entries = split_entries(raw);
        assert_eq!(entries, vec![r"C:\a\x.jar".to_string(), r"C:\b\y.jar".to_string()]);
    }

    #[test]
    fn split_unix_classpath() {
        let raw = "/home/u/.m2/a.jar:/home/u/.m2/b.jar";
        let entries = split_entries(raw);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0], "/home/u/.m2/a.jar");
    }

    #[test]
    fn split_unix_path_list_with_windows_drive_entries() {
        // A `:`-joined list whose entries carry Windows drive prefixes must not split
        // on the drive colon.
        let raw = r"C:\a\x.jar:C:\b\y.jar";
        let entries = split_entries(raw);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0], r"C:\a\x.jar");
        assert_eq!(entries[1], r"C:\b\y.jar");
    }

    #[test]
    fn nonexistent_entries_go_to_unresolved() {
        let raw = concat!(r"C:\definitely\missing\nope-1.0.jar", ";", r"C:\also\gone-2.0.jar");
        let (jars, unresolved) = split_classpath(raw);
        assert!(jars.is_empty());
        assert_eq!(unresolved.len(), 2);
    }

    /// A launch reads directories off the classpath — a sibling module resolved from inside the
    /// reactor is its `target/classes` — while the index's `jars` stay files only.
    #[test]
    fn entries_keep_directories_in_order_and_jars_stay_files() {
        let dir = std::env::temp_dir().join(format!("bennu-cp-entries-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let classes = dir.join("core/target/classes");
        fs::create_dir_all(&classes).unwrap();
        let jar = dir.join("lib.jar");
        fs::write(&jar, b"PK").unwrap();
        let missing = dir.join("gone.jar");

        let cp = classpath_from(
            vec![
                classes.display().to_string(),
                jar.display().to_string(),
                missing.display().to_string(),
            ],
            true,
        );
        assert_eq!(cp.entries, vec![classes.clone(), jar.clone()], "order kept, missing dropped");
        assert_eq!(cp.jars, vec![jar]);
        assert!(cp.unresolved.contains(&missing));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_reactor_module_resolve_has_its_own_cache_slot() {
        assert_eq!(cache_slot(Some("runtime"), None), "runtime");
        assert_eq!(cache_slot(Some("runtime"), Some("web")), "runtime@web");
        assert_eq!(cache_slot(None, Some("  ")), "", "a blank module is the reactor's union");
    }

    #[test]
    fn empty_entries_skipped() {
        let (jars, unresolved) = split_classpath(";;  ;");
        assert!(jars.is_empty());
        assert!(unresolved.is_empty());
    }

    // ── Multi-module discovery + union (no mvn needed) ────────────────────────

    /// Write `text` to `path`, creating parents.
    fn seed(path: &Path, text: &str) {
        let _ = fs::create_dir_all(path.parent().unwrap());
        fs::write(path, text).unwrap();
    }

    /// A reactor fixture: the root plus two modules (one nested) each carrying an output file, plus
    /// two decoys under directories the walk must not enter.
    fn reactor_fixture(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("bennu-reactor-{tag}"));
        let _ = fs::remove_dir_all(&root);
        seed(&root.join("pom.xml"), "<project/>");
        seed(&root.join("target").join(OUTPUT_FILE_NAME), "/m2/shared.jar:/m2/root.jar");
        seed(&root.join("api/target").join(OUTPUT_FILE_NAME), "/m2/shared.jar:/m2/api.jar");
        seed(&root.join("group/impl/target").join(OUTPUT_FILE_NAME), "/m2/impl.jar");
        // Decoys: a `target` inside `src` (skipped dir) and one under `.git`.
        seed(&root.join("src/main/java/target").join(OUTPUT_FILE_NAME), "/m2/decoy-src.jar");
        seed(&root.join(".git/x/target").join(OUTPUT_FILE_NAME), "/m2/decoy-git.jar");
        root
    }

    /// The bug this fixes: `dependency:build-classpath` runs once per reactor module, so the resolve
    /// has to read *every* module's file — reading one meant a module's deps were simply absent and
    /// every library type in it was unresolvable.
    #[test]
    fn find_output_files_collects_every_module() {
        let root = reactor_fixture("find");
        let found = find_output_files(&root);
        assert_eq!(found.len(), 3, "root + api + group/impl: {found:?}");
        assert!(found.windows(2).all(|w| w[0] <= w[1]), "sorted for a stable union: {found:?}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn find_output_files_skips_src_and_noise_dirs() {
        let root = reactor_fixture("noise");
        let found = find_output_files(&root);
        let joined = found.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(" ");
        assert!(!joined.contains("src"), "a target under src/ is not a module: {joined}");
        assert!(!joined.contains(".git"), "{joined}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn union_deduplicates_the_jar_shared_by_two_modules() {
        let root = reactor_fixture("union");
        let entries = union_entries(&find_output_files(&root));
        assert_eq!(
            entries.iter().filter(|e| e.ends_with("shared.jar")).count(),
            1,
            "root and api both list it: {entries:?}"
        );
        for expected in ["/m2/root.jar", "/m2/api.jar", "/m2/impl.jar"] {
            assert!(entries.iter().any(|e| e == expected), "missing {expected}: {entries:?}");
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn failure_reason_reads_maven_errors_off_stdout() {
        // The shape of a real run: `--fail-never` swallows the exit code, `-q` swallows INFO,
        // stderr is empty, and the whole story is on stdout.
        let stdout = "[ERROR] Failed to execute goal on project portal-web: Could not resolve \
                      dependencies for project it.acme:portal-web:war:1.0\n\
                      [ERROR] -> [Help 1]\n\
                      [ERROR] To see the full stack trace of the errors, re-run with -e.\n";
        let reason = maven_failure_reason(stdout, "");
        assert!(reason.starts_with("Maven said: "), "{reason}");
        assert!(reason.contains("portal-web"), "{reason}");
        assert!(!reason.contains("[Help 1]"), "boilerplate should be dropped: {reason}");
    }

    #[test]
    fn failure_reason_falls_back_to_whatever_was_printed() {
        // No `[ERROR]` tag at all — a launcher that failed before Maven ever logged anything.
        let reason = maven_failure_reason("", "env: java: No such file or directory\n");
        assert!(reason.contains("No such file or directory"), "{reason}");
    }

    #[test]
    fn failure_reason_says_so_when_maven_printed_nothing() {
        let reason = maven_failure_reason("", "");
        assert!(reason.contains("printed nothing"), "{reason}");
    }

    #[test]
    fn union_of_nothing_is_empty() {
        assert!(union_entries(&[]).is_empty());
        // A path that doesn't exist is skipped, not fatal.
        assert!(union_entries(&[PathBuf::from("/definitely/missing/bennu-classpath.txt")]).is_empty());
    }

    // ── Cache semantics (no mvn: exercised via a missing-pom project) ─────────

    #[test]
    fn cache_get_errors_without_pom_and_records_nothing() {
        let mut cache = MavenClasspathCache::new();
        let dir = std::env::temp_dir().join("bennu-no-pom-xyz");
        let _ = fs::create_dir_all(&dir);
        let opts = MavenResolveOpts::default();
        // No pom.xml → stat fails → Err, and nothing is cached.
        assert!(cache.get(&dir, &opts).is_err());
        assert!(!cache.is_cached(&dir));
    }

    // ── mvn-backed integration (skips gracefully when mvn/deps absent) ────────

    /// Resolve a real Maven project when `mvn` + a populated `~/.m2` are available;
    /// otherwise skip (the leaf crate must build/test on a machine without Maven).
    ///
    /// The project comes from `BENNU_TEST_MAVEN_PROJECT` and the JDK from
    /// `BENNU_TEST_JAVA_HOME`, because a checkout path is one machine's: hard-coded,
    /// this skipped silently everywhere else and looked like a passing test.
    #[test]
    fn maven_resolve_real_project_when_available() {
        let Ok(project) = std::env::var("BENNU_TEST_MAVEN_PROJECT") else {
            eprintln!("SKIP maven_resolve: BENNU_TEST_MAVEN_PROJECT not set");
            return;
        };
        let project = std::path::PathBuf::from(project);
        let project = project.as_path();
        if !project.join("pom.xml").is_file() {
            eprintln!("SKIP maven_resolve: no pom.xml at BENNU_TEST_MAVEN_PROJECT");
            return;
        }
        // Only attempt when a Maven launcher is discoverable — `mvn` on PATH, or
        // one named outright.
        let launcher = std::env::var("BENNU_TEST_MVN").unwrap_or_else(|_| "mvn".to_string());
        let mvn = [launcher.as_str(), "mvn"]
            .into_iter()
            .find(|p| *p == "mvn" || Path::new(p).is_file());
        let Some(mvn) = mvn else {
            eprintln!("SKIP maven_resolve: no mvn");
            return;
        };
        let mut opts = MavenResolveOpts::with_mvn(mvn);
        if let Ok(java_home) = std::env::var("BENNU_TEST_JAVA_HOME") {
            opts = opts.java_home(java_home);
        }

        let mut cache = MavenClasspathCache::new();
        let Ok(cp) = cache.get(project, &opts) else {
            eprintln!("SKIP maven_resolve: resolve failed (mvn/deps unavailable)");
            return;
        };
        // Partial or full, we should have at least one dep jar to source.
        assert!(cp.resolved_count() > 0, "expected some resolved dep jars");
        assert!(cache.is_cached(project), "second get should be a cache hit");

        // A servlet type on the classpath must now resolve members.
        let source = cp.into_source();
        let idx = crate::members::SourceMemberIndex::new(source);
        if let Some(req) =
            idx.members_of("javax/servlet/http/HttpServletRequest")
        {
            assert!(
                req.methods.iter().any(|m| m.name == "getHeader"),
                "HttpServletRequest.getHeader should be present"
            );
        } else {
            eprintln!("note: servlet-api not in this ~/.m2; dep sourcing still exercised");
        }
    }
}
