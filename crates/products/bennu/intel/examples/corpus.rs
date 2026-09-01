//! Runs Bennu's validation over a real project that **compiles**, and reports what it flagged.
//!
//! The complement to [`langtools`](../langtools.rs): that corpus is code javac rejects, and scores
//! what we SEE; this one is code javac accepts, so every error printed here is a false positive by
//! construction. A check that gains coverage without being run against both has only been measured
//! on the half that flatters it.
//!
//! Not a test — it needs a corpus that isn't in this repo:
//!
//! ```sh
//! git clone --depth 1 https://github.com/google/guava.git
//! cargo run -p bennu-intel --release --example corpus -- guava
//! ```
//!
//! The whole tree is indexed as ONE project (a real project is what it is), skipping `target/`.
//! Library jars are NOT resolved, so a type from a dependency stays unknown and every check that
//! needs it correctly says nothing — which is why the codes worth reading here are the ones about
//! the project's own declarations.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use bennu_check::prelude::{check_file_resolved, FileContext};
use bennu_classpath::prelude::{resolve_jdk_classpath, SourceMemberIndex};
use bennu_index::prelude::PersistedIndex;
use bennu_intel::prelude::build_project_index_from_sources;
use bennu_query::prelude::IndexResolver;

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(root) = args.next() else {
        eprintln!("usage: corpus <project-dir> [code-prefix …]");
        std::process::exit(2);
    };
    // Optional filter: print only diagnostics whose code starts with one of these.
    let only: Vec<String> = args.collect();

    let mut javas = Vec::new();
    collect(Path::new(&root), &mut javas);
    javas.sort();
    eprintln!("java files   : {}", javas.len());
    if javas.is_empty() {
        std::process::exit(1);
    }

    let sources: Vec<(PathBuf, String)> = javas
        .iter()
        .filter_map(|p| fs::read_to_string(p).ok().map(|t| (p.clone(), t)))
        .collect();

    let temp = std::env::temp_dir().join(format!("bennu-corpus-{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp);
    fs::create_dir_all(&temp).expect("temp dir");
    let built = build_project_index_from_sources(&sources, &temp);
    built.builder.persist().expect("persist");
    let persisted =
        PersistedIndex::open(built.builder.blob_path(), built.builder.fst_path()).expect("open");
    let jdk = match resolve_jdk_classpath("21") {
        Ok(s) => s,
        Err(why) => {
            eprintln!("no JDK ({why}) — a JDK-less run would score every library type unresolved.");
            std::process::exit(1);
        }
    };
    let mut resolver = IndexResolver::new(persisted, SourceMemberIndex::new(jdk));
    for (simple, binary) in built.type_map.iter() {
        resolver.add_simple_hint(simple, binary);
    }

    let mut by_code: BTreeMap<String, usize> = BTreeMap::new();
    let mut total = 0usize;
    for (path, text) in &sources {
        let ctx = FileContext {
            file_stem: path.file_stem().map(|s| s.to_string_lossy().to_string()),
            expected_package: None,
            java_major: Some(21),
            classpath_complete: false,
        };
        for d in check_file_resolved(text, &ctx, &resolver, true) {
            if d.severity != "error" {
                continue;
            }
            total += 1;
            *by_code.entry(d.code.clone()).or_default() += 1;
            if only.is_empty() || only.iter().any(|p| d.code.starts_with(p)) {
                let line = text[..d.start.min(text.len())].matches('\n').count() + 1;
                println!("{}:{line}: {} — {}", path.display(), d.code, d.message);
            }
        }
    }
    let _ = fs::remove_dir_all(&temp);

    eprintln!("\nerrors       : {total}");
    for (code, n) in &by_code {
        eprintln!("  {n:>5}  {code}");
    }
}

/// Every `.java` under `dir`, skipping build output and VCS metadata.
fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        let name = e.file_name();
        let name = name.to_string_lossy();
        if p.is_dir() {
            if name == "target" || name == ".git" || name == "build" {
                continue;
            }
            collect(&p, out);
        } else if p.extension().is_some_and(|x| x == "java") {
            out.push(p);
        }
    }
}
