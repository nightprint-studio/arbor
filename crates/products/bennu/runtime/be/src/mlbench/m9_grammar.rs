//! M9 — methods "alla Spring Data": the *name* is read in a fixed vocabulary and the *body* is composed from the open project.
//!
//! 1. A deterministic grammar for names: action (find/get/load/list/search → `find`, count, exists/has/is, delete/remove, save/create/
//!    update/put/add, to/convert/map/as, validate/check/verify/ensure, set, build), subject, conditions (`By X And/Or Y` with the usual
//!    operators), ordering (`OrderBy … Asc/Desc`), modifiers (`First`/`Top`/`Distinct`) and the shape of the result read off the signature.
//!    A name is *read* when the action is known and every condition's property words are parameters or fields of the class.
//! 2. For the key (action, shape of the result, number and types of the parameters) the open project's own methods are the neighbours —
//!    the method being written is left out, nothing comes from outside the project. The neighbour's body is rewritten: the words of its
//!    subject and conditions become ours inside every identifier (`findByCodiceCliente` → `findByCodiceFornitore`, `Cliente` → `Fornitore`),
//!    then M4's adaptation (parameters by position, derived property, free names by `flc_rank`). A body is produced only if every identifier
//!    it uses exists somewhere in the project.
//!
//! Three variants, to isolate what the grammar adds: **B** the neighbour by name+signature only, as M4 did (no key, no vocabulary check);
//! **K** the neighbour under the key, names untouched; **G** the key and the word substitution.
//!
//! `cargo test --release -p bennu-be mlbench::m9_grammar -- --ignored --nocapture`  (`BENNU_MLB_N` methods per project, default 800;
//! `BENNU_MLB_NC` methods per project that are also compiled, default 100)

use std::collections::{BTreeMap, HashMap, HashSet};

use bennu_java::prelude::parse_java;
use tree_sitter::Node;

use super::common::*;
use super::m4_fill::{adapt, body_errors, body_text, class_fields};
use super::m4_methods::{class_of, features, normalize, rust_files, subwords, Corpus, Method};
use super::m5_ngram::{abstract_token, tokens};
use super::m78_shared::*;
use crate::index_service::IndexService;

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub(super) enum Action { Find, Count, Exists, Delete, Save, Convert, Validate, Set, Build }

impl Action {
    pub(super) fn label(self) -> &'static str {
        match self { Action::Find => "find/get/load/list", Action::Count => "count", Action::Exists => "exists/has/is", Action::Delete => "delete/remove", Action::Save => "save/create/update/add", Action::Convert => "to/convert/map/as", Action::Validate => "validate/check/verify", Action::Set => "set", Action::Build => "build" }
    }
}

const ACTIONS: &[(&str, Action)] = &[
    ("find", Action::Find), ("get", Action::Find), ("load", Action::Find), ("list", Action::Find), ("search", Action::Find), ("read", Action::Find), ("fetch", Action::Find), ("query", Action::Find), ("select", Action::Find), ("retrieve", Action::Find), ("lookup", Action::Find),
    ("count", Action::Count),
    ("exists", Action::Exists), ("has", Action::Exists), ("is", Action::Exists), ("contains", Action::Exists),
    ("delete", Action::Delete), ("remove", Action::Delete), ("clear", Action::Delete), ("drop", Action::Delete),
    ("save", Action::Save), ("create", Action::Save), ("update", Action::Save), ("put", Action::Save), ("add", Action::Save), ("insert", Action::Save), ("store", Action::Save), ("persist", Action::Save), ("register", Action::Save),
    ("to", Action::Convert), ("convert", Action::Convert), ("map", Action::Convert), ("as", Action::Convert), ("from", Action::Convert), ("of", Action::Convert), ("parse", Action::Convert), ("format", Action::Convert), ("decode", Action::Convert), ("encode", Action::Convert), ("wrap", Action::Convert),
    ("validate", Action::Validate), ("check", Action::Validate), ("verify", Action::Validate), ("ensure", Action::Validate), ("assert", Action::Validate), ("require", Action::Validate),
    ("set", Action::Set),
    ("build", Action::Build), ("make", Action::Build), ("new", Action::Build),
];
const MODIFIERS: &[&str] = &["first", "top", "distinct", "all", "one", "optional", "any", "latest", "last"];
const OPERATORS: &[&str] = &["between", "in", "not", "null", "like", "containing", "contains", "starts", "ends", "with", "greater", "than", "less", "after", "before", "is", "true", "false", "ignore", "case", "equals", "empty", "or", "equal"];

