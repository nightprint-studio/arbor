//! The body of a **sibling method**, with the names changed — for the method whose signature was just written.
//!
//! CRUD-heavy code is full of twins: `findByCodiceCliente(String codiceCliente)` in one service and, when the
//! `Fornitore` side is written, `findByCodiceFornitore(String codiceFornitore)` in the next. The body of the second is
//! the body of the first with one word swapped everywhere it appears — in identifiers (`clienteRepository`,
//! `ClienteNotFoundException`), in the parameter, in the return type. No model is needed for that, and no guess is
//! made: the difference between the two signatures *is* the mapping.
//!
//! What is kept: for every method with a short body, its signature and its body text, **in memory only** (it is
//! source, like `bennu_flc::freq::LineIndex`, and is never written to disk). What is offered: the body of the sibling
//! whose signature is closest, with the words that differ between the two names replaced in the body, indented for
//! where it goes. How sure it is: how much of the two names is shared, whether the parameters and types show the same
//! change as the names do, and whether every identifier the substitution produced exists somewhere in the project —
//! a `findByClienteAndStato` that becomes `findByFornitoreAndStato` is a method nobody wrote.
//!
//! Wiring (the caller owns it): feed `add_file`/`remove_file` from the same scan that counts the log statements and
//! call `suggest` with the buffer and the caret before the model, after the project's repeated lines. Two caret
//! shapes are answered: right after the `{` of a signature (with or without an auto-closed `}` after the caret), and
//! on the empty line below it.
//!
//! This module is not wired yet; the allow keeps the build quiet until it is.


use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, OnceLock};

/// More methods than this are not indexed: the cost is memory and the longer scan at the query.
const MAX_METHODS: usize = 20_000;
/// A body of one to this many lines is worth copying. Longer ones carry too much of their own domain.
const MAX_BODY_LINES: usize = 25;
/// Files larger than this are not parsed per change.
const MAX_FILE_BYTES: usize = 400_000;
/// Below this the suggestion is not made at all.
const MIN_CONFIDENCE: f32 = 0.6;
/// What a substitution that cannot be fully vouched for is worth.
const UNVOUCHED: f32 = 0.6;
/// The signature before the caret is read back this far.
const MAX_SIGNATURE_BYTES: usize = 600;

// ---- words ------------------------------------------------------------------------------------------------------

fn ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

/// Byte spans of the words of an identifier: `findByCodice_Cliente` → `find`, `By`, `Codice`, `Cliente`.
fn word_spans(name: &str) -> Vec<(usize, usize)> {
    let chars: Vec<(usize, char)> = name.char_indices().collect();
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for (k, &(i, c)) in chars.iter().enumerate() {
        if !c.is_alphanumeric() {
            if let Some(s) = start.take() {
                out.push((s, i));
            }
            continue;
        }
        let boundary = c.is_uppercase()
            && k > 0
            && chars[k - 1].1.is_alphanumeric()
            && (chars[k - 1].1.is_lowercase() || chars.get(k + 1).is_some_and(|n| n.1.is_lowercase()));
        if boundary {
            if let Some(s) = start.take() {
                out.push((s, i));
            }
        }
        if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(s) = start {
        out.push((s, name.len()));
    }
    out
}

fn words(name: &str) -> Vec<String> {
    word_spans(name).into_iter().map(|(a, b)| name[a..b].to_lowercase()).collect()
}

/// `from` → `to` as word sequences.
type Phrase = (Vec<String>, Vec<String>);

/// Where two word lists differ, as phrases: the stretches between the words they share, in order. `None` when the
/// lists share nothing, are equal, or differ by an insertion or a deletion rather than a replacement.
fn diff_phrases(source: &[String], target: &[String]) -> Option<Vec<Phrase>> {
    // longest common subsequence, small lists
    let (n, m) = (source.len(), target.len());
    let mut lcs = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if source[i] == target[j] { lcs[i + 1][j + 1] + 1 } else { lcs[i + 1][j].max(lcs[i][j + 1]) };
        }
    }
    if lcs[0][0] == 0 {
        return None;
    }
    let (mut i, mut j) = (0, 0);
    let (mut from, mut to): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
    let mut out: Vec<Phrase> = Vec::new();
    let close = |from: &mut Vec<String>, to: &mut Vec<String>, out: &mut Vec<Phrase>| -> bool {
        if from.is_empty() && to.is_empty() {
            return true;
        }
        if from.is_empty() || to.is_empty() {
            return false;
        }
        out.push((std::mem::take(from), std::mem::take(to)));
        true
    };
    while i < n || j < m {
        if i < n && j < m && source[i] == target[j] {
            if !close(&mut from, &mut to, &mut out) {
                return None;
            }
            i += 1;
            j += 1;
        } else if j >= m || (i < n && lcs[i + 1][j] >= lcs[i][j + 1]) {
            from.push(source[i].clone());
            i += 1;
        } else {
            to.push(target[j].clone());
            j += 1;
        }
    }
    if !close(&mut from, &mut to, &mut out) || out.is_empty() || out.len() > 2 {
        return None;
    }
    Some(out)
}

