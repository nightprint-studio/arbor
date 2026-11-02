//! What the three measures share: where the public corpus lives, how a held-out file becomes a project the index can
//! open, and the small statistics helpers. Public code only — see `docs/bennu-ml-bench.md` for the rules.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use arbor_ipc::prelude::EventSink;
use serde_json::Value;

use crate::index_service::IndexService;

pub(super) struct Quiet;
impl EventSink for Quiet {
    fn emit(&self, _topic: &str, _payload: Value) {}
}

pub(super) fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

pub(super) fn env_num(name: &str, default: usize) -> usize {
    env(name).and_then(|v| v.parse().ok()).unwrap_or(default)
}

/// The bench lab: `BENNU_MLB_LAB` (default: the sibling `bennu-flc` folder of the workspace).
pub(super) fn lab() -> PathBuf {
    env("BENNU_MLB_LAB").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/Users/christian/sviluppo/mio/workspace/rust/apps/bennu-flc"))
}

pub(super) fn corpus() -> PathBuf {
    lab().join("corpus")
}

pub(super) fn eval_dir() -> PathBuf {
    env("BENNU_MLB_EVAL").map(PathBuf::from).unwrap_or_else(|| lab().join("data2/eval"))
}

/// The public base model: `data8`, whose SOURCES.txt lists public repositories only (`data2` is an older byte-level model
/// whose folder has no vocabulary). Override with `BENNU_FLC_MODEL`.
pub(super) fn model_dir() -> PathBuf {
    env("BENNU_FLC_MODEL").map(PathBuf::from).unwrap_or_else(|| lab().join("data8"))
}

/// Where projects are assembled and results written.
pub(super) fn work_dir() -> PathBuf {
    let dir = env("BENNU_MLB_WORK").map(PathBuf::from).unwrap_or_else(|| lab().join("ml_bench/work"));
    let _ = std::fs::create_dir_all(&dir);
    isolate_home(&dir);
    dir
}

/// The index the bench builds is persisted by Bennu under the user's data folder (`$HOME/Library/Application Support/arbor`
/// on macOS). Pointing `HOME` at `work/cache/home` keeps the bench's indexes in the cache folder — reopened by the next run,
/// out of the real profile, deleted with one `rm -r`. `BENNU_MLB_REAL_HOME=1` leaves `HOME` alone.
fn isolate_home(work: &Path) {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        if env("BENNU_MLB_REAL_HOME").is_some() {
            return;
        }
        // a collection child: its parent prepared the home, which must not be wiped or shared
        if let Some(home) = env("BENNU_MLB_HOME_OVERRIDE") {
            std::env::set_var("HOME", home);
            return;
        }
        let home = work.join("cache/home");
        let _ = std::fs::create_dir_all(&home);
        if env("BENNU_MLB_REBUILD").is_some() {
            let _ = std::fs::remove_dir_all(&home);
            let _ = std::fs::create_dir_all(&home);
        }
        std::env::set_var("HOME", &home);
    });
}

pub(super) fn rebuild() -> bool {
    env("BENNU_MLB_REBUILD").is_some()
}

/// Bumped when the assembled layout or what is copied changes: part of every cache key. The indexer's own version is the
/// crate's (`CARGO_PKG_VERSION`) plus its persisted-schema constants, which Bennu validates itself when it reopens an index.
const LAYOUT_VERSION: u32 = 1;

pub(super) fn fnv(h: &mut u64, bytes: &[u8]) {
    for b in bytes {
        *h ^= *b as u64;
        *h = h.wrapping_mul(0x100000001b3);
    }
}

/// Hash of a folder's java files: relative path + size (a dependency tree is large and only changes by being replaced).
pub(super) fn tree_fingerprint(dir: &Path, h: &mut u64) {
    let mut files = Vec::new();
    fn walk(d: &Path, out: &mut Vec<(PathBuf, u64)>) {
        for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() { walk(&p, out) } else if p.extension().is_some_and(|x| x == "java") { out.push((p.clone(), e.metadata().map_or(0, |m| m.len()))) }
        }
    }
    walk(dir, &mut files);
    files.sort();
    for (p, len) in files {
        fnv(h, p.strip_prefix(dir).unwrap_or(&p).to_string_lossy().as_bytes());
        fnv(h, &len.to_le_bytes());
    }
}

