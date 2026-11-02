//! The **next statement** of a method, from the structure already written — on the empty line inside a body.
//!
//! A method of the open project that begins like this one (same shapes of its first statements, identifiers → `I` and literals →
//! `L`) and has a signature like this one is the best guide to what comes next: its following statement, with the names changed,
//! is offered. From the third statement on, the most frequent successor of the last three shapes (backing off to two and one)
//! decides which shape comes next, and the best neighbour with that shape writes it. Measured on public held-out code
//! (`mlbench::m7_prefix`, `mlbench::m7_next`): the first statement and the signature carry nearly all of the gain; a longer
//! opening adds little.
//!
//! Nothing is learned from outside the open project — the same shape repeats inside a project far more than across projects
//! (`mlbench::m8_portions`). The index is in memory only: it holds source, like `flc_similar`, and is never written to disk.
//!
//! What is offered is the neighbour's statement with its names changed: its i-th parameter becomes ours, its i-th local (among
//! the statements already written) becomes our i-th, the property its name derives becomes ours, any other free name is chosen
//! among our parameters, locals and fields by the argument ranker (`flc_rank`) when it shares a word with it. A statement that still
//! reads a variable we do not have is dropped: it would not compile. How sure it is: the share of the neighbours (or of the sightings
//! of the context) that agree on the shape; below `MIN_SHARE` nothing is offered.
//!
//! Wiring (the caller owns it): `add_file`/`remove_file` from the same scans as `flc_similar`, `clear` where the others are cleared,
//! `method_count` where they report their sizes, `suggest` right after `flc_similar::suggest`.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, OnceLock};

use bennu_java::prelude::parse_java;
use tree_sitter::Node;

use crate::flc_body::fields_of;
use crate::flc_rank::{self, Row};

const MAX_METHODS: usize = 30_000;
const MAX_STATEMENTS: usize = 30;
const MAX_STATEMENT_BYTES: usize = 400;
const MAX_FILE_BYTES: usize = 400_000;
/// The neighbours that vote on the shape of the next statement.
const NEIGHBOURS: usize = 10;
const MIN_NEIGHBOURS: usize = 3;
/// A context seen fewer times than this does not decide.
const MIN_SIGHTINGS: u32 = 3;
/// From this many statements written, the last shapes decide the shape and the neighbours only write it.
const LONG_PREFIX: usize = 3;
/// Below this share of agreement nothing is offered.
pub(crate) const MIN_SHARE: f32 = 0.7;

const KEYWORDS: &[&str] = &[
    "abstract", "assert", "boolean", "break", "byte", "case", "catch", "char", "class", "continue", "default", "do", "double", "else", "enum", "extends", "final", "finally", "float", "for", "if", "implements", "import", "instanceof", "int", "interface", "long", "new", "null", "package", "private", "protected", "public", "return", "short", "static", "super", "switch", "synchronized", "this", "throw", "throws", "try", "void", "volatile", "while", "true", "false", "var",
];

// ---- tokens -----------------------------------------------------------------------------------------------------

/// Identifiers, numbers, string/char literals (one token each) and single punctuation characters, with their byte spans.
fn scan(line: &str) -> Vec<(usize, usize)> {
    let b = line.as_bytes();
    let (mut out, mut i) = (Vec::new(), 0);
    while i < b.len() {
        let c = b[i];
        let s = i;
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        } else if c.is_ascii_alphabetic() || c == b'_' || c == b'$' {
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'$') { i += 1 }
        } else if c.is_ascii_digit() {
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'.' || b[i] == b'_') { i += 1 }
        } else if c == b'"' || c == b'\'' {
            i += 1;
            while i < b.len() && b[i] != c { i += if b[i] == b'\\' { 2 } else { 1 } }
            i = (i + 1).min(b.len());
        } else {
            // a non-ascii byte run is one token; ascii punctuation is one character
            i += 1;
            while i < b.len() && !b[i].is_ascii() { i += 1 }
        }
        out.push((s, i.min(b.len())));
    }
    out
}

fn tokens(line: &str) -> Vec<&str> {
    scan(line).into_iter().map(|(a, b)| &line[a..b]).collect()
}

fn is_name(t: &str) -> bool {
    t.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$') && !KEYWORDS.contains(&t)
}