/// The same phrase in its other number: `cliente` → `clienti`, `fornitore` → `fornitori`, `order` → `orders`, and
/// back — `clienti` also reaches the `cliente` of `clienteRepository`.
fn plural_phrases(phrases: &[Phrase]) -> Vec<Phrase> {
    let mut out = Vec::new();
    for (from, to) in phrases {
        let (Some(f), Some(t)) = (from.last(), to.last()) else { continue };
        let ends = |w: &str, c: char| w.ends_with(c);
        let mut forms: Vec<(String, String)> = vec![(format!("{f}s"), format!("{t}s"))];
        if f.len() > 3 && t.len() > 3 {
            if ends(f, 's') && ends(t, 's') {
                forms.push((f[..f.len() - 1].to_string(), t[..t.len() - 1].to_string()));
            }
            if ends(f, 'i') && ends(t, 'i') {
                for c in ['e', 'o'] {
                    forms.push((format!("{}{c}", &f[..f.len() - 1]), format!("{}{c}", &t[..t.len() - 1])));
                }
            }
            if ends(f, 'e') && ends(t, 'e') {
                forms.push((format!("{}a", &f[..f.len() - 1]), format!("{}a", &t[..t.len() - 1])));
            }
        }
        if ends(f, 'e') && ends(t, 'e') {
            forms.push((format!("{}i", &f[..f.len() - 1]), format!("{}i", &t[..t.len() - 1])));
        }
        if ends(f, 'o') && ends(t, 'o') {
            forms.push((format!("{}i", &f[..f.len() - 1]), format!("{}i", &t[..t.len() - 1])));
        }
        if ends(f, 'a') && ends(t, 'a') {
            forms.push((format!("{}e", &f[..f.len() - 1]), format!("{}e", &t[..t.len() - 1])));
        }
        for (pf, pt) in forms {
            let (mut nf, mut nt) = (from.clone(), to.clone());
            *nf.last_mut().unwrap() = pf;
            *nt.last_mut().unwrap() = pt;
            out.push((nf, nt));
        }
    }
    out
}

/// The phrase `to`, written in the style of the text it replaces: `UpperCamel`, `lowerCamel`, `snake_case` or
/// `UPPER_SNAKE`.
fn render(to: &[String], replaced: &str) -> String {
    let snake = replaced.contains('_');
    let upper = replaced.chars().any(|c| c.is_alphabetic()) && !replaced.chars().any(|c| c.is_lowercase()) && replaced.chars().count() > 1;
    let cap = |w: &str| {
        let mut cs = w.chars();
        cs.next().map(|f| f.to_uppercase().collect::<String>() + cs.as_str()).unwrap_or_default()
    };
    if upper {
        return to.iter().map(|w| w.to_uppercase()).collect::<Vec<_>>().join(if snake { "_" } else { "" });
    }
    if snake {
        return to.join("_");
    }
    let first_upper = replaced.chars().next().is_some_and(char::is_uppercase);
    to.iter().enumerate().map(|(k, w)| if k == 0 && !first_upper { w.clone() } else { cap(w) }).collect()
}

/// One identifier with the phrases replaced wherever its words match, leaving the rest of it as written.
fn map_phrases(token: &str, phrases: &[Phrase]) -> Option<String> {
    let spans = word_spans(token);
    let ws: Vec<String> = spans.iter().map(|&(a, b)| token[a..b].to_lowercase()).collect();
    let (mut out, mut last, mut i, mut changed) = (String::new(), 0usize, 0usize, false);
    while i < ws.len() {
        let best = phrases
            .iter()
            .filter(|(from, _)| i + from.len() <= ws.len() && ws[i..i + from.len()] == from[..])
            .max_by_key(|(from, _)| from.len());
        match best {
            Some((from, to)) => {
                let (a, b) = (spans[i].0, spans[i + from.len() - 1].1);
                out.push_str(&token[last..a]);
                out.push_str(&render(to, &token[a..b]));
                last = b;
                i += from.len();
                changed = true;
            }
            None => i += 1,
        }
    }
    out.push_str(&token[last..]);
    changed.then_some(out)
}

// ---- scanning source text ---------------------------------------------------------------------------------------

struct Tok {
    start: usize,
    end: usize,
    /// Inside a string, a char literal or a comment.
    literal: bool,
}

/// Identifier runs of `text`, with whether each sits in a string or comment.
fn scan(text: &str) -> Vec<Tok> {
    let b: Vec<(usize, char)> = text.char_indices().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let (pos, c) = b[i];
        let next = b.get(i + 1).map(|x| x.1);
        let literal_end = |from: usize, close: &dyn Fn(usize) -> bool, b: &[(usize, char)]| -> usize {
            let mut k = from;
            while k < b.len() && !close(k) {
                k += if b[k].1 == '\\' { 2 } else { 1 };
            }
            k
        };
        if c == '/' && next == Some('/') {
            let end = literal_end(i, &|k| b[k].1 == '\n', &b);
            literal_tokens(text, pos, b.get(end).map_or(text.len(), |x| x.0), &mut out);
            i = end;
        } else if c == '/' && next == Some('*') {
            let end = literal_end(i + 2, &|k| b[k].1 == '*' && b.get(k + 1).map(|x| x.1) == Some('/'), &b);
            let stop = b.get(end + 2).map_or(text.len(), |x| x.0);
            literal_tokens(text, pos, stop, &mut out);
            i = (end + 2).min(b.len());
        } else if c == '"' || c == '\'' {
            let end = literal_end(i + 1, &|k| b[k].1 == c, &b);
            let stop = b.get(end + 1).map_or(text.len(), |x| x.0);
            literal_tokens(text, pos, stop, &mut out);
            i = (end + 1).min(b.len());
        } else if ident_char(c) && !c.is_ascii_digit() {
            let mut k = i;
            while k < b.len() && ident_char(b[k].1) {
                k += 1;
            }
            out.push(Tok { start: pos, end: b.get(k).map_or(text.len(), |x| x.0), literal: false });
            i = k;
        } else if c.is_ascii_digit() {
            let mut k = i;
            while k < b.len() && (ident_char(b[k].1) || b[k].1 == '.') {
                k += 1;
            }
            i = k;
        } else {
            i += 1;
        }
    }
    out
}