/// The cache key of a folder that was assembled from `inputs`.
pub(super) fn cache_key(inputs: impl FnOnce(&mut u64)) -> String {
    let mut h = 0xcbf29ce484222325u64;
    fnv(&mut h, env!("CARGO_PKG_VERSION").as_bytes());
    fnv(&mut h, &LAYOUT_VERSION.to_le_bytes());
    inputs(&mut h);
    format!("{h:016x}")
}

/// Whether `root` holds a folder assembled for `key`; when not (or when a rebuild is asked for) it is emptied and the caller
/// assembles again. The marker is written by [`mark_assembled`] once the copy is complete, so an interrupted copy is redone.
pub(super) fn needs_assembly(root: &Path, key: &str) -> bool {
    let fresh = rebuild() || std::fs::read_to_string(root.join(".assembled")).map_or(true, |k| k.trim() != key);
    if fresh {
        let _ = std::fs::remove_dir_all(root);
    }
    fresh
}

pub(super) fn mark_assembled(root: &Path, key: &str) {
    let _ = std::fs::create_dir_all(root);
    let _ = std::fs::write(root.join(".assembled"), key);
}

/// The held-out files of one source (`commons-lang__12.java` → source `commons-lang`).
pub(super) fn eval_files() -> BTreeMap<String, Vec<PathBuf>> {
    let mut out: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    let mut files: Vec<PathBuf> = std::fs::read_dir(eval_dir()).into_iter().flatten().flatten().map(|e| e.path()).collect();
    files.sort();
    for f in files {
        let name = f.file_name().unwrap().to_string_lossy().into_owned();
        out.entry(name.split("__").next().unwrap_or("").to_string()).or_default().push(f);
    }
    out
}

/// A project to open: held-out files + the dependency sources they would see on a classpath.
pub(super) struct ProjectSpec {
    pub name: &'static str,
    pub sources: &'static [&'static str],
    /// Source folders of public libraries (relative to the corpus) the held-out code depends on.
    pub deps: &'static [&'static str],
}

pub(super) const PROJECTS: &[ProjectSpec] = &[
    ProjectSpec { name: "commons-lang", sources: &["commons-lang"], deps: &[] },
    ProjectSpec { name: "commons-text", sources: &["commons-text"], deps: &["commons-lang/src/main/java"] },
    ProjectSpec {
        name: "spring-reactive",
        sources: &["spring-framework-webflux", "spring-framework-r2dbc", "tutorials-webflux"],
        deps: &[
            "spring-framework/spring-core/src/main/java",
            "spring-framework/spring-beans/src/main/java",
            "spring-framework/spring-context/src/main/java",
            "spring-framework/spring-web/src/main/java",
            "spring-framework/spring-tx/src/main/java",
            "spring-framework/spring-jdbc/src/main/java",
            "reactor-core/reactor-core/src/main/java",
        ],
    },
];

fn copy_java_tree(from: &Path, to: &Path) {
    let Ok(entries) = std::fs::read_dir(from) else { return };
    for e in entries.flatten() {
        let (p, name) = (e.path(), e.file_name().to_string_lossy().into_owned());
        if p.is_dir() {
            copy_java_tree(&p, &to.join(&name));
        } else if name.ends_with(".java") {
            let _ = std::fs::create_dir_all(to);
            let _ = std::fs::copy(&p, to.join(&name));
        }
    }
}

fn declared_path(text: &str, fallback: &str) -> PathBuf {
    let pkg = text.lines().find_map(|l| l.trim().strip_prefix("package ").map(|r| r.trim_end_matches(';').trim().replace('.', "/"))).unwrap_or_default();
    let class = text
        .lines()
        .find_map(|l| {
            let t = l.trim_start();
            ["public class ", "public final class ", "public abstract class ", "public interface ", "public enum ", "public record ", "public sealed interface ", "public sealed class "]
                .iter()
                .find_map(|k| t.strip_prefix(k))
                .map(|r| r.split(|c: char| !(c.is_alphanumeric() || c == '_')).next().unwrap_or("").to_string())
        })
        .filter(|c| !c.is_empty())
        .unwrap_or_else(|| fallback.trim_end_matches(".java").to_string());
    Path::new(&pkg).join(format!("{class}.java"))
}

