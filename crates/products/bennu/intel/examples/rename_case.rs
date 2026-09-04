//! Renames every method and field of a real project to a name differing ONLY in case, applies the
//! plan, and compiles the result with `javac`. Every error javac then reports is a site the rename
//! got wrong.
//!
//! ## Why a case change is the right stress test
//!
//! A rename is judged by what it MISSES, and a missed site is normally invisible: rename `count` to
//! `total` and a forgotten `count` may still compile, because a field of that name exists elsewhere,
//! or the old name was never the one that mattered. A case-only rename removes that cover. The new
//! name collides with nothing (Java is case-sensitive; `getFoo` and `GetFoo` are unrelated), so
//! afterwards **every** surviving reference to the old spelling is a compile error, and every edit
//! aimed at something that was not this symbol is one too. javac counts both, exactly.
//!
//! Only methods and fields are renamed. A type's rename moves its file, and on a case-insensitive
//! filesystem — macOS's default — a case-only file rename is not a rename at all, so that half of
//! the question cannot be asked here honestly.
//!
//! ```sh
//! cargo run -p bennu-intel --release --example rename_case -- <project-dir> [stride] [projectonly] [name=<decl>]
//! ```
//!
//! `stride` samples the declarations (every Nth, default 20) — one javac run per rename is the cost.
//!
//! ## The resolvers are wired the way the product wires them
//!
//! `index_service` lends the engine a **JDK-backed** walk resolver and, separately, the full
//! classpath **for policy only** — the question "does this method override something in a jar I
//! cannot edit", asked once per rename, which is what makes the engine REFUSE a rename instead of
//! planning a broken one. A harness that leaves both project-only measures something the product
//! never does: with no JDK in the policy resolver, `java.util.Map.putAll` is invisible, every
//! implementation of a JDK interface plans clean, and the "errors introduced" number is really a
//! count of refusals that never fired. `projectonly` reproduces that degraded wiring on purpose.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use bennu_classpath::prelude::resolve_jdk_classpath;
use bennu_index::prelude::PersistedIndex;
use bennu_intel::prelude::{build_project_index_from_sources, RenamePlan, SemanticEngine};
use bennu_java::prelude::TypeResolver;
use bennu_query::prelude::{IndexResolver, JdkMemberIndex};

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(root) = args.next() else {
        eprintln!("usage: rename_case <project-dir> [stride] [projectonly]");
        std::process::exit(2);
    };
    let stride: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(20);
    let rest: Vec<String> = args.collect();
    let project_only = rest.iter().any(|a| a == "projectonly");
    // `name=isStarted` narrows the sweep to one declaration and prints javac's own words for it —
    // the report says WHICH rename broke the build; this says how.
    let only: Option<String> = rest.iter().find_map(|a| a.strip_prefix("name=").map(String::from));

    let root = PathBuf::from(&root);
    let mut javas = Vec::new();
    collect(&root, &mut javas);
    javas.sort();
    eprintln!("java files : {}", javas.len());
    let sources: Vec<(PathBuf, String)> = javas
        .iter()
        .filter_map(|p| fs::read_to_string(p).ok().map(|t| (p.clone(), t)))
        .collect();

    let temp = std::env::temp_dir().join(format!("bennu-rename-{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp);
    fs::create_dir_all(&temp).expect("temp dir");
    let built = build_project_index_from_sources(&sources, &temp);
    built.builder.persist().expect("persist");
    let simple_names: Vec<(String, String)> =
        built.type_map.iter().map(|(s, b)| (s.clone(), b.clone())).collect();

    let jdk_resolver = || {
        let source = resolve_jdk_classpath("21").expect("a JDK");
        let persisted = PersistedIndex::open(built.builder.blob_path(), built.builder.fst_path())
            .expect("open index");
        let mut r = IndexResolver::new(persisted, JdkMemberIndex::new(Box::new(source)));
        for (s, b) in &simple_names {
            r.add_simple_hint(s, b);
        }
        Arc::new(r) as Arc<dyn TypeResolver + Send + Sync>
    };
    let shared: Option<Arc<dyn TypeResolver + Send + Sync>> =
        (!project_only).then(&jdk_resolver);

    let engine = SemanticEngine::for_project(
        &temp,
        "21",
        &simple_names,
        sources.iter().map(|(p, s)| (slash(p), s.clone())).collect(),
        Vec::new(),
        shared,
        &|_, _| {},
    )
    .expect("engine");
    // The refusal question needs the classpath even when the walk does not — exactly as the BE
    // wires it. Leaving it off is the difference between a rename that is refused and one that is
    // planned and breaks the build.
    let engine = if project_only { engine } else { engine.with_policy_resolver(jdk_resolver()) };
    eprintln!(
        "engine ready (resolver: {})",
        if project_only { "project-only, no policy classpath" } else { "JDK-backed walk + policy" }
    );

    // A working copy javac can be pointed at, and a compile of it untouched — a project that does
    // not build cleanly to begin with would otherwise charge its own errors to the rename.
    let work = std::env::temp_dir().join(format!("bennu-rename-work-{}", std::process::id()));
    let _ = fs::remove_dir_all(&work);
    let mut work_paths: HashMap<PathBuf, PathBuf> = HashMap::new();
    for (p, text) in &sources {
        let rel = p.strip_prefix(&root).unwrap_or(p);
        let dest = work.join(rel);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).ok();
        }
        fs::write(&dest, text).ok();
        work_paths.insert(p.clone(), dest);
    }
    let baseline = compile(&work);
    eprintln!("baseline javac errors: {baseline}");

    let mut targets = targets(&sources, if only.is_some() { 1 } else { stride });
    if let Some(want) = &only {
        targets.retain(|(_, _, name, _)| name == want);
    }
    eprintln!("rename targets: {} (every {stride}th declaration)\n", targets.len());

    let (mut planned, mut none, mut refused, mut clean, mut broken) = (0, 0, 0, 0, 0);
    let mut edits_total = 0usize;
    let mut failures: Vec<(String, usize, String)> = Vec::new();

    for (path, offset, old, kind) in &targets {
        let source = sources.iter().find(|(p, _)| p == path).map(|(_, s)| s).unwrap();
        let new_name = flip_case(old);
        if new_name == *old {
            continue;
        }
        let Some(plan) = engine.plan(&slash(path), source, *offset, &new_name) else {
            none += 1;
            continue;
        };
        if plan.blocked.is_some() {
            refused += 1;
            continue;
        }
        planned += 1;
        edits_total += plan.total_edits();

        // Apply, compile, restore. Only the files the plan touches are rewritten; javac still reads
        // the whole project, which is the point — a missed site anywhere in it must surface.
        let touched = apply(&plan, &sources, &work_paths);
        let errors = compile(&work);
        if only.is_some() {
            println!("--- {kind} {old} → {new_name}  ({} edits in {} files) ---", plan.total_edits(), plan.files.len());
            for fe in &plan.files {
                println!("  {}", short(Path::new(&fe.file), &root));
                for e in &fe.edits {
                    println!("      {}:{} {:?} `{}`", e.start, e.end, e.reason, e.old);
                }
            }
            println!("  javac errors: {errors} (baseline {baseline})");
            println!("{}", compile_output(&work));
        }
        for (p, original) in &touched {
            fs::write(p, original).ok();
        }
        if errors > baseline {
            broken += 1;
            failures.push((format!("{kind} {old} → {new_name}"), errors - baseline, short(path, &root)));
        } else {
            clean += 1;
        }
    }

    println!("\n=== case-only rename over {} ===", root.display());
    println!(
        "resolver              : {}",
        if project_only { "project-only, no policy classpath" } else { "JDK-backed walk + policy (as the product wires it)" }
    );
    println!("declarations sampled  : {}", targets.len());
    println!("  not renameable      : {none}");
    println!("  refused by design   : {refused}   (overrides a library type)");
    println!("  planned & applied   : {planned}");
    println!("    compiled clean    : {clean}");
    println!("    BROKE THE BUILD   : {broken}");
    if planned > 0 {
        println!("edits per rename (avg): {:.1}", edits_total as f64 / planned as f64);
        println!("correct rate          : {:.1}%", 100.0 * clean as f64 / planned as f64);
    }
    if !failures.is_empty() {
        println!("\nrenames that broke the build:");
        for (what, extra, file) in &failures {
            println!("  +{extra:<4} errors  {what}   [{file}]");
        }
    }
    let _ = fs::remove_dir_all(&work);
    let _ = fs::remove_dir_all(&temp);
}