fn literal_tokens(text: &str, from: usize, to: usize, out: &mut Vec<Tok>) {
    let mut start = None;
    for (off, c) in text[from..to].char_indices() {
        match (ident_char(c), start) {
            (true, None) => start = Some(from + off),
            (false, Some(s)) => {
                out.push(Tok { start: s, end: from + off, literal: true });
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        out.push(Tok { start: s, end: to, literal: true });
    }
}

/// Braces left open in `body`, outside strings and comments: the body of a method must come out at zero.
fn brace_balance(body: &str) -> i32 {
    let literal: Vec<(usize, usize)> = {
        let mut spans = Vec::new();
        let b = body.as_bytes();
        let mut i = 0;
        while i < b.len() {
            let start = i;
            if b[i] == b'"' || b[i] == b'\'' {
                let q = b[i];
                i += 1;
                while i < b.len() && b[i] != q {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
                spans.push((start, (i + 1).min(b.len())));
            } else if b[i] == b'/' && b.get(i + 1) == Some(&b'/') {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
                spans.push((start, i));
            } else if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
                i += 2;
                while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                    i += 1;
                }
                spans.push((start, (i + 2).min(b.len())));
            }
            i += 1;
        }
        spans
    };
    body.bytes()
        .enumerate()
        .filter(|(i, _)| !literal.iter().any(|&(a, b)| *i >= a && *i < b))
        .fold(0, |d, (_, c)| d + i32::from(c == b'{') - i32::from(c == b'}'))
}

fn hash_of(parts: &[&str]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for p in parts {
        p.hash(&mut h);
    }
    h.finish()
}

fn leading_ws(line: &str) -> &str {
    &line[..line.len() - line.trim_start().len()]
}

// ---- methods ----------------------------------------------------------------------------------------------------

/// A method as the index and the query see it.
#[derive(Clone)]
struct Method {
    name: String,
    words: Vec<String>,
    ret: String,
    params: Vec<(String, String)>,
    /// The body's lines with the body's own indentation taken off (blank lines are empty).
    body: Vec<String>,
    /// The step the body is indented by, relative to the line of the signature.
    unit: String,
}

struct Entry {
    method: Method,
    refs: u32,
    files: Vec<String>,
}

fn text_of<'a>(node: tree_sitter::Node, src: &'a str) -> &'a str {
    node.utf8_text(src.as_bytes()).unwrap_or("")
}

/// Name, return type and parameters of a `method_declaration` node.
fn signature_of(node: tree_sitter::Node, src: &str) -> Option<(String, String, Vec<(String, String)>)> {
    let name = text_of(node.child_by_field_name("name")?, src).to_string();
    let ret = text_of(node.child_by_field_name("type")?, src).split_whitespace().collect::<Vec<_>>().join(" ");
    let params_node = node.child_by_field_name("parameters")?;
    let mut params = Vec::new();
    let mut cursor = params_node.walk();
    for p in params_node.children(&mut cursor) {
        if !matches!(p.kind(), "formal_parameter" | "spread_parameter") {
            continue;
        }
        let ty = p.child_by_field_name("type").map(|t| text_of(t, src).to_string())?;
        let pname = match p.child_by_field_name("name") {
            Some(n) => text_of(n, src).to_string(),
            None => {
                let mut c = p.walk();
                let found = p.children(&mut c).filter(|k| k.kind() == "variable_declarator").last().map(|k| text_of(k, src).to_string());
                found?
            }
        };
        params.push((ty, pname));
    }
    Some((name, ret, params))
}

/// The line holding byte `at` in `src`, as (its start, its leading whitespace).
fn line_indent(src: &str, at: usize) -> &str {
    let start = src[..at.min(src.len())].rfind('\n').map_or(0, |p| p + 1);
    leading_ws(&src[start..])
}