/// A held-out file as it sits in the assembled project.
pub(super) struct Held {
    pub path: PathBuf,
    pub text: String,
    pub source: String,
}

/// Assembles `spec` under the work folder and returns its root and its held-out files. Reused as it is when the key — the
/// held-out files' contents, the dependency trees' file lists and sizes, the layout and indexer versions — has not changed.
pub(super) fn assemble(spec: &ProjectSpec) -> (PathBuf, Vec<Held>) {
    let started = std::time::Instant::now();
    let root = work_dir().join("projects").join(spec.name);
    let all = eval_files();
    let key = cache_key(|h| {
        for s in spec.sources {
            for f in all.get(*s).into_iter().flatten() {
                fnv(h, f.file_name().unwrap().to_string_lossy().as_bytes());
                fnv(h, &std::fs::read(f).unwrap_or_default());
            }
        }
        for dep in spec.deps {
            fnv(h, dep.as_bytes());
            tree_fingerprint(&corpus().join(dep), h);
        }
    });
    let mut held = Vec::new();
    let fresh = needs_assembly(&root, &key);
    if fresh {
        for dep in spec.deps {
            copy_java_tree(&corpus().join(dep), &root.join("deps").join(dep.replace('/', "_")));
        }
    }
    for s in spec.sources {
        for f in all.get(*s).into_iter().flatten() {
            let bytes = std::fs::read(f).unwrap_or_default();
            let text = String::from_utf8_lossy(&bytes).into_owned();
            let rel = declared_path(&text, &f.file_name().unwrap().to_string_lossy());
            let dest = root.join("held").join(format!("{s}")).join(rel);
            if fresh {
                let _ = std::fs::create_dir_all(dest.parent().unwrap());
                let _ = std::fs::write(&dest, &text);
            }
            held.push(Held { path: dest, text, source: s.to_string() });
        }
    }
    if fresh {
        mark_assembled(&root, &key);
    }
    record_timing(spec.name, "assemble", started.elapsed(), if fresh { "built" } else { "reused" });
    (root, held)
}

/// One line per phase, appended to `results/timing.tsv`: project, phase, milliseconds, whether the cache was used.
pub(super) fn record_timing(project: &str, phase: &str, took: Duration, how: &str) {
    use std::io::Write;
    let dir = work_dir().join("results");
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("timing.tsv")) {
        let profile = if cfg!(debug_assertions) { "debug" } else { "release" };
        let _ = writeln!(f, "{project}\t{phase}\t{}\t{how}\t{profile}", took.as_millis());
    }
    eprintln!("[timing] {project}: {phase} {} ms ({how})", took.as_millis());
}

/// Where Bennu persists the index of `root` (the same hash `IndexService` names it by).
fn index_base(root: &str) -> PathBuf {
    let mut h = 0xcbf29ce484222325u64;
    fnv(&mut h, root.as_bytes());
    arbor_core::prelude::bennu_data_dir().join("index").join(format!("{h:016x}"))
}