fn abstract_token(t: &str) -> &str {
    let c = t.as_bytes()[0];
    if c == b'"' || c == b'\'' || c.is_ascii_digit() { "L" } else if is_name(t) { "I" } else { t }
}

/// The shape of a statement as one number; never `BOS`.
fn shape_of(stmt: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for t in tokens(stmt) { abstract_token(t).hash(&mut h) }
    h.finish().max(1)
}

fn hash_of(text: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

fn subwords(name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in name.chars() {
        if ch == '_' || !ch.is_alphanumeric() {
            if !cur.is_empty() { out.push(std::mem::take(&mut cur).to_lowercase()) }
        } else if ch.is_uppercase() && cur.chars().last().is_some_and(|l| l.is_lowercase()) {
            out.push(std::mem::take(&mut cur).to_lowercase());
            cur.push(ch);
        } else {
            cur.push(ch);
        }
    }
    if !cur.is_empty() { out.push(cur.to_lowercase()) }
    out
}

fn simple_type(t: &str) -> &str {
    t.split('<').next().unwrap_or(t).trim_end_matches("[]").trim_end_matches("...").rsplit('.').next().unwrap_or("").trim()
}

/// The names a statement declares (`Foo x = …`, `for (Foo x : …)`, `catch (E e)`) and the parameters of a lambda in it.
fn declared_names(stmt: &str) -> Vec<String> {
    let t = tokens(stmt);
    let mut out: Vec<String> = Vec::new();
    let mut push = |n: &str| if !out.iter().any(|o| o == n) { out.push(n.to_string()) };
    for w in t.windows(3) {
        let typed = is_name(w[0]) || ["int", "long", "boolean", "double", "float", "char", "byte", "short", ">", "]"].contains(&w[0]);
        if typed && is_name(w[1]) && matches!(w[2], "=" | ";" | ":" | "," | ")") && w[0] != "return" && w[0] != "new" { push(w[1]) }
    }
    for (i, x) in t.iter().enumerate() {
        if *x != "-" || t.get(i + 1) != Some(&">") || i == 0 { continue }
        if is_name(t[i - 1]) {
            push(t[i - 1]);
        } else if t[i - 1] == ")" {
            let mut j = i - 1;
            while j > 0 && t[j] != "(" {
                j -= 1;
                if is_name(t[j]) { push(t[j]) }
            }
        }
    }
    out
}

/// Where a name in `stmt` is a variable that has to exist: lower-case, not a member after a dot (unless `this.`), not called.
fn free_variable(toks: &[&str], i: usize) -> bool {
    let t = toks[i];
    let after_dot = i >= 1 && toks[i - 1] == "." && !(i >= 2 && toks[i - 2] == "this");
    abstract_token(t) == "I" && t.starts_with(|c: char| c.is_lowercase() || c == '_') && !after_dot && toks.get(i + 1) != Some(&"(")
}

fn rewrite(stmt: &str, map: &dyn Fn(&str) -> Option<String>) -> String {
    let spans = scan(stmt);
    let toks: Vec<&str> = spans.iter().map(|&(a, b)| &stmt[a..b]).collect();
    let (mut out, mut last) = (String::new(), 0usize);
    for (i, &(a, b)) in spans.iter().enumerate() {
        if free_variable(&toks, i) {
            if let Some(new) = map(toks[i]) {
                out.push_str(&stmt[last..a]);
                out.push_str(&new);
                last = b;
            }
        }
    }
    out.push_str(&stmt[last..]);
    out
}

fn property_of(method: &str) -> Option<String> {
    let rest = ["get", "is", "has", "set"].iter().find_map(|p| method.strip_prefix(p))?;
    let mut c = rest.chars();
    let first = c.next().filter(|f| f.is_uppercase())?;
    Some(format!("{}{}", first.to_lowercase(), c.as_str()))
}

// ---- the index --------------------------------------------------------------------------------------------------

type Key = (u8, [u64; 3]);
const BOS: u64 = 0;

/// Every (context, next shape) pair of a sequence of shapes, for the orders 1..=3, padded with `BOS` at the start.
fn pairs(seq: &[u64]) -> Vec<(Key, u64)> {
    let mut out = Vec::new();
    for p in 0..seq.len() {
        for order in 1..=3usize {
            out.push((context(&seq[..p], order), seq[p]));
        }
    }
    out
}

fn context(prefix: &[u64], order: usize) -> Key {
    let mut key = [BOS; 3];
    for j in 0..order {
        let at = prefix.len() as isize - (order - j) as isize;
        key[3 - order + j] = if at < 0 { BOS } else { prefix[at as usize] };
    }
    (order as u8, key)
}

struct Rec {
    file: String,
    name: String,
    params: Vec<(String, String)>,
    statements: Vec<String>,
    shapes: Vec<u64>,
    feats: Vec<(String, f32)>,
    size: f32,
}

fn features(name: &str, ret: &str, params: &[(String, String)]) -> Vec<(String, f32)> {
    let mut f: Vec<(String, f32)> = subwords(name).into_iter().map(|w| (format!("n:{w}"), 2.0)).collect();
    f.push((format!("t:{}", simple_type(ret)), 1.0));
    f.extend(params.iter().map(|(t, _)| (format!("t:{}", simple_type(t)), 1.0)));
    f.push((format!("a:{}", params.len()), 1.0));
    f
}

#[derive(Default)]
pub(crate) struct NextIndex {
    recs: HashMap<u64, Rec>,
    next_id: u64,
    by_file: HashMap<String, Vec<u64>>,
    seen: HashMap<String, u64>,
    inverted: HashMap<String, Vec<(u64, f32)>>,
    first_of: HashMap<u64, Vec<u64>>,
    table: HashMap<Key, HashMap<u64, u32>>,
    vocab: HashMap<String, u32>,
    ids_by_file: HashMap<String, Vec<String>>,
    stale: usize,
}

/// What the suggestion is: the statement, written for the caret, and how sure it is.
pub(crate) struct Next {
    pub text: String,
    pub confidence: f32,
}

fn text_of<'s>(src: &'s str, n: &Node) -> &'s str {
    &src[n.start_byte()..n.end_byte()]
}

