//! The project's log statements, as **counters per pattern**.
//!
//! A project writes its logs in a style — the same logger, the same entry line (`START - name({}, {})`), the same
//! way of reporting a caught exception — and that style is a fact the editor can use: when the caret starts a log
//! statement, the likeliest statement for *this* place is the pattern the project used most in places like it.
//!
//! ## What is kept
//!
//! Not the calls. Each call is reduced to a **pattern** — the level and a template in which the logger, the
//! method's name, its parameters and the caught exception are slots — and only a counter per pattern is kept, in a
//! bucket per [`Placement`] (the first statement of a method, inside a `catch`, anywhere else). Per file, the
//! `(placement, pattern)` hashes it contributed are remembered, so that a changed file replaces its contribution
//! instead of being added twice, and a deleted one takes it away. Memory is the number of distinct patterns plus
//! eight bytes per call, not the text of every call.
//!
//! ## What it suggests
//!
//! [`LogProfile::suggest`] fills the best pattern's slots from the caret's [`LogContext`] and returns the rest of
//! the statement after what was already typed. It answers only when one pattern clearly leads its bucket: a
//! project with no house style gets nothing, and the model (or nothing) answers instead.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

const LEVELS: &[&str] = &["trace", "debug", "info", "warn", "warning", "error", "fatal", "fine", "finer", "severe"];
const CONTROL: &[&str] = &["if", "for", "while", "switch", "catch", "else", "try", "do", "synchronized", "new", "return", "throw"];
/// A bucket never holds more distinct patterns than this; the rarest are dropped first.
const BUCKET_CAP: usize = 400;
/// A pattern leads when it holds at least this share of its bucket...
const MIN_SHARE: f32 = 0.5;
/// ...and was seen at least this often.
const MIN_COUNT: u32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Placement {
    /// The first statement of a method body.
    Entry,
    /// Inside a `catch` block.
    Catch,
    /// Anywhere else.
    Body,
}

/// What the place a log statement would go into says about its content.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LogContext {
    pub placement: Option<Placement>,
    pub method: String,
    pub params: Vec<String>,
    /// The variable a `catch` around the caret binds.
    pub exception: Option<String>,
    /// The logger the file declares (`LOG`, `logger`, ...), when it declares one.
    pub logger: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LogSuggestion {
    /// The rest of the statement after what was typed.
    pub text: String,
    /// The share of the bucket the pattern holds.
    pub confidence: f32,
    pub count: u32,
}

#[derive(Clone, Debug)]
struct Pattern {
    level: String,
    template: String,
    count: u32,
}

#[derive(Default, Debug)]
struct Bucket {
    patterns: HashMap<u64, Pattern>,
    total: u32,
}

/// A [`LogProfile`] as plain data, for a copy that outlives the process. Only templates (a level and a message with
/// slots) and logger names are in it, never a line of source.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LogSnapshot {
    /// `(pattern hash, level, template)`.
    pub patterns: Vec<(u64, String, String)>,
    /// `(file key, [(placement, pattern hash, logger)])`.
    pub files: Vec<(String, Vec<(Placement, u64, String)>)>,
}

#[derive(Default, Debug)]
pub struct LogProfile {
    buckets: HashMap<Option<Placement>, Bucket>,
    receivers: HashMap<String, u32>,
    /// Per file: the `(placement, pattern hash, receiver)` of each call it contributed.
    files: HashMap<String, Vec<(Placement, u64, String)>>,
}

fn hash_of(level: &str, template: &str) -> u64 {
    let mut h = DefaultHasher::new();
    level.hash(&mut h);
    template.hash(&mut h);
    h.finish()
}

fn ident_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'$'
}

// ---- reading a source file line by line ---------------------------------------------------------------------------

struct Method {
    name: String,
    params: Vec<String>,
    indent: usize,
}

struct Catch {
    var: String,
    indent: usize,
}

/// The state a file's lines leave behind: which method, which `catch`, how many statements into it.
#[derive(Default)]
struct Walker {
    method: Option<Method>,
    catch: Option<Catch>,
    statements: u32,
}

