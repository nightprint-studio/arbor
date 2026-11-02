//! M9b — a typed enumerator of expressions, with the open project's habits and the words of the context to order them.
//!
//! Positions: the right of `=` (declaration or assignment), after `return`, a call argument — the three of M2. At each site the
//! expression written there is cut out and the real popup is asked what is in scope (variables, fields, own methods, with their
//! types); the members after a dot come from the popup too (memoised by type). The space, to depth 2:
//! `v`, `v.f`, `v.m()`, `v.m().n()`, `own()`, `Type.m()` and `new Type()` for the types and statics the project itself uses, and the
//! trivial literals; arguments are holes (`()` stands for any argument list — the argument machinery fills them). Filtered by the
//! expected type (the declared type, the method's return type, the parameter's type from the signature).
//! Ordered by three signals, with fixed weights (nothing is fitted): how often the project uses that member/form (the site's own use taken
//! out), the words the target's name shares with the expression, and the production argument ranker for the leaf variable.
//!
//! `cargo test --release -p bennu-be mlbench::m9b_expr -- --ignored --nocapture`  (`BENNU_MLB_N` sites per position and project, default 60)

use std::collections::{BTreeMap, HashMap, HashSet};

use super::common::*;
use super::m3_rank::{popup, simple};
use super::m4_methods::subwords;
use super::m5_ngram::{abstract_token, tokens};
use super::sites::{shape_of, sites, Position, Shape};
use crate::flc_rank::{self, Row};
use crate::index_service::IndexService;

const OBJECT_NOISE: &[&str] = &["getClass", "notify", "notifyAll", "wait", "clone", "finalize"];

fn head(t: &str) -> String {
    t.split('<').next().unwrap_or(t).trim().trim_end_matches("[]").trim_end_matches("...").rsplit('.').next().unwrap_or("").to_string()
}

fn boxed(t: &str) -> &str {
    match t { "int" => "Integer", "long" => "Long", "boolean" => "Boolean", "double" => "Double", "float" => "Float", "char" => "Character", "short" => "Short", "byte" => "Byte", x => x }
}

const NUMERIC: &[&str] = &["byte", "short", "char", "int", "long", "float", "double"];
const IMPLEMENTS: &[(&str, &[&str])] = &[
    ("List", &["ArrayList", "LinkedList", "List", "CopyOnWriteArrayList"]),
    ("Map", &["HashMap", "LinkedHashMap", "TreeMap", "ConcurrentHashMap", "Map", "EnumMap"]),
    ("Set", &["HashSet", "LinkedHashSet", "TreeSet", "Set", "EnumSet"]),
    ("Collection", &["List", "Set", "ArrayList", "HashSet", "LinkedList", "LinkedHashSet", "TreeSet", "Collection"]),
    ("Iterable", &["List", "Set", "Collection", "ArrayList", "HashSet"]),
    ("CharSequence", &["String", "StringBuilder", "StringBuffer"]),
    ("Number", &["Integer", "Long", "Double", "Float", "Short", "Byte", "BigDecimal", "BigInteger", "int", "long", "double"]),
    ("Queue", &["ArrayDeque", "LinkedList", "PriorityQueue", "Deque"]),
    ("Deque", &["ArrayDeque", "LinkedList"]),
];

/// Whether something of type `got` can stand where `expected` is wanted — equality, boxing, numeric widening, a few JDK families.
pub(super) fn compatible(expected: &str, got: &str) -> bool {
    let (e, g) = (head(expected), head(got));
    if e.is_empty() || g.is_empty() || e == "Object" || e == "var" || (e.len() <= 2 && e.starts_with(|c: char| c.is_uppercase())) { return true }
    if e == g || boxed(&e) == boxed(&g) { return true }
    if let (Some(a), Some(b)) = (NUMERIC.iter().position(|x| *x == e || boxed(x) == e), NUMERIC.iter().position(|x| *x == g || boxed(x) == g)) {
        return b <= a && e != "char" && g != "boolean";
    }
    IMPLEMENTS.iter().any(|(k, v)| *k == e && v.contains(&g.as_str()))
}