/// A method name read in the grammar.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Parsed {
    pub action: Action,
    pub subject: Vec<String>,
    pub modifiers: Vec<String>,
    /// The property words of each condition after `By`.
    pub conds: Vec<Vec<String>>,
    pub order: Vec<String>,
}

pub(super) fn parse_name(name: &str) -> Option<Parsed> {
    let w = subwords(name);
    let action = ACTIONS.iter().find(|(a, _)| Some(*a) == w.first().map(String::as_str))?.1;
    let rest = &w[1..];
    let (head, order) = match rest.windows(2).position(|p| p[0] == "order" && p[1] == "by") {
        Some(i) => (&rest[..i], rest[i + 2..].iter().filter(|x| *x != "asc" && *x != "desc").cloned().collect()),
        None => (rest, Vec::new()),
    };
    let (pre, cond_words) = match head.iter().position(|x| x == "by") {
        Some(i) => (&head[..i], &head[i + 1..]),
        None => (head, &[][..]),
    };
    let (modifiers, subject): (Vec<String>, Vec<String>) = pre.iter().cloned().partition(|x| MODIFIERS.contains(&x.as_str()));
    let mut conds: Vec<Vec<String>> = Vec::new();
    let mut cur: Vec<String> = Vec::new();
    for x in cond_words {
        if x == "and" || x == "or" {
            conds.push(std::mem::take(&mut cur));
        } else if !OPERATORS.contains(&x.as_str()) {
            cur.push(x.clone());
        }
    }
    conds.push(cur);
    conds.retain(|c| !c.is_empty());
    Some(Parsed { action, subject, modifiers, conds, order })
}

impl Parsed {
    /// Every condition names a parameter or a field of the class.
    pub(super) fn resolved(&self, params: &[(String, String)], fields: &[(String, String)]) -> bool {
        let known: HashSet<String> = params.iter().map(|p| &p.1).chain(fields.iter().map(|f| &f.1)).flat_map(|n| subwords(n)).collect();
        self.conds.iter().all(|c| c.iter().all(|w| known.contains(w)))
    }
}

pub(super) fn result_form(ret: &str) -> &'static str {
    let head = ret.split('<').next().unwrap_or(ret).trim().rsplit('.').next().unwrap_or("");
    match head {
        "void" => "void",
        "boolean" | "Boolean" => "boolean",
        "int" | "long" | "short" | "byte" | "double" | "float" | "Integer" | "Long" | "Short" | "Double" | "Float" | "BigDecimal" | "BigInteger" => "number",
        "Optional" | "OptionalInt" | "OptionalLong" => "optional",
        "List" | "Set" | "Collection" | "Iterable" | "Page" | "Slice" | "Stream" | "Map" | "ArrayList" | "Iterator" | "Flux" => "list",
        h if ret.ends_with("[]") || h.is_empty() => "list",
        _ => "object",
    }
}

