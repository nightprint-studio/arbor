//! M9c — blocks of statements: the next n = 2, 3, 5 statements, chained.
//!
//! Given the signature and the first k = 1, 2, 3 statements of a held-out method, `flc_next` (the open project's methods that begin the
//! same way: the shape of the next statement, its names filled by role) writes the next statement; that statement is appended to the
//! buffer and asked again, up to five times. Each step is checked by Bennu: the errors inside the statement just written must be
//! errors the original file already had; the block stops at the first step that does not hold (on the checked subset). The chain is run
//! with no threshold on the agreement and every step's share is kept, so "stop below 0.7" is read off afterwards exactly.
//!
//! "With a description" is the method's name in the signature, as always; "without" hides it (the method is renamed `x`, and the file's own
//! copy of the method is blanked in the index, since the name no longer excludes it). Expressions come from the neighbour's statement
//! with its names changed; M9b's enumerator is not used — it is far less exact than copying a neighbour of the project (M9b).
//!
//! `cargo test --release -p bennu-be mlbench::m9c_block -- --ignored --nocapture`  (`BENNU_MLB_N` methods per project, default 150;
//! `BENNU_MLB_NC` methods per project that are also compile-checked, default 50)

use std::collections::BTreeMap;

use super::common::*;
use super::m4_fill::body_errors;
use super::m4_methods::{class_of, normalize, rust_files};
use super::m78_shared::*;
use crate::flc_next::NextIndex;
use crate::index_service::IndexService;

const MAX_STEPS: usize = 5;
const STOP_BELOW: f32 = 0.7;

#[derive(Clone, Default)]
struct Step {
    text: String,
    share: f32,
    exact: bool,
    shape_ok: bool,
    /// `None` when the step was not compile-checked.
    fine: Option<bool>,
}

#[derive(Clone)]
struct Chain {
    k: usize,
    class: &'static str,
    steps: Vec<Step>,
    available: usize,
    named: bool,
    wanted: Vec<String>,
}

impl Chain {
    /// The steps kept by the stopping rules: below the share, and (when asked) at the first failed check.
    fn kept(&self, below: f32, check: bool) -> &[Step] {
        let n = self.steps.iter().position(|s| s.share < below || (check && s.fine == Some(false))).unwrap_or(self.steps.len());
        &self.steps[..n]
    }
}

fn run_chain(ix: &NextIndex, svc: Option<&IndexService>, file: &str, text: &str, b: &Body, k: usize, original_errors: Option<&std::collections::HashSet<(String, String)>>, rename: bool) -> Vec<Step> {
    let q = &b.m;
    let indent_start = text[..b.spans[0].0].rfind('\n').map_or(0, |p| p + 1);
    let indent = &text[indent_start..b.spans[0].0];
    let head = if rename { text[..b.spans[k - 1].1].replacen(&format!(" {}(", q.name), " x(", 1) } else { text[..b.spans[k - 1].1].to_string() };
    let mut written: Vec<String> = Vec::new();
    let mut steps = Vec::new();
    for j in 0..MAX_STEPS.min(q.statements.len() - k) {
        let mut prefix = head.clone();
        for g in &written { prefix.push('\n'); prefix.push_str(indent); prefix.push_str(g) }
        prefix.push('\n');
        prefix.push_str(indent);
        let buffer = format!("{prefix}\n{}", &text[q.body.1 - 1..]);
        let Some(next) = ix.suggest_at(file, &buffer, prefix.len(), 0.0) else { break };
        let truth = &q.statements[k + j];
        let mut step = Step { share: next.confidence, exact: normalize(&next.text) == normalize(truth), shape_ok: shape(&next.text) == shape(truth), text: next.text.clone(), fine: None };
        if let (Some(svc), Some(orig)) = (svc, original_errors) {
            // the statement written into the file, nothing after it but the closing brace
            let at = prefix.len();
            let candidate = format!("{}{}\n{}", &prefix, next.text, &text[q.body.1 - 1..]);
            let errors = body_errors(svc, file, &candidate, at, at + next.text.len());
            step.fine = Some(errors.is_subset(orig));
        }
        written.push(next.text);
        steps.push(step);
    }
    steps
}

fn blank_body(text: &str, b: &Body) -> String {
    format!("{}{{ }}{}", &text[..b.m.body.0], &text[b.m.body.1..])
}

