//! Asks for a **safe delete** at every method and field of a real project, applies the ones it
//! calls safe, and compiles the whole project with `javac`.
//!
//! ## The only failure that counts
//!
//! A safe delete has one job and one way to fail it. Refusing too much is conservative — annoying,
//! never dangerous. Saying **"nothing uses this"** about something that is used is the whole bug,
//! and it is invisible until a build breaks, possibly in a file nobody had open.
//!
//! So the measurement is exactly that: apply every deletion the engine calls safe, compile the
//! project, and count the ones that then fail. A single one is a defect; the refusals are not
//! scored at all, because there is no honest ceiling to compare them against.
//!
//! ## Why the whole project is recompiled each time
//!
//! Deleting a member breaks its **callers**, which live in other files by definition — a
//! single-file compile would miss precisely the case this exists to find. One javac run per
//! deletion is the cost, so `stride` samples the declarations.
//!
//! ```sh
//! cargo run -p bennu-intel --release --example safe_delete_case -- <project-dir> [stride]
//! ```
//!
//! ## The resolvers are wired the way the product wires them
//!
//! Both, in both roles — see `rename_case`'s note, which is the same trap: the policy resolver is
//! the one that answers "does this override something in a jar", and it is what makes a delete
//! refuse rather than break the build. Project-only here would report every override of a JDK
//! method as safely deletable and then congratulate itself on the result.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use bennu_classpath::prelude::resolve_jdk_classpath;
use bennu_index::prelude::PersistedIndex;
use bennu_intel::prelude::{build_project_index_from_sources, SemanticEngine};
use bennu_java::prelude::TypeResolver;
use bennu_query::prelude::{IndexResolver, JdkMemberIndex};

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(root) = args.next() else {
        eprintln!("usage: safe_delete_case <project-dir> [stride]");
        std::process::exit(2);
    };
    let stride: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(20);

    let root = PathBuf::from(&root);
    let mut javas = Vec::new();
    collect(&root, &mut javas);
    javas.sort();
    eprintln!("java files : {}", javas.len());
    let sources: Vec<(PathBuf, String)> = javas
        .iter()
        .filter_map(|p| fs::read_to_string(p).ok().map(|t| (p.clone(), t)))
        .collect();

    let temp = std::env::temp_dir().join(format!("bennu-sd-{}", std::process::id()));
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
    let engine = SemanticEngine::for_project(
        &temp,
        "21",
        &simple_names,
        sources.iter().map(|(p, s)| (slash(p), s.clone())).collect(),
        Vec::new(),
        Some(jdk_resolver()),
        &|_, _| {},
    )
    .expect("engine")
    .with_policy_resolver(jdk_resolver());
    eprintln!("engine ready (JDK-backed walk + policy)");

    // A working copy javac can be pointed at, plus a compile of it untouched: a project that does
    // not build cleanly to begin with would charge its own errors to the deletion.
    let work = std::env::temp_dir().join(format!("bennu-sd-work-{}", std::process::id()));
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

    let targets = targets(&sources, stride);
    eprintln!("declarations sampled: {} (every {stride}th)\n", targets.len());

    let (mut none, mut blocked, mut used, mut safe, mut clean, mut broken) = (0, 0, 0, 0, 0, 0);
    let mut reasons: HashMap<String, usize> = HashMap::new();
    let mut failures: Vec<String> = Vec::new();

    for (path, offset, name, kind) in &targets {
        let source = sources.iter().find(|(p, _)| p == path).map(|(_, s)| s).unwrap();
        let Some(plan) = engine.safe_delete(&slash(path), source, *offset) else {
            none += 1;
            continue;
        };
        if let Some(reason) = &plan.blocked {
            blocked += 1;
            *reasons.entry(first_clause(reason)).or_default() += 1;
            continue;
        }
        if !plan.usages.is_empty() {
            used += 1;
            continue;
        }
        safe += 1;

        // Apply to the working copy, compile the WHOLE project, restore.
        let Some(dest) = work_paths.get(&PathBuf::from(plan.file.replace('/', &sep()))) else {
            // The declaring file is outside the sample — restore nothing and skip.
            continue;
        };
        let Ok(before) = fs::read_to_string(dest) else { continue };
        if plan.end > before.len() || plan.start > plan.end {
            failures.push(format!("{kind} {name}: span {}..{} outside the file", plan.start, plan.end));
            broken += 1;
            continue;
        }
        let after = format!("{}{}", &before[..plan.start], &before[plan.end..]);
        fs::write(dest, &after).ok();
        let errors = compile(&work);
        fs::write(dest, &before).ok();

        if errors > baseline {
            broken += 1;
            failures.push(format!(
                "{kind} `{name}` in {} — {} new javac error(s)",
                short(path, &root),
                errors - baseline
            ));
        } else {
            clean += 1;
        }
    }

    println!("\n=== safe delete over {} ===", root.display());
    println!("declarations sampled  : {}", targets.len());
    println!("  not a target        : {none}   (a local, or nothing this project declares)");
    println!("  refused outright    : {blocked}");
    println!("  has usages          : {used}   (correctly not offered)");
    println!("  called SAFE         : {safe}");
    println!("    compiled clean    : {clean}");
    println!("    BROKE THE BUILD   : {broken}   ← the only number that is a defect");
    if safe > 0 {
        println!("  soundness           : {:.1}%", 100.0 * clean as f64 / safe as f64);
    }

    if !reasons.is_empty() {
        println!("\n=== why it refused ===");
        let mut rows: Vec<_> = reasons.into_iter().collect();
        rows.sort_by(|a, b| b.1.cmp(&a.1));
        for (reason, n) in rows {
            println!("  {n:>4}  {reason}");
        }
    }
    if !failures.is_empty() {
        println!("\n=== the deletions that were not safe ===");
        for f in &failures {
            println!("  {f}");
        }
    }
    let _ = fs::remove_dir_all(&temp);
    let _ = fs::remove_dir_all(&work);
}

fn sep() -> String {
    std::path::MAIN_SEPARATOR.to_string()
}

/// The first clause of a refusal, so the tally groups by reason rather than by member name.
fn first_clause(reason: &str) -> String {
    reason.split(" — ").next().unwrap_or(reason).split(',').next().unwrap_or(reason).to_string()
}

fn compile(dir: &Path) -> usize {
    let mut files = Vec::new();
    collect(dir, &mut files);
    if files.is_empty() {
        return 0;
    }
    let list = dir.join("sources.txt");
    let joined: Vec<String> = files.iter().map(|p| p.display().to_string()).collect();
    fs::write(&list, joined.join("\n")).ok();
    let out = Command::new("javac")
        .arg("-nowarn")
        .arg("-proc:none")
        .arg("-d")
        .arg(dir.join("classes"))
        .arg(format!("@{}", list.display()))
        .output();
    let Ok(out) = out else { return 0 };
    String::from_utf8_lossy(&out.stderr)
        .lines()
        .filter(|l| l.contains(": error:"))
        .count()
}

/// Every method and field declaration, sampled.
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

fn slash(p: &Path) -> String {
    p.display().to_string().replace('\\', "/")
}

fn short(p: &Path, root: &Path) -> String {
    p.strip_prefix(root).unwrap_or(p).display().to_string().replace('\\', "/")
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect(&p, out);
        } else if p.extension().is_some_and(|x| x == "java") {
            out.push(p);
        }
    }
}