fn statements_of<'t>(body: Node<'t>) -> Vec<Node<'t>> {
    let mut c = body.walk();
    body.children(&mut c).filter(|s| s.is_named() && !s.kind().ends_with("comment")).collect()
}

fn signature_of(method: Node, src: &str) -> Option<(String, String, Vec<(String, String)>)> {
    let name = text_of(src, &method.child_by_field_name("name")?).to_string();
    let ret = method.child_by_field_name("type").map(|t| text_of(src, &t).to_string()).unwrap_or_default();
    let mut params = Vec::new();
    if let Some(list) = method.child_by_field_name("parameters") {
        let mut pc = list.walk();
        for p in list.children(&mut pc).filter(|p| p.kind() == "formal_parameter") {
            if let (Some(t), Some(n)) = (p.child_by_field_name("type"), p.child_by_field_name("name")) {
                params.push((text_of(src, &t).to_string(), text_of(src, &n).to_string()));
            }
        }
    }
    Some((name, ret, params))
}

fn line_indent(src: &str, at: usize) -> &str {
    let start = src[..at].rfind('\n').map_or(0, |p| p + 1);
    let end = src[start..].find(|c: char| c != ' ' && c != '\t').map_or(src.len(), |p| start + p);
    &src[start..end]
}

/// The statement as text, its continuation lines relative to the indentation of the line it starts on.
fn statement_text(src: &str, n: &Node) -> String {
    let indent = line_indent(src, n.start_byte());
    let raw = text_of(src, n);
    let mut lines = raw.split('\n');
    let mut out = lines.next().unwrap_or("").to_string();
    for l in lines {
        out.push('\n');
        out.push_str(l.strip_prefix(indent).unwrap_or(l.trim_start()));
    }
    out
}

