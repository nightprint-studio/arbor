//! What the **direct read** thinks a project's classpath is, and where it differs from Maven's.
//!
//! ## Why this exists
//!
//! The offline resolver ([`bennu_maven::prelude::resolve_offline`]) is the answer the editor uses
//! whenever it is complete, and it is the only one when Maven cannot run. It is also the one nobody
//! could interrogate: its output reached a person as a single sentence in a notification — *"19 of
//! this project's dependencies could not be resolved"* — with no way to ask whether those nineteen
//! are artifacts a build actually wants.
//!
//! They were not. Every one was the right library at a version the project's BOM overrides, so the
//! warning named artifacts nothing would ever download and the classpath was missing the versions
//! that *are* used. A sentence cannot show that. This diff does, in one run:
//!
//! ```sh
//! # what the direct read says
//! cargo run -p bennu-maven --example offline_case -- <project-dir>
//!
//! # …against the truth, for a project Maven can resolve
//! mvn -q dependency:build-classpath -Dmdep.outputFile=target/cp.txt -Dmdep.ignoreMissing=true \
//!     --fail-never --batch-mode
//! cargo run -p bennu-maven --example offline_case -- <project-dir> <project-dir>/target/cp.txt
//! ```
//!
//! The number that matters is **claimed missing, and not on Maven's classpath at all**: an artifact
//! reported absent that no build ever asks for is a phantom, and a phantom cannot be fixed by
//! downloading anything. Any other disagreement is worth reading; that one is a defect.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use bennu_maven::prelude::{resolve_offline, LocalRepo};

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(root) = args.next() else {
        eprintln!("usage: offline_case <project-dir> [maven-classpath-file]");
        std::process::exit(2);
    };
    let root = PathBuf::from(root);
    let repo = LocalRepo::discover();
    println!("project    : {}", root.display());
    println!("repository : {}\n", repo.root().display());

    let started = std::time::Instant::now();
    let res = resolve_offline(&root, &repo);
    let took = started.elapsed();

    println!("jars       : {}", res.jars.len());
    println!("missing    : {}", res.missing.len());
    println!("unversioned: {}", res.unversioned.len());
    println!("reactor    : {}", res.reactor.len());
    println!("read in    : {took:?}\n");

    for coord in &res.missing {
        println!("  MISSING     {}", coord.gav());
    }
    for coord in &res.unversioned {
        println!("  NO VERSION  {}", coord.gav());
    }

    let Some(cp) = args.next() else { return };
    let Ok(text) = std::fs::read_to_string(&cp) else {
        eprintln!("\ncannot read {cp}");
        std::process::exit(2);
    };
    // Maven writes one line, separated by the platform's path separator.
    let truth: Vec<String> =
        text.trim().split([':', ';']).filter(|s| !s.is_empty()).map(str::to_string).collect();
    let truth_files: HashSet<String> = truth.iter().map(|p| file_name(p)).collect();
    // Without the version, so "the same library at another version" is recognisable as such.
    let truth_stems: HashSet<String> = truth_files.iter().map(|f| stem(f)).collect();
    let ours: HashSet<String> = res.jars.iter().map(|p| file_name(&p.display().to_string())).collect();

    println!("\n=== against `mvn dependency:build-classpath` ===");
    println!("maven jars : {}", truth_files.len());
    println!("ours       : {}", ours.len());

    let mut phantom = 0;
    for coord in &res.missing {
        let file = format!("{}-{}.jar", coord.artifact_id, coord.version);
        let on_classpath = truth_files.contains(&file);
        let other_version = truth_files.iter().find(|f| stem(f) == stem(&file));
        if !on_classpath {
            phantom += 1;
            match other_version {
                Some(actual) => println!("  PHANTOM  {file}  →  maven uses {actual}"),
                None => println!("  PHANTOM  {file}  →  maven never asks for it"),
            }
        }
    }
    let unseen: Vec<&String> = truth_files
        .iter()
        .filter(|f| !ours.contains(*f) && !res.missing.iter().any(|c| stem(f) == c.artifact_id))
        .collect();

    println!("\n  claimed missing, absent from Maven's classpath : {phantom}   ← the defect");
    println!("  on Maven's classpath, unseen by the direct read : {}", unseen.len());
    for f in unseen.iter().take(20) {
        println!("      {f}");
    }
    if unseen.len() > 20 {
        println!("      … +{} more", unseen.len() - 20);
    }
    // A missing artifact Maven also wants is a REAL gap — the case the download exists for.
    let real: Vec<String> = res
        .missing
        .iter()
        .map(|c| format!("{}-{}.jar", c.artifact_id, c.version))
        .filter(|f| truth_files.contains(f))
        .collect();
    println!("  claimed missing AND on Maven's classpath        : {} (real gaps)", real.len());
    for f in &real {
        println!("      {f}");
    }
    let _ = truth_stems;
}

fn file_name(p: &str) -> String {
    Path::new(p).file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default()
}

/// A jar's name without its version — `jackson-databind-2.21.4.jar` → `jackson-databind`.
fn stem(file: &str) -> String {
    let base = file.trim_end_matches(".jar");
    match base.rfind('-') {
        Some(at) if base[at + 1..].starts_with(|c: char| c.is_ascii_digit()) => base[..at].to_string(),
        _ => base.to_string(),
    }
}