/// The expression with every argument list emptied, generics and `this.` dropped: the form candidates are compared in.
pub(super) fn form_of(expr: &str) -> String {
    let mut out = String::new();
    let (mut depth, mut angle) = (0usize, 0usize);
    let s = expr.trim().replace("this.", "");
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '(' => { if depth == 0 { out.push('(') } depth += 1 }
            ')' => { depth = depth.saturating_sub(1); if depth == 0 { out.push(')') } }
            '<' if depth == 0 && out.ends_with(|ch: char| ch.is_alphanumeric()) && chars[i..].iter().take_while(|x| **x != ' ').any(|x| *x == '>') => angle += 1,
            '>' if angle > 0 && depth == 0 => angle -= 1,
            _ if depth == 0 && angle == 0 && !c.is_whitespace() => out.push(c),
            _ => {}
        }
        i += 1;
    }
    out
}

#[derive(Clone, Debug)]
struct Cand {
    form: String,
    depth: usize,
    use_count: u32,
    /// Words of the identifiers in the form.
    words: Vec<String>,
    /// Rank score (0..1) of the leaf variable by the production ranker; 0 for forms without one.
    rank: f32,
    /// A member name, for the keyword hints.
    member: String,
}

fn words_of(ids: &[&str]) -> Vec<String> {
    let mut w: Vec<String> = ids.iter().flat_map(|i| subwords(i)).collect();
    w.sort();
    w.dedup();
    w
}

fn jaccard(a: &[String], b: &[String]) -> f32 {
    if a.is_empty() || b.is_empty() { return 0.0 }
    let common = a.iter().filter(|x| b.contains(x)).count();
    common as f32 / (a.len() + b.len() - common) as f32
}

/// The project's own habits: (receiver type, member) from typed member sites, ("Type.member") statics and `new Type` from the text.
#[derive(Default)]
struct Habits {
    member: HashMap<(String, String), u32>,
    statics: HashMap<(String, String), u32>,
    news: HashMap<String, u32>,
}

fn static_pairs(text: &str) -> Vec<(String, String)> {
    let t = tokens(text);
    let mut out = Vec::new();
    for w in t.windows(4) {
        if w[0].starts_with(|c: char| c.is_ascii_uppercase()) && abstract_token(w[0]) == "I" && w[1] == "." && w[2].starts_with(|c: char| c.is_ascii_lowercase()) && w[3] == "(" {
            out.push((w[0].to_string(), w[2].to_string()));
        }
    }
    out
}

fn new_types(text: &str) -> Vec<String> {
    let t = tokens(text);
    t.windows(2).filter(|w| w[0] == "new" && w[1].starts_with(|c: char| c.is_ascii_uppercase())).map(|w| w[1].to_string()).collect()
}

impl Habits {
    fn build(svc: &IndexService, files: &[(String, String)], cap: usize) -> Habits {
        let mut h = Habits::default();
        let mut seen_sites = 0usize;
        for (file, text) in files {
            for p in static_pairs(text) { *h.statics.entry(p).or_default() += 1 }
            for t in new_types(text) { *h.news.entry(t).or_default() += 1 }
            for s in sites(text).into_iter().filter(|s| s.position == Position::MemberAfterDot) {
                if seen_sites >= cap { break }
                seen_sites += 1;
                let name = &text[s.start..s.end];
                let source = format!("{}{}", &text[..s.start], &text[s.end..]);
                if let Some(r) = svc.receiver_type_for(file, &source, s.start) {
                    *h.member.entry((simple(&r), name.to_string())).or_default() += 1;
                }
            }
        }
        h
    }
}

struct Memo<'a> {
    svc: &'a IndexService,
    members: HashMap<String, Vec<(String, String, String)>>,
}

