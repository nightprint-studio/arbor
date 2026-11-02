//! M8 — methods in *portions*: pieces of a body with a purpose.
//!
//! (a) two segmentations — by data-flow (a new portion starts when a statement reads none of the variables the current portion
//!     defined or updated; a portion that defines nothing is held together by the identifiers of its last statement) and by the
//!     author (a blank line or a line comment between two statements) — and how well they agree, on the methods that have at
//!     least one blank-line boundary;
//! (b) a role per portion from its structure, and their distribution by class of method;
//! (c) the share of held-out portions whose *form* (the shapes of its statements) is among the K most frequent forms of the
//!     public training projects: the question that decides whether a method can be built from portions;
//! (d) retrieval at portion level — role + types coming in + name of the method — and whether the portion's form is within the
//!     first 20 retrieved, to put next to the 48.3% of whole-method skeletons (M4).
//!
//! Methods of 2..=30 statements are used here (M4's 1–8 would cut most multi-portion bodies away).
//!
//! `cargo test --release -p bennu-be mlbench::m8_portions -- --ignored --nocapture`  (`BENNU_MLB_N` methods per project for (d), default 800)

use std::collections::{BTreeMap, HashMap, HashSet};

use super::common::*;
use super::m4_methods::{class_of, subwords};
use super::m5_ngram::tokens;
use super::m78_shared::*;

pub(super) const ROLES: [&str; 8] = ["guard", "preparation", "loop", "try/catch", "construction", "fields", "return", "effect"];

/// Indices of the statements that open a portion, by data-flow.
pub(super) fn dataflow_starts(stmts: &[String]) -> Vec<usize> {
    let mut starts = vec![0];
    let seed = |s: &str| { let d = defined(s); if d.is_empty() { variables(s) } else { d } };
    let mut current = seed(&stmts[0]);
    for (i, s) in stmts.iter().enumerate().skip(1) {
        if variables(s).iter().any(|v| current.contains(v)) {
            current.extend(defined(s));
        } else {
            starts.push(i);
            current = seed(s);
        }
    }
    starts
}

/// Gaps (statement index i ≥ 1 = between i-1 and i) where the author left a blank line / a line comment.
pub(super) fn author_boundaries(src: &str, spans: &[(usize, usize)]) -> (HashSet<usize>, HashSet<usize>) {
    let (mut blank, mut comment) = (HashSet::new(), HashSet::new());
    for i in 1..spans.len() {
        let gap = &src[spans[i - 1].1..spans[i].0];
        let Some(nl) = gap.find('\n') else { continue };
        let lines: Vec<&str> = gap[nl + 1..].split('\n').collect();
        let inner = &lines[..lines.len() - 1];
        if inner.iter().any(|l| l.trim().is_empty()) { blank.insert(i); }
        if lines.iter().any(|l| l.trim_start().starts_with("//") || l.trim_start().starts_with("/*")) { comment.insert(i); }
    }
    (blank, comment)
}

fn portions(stmts: &[String], starts: &[usize]) -> Vec<std::ops::Range<usize>> {
    starts.iter().enumerate().map(|(n, &s)| s..starts.get(n + 1).copied().unwrap_or(stmts.len())).collect()
}

fn word_start(s: &str, word: &str) -> bool {
    s.strip_prefix(word).is_some_and(|r| !r.starts_with(|c: char| c.is_alphanumeric() || c == '_'))
}

fn is_guard(s: &str) -> bool {
    let s = s.trim();
    (word_start(s, "if") && (s.contains("throw ") || s.contains("return")) && !s.contains("else"))
        || ["Objects.requireNonNull", "Validate.", "Assert.", "Preconditions.", "checkArgument", "checkNotNull", "requireNonNull", "assert "].iter().any(|p| s.starts_with(p))
}

