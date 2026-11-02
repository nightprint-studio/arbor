//! M3 — which candidate comes first: the popup today, the popup with habit tables filled from public code, and a ranking
//! learned from the same code.
//!
//! Two positions: the member after a dot, and a call argument that is a plain name (variable or field). The label is the
//! code already written; the candidates are what the real popup (`IndexService::completion`) lists at that place, so (a) is
//! the product's own order — eight hand-written signals plus whatever in-project habits it carries.
//!
//! Training data is public code that is NOT the held-out set: `TRAIN` below. The held-out projects are assembled from
//! `data2/eval`, so no table count and no weight has seen them. The ranker is a softmax regression written here (no
//! library), trained on the training projects' own popups with a site's own contribution taken out of the tables it reads.
//!
//! `cargo test -p bennu-be mlbench::m3_rank -- --ignored --nocapture`
//! (`BENNU_MLB_N` sites per kind and project, default 500; `BENNU_MLB_TRAIN=a,b` training projects; `BENNU_MLB_SHOW=1`)

use std::collections::HashMap;
use std::path::Path;

use super::common::*;
use super::sites::{sites, Position};
use crate::index_service::IndexService;
use bennu_complete::prelude::MatchCase;

const TRAIN: &[&str] = &["commons-io", "commons-collections", "mockito", "junit5", "jackson-databind"];
const POPUP_CAP: usize = 300;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub(super) enum Kind {
    Member,
    Argument,
}

/// A candidate as the popup lists it, with what the features need beyond the popup.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(super) struct Cand {
    pub name: String,
    pub rank: usize,
    pub kind: String,
    pub detail: String,
    pub deprecated: bool,
    /// Occurrences of the name in the 60 lines above the site.
    pub recency: usize,
}

/// One place in the code, with everything needed to rank it, kept as plain data.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(super) struct Instance {
    pub kind: Kind,
    pub typed: usize,
    pub truth: usize,
    pub cands: Vec<Cand>,
    pub popup_len: usize,
    pub recv_type: String,
    pub prev: String,
    pub callee: String,
    pub arg_index: usize,
    pub target: String,
    pub param_type: String,
    pub param_name: String,
}

pub(super) fn ident_of(label: &str) -> &str {
    let end = label.find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$')).unwrap_or(label.len());
    &label[..end]
}

pub(super) fn simple(binary: &str) -> String {
    binary.rsplit(['/', '.', '$']).next().unwrap_or(binary).to_string()
}

/// `end`, moved past the balanced `( … )` that directly follows it, if any.
pub(super) fn after_call(text: &str, end: usize) -> usize {
    if !text[end..].starts_with('(') {
        return end;
    }
    let mut depth = 0usize;
    for (i, ch) in text[end..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return end + i + 1;
                }
            }
            _ => {}
        }
    }
    end
}

fn count_word(haystack: &str, word: &str) -> usize {
    if word.is_empty() {
        return 0;
    }
    haystack.match_indices(word).filter(|(i, _)| {
        let before = haystack[..*i].chars().next_back();
        let after = haystack[i + word.len()..].chars().next();
        !before.is_some_and(|c| c.is_alphanumeric() || c == '_') && !after.is_some_and(|c| c.is_alphanumeric() || c == '_')
    }).count()
}

pub(super) fn is_plain_name(t: &str) -> bool {
    t.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
        && t.chars().all(|c| c.is_alphanumeric() || c == '_')
        && !["this", "super", "null", "true", "false", "new"].contains(&t)
}

// ---- tables ----------------------------------------------------------------------------------------------------------

#[derive(Default)]
pub(super) struct Tables {
    /// receiver type → member → count
    pub member: HashMap<String, HashMap<String, u32>>,
    /// previous call of the chain → member → count
    pub chain: HashMap<String, HashMap<String, u32>>,
    /// (callee, argument index) → name → count
    pub arg: HashMap<(String, usize), HashMap<String, u32>>,
}