impl NextIndex {
    pub(crate) fn add_file(&mut self, key: &str, source: &str) {
        if source.len() > MAX_FILE_BYTES {
            self.remove_file(key);
            return;
        }
        let h = hash_of(source);
        if self.seen.get(key) == Some(&h) {
            return;
        }
        self.remove_file(key);
        self.seen.insert(key.to_string(), h);
        let Some(tree) = parse_java(source) else { return };
        let mut ids: Vec<String> = tokens(source).into_iter().filter(|t| is_name(t) && t.len() >= 3).map(str::to_string).collect();
        ids.sort();
        ids.dedup();
        for id in &ids { *self.vocab.entry(id.clone()).or_insert(0) += 1 }
        self.ids_by_file.insert(key.to_string(), ids);
        let mut mine = Vec::new();
        let mut stack = vec![tree.root_node()];
        while let Some(node) = stack.pop() {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                stack.push(child);
                if child.kind() != "method_declaration" || self.recs.len() >= MAX_METHODS { continue }
                let Some(body) = child.child_by_field_name("body") else { continue };
                let stmts = statements_of(body);
                if !(2..=MAX_STATEMENTS).contains(&stmts.len()) || stmts.iter().any(|s| s.end_byte() - s.start_byte() > MAX_STATEMENT_BYTES) { continue }
                let Some((name, ret, params)) = signature_of(child, source) else { continue };
                let statements: Vec<String> = stmts.iter().map(|s| statement_text(source, s)).collect();
                let shapes: Vec<u64> = statements.iter().map(|s| shape_of(s)).collect();
                let feats = features(&name, &ret, &params);
                let id = self.next_id;
                self.next_id += 1;
                for (f, w) in &feats { self.inverted.entry(f.clone()).or_default().push((id, *w)) }
                self.first_of.entry(shapes[0]).or_default().push(id);
                for (k, next) in pairs(&shapes) { *self.table.entry(k).or_default().entry(next).or_default() += 1 }
                self.recs.insert(id, Rec { file: key.to_string(), size: feats.iter().map(|f| f.1).sum(), name, params, statements, shapes, feats });
                mine.push(id);
            }
        }
        self.by_file.insert(key.to_string(), mine);
    }

    pub(crate) fn remove_file(&mut self, key: &str) {
        self.seen.remove(key);
        for id in self.by_file.remove(key).unwrap_or_default() {
            if let Some(rec) = self.recs.remove(&id) {
                for (k, next) in pairs(&rec.shapes) {
                    if let Some(m) = self.table.get_mut(&k) {
                        if let Some(n) = m.get_mut(&next) { *n = n.saturating_sub(1) }
                        m.retain(|_, n| *n > 0);
                        if m.is_empty() { self.table.remove(&k); }
                    }
                }
                self.stale += 1;
            }
        }
        for id in self.ids_by_file.remove(key).unwrap_or_default() {
            if let Some(n) = self.vocab.get_mut(&id) {
                *n -= 1;
                if *n == 0 { self.vocab.remove(&id); }
            }
        }
        if self.stale > self.recs.len().max(200) { self.compact() }
    }

    /// The lists of ids keep ids of removed methods until this: it drops them.
    fn compact(&mut self) {
        self.inverted.clear();
        self.first_of.clear();
        for (&id, rec) in &self.recs {
            for (f, w) in &rec.feats { self.inverted.entry(f.clone()).or_default().push((id, *w)) }
            self.first_of.entry(rec.shapes[0]).or_default().push(id);
        }
        self.stale = 0;
    }

    pub(crate) fn clear(&mut self) {
        *self = NextIndex::default();
    }

    pub(crate) fn method_count(&self) -> usize {
        self.recs.len()
    }

    pub(crate) fn suggest(&self, file: &str, source: &str, offset: usize) -> Option<Next> {
        self.suggest_at(file, source, offset, MIN_SHARE)
    }

    /// `suggest` with the threshold of agreement given, which is how the bench draws the curve of precision against it.
    pub(crate) fn suggest_at(&self, file: &str, source: &str, offset: usize, min_share: f32) -> Option<Next> {
        if self.recs.is_empty() || source.len() > MAX_FILE_BYTES || !source.is_char_boundary(offset) { return None }
        let line_start = source[..offset].rfind('\n').map_or(0, |p| p + 1);
        let present = &source[line_start..offset];
        let line_end = source[offset..].find('\n').map_or(source.len(), |p| offset + p);
        if !present.trim().is_empty() || !source[offset..line_end].trim().is_empty() { return None }
        let tree = parse_java(source)?;
        let at = source[..line_start].trim_end().len().checked_sub(1)?;
        let mut node = tree.root_node().descendant_for_byte_range(at, at)?;
        let body = loop {
            if node.kind() == "block" && node.parent().is_some_and(|p| p.kind() == "method_declaration") {
                break node;
            }
            node = node.parent()?;
        };
        if body.has_error() || body.start_byte() >= line_start || body.end_byte() <= offset { return None }
        let stmts = statements_of(body);
        // everything already written ends above the caret; nothing is written below it
        if stmts.iter().any(|s| s.end_byte() > line_start && s.start_byte() < offset || s.start_byte() >= offset) { return None }
        let before: Vec<Node> = stmts.into_iter().filter(|s| s.end_byte() <= line_start).collect();
        let k = before.len();
        if k == 0 || k >= MAX_STATEMENTS { return None }
        let method = body.parent()?;
        let (name, ret, params) = signature_of(method, source)?;
        let class_body = method.parent().filter(|p| p.kind() == "class_body" || p.kind() == "enum_body");
        let written: Vec<String> = before.iter().map(|s| statement_text(source, s)).collect();
        let qshapes: Vec<u64> = written.iter().map(|s| shape_of(s)).collect();
        let types: Vec<&str> = params.iter().map(|p| p.0.as_str()).collect();

        // the record of this very method, as it was last indexed, teaches nothing about itself
        let own: HashSet<u64> = self.by_file.get(file).into_iter().flatten().copied()
            .filter(|id| self.recs.get(id).is_some_and(|r| r.name == name && r.params.iter().map(|p| p.0.as_str()).eq(types.iter().copied())))
            .collect();
        let mut own_pairs: HashMap<(Key, u64), u32> = HashMap::new();
        for id in &own { for p in pairs(&self.recs[id].shapes) { *own_pairs.entry(p).or_default() += 1 } }

        let qf = features(&name, &ret, &params);
        let qsize: f32 = qf.iter().map(|f| f.1).sum();
        let mut inter: HashMap<u64, f32> = HashMap::new();
        for (f, w) in &qf {
            for &(id, cw) in self.inverted.get(f).map(Vec::as_slice).unwrap_or(&[]) { *inter.entry(id).or_default() += w.min(cw) }
        }
        let mut ids: Vec<u64> = inter.keys().copied().collect();
        ids.extend(self.first_of.get(&qshapes[0]).into_iter().flatten().copied().filter(|i| !inter.contains_key(i)));
        let mut ranked: Vec<(u64, usize, f32)> = ids
            .into_iter()
            .filter(|id| !own.contains(id))
            .filter_map(|id| {
                let r = self.recs.get(&id)?;
                (r.shapes.len() > k).then(|| (id, r.shapes.iter().zip(&qshapes).take_while(|(a, b)| a == b).count(), inter.get(&id).map_or(0.0, |x| x / (qsize + r.size - x))))
            })
            .collect();
        let cmp = |a: &(u64, usize, f32), b: &(u64, usize, f32)| b.1.cmp(&a.1).then(b.2.total_cmp(&a.2)).then(a.0.cmp(&b.0));
        if ranked.len() > 300 {
            ranked.select_nth_unstable_by(299, cmp);
            ranked.truncate(300);
        }
        ranked.sort_by(cmp);

        // which shape comes next, and how many agree
        let voted = || {
            let top = &ranked[..ranked.len().min(NEIGHBOURS)];
            if top.len() < MIN_NEIGHBOURS { return None }
            let mut votes: Vec<(u64, usize)> = Vec::new();
            for (id, ..) in top {
                let s = self.recs[id].shapes[k];
                match votes.iter_mut().find(|v| v.0 == s) { Some(v) => v.1 += 1, None => votes.push((s, 1)) }
            }
            let best = votes.iter().max_by(|a, b| a.1.cmp(&b.1).then(std::cmp::Ordering::Greater))?; // ties: the first seen, i.e. the better neighbour
            Some((best.0, best.1 as f32 / top.len() as f32))
        };
        let from_table = || {
            for order in [3usize, 2, 1] {
                let key = context(&qshapes, order);
                let Some(m) = self.table.get(&key) else { continue };
                let mut v: Vec<(u64, u32)> = m.iter().map(|(&s, &n)| (s, n.saturating_sub(own_pairs.get(&(key, s)).copied().unwrap_or(0)))).filter(|x| x.1 > 0).collect();
                let total: u32 = v.iter().map(|x| x.1).sum();
                if total < MIN_SIGHTINGS { continue }
                v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
                return Some((v[0].0, v[0].1 as f32 / total as f32));
            }
            None
        };
        let (shape, share) = if k >= LONG_PREFIX { from_table().or_else(voted)? } else { voted()? };
        if share < min_share { return None }

        let template = ranked.iter().map(|r| &self.recs[&r.0]).find(|r| r.shapes[k] == shape)?;
        let tfields: Vec<(String, String)> = class_body.map(|b| fields_of(source, &b)).unwrap_or_default().into_iter().filter(|f| !f.is_static).map(|f| (f.ty, f.name)).collect();
        let target = Target { params: &params, locals: written.iter().flat_map(|s| declared_names(s)).collect(), fields: &tfields, property: property_of(&name) };
        let statement = adapt(template, k, &target)?;
        if !self.vouched(&statement, &target) { return None }

        let indent = line_indent(source, before[0].start_byte());
        let lead = indent.strip_prefix(present).unwrap_or("");
        let tabs = indent.contains('\t');
        let restyled: Vec<String> = statement.split('\n').enumerate().map(|(i, l)| {
            if i == 0 { return l.to_string() }
            // the neighbour's file may indent with the other character
            let body = l.trim_start_matches([' ', '\t']);
            let ws = &l[..l.len() - body.len()];
            format!("{indent}{}{body}", if tabs { ws.replace("    ", "\t") } else { ws.replace('\t', "    ") })
        }).collect();
        let text = format!("{lead}{}", restyled.join("\n"));
        Some(Next { text, confidence: share })
    }

    /// Every name the statement reads as a variable exists for the target, and every other name is somewhere in the project.
    fn vouched(&self, statement: &str, target: &Target) -> bool {
        let own = declared_names(statement);
        let toks = tokens(statement);
        (0..toks.len()).all(|i| {
            let t = toks[i];
            if abstract_token(t) != "I" { return true }
            if free_variable(&toks, i) {
                own.iter().any(|o| o == t) || target.params.iter().any(|p| p.1 == t) || target.locals.iter().any(|l| l == t) || target.fields.iter().any(|f| f.1 == t)
            } else {
                t.len() < 3 || self.vocab.contains_key(t)
            }
        })
    }
}

