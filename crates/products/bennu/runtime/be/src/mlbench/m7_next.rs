//! M7, in production form: `flc_next` on the held-out methods, with an index of the open project ONLY (the held-out files and the
//! dependency sources assembled with them — nothing from the training projects).
//!
//! Each held-out method is cut after k statements the way the editor leaves it: the text up to the end of statement k, an empty
//! indented line with the caret, and the file from the method's closing brace on. The layer is asked with no threshold; the curve
//! is then read off: for each minimum share of agreement, how often it speaks, how often what it writes is exactly the next
//! statement, and how often it is at least of the right shape.
//!
//! `cargo test --release -p bennu-be mlbench::m7_next -- --ignored --nocapture`  (`BENNU_MLB_N` methods per project, default 800)

use std::collections::BTreeMap;

use super::common::*;
use super::m4_methods::{class_of, normalize, rust_files};
use super::m78_shared::*;
use crate::flc_next::NextIndex;

#[derive(Default)]
struct Row {
    share: f32,
    exact: bool,
    shape: bool,
    late: bool,
}

#[test]
#[ignore = "needs the public corpus: see the module docs"]
fn m7_next_layer_on_held_out_methods() {
    let wanted = env_num("BENNU_MLB_N", 800);
    let mut rows: Vec<(Row, &'static str)> = Vec::new();
    let mut cases: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut total = 0usize;
    for spec in PROJECTS {
        let (root, held) = assemble(spec);
        let mut ix = NextIndex::default();
        let mut files = Vec::new();
        rust_files(&root, &mut files);
        files.sort();
        for f in &files {
            let bytes = std::fs::read(f).unwrap_or_default();
            ix.add_file(&f.to_string_lossy(), &String::from_utf8_lossy(&bytes));
        }
        eprintln!("M7 next: {} — {} files, {} methods indexed", spec.name, files.len(), ix.method_count());
        let mut queries = Vec::new();
        for (hi, h) in held.iter().enumerate() {
            for b in bodies_of(&h.text, 1_000_000 + hi, 8) { queries.push((hi, b)) }
        }
        let stride = (queries.len() / wanted.max(1)).max(1);
        for (hi, b) in queries.iter().step_by(stride).take(wanted) {
            let text = &held[*hi].text;
            let n = b.m.statements.len();
            if n < 2 { continue }
            let class = class_of(&b.m);
            let indent_start = text[..b.spans[0].0].rfind('\n').map_or(0, |p| p + 1);
            let indent = &text[indent_start..b.spans[0].0];
            for k in 1..n {
                let prefix = format!("{}\n{indent}", &text[..b.spans[k - 1].1]);
                let buffer = format!("{prefix}\n{}", &text[b.m.body.1 - 1..]);
                total += 1;
                *cases.entry(class).or_default() += 1;
                if let Some(next) = ix.suggest_at(&held[*hi].path.to_string_lossy(), &buffer, prefix.len(), 0.0) {
                    let truth = &b.m.statements[k];
                    rows.push((Row { share: next.confidence, exact: normalize(&next.text) == normalize(truth), shape: shape(&next.text) == shape(truth), late: k >= 3 }, class));
                }
            }
        }
    }
    let mut out = format!("## M7 in production form — `flc_next`, index of the open project only\n\nCases (held-out method × statements already written): {total}. It spoke at all (no threshold): {} ({:.1}%).\n\n| minimum share of agreement | speaks in | exact statement | right shape |\n|---|---|---|---|\n", rows.len(), pct(rows.len(), total));
    let at = |t: f32, f: &dyn Fn(&(Row, &'static str)) -> bool| {
        let sel: Vec<_> = rows.iter().filter(|r| r.0.share >= t && f(r)).collect();
        (sel.len(), sel.iter().filter(|r| r.0.exact).count(), sel.iter().filter(|r| r.0.shape).count())
    };
    for t in [0.0f32, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0] {
        let (n, e, s) = at(t, &|_| true);
        out.push_str(&format!("| ≥ {t:.1} | {:.1}% of cases ({n}) | {:.1}% | {:.1}% |\n", pct(n, total), pct(e, n), pct(s, n)));
    }
    out.push_str("\nBy band of agreement (not cumulative):\n\n| share | times | exact | right shape |\n|---|---|---|---|\n");
    for (lo, hi) in [(0.0f32, 0.5), (0.5, 0.6), (0.6, 0.7), (0.7, 0.8), (0.8, 0.9), (0.9, 1.01)] {
        let sel: Vec<_> = rows.iter().filter(|r| r.0.share >= lo && r.0.share < hi).collect();
        out.push_str(&format!("| [{lo:.1}, {:.1}) | {} | {:.1}% | {:.1}% |\n", hi.min(1.0), sel.len(), pct(sel.iter().filter(|r| r.0.exact).count(), sel.len()), pct(sel.iter().filter(|r| r.0.shape).count(), sel.len())));
    }
    out.push_str("\nBy class of method at threshold 0.6:\n\n| class | cases | speaks | exact when it speaks |\n|---|---|---|---|\n");
    for (c, n) in &cases {
        let sel: Vec<_> = rows.iter().filter(|r| r.1 == *c && r.0.share >= 0.6).collect();
        out.push_str(&format!("| {c} | {n} | {:.1}% | {:.1}% |\n", pct(sel.len(), *n), pct(sel.iter().filter(|r| r.0.exact).count(), sel.len())));
    }
    let (n3, e3, _) = at(0.6, &|r| r.0.late);
    let (n2, e2, _) = at(0.6, &|r| !r.0.late);
    out.push_str(&format!("\nAt 0.6: fewer than 3 statements written — speaks {n2} times, exact {:.1}%; 3 or more — {n3} times, exact {:.1}%.\n", pct(e2, n2), pct(e3, n3)));
    let path = write_result("m7_next_report.md", &out);
    eprintln!("{out}\nreport: {}", path.display());
}