#[test]
#[ignore = "needs the public corpus and a project index: see the module docs"]
fn m9c_blocks_of_statements() {
    let wanted = env_num("BENNU_MLB_N", 150);
    let compile_n = env_num("BENNU_MLB_NC", 50);
    let svc = IndexService::global();
    let mut chains: Vec<Chain> = Vec::new();
    for spec in PROJECTS {
        let (root, held) = assemble(spec);
        let mut ix = NextIndex::default();
        let mut files = Vec::new();
        rust_files(&root, &mut files);
        files.sort();
        for f in &files { ix.add_file(&f.to_string_lossy(), &String::from_utf8_lossy(&std::fs::read(f).unwrap_or_default())) }
        if compile_n > 0 { open_and_wait(&root) }
        let mut queries = Vec::new();
        for (hi, h) in held.iter().enumerate() {
            for b in bodies_of(&h.text, 1_000_000 + hi, 30).into_iter().filter(|b| b.m.statements.len() >= 4) { queries.push((hi, b)) }
        }
        let stride = (queries.len() / wanted.max(1)).max(1);
        let mut budget = compile_n;
        for (hi, b) in queries.iter().step_by(stride).take(wanted) {
            let h = &held[*hi];
            let file = h.path.to_string_lossy().into_owned();
            let class = class_of(&b.m);
            let original = (budget > 0).then(|| body_errors(svc, &file, &h.text, b.m.body.0, b.m.body.1));
            let mut compile_here = original.is_some();
            for k in 1..=3usize {
                if b.m.statements.len() <= k { continue }
                let wanted_rest: Vec<String> = b.m.statements[k..].iter().map(|s| normalize(s)).collect();
                // with the name
                let steps = run_chain(&ix, if k == 1 && compile_here { Some(svc) } else { None }, &file, &h.text, b, k, if k == 1 { original.as_ref() } else { None }, false);
                chains.push(Chain { k, class, steps, available: b.m.statements.len() - k, named: true, wanted: wanted_rest.clone() });
                // without it: the file's own copy of the method is blanked, the name hidden
                let blanked = blank_body(&h.text, b);
                ix.remove_file(&file);
                ix.add_file(&file, &blanked);
                let steps = run_chain(&ix, None, &file, &h.text, b, k, None, true);
                ix.remove_file(&file);
                ix.add_file(&file, &h.text);
                chains.push(Chain { k, class, steps, available: b.m.statements.len() - k, named: false, wanted: wanted_rest });
            }
            if compile_here { budget -= 1; compile_here = false }
            let _ = compile_here;
        }
        if compile_n > 0 { svc.forget(&root.to_string_lossy()) }
    }
    report(&chains);
}

fn consecutive_exact(steps: &[Step], n: usize) -> usize {
    steps.iter().take(n).take_while(|s| s.exact).count()
}