struct Target<'a> {
    params: &'a [(String, String)],
    locals: Vec<String>,
    fields: &'a [(String, String)],
    property: Option<String>,
}

/// The template's statement `k` written for the target.
fn adapt(template: &Rec, k: usize, target: &Target) -> Option<String> {
    let stmt = &template.statements[k];
    let template_locals: Vec<String> = template.statements[..k].iter().flat_map(|s| declared_names(s)).collect();
    let own = declared_names(stmt);
    let tproperty = property_of(&template.name);
    let offers: Vec<(&str, &str, &str)> = target.params.iter().map(|(t, n)| (n.as_str(), t.as_str(), "local")).chain(target.locals.iter().map(|n| (n.as_str(), "", "local"))).chain(target.fields.iter().map(|(t, n)| (n.as_str(), t.as_str(), "field"))).collect();
    let map = |name: &str| -> Option<String> {
        if own.iter().any(|o| o == name) { return None }
        if let Some(i) = template_locals.iter().position(|l| l == name) {
            return target.locals.get(i).cloned().filter(|n| n != name);
        }
        if let Some(i) = template.params.iter().position(|p| p.1 == name) {
            return target.params.get(i).map(|p| p.1.clone()).filter(|n| n != name);
        }
        if tproperty.as_deref() == Some(name) {
            if let Some(p) = &target.property { return Some(p.clone()).filter(|p| p != name) }
        }
        if offers.iter().any(|o| o.0 == name) || offers.is_empty() { return None }
        let rows: Vec<Row> = offers.iter().enumerate().map(|(rank, (n, t, k))| Row { name: n, kind: k, detail: t, deprecated: false, recency: 0, rank }).collect();
        let best = flc_rank::order("", name, &rows, 0)[0];
        let n = offers[best].0;
        let shared = subwords(n).iter().any(|w| subwords(name).contains(w));
        (shared && n != name).then(|| n.to_string())
    };
    let out = rewrite(stmt, &map);
    (!out.trim().is_empty()).then_some(out)
}