/// A body worth copying: short, real code. `None` otherwise.
fn body_lines(inner: &str, method_indent: &str) -> Option<(Vec<String>, String)> {
    let mut lines: Vec<&str> = inner.lines().collect();
    while lines.first().is_some_and(|l| l.trim().is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    if lines.is_empty() || lines.len() > MAX_BODY_LINES {
        return None;
    }
    let lower = inner.to_lowercase();
    if lower.contains("todo") || lower.contains("unsupportedoperation") || inner.trim() == "return null;" {
        return None;
    }
    let base = lines.iter().filter(|l| !l.trim().is_empty()).map(|l| leading_ws(l)).min_by_key(|w| w.len())?.to_string();
    let rel: Vec<String> = lines.iter().map(|l| if l.trim().is_empty() { String::new() } else { l.strip_prefix(&base).unwrap_or(l.trim_start()).to_string() }).collect();
    let unit = base.strip_prefix(method_indent).filter(|u| !u.is_empty()).unwrap_or("    ").to_string();
    Some((rel, unit))
}

// ---- the index --------------------------------------------------------------------------------------------------

#[derive(Default)]
pub(crate) struct SimilarIndex {
    entries: HashMap<u64, Entry>,
    by_file: HashMap<String, Vec<u64>>,
    /// The hash of each file's text as last read: an unchanged file is not parsed again.
    seen: HashMap<String, u64>,
    /// Every identifier of the code and how many files hold it — what "this name exists in the project" means.
    vocab: HashMap<String, u32>,
    ids_by_file: HashMap<String, Vec<String>>,
}

/// What to draw and how sure the body is.
#[derive(Debug, PartialEq)]
pub(crate) struct SimilarBody {
    pub text: String,
    pub confidence: f32,
}

impl SimilarIndex {
    pub(crate) fn add_file(&mut self, key: &str, source: &str) {
        if source.len() > MAX_FILE_BYTES {
            self.remove_file(key);
            return;
        }
        let h = hash_of(&[source]);
        if self.seen.get(key) == Some(&h) {
            return;
        }
        self.remove_file(key);
        self.seen.insert(key.to_string(), h);
        let Some(tree) = bennu_java::prelude::parse_java(source) else { return };

        let mut ids: Vec<String> = scan(source).into_iter().filter(|t| !t.literal && t.end - t.start >= 3).map(|t| source[t.start..t.end].to_string()).collect();
        ids.sort();
        ids.dedup();
        for id in &ids {
            *self.vocab.entry(id.clone()).or_insert(0) += 1;
        }
        self.ids_by_file.insert(key.to_string(), ids);

        let mut mine = Vec::new();
        let mut stack = vec![tree.root_node()];
        while let Some(node) = stack.pop() {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() != "method_declaration" {
                    stack.push(child);
                    continue;
                }
                if let Some(entry) = self.method_of(child, source) {
                    if self.entries.len() >= MAX_METHODS && !self.entries.contains_key(&entry.0) {
                        continue;
                    }
                    let slot = self.entries.entry(entry.0).or_insert_with(|| Entry { method: entry.1, refs: 0, files: Vec::new() });
                    slot.refs += 1;
                    slot.files.push(key.to_string());
                    mine.push(entry.0);
                }
                // methods of an anonymous or inner class inside the body are found through the body
                stack.push(child);
            }
        }
        self.by_file.insert(key.to_string(), mine);
    }

    fn method_of(&self, node: tree_sitter::Node, src: &str) -> Option<(u64, Method)> {
        let body = node.child_by_field_name("body")?;
        let (name, ret, params) = signature_of(node, src)?;
        let name_node = node.child_by_field_name("name")?;
        let indent = line_indent(src, name_node.start_byte());
        let inner = src.get(body.start_byte() + 1..body.end_byte().saturating_sub(1))?;
        let (lines, unit) = body_lines(inner, indent)?;
        let types: Vec<&str> = params.iter().map(|p| p.0.as_str()).collect();
        let key = hash_of(&[&name, &ret, &types.join(","), &lines.join("\n")]);
        let words = words(&name);
        Some((key, Method { name, words, ret, params, body: lines, unit }))
    }

    pub(crate) fn remove_file(&mut self, key: &str) {
        self.seen.remove(key);
        for h in self.by_file.remove(key).unwrap_or_default() {
            let gone = self.entries.get_mut(&h).is_some_and(|e| {
                e.refs = e.refs.saturating_sub(1);
                if let Some(p) = e.files.iter().position(|f| f == key) {
                    e.files.remove(p);
                }
                e.refs == 0
            });
            if gone {
                self.entries.remove(&h);
            }
        }
        for id in self.ids_by_file.remove(key).unwrap_or_default() {
            if let Some(n) = self.vocab.get_mut(&id) {
                *n -= 1;
                if *n == 0 {
                    self.vocab.remove(&id);
                }
            }
        }
    }

    pub(crate) fn method_count(&self) -> usize {
        self.entries.len()
    }

    /// The sibling body for the signature just written before `offset` in `source`, or `None`.
    pub(crate) fn suggest(&self, file: &str, source: &str, offset: usize) -> Option<SimilarBody> {
        if source.len() > MAX_FILE_BYTES || self.entries.is_empty() {
            return None;
        }
        let target = target_at(source, offset)?;
        let t_words = words(&target.method.name);
        // the names the file being written already has count as existing: its fields, its other methods
        let local: HashSet<String> = scan(source).into_iter().filter(|t| !t.literal).map(|t| source[t.start..t.end].to_string()).collect();
        let mut found: Vec<(f32, bool, String)> = Vec::new(); // (confidence, same file, text)
        for entry in self.entries.values() {
            let s = &entry.method;
            if s.params.len() != target.method.params.len() || s.words.iter().all(|w| !t_words.contains(w)) {
                continue;
            }
            let same_file = entry.files.iter().any(|f| f == file);
            // the method being written is not its own sibling
            if same_file && s.name == target.method.name && s.params.iter().map(|p| &p.0).eq(target.method.params.iter().map(|p| &p.0)) {
                continue;
            }
            if let Some((text, confidence)) = self.transplant(s, &target, same_file, &local) {
                found.push((confidence, same_file, text));
            }
        }
        // the best text; a second sibling that gives the very same body is one more reason to believe it
        found.sort_by(|a, b| b.0.total_cmp(&a.0).then(b.1.cmp(&a.1)).then(a.2.cmp(&b.2)));
        let (confidence, _, text) = found.first().cloned()?;
        let agreeing = found.iter().filter(|f| f.2 == text).count().saturating_sub(1).min(2) as f32;
        let confidence = (confidence + 0.05 * agreeing).min(1.0);
        (confidence >= MIN_CONFIDENCE).then_some(SimilarBody { text, confidence })
    }

    /// `s`'s body rewritten for `t`, and how much to trust it.
    fn transplant(&self, s: &Method, t: &Target, same_file: bool, local: &HashSet<String>) -> Option<(String, f32)> {
        let t_method = &t.method;
        let t_words = words(&t_method.name);
        let phrases = diff_phrases(&s.words, &t_words)?;
        let all_phrases: Vec<Phrase> = phrases.iter().cloned().chain(plural_phrases(&phrases)).collect();

        // whole-identifier changes the signatures show: parameter names, and the types in the parameters and result
        let mut ids: HashMap<String, String> = HashMap::new();
        let mut conflict = false;
        for (sp, tp) in s.params.iter().zip(&t_method.params) {
            note(&sp.1, &tp.1, &mut ids, &mut conflict);
            let (a, b) = (idents(&sp.0), idents(&tp.0));
            if a.len() == b.len() {
                a.iter().zip(&b).for_each(|(x, y)| note(x, y, &mut ids, &mut conflict));
            } else if sp.0 != tp.0 {
                conflict = true;
            }
        }
        let (a, b) = (idents(&s.ret), idents(&t_method.ret));
        if a.len() == b.len() {
            a.iter().zip(&b).for_each(|(x, y)| note(x, y, &mut ids, &mut conflict));
        } else if s.ret != t_method.ret {
            conflict = true;
        }

        // is the change of name backed by the same change in the signature?
        let corroborated = ids.iter().any(|(from, to)| {
            let (fw, tw) = (words(from), words(to));
            phrases.iter().any(|(pf, pt)| contains_run(&fw, pf) && contains_run(&tw, pt))
        });

        let body = s.body.join("\n");
        let (new_body, changed) = rewrite(&body, &ids, &all_phrases);
        if new_body.is_empty() || brace_balance(&new_body) != 0 {
            return None;
        }

        // the names the substitution made must exist: here, in the signature, or twice in the body (declared and used)
        let own: HashSet<String> = t_method.params.iter().map(|p| p.1.clone()).chain(idents(&t_method.ret)).chain(t_method.params.iter().flat_map(|p| idents(&p.0))).collect();
        let tokens = scan(&new_body);
        let count_of = |w: &str| tokens.iter().filter(|k| &new_body[k.start..k.end] == w).count();
        let mut unvouched = conflict || changed.is_empty();
        for w in &changed {
            if !self.vocab.contains_key(w) && !own.contains(w) && !local.contains(w) && count_of(w) < 2 {
                unvouched = true;
            }
        }
        // a word of the sibling's domain that no phrase reached (`clientela` after cliente → fornitore)
        let leftovers = phrases.iter().flat_map(|(f, _)| f.iter()).any(|w| {
            w.len() >= 4 && tokens.iter().any(|k| !k.literal && new_body[k.start..k.end].to_lowercase().contains(w.as_str()))
        });
        unvouched |= leftovers;

        let common = s.words.iter().filter(|w| t_words.contains(w)).count() as f32;
        let overlap = common / s.words.len().max(t_words.len()) as f32;
        let base = (overlap + if corroborated { 0.25 } else { 0.0 } + if same_file { 0.1 } else { 0.0 }).min(1.0);
        let confidence = base * if unvouched { UNVOUCHED } else { 1.0 };

        // indentation: relative lines keep their shape on the target's base
        let (first_indent, rest_indent) = t.indents(&s.unit);
        let lines: Vec<&str> = new_body.lines().collect();
        let mut text = String::new();
        for (k, l) in lines.iter().enumerate() {
            let indent = if k == 0 { &first_indent } else { &rest_indent };
            if k > 0 {
                text.push('\n');
            }
            if !l.is_empty() {
                text.push_str(indent);
                text.push_str(l);
            }
        }
        Some((t.frame(text), confidence))
    }
}

