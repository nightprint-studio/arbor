//! Proof that a performance change leaves the answers alone: after opening a project, ask find-usages at a few hundred places
//! (members after a dot, class declarations) and hash everything that comes back. Run before and after a change: the digests
//! must be equal. The reference walk's own totals (declarations, attempted, resolved) are in the log line it prints.
//!
//! `cargo test -p bennu-be mlbench::perf_identity -- --ignored --nocapture`  (`BENNU_MLB_ONLY`, `BENNU_MLB_N` places, default 400)

use super::common::*;
use super::sites::{sites, Position};
use crate::index_service::IndexService;

fn fnv(h: &mut u64, bytes: &[u8]) {
    for b in bytes {
        *h ^= *b as u64;
        *h = h.wrapping_mul(0x100000001b3);
    }
}

#[test]
#[ignore = "needs the public corpus: see the module docs"]
fn the_answers_do_not_change() {
    let wanted = env_num("BENNU_MLB_N", 400);
    let only = env("BENNU_MLB_ONLY");
    let svc = IndexService::global();
    let mut report = String::new();
    for spec in PROJECTS.iter().filter(|p| only.as_deref().is_none_or(|o| o == p.name)) {
        let (root, held) = assemble(spec);
        open_and_wait(&root);
        let mut places: Vec<(usize, usize)> = Vec::new();
        for (hi, h) in held.iter().enumerate() {
            for s in sites(&h.text) {
                if s.position == Position::MemberAfterDot {
                    places.push((hi, s.start));
                }
            }
            for (i, _) in h.text.match_indices("\nclass ").chain(h.text.match_indices(" class ")).take(2) {
                places.push((hi, i + 7));
            }
        }
        let stride = (places.len() / wanted.max(1)).max(1);
        let (mut digest, mut answered, mut usages) = (0xcbf29ce484222325u64, 0usize, 0usize);
        for (hi, off) in places.iter().step_by(stride).take(wanted) {
            let h = &held[*hi];
            let file = h.path.to_string_lossy().into_owned();
            let result = svc.find_usages(&file, &h.text, *off);
            // the order of a result list is not part of the contract (it varies between identical runs): hash it sorted
            let mut lines: Vec<String> = match &result {
                Some(r) => r.usages.iter().map(|u| format!("{u:?}")).chain(r.aliases.iter().flat_map(|a| a.usages.iter().map(move |u| format!("{}:{u:?}", a.label)))).collect(),
                None => Vec::new(),
            };
            lines.sort();
            fnv(&mut digest, format!("{file}@{off}:{}:{}", result.is_some(), lines.join("|")).as_bytes());
            if let Some(r) = &result {
                answered += 1;
                usages += r.usages.len() + r.aliases.iter().map(|a| a.usages.len()).sum::<usize>();
            }
        }
        report.push_str(&format!("{}\tplaces {}\tanswered {answered}\tusages {usages}\tdigest {digest:016x}\n", spec.name, places.iter().step_by(stride).take(wanted).count()));
        svc.forget(&root.to_string_lossy());
    }
    write_result("perf_identity.tsv", &report);
    eprintln!("IDENTITY\n{report}");
}

/// Opens a public corpus project **in place** (with its `pom.xml`, so its dependency jars resolve from the local Maven repository)
/// and asks find-usages at member positions of its sources. With and without `BENNU_RENAME_FULL_RESOLVER=1` (the walk over the
/// dependency tier) the totals and the answers can be compared: how many more references the walk finds, and at what cost.
///
/// `BENNU_MLB_CORPUS_PROJECT=commons-text cargo test -p bennu-be mlbench::perf_identity::the_walk_with_dependencies -- --ignored --nocapture`
#[test]
#[ignore = "needs a public corpus project with resolvable Maven dependencies"]
fn the_walk_with_dependencies() {
    let Some(name) = env("BENNU_MLB_CORPUS_PROJECT") else { return };
    let wanted = env_num("BENNU_MLB_N", 600);
    let root = corpus().join(&name);
    let svc = IndexService::global();
    let started = std::time::Instant::now();
    open_and_wait(&root);
    // the dependency tier lands on its own thread: wait for the jars to be the project's before asking
    let key = root.to_string_lossy().into_owned();
    let waited = std::time::Instant::now();
    let jars = loop {
        let jars = svc.classpath_snapshot().into_iter().find(|(r, ..)| *r == key).map_or(0, |(_, _, j)| j.len());
        if jars > 0 || waited.elapsed().as_secs() > 120 {
            break jars;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    };
    eprintln!("DEPS jars on the classpath: {jars}, ready {:.1}s after open", started.elapsed().as_secs_f32());
    let mut files = Vec::new();
    super::m4_methods::rust_files(&root.join("src/main"), &mut files);
    files.sort();
    let mut places: Vec<(String, String, usize)> = Vec::new();
    for f in &files {
        let text = std::fs::read_to_string(f).unwrap_or_default();
        for s in sites(&text) {
            if s.position == Position::MemberAfterDot {
                places.push((f.to_string_lossy().into_owned(), text.clone(), s.start));
            }
        }
    }
    let stride = (places.len() / wanted.max(1)).max(1);
    let mut per_place = String::new();
    let (mut digest, mut answered, mut usages) = (0xcbf29ce484222325u64, 0usize, 0usize);
    let sampled: Vec<_> = places.iter().step_by(stride).take(wanted).collect();
    for (file, text, off) in &sampled {
        let result = svc.find_usages(file, text, *off);
        let mut lines: Vec<String> = match &result {
            Some(r) => r.usages.iter().map(|u| format!("{u:?}")).collect(),
            None => Vec::new(),
        };
        lines.sort();
        fnv(&mut digest, format!("{file}@{off}:{}:{}", result.is_some(), lines.join("|")).as_bytes());
        per_place.push_str(&format!("{file}@{off}\t{}\t{}\n", result.is_some(), lines.len()));
        if let Some(r) = &result {
            answered += 1;
            usages += r.usages.len();
        }
    }
    write_result(&format!("depswalk_{name}_{}.tsv", if env("BENNU_RENAME_FULL_RESOLVER").is_some() { "full" } else { "jdk" }), &per_place);
    eprintln!("DEPSWALK\t{name}\tplaces {}\tanswered {answered}\tusages {usages}\tdigest {digest:016x}", sampled.len());
}