/// A call found on a line.
struct Call {
    receiver: String,
    level: String,
    args: String,
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// `name(Type a, List<X> b)` → the parameter names.
fn param_names(list: &str) -> Vec<String> {
    let (mut out, mut cur, mut depth) = (Vec::new(), String::new(), 0i32);
    for ch in list.chars() {
        match ch {
            '<' | '(' => depth += 1,
            '>' | ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(std::mem::take(&mut cur));
                continue;
            }
            _ => {}
        }
        cur.push(ch);
    }
    out.push(cur);
    out.iter().filter_map(|p| p.split_whitespace().last().map(str::to_string)).filter(|n| n.bytes().all(ident_byte)).collect()
}

/// A method header on one line: `[modifiers] Type name(params) [throws X] {`.
fn method_header(t: &str, indent: usize) -> Option<Method> {
    if !t.ends_with('{') {
        return None;
    }
    let open = t.find('(')?;
    let before = &t[..open];
    if before.contains('=') {
        return None;
    }
    let first = t.trim_start_matches(|c: char| c == '}' || c.is_whitespace()).split(|c: char| !c.is_alphanumeric()).next().unwrap_or("");
    if CONTROL.contains(&first) {
        return None;
    }
    let words: Vec<&str> = before.split_whitespace().collect();
    let name = *words.last()?;
    if words.len() < 2 || !name.bytes().all(ident_byte) {
        return None;
    }
    let close = t.rfind(')')?;
    (close > open).then(|| Method { name: name.to_string(), params: param_names(&t[open + 1..close]), indent })
}

/// `catch (Type name) {` → `name`.
fn catch_var(t: &str) -> Option<String> {
    let at = t.find("catch")?;
    let rest = t[at + 5..].trim_start().strip_prefix('(')?;
    let inner = &rest[..rest.find(')')?];
    inner.split_whitespace().last().map(str::to_string).filter(|n| n.bytes().all(ident_byte))
}

/// A statement `receiver.level(args);` that fits on this line.
fn log_call(t: &str) -> Option<Call> {
    let dot = t.find('.')?;
    let receiver = &t[..dot];
    if receiver.is_empty() || !receiver.bytes().all(ident_byte) || !receiver.to_ascii_lowercase().contains("log") {
        return None;
    }
    let after = &t[dot + 1..];
    let open = after.find('(')?;
    let level = &after[..open];
    if !LEVELS.contains(&level) {
        return None;
    }
    let body = after[open + 1..].strip_suffix(';')?.trim_end().strip_suffix(')')?;
    Some(Call { receiver: receiver.to_string(), level: level.to_string(), args: body.to_string() })
}

impl Walker {
    /// Advances over one line and says what it is, if it is a log call.
    fn step(&mut self, line: &str) -> Option<(Placement, Call)> {
        let t = line.trim();
        if t.is_empty() || t.starts_with("//") || t.starts_with('*') || t.starts_with("/*") || t.starts_with('@') {
            return None;
        }
        let indent = indent_of(line);
        if self.catch.as_ref().is_some_and(|c| indent <= c.indent) && !(t.starts_with('}') && t.contains("catch")) {
            self.catch = None;
        }
        if t.contains("catch") && t.ends_with('{') {
            if let Some(var) = catch_var(t) {
                self.catch = Some(Catch { var, indent });
                return None;
            }
        }
        if let Some(m) = method_header(t, indent) {
            self.method = Some(m);
            self.catch = None;
            self.statements = 0;
            return None;
        }
        let inside = self.method.as_ref().is_some_and(|m| indent > m.indent);
        if !inside {
            return None;
        }
        let call = log_call(t);
        // a level guard (`if (log.isDebugEnabled()) {`) is not a statement the log comes after
        let guard = t.starts_with("if") && t.contains("Enabled");
        let placement = if self.catch.is_some() {
            Placement::Catch
        } else if self.statements == 0 {
            Placement::Entry
        } else {
            Placement::Body
        };
        if !guard && t != "}" && (t.ends_with(';') || t.ends_with('{')) {
            self.statements += 1;
        }
        call.map(|c| (placement, c))
    }