/// The role of a portion, from its structure. `locals` are the method's parameters and declared locals.
pub(super) fn role_of(stmts: &[String], locals: &HashSet<String>) -> &'static str {
    let starts = |w: &str| stmts.iter().any(|s| word_start(s.trim(), w));
    if stmts.iter().all(|s| is_guard(s)) { return "guard" }
    if starts("try") { return "try/catch" }
    if starts("for") || starts("while") || starts("do") { return "loop" }
    let field_assign = |s: &String| {
        let s = s.trim();
        s.strip_prefix("this.").is_some() && defined(&format!("x.{s}")).is_empty() && s.contains('=')
            || defined(s).iter().any(|v| !locals.contains(v)) && declaration(s).is_none() && !defined(s).is_empty()
    };
    if stmts.iter().all(field_assign) { return "fields" }
    let ends_return = stmts.last().is_some_and(|s| word_start(s.trim(), "return") || word_start(s.trim(), "throw"));
    if ends_return && stmts.len() == 1 { return "return" }
    let body: &[String] = if ends_return { &stmts[..stmts.len() - 1] } else { stmts };
    let constructs = |s: &String| {
        let t = tokens(s);
        s.contains("new ") || s.contains("->") || s.contains("::")
            || (0..t.len()).any(|i| t.get(i + 1) == Some(&"(") && subwords(t[i]).first().is_some_and(|w| ["to", "build", "create", "map", "convert", "parse", "format", "of", "from", "with", "as", "copy", "wrap"].contains(&w.as_str())))
    };
    if body.iter().any(constructs) { "construction" }
    else if body.iter().any(|s| declaration(s).is_some() || !defined(s).is_empty()) { "preparation" }
    else if ends_return { "return" }
    else { "effect" }
}

fn locals_of(b: &Body) -> HashSet<String> {
    b.m.params.iter().map(|p| p.1.clone()).chain(b.m.statements.iter().filter_map(|s| declaration(s).map(|d| d.1))).collect()
}

fn form_of(stmts: &[String]) -> String {
    stmts.iter().map(|s| shape(s)).collect::<Vec<_>>().join(" ¦ ")
}

/// Types of what a portion reads and did not define itself: parameters and earlier declarations.
fn types_in(b: &Body, range: &std::ops::Range<usize>) -> Vec<String> {
    let mut env: HashMap<String, String> = b.m.params.iter().map(|(t, n)| (n.clone(), simple_type(t).to_string())).collect();
    for s in &b.m.statements[..range.start] {
        if let Some((t, n)) = declaration(s) { env.insert(n, simple_type(&t).to_string()); }
    }
    let local: HashSet<String> = b.m.statements[range.clone()].iter().filter_map(|s| declaration(s).map(|d| d.1)).collect();
    let mut out: Vec<String> = b.m.statements[range.clone()].iter().flat_map(|s| variables(s)).filter(|v| !local.contains(v)).filter_map(|v| env.get(&v).cloned()).collect();
    out.sort();
    out.dedup();
    out
}

struct Portion {
    method: usize,
    role: &'static str,
    form: String,
    len: usize,
    feats: Vec<(String, f32)>,
}

fn portions_of(b: &Body, method: usize) -> Vec<Portion> {
    let stmts = &b.m.statements;
    let locals = locals_of(b);
    let name_words = subwords(&b.m.name);
    portions(stmts, &dataflow_starts(stmts))
        .into_iter()
        .map(|r| {
            let role = role_of(&stmts[r.clone()], &locals);
            let mut feats: Vec<(String, f32)> = vec![(format!("r:{role}"), 3.0)];
            feats.extend(name_words.iter().map(|w| (format!("n:{w}"), 2.0)));
            feats.extend(types_in(b, &r).into_iter().map(|t| (format!("t:{t}"), 1.0)));
            Portion { method, role, form: form_of(&stmts[r.clone()]), len: r.len(), feats }
        })
        .collect()
}

fn render_segmented(b: &Body, src: &str) -> String {
    let stmts = &b.m.statements;
    let starts = dataflow_starts(stmts);
    let (blank, comment) = author_boundaries(src, &b.spans);
    let locals = locals_of(b);
    let params = b.m.params.iter().map(|(t, n)| format!("{t} {n}")).collect::<Vec<_>>().join(", ");
    let mut out = format!("{} {}({params}) {{\n", b.m.ret, b.m.name);
    for r in portions(stmts, &starts) {
        let mark = if blank.contains(&r.start) || comment.contains(&r.start) { " · the author also separated here" } else { "" };
        out.push_str(&format!("    // --- {}{mark}\n", role_of(&stmts[r.clone()], &locals)));
        for s in &stmts[r] { out.push_str(&format!("    {}\n", s.replace('\n', "\n    "))) }
    }
    out.push('}');
    out
}

#[derive(Default, Clone)]
struct PIndex {
    inverted: HashMap<String, Vec<(u32, f32)>>,
    sizes: Vec<f32>,
    method: Vec<usize>,
    form: Vec<String>,
}

impl PIndex {
    fn add(&mut self, p: &Portion) {
        let id = self.sizes.len() as u32;
        self.sizes.push(p.feats.iter().map(|f| f.1).sum());
        self.method.push(p.method);
        self.form.push(p.form.clone());
        for (f, w) in &p.feats { self.inverted.entry(f.clone()).or_default().push((id, *w)) }
    }
}