/// Apply `plan` to the working copy, returning each touched path with the text it held before.
fn apply(
    plan: &RenamePlan,
    sources: &[(PathBuf, String)],
    work_paths: &HashMap<PathBuf, PathBuf>,
) -> Vec<(PathBuf, String)> {
    let mut touched = Vec::new();
    for fe in &plan.files {
        let Some((orig_path, text)) = sources.iter().find(|(p, _)| slash(p) == fe.file) else {
            continue;
        };
        let Some(dest) = work_paths.get(orig_path) else { continue };
        // Descending, so an earlier edit's offsets stay valid while a later one is applied.
        let mut edits = fe.edits.clone();
        edits.sort_by_key(|e| std::cmp::Reverse(e.start));
        let mut out = text.clone();
        for e in &edits {
            if e.end <= out.len() {
                out.replace_range(e.start..e.end, &e.new_text);
            }
        }
        touched.push((dest.clone(), text.clone()));
        fs::write(dest, out).ok();
    }
    touched
}

/// javac's raw transcript for `dir` — for the single-target mode, where the count is not the answer.
fn compile_output(dir: &Path) -> String {
    let mut files = Vec::new();
    collect(dir, &mut files);
    let list = dir.join("_sources.txt");
    let joined: Vec<String> = files.iter().map(|p| p.display().to_string()).collect();
    fs::write(&list, joined.join("\n")).ok();
    let classes = dir.join("_classes");
    let out = Command::new("javac")
        .arg("-nowarn")
        .arg("-d")
        .arg(&classes)
        .arg(format!("@{}", list.display()))
        .output();
    let _ = fs::remove_dir_all(&classes);
    let _ = fs::remove_file(&list);
    out.map(|o| String::from_utf8_lossy(&o.stderr).into_owned()).unwrap_or_default()
}