// ---- the process-wide index -------------------------------------------------------------------------------------

fn global() -> &'static Mutex<NextIndex> {
    static INDEX: OnceLock<Mutex<NextIndex>> = OnceLock::new();
    INDEX.get_or_init(|| Mutex::new(NextIndex::default()))
}

pub(crate) fn add_file(key: &str, source: &str) {
    global().lock().unwrap_or_else(|p| p.into_inner()).add_file(key, source);
}

pub(crate) fn remove_file(key: &str) {
    global().lock().unwrap_or_else(|p| p.into_inner()).remove_file(key);
}

pub(crate) fn clear() {
    global().lock().unwrap_or_else(|p| p.into_inner()).clear();
}

pub(crate) fn method_count() -> usize {
    global().lock().unwrap_or_else(|p| p.into_inner()).method_count()
}

/// The next statement for the empty line at `offset`, or `None`; see the module doc.
pub(crate) fn suggest(file: &str, source: &str, offset: usize) -> Option<Next> {
    global().lock().unwrap_or_else(|p| p.into_inner()).suggest(file, source, offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service(class: &str, ty: &str, repo: &str) -> String {
        format!("public class {class} {{\n    private {repo} repo;\n\n    public {ty} find(String id) {{\n        {ty} f = repo.get(id);\n        if (f == null) {{ throw new NotFound(id); }}\n        return f;\n    }}\n\n    public {ty} load(String key) {{\n        {ty} f = repo.get(key);\n        if (f == null) {{ throw new NotFound(key); }}\n        return f;\n    }}\n}}\n")
    }

    fn index() -> NextIndex {
        let mut ix = NextIndex::default();
        ix.add_file("A.java", &service("AService", "Alpha", "AlphaRepo"));
        ix.add_file("B.java", &service("BService", "Beta", "BetaRepo"));
        ix.add_file("C.java", &service("CService", "Gamma", "GammaRepo"));
        ix
    }

    /// The caret goes where the `§` stands, on an otherwise empty line.
    fn target(body: &str) -> (String, usize) {
        let src = format!("public class DService {{\n    private DeltaRepo repo;\n\n    public Delta fetch(String code) {{\n{body}\n    }}\n}}\n");
        let offset = src.find('§').unwrap();
        (src.replacen('§', "", 1), offset)
    }

    fn drawn(ix: &NextIndex, body: &str) -> Option<String> {
        let (src, offset) = target(body);
        ix.suggest("D.java", &src, offset).map(|n| n.text)
    }

    #[test]
    fn the_next_statement_of_the_neighbours_is_written_with_our_names() {
        let got = drawn(&index(), "        Delta d = repo.get(code);\n        §");
        assert_eq!(got.as_deref(), Some("if (d == null) { throw new NotFound(code); }"));
    }

    #[test]
    fn the_confidence_is_the_share_of_neighbours_that_agree() {
        let (src, offset) = target("        Delta d = repo.get(code);\n        §");
        let n = index().suggest("D.java", &src, offset).unwrap();
        assert!(n.confidence >= MIN_SHARE && n.confidence <= 1.0);
    }

    #[test]
    fn the_answer_is_the_same_every_time() {
        let ix = index();
        let first = drawn(&ix, "        Delta d = repo.get(code);\n        §");
        for _ in 0..5 { assert_eq!(drawn(&ix, "        Delta d = repo.get(code);\n        §"), first) }
    }

    #[test]
    fn one_method_alone_does_not_make_a_habit() {
        let mut ix = NextIndex::default();
        ix.add_file("A.java", &service("AService", "Alpha", "AlphaRepo").replace("    public Alpha load(String key) {", "    private void other() {"));
        assert_eq!(drawn(&ix, "        Delta d = repo.get(code);\n        §"), None);
    }

    #[test]
    fn no_neighbour_no_answer() {
        assert_eq!(drawn(&NextIndex::default(), "        Delta d = repo.get(code);\n        §"), None);
        // a first statement nobody begins with
        assert_eq!(drawn(&index(), "        while (running) { tick(); }\n        §"), None);
    }

    #[test]
    fn it_speaks_only_on_an_empty_line_after_a_complete_statement() {
        let ix = index();
        assert_eq!(drawn(&ix, "        §"), None, "no statement written yet");
        assert_eq!(drawn(&ix, "        Delta d = repo.get(code);\n        if (§d == null) {}"), None, "caret in the middle of a line");
        assert_eq!(drawn(&ix, "        Delta d = repo.get(code);\n        §\n        return d;"), None, "statements below the caret");
        assert_eq!(drawn(&ix, "        Delta d = repo.get(\n        §"), None, "unfinished statement");
    }

    #[test]
    fn a_removed_file_is_forgotten() {
        let mut ix = index();
        assert!(drawn(&ix, "        Delta d = repo.get(code);\n        §").is_some());
        ix.remove_file("A.java");
        ix.remove_file("B.java");
        assert_eq!(ix.method_count(), 2);
        assert_eq!(drawn(&ix, "        Delta d = repo.get(code);\n        §"), None);
        ix.remove_file("C.java");
        assert_eq!(ix.method_count(), 0);
        assert!(ix.table.is_empty() && ix.vocab.is_empty());
    }

    #[test]
    fn re_adding_a_file_replaces_what_it_held() {
        let mut ix = index();
        let n = ix.method_count();
        ix.add_file("A.java", &service("AService", "Alpha", "AlphaRepo"));
        assert_eq!(ix.method_count(), n);
        ix.add_file("A.java", "class A {}");
        assert_eq!(ix.method_count(), n - 2);
    }

    #[test]
    fn a_statement_that_reads_a_variable_we_do_not_have_is_dropped() {
        let mut ix = NextIndex::default();
        let with_field = |c: &str, t: &str| format!("class {c} {{\n    int limit;\n    {t} run(String id) {{\n        {t} x = make(id);\n        check(limit, x);\n    }}\n    {t} go(String id) {{\n        {t} x = make(id);\n        check(limit, x);\n    }}\n}}\n");
        for (c, t) in [("P", "Pa"), ("Q", "Qa"), ("R", "Ra")] { ix.add_file(&format!("{c}.java"), &with_field(c, t)) }
        let src = "class S {\n    Sa run(String id) {\n        Sa x = make(id);\n        \n    }\n}\n";
        let offset = src.find("        \n").unwrap() + 8;
        assert_eq!(ix.suggest("S.java", src, offset).map(|n| n.text), None, "`limit` is not a field of S");
        let src = "class S {\n    int limit;\n    Sa run(String id) {\n        Sa x = make(id);\n        \n    }\n}\n";
        let offset = src.find("        \n").unwrap() + 8;
        assert_eq!(ix.suggest("S.java", src, offset).map(|n| n.text).as_deref(), Some("check(limit, x);"));
    }

    #[test]
    fn utf8_before_the_caret_and_in_the_index_is_safe() {
        let mut ix = index();
        ix.add_file("U.java", "public class Ü {\n    String é(String ñ) {\n        String ü = \"è€\";\n        return ü;\n    }\n}\n");
        let (src, offset) = target("        String nome = \"à€ü\";\n        §");
        let _ = ix.suggest("D.java", &src, offset);
        // an offset inside a multi-byte char is refused, not a panic
        let bad = src.find('€').unwrap() + 1;
        assert!(ix.suggest("D.java", &src, bad).is_none());
    }

    #[test]
    fn the_continuation_lines_follow_the_indentation_of_the_target() {
        let multi = |c: &str, t: &str| format!("class {c} {{\n    {t} run(String id) {{\n        {t} x = make(id);\n        return x\n            .with(id)\n            .done();\n    }}\n}}\n");
        let mut ix = NextIndex::default();
        for (c, t) in [("P", "Pa"), ("Q", "Qa"), ("R", "Ra")] { ix.add_file(&format!("{c}.java"), &multi(c, t)) }
        let src = "class S {\n\tSa run(String id) {\n\t\tSa y = make(id);\n\t\t\n\t}\n}\n";
        let offset = src.find("\t\t\n").unwrap() + 2;
        assert_eq!(ix.suggest("S.java", src, offset).map(|n| n.text).as_deref(), Some("return y\n\t\t\t.with(id)\n\t\t\t.done();"));
    }
}