impl Memo<'_> {
    /// The members listed after `recv.` at the site, memoised by the receiver's type.
    fn members(&mut self, file: &str, text: &str, s: &super::sites::Site, recv: &str, ty: &str) -> Vec<(String, String, String)> {
        let key = head(ty);
        if let Some(m) = self.members.get(&key) { return m.clone() }
        let buf = format!("{}{}.{}", &text[..s.start], recv, &text[s.end..]);
        let list: Vec<(String, String, String)> = popup(self.svc, file, &buf, s.start + recv.len() + 1)
            .into_iter()
            .filter(|(n, k, ..)| (k.contains("method") || k.contains("field")) && !OBJECT_NOISE.contains(&n.as_str()))
            .map(|(n, k, d, _)| (n, k, d))
            .collect();
        self.members.insert(key, list.clone());
        list
    }
}

fn trivial_literals(expected: &str) -> Vec<&'static str> {
    match head(expected).as_str() {
        "boolean" | "Boolean" => vec!["true", "false"],
        "String" => vec!["\"\"", "null"],
        "int" | "long" | "short" | "byte" | "Integer" | "Long" | "double" | "float" => vec!["0", "1", "-1"],
        "" => vec![],
        _ => vec!["null"],
    }
}

#[derive(Default)]
struct Row9 {
    position: &'static str,
    shape: &'static str,
    in_space: bool,
    space: usize,
    space_d1: usize,
    /// 1-based rank of the truth for each signal order; usize::MAX when absent.
    rank_all: usize,
    rank_use: usize,
    rank_words: usize,
    rank_rank: usize,
    rank_d1: usize,
    /// Rank among the leaf variables by the production ranker alone (what the popup + ranker already shows).
    rank_popup: usize,
    depth_truth: usize,
    keyword: Option<&'static str>,
    keyword_space: usize,
    rank_keyword: usize,
    place: String,
    start_col: usize,
    truth: String,
    top: String,
}

const KEYWORDS: &[(&str, &[&str])] = &[
    ("first", &["first", "get", "getFirst", "findFirst", "peek", "iterator", "next"]),
    ("filter", &["filter", "stream", "removeIf", "select"]),
    ("map", &["map", "collect", "transform", "convert", "stream"]),
    ("sum", &["sum", "reduce", "add", "total"]),
    ("exists", &["contains", "isPresent", "anyMatch", "exists", "has", "isEmpty", "containsKey"]),
    ("count", &["size", "length", "count", "indexOf"]),
    ("sorted", &["sort", "sorted", "compare", "comparing", "reverse"]),
];

fn keyword_of(words: &[String]) -> Option<&'static str> {
    KEYWORDS.iter().find(|(k, _)| words.iter().any(|w| w == k || (k.len() > 4 && w.starts_with(&k[..k.len() - 1])) || (*k == "count" && (w == "number" || w == "size" || w == "total")) || (*k == "sorted" && w == "ordered") || (*k == "exists" && (w == "has" || w == "contains" || w == "found")) || (*k == "filter" && w == "filtered") || (*k == "map" && w == "mapped"))).map(|(k, _)| *k)
}

fn percentile(v: &mut Vec<usize>, p: f64) -> usize {
    if v.is_empty() { return 0 }
    v.sort();
    v[((v.len() - 1) as f64 * p) as usize]
}