type Key = (Action, &'static str, usize, String);

fn key_of(p: &Parsed, m: &Method) -> Key {
    (p.action, result_form(&m.ret), m.params.len(), m.params.iter().map(|(t, _)| simple_type(t)).collect::<Vec<_>>().join(","))
}

// ---- words in identifiers ---------------------------------------------------------------------------------------------------

/// Byte spans of the words of an identifier (`findByCodice_Cliente` → find, By, Codice, Cliente).
fn word_spans(id: &str) -> Vec<(usize, usize)> {
    let chars: Vec<(usize, char)> = id.char_indices().collect();
    let (mut out, mut start) = (Vec::new(), None::<usize>);
    for (k, &(i, c)) in chars.iter().enumerate() {
        if !c.is_alphanumeric() {
            if let Some(s) = start.take() { out.push((s, i)) }
            continue;
        }
        let boundary = c.is_uppercase() && k > 0 && chars[k - 1].1.is_alphanumeric() && (chars[k - 1].1.is_lowercase() || chars.get(k + 1).is_some_and(|n| n.1.is_lowercase()));
        if boundary { if let Some(s) = start.take() { out.push((s, i)) } }
        if start.is_none() { start = Some(i) }
    }
    if let Some(s) = start { out.push((s, id.len())) }
    out
}

fn capitalised(w: &str) -> String {
    let mut c = w.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// `id` with the run of words `from` replaced by `to`, written in the style of what it replaces.
pub(super) fn replace_words(id: &str, from: &[String], to: &[String]) -> String {
    if from.is_empty() || to.is_empty() { return id.to_string() }
    let spans = word_spans(id);
    let lower: Vec<String> = spans.iter().map(|&(a, b)| id[a..b].to_lowercase()).collect();
    let Some(at) = (0..lower.len()).find(|&i| lower[i..].starts_with(from)) else { return id.to_string() };
    let (a, b) = (spans[at].0, spans[at + from.len() - 1].1);
    let upper_first = id[a..].chars().next().is_some_and(char::is_uppercase);
    let snake = id.contains('_');
    let upper_all = id[a..b].chars().all(|c| !c.is_lowercase());
    let written: String = to.iter().enumerate().map(|(i, w)| {
        if upper_all && id[a..b].len() > 1 { w.to_uppercase() + if i + 1 < to.len() { "_" } else { "" } }
        else if snake { w.clone() + if i + 1 < to.len() { "_" } else { "" } }
        else if i == 0 && !upper_first { w.clone() } else { capitalised(w) }
    }).collect();
    format!("{}{}{}", &id[..a], written, &id[b..])
}

fn singular(w: &str) -> String {
    w.strip_suffix("ies").map(|s| format!("{s}y")).or_else(|| w.strip_suffix('s').filter(|s| s.len() > 2).map(str::to_string)).unwrap_or_else(|| w.to_string())
}

/// The word groups of the neighbour that become ours: subject, then each condition, then the ordering.
fn word_pairs(n: &Parsed, t: &Parsed) -> Vec<(Vec<String>, Vec<String>)> {
    let mut pairs = Vec::new();
    let mut push = |a: &[String], b: &[String]| {
        if !a.is_empty() && !b.is_empty() && a != b {
            // the words that differ: what the two groups share at either end is not a change
            let head = a.iter().zip(b).take_while(|(x, y)| x == y).count();
            let tail = a[head..].iter().rev().zip(b[head..].iter().rev()).take_while(|(x, y)| x == y).count();
            let (da, db) = (&a[head..a.len() - tail], &b[head..b.len() - tail]);
            if !da.is_empty() && !db.is_empty() && (da.len() < a.len() || db.len() < b.len()) { pairs.push((da.to_vec(), db.to_vec())) }
            pairs.push((a.to_vec(), b.to_vec()));
            let (sa, sb): (Vec<String>, Vec<String>) = (a.iter().map(|w| singular(w)).collect(), b.iter().map(|w| singular(w)).collect());
            if (sa.as_slice(), sb.as_slice()) != (a, b) && sa != sb { pairs.push((sa, sb)) }
        }
    };
    push(&n.subject, &t.subject);
    if n.conds.len() == t.conds.len() { for (a, b) in n.conds.iter().zip(&t.conds) { push(a, b) } }
    push(&n.order, &t.order);
    pairs.sort_by_key(|p| std::cmp::Reverse(p.0.len()));
    pairs
}

fn substitute(stmt: &str, pairs: &[(Vec<String>, Vec<String>)]) -> String {
    let toks = tokens(stmt);
    let base = stmt.as_ptr() as usize;
    let (mut out, mut last) = (String::new(), 0usize);
    for t in toks {
        if abstract_token(t) != "I" { continue }
        let new = pairs.iter().fold(t.to_string(), |id, (a, b)| replace_words(&id, a, b));
        if new != t {
            let a = t.as_ptr() as usize - base;
            out.push_str(&stmt[last..a]);
            out.push_str(&new);
            last = a + t.len();
        }
    }
    out.push_str(&stmt[last..]);
    out
}

// ---- the open project -------------------------------------------------------------------------------------------------------

struct Project {
    corpus: Corpus,
    parsed: Vec<Option<Parsed>>,
    by_key: HashMap<Key, Vec<usize>>,
    by_loose: HashMap<(Action, &'static str, usize), Vec<usize>>,
    vocab: HashSet<String>,
}

impl Project {
    fn build(root: &std::path::Path, held_ids: &HashMap<std::path::PathBuf, usize>) -> Project {
        let mut p = Project { corpus: Corpus::empty(), parsed: Vec::new(), by_key: HashMap::new(), by_loose: HashMap::new(), vocab: HashSet::new() };
        let mut files = Vec::new();
        rust_files(root, &mut files);
        files.sort();
        let mut next_id = 0usize;
        for f in files {
            let src = std::fs::read_to_string(&f).unwrap_or_default();
            for t in tokens(&src) { if t.len() >= 3 && abstract_token(t) == "I" && !p.vocab.contains(t) { p.vocab.insert(t.to_string()); } }
            next_id += 1;
            let fid = held_ids.get(&f).copied().unwrap_or(next_id);
            for b in bodies_of(&src, fid, 30) {
                let id = p.corpus.methods.len();
                let parsed = parse_name(&b.m.name);
                if let Some(pa) = &parsed {
                    p.by_key.entry(key_of(pa, &b.m)).or_default().push(id);
                    p.by_loose.entry((pa.action, result_form(&b.m.ret), b.m.params.len())).or_default().push(id);
                }
                p.parsed.push(parsed);
                p.corpus.add(b.m);
            }
        }
        p
    }

    fn vouched(&self, stmts: &[String]) -> bool {
        stmts.iter().all(|s| tokens(s).iter().all(|t| t.len() < 3 || abstract_token(t) != "I" || self.vocab.contains(*t)))
    }
}

fn jaccard(a: &Method, b: &Method) -> f32 {
    let (fa, fb) = (features(a), features(b));
    let inter: f32 = fa.iter().map(|(f, w)| fb.iter().find(|(g, _)| g == f).map_or(0.0, |(_, v)| w.min(*v))).sum();
    let (sa, sb): (f32, f32) = (fa.iter().map(|f| f.1).sum(), fb.iter().map(|f| f.1).sum());
    inter / (sa + sb - inter).max(1e-6)
}

#[derive(Default, Clone)]
struct Cell {
    n: usize,
    produced: usize,
    exact: usize,
    exact5: usize,
    form: usize,
    stmt_ok: usize,
    stmt_total: usize,
    compiled: usize,
    compiles: usize,
    compiles5: usize,
}

impl Cell {
    fn merge(&mut self, o: &Cell) {
        self.n += o.n; self.produced += o.produced; self.exact += o.exact; self.exact5 += o.exact5; self.form += o.form;
        self.stmt_ok += o.stmt_ok; self.stmt_total += o.stmt_total; self.compiled += o.compiled; self.compiles += o.compiles; self.compiles5 += o.compiles5;
    }
}

fn shapes_of(stmts: &[String]) -> Vec<String> {
    stmts.iter().map(|s| shape(s)).collect()
}

struct Example { action: Action, name: String, want: String, got: String, exact: bool, rejected: bool, size: usize }

fn render(stmts: &[String]) -> String {
    stmts.join(" ")
}

#[test]
#[ignore = "needs the public corpus and a project index: see the module docs"]
fn m9_names_read_in_a_grammar() {
    let wanted = env_num("BENNU_MLB_N", 800);
    let compile_n = env_num("BENNU_MLB_NC", 100);
    let svc = IndexService::global();
    let variants = ["B  nearest by name+signature (M4 style)", "K  neighbour under the key", "G  key + words of the name"];
    let mut cells: BTreeMap<(usize, Action), Cell> = BTreeMap::new();
    // coverage of the grammar over every method of the held-out files
    let mut cover: BTreeMap<(String, Action), [usize; 3]> = BTreeMap::new();
    let mut project_cover: BTreeMap<String, [usize; 4]> = BTreeMap::new();
    let (mut levels, mut examples): ([usize; 3], Vec<Example>) = ([0; 3], Vec::new());
    let (mut next_stats, mut body_stats) = ([0usize; 3], [0usize; 3]);
    let (mut gen_second, mut gen_second_total) = (0usize, 0usize);
    let mut vocabulary_total = 0usize;
    for spec in PROJECTS {
        let (root, held) = assemble(spec);
        for h in &held {
            for (name, ret, params, fields, has_body, _nstmts) in signatures(&h.text) {
                let _ = ret;
                let c = project_cover.entry(spec.name.to_string()).or_default();
                c[0] += 1;
                if let Some(p) = parse_name(&name) {
                    let resolved = p.resolved(&params, &fields);
                    let e = cover.entry((spec.name.to_string(), p.action)).or_default();
                    e[0] += 1;
                    if p.conds.is_empty() { e[1] += 1; c[2] += 1 }
                    if resolved { e[2] += 1; c[1] += 1 }
                    if resolved && has_body { c[3] += 1 }
                }
            }
        }
        let held_ids: HashMap<std::path::PathBuf, usize> = held.iter().enumerate().map(|(i, h)| (h.path.clone(), 1_000_000 + i)).collect();
        let project = Project::build(&root, &held_ids);
        let mut next_ix = crate::flc_next::NextIndex::default();
        let mut files = Vec::new();
        rust_files(&root, &mut files);
        files.sort();
        for f in &files { next_ix.add_file(&f.to_string_lossy(), &String::from_utf8_lossy(&std::fs::read(f).unwrap_or_default())) }
        let mut queries = Vec::new();
        for (hi, h) in held.iter().enumerate() {
            for b in bodies_of(&h.text, 1_000_000 + hi, 30) { queries.push((hi, b)) }
        }
        let stride = (queries.len() / wanted.max(1)).max(1);
        let mut compile_budget = compile_n;
        if compile_n > 0 { open_and_wait(&root) }
        for (hi, b) in queries.iter().step_by(stride).take(wanted) {
            let q = &b.m;
            let h = &held[*hi];
            let Some(parsed) = parse_name(&q.name) else { continue };
            let fields = class_fields(&h.text, q.start);
            if !parsed.resolved(&q.params, &fields) { continue }
            vocabulary_total += 1;
            let truth_n: Vec<String> = q.statements.iter().map(|s| normalize(s)).collect();
            let truth_shapes = shapes_of(&q.statements);
            // the candidates of each variant
            let nearest: Vec<usize> = project.corpus.top(q, 40).into_iter().filter(|&i| project.corpus.methods[i].params.len() == q.params.len()).take(5).collect();
            let key = key_of(&parsed, q);
            let alive = |v: Vec<usize>| -> Vec<usize> { v.into_iter().filter(|&i| { let m = &project.corpus.methods[i]; !(m.file == q.file && m.start == q.start) }).collect() };
            let mut under = alive(project.by_key.get(&key).cloned().unwrap_or_default());
            let mut level = 0;
            if under.is_empty() {
                under = alive(project.by_loose.get(&(parsed.action, result_form(&q.ret), q.params.len())).cloned().unwrap_or_default());
                level = 1;
            }
            if under.is_empty() { level = 2 }
            levels[level] += 1;
            under.sort_by(|&a, &c| jaccard(q, &project.corpus.methods[c]).total_cmp(&jaccard(q, &project.corpus.methods[a])).then(a.cmp(&c)));
            under.truncate(5);
            let candidates: [Vec<Vec<String>>; 3] = [
                nearest.iter().map(|&i| adapt(&project.corpus.methods[i], q, &fields)).collect(),
                under.iter().map(|&i| adapt(&project.corpus.methods[i], q, &fields)).filter(|s| project.vouched(s)).collect(),
                under.iter().filter_map(|&i| {
                    let n = &project.corpus.methods[i];
                    let np = project.parsed[i].as_ref()?;
                    let pairs = word_pairs(np, &parsed);
                    let tmpl = Method { statements: n.statements.iter().map(|s| substitute(s, &pairs)).collect(), ..n.clone() };
                    let s = adapt(&tmpl, q, &fields);
                    project.vouched(&s).then_some(s)
                }).collect(),
            ];
            // compile check on a subsample
            let file = h.path.to_string_lossy().into_owned();
            let compile_this = compile_budget > 0 && vocabulary_total % (wanted / compile_n.max(1)).max(1) == 0;
            let original_errors = if compile_this { Some(body_errors(svc, &file, &h.text, q.body.0, q.body.1)) } else { None };
            let line_start = h.text[..q.start].rfind('\n').map_or(0, |p| p + 1);
            let outer: String = h.text[line_start..].chars().take_while(|c| *c == ' ' || *c == '\t').collect();
            let inner = format!("{outer}    ");
            if compile_this { compile_budget -= 1 }
            let mut g_rejected = false;
            for (v, cands) in candidates.iter().enumerate() {
                let c = cells.entry((v, parsed.action)).or_default();
                c.n += 1;
                let Some(first) = cands.first() else { continue };
                c.produced += 1;
                let exact = first.iter().map(|s| normalize(s)).collect::<Vec<_>>() == truth_n;
                c.exact += usize::from(exact);
                c.exact5 += usize::from(cands.iter().any(|s| s.iter().map(|x| normalize(x)).collect::<Vec<_>>() == truth_n));
                c.form += usize::from(shapes_of(first) == truth_shapes);
                c.stmt_total += truth_n.len();
                c.stmt_ok += first.iter().zip(&truth_n).filter(|(a, b)| &normalize(a) == *b).count();
                if v == 2 {
                    if truth_n.len() >= 2 { gen_second_total += 1; gen_second += usize::from(first.get(1).is_some_and(|s| normalize(s) == truth_n[1])) }
                }
                if let Some(orig) = &original_errors {
                    let fine = |stmts: &Vec<String>| {
                        let body = body_text(stmts, &inner, &outer);
                        let candidate = format!("{}{}{}", &h.text[..q.body.0], body, &h.text[q.body.1..]);
                        body_errors(svc, &file, &candidate, q.body.0, q.body.0 + body.len()).is_subset(orig)
                    };
                    c.compiled += 1;
                    let first_fine = fine(first);
                    c.compiles += usize::from(first_fine);
                    c.compiles5 += usize::from(first_fine || cands.iter().skip(1).any(|s| fine(s)));
                    if v == 2 { g_rejected = !first_fine }
                }
            }
            // flc_next on the second statement, flc_body on accessors, on the same methods
            if q.statements.len() >= 2 {
                let indent_start = h.text[..b.spans[0].0].rfind('\n').map_or(0, |p| p + 1);
                let indent = &h.text[indent_start..b.spans[0].0];
                let prefix = format!("{}\n{indent}", &h.text[..b.spans[0].1]);
                let buffer = format!("{prefix}\n{}", &h.text[q.body.1 - 1..]);
                next_stats[0] += 1;
                if let Some(n) = next_ix.suggest_at(&file, &buffer, prefix.len(), 0.7) {
                    next_stats[1] += 1;
                    next_stats[2] += usize::from(normalize(&n.text) == truth_n[1]);
                }
            }
            if matches!(class_of(q), "getter" | "setter") {
                let indent_start = h.text[..b.spans[0].0].rfind('\n').map_or(0, |p| p + 1);
                let indent = &h.text[indent_start..b.spans[0].0];
                let prefix = format!("{}{{\n{indent}", &h.text[..q.body.0]);
                let buffer = format!("{prefix}\n{}", &h.text[q.body.1 - 1..]);
                body_stats[0] += 1;
                if let Some(bd) = crate::flc_body::suggest(&buffer, prefix.len()) {
                    body_stats[1] += 1;
                    body_stats[2] += usize::from(normalize(&bd.text) == truth_n.join(" "));
                }
            }
            if let Some(first) = candidates[2].first() {
                let exact = first.iter().map(|s| normalize(s)).collect::<Vec<_>>() == truth_n;
                examples.push(Example { action: parsed.action, name: format!("{}({})", q.name, q.params.iter().map(|p| format!("{} {}", p.0, p.1)).collect::<Vec<_>>().join(", ")), want: render(&q.statements), got: render(first), exact, rejected: g_rejected, size: render(&q.statements).len() + render(first).len() });
            }
        }
        if compile_n > 0 { svc.forget(&root.to_string_lossy()) }
    }

    // the typical application: repository, service and controller layers of the Spring projects at hand
    let mut spring: Vec<(String, [usize; 5])> = Vec::new();
    for name in ["spring-petclinic", "spring-petclinic-rest", "spring-petclinic-microservices", "spring-data-examples", "spring-boot"] {
        let root = super::m3_rank::assemble_training(name);
        let mut files = Vec::new();
        rust_files(&root, &mut files);
        let (mut all, mut layered) = ([0usize; 5], [0usize; 5]);
        for f in files {
            let layer = f.file_stem().map(|s| s.to_string_lossy().into_owned()).is_some_and(|n| ["Repository", "Service", "Controller", "Resource", "ServiceImpl"].iter().any(|k| n.ends_with(k)));
            for (n, _ret, params, fields, has_body, _) in signatures(&std::fs::read_to_string(&f).unwrap_or_default()) {
                for t in if layer { vec![&mut all, &mut layered] } else { vec![&mut all] } {
                    t[0] += 1;
                    if let Some(p) = parse_name(&n) {
                        if p.resolved(&params, &fields) { t[1] += 1 }
                        if !p.conds.is_empty() && p.resolved(&params, &fields) { t[2] += 1 }
                        if !has_body { t[3] += 1; if p.resolved(&params, &fields) { t[4] += 1 } }
                    } else if !has_body { t[3] += 1 }
                }
            }
        }
        spring.push((name.to_string(), all));
        spring.push((format!("{name} (repository, service, controller)"), layered));
    }
    let mut out = String::from("## M9 — names read in a grammar, bodies composed from the open project\n\n### 1. How much of the code the grammar reads (every method of the held-out files)\n\n| project | methods | name read (action known, conditions resolve) | of those, action only (no condition) | of the read, with a body | action only, all |\n|---|---|---|---|---|---|\n");
    for (p, c) in &project_cover {
        out.push_str(&format!("| {p} | {} | {:.1}% | {:.1}% | {:.1}% | {:.1}% |\n", c[0], pct(c[1], c[0]), pct(c[2].min(c[1]), c[0]), pct(c[3], c[0]), pct(c[2], c[0])));
    }
    out.push_str("\nBy action (all three projects): methods starting with the action / name read / action only\n\n| action | methods | read | no condition |\n|---|---|---|---|\n");
    let mut by_action: BTreeMap<Action, [usize; 3]> = BTreeMap::new();
    for ((_, a), v) in &cover { let e = by_action.entry(*a).or_default(); for i in 0..3 { e[i] += v[i] } }
    let total_methods: usize = project_cover.values().map(|c| c[0]).sum();
    for (a, v) in &by_action { out.push_str(&format!("| {} | {} | {:.1}% | {:.1}% |\n", a.label(), v[0], pct(v[2], v[0]), pct(v[1], v[0]))) }
    let read_all: usize = project_cover.values().map(|c| c[1]).sum();
    out.push_str("\nSpring projects at hand (all methods, bodiless included): methods / read / read with conditions / without a body / without a body and read\n\n| corpus | methods | read | read with a `By…` condition | without a body | without a body, read |\n|---|---|---|---|---|---|\n");
    for (n, c) in &spring { out.push_str(&format!("| {n} | {} | {:.1}% | {:.1}% | {:.1}% | {:.1}% |\n", c[0], pct(c[1], c[0]), pct(c[2], c[0]), pct(c[3], c[0]), pct(c[4], c[3]))) }
    out.push_str(&format!("\nIn all: {read_all} of {total_methods} methods ({:.1}%) are read by the grammar.\n", pct(read_all, total_methods)));
    out.push_str(&format!("\nThe key found its neighbours at: exact key {} / same action, shape and arity {} / none {}.\n", levels[0], levels[1], levels[2]));
    out.push_str("\n### 2. Bodies, per variant and action (methods read, sampled)\n\nexact = the whole body, statement by statement, as written; form = the same shapes; statements = share of the true statements that are exactly right at their place; compiles = no error beyond those the original body already had.\n\n");
    for (v, name) in variants.iter().enumerate() {
        out.push_str(&format!("**{name}**\n\n| action | methods | produces a body | exact | exact in one of 5 | form right | statements right | compiles (of those checked) | compiles, one of 5 |\n|---|---|---|---|---|---|---|---|---|\n"));
        let mut total = Cell::default();
        for ((vv, a), c) in &cells {
            if *vv != v { continue }
            total.merge(c);
            out.push_str(&format!("| {} | {} | {:.1}% | {:.1}% | {:.1}% | {:.1}% | {:.1}% | {:.1}% ({}) | {:.1}% |\n", a.label(), c.n, pct(c.produced, c.n), pct(c.exact, c.n), pct(c.exact5, c.n), pct(c.form, c.n), pct(c.stmt_ok, c.stmt_total), pct(c.compiles, c.compiled), c.compiled, pct(c.compiles5, c.compiled)));
        }
        out.push_str(&format!("| **all** | {} | {:.1}% | {:.1}% | {:.1}% | {:.1}% | {:.1}% | {:.1}% ({}) | {:.1}% |\n\n", total.n, pct(total.produced, total.n), pct(total.exact, total.n), pct(total.exact5, total.n), pct(total.form, total.n), pct(total.stmt_ok, total.stmt_total), pct(total.compiles, total.compiled), total.compiled, pct(total.compiles5, total.compiled)));
    }
    out.push_str(&format!("### 3. Against what exists\n\n- `flc_next` on the second statement of the same methods (threshold 0.7): speaks {} times of {} ({:.1}%), exact {:.1}% of those.\n- the generated body (G), second statement: exact {:.1}% of {} methods with two statements or more.\n- `flc_body` on the getters and setters among them: speaks {} of {}, exact {:.1}% of those.\n", next_stats[1], next_stats[0], pct(next_stats[1], next_stats[0]), pct(next_stats[2], next_stats[1]), pct(gen_second, gen_second_total), gen_second_total, body_stats[1], body_stats[0], pct(body_stats[2], body_stats[1])));
    out.push_str(&format!("\nMethods read in the sample: {vocabulary_total}.\n"));
    examples.sort_by_key(|e| e.size);
    out.push_str("\n### 5. Examples (variant G)\n\n");
    let pick = |f: &dyn Fn(&Example) -> bool, n: usize| examples.iter().filter(|e| f(e) && !e.got.is_empty() && e.want.len() > 30).take(n).collect::<Vec<_>>();
    for (title, list) in [("right", pick(&|e| e.exact && e.action != Action::Set, 2)), ("wrong", pick(&|e| !e.exact, 2)), ("rejected by the compile check", pick(&|e| e.rejected, 1))] {
        for e in list { out.push_str(&format!("**{title}** — `{}`\n\n```java\n// wanted\n{}\n// generated\n{}\n```\n\n", e.name, e.want, e.got)) }
    }
    let path = write_result("m9_report.md", &out);
    eprintln!("{out}\nreport: {}", path.display());
}

/// (name, return type, parameters, fields of the class, has a body, statements) of every method declaration in `src`.
pub(super) fn signatures(src: &str) -> Vec<(String, String, Vec<(String, String)>, Vec<(String, String)>, bool, usize)> {
    let Some(tree) = parse_java(src) else { return Vec::new() };
    let text = |n: &Node| src[n.start_byte()..n.end_byte()].to_string();
    let mut out = Vec::new();
    let mut stack = vec![tree.root_node()];
    while let Some(n) = stack.pop() {
        let mut c = n.walk();
        let kids: Vec<Node> = n.children(&mut c).collect();
        if n.kind() == "method_declaration" {
            if let Some(name) = n.child_by_field_name("name") {
                let mut params = Vec::new();
                if let Some(list) = n.child_by_field_name("parameters") {
                    let mut pc = list.walk();
                    for p in list.children(&mut pc).filter(|p| p.kind() == "formal_parameter") {
                        if let (Some(t), Some(nm)) = (p.child_by_field_name("type"), p.child_by_field_name("name")) { params.push((text(&t), text(&nm))) }
                    }
                }
                let body = n.child_by_field_name("body");
                let nstmts = body.map_or(0, |b| { let mut bc = b.walk(); b.children(&mut bc).filter(|s| s.is_named() && !s.kind().ends_with("comment")).count() });
                out.push((text(&name), n.child_by_field_name("type").map(|t| text(&t)).unwrap_or_default(), params, class_fields(src, n.start_byte()), body.is_some(), nstmts));
            }
        }
        stack.extend(kids);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_spring_data_style_name_is_read_into_its_parts() {
        let p = parse_name("findFirstByLastNameAndAgeGreaterThanOrderByAgeDesc").unwrap();
        assert_eq!(p.action, Action::Find);
        assert_eq!(p.modifiers, vec!["first"]);
        assert_eq!(p.conds, vec![vec!["last".to_string(), "name".to_string()], vec!["age".to_string()]]);
        assert_eq!(p.order, vec!["age"]);
        assert_eq!(parse_name("countByStatus").unwrap().action, Action::Count);
        assert_eq!(parse_name("existsByEmailIsNull").unwrap().conds, vec![vec!["email".to_string()]]);
    }

    #[test]
    fn a_name_without_a_known_action_is_not_read() {
        assert!(parse_name("frobnicate").is_none());
        assert!(parse_name("").is_none());
    }

    #[test]
    fn conditions_resolve_against_parameters_and_fields() {
        let p = parse_name("findByLastNameAndAge").unwrap();
        let params = vec![("String".to_string(), "lastName".to_string())];
        let fields = vec![("int".to_string(), "age".to_string())];
        assert!(p.resolved(&params, &fields));
        assert!(!p.resolved(&params, &[]));
    }

    #[test]
    fn the_result_is_told_from_the_signature() {
        assert_eq!(result_form("void"), "void");
        assert_eq!(result_form("Optional<User>"), "optional");
        assert_eq!(result_form("List<User>"), "list");
        assert_eq!(result_form("User[]"), "list");
        assert_eq!(result_form("long"), "number");
        assert_eq!(result_form("User"), "object");
    }

    #[test]
    fn words_are_replaced_inside_identifiers_in_their_style() {
        let w = |s: &str| s.split(' ').map(str::to_string).collect::<Vec<_>>();
        assert_eq!(replace_words("findByCodiceCliente", &w("cliente"), &w("fornitore")), "findByCodiceFornitore");
        assert_eq!(replace_words("clienteRepository", &w("cliente"), &w("fornitore")), "fornitoreRepository");
        assert_eq!(replace_words("ClienteNotFoundException", &w("cliente"), &w("fornitore")), "FornitoreNotFoundException");
        assert_eq!(replace_words("user", &w("cliente"), &w("fornitore")), "user");
        assert_eq!(replace_words("CLIENTE_ID", &w("cliente"), &w("fornitore")), "FORNITORE_ID");
    }

    #[test]
    fn the_neighbours_words_become_ours_in_the_body() {
        let n = parse_name("findByCodiceCliente").unwrap();
        let t = parse_name("findByCodiceFornitore").unwrap();
        let pairs = word_pairs(&n, &t);
        assert_eq!(substitute("return clienteRepository.findByCodiceCliente(codiceCliente);", &pairs), "return fornitoreRepository.findByCodiceFornitore(codiceFornitore);");
    }
}