fn bump<K: std::hash::Hash + Eq>(m: &mut HashMap<K, HashMap<String, u32>>, k: K, v: &str) {
    *m.entry(k).or_default().entry(v.to_string()).or_default() += 1;
}

fn lookup<K: std::hash::Hash + Eq>(m: &HashMap<K, HashMap<String, u32>>, k: &K, v: &str) -> (u32, u32) {
    m.get(k).map_or((0, 0), |inner| (inner.get(v).copied().unwrap_or(0), inner.values().sum()))
}

impl Tables {
    /// Adds the counts of `other` to these.
    pub(super) fn merge(&mut self, other: Tables) {
        fn add<K: std::hash::Hash + Eq>(into: &mut HashMap<K, HashMap<String, u32>>, from: HashMap<K, HashMap<String, u32>>) {
            for (k, inner) in from {
                let slot = into.entry(k).or_default();
                for (name, n) in inner { *slot.entry(name).or_default() += n }
            }
        }
        add(&mut self.member, other.member);
        add(&mut self.chain, other.chain);
        add(&mut self.arg, other.arg);
    }

    pub(super) fn is_empty(&self) -> bool {
        self.member.is_empty() && self.arg.is_empty()
    }

    /// Counts for a candidate at an instance; `own` takes out the site's own sighting (the true candidate of a training site).
    fn counts(&self, i: &Instance, c: &Cand, own: bool) -> [(u32, u32); 2] {
        let minus = |(n, t): (u32, u32)| if own { (n.saturating_sub(1), t.saturating_sub(1)) } else { (n, t) };
        match i.kind {
            Kind::Member => [minus(lookup(&self.member, &i.recv_type, &c.name)), minus(lookup(&self.chain, &i.prev, &c.name))],
            Kind::Argument => [minus(lookup(&self.arg, &(i.callee.clone(), i.arg_index), &c.name)), (0, 0)],
        }
    }
}

// ---- features and the ranker ---------------------------------------------------------------------------------------

const NF_MEMBER: usize = 14;

fn row_of(c: &Cand) -> crate::flc_rank::Row<'_> {
    crate::flc_rank::Row { name: &c.name, kind: &c.kind, detail: &c.detail, deprecated: c.deprecated, recency: c.recency, rank: c.rank }
}

/// What `flc_rank::rerank` does in the editor, on the recorded popup: the production ranker with its embedded weights.
pub(super) fn order_production(i: &Instance) -> Vec<usize> {
    if i.kind != Kind::Argument {
        return order_popup(i);
    }
    let rows: Vec<crate::flc_rank::Row> = i.cands.iter().map(row_of).collect();
    let pinned = i.cands.iter().take_while(|c| c.detail.starts_with("used ") && c.detail.contains("× in .")).count();
    crate::flc_rank::order(&i.param_type, &i.param_name, &rows, pinned)
}

fn tokens(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in s.chars() {
        if ch == '_' || !ch.is_alphanumeric() {
            if !cur.is_empty() { out.push(std::mem::take(&mut cur).to_lowercase()); }
        } else if ch.is_uppercase() && !cur.is_empty() && cur.chars().last().is_some_and(|l| l.is_lowercase()) {
            out.push(std::mem::take(&mut cur).to_lowercase());
            cur.push(ch);
        } else {
            cur.push(ch);
        }
    }
    if !cur.is_empty() { out.push(cur.to_lowercase()); }
    out
}

fn jaccard(a: &str, b: &str) -> f32 {
    let (ta, tb) = (tokens(a), tokens(b));
    if ta.is_empty() || tb.is_empty() { return 0.0; }
    let common = ta.iter().filter(|t| tb.contains(t)).count();
    common as f32 / (ta.len() + tb.len() - common).max(1) as f32
}