#[test]
#[ignore = "needs the public corpus and a project index: see the module docs"]
fn m9b_typed_enumeration() {
    let wanted = env_num("BENNU_MLB_N", 60);
    let svc = IndexService::global();
    crate::flc_rank::OFF.store(true, std::sync::atomic::Ordering::Relaxed);
    let m1: HashMap<String, (String, bool, String)> = std::fs::read_to_string(work_dir().join("results/full_m1_rows.tsv")).unwrap_or_default().lines().skip(1).filter_map(|l| {
        let f: Vec<&str> = l.split('\t').collect();
        (f.len() >= 10).then(|| (format!("{}|{}", f[0], f[7]), (f[1].to_string(), f[3] == "1", f[9].to_string())))
    }).collect();
    let mut rows: Vec<Row9> = Vec::new();
    let mut comments = (0usize, 0usize);
    for spec in PROJECTS {
        let (root, held) = assemble(spec);
        open_and_wait(&root);
        let mut paths = Vec::new();
        super::m3_rank::java_files(&root, &mut paths);
        paths.sort();
        let files: Vec<(String, String)> = paths.iter().map(|p| (p.to_string_lossy().into_owned(), std::fs::read_to_string(p).unwrap_or_default())).collect();
        let habits = Habits::build(svc, &files, 12_000);
        let mut memo = Memo { svc, members: HashMap::new() };
        let mut by_position: BTreeMap<Position, Vec<(usize, super::sites::Site)>> = BTreeMap::new();
        for (hi, h) in held.iter().enumerate() {
            for s in sites(&h.text) {
                if matches!(s.position, Position::AfterAssign | Position::AfterReturn | Position::CallArgument) { by_position.entry(s.position).or_default().push((hi, s)) }
            }
        }
        for (position, list) in by_position {
            let stride = (list.len() / wanted.max(1)).max(1);
            for (hi, s) in list.iter().step_by(stride).take(wanted) {
                let h = &held[*hi];
                let file = h.path.to_string_lossy().into_owned();
                let truth_text = &h.text[s.start..s.end];
                let truth_form = form_of(truth_text);
                let shape = shape_of(truth_text);
                let buf = format!("{}{}", &h.text[..s.start], &h.text[s.end..]);
                let root_list = popup(svc, &file, &buf, s.start);
                // what the position wants, and the words of the name it is for
                let (expected, target_name, param_name) = match position {
                    Position::AfterAssign => {
                        let t = s.target.clone().unwrap_or_default();
                        (s.declared.clone().or_else(|| root_list.iter().find(|x| x.0 == t).map(|x| x.2.clone())).unwrap_or_default(), t, String::new())
                    }
                    Position::AfterReturn => {
                        let method = h.text[..s.start].rfind(") {").and_then(|e| h.text[..e].rfind('(')).map(|p| h.text[..p].rsplit(|c: char| !c.is_alphanumeric() && c != '_').next().unwrap_or("").to_string()).unwrap_or_default();
                        (s.declared.clone().unwrap_or_default(), method.trim_start_matches("get").trim_start_matches("is").to_string(), String::new())
                    }
                    _ => {
                        let (mut ty, mut nm) = (String::new(), String::new());
                        if let Some(sig) = svc.signature_at(&file, &buf, s.start) {
                            if let Some((a, b)) = sig.params.get(sig.active).copied() {
                                if let Some((t, n)) = sig.label.get(a..b).unwrap_or("").trim().rsplit_once(' ') { ty = t.trim().to_string(); nm = n.trim().to_string() }
                            }
                        }
                        (ty, nm.clone(), nm)
                    }
                };
                let _ = param_name;
                let target_words = subwords(&target_name);
                // the ranker's view of the leaf names
                let leaf: Vec<&(String, String, String, bool)> = root_list.iter().filter(|x| x.1.contains("local") || x.1.contains("param") || x.1.contains("field")).collect();
                let leaf_rows: Vec<Row> = leaf.iter().enumerate().map(|(rank, (n, k, d, dep))| Row { name: n, kind: k, detail: d, deprecated: *dep, recency: 0, rank }).collect();
                let order = flc_rank::order(&expected, &target_name, &leaf_rows, 0);
                let rank_score: HashMap<&str, f32> = order.iter().enumerate().map(|(pos, &i)| (leaf[i].0.as_str(), 1.0 / (1.0 + pos as f32))).collect();
                let above = { let upto = &h.text[..s.start]; let from = upto.rmatch_indices('\n').nth(60).map_or(0, |(i, _)| i); &upto[from..] };
                let mut cands: Vec<Cand> = Vec::new();
                let mut seen: HashSet<String> = HashSet::new();
                let own_pair_key = |f: &str| form_of(f) == truth_form;
                let mut push = |c: Cand, cands: &mut Vec<Cand>| { if seen.insert(c.form.clone()) { cands.push(c) } };
                for (name, kind, ty, _) in &root_list {
                    let is_var = kind.contains("local") || kind.contains("param") || kind.contains("field");
                    if is_var && compatible(&expected, ty) {
                        let recency = above.matches(name.as_str()).count() as u32;
                        push(Cand { form: name.clone(), depth: 0, use_count: recency, words: words_of(&[name]), rank: rank_score.get(name.as_str()).copied().unwrap_or(0.0), member: String::new() }, &mut cands);
                    }
                    if kind.contains("method") && compatible(&expected, ty) && ty != "void" {
                        push(Cand { form: format!("{name}()"), depth: 1, use_count: 0, words: words_of(&[name]), rank: 0.0, member: name.clone() }, &mut cands);
                    }
                }
                // members of the variables, one and two links
                for (v, kind, ty, _) in leaf.iter().map(|x| (&x.0, &x.1, &x.2, x.3)) {
                    let _ = kind;
                    let r = rank_score.get(v.as_str()).copied().unwrap_or(0.0);
                    let tyh = simple(ty);
                    for (m, mk, mt) in memo.members(&file, &h.text, s, v, ty) {
                        let is_call = mk.contains("method");
                        let form1 = if is_call { format!("{v}.{m}()") } else { format!("{v}.{m}") };
                        let used = habits.member.get(&(tyh.clone(), m.clone())).copied().unwrap_or(0);
                        if mt != "void" && compatible(&expected, &mt) {
                            push(Cand { form: form1.clone(), depth: 1, use_count: used, words: words_of(&[v, &m]), rank: r, member: m.clone() }, &mut cands);
                        }
                        // depth 2: the result is a receiver again
                        if is_call && !matches!(head(&mt).as_str(), "void" | "int" | "long" | "boolean" | "double" | "float" | "char" | "byte" | "short" | "") {
                            for (m2, mk2, mt2) in memo.members(&file, &h.text, s, &form1, &mt) {
                                if mt2 == "void" || !compatible(&expected, &mt2) { continue }
                                let form2 = if mk2.contains("method") { format!("{form1}.{m2}()") } else { format!("{form1}.{m2}") };
                                let used2 = habits.member.get(&(head(&mt), m2.clone())).copied().unwrap_or(0);
                                push(Cand { form: form2, depth: 2, use_count: used.min(used2).max(used2 / 2), words: words_of(&[v, &m, &m2]), rank: r, member: m2.clone() }, &mut cands);
                            }
                        }
                    }
                }
                // statics and constructors the project itself uses
                let mut statics: Vec<(&(String, String), &u32)> = habits.statics.iter().collect();
                statics.sort();
                for ((t, m), n) in statics {
                    let ret = memo.members(&file, &h.text, s, t, t).into_iter().find(|x| &x.0 == m).map(|x| x.2);
                    if ret.as_deref().is_some_and(|r| r != "void" && compatible(&expected, r)) {
                        push(Cand { form: format!("{t}.{m}()"), depth: 1, use_count: *n, words: words_of(&[t, m]), rank: 0.0, member: m.clone() }, &mut cands);
                    }
                }
                let mut news: Vec<(&String, &u32)> = habits.news.iter().collect();
                news.sort();
                for (t, n) in news {
                    if compatible(&expected, t) && !expected.is_empty() {
                        push(Cand { form: format!("new{t}()"), depth: 1, use_count: *n, words: words_of(&[t]), rank: 0.0, member: t.clone() }, &mut cands);
                    }
                }
                for lit in trivial_literals(&expected) {
                    push(Cand { form: lit.to_string(), depth: 0, use_count: 0, words: Vec::new(), rank: 0.0, member: String::new() }, &mut cands);
                }
                // leave the site's own use out of the counts
                for c in cands.iter_mut() { if own_pair_key(&c.form) { c.use_count = c.use_count.saturating_sub(1) } }
                // order
                let hint = keyword_of(&target_words);
                let score = |c: &Cand, w: [f32; 3]| w[0] * (1.0 + c.use_count as f32).ln() + w[1] * jaccard(&target_words, &c.words) + w[2] * c.rank - 0.01 * c.depth as f32;
                let rank_of = |w: [f32; 3], only_d1: bool, boost: Option<&[&str]>| -> usize {
                    let boosted = |c: &Cand| boost.is_some_and(|b| b.iter().any(|x| c.member == *x));
                    let mut v: Vec<(&Cand, f32)> = cands.iter().filter(|c| !only_d1 || c.depth <= 1).map(|c| (c, score(c, w) + if boosted(c) { 5.0 } else { 0.0 })).collect();
                    v.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.form.cmp(&b.0.form)));
                    v.iter().position(|x| x.0.form == truth_form).map_or(usize::MAX, |p| p + 1)
                };
                let all = [1.0, 3.0, 2.0];
                let mut r = Row9 { position: position.label(), shape: shape.label(), place: format!("{}:{}", h.path.file_name().unwrap().to_string_lossy(), h.text[..s.start].matches('\n').count()), start_col: s.start - h.text[..s.start].rfind('\n').map_or(0, |p| p + 1), truth: truth_form.clone(), ..Row9::default() };
                r.in_space = cands.iter().any(|c| c.form == truth_form);
                r.space = cands.len();
                r.space_d1 = cands.iter().filter(|c| c.depth <= 1).count();
                r.depth_truth = cands.iter().find(|c| c.form == truth_form).map_or(0, |c| c.depth);
                r.rank_all = rank_of(all, false, None);
                r.rank_use = rank_of([1.0, 0.0, 0.0], false, None);
                r.rank_words = rank_of([0.0, 1.0, 0.0], false, None);
                r.rank_rank = rank_of([0.0, 0.0, 1.0], false, None);
                r.rank_d1 = rank_of(all, true, None);
                r.rank_popup = if cands.iter().any(|c| c.form == truth_form && c.depth == 0 && c.rank > 0.0) { cands.iter().filter(|c| c.depth == 0 && c.rank > 0.0).filter(|c| c.rank > cands.iter().find(|x| x.form == truth_form).map_or(0.0, |x| x.rank)).count() + 1 } else { usize::MAX };
                if let Some(k) = hint {
                    let hints = KEYWORDS.iter().find(|x| x.0 == k).map(|x| x.1).unwrap_or(&[]);
                    r.keyword = Some(k);
                    r.keyword_space = cands.iter().filter(|c| c.depth == 0 || hints.iter().any(|x| c.member == *x)).count();
                    r.rank_keyword = rank_of(all, false, Some(hints));
                }
                // the first line comment just above, if there is one
                let prev_line = h.text[..s.start].lines().rev().nth(1).unwrap_or("").trim();
                comments.1 += 1;
                if prev_line.starts_with("//") { comments.0 += 1 }
                let mut best: Vec<(&Cand, f32)> = cands.iter().map(|c| (c, score(c, all))).collect();
                best.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.form.cmp(&b.0.form)));
                r.top = best.first().map(|x| x.0.form.clone()).unwrap_or_default();
                let key = format!("{}|{}:{}:{}", spec.name, h.path.file_name().unwrap().to_string_lossy(), h.text[..s.start].matches('\n').count(), r.start_col);
                if let Some((layer, exact, got)) = m1.get(&key) {
                    // the cascade was asked at exactly this place: did it write this expression?
                    let found = *exact || form_of(got.trim().trim_end_matches(';')) == truth_form;
                    r.place = format!("M1:{layer}:{}", u8::from(found));
                }
                rows.push(r);
            }
        }
        svc.forget(&root.to_string_lossy());
    }
    report(&rows, comments);
}