    fn context(&self) -> LogContext {
        let placement = if self.catch.is_some() {
            Placement::Catch
        } else if self.statements == 0 && self.method.is_some() {
            Placement::Entry
        } else {
            Placement::Body
        };
        LogContext {
            placement: Some(placement),
            method: self.method.as_ref().map(|m| m.name.clone()).unwrap_or_default(),
            params: self.method.as_ref().map(|m| m.params.clone()).unwrap_or_default(),
            exception: self.catch.as_ref().map(|c| c.var.clone()),
            logger: None,
        }
    }
}

/// The logger a file declares: `Logger LOG = ...`, `Log log = ...`.
fn declared_logger(source: &str) -> Option<String> {
    source.lines().find_map(|l| {
        let t = l.trim();
        let (lhs, _) = t.split_once('=')?;
        let words: Vec<&str> = lhs.split_whitespace().collect();
        let name = *words.last()?;
        (words.len() >= 2 && words[..words.len() - 1].iter().any(|w| w.contains("Log")) && name.bytes().all(ident_byte)).then(|| name.to_string())
    })
}

/// The context of a log statement written at `offset` of `source`.
pub fn context_at(source: &str, offset: usize) -> LogContext {
    let mut end = offset.min(source.len());
    while !source.is_char_boundary(end) {
        end -= 1;
    }
    let line_start = source[..end].rfind('\n').map_or(0, |p| p + 1);
    let mut walker = Walker::default();
    for line in source[..line_start].lines() {
        walker.step(line);
    }
    let mut ctx = walker.context();
    ctx.logger = declared_logger(source);
    ctx
}

// ---- reducing a call to a pattern ----------------------------------------------------------------------------------

/// The string literals and everything between them, so each part can be treated by its own rule.
fn split_literals(args: &str) -> Vec<(bool, String)> {
    let (mut out, mut cur, mut in_str, mut esc) = (Vec::new(), String::new(), false, false);
    for ch in args.chars() {
        if in_str {
            cur.push(ch);
            if esc {
                esc = false;
            } else if ch == '\\' {
                esc = true;
            } else if ch == '"' {
                out.push((true, std::mem::take(&mut cur)));
                in_str = false;
            }
        } else if ch == '"' {
            if !cur.is_empty() {
                out.push((false, std::mem::take(&mut cur)));
            }
            cur.push(ch);
            in_str = true;
        } else {
            cur.push(ch);
        }
    }
    if !cur.is_empty() {
        out.push((in_str, cur));
    }
    out
}