/// Records that `from` is to be written `to`; two different answers for one name are a conflict.
fn note(from: &str, to: &str, ids: &mut HashMap<String, String>, conflict: &mut bool) {
    if from == to || from.is_empty() {
        return;
    }
    if ids.get(from).is_some_and(|prev| prev != to) {
        *conflict = true;
    }
    ids.insert(from.to_string(), to.to_string());
}

/// Identifiers of a type or declaration text, in order: `List<Cliente>` → `List`, `Cliente`.
fn idents(text: &str) -> Vec<String> {
    scan(text).into_iter().map(|t| text[t.start..t.end].to_string()).collect()
}

/// Two words that are one word in a different number or ending: `cliente`/`clienti`, `order`/`orders`.
fn same_stem(a: &str, b: &str) -> bool {
    let stem = |w: &str| if w.len() > 4 && w.ends_with(['s', 'i', 'e', 'o', 'a']) { w[..w.len() - 1].to_string() } else { w.to_string() };
    a == b || stem(a) == stem(b)
}

fn contains_run(haystack: &[String], run: &[String]) -> bool {
    !run.is_empty() && haystack.windows(run.len()).any(|w| w.iter().zip(run).all(|(x, y)| same_stem(x, y)))
}

/// `body` with the identifier changes applied; also the new spellings that came out of it.
fn rewrite(body: &str, ids: &HashMap<String, String>, phrases: &[Phrase]) -> (String, Vec<String>) {
    let (mut out, mut last, mut changed) = (String::new(), 0usize, Vec::new());
    for t in scan(body) {
        let tok = &body[t.start..t.end];
        let new = match ids.get(tok) {
            Some(n) if !t.literal => Some(n.clone()),
            _ => map_phrases(tok, phrases),
        };
        if let Some(n) = new {
            out.push_str(&body[last..t.start]);
            out.push_str(&n);
            last = t.end;
            if !t.literal {
                changed.push(n);
            }
        }
    }
    out.push_str(&body[last..]);
    changed.sort();
    changed.dedup();
    (out, changed)
}