fn report(rows: &[Row9], comments: (usize, usize)) {
    let at = |f: &dyn Fn(&Row9) -> usize, k: usize, sel: &[&Row9]| pct(sel.iter().filter(|r| f(r) <= k).count(), sel.len());
    let mut out = String::from("## M9b — typed enumeration of expressions (arguments are holes; depth counts links of a chain)\n\n### Space and rank, by position\n\n| position | sites | truth in the space | space median | p90 | depth ≤ 1 median | top-1 | top-3 | top-5 | top-5 at depth ≤ 1 |\n|---|---|---|---|---|---|---|---|---|---|\n");
    let mut all_rows: Vec<(&str, Vec<&Row9>)> = Vec::new();
    for p in ["after =", "after return", "call argument"] { all_rows.push((p, rows.iter().filter(|r| r.position == p).collect())) }
    all_rows.push(("**all**", rows.iter().collect()));
    for (name, sel) in &all_rows {
        let (mut sp, mut sp1): (Vec<usize>, Vec<usize>) = (sel.iter().map(|r| r.space).collect(), sel.iter().map(|r| r.space_d1).collect());
        out.push_str(&format!("| {name} | {} | {:.1}% | {} | {} | {} | {:.1}% | {:.1}% | {:.1}% | {:.1}% |\n", sel.len(), pct(sel.iter().filter(|r| r.in_space).count(), sel.len()), percentile(&mut sp, 0.5), percentile(&mut sp, 0.9), percentile(&mut sp1, 0.5), at(&|r| r.rank_all, 1, sel), at(&|r| r.rank_all, 3, sel), at(&|r| r.rank_all, 5, sel), at(&|r| r.rank_d1, 5, sel)));
    }
    out.push_str("\n### By shape of the right-hand side\n\n| shape | sites | in the space | top-1 | top-3 | top-5 |\n|---|---|---|---|---|---|\n");
    let mut shapes: Vec<&str> = rows.iter().map(|r| r.shape).collect();
    shapes.sort();
    shapes.dedup();
    for s in shapes {
        let sel: Vec<&Row9> = rows.iter().filter(|r| r.shape == s).collect();
        out.push_str(&format!("| {s} | {} | {:.1}% | {:.1}% | {:.1}% | {:.1}% |\n", sel.len(), pct(sel.iter().filter(|r| r.in_space).count(), sel.len()), at(&|r| r.rank_all, 1, &sel), at(&|r| r.rank_all, 3, &sel), at(&|r| r.rank_all, 5, &sel)));
    }
    let all: Vec<&Row9> = rows.iter().collect();
    out.push_str("\n### Which signal orders\n\n| order by | top-1 | top-3 | top-5 |\n|---|---|---|---|\n");
    for (n, f) in [("use in the project", (|r: &Row9| r.rank_use) as fn(&Row9) -> usize), ("words of the name", |r| r.rank_words), ("production ranker (leaf variable)", |r| r.rank_rank), ("all three", |r| r.rank_all)] {
        out.push_str(&format!("| {n} | {:.1}% | {:.1}% | {:.1}% |\n", at(&f, 1, &all), at(&f, 3, &all), at(&f, 5, &all)));
    }
    let var: Vec<&Row9> = rows.iter().filter(|r| r.shape == Shape::Variable.label()).collect();
    out.push_str(&format!("\nPlain variables: popup + production ranker alone puts the right one first in {:.1}% / top-3 {:.1}%; the enumeration in {:.1}% / {:.1}%.\n", at(&|r| r.rank_popup, 1, &var), at(&|r| r.rank_popup, 3, &var), at(&|r| r.rank_all, 1, &var), at(&|r| r.rank_all, 3, &var)));
    // the cascade, where it was asked at the very same place
    let asked: Vec<&Row9> = rows.iter().filter(|r| r.place.starts_with("M1:")).collect();
    let found = |r: &&Row9| r.place.ends_with(":1");
    let (cascade, synth5) = (asked.iter().filter(|r| found(r)).count(), asked.iter().filter(|r| r.rank_all <= 5).count());
    let marginal = asked.iter().filter(|r| !found(r) && r.rank_all <= 5).count();
    let marginal1 = asked.iter().filter(|r| !found(r) && r.rank_all <= 1).count();
    out.push_str(&format!("\n### Against the cascade (M1 asked at exactly the same place)\n\nSites where M1 asked at the very place: {}. The cascade wrote that expression in {} ({:.1}%); the enumeration has it in the top-5 in {} ({:.1}%); **right in the top-5 where no cascade layer found it: {} ({:.1}% of those places)**, first place: {} ({:.1}%).\n", asked.len(), cascade, pct(cascade, asked.len()), synth5, pct(synth5, asked.len()), marginal, pct(marginal, asked.len()), marginal1, pct(marginal1, asked.len())));
    let kw: Vec<&Row9> = rows.iter().filter(|r| r.keyword.is_some()).collect();
    let (mut sp_plain, mut sp_kw): (Vec<usize>, Vec<usize>) = (kw.iter().map(|r| r.space).collect(), kw.iter().map(|r| r.keyword_space).collect());
    out.push_str(&format!("\n### With a keyword (read from the name of the target)\n\nSites whose target name carries a keyword (first, filter, map, sum, exists, count, sorted): {} of {}. Space median {} → {} with the hint as a filter on chains; top-1 {:.1}% → {:.1}%, top-5 {:.1}% → {:.1}% when candidates whose member matches the hint are boosted. Line comments right above the site: {} of {} sites.\n", kw.len(), rows.len(), percentile(&mut sp_plain, 0.5), percentile(&mut sp_kw, 0.5), at(&|r| r.rank_all, 1, &kw), at(&|r| r.rank_keyword, 1, &kw), at(&|r| r.rank_all, 5, &kw), at(&|r| r.rank_keyword, 5, &kw), comments.0, comments.1));
    // examples
    let mut ex = String::from("\n### Examples\n\n");
    let mut shown = (0, 0, 0);
    for r in rows.iter().filter(|r| r.truth.len() > 6 && r.truth.len() < 40) {
        if r.rank_all == 1 && shown.0 < 2 { shown.0 += 1; ex.push_str(&format!("- right ({}): `{}`\n", r.position, r.truth)) }
        else if r.in_space && r.rank_all > 5 && shown.1 < 2 { shown.1 += 1; ex.push_str(&format!("- wrong ({}): wanted `{}` (rank {}), first was `{}`\n", r.position, r.truth, r.rank_all, r.top)) }
        else if r.shape != Shape::Variable.label() && r.shape != Shape::Literal.label() && r.shape != Shape::New.label() && r.rank_all <= 3 && r.rank_all > 1 && shown.2 < 1 { shown.2 += 1; ex.push_str(&format!("- a call the popup does not list ({}): `{}` — enumeration rank {}, first was `{}`\n", r.position, r.truth, r.rank_all, r.top)) }
    }
    out.push_str(&ex);
    let path = write_result("m9b_report.md", &out);
    eprintln!("{out}\nreport: {}", path.display());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_form_empties_arguments_and_drops_generics_and_this() {
        assert_eq!(form_of("this.items.get(i).name()"), "items.get().name()");
        assert_eq!(form_of("new ArrayList<>(other)"), "newArrayList()");
        assert_eq!(form_of("Collections.emptyList()"), "Collections.emptyList()");
        assert_eq!(form_of("a"), "a");
    }

    #[test]
    fn types_are_compatible_by_name_boxing_widening_and_the_jdk_families() {
        assert!(compatible("String", "String"));
        assert!(compatible("long", "int"));
        assert!(!compatible("int", "long"));
        assert!(compatible("List<String>", "ArrayList<String>"));
        assert!(compatible("Integer", "int"));
        assert!(compatible("Object", "Foo"));
        assert!(!compatible("String", "Foo"));
        assert!(compatible("T", "Foo"));
    }

    #[test]
    fn statics_and_constructors_are_read_from_the_text() {
        let t = "x = Paths.get(a); y = new HashMap<>(); z = foo.bar(); w = Objects.requireNonNull(q);";
        assert_eq!(static_pairs(t), vec![("Paths".to_string(), "get".to_string()), ("Objects".to_string(), "requireNonNull".to_string())]);
        assert_eq!(new_types(t), vec!["HashMap".to_string()]);
    }

    #[test]
    fn a_keyword_is_read_from_the_words_of_a_name() {
        assert_eq!(keyword_of(&subwords("firstUser")), Some("first"));
        assert_eq!(keyword_of(&subwords("userCount")), Some("count"));
        assert_eq!(keyword_of(&subwords("filteredItems")), Some("filter"));
        assert_eq!(keyword_of(&subwords("name")), None);
    }
}