/// Opens `root` in the index and waits until the resolver and the semantic engine answer. Timed in three phases: parsing
/// every source once (what any index has to do first), the symbol index until the provider answers, and the semantic
/// engine's reference walk until navigation is up. `cold` = no persisted index existed for the root.
pub(super) fn open_and_wait(root: &Path) {
    let name = root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let root = root.to_string_lossy().into_owned();
    // `BENNU_MLB_COLD_PROJECT=1`: forget this project's index only. The JDK member memo is shared by every project and
    // persists between sessions, so a project opened for the first time in daily use meets a warm JDK; `REBUILD` wipes that too.
    if env("BENNU_MLB_COLD_PROJECT").is_some() {
        let _ = std::fs::remove_dir_all(index_base(&root));
    }
    let cold = !index_base(&root).exists();
    let how = format!("{}{}", if cold { "cold" } else { "warm" }, env("BENNU_MLB_WORKERS").map(|w| format!(" workers={w}")).unwrap_or_default());
    let how = how.as_str();
    // parse-only cost: the floor under any index of these files
    let t = Instant::now();
    let mut files = Vec::new();
    collect_java(Path::new(&root), &mut files);
    for f in &files {
        if let Ok(text) = std::fs::read_to_string(f) {
            let _ = bennu_java::prelude::parse_java(&text);
        }
    }
    record_timing(&name, &format!("parse_only ({} files)", files.len()), t.elapsed(), "n/a");
    // the indexing CPU budget (`index_threads`: 1 = serial, the shipped default; 0 = every core but two), written only into the
    // bench's own isolated profile
    if let (Some(n), None) = (env("BENNU_MLB_WORKERS").and_then(|v| v.parse::<usize>().ok()), env("BENNU_MLB_REAL_HOME")) {
        let mut cfg = bennu_core::config::load();
        cfg.index_threads = n;
        let _ = bennu_core::config::save(&cfg);
    }
    let svc = IndexService::global();
    let started = Instant::now();
    svc.open(&root, &env("BENNU_QUALITY_JDK").unwrap_or_else(|| "21".into()), "UTF-8", Arc::new(Quiet));
    while !svc.has_resolver(&root) {
        assert!(started.elapsed() < Duration::from_secs(1800), "the index did not come up in 30 minutes");
        std::thread::sleep(Duration::from_millis(100));
    }
    record_timing(&name, "index_symbols", started.elapsed(), how);
    let sem = Instant::now();
    while !svc.has_semantic_engine(&root) && sem.elapsed() < Duration::from_secs(600) {
        std::thread::sleep(Duration::from_millis(100));
    }
    record_timing(&name, "semantic_engine", sem.elapsed(), how);
}

fn collect_java(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() { collect_java(&p, out) } else if p.extension().is_some_and(|x| x == "java") { out.push(p) }
    }
}

/// 64-bit FNV-1a of `name` and `line`: the draw for a line depends on nothing but its place (the lab's `FLC_LINE_SEEDED`).
pub(super) fn line_seed(file: &str, line_no: usize) -> u64 {
    let rng = file.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3));
    (rng ^ line_no as u64).wrapping_mul(0x100000001b3).wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407)
}

pub(super) fn pct(x: usize, of: usize) -> f64 {
    if of == 0 { 0.0 } else { 100.0 * x as f64 / of as f64 }
}

/// Wilson-free, honest-enough: the half-width of a 95% normal interval for a proportion, in points.
pub(super) fn ci95(x: usize, of: usize) -> f64 {
    if of == 0 { return 0.0 }
    let p = x as f64 / of as f64;
    196.0 * (p * (1.0 - p) / of as f64).sqrt()
}

/// Writes a results file under the work folder and returns its path.
pub(super) fn write_result(name: &str, text: &str) -> PathBuf {
    let dir = work_dir().join("results");
    let _ = std::fs::create_dir_all(&dir);
    let path = match env("BENNU_MLB_TAG") {
        Some(tag) => dir.join(format!("{tag}_{name}")),
        None => dir.join(name),
    };
    let _ = std::fs::write(&path, text);
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_draw_of_a_line_depends_on_its_place_only() {
        assert_eq!(line_seed("A.java", 3), line_seed("A.java", 3));
        assert_ne!(line_seed("A.java", 3), line_seed("A.java", 4));
        assert_ne!(line_seed("A.java", 3), line_seed("B.java", 3));
    }

    #[test]
    fn a_held_out_file_lands_where_its_package_says() {
        let p = declared_path("package a.b;\n\npublic final class Foo<T> {}", "x__1.java");
        assert_eq!(p, Path::new("a/b/Foo.java"));
        assert_eq!(declared_path("class Hidden {}", "x__2.java"), Path::new("x__2.java"));
    }

    #[test]
    fn the_interval_shrinks_with_the_sample() {
        assert!(ci95(50, 100) > ci95(500, 1000));
        assert_eq!(ci95(0, 0), 0.0);
    }
}