// ---- the signature being written --------------------------------------------------------------------------------

/// Where the caret is, relative to the signature it follows.
enum Shape {
    /// Right after the `{`; `closed` when an auto-closed `}` follows the caret on the line.
    SameLine { closed: bool },
    /// On the empty line below the `{`; `indent` is what the line already holds before the caret.
    NextLine { indent: String },
}

struct Target {
    method: Method,
    method_indent: String,
    shape: Shape,
}

impl Target {
    /// The indentation of the first line and of the others, given the sibling's step.
    fn indents(&self, unit: &str) -> (String, String) {
        match &self.shape {
            Shape::SameLine { .. } => {
                let body = format!("{}{}", self.method_indent, unit);
                (body.clone(), body)
            }
            // the caret already stands after the line's indentation: the first line adds none, the others repeat it
            Shape::NextLine { indent } if !indent.is_empty() => (String::new(), indent.clone()),
            Shape::NextLine { .. } => {
                let body = format!("{}{}", self.method_indent, unit);
                (body.clone(), body)
            }
        }
    }

    /// The body, shaped for where the caret is.
    fn frame(&self, body: String) -> String {
        match &self.shape {
            Shape::SameLine { closed } => {
                let tail = if *closed { format!("\n{}", self.method_indent) } else { String::new() };
                format!("\n{body}{tail}")
            }
            Shape::NextLine { .. } => body,
        }
    }
}

/// The method whose signature ends just before the caret, if the caret is where a body starts.
fn target_at(source: &str, offset: usize) -> Option<Target> {
    if offset > source.len() || !source.is_char_boundary(offset) {
        return None;
    }
    let line_start = source[..offset].rfind('\n').map_or(0, |p| p + 1);
    let line_end = source[offset..].find('\n').map_or(source.len(), |p| offset + p);
    let (prefix, rest) = (&source[line_start..offset], source[offset..line_end].trim());
    let next_closes = |from: usize| source[from..].lines().find(|l| !l.trim().is_empty()).is_some_and(|l| l.trim_start().starts_with('}'));

    let (brace, shape) = if prefix.trim_end().ends_with('{') && !prefix.trim().is_empty() {
        let at = line_start + prefix.trim_end().len() - 1;
        match rest {
            "}" => (at, Shape::SameLine { closed: true }),
            "" if next_closes(line_end) => (at, Shape::SameLine { closed: false }),
            _ => return None,
        }
    } else if prefix.trim().is_empty() && rest.is_empty() {
        let before = source[..line_start].trim_end();
        if !before.ends_with('{') || !next_closes(line_end) {
            return None;
        }
        (before.len() - 1, Shape::NextLine { indent: prefix.to_string() })
    } else {
        return None;
    };

    // the signature runs back to the previous statement or block boundary
    let begin = source[..brace].rfind([';', '{', '}']).map_or(0, |p| p + 1);
    if brace - begin > MAX_SIGNATURE_BYTES {
        return None;
    }
    let sig = &source[begin..brace];
    let wrapped = format!("class _W {{\n{sig} {{}}\n}}");
    let tree = bennu_java::prelude::parse_java(&wrapped)?;
    let root = tree.root_node();
    if root.has_error() {
        return None;
    }
    let class_body = root.child(0)?.child_by_field_name("body")?;
    let mut cursor = class_body.walk();
    let method = class_body.children(&mut cursor).find(|n| n.kind() == "method_declaration")?;
    let (name, ret, params) = signature_of(method, &wrapped)?;
    let name_at = begin + method.child_by_field_name("name")?.start_byte().saturating_sub("class _W {\n".len());
    let method_indent = line_indent(source, name_at).to_string();
    let method = Method { words: words(&name), name, ret, params, body: Vec::new(), unit: String::new() };
    Some(Target { method, method_indent, shape })
}