pub(super) fn features(t: &Tables, i: &Instance, c: &Cand, own: bool) -> Vec<f32> {
    if i.kind == Kind::Argument {
        return crate::flc_rank::features(&i.param_type, &i.param_name, &row_of(c)).to_vec();
    }
    let [(n1, t1), (n2, t2)] = t.counts(i, c, own);
    let ln = |x: f32| (1.0 + x).ln();
    let is = |k: &str| f32::from(c.kind.contains(k));
    let type_match = false;
    vec![
        ln(c.rank as f32),
        1.0 / (1.0 + c.rank as f32),
        is("method"),
        is("field"),
        ln(n1 as f32),
        if t1 > 0 { n1 as f32 / t1 as f32 } else { 0.0 },
        ln(n2 as f32),
        if t2 > 0 { n2 as f32 / t2 as f32 } else { 0.0 },
        ln(c.recency as f32),
        jaccard(&c.name, &i.target),
        f32::from(type_match),
        ln(c.name.len() as f32),
        f32::from(c.deprecated),
        f32::from(t1 == 0),
    ]
}

/// Softmax regression over the candidates of one site: score = w·f, loss = −log softmax(score of the true one).
#[derive(Clone)]
pub(super) struct Ranker {
    pub w: Vec<f32>,
}

impl Ranker {
    fn score(&self, f: &[f32]) -> f32 {
        self.w.iter().zip(f).map(|(w, x)| w * x).sum()
    }

    pub(super) fn train(data: &[(Vec<Vec<f32>>, usize)], nf: usize, epochs: usize) -> Ranker {
        let mut r = Ranker { w: vec![0.0; nf] };
        // start from "trust the popup": a negative weight on the log-rank
        r.w[0] = -0.5;
        let (lr, l2) = (0.05f32, 1e-3f32);
        for epoch in 0..epochs {
            let rate = lr / (1.0 + epoch as f32 * 0.2);
            for (feats, truth) in data {
                let scores: Vec<f32> = feats.iter().map(|f| r.score(f)).collect();
                let m = scores.iter().cloned().fold(f32::MIN, f32::max);
                let exps: Vec<f32> = scores.iter().map(|s| (s - m).exp()).collect();
                let z: f32 = exps.iter().sum();
                for k in 0..nf {
                    let expected: f32 = feats.iter().zip(&exps).map(|(f, e)| f[k] * e / z).sum();
                    let grad = feats[*truth][k] - expected;
                    r.w[k] += rate * (grad - l2 * r.w[k]);
                }
            }
        }
        r
    }
}

// ---- evaluation ------------------------------------------------------------------------------------------------------

#[derive(Default, Clone)]
struct Score {
    n: usize,
    top1: usize,
    top3: usize,
    mrr: f64,
}

impl Score {
    fn add(&mut self, rank: Option<usize>) {
        self.n += 1;
        if let Some(r) = rank {
            self.top1 += usize::from(r == 0);
            self.top3 += usize::from(r < 3);
            self.mrr += 1.0 / (r as f64 + 1.0);
        }
    }
}

/// Position of the truth after ordering `order` (indexes into cands), `None` when it is not among them.
fn position_of(order: &[usize], truth: usize) -> Option<usize> {
    order.iter().position(|&k| k == truth)
}

pub(super) fn order_popup(i: &Instance) -> Vec<usize> {
    (0..i.cands.len()).collect()
}

/// (b): what the tables have seen most often leads, in the popup's order within a count; the rest keep the popup's order.
pub(super) fn order_lifted(t: &Tables, i: &Instance, min_count: u32) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..i.cands.len()).collect();
    let key = |k: usize| {
        let [(n1, _), (n2, _)] = t.counts(i, &i.cands[k], false);
        let n = match i.kind { Kind::Member => n1.max(n2), Kind::Argument => n1 };
        if n >= min_count { n } else { 0 }
    };
    idx.sort_by(|&a, &b| key(b).cmp(&key(a)).then(a.cmp(&b)));
    idx
}

pub(super) fn order_learned(t: &Tables, r: &Ranker, i: &Instance) -> Vec<usize> {
    let scores: Vec<f32> = i.cands.iter().map(|c| r.score(&features(t, i, c, false))).collect();
    let mut idx: Vec<usize> = (0..i.cands.len()).collect();
    idx.sort_by(|&a, &b| scores[b].partial_cmp(&scores[a]).unwrap_or(std::cmp::Ordering::Equal).then(a.cmp(&b)));
    idx
}