fn report(chains: &[Chain]) {
    let named: Vec<&Chain> = chains.iter().filter(|c| c.named).collect();
    let mut out = String::from("## M9c — blocks of statements, chained\n\n");
    for (title, below, check) in [("always continue (no threshold)", 0.0f32, false), ("stop when the agreement falls below 0.7", STOP_BELOW, false)] {
        out.push_str(&format!("### {title}\n\n| n | cases | block shown whole | 0 exact | 1 | 2 | 3 | 4 | 5 | block exact | shape of the whole block right |\n|---|---|---|---|---|---|---|---|---|---|---|\n"));
        for n in [2usize, 3, 5] {
            let pop: Vec<&&Chain> = named.iter().filter(|c| c.available >= n).collect();
            let whole = |c: &&&Chain| c.kept(below, check).len() >= n;
            let shown: Vec<&&&Chain> = pop.iter().filter(|c| whole(c)).collect();
            let dist = |m: usize| pct(pop.iter().filter(|c| consecutive_exact(c.kept(below, check), n) == m).count(), pop.len());
            out.push_str(&format!("| {n} | {} | {:.1}% | {:.1}% | {:.1}% | {:.1}% | {:.1}% | {:.1}% | {:.1}% | {:.1}% | {:.1}% |\n", pop.len(), pct(shown.len(), pop.len()), dist(0), dist(1), dist(2), dist(3), dist(4), dist(5), pct(pop.iter().filter(|c| consecutive_exact(c.kept(below, check), n) == n).count(), pop.len()), pct(shown.iter().filter(|c| c.kept(below, check).iter().take(n).all(|s| s.shape_ok)).count(), shown.len().max(1))));
        }
        out.push('\n');
    }
    out.push_str("### Precision per step (the chain goes on whatever happens; shown = the step was written)\n\n| step | written | exact when written | exact when all the steps before were exact too | mean agreement |\n|---|---|---|---|---|\n");
    for j in 0..MAX_STEPS {
        let shown: Vec<&&Chain> = named.iter().filter(|c| c.steps.len() > j).collect();
        let first_ok: Vec<&&&Chain> = shown.iter().filter(|c| c.steps[..j].iter().all(|s| s.exact)).collect();
        out.push_str(&format!("| {} | {} | {:.1}% | {:.1}% ({}) | {:.2} |\n", j + 1, shown.len(), pct(shown.iter().filter(|c| c.steps[j].exact).count(), shown.len()), pct(first_ok.iter().filter(|c| c.steps[j].exact).count(), first_ok.len()), first_ok.len(), shown.iter().map(|c| c.steps[j].share as f64).sum::<f64>() / shown.len().max(1) as f64));
    }
    out.push_str("\nSame, writing only the steps whose agreement is ≥ 0.7 (the chain stops at the first lower one):\n\n| step | written | exact when written | exact when the steps before were exact |\n|---|---|---|---|\n");
    for j in 0..MAX_STEPS {
        let shown: Vec<&&Chain> = named.iter().filter(|c| c.kept(STOP_BELOW, false).len() > j).collect();
        let first_ok: Vec<&&&Chain> = shown.iter().filter(|c| c.steps[..j].iter().all(|s| s.exact)).collect();
        out.push_str(&format!("| {} | {} | {:.1}% | {:.1}% ({}) |\n", j + 1, shown.len(), pct(shown.iter().filter(|c| c.steps[j].exact).count(), shown.len()), pct(first_ok.iter().filter(|c| c.steps[j].exact).count(), first_ok.len()), first_ok.len()));
    }
    // the threshold asked for: a block of three, the first two exact
    let three: Vec<&&Chain> = named.iter().filter(|c| c.kept(STOP_BELOW, false).len() >= 3).collect();
    let two_ok = three.iter().filter(|c| consecutive_exact(c.kept(STOP_BELOW, false), 2) == 2).count();
    out.push_str(&format!("\nBlocks of three written whole at ≥ 0.7: {} of {} ({:.1}%); the first two exact in {:.1}% of them, all three in {:.1}%.\n", three.len(), named.iter().filter(|c| c.available >= 3).count(), pct(three.len(), named.iter().filter(|c| c.available >= 3).count()), pct(two_ok, three.len()), pct(three.iter().filter(|c| consecutive_exact(c.kept(STOP_BELOW, false), 3) == 3).count(), three.len())));
    // by class
    out.push_str("\n### By class of method (n = 3, stop below 0.7)\n\n| class | cases | block shown whole | first two exact (of those shown) | all three exact (of those shown) | block exact (of all) |\n|---|---|---|---|---|---|\n");
    let mut classes: BTreeMap<&str, Vec<&&Chain>> = BTreeMap::new();
    for c in named.iter().filter(|c| c.available >= 3) { classes.entry(c.class).or_default().push(c) }
    for (cl, v) in &classes {
        let shown: Vec<&&&Chain> = v.iter().filter(|c| c.kept(STOP_BELOW, false).len() >= 3).collect();
        out.push_str(&format!("| {cl} | {} | {:.1}% | {:.1}% | {:.1}% | {:.1}% |\n", v.len(), pct(shown.len(), v.len()), pct(shown.iter().filter(|c| consecutive_exact(c.kept(STOP_BELOW, false), 2) == 2).count(), shown.len()), pct(shown.iter().filter(|c| consecutive_exact(c.kept(STOP_BELOW, false), 3) == 3).count(), shown.len()), pct(v.iter().filter(|c| consecutive_exact(c.kept(STOP_BELOW, false), 3) == 3).count(), v.len())));
    }
    // with and without the name
    out.push_str("\n### With the name of the method and without it (n = 3, no threshold / stop below 0.7)\n\n| | cases | first step exact | 2 consecutive exact | 3 consecutive exact | written whole at ≥ 0.7 |\n|---|---|---|---|---|---|\n");
    for (label, flag) in [("with the name", true), ("without", false)] {
        let pop: Vec<&Chain> = chains.iter().filter(|c| c.named == flag && c.available >= 3).collect();
        let at = |m: usize, below: f32| pct(pop.iter().filter(|c| consecutive_exact(c.kept(below, false), 3) >= m).count(), pop.len());
        out.push_str(&format!("| {label} | {} | {:.1}% / {:.1}% | {:.1}% / {:.1}% | {:.1}% / {:.1}% | {:.1}% |\n", pop.len(), at(1, 0.0), at(1, STOP_BELOW), at(2, 0.0), at(2, STOP_BELOW), at(3, 0.0), at(3, STOP_BELOW), pct(pop.iter().filter(|c| c.kept(STOP_BELOW, false).len() >= 3).count(), pop.len())));
    }
    // the check
    let checked: Vec<&&Chain> = named.iter().filter(|c| c.k == 1 && c.steps.iter().any(|s| s.fine.is_some())).collect();
    let steps_checked: Vec<&Step> = checked.iter().flat_map(|c| c.steps.iter()).collect();
    let failed = steps_checked.iter().filter(|s| s.fine == Some(false)).count();
    let exact_failed = steps_checked.iter().filter(|s| s.fine == Some(false) && s.exact).count();
    out.push_str(&format!("\n### The compile check (k = 1, {} chains checked)\n\nSteps checked: {}; the check rejected {} ({:.1}%); of the rejected, {} were exact statements (the check is not meant to reject those).\n", checked.len(), steps_checked.len(), failed, pct(failed, steps_checked.len()), exact_failed));
    let pop3: Vec<&&&Chain> = checked.iter().filter(|c| c.available >= 3).collect();
    out.push_str(&format!("Blocks of three at ≥ 0.7 on those: first step exact {:.1}% without the check, {:.1}% when the block stops at the first rejected step ({} chains).\n", pct(pop3.iter().filter(|c| consecutive_exact(c.kept(STOP_BELOW, false), 1) >= 1).count(), pop3.len()), pct(pop3.iter().filter(|c| consecutive_exact(c.kept(STOP_BELOW, true), 1) >= 1).count(), pop3.len()), pop3.len()));
    // examples
    let mut ex = String::from("\n### Examples (k = 1, n = 3, written at ≥ 0.7)\n\n");
    let (mut right, mut wrong) = (0, 0);
    let mut cand: Vec<&&Chain> = named.iter().filter(|c| c.k == 1 && c.kept(STOP_BELOW, false).len() >= 3).collect();
    cand.sort_by_key(|c| c.steps.iter().take(3).map(|s| s.text.len()).sum::<usize>());
    for c in cand {
        let ok = consecutive_exact(&c.steps, 3) == 3;
        if (ok && right < 2) || (!ok && wrong < 3 && c.steps.iter().take(3).map(|s| s.text.len()).sum::<usize>() > 30) {
            if ok { right += 1 } else { wrong += 1 }
            ex.push_str(&format!("**{}**\n\n```java\n// written\n{}\n// wanted\n{}\n```\n\n", if ok { "right" } else { "wrong" }, c.steps.iter().take(3).map(|s| s.text.clone()).collect::<Vec<_>>().join("\n"), c.wanted.iter().take(3).cloned().collect::<Vec<_>>().join("\n")));
        }
    }
    out.push_str(&ex);
    let path = write_result("m9c_report.md", &out);
    eprintln!("{out}\nreport: {}", path.display());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(share: f32, exact: bool, fine: Option<bool>) -> Step {
        Step { text: String::new(), share, exact, shape_ok: exact, fine }
    }

    #[test]
    fn a_chain_stops_at_the_first_low_agreement_or_failed_check() {
        let c = Chain { k: 1, class: "other", available: 4, named: true, wanted: Vec::new(), steps: vec![step(0.9, true, Some(true)), step(0.8, true, Some(false)), step(0.5, false, None), step(0.9, true, None)] };
        assert_eq!(c.kept(0.0, false).len(), 4);
        assert_eq!(c.kept(0.7, false).len(), 2);
        assert_eq!(c.kept(0.0, true).len(), 1);
        assert_eq!(consecutive_exact(c.kept(0.0, false), 3), 2);
    }

    #[test]
    fn consecutive_exact_counts_the_unbroken_run_from_the_first() {
        let steps = vec![step(1.0, true, None), step(1.0, false, None), step(1.0, true, None)];
        assert_eq!(consecutive_exact(&steps, 3), 1);
        assert_eq!(consecutive_exact(&steps[1..], 2), 0);
    }
}