#[derive(Default, Clone)]
struct Agree { tp: usize, fp: usize, fneg: usize, gaps: usize, methods: usize, f1_sum: f64 }

impl Agree {
    fn add(&mut self, found: &HashSet<usize>, truth: &HashSet<usize>, gaps: usize) {
        let tp = found.intersection(truth).count();
        let (fp, fneg) = (found.len() - tp, truth.len() - tp);
        self.tp += tp; self.fp += fp; self.fneg += fneg; self.gaps += gaps; self.methods += 1;
        let (p, r) = (if tp + fp == 0 { 0.0 } else { tp as f64 / (tp + fp) as f64 }, if tp + fneg == 0 { 0.0 } else { tp as f64 / (tp + fneg) as f64 });
        self.f1_sum += if p + r == 0.0 { 0.0 } else { 2.0 * p * r / (p + r) };
    }
    fn line(&self, name: &str) -> String {
        let p = pct(self.tp, self.tp + self.fp);
        let r = pct(self.tp, self.tp + self.fneg);
        let f = if p + r == 0.0 { 0.0 } else { 2.0 * p * r / (p + r) };
        format!("| {name} | {} | {:.1} | {:.1} | {:.1} | {:.1} |\n", self.methods, p, r, f, 100.0 * self.f1_sum / self.methods.max(1) as f64)
    }
}