// ---- collecting ------------------------------------------------------------------------------------------------------

fn copy_without_tests(from: &Path, to: &Path) {
    let Ok(entries) = std::fs::read_dir(from) else { return };
    for e in entries.flatten() {
        let (p, name) = (e.path(), e.file_name().to_string_lossy().into_owned());
        if p.is_dir() {
            if !matches!(name.as_str(), "test" | "tests" | "it" | ".git" | "target" | "build" | "node_modules" | "benchmarks" | "jmh" | "testFixtures") {
                copy_without_tests(&p, &to.join(&name));
            }
        } else if name.ends_with(".java") {
            let _ = std::fs::create_dir_all(to);
            let _ = std::fs::copy(&p, to.join(&name));
        }
    }
}

pub(super) fn assemble_training(name: &str) -> std::path::PathBuf {
    let root = work_dir().join("projects").join(format!("train-{name}"));
    let key = cache_key(|h| tree_fingerprint(&corpus().join(name), h));
    if needs_assembly(&root, &key) {
        copy_without_tests(&corpus().join(name), &root);
        mark_assembled(&root, &key);
    }
    root
}

pub(super) fn java_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() { java_files(&p, out) } else if p.extension().is_some_and(|x| x == "java") { out.push(p) }
    }
}

/// The popup at `offset` of `source`, reduced to what ranking needs.
pub(super) fn popup(svc: &IndexService, file: &str, source: &str, offset: usize) -> Vec<(String, String, String, bool)> {
    let mut seen = std::collections::HashSet::new();
    svc.completion(file, offset, Some(source), MatchCase::default())
        .into_iter()
        .filter_map(|it| {
            let name = ident_of(&it.label).to_string();
            (!name.is_empty() && seen.insert(name.clone())).then_some((name, it.kind, it.detail.unwrap_or_default(), it.deprecated))
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn instance_at(svc: &IndexService, file: &str, text: &str, s: &super::sites::Site, typed: usize, kind: Kind) -> Option<Instance> {
    let truth_name = &text[s.start..s.end];
    if typed > truth_name.len() || !truth_name.is_char_boundary(typed) {
        return None;
    }
    let offset = s.start + typed;
    // the caret sits right after the dot with nothing written yet: a member that was a call loses its argument list too, or
    // `recv.(args)` would not parse and the receiver would not type (the popup comes back empty)
    let cut = if kind == Kind::Member { after_call(text, s.end) } else { s.end };
    let source = format!("{}{}", &text[..offset], &text[cut..]);
    let list = popup(svc, file, &source, offset);
    let truth = list.iter().position(|(n, ..)| n == truth_name);
    if env("BENNU_MLB_SHOW").is_some() {
        eprintln!("[popup] {kind:?} typed {typed} len {} truth {:?} want {truth_name:?} first {:?}", list.len(), truth, list.iter().take(4).map(|x| x.0.as_str()).collect::<Vec<_>>());
    }
    truth?;
    let above = {
        let upto = &text[..s.start];
        let from = upto.rmatch_indices('\n').nth(60).map_or(0, |(i, _)| i);
        &upto[from..]
    };
    let popup_len = list.len();
    let cands: Vec<Cand> = list.into_iter().take(POPUP_CAP).enumerate().map(|(rank, (name, k, detail, deprecated))| Cand { recency: count_word(above, &name), name, rank, kind: k, detail, deprecated }).collect();
    let truth = cands.iter().position(|c| c.name == truth_name).map_or(usize::MAX, |p| p);
    let mut inst = Instance { kind, typed, truth, cands, popup_len, recv_type: String::new(), prev: s.callee.clone().filter(|_| kind == Kind::Member).unwrap_or_default(), callee: String::new(), arg_index: s.arg_index, target: s.target.clone().unwrap_or_default(), param_type: String::new(), param_name: String::new() };
    match kind {
        Kind::Member => {
            let recv = svc.receiver_type_for(file, &source, s.start)?;
            inst.recv_type = simple(&recv);
        }
        Kind::Argument => {
            inst.callee = s.callee.clone().unwrap_or_default();
            if let Some(h) = svc.signature_at(file, &source, offset) {
                if let Some((a, b)) = h.params.get(h.active).copied() {
                    let p = h.label.get(a..b).unwrap_or("").trim();
                    if let Some((ty, name)) = p.rsplit_once(' ') {
                        inst.param_type = ty.trim().to_string();
                        inst.param_name = name.trim().to_string();
                    }
                }
            }
        }
    }
    Some(inst)
}

pub(super) struct Collected {
    pub instances: Vec<Instance>,
    /// Sites asked / sites whose truth was not in the capped popup.
    pub asked: HashMap<(Kind, usize), usize>,
}

pub(super) fn collect(svc: &IndexService, files: &[(String, String)], wanted: usize, harvest: Option<&mut Tables>) -> Collected {
    let mut tables = harvest;
    let mut all: HashMap<Kind, Vec<(usize, super::sites::Site)>> = HashMap::new();
    for (fi, (_, text)) in files.iter().enumerate() {
        for s in sites(text) {
            let t = &text[s.start..s.end];
            match s.position {
                Position::MemberAfterDot if is_plain_name(t) => all.entry(Kind::Member).or_default().push((fi, s)),
                Position::CallArgument if is_plain_name(t) && s.callee.is_some() => all.entry(Kind::Argument).or_default().push((fi, s)),
                _ => {}
            }
        }
    }
    // the habit tables count every site of the training files, sampled or not
    if let Some(tb) = tables.as_deref_mut() {
        for (kind, list) in &all {
            for (fi, s) in list.iter().take(env_num("BENNU_MLB_HARVEST", 8000)) {
                let (file, text) = &files[*fi];
                let name = &text[s.start..s.end];
                match kind {
                    Kind::Member => {
                        let source = format!("{}{}", &text[..s.start], &text[s.end..]);
                        if let Some(r) = svc.receiver_type_for(file, &source, s.start) {
                            bump(&mut tb.member, simple(&r), name);
                            if let Some(p) = &s.callee { bump(&mut tb.chain, p.clone(), name); }
                        }
                    }
                    Kind::Argument => bump(&mut tb.arg, (s.callee.clone().unwrap_or_default(), s.arg_index), name),
                }
            }
        }
    }
    let mut out = Collected { instances: Vec::new(), asked: HashMap::new() };
    // a HashMap's order differs from one process to the next; the order of the instances decides the fit
    let mut all: Vec<_> = all.into_iter().collect();
    all.sort_by_key(|x| x.0);
    for (kind, list) in all {
        let stride = (list.len() / wanted.max(1)).max(1);
        for (fi, s) in list.iter().step_by(stride).take(wanted) {
            let (file, text) = &files[*fi];
            for typed in [0usize, 1] {
                *out.asked.entry((kind, typed)).or_default() += 1;
                if let Some(i) = instance_at(svc, file, text, s, typed, kind) {
                    out.instances.push(i);
                }
            }
        }
    }
    out
}

#[test]
#[ignore = "needs the public corpus: see the module docs"]
fn m3_learned_ranking() {
    let wanted = env_num("BENNU_MLB_N", 500);
    // the popup is read as the provider leaves it: the production ranker is evaluated on it, not stacked on itself
    crate::flc_rank::OFF.store(true, std::sync::atomic::Ordering::Relaxed);
    let skip_train = env("BENNU_MLB_SKIP_TRAIN").is_some();
    let train: Vec<String> = if skip_train { Vec::new() } else { env("BENNU_MLB_TRAIN").map(|v| v.split(',').map(str::to_string).collect()).unwrap_or_else(|| TRAIN.iter().map(|s| s.to_string()).collect()) };
    let (mut tables, mut train_inst) = (Tables::default(), Vec::new());

    for d in super::m3_cache::train_data(&train, wanted) {
        tables.merge(d.tables());
        train_inst.extend(d.instances);
    }
    assert!(skip_train || !tables.is_empty(), "no training data");

    // the ranker: one per kind, on the training popups, each site reading the tables without itself
    let mut rankers: HashMap<Kind, Ranker> = HashMap::new();
    for kind in [Kind::Member, Kind::Argument] {
        let data: Vec<(Vec<Vec<f32>>, usize)> = train_inst
            .iter()
            .filter(|i| i.kind == kind && i.truth < i.cands.len())
            .map(|i| (i.cands.iter().enumerate().map(|(k, c)| features(&tables, i, c, k == i.truth)).collect(), i.truth))
            .collect();
        eprintln!("ranker {kind:?}: {} training sites", data.len());
        rankers.insert(kind, Ranker::train(&data, if kind == Kind::Member { NF_MEMBER } else { crate::flc_rank::N }, 25));
    }

    let only = env("BENNU_MLB_ONLY");
    let mut held_inst: Vec<(String, Instance)> = Vec::new();
    let mut asked: HashMap<(Kind, usize), usize> = HashMap::new();
    let specs: Vec<&ProjectSpec> = PROJECTS.iter().filter(|p| only.as_deref().is_none_or(|o| o == p.name)).collect();
    for (spec, d) in specs.iter().zip(super::m3_cache::eval_data(&specs, wanted)) {
        for (k, v) in d.asked() {
            *asked.entry(k).or_default() += v;
        }
        held_inst.extend(d.instances.into_iter().map(|i| (spec.name.to_string(), i)));
    }
    let report = report(&tables, &rankers, &held_inst, &asked);
    let md = write_result("m3_report.md", &report);
    let mut by_kind: Vec<_> = rankers.iter().collect();
    by_kind.sort_by_key(|x| *x.0);
    let weights: String = by_kind.iter().map(|(k, r)| format!("{k:?}: {:?}\n", r.w)).collect();
    if let Some(r) = rankers.get(&Kind::Argument) {
        eprintln!("pub(crate) const WEIGHTS: [f32; N] = {:?};", r.w);
    }
    write_result("m3_weights.txt", &weights);
    eprintln!("{report}\nreport: {}", md.display());
}

fn report(t: &Tables, rankers: &HashMap<Kind, Ranker>, held: &[(String, Instance)], asked: &HashMap<(Kind, usize), usize>) -> String {
    let mut out = String::from("## M3 — first and top-3 of the right candidate\n\n");
    out.push_str("| position | typed | sites | truth in popup (first 300) | ranker | top-1 | top-3 | MRR |\n|---|---|---|---|---|---|---|---|\n");
    for kind in [Kind::Member, Kind::Argument] {
        for typed in [0usize, 1] {
            let sel: Vec<&Instance> = held.iter().map(|(_, i)| i).filter(|i| i.kind == kind && i.typed == typed).collect();
            let n_asked = asked.get(&(kind, typed)).copied().unwrap_or(sel.len());
            // sites whose truth is outside the capped popup (or not in the popup at all) count as misses for everyone
            let in_top = sel.iter().filter(|i| i.truth < i.cands.len()).count();
            let mut rows: Vec<(&str, Score)> = Vec::new();
            let eval = |f: &dyn Fn(&Instance) -> Vec<usize>| {
                let mut s = Score::default();
                for i in &sel {
                    s.add(if i.truth < i.cands.len() { position_of(&f(i), i.truth) } else { None });
                }
                // popup misses (truth absent from the list) are in `n_asked` but never in `sel`
                s.n = n_asked;
                s
            };
            rows.push(("(a) popup order", eval(&|i| order_popup(i))));
            if !t.is_empty() {
                rows.push(("(b) popup + habit tables (count ≥ 1)", eval(&|i| order_lifted(t, i, 1))));
                rows.push(("(b') popup + habit tables (count ≥ 3)", eval(&|i| order_lifted(t, i, 3))));
                rows.push(("(c) learned ranker", eval(&|i| order_learned(t, &rankers[&kind], i))));
            }
            rows.push(("(p) production ranker (embedded weights)", eval(&|i| order_production(i))));
            for (label, s) in rows {
                out.push_str(&format!(
                    "| {} | {} | {} | {:.1}% | {} | {:.1}% ±{:.1} | {:.1}% | {:.3} |\n",
                    if kind == Kind::Member { "member after dot" } else { "call argument (plain name)" },
                    typed, s.n, pct(in_top, n_asked), label, pct(s.top1, s.n), ci95(s.top1, s.n), pct(s.top3, s.n), s.mrr / s.n.max(1) as f64
                ));
            }
        }
    }
    out.push_str("\n`sites` counts every asked site, including those whose true candidate the popup does not list (they are misses for all four rows).\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inst(names: &[&str], truth: usize) -> Instance {
        Instance {
            kind: Kind::Member, typed: 0, truth,
            cands: names.iter().enumerate().map(|(rank, n)| Cand { name: n.to_string(), rank, kind: "method".into(), detail: String::new(), deprecated: false, recency: 0 }).collect(),
            popup_len: names.len(), recv_type: "List".into(), prev: String::new(), callee: String::new(), arg_index: 0, target: String::new(), param_type: String::new(), param_name: String::new(),
        }
    }

    #[test]
    fn a_member_that_was_a_call_loses_its_argument_list_with_it() {
        let t = "x.get(a, f(b)).size();";
        assert_eq!(&t[after_call(t, 5)..], ".size();");
        assert_eq!(after_call("x.size;", 6), 6);
        assert_eq!(after_call("x.get(", 5), 5);
    }

    #[test]
    fn a_label_is_cut_at_its_identifier() {
        assert_eq!(ident_of("getFoo(int)"), "getFoo");
        assert_eq!(ident_of("size"), "size");
        assert_eq!(ident_of("(x)"), "");
    }

    #[test]
    fn the_tables_lift_what_was_seen_and_leave_the_rest_in_popup_order() {
        let mut t = Tables::default();
        for _ in 0..4 { bump(&mut t.member, "List".to_string(), "size"); }
        bump(&mut t.member, "List".to_string(), "isEmpty");
        let i = inst(&["add", "clear", "isEmpty", "size"], 3);
        assert_eq!(order_lifted(&t, &i, 1), vec![3, 2, 0, 1]);
        assert_eq!(order_lifted(&t, &i, 3), vec![3, 0, 1, 2]);
        assert_eq!(order_popup(&i), vec![0, 1, 2, 3]);
    }

    #[test]
    fn a_sites_own_sighting_is_taken_out_of_the_counts_it_trains_on() {
        let mut t = Tables::default();
        bump(&mut t.member, "List".to_string(), "size");
        let i = inst(&["size"], 0);
        assert_eq!(t.counts(&i, &i.cands[0], false)[0], (1, 1));
        assert_eq!(t.counts(&i, &i.cands[0], true)[0], (0, 0));
    }

    #[test]
    fn the_ranker_learns_to_prefer_what_the_tables_saw() {
        // the true candidate is always the one with a table count, and never first in the popup
        let mut t = Tables::default();
        for _ in 0..5 { bump(&mut t.member, "List".to_string(), "size"); }
        let i = inst(&["add", "clear", "size"], 2);
        let data: Vec<(Vec<Vec<f32>>, usize)> = (0..30).map(|_| (i.cands.iter().map(|c| features(&t, &i, c, false)).collect(), 2)).collect();
        let r = Ranker::train(&data, NF_MEMBER, 40);
        assert_eq!(order_learned(&t, &r, &i)[0], 2);
    }

    #[test]
    fn a_plain_name_is_a_variable_not_a_keyword_or_a_type() {
        assert!(is_plain_name("order"));
        assert!(!is_plain_name("Order"));
        assert!(!is_plain_name("null"));
        assert!(!is_plain_name("a.b"));
    }

    #[test]
    fn tokens_split_camel_and_snake() {
        assert_eq!(tokens("getFooBar"), vec!["get", "foo", "bar"]);
        assert!(jaccard("fooBar", "foo_bar") > 0.99);
    }
}