/// Compile every `.java` under `dir` and count the errors javac reports.
fn compile(dir: &Path) -> usize {
    let mut files = Vec::new();
    collect(dir, &mut files);
    let list = dir.join("_sources.txt");
    let joined: Vec<String> = files.iter().map(|p| p.display().to_string()).collect();
    fs::write(&list, joined.join("\n")).ok();
    let classes = dir.join("_classes");
    let out = Command::new("javac")
        .arg("-nowarn")
        .arg("-d")
        .arg(&classes)
        .arg(format!("@{}", list.display()))
        .output();
    let _ = fs::remove_dir_all(&classes);
    let _ = fs::remove_file(&list);
    match out {
        Ok(o) => String::from_utf8_lossy(&o.stderr).matches("error:").count(),
        Err(_) => 0,
    }
}

/// Every Nth method / field DECLARATION name in the project: its file, the offset of the name, the
/// name, and what it is.
fn targets(sources: &[(PathBuf, String)], stride: usize) -> Vec<(PathBuf, usize, String, &'static str)> {
    let mut all = Vec::new();
    for (path, text) in sources {
        let Some(tree) = bennu_java::prelude::parse_java(text) else { continue };
        let bytes = text.as_bytes();
        let mut stack = vec![tree.root_node()];
        while let Some(n) = stack.pop() {
            match n.kind() {
                "method_declaration" => {
                    if let Some(name) = n.child_by_field_name("name") {
                        if let Ok(t) = name.utf8_text(bytes) {
                            all.push((path.clone(), name.start_byte(), t.to_string(), "method"));
                        }
                    }
                }
                "field_declaration" => {
                    let mut c = n.walk();
                    for d in n.named_children(&mut c) {
                        if d.kind() != "variable_declarator" {
                            continue;
                        }
                        if let Some(name) = d.child_by_field_name("name") {
                            if let Ok(t) = name.utf8_text(bytes) {
                                all.push((path.clone(), name.start_byte(), t.to_string(), "field"));
                            }
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
    }
    all.sort();
    all.into_iter().step_by(stride.max(1)).collect()
}

/// `getFoo` → `GetFoo`, `Value` → `value`. Only the first letter moves: enough to make the two
/// names unrelated to a case-sensitive compiler, small enough that nothing else about the code
/// changes.
fn flip_case(name: &str) -> String {
    let mut cs = name.chars();
    match cs.next() {
        Some(c) if c.is_uppercase() => c.to_lowercase().collect::<String>() + cs.as_str(),
        Some(c) if c.is_lowercase() => c.to_uppercase().collect::<String>() + cs.as_str(),
        _ => name.to_string(),
    }
}

fn slash(p: &Path) -> String {
    p.display().to_string().replace('\\', "/")
}

fn short(p: &Path, root: &Path) -> String {
    p.strip_prefix(root).unwrap_or(p).display().to_string()
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        let name = e.file_name();
        let name = name.to_string_lossy();
        if p.is_dir() {
            if name == "target" || name == ".git" || name == "build" || name == "_classes" {
                continue;
            }
            collect(&p, out);
        } else if p.extension().is_some_and(|x| x == "java") {
            out.push(p);
        }
    }
}