#[test]
#[ignore = "needs the public corpus: see the module docs"]
fn m8_portions_of_methods() {
    let wanted = env_num("BENNU_MLB_N", 800);
    let train = train_bodies(30);
    let train: Vec<&Body> = train.iter().filter(|b| b.m.statements.len() >= 2).collect();
    eprintln!("M8: {} training methods of 2..=30 statements", train.len());
    // (c) vocabulary
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut train_portions: Vec<Portion> = Vec::new();
    for (i, b) in train.iter().enumerate() {
        for p in portions_of(b, i) { *counts.entry(p.form.clone()).or_default() += 1; train_portions.push(p) }
    }
    let mut ranked: Vec<(&String, &usize)> = counts.iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    let ks = [20usize, 50, 200, 1000, usize::MAX];
    let tops: Vec<HashSet<&String>> = ks.iter().map(|&k| ranked.iter().take(k).map(|x| x.0).collect()).collect();
    let mut index = PIndex::default();
    for p in &train_portions { index.add(p) }
    let (mut agree_blank, mut agree_any, mut every_gap) = (Agree::default(), Agree::default(), Agree::default());
    let mut roles: BTreeMap<&'static str, BTreeMap<&'static str, usize>> = BTreeMap::new();
    let mut per_method: BTreeMap<&'static str, (usize, usize)> = BTreeMap::new();
    let mut cover: BTreeMap<&'static str, Vec<(usize, usize)>> = BTreeMap::new();
    let mut retr: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut examples: Vec<(usize, String, Vec<&'static str>)> = Vec::new();
    let mut next_method = train.len();
    for spec in PROJECTS {
        let (_, held) = assemble(spec);
        // the project's own portions join the index; the query's own method is skipped by id
        let mut own_methods: Vec<(Body, usize)> = Vec::new();
        for (hi, h) in held.iter().enumerate() {
            for b in bodies_of(&h.text, 1_000_000 + hi, 30).into_iter().filter(|b| b.m.statements.len() >= 2) {
                own_methods.push((b, hi));
            }
        }
        let mut own_index = index.clone();
        let mut all_portions: Vec<Vec<Portion>> = Vec::new();
        for (b, _) in &own_methods {
            let ps = portions_of(b, next_method);
            next_method += 1;
            for p in &ps { own_index.add(p) }
            all_portions.push(ps);
        }
        let mut own_forms: HashMap<&str, usize> = HashMap::new();
        for ps in &all_portions { for p in ps { *own_forms.entry(p.form.as_str()).or_default() += 1 } }
        for (mi, ((b, hi), ps)) in own_methods.iter().zip(&all_portions).enumerate() {
            let class = class_of(&b.m);
            let n = b.m.statements.len();
            // (a)
            let starts = dataflow_starts(&b.m.statements);
            let found: HashSet<usize> = starts.iter().copied().filter(|&s| s > 0).collect();
            let (blank, comment) = author_boundaries(&held[*hi].text, &b.spans);
            let any: HashSet<usize> = blank.union(&comment).copied().collect();
            if !blank.is_empty() {
                agree_blank.add(&found, &blank, n - 1);
                agree_any.add(&found, &any, n - 1);
                every_gap.add(&(1..n).collect(), &any, n - 1);
            }
            // (b)
            let e = per_method.entry(class).or_default();
            e.0 += ps.len();
            e.1 += 1;
            for p in ps { *roles.entry(class).or_default().entry(p.role).or_default() += 1 }
            // (c)
            for p in ps {
                let bucket = match p.len { 1 => "1 statement", 2 => "2 statements", _ => "3 or more" };
                let c = cover.entry(bucket).or_default();
                if c.is_empty() { c.resize(ks.len() + 3, (0, 0)) }
                for (i, t) in tops.iter().enumerate() { c[i].0 += usize::from(t.contains(&p.form)); c[i].1 += 1 }
                let same = ps.iter().filter(|q| q.form == p.form).count();
                let elsewhere = own_forms.get(p.form.as_str()).copied().unwrap_or(0) > same;
                c[ks.len()].0 += usize::from(elsewhere);
                c[ks.len() + 1].0 += usize::from(elsewhere || counts.contains_key(&p.form));
                c[ks.len() + 2].1 += 1;
            }
            // examples
            if (4..=8).contains(&n) && ps.len() >= 3 && !any.is_empty() {
                let rs: HashSet<&str> = ps.iter().map(|p| p.role).collect();
                if rs.len() >= 3 { examples.push((b.m.statements.iter().map(String::len).sum(), render_segmented(b, &held[*hi].text), ps.iter().map(|p| p.role).collect())) }
            }
            // (d) every held-out portion of every `stride`-th method
            let stride = (own_methods.len() / wanted.max(1)).max(1);
            if mi % stride != 0 { continue }
            for p in ps {
                let qsize: f32 = p.feats.iter().map(|f| f.1).sum();
                let mut inter: HashMap<u32, f32> = HashMap::new();
                for (f, w) in &p.feats {
                    for &(id, cw) in own_index.inverted.get(f).map(Vec::as_slice).unwrap_or(&[]) { *inter.entry(id).or_default() += w.min(cw) }
                }
                let me = next_method - own_methods.len() + mi;
                let mut sc: Vec<(usize, f32)> = inter.into_iter().filter(|&(id, _)| own_index.method[id as usize] != me).map(|(id, x)| (id as usize, x / (qsize + own_index.sizes[id as usize] - x))).collect();
                let k = 20.min(sc.len());
                if k > 0 {
                    sc.select_nth_unstable_by(k - 1, |a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
                    sc.truncate(k);
                }
                let found = sc.iter().any(|&(id, _)| own_index.form[id] == p.form);
                let in_vocab = if tops[3].contains(&p.form) { "form among the 1000 most frequent" } else { "rarer form" };
                for key in ["all portions".to_string(), format!("role: {}", p.role), format!("length: {}", match p.len { 1 => "1 statement", 2 => "2 statements", _ => "3 or more" }), in_vocab.to_string()] {
                    let e = retr.entry(key).or_default();
                    e.0 += usize::from(found);
                    e.1 += 1;
                }
            }
        }
    }

    let mut out = String::from("## M8 — methods in portions\n\n### (a) Agreement between the data-flow cuts and the author's cuts\n\nMethods with ≥ 2 statements and at least one blank line between two of them.\n\n| compared with | methods | precision | recall | F1 | mean F1 per method |\n|---|---|---|---|---|---|\n");
    out.push_str(&agree_blank.line("author = blank lines (data-flow vs author)"));
    out.push_str(&agree_any.line("author = blank lines or line comments"));
    out.push_str(&every_gap.line("baseline: cut at every gap (vs blank lines or comments)"));
    out.push_str(&format!("\nGaps between statements in those methods: {}.\n", agree_blank.gaps));
    out.push_str("\n### (b) Roles of the data-flow portions, by class of method (% of portions)\n\n| class | methods | portions per method |");
    for r in ROLES { out.push_str(&format!(" {r} |")) }
    out.push_str("\n|---|---|---|");
    for _ in ROLES { out.push_str("---|") }
    out.push('\n');
    let mut total_roles: BTreeMap<&str, usize> = BTreeMap::new();
    let (mut tp, mut tm) = (0, 0);
    for (c, (np, nm)) in &per_method {
        let rs = &roles[c];
        let sum: usize = rs.values().sum();
        out.push_str(&format!("| {c} | {nm} | {:.2} |", *np as f64 / *nm as f64));
        for r in ROLES { out.push_str(&format!(" {:.1} |", pct(rs.get(r).copied().unwrap_or(0), sum))); *total_roles.entry(r).or_default() += rs.get(r).copied().unwrap_or(0) }
        out.push('\n');
        tp += np; tm += nm;
    }
    let tsum: usize = total_roles.values().sum();
    out.push_str(&format!("| **all** | {tm} | {:.2} |", tp as f64 / tm.max(1) as f64));
    for r in ROLES { out.push_str(&format!(" {:.1} |", pct(total_roles.get(r).copied().unwrap_or(0), tsum))) }
    out.push_str(&format!("\n\n### (c) Vocabulary of portion forms\n\nDistinct forms in the training projects: {}. Share of held-out portions whose form is among the K most frequent of the training projects:\n\n| portions of | held-out portions | K=20 | K=50 | K=200 | K=1000 | any training form | seen in the project's other methods | either |\n|---|---|---|---|---|---|---|---|---|\n", ranked.len()));
    let (mut tc, mut tn) = (vec![0usize; ks.len() + 2], 0usize);
    for (b, c) in &cover {
        let n = c[ks.len() + 2].1;
        out.push_str(&format!("| {b} | {n} |{}\n", (0..ks.len() + 2).map(|i| format!(" {:.1} |", pct(c[i].0, n))).collect::<String>()));
        for i in 0..ks.len() + 2 { tc[i] += c[i].0 }
        tn += n;
    }
    out.push_str(&format!("| **all** | {tn} |{}\n", (0..ks.len() + 2).map(|i| format!(" {:.1} |", pct(tc[i], tn))).collect::<String>()));
    out.push_str("\n### (d) Is the portion's form among the first 20 retrieved by role + incoming types + method name?\n\n| | portions | form in the top 20 |\n|---|---|---|\n");
    for (k, (h, n)) in &retr { out.push_str(&format!("| {k} | {n} | {:.1}% |\n", pct(*h, *n))) }
    out.push_str("\nCompare: the skeleton of the whole method, M4: 48.3% in the top 20 (different population: methods of 1–8 statements).\n");
    examples.sort_by_key(|e| e.0);
    let mut seen: Vec<Vec<&str>> = Vec::new();
    out.push_str("\n### Five methods, as they are cut\n\n");
    for (_, text, rs) in &examples {
        if seen.contains(rs) { continue }
        seen.push(rs.clone());
        out.push_str(&format!("```java\n{text}\n```\n\n"));
        if seen.len() == 5 { break }
    }
    let path = write_result("m8_report.md", &out);
    eprintln!("{out}\nreport: {}", path.display());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stmts(v: &[&str]) -> Vec<String> { v.iter().map(|s| s.to_string()).collect() }

    #[test]
    fn a_statement_that_reads_nothing_of_the_portion_opens_a_new_one() {
        let s = stmts(&["int a = f();", "int b = a + 1;", "log(c);", "return b;"]);
        assert_eq!(dataflow_starts(&s), vec![0, 2, 3]);
    }

    #[test]
    fn the_authors_boundaries_are_blank_lines_and_line_comments_not_trailing_ones() {
        let src = "{\n  a();  // trailing\n  b();\n\n  c();\n  // note\n  d();\n}";
        let at = |w: &str| { let i = src.find(w).unwrap(); (i, i + w.len()) };
        let spans = [at("a();"), at("b();"), at("c();"), at("d();")];
        let (blank, comment) = author_boundaries(src, &spans);
        assert_eq!(blank, HashSet::from([2]));
        assert_eq!(comment, HashSet::from([3]));
    }

    #[test]
    fn roles_follow_the_structure() {
        let l: HashSet<String> = ["a".to_string(), "list".to_string()].into();
        assert_eq!(role_of(&stmts(&["if (a == null) { throw new X(); }"]), &l), "guard");
        assert_eq!(role_of(&stmts(&["for (X x : list) { n += x.size(); }"]), &l), "loop");
        assert_eq!(role_of(&stmts(&["try { f(); } catch (E e) { g(); }"]), &l), "try/catch");
        assert_eq!(role_of(&stmts(&["this.a = a;", "this.b = b;"]), &l), "fields");
        assert_eq!(role_of(&stmts(&["return a;"]), &l), "return");
        assert_eq!(role_of(&stmts(&["X x = new X(a);", "return x;"]), &l), "construction");
        assert_eq!(role_of(&stmts(&["int n = a.size();"]), &l), "preparation");
        assert_eq!(role_of(&stmts(&["log(a);"]), &l), "effect");
    }
}