/// Replaces whole-word occurrences of `word` in `text` by `slot`.
fn replace_word(text: &str, word: &str, slot: &str) -> String {
    if word.is_empty() {
        return text.to_string();
    }
    let (b, mut out, mut i) = (text.as_bytes(), String::new(), 0);
    while i < b.len() {
        if text[i..].starts_with(word) && (i == 0 || !ident_byte(b[i - 1])) && !b.get(i + word.len()).is_some_and(|&c| ident_byte(c)) {
            out.push_str(slot);
            i += word.len();
        } else {
            let ch = text[i..].chars().next().unwrap_or(' ');
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

/// The template of a call's arguments, with the method, its parameters and the caught exception as slots.
fn template_of(args: &str, method: &str, params: &[String], exception: Option<&str>) -> String {
    let mut parts = split_literals(args);
    // `"…{}, {}…", a, b` → `"…«ARGS»…", «PARAMS»` when the placeholders are exactly the parameters
    let joined = params.join(", ");
    if !params.is_empty() {
        let placeholders = vec!["{}"; params.len()].join(", ");
        if let Some((_, last)) = parts.last_mut().filter(|(lit, _)| !*lit) {
            if last.trim_start_matches([',', ' ']).trim() == joined {
                *last = last.replace(&joined, "«PARAMS»");
                for (lit, text) in parts.iter_mut() {
                    if *lit && text.contains(&placeholders) {
                        *text = text.replace(&placeholders, "«ARGS»");
                    }
                }
            }
        }
    }
    parts
        .into_iter()
        .map(|(lit, text)| {
            let text = if lit { replace_word(&text, method, "«M»") } else { text };
            match (lit, exception) {
                (false, Some(ex)) => replace_word(&text, ex, "«EX»"),
                _ => text,
            }
        })
        .collect()
}

/// Fills a template's slots from the context; `None` when a slot it needs has nothing to fill it.
fn fill(template: &str, ctx: &LogContext) -> Option<String> {
    let mut out = template.to_string();
    if out.contains("«M»") {
        if ctx.method.is_empty() {
            return None;
        }
        out = out.replace("«M»", &ctx.method);
    }
    if out.contains("«ARGS»") || out.contains("«PARAMS»") {
        if ctx.params.is_empty() {
            return None;
        }
        out = out.replace("«ARGS»", &vec!["{}"; ctx.params.len()].join(", ")).replace("«PARAMS»", &ctx.params.join(", "));
    }
    if out.contains("«EX»") {
        out = out.replace("«EX»", ctx.exception.as_deref()?);
    }
    Some(out)
}

// ---- the profile ---------------------------------------------------------------------------------------------------

impl LogProfile {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `source`'s log statements under `key`, replacing what an earlier version of the same file contributed.
    pub fn add_file(&mut self, key: &str, source: &str) {
        self.remove_file(key);
        // A cheap gate: most files have no log call at all and need no line walk.
        if !LEVELS.iter().any(|l| source.contains(&format!(".{l}("))) {
            return;
        }
        let mut walker = Walker::default();
        let mut found: Vec<(Placement, u64, String)> = Vec::new();
        for line in source.lines() {
            let Some((placement, call)) = walker.step(line) else { continue };
            let (method, params, exception) = match &walker.method {
                Some(m) => (m.name.as_str(), m.params.as_slice(), walker.catch.as_ref().map(|c| c.var.as_str())),
                None => continue,
            };
            let template = template_of(&call.args, method, params, exception);
            let hash = hash_of(&call.level, &template);
            let bucket = self.buckets.entry(Some(placement)).or_default();
            bucket.patterns.entry(hash).or_insert_with(|| Pattern { level: call.level.clone(), template, count: 0 }).count += 1;
            bucket.total += 1;
            *self.receivers.entry(call.receiver.clone()).or_default() += 1;
            found.push((placement, hash, call.receiver));
        }
        for b in self.buckets.values_mut() {
            if b.patterns.len() > BUCKET_CAP {
                let mut counts: Vec<u32> = b.patterns.values().map(|p| p.count).collect();
                counts.sort_unstable();
                let floor = counts[b.patterns.len() - BUCKET_CAP];
                b.patterns.retain(|_, p| p.count >= floor.max(2));
            }
        }
        if !found.is_empty() {
            self.files.insert(key.to_string(), found);
        }
    }

    /// Takes away what `key` contributed.
    pub fn remove_file(&mut self, key: &str) {
        let Some(found) = self.files.remove(key) else { return };
        for (placement, hash, receiver) in found {
            if let Some(b) = self.buckets.get_mut(&Some(placement)) {
                b.total = b.total.saturating_sub(1);
                if let Some(p) = b.patterns.get_mut(&hash) {
                    p.count = p.count.saturating_sub(1);
                    if p.count == 0 {
                        b.patterns.remove(&hash);
                    }
                }
            }
            if let Some(n) = self.receivers.get_mut(&receiver) {
                *n = n.saturating_sub(1);
                if *n == 0 {
                    self.receivers.remove(&receiver);
                }
            }
        }
    }

    /// The calls of every file `include` accepts, with the patterns they refer to — what a persisted copy needs.
    /// Counters are not part of it: they are the sum of the contributions, so [`Self::restore`] recounts them.
    pub fn snapshot(&self, include: impl Fn(&str) -> bool) -> LogSnapshot {
        let mut patterns: HashMap<u64, (String, String)> = HashMap::new();
        let mut files = Vec::new();
        for (key, calls) in &self.files {
            if !include(key) {
                continue;
            }
            for (placement, hash, _) in calls {
                if let Some(p) = self.buckets.get(&Some(*placement)).and_then(|b| b.patterns.get(hash)) {
                    patterns.entry(*hash).or_insert_with(|| (p.level.clone(), p.template.clone()));
                }
            }
            files.push((key.clone(), calls.clone()));
        }
        let mut patterns: Vec<_> = patterns.into_iter().map(|(h, (level, template))| (h, level, template)).collect();
        patterns.sort();
        files.sort_by(|a, b| a.0.cmp(&b.0));
        LogSnapshot { patterns, files }
    }

    /// Adds the contributions of a [`Self::snapshot`], replacing what the same files contributed before. A call whose
    /// pattern the snapshot does not carry is dropped rather than guessed.
    pub fn restore(&mut self, snapshot: &LogSnapshot) {
        let patterns: HashMap<u64, (&str, &str)> = snapshot.patterns.iter().map(|(h, l, t)| (*h, (l.as_str(), t.as_str()))).collect();
        for (key, calls) in &snapshot.files {
            self.remove_file(key);
            let mut kept = Vec::with_capacity(calls.len());
            for (placement, hash, receiver) in calls {
                let Some((level, template)) = patterns.get(hash) else { continue };
                let bucket = self.buckets.entry(Some(*placement)).or_default();
                bucket.patterns.entry(*hash).or_insert_with(|| Pattern { level: level.to_string(), template: template.to_string(), count: 0 }).count += 1;
                bucket.total += 1;
                *self.receivers.entry(receiver.clone()).or_default() += 1;
                kept.push((*placement, *hash, receiver.clone()));
            }
            if !kept.is_empty() {
                self.files.insert(key.clone(), kept);
            }
        }
    }

    /// A number that changes if the hash function behind the pattern ids does (a different Rust release): a persisted
    /// copy written under another value is not trusted.
    pub fn hash_probe() -> u64 {
        hash_of("probe", "{}")
    }

    /// Distinct patterns kept, over all buckets.
    pub fn pattern_count(&self) -> usize {
        self.buckets.values().map(|b| b.patterns.len()).sum()
    }

    /// Log calls counted, over all buckets.
    pub fn call_count(&self) -> u32 {
        self.buckets.values().map(|b| b.total).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.call_count() == 0
    }

    fn logger(&self, ctx: &LogContext) -> Option<String> {
        ctx.logger.clone().or_else(|| self.receivers.iter().max_by_key(|(_, n)| **n).map(|(r, _)| r.clone()))
    }

    /// The rest of the log statement the project would write here, given what is typed of it so far.
    ///
    /// Nothing unless the user has begun the statement (`typed` is a non-empty start of the logger's name or of
    /// the statement), one pattern holds at least [`MIN_SHARE`] of this place's bucket and was seen
    /// [`MIN_COUNT`] times, and every slot it needs can be filled.
    pub fn suggest(&self, ctx: &LogContext, typed: &str) -> Option<LogSuggestion> {
        let typed = typed.trim_start();
        if typed.is_empty() {
            return None;
        }
        let bucket = self.buckets.get(&ctx.placement)?;
        if bucket.total == 0 {
            return None;
        }
        let logger = self.logger(ctx)?;
        let mut ranked: Vec<&Pattern> = bucket.patterns.values().collect();
        ranked.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.template.cmp(&b.template)));
        // the leader among the patterns this place can fill; its share is of the whole bucket
        for p in ranked {
            let Some(args) = fill(&p.template, ctx) else { continue };
            let confidence = p.count as f32 / bucket.total as f32;
            if p.count < MIN_COUNT || confidence < MIN_SHARE {
                return None;
            }
            let statement = format!("{logger}.{}({args});", p.level);
            return statement.strip_prefix(typed).filter(|rest| !rest.is_empty()).map(|rest| LogSuggestion { text: rest.to_string(), confidence, count: p.count });
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SERVICE: &str = r#"
class A {
    private static final Logger LOG = LoggerFactory.getLogger(A.class);

    void load(String id, int times) {
        LOG.debug("START - load({}, {})", id, times);
        run(id);
    }

    void save(String name) {
        LOG.debug("START - save({})", name);
        try {
            write(name);
        } catch (IOException ex) {
            LOG.error("Failed to process: " + ex.getMessage(), ex);
        }
    }

    void drop(String key) {
        LOG.debug("START - drop({})", key);
        try {
            remove(key);
        } catch (IOException e) {
            LOG.error("Failed to process: " + e.getMessage(), e);
        }
    }
}
"#;

    fn profile() -> LogProfile {
        let mut p = LogProfile::new();
        p.add_file("A.java", SERVICE);
        p
    }

    #[test]
    fn entry_logs_become_one_pattern_with_slots() {
        let p = profile();
        let entry = p.buckets.get(&Some(Placement::Entry)).expect("entry bucket");
        assert_eq!(entry.total, 3);
        assert_eq!(entry.patterns.len(), 1, "{:?}", entry.patterns.values().map(|p| &p.template).collect::<Vec<_>>());
        assert!(entry.patterns.values().next().unwrap().template.contains("«ARGS»"));
    }

    #[test]
    fn caught_exceptions_share_a_pattern_whatever_the_variable_is_called() {
        let p = profile();
        let catches = p.buckets.get(&Some(Placement::Catch)).expect("catch bucket");
        assert_eq!(catches.patterns.len(), 1);
        assert_eq!(catches.patterns.values().next().unwrap().count, 2);
    }

    #[test]
    fn suggests_the_entry_line_for_a_new_method_from_its_declaration() {
        let p = profile();
        let src = "class B {\n    void rename(String from, String to) {\n        LOG";
        let ctx = context_at(src, src.len());
        assert_eq!(ctx.placement, Some(Placement::Entry));
        let s = p.suggest(&ctx, "LOG").expect("a suggestion");
        assert_eq!(s.text, r#".debug("START - rename({}, {})", from, to);"#);
        assert_eq!(s.confidence, 1.0);
    }

    #[test]
    fn suggests_the_catch_line_with_the_caught_variable() {
        let p = profile();
        let src = "class B {\n    void f() {\n        g();\n        try {\n            h();\n        } catch (Exception problem) {\n            LOG.";
        let ctx = context_at(src, src.len());
        assert_eq!(ctx.placement, Some(Placement::Catch));
        assert_eq!(ctx.exception.as_deref(), Some("problem"));
        let s = p.suggest(&ctx, "LOG.").expect("a suggestion");
        assert_eq!(s.text, r#"error("Failed to process: " + problem.getMessage(), problem);"#);
    }

    #[test]
    fn silent_without_a_leader_and_before_the_statement_is_begun() {
        let p = profile();
        let ctx = LogContext { placement: Some(Placement::Body), ..LogContext::default() };
        assert!(p.suggest(&ctx, "LOG").is_none(), "no body logs were counted");
        let src = "class B {\n    void f() {\n        ";
        assert!(p.suggest(&context_at(src, src.len()), "").is_none());
    }

    #[test]
    fn a_snapshot_restores_to_the_same_counters_and_suggestions() {
        let src = "class A {\n    void save(String id) {\n        log.debug(\"START - save({})\", id);\n    }\n}\n";
        let mut a = LogProfile::new();
        a.add_file("A.java", src);
        a.add_file("B.java", src);
        let snap = a.snapshot(|k| k != "B.java");
        assert_eq!(snap.files.len(), 1);
        let mut b = LogProfile::new();
        b.restore(&snap);
        assert_eq!(b.call_count(), 1);
        b.restore(&snap); // restoring twice replaces, it does not double
        assert_eq!(b.call_count(), 1);
        b.remove_file("A.java");
        assert!(b.is_empty());
    }

    #[test]
    fn a_changed_file_replaces_its_contribution_and_a_removed_one_leaves() {
        let mut p = profile();
        let calls = p.call_count();
        p.add_file("A.java", SERVICE);
        assert_eq!(p.call_count(), calls, "adding the same file twice must not double the counters");
        p.remove_file("A.java");
        assert!(p.is_empty());
        assert_eq!(p.pattern_count(), 0);
    }
}