// ---- the process-wide index -------------------------------------------------------------------------------------

fn global() -> &'static Mutex<SimilarIndex> {
    static INDEX: OnceLock<Mutex<SimilarIndex>> = OnceLock::new();
    INDEX.get_or_init(|| Mutex::new(SimilarIndex::default()))
}

/// Counts the methods of `source` under `key`, replacing what the same key held before.
pub(crate) fn add_file(key: &str, source: &str) {
    global().lock().unwrap_or_else(|p| p.into_inner()).add_file(key, source);
}

/// Takes back what `key` held (a deleted file).
pub(crate) fn remove_file(key: &str) {
    global().lock().unwrap_or_else(|p| p.into_inner()).remove_file(key);
}

/// The body of the sibling of the method whose signature ends at the caret; see the module doc for the shapes.
pub(crate) fn suggest(file: &str, source: &str, offset: usize) -> Option<SimilarBody> {
    global().lock().unwrap_or_else(|p| p.into_inner()).suggest(file, source, offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLIENTE_SERVICE: &str = "package app;\n\npublic class ClienteService {\n    private final ClienteRepository clienteRepository;\n\n    public Cliente findByCodiceCliente(String codiceCliente) {\n        return clienteRepository.findByCodiceCliente(codiceCliente)\n                .orElseThrow(() -> new ClienteNotFoundException(codiceCliente));\n    }\n\n    public List<Cliente> listClienti() {\n        return clienteRepository.findAll();\n    }\n}\n";
    const FORNITORE_REPO: &str = "package app;\n\npublic interface FornitoreRepository {\n    Optional<Fornitore> findByCodiceFornitore(String codiceFornitore);\n    List<Fornitore> findAll();\n}\n";
    const FORNITORE_EXCEPTION: &str = "package app;\n\npublic class FornitoreNotFoundException extends RuntimeException {\n    public FornitoreNotFoundException(String codice) { super(codice); }\n}\n";
    const FORNITORE_SERVICE_HEAD: &str = "package app;\n\npublic class FornitoreService {\n    private final FornitoreRepository fornitoreRepository;\n\n    public Fornitore findByCodiceFornitore(String codiceFornitore) {";

    fn index() -> SimilarIndex {
        let mut ix = SimilarIndex::default();
        ix.add_file("ClienteService.java", CLIENTE_SERVICE);
        ix.add_file("FornitoreRepository.java", FORNITORE_REPO);
        ix.add_file("FornitoreNotFoundException.java", FORNITORE_EXCEPTION);
        ix
    }

    /// The service being written, the caret after the `{` and an auto-closed `}`.
    fn same_line() -> (String, usize) {
        let src = format!("{FORNITORE_SERVICE_HEAD}}}\n}}\n");
        let at = FORNITORE_SERVICE_HEAD.len();
        (src, at)
    }

    /// The same, the caret on the empty line below, indented as the editor leaves it.
    fn next_line() -> (String, usize) {
        let src = format!("{FORNITORE_SERVICE_HEAD}\n        \n    }}\n}}\n");
        let at = FORNITORE_SERVICE_HEAD.len() + 1 + 8;
        (src, at)
    }

    const EXPECTED: [&str; 2] = ["return fornitoreRepository.findByCodiceFornitore(codiceFornitore)", "        .orElseThrow(() -> new FornitoreNotFoundException(codiceFornitore));"];

    #[test]
    fn the_body_of_the_twin_comes_with_the_names_changed() {
        let ix = index();
        let (src, at) = next_line();
        let got = ix.suggest("FornitoreService.java", &src, at).expect("a sibling");
        // the empty line is already indented: the first line goes right after it, the others line up under it
        assert_eq!(
            got.text,
            "return fornitoreRepository.findByCodiceFornitore(codiceFornitore)\n                .orElseThrow(() -> new FornitoreNotFoundException(codiceFornitore));"
        );
    }

    #[test]
    fn right_after_the_brace_the_body_starts_on_a_new_line_and_the_closer_gets_its_own() {
        let ix = index();
        let (src, at) = same_line();
        let got = ix.suggest("FornitoreService.java", &src, at).expect("a sibling");
        assert!(got.text.starts_with("\n        return fornitoreRepository.findByCodiceFornitore(codiceFornitore)\n"), "{:?}", got.text);
        assert!(got.text.ends_with("\n    "), "the auto-closed brace lands on its own line: {:?}", got.text);
        assert!(got.text.contains("new FornitoreNotFoundException(codiceFornitore)"));
        assert_eq!(brace_balance(&got.text), 0);
        assert!(got.confidence >= 0.9, "{}", got.confidence);
    }

    #[test]
    fn a_name_the_project_does_not_have_lowers_the_confidence() {
        let mut ix = SimilarIndex::default();
        ix.add_file("ClienteService.java", CLIENTE_SERVICE);
        // no FornitoreNotFoundException and no repository method anywhere
        let (src, at) = next_line();
        let got = ix.suggest("FornitoreService.java", &src, at).map(|g| g.confidence);
        assert!(got.is_none_or(|c| c <= 0.6 + f32::EPSILON), "{got:?}");
    }

    #[test]
    fn plural_forms_follow_the_singular() {
        let mut ix = SimilarIndex::default();
        ix.add_file("ClienteService.java", CLIENTE_SERVICE);
        ix.add_file("FornitoreRepository.java", FORNITORE_REPO);
        let src = "package app;\npublic class FornitoreService {\n    private final FornitoreRepository fornitoreRepository;\n    public List<Fornitore> listFornitori() {\n        \n    }\n}\n";
        let at = src.find("        \n").unwrap() + 8;
        let got = ix.suggest("FornitoreService.java", src, at).expect("a sibling");
        assert_eq!(got.text, "return fornitoreRepository.findAll();");
        assert!(got.confidence >= MIN_CONFIDENCE);
    }

    #[test]
    fn no_shared_word_or_a_different_arity_is_no_sibling() {
        let ix = index();
        let src = "package app;\npublic class X {\n    public Fornitore archiviaRegistro(String a, String b) {\n        \n    }\n}\n";
        let at = src.find("        \n").unwrap() + 8;
        assert!(ix.suggest("X.java", src, at).is_none());
        let src = "package app;\npublic class X {\n    public Fornitore findByCodiceFornitore(String a, String b) {\n        \n    }\n}\n";
        let at = src.find("        \n").unwrap() + 8;
        assert!(ix.suggest("X.java", src, at).is_none());
    }

    #[test]
    fn only_an_empty_body_is_completed() {
        let ix = index();
        let src = format!("{FORNITORE_SERVICE_HEAD}\n        \n        return null;\n    }}\n}}\n");
        let at = FORNITORE_SERVICE_HEAD.len() + 1 + 8;
        assert!(ix.suggest("FornitoreService.java", &src, at).is_none());
    }

    #[test]
    fn a_sibling_in_the_same_file_counts_and_a_removed_file_stops_counting() {
        let mut ix = SimilarIndex::default();
        let both = "package app;\npublic class S {\n    public Cliente byCliente(Cliente cliente) {\n        return repo.load(cliente);\n    }\n\n    public Fornitore byFornitore(Fornitore fornitore) {\n        \n    }\n}\n";
        ix.add_file("S.java", &both.replace("        \n    }\n}\n", "        return null;\n    }\n}\n"));
        assert_eq!(ix.method_count(), 1, "the stub `return null;` is not a sibling");
        let at = both.find("        \n").unwrap() + 8;
        let got = ix.suggest("S.java", both, at).expect("same-file sibling");
        assert_eq!(got.text, "return repo.load(fornitore);");
        ix.remove_file("S.java");
        assert_eq!(ix.method_count(), 0);
        assert!(ix.suggest("S.java", both, at).is_none());
    }

    #[test]
    fn identical_methods_in_two_files_are_kept_once_and_outlive_one_removal() {
        let mut ix = SimilarIndex::default();
        ix.add_file("A.java", CLIENTE_SERVICE);
        ix.add_file("B.java", CLIENTE_SERVICE);
        let n = ix.method_count();
        ix.remove_file("A.java");
        assert_eq!(ix.method_count(), n, "B still holds them");
        ix.remove_file("B.java");
        assert_eq!(ix.method_count(), 0);
    }

    #[test]
    fn the_style_of_the_replaced_text_is_kept() {
        let phrases = vec![(vec!["cliente".to_string()], vec!["fornitore".to_string()])];
        assert_eq!(map_phrases("getCliente", &phrases).as_deref(), Some("getFornitore"));
        assert_eq!(map_phrases("clienteRepository", &phrases).as_deref(), Some("fornitoreRepository"));
        assert_eq!(map_phrases("ID_CLIENTE", &phrases).as_deref(), Some("ID_FORNITORE"));
        assert_eq!(map_phrases("cliente_id", &phrases).as_deref(), Some("fornitore_id"));
        assert_eq!(map_phrases("getIDCliente", &phrases).as_deref(), Some("getIDFornitore"));
        assert_eq!(map_phrases("orders", &phrases), None);
    }

    #[test]
    fn names_differ_by_a_replacement_not_by_an_insertion() {
        let w = |s: &str| words(s);
        assert!(diff_phrases(&w("findByCliente"), &w("findByFornitore")).is_some());
        assert!(diff_phrases(&w("findByCliente"), &w("findByClienteAndStato")).is_none());
        assert!(diff_phrases(&w("findCliente"), &w("archiviaRegistro")).is_none());
    }

    #[test]
    fn the_global_index_wraps_the_same_calls() {
        add_file("/tmp/flc-similar-test/ClienteService.java", CLIENTE_SERVICE);
        let (src, at) = next_line();
        let got = suggest("/tmp/flc-similar-test/Other.java", &src, at);
        remove_file("/tmp/flc-similar-test/ClienteService.java");
        assert!(got.is_some_and(|g| g.text.contains("fornitoreRepository") && !g.text.contains("cliente")));
    }
}
