//! Style detection: measure a project's Java sources and report the conventions it follows
//! **nearly always**.
//!
//! ## Design
//!
//! Every measurement is a *vote* (`Votes<K>`): each occurrence of a construct adds one to the
//! option it exhibits (a `{` ending a line votes `same_line` for the construct it opens; a `if (`
//! votes "space before paren"...). A key reaches the output patch only when it has at least
//! [`MIN_SAMPLES`] votes **and** the winning option holds at least [`MIN_SHARE`] of them. Anything
//! less is the project being inconsistent (or too small to tell), and a formatter must not impose a
//! guess: the key is simply absent and the user's own style stays in charge. One threshold pair for
//! every key keeps the behaviour easy to explain; rare constructs (`else`/`catch` placement) just
//! need a bigger corpus.
//!
//! Sources are *masked* first: comment bodies and string / char / text-block contents become
//! spaces (newlines and offsets preserved), so a `{` in a string or a `//` in a URL cannot vote.
//! Line-length and blank-line statistics read the original text instead (comments count there).
//!
//! What is deliberately **not** detected: `organize_imports` (a policy, never inferred), the wrap
//! policies, `wrap_chain`, wildcard thresholds, tab width of tab-indented projects (the width of a
//! tab cannot be seen in the text, so `indent_width` is omitted for them).
//!
//! Detection is text based (no parser), so it is a heuristic by design: it must be right about the
//! 80 % case and silent about the rest.

use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

/// Votes a key needs before it may be reported. Counted in occurrences (lines, braces, commas...)
/// for in-file constructs and in files for per-file ones (line ending, final newline, import
/// layout).
pub const MIN_SAMPLES: u32 = 30;
/// Share of the votes the winning option must hold.
pub const MIN_SHARE: f32 = 0.8;

/// Why one key is in the patch.
#[derive(Debug, Clone, Serialize)]
pub struct Evidence {
    pub key: String,
    pub value: Value,
    /// Fraction of the votes the reported value won.
    pub share: f32,
    /// Votes cast.
    pub samples: u32,
}

/// The result of [`detect_style`].
#[derive(Debug, Clone, Default, Serialize)]
pub struct Detection {
    /// A `FormatStyle` override naming only the detected keys.
    pub patch: Map<String, Value>,
    pub evidence: Vec<Evidence>,
    /// Sources measured.
    pub files: usize,
}

/// Measure `sources` (the full text of Java files) and report the conventions they share.
pub fn detect_style(sources: &[&str]) -> Detection {
    let mut acc = Acc::default();
    for source in sources {
        scan_file(source, &mut acc);
    }
    acc.finish(sources.len())
}

// ── Votes ───────────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
struct Votes<K: Ord> {
    counts: BTreeMap<K, u32>,
}

impl<K: Ord + Clone> Votes<K> {
    fn add(&mut self, key: K) {
        *self.counts.entry(key).or_insert(0) += 1;
    }

    fn total(&self) -> u32 {
        self.counts.values().sum()
    }

    /// `(option, its votes, all votes)` when the evidence is strong enough.
    fn winner(&self) -> Option<(K, u32, u32)> {
        let total = self.total();
        if total < MIN_SAMPLES {
            return None;
        }
        let (key, count) = self.counts.iter().max_by_key(|(_, count)| **count)?;
        if (*count as f32) / (total as f32) < MIN_SHARE {
            return None;
        }
        Some((key.clone(), *count, total))
    }
}

#[derive(Default)]
struct Out {
    patch: Map<String, Value>,
    evidence: Vec<Evidence>,
}

impl Out {
    fn push(&mut self, key: &str, value: Value, count: u32, total: u32) {
        self.evidence.push(Evidence {
            key: key.to_string(),
            value: value.clone(),
            share: count as f32 / total.max(1) as f32,
            samples: total,
        });
        self.patch.insert(key.to_string(), value);
    }

    fn flag(&mut self, key: &str, votes: &Votes<bool>) {
        if let Some((value, count, total)) = votes.winner() {
            self.push(key, Value::Bool(value), count, total);
        }
    }

    /// `true` votes mean `if_true`, `false` votes `if_false`.
    fn choice(&mut self, key: &str, votes: &Votes<bool>, if_true: &str, if_false: &str) {
        if let Some((value, count, total)) = votes.winner() {
            self.push(key, Value::from(if value { if_true } else { if_false }), count, total);
        }
    }

    fn number(&mut self, key: &str, votes: &Votes<u32>, accept: impl Fn(u32) -> bool) {
        if let Some((value, count, total)) = votes.winner() {
            if accept(value) {
                self.push(key, Value::from(value as u64), count, total);
            }
        }
    }
}

// ── Accumulator ─────────────────────────────────────────────────────────────────────────────

#[derive(Default)]
struct Acc {
    /// `true` = the file uses CRLF.
    crlf: Votes<bool>,
    final_newline: Votes<bool>,
    /// `true` = a tab-indented line.
    tabs: Votes<bool>,
    /// Indent step after a block-opening line, in columns (0 = not a plain step).
    step: Votes<u32>,
    /// Extra indent of a continuation line, in columns (0 = not a plain value).
    cont: Votes<u32>,
    /// For the brace votes, `true` = `{` stays on the line of its construct.
    brace_type: Votes<bool>,
    brace_method: Votes<bool>,
    brace_block: Votes<bool>,
    brace_cont: Votes<bool>,
    control_space: Votes<bool>,
    lbrace_space: Votes<bool>,
    comma_space: Votes<bool>,
    cast_space: Votes<bool>,
    binary_space: Votes<bool>,
    /// `true` = a wrapped line starts with its operator.
    op_next: Votes<bool>,
    /// Runs of 1, 2, and 3-or-more consecutive blank lines inside files.
    blank_runs: [u32; 3],
    /// Blank lines (0..=2) above a member that follows another member.
    member_gap: Votes<u32>,
    /// `(characters that are not tabs, tabs)` of every non-blank line.
    line_lens: Vec<(usize, usize)>,
    /// 0 = static block first, 1 = last, 2 = interleaved.
    static_pos: Votes<u8>,
    import_seqs: Vec<Vec<String>>,
    blank_between: Votes<bool>,
    /// `true` = the marker annotation sits on its own line.
    ann_type: Votes<bool>,
    ann_method: Votes<bool>,
    ann_field: Votes<bool>,
}

impl Acc {
    fn brace(&mut self, kind: Kind, same_line: bool) {
        match kind {
            Kind::Type => self.brace_type.add(same_line),
            Kind::Method => self.brace_method.add(same_line),
            Kind::Block => self.brace_block.add(same_line),
        }
    }

    fn finish(self, files: usize) -> Detection {
        let mut out = Out::default();

        out.choice("line_ending", &self.crlf, "crlf", "lf");
        out.flag("ensure_final_newline", &self.final_newline);

        out.flag("use_tabs", &self.tabs);
        let tab_project = self.tabs.counts.get(&true).copied().unwrap_or(0)
            > self.tabs.counts.get(&false).copied().unwrap_or(0);
        if !tab_project {
            out.number("indent_width", &self.step, |w| matches!(w, 2 | 3 | 4 | 8));
        }
        out.number("continuation_indent", &self.cont, |w| (1..=16).contains(&w));

        out.choice("brace_type", &self.brace_type, "same_line", "next_line");
        out.choice("brace_method", &self.brace_method, "same_line", "next_line");
        out.choice("brace_block", &self.brace_block, "same_line", "next_line");
        out.choice("brace_continuation", &self.brace_cont, "same_line", "next_line");

        out.flag("space_before_control_paren", &self.control_space);
        out.flag("space_before_lbrace", &self.lbrace_space);
        out.flag("space_after_comma", &self.comma_space);
        out.flag("space_after_cast", &self.cast_space);
        out.flag("space_around_binary", &self.binary_space);
        out.flag("operator_on_next_line", &self.op_next);

        let indent_for_tabs = match out.patch.get("indent_width").and_then(Value::as_u64) {
            Some(width) => width as usize,
            None => 4,
        };
        self.line_width(&mut out, indent_for_tabs);
        self.blank_lines(&mut out);
        out.number("blank_lines_before_member", &self.member_gap, |_| true);

        self.imports(&mut out);

        out.choice("annotations_type", &self.ann_type, "own_line", "same_line");
        out.choice("annotations_method", &self.ann_method, "own_line", "same_line");
        out.choice("annotations_field", &self.ann_field, "own_line", "same_line");

        Detection { patch: out.patch, evidence: out.evidence, files }
    }

    /// The smallest conventional margin that holds 98 % of the lines.
    fn line_width(&self, out: &mut Out, tab_width: usize) {
        if self.line_lens.len() < 200 {
            return;
        }
        let mut lengths: Vec<usize> =
            self.line_lens.iter().map(|&(chars, tabs)| chars + tabs * tab_width).collect();
        lengths.sort_unstable();
        let rank = ((lengths.len() as f64) * 0.98).ceil() as usize;
        let p98 = lengths[rank.saturating_sub(1).min(lengths.len() - 1)];
        if let Some(&margin) = [80usize, 100, 120, 140, 160].iter().find(|&&m| m >= p98) {
            let within = lengths.iter().filter(|&&l| l <= margin).count();
            out.push("line_width", Value::from(margin as u64), within as u32, lengths.len() as u32);
        }
    }

    /// 1 when double blank lines are (almost) never used, 2 when they are routine, else silent.
    fn blank_lines(&self, out: &mut Out) {
        let total: u32 = self.blank_runs.iter().sum();
        if total < MIN_SAMPLES {
            return;
        }
        let long = self.blank_runs[1] + self.blank_runs[2];
        let ratio = long as f32 / total as f32;
        if long == 0 || ratio <= 0.05 {
            out.push("max_blank_lines", Value::from(1u64), total - long, total);
        } else if ratio >= 0.2 {
            out.push("max_blank_lines", Value::from(2u64), long, total);
        }
    }

    fn imports(&self, out: &mut Out) {
        if let Some((pos, count, total)) = self.static_pos.winner() {
            match pos {
                0 => out.push("static_imports", Value::from("first"), count, total),
                1 => out.push("static_imports", Value::from("last"), count, total),
                _ => {}
            }
        }
        out.flag("blank_line_between_import_groups", &self.blank_between);

        let files = self.import_seqs.len() as u32;
        if files < MIN_SAMPLES {
            return;
        }
        let distinct: BTreeSet<&Vec<String>> = self.import_seqs.iter().collect();
        let mut best: Option<(&Vec<String>, u32)> = None;
        for candidate in distinct {
            let agreeing =
                self.import_seqs.iter().filter(|seq| is_subsequence(seq, candidate)).count() as u32;
            let better = match best {
                None => true,
                Some((current, n)) => agreeing > n || (agreeing == n && candidate.len() > current.len()),
            };
            if better {
                best = Some((candidate, agreeing));
            }
        }
        if let Some((groups, agreeing)) = best {
            if groups.len() >= 2 && agreeing as f32 / files as f32 >= MIN_SHARE {
                let value = Value::Array(groups.iter().map(|g| Value::String(g.clone())).collect());
                out.push("import_groups", value, agreeing, files);
            }
        }
    }
}

fn is_subsequence(small: &[String], big: &[String]) -> bool {
    let mut rest = big.iter();
    small.iter().all(|wanted| rest.any(|have| have == wanted))
}

// ── Masking ─────────────────────────────────────────────────────────────────────────────────

/// `src` with comment bodies and string / char / text-block contents replaced by spaces. Newlines
/// (and `\r`) are kept, so line numbers and line counts are unchanged.
fn mask(src: &str) -> String {
    #[derive(PartialEq, Clone, Copy)]
    enum State {
        Code,
        LineComment,
        BlockComment,
        Str,
        Chr,
        TextBlock,
    }
    fn blank(c: char) -> char {
        if c == '\n' || c == '\r' {
            c
        } else {
            ' '
        }
    }

    let chars: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut state = State::Code;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        match state {
            State::Code => {
                if c == '/' && next == Some('/') {
                    state = State::LineComment;
                    out.push_str("  ");
                    i += 2;
                } else if c == '/' && next == Some('*') {
                    state = State::BlockComment;
                    out.push_str("  ");
                    i += 2;
                } else if c == '"' {
                    if next == Some('"') && chars.get(i + 2) == Some(&'"') {
                        state = State::TextBlock;
                        out.push_str("\"\"\"");
                        i += 3;
                    } else {
                        state = State::Str;
                        out.push('"');
                        i += 1;
                    }
                } else if c == '\'' {
                    state = State::Chr;
                    out.push('\'');
                    i += 1;
                } else {
                    out.push(c);
                    i += 1;
                }
            }
            State::LineComment => {
                if c == '\n' {
                    state = State::Code;
                    out.push('\n');
                } else {
                    out.push(blank(c));
                }
                i += 1;
            }
            State::BlockComment => {
                if c == '*' && next == Some('/') {
                    state = State::Code;
                    out.push_str("  ");
                    i += 2;
                } else {
                    out.push(blank(c));
                    i += 1;
                }
            }
            State::Str | State::Chr => {
                let quote = if state == State::Str { '"' } else { '\'' };
                if c == '\\' {
                    out.push(' ');
                    if let Some(escaped) = next {
                        out.push(blank(escaped));
                    }
                    i += 2;
                } else if c == quote {
                    state = State::Code;
                    out.push(quote);
                    i += 1;
                } else if c == '\n' {
                    // Unterminated literal: do not let it swallow the rest of the file.
                    state = State::Code;
                    out.push('\n');
                    i += 1;
                } else {
                    out.push(blank(c));
                    i += 1;
                }
            }
            State::TextBlock => {
                if c == '\\' {
                    out.push(' ');
                    if let Some(escaped) = next {
                        out.push(blank(escaped));
                    }
                    i += 2;
                } else if c == '"' && next == Some('"') && chars.get(i + 2) == Some(&'"') {
                    state = State::Code;
                    out.push_str("\"\"\"");
                    i += 3;
                } else {
                    out.push(blank(c));
                    i += 1;
                }
            }
        }
    }
    out
}

fn lines_of(text: &str) -> Vec<&str> {
    text.split('\n').map(|line| line.strip_suffix('\r').unwrap_or(line)).collect()
}

// ── Per-file scan ───────────────────────────────────────────────────────────────────────────

fn scan_file(src: &str, acc: &mut Acc) {
    if src.is_empty() {
        return;
    }
    if src.contains('\n') {
        acc.crlf.add(src.contains("\r\n"));
    }
    acc.final_newline.add(src.ends_with('\n'));

    let masked = mask(src);
    let orig = lines_of(src);
    let code = lines_of(&masked);

    for line in &orig {
        if !line.trim().is_empty() {
            let tabs = line.chars().filter(|&c| c == '\t').count();
            acc.line_lens.push((line.chars().count() - tabs, tabs));
        }
    }
    scan_blank_runs(&orig, acc);
    scan_imports(&orig, acc);
    scan_annotations(&code, acc);
    scan_code(&orig, &code, acc);
}

fn scan_blank_runs(orig: &[&str], acc: &mut Acc) {
    let Some(first) = orig.iter().position(|l| !l.trim().is_empty()) else { return };
    let Some(last) = orig.iter().rposition(|l| !l.trim().is_empty()) else { return };
    let mut run = 0usize;
    for line in &orig[first..=last] {
        if line.trim().is_empty() {
            run += 1;
        } else {
            if run > 0 {
                acc.blank_runs[(run - 1).min(2)] += 1;
            }
            run = 0;
        }
    }
}

// ── Imports ─────────────────────────────────────────────────────────────────────────────────

/// Top-level packages that get a group of their own; everything else is the `*` group.
const PLATFORM_ROOTS: [&str; 7] = ["java", "javax", "jakarta", "android", "androidx", "kotlin", "scala"];

struct SeenImport {
    is_static: bool,
    top: String,
    blank_before: bool,
}

fn scan_imports(orig: &[&str], acc: &mut Acc) {
    let mut seen: Vec<SeenImport> = Vec::new();
    let mut blank_before = false;
    for line in orig {
        let t = line.trim();
        if t.is_empty() {
            blank_before = true;
            continue;
        }
        if let Some(rest) = t.strip_prefix("import ") {
            let rest = rest.trim();
            let (is_static, path) = match rest.strip_prefix("static ") {
                Some(path) => (true, path.trim()),
                None => (false, rest),
            };
            let path = path.trim_end_matches(';').trim();
            let top = path.split('.').next().unwrap_or("").to_string();
            seen.push(SeenImport { is_static, top, blank_before });
            blank_before = false;
            continue;
        }
        if t.starts_with("package ") || t.starts_with("//") || t.starts_with("/*") || t.starts_with('*') {
            blank_before = false;
            continue;
        }
        break;
    }

    let ordinary: Vec<&SeenImport> = seen.iter().filter(|i| !i.is_static).collect();
    if !ordinary.is_empty() && ordinary.len() < seen.len() {
        let first_ordinary = seen.iter().position(|i| !i.is_static).unwrap_or(0);
        let last_ordinary = seen.iter().rposition(|i| !i.is_static).unwrap_or(0);
        let first_static = seen.iter().position(|i| i.is_static).unwrap_or(0);
        let last_static = seen.iter().rposition(|i| i.is_static).unwrap_or(0);
        if last_static < first_ordinary {
            acc.static_pos.add(0);
        } else if first_static > last_ordinary {
            acc.static_pos.add(1);
        } else {
            acc.static_pos.add(2);
        }
    }

    if ordinary.len() < 6 {
        return;
    }
    let mut blocks: Vec<Vec<&str>> = Vec::new();
    for (index, import) in ordinary.iter().enumerate() {
        if index == 0 || import.blank_before {
            blocks.push(Vec::new());
        }
        if let Some(block) = blocks.last_mut() {
            block.push(import.top.as_str());
        }
    }
    let mut sequence: Vec<String> = Vec::new();
    for block in &blocks {
        let uniform = block.iter().all(|top| *top == block[0]);
        let label = if uniform && PLATFORM_ROOTS.contains(&block[0]) { block[0] } else { "*" };
        if sequence.last().map(String::as_str) != Some(label) {
            sequence.push(label.to_string());
        }
    }
    acc.import_seqs.push(sequence);

    if blocks.len() >= 2 {
        acc.blank_between.add(true);
    } else {
        let distinct: BTreeSet<&str> = ordinary.iter().map(|i| i.top.as_str()).collect();
        if distinct.len() >= 2 {
            acc.blank_between.add(false);
        }
    }
}

// ── Annotations ─────────────────────────────────────────────────────────────────────────────

/// What a marker annotation decorates.
#[derive(Clone, Copy)]
enum Target {
    Type,
    Method,
    Field,
}

/// The text after the leading run of marker annotations (`@A @B rest` -> `rest`). `None` when the
/// line does not start with one, or when an annotation takes arguments.
fn split_markers(t: &str) -> Option<&str> {
    let mut rest = t;
    let mut count = 0;
    while let Some(after_at) = rest.strip_prefix('@') {
        let end = after_at
            .find(|c: char| !(c.is_alphanumeric() || matches!(c, '_' | '.' | '$')))
            .unwrap_or(after_at.len());
        if end == 0 || &after_at[..end] == "interface" {
            return None;
        }
        let after = after_at[end..].trim_start();
        if after.starts_with('(') {
            return None;
        }
        rest = after;
        count += 1;
    }
    (count > 0).then_some(rest)
}

fn decl_target(decl: &str) -> Option<Target> {
    if is_type_decl(decl) {
        return Some(Target::Type);
    }
    let paren = decl.find('(');
    let equals = decl.find('=');
    match (paren, equals) {
        (Some(p), Some(e)) if e < p => Some(Target::Field),
        (Some(_), _) => Some(Target::Method),
        (None, _) if decl.ends_with(';') || decl.contains('=') => Some(Target::Field),
        _ => None,
    }
}

fn scan_annotations(code: &[&str], acc: &mut Acc) {
    for (i, &line) in code.iter().enumerate() {
        let t = line.trim();
        if !t.starts_with('@') {
            continue;
        }
        let Some(rest) = split_markers(t) else { continue };
        let (own_line, decl) = if rest.is_empty() {
            let mut declaration = None;
            for next in &code[i + 1..] {
                let n = next.trim();
                if n.is_empty() || n.starts_with('@') {
                    continue;
                }
                declaration = Some(n);
                break;
            }
            match declaration {
                Some(d) => (true, d),
                None => continue,
            }
        } else {
            (false, rest)
        };
        match decl_target(decl) {
            Some(Target::Type) => acc.ann_type.add(own_line),
            Some(Target::Method) => acc.ann_method.add(own_line),
            Some(Target::Field) => acc.ann_field.add(own_line),
            None => {}
        }
    }
}

// ── Code structure ──────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Type,
    Method,
    Block,
}

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

fn word_spans(s: &str) -> Vec<(usize, usize)> {
    let bytes = s.as_bytes();
    let mut spans = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if is_ident(bytes[i]) {
            let start = i;
            while i < bytes.len() && is_ident(bytes[i]) {
                i += 1;
            }
            spans.push((start, i));
        } else {
            i += 1;
        }
    }
    spans
}

fn has_word(s: &str, word: &str) -> bool {
    word_spans(s).iter().any(|&(a, b)| &s[a..b] == word)
}

fn starts_with_word(s: &str, word: &str) -> bool {
    s.starts_with(word) && !s.as_bytes().get(word.len()).is_some_and(|&b| is_ident(b))
}

/// A `class` / `interface` / `enum` / `record` / `@interface` header.
fn is_type_decl(s: &str) -> bool {
    let head = match s.find('(') {
        Some(p) => &s[..p],
        None => s,
    };
    let spans = word_spans(head);
    for (k, &(a, b)) in spans.iter().enumerate() {
        if head[..a].trim_end().ends_with('.') {
            continue;
        }
        match &head[a..b] {
            "class" | "interface" | "enum" => return true,
            "record" => {
                if let Some(&(_, next_end)) = spans.get(k + 1) {
                    let after = s[next_end..].trim_start();
                    if after.starts_with('(') || after.starts_with('<') {
                        return true;
                    }
                }
            }
            _ => {}
        }
    }
    false
}

/// What the `{` that ends (or follows) statement `text` opens; `None` when it is not a construct
/// whose brace style is a project convention (lambdas, anonymous classes, array initialisers).
fn classify(text: &str) -> Option<Kind> {
    let mut s = text.trim();
    while let Some(rest) = s.strip_prefix('}') {
        s = rest.trim_start();
    }
    let s = s.trim_end_matches('{').trim_end();
    if s.is_empty() {
        return Some(Kind::Block);
    }
    let spans = word_spans(s);
    let first = match spans.first() {
        Some(&(0, end)) => &s[..end],
        _ => "",
    };
    const CONTROL: [&str; 9] = ["if", "else", "for", "while", "do", "try", "catch", "finally", "switch"];
    if CONTROL.contains(&first) {
        return Some(Kind::Block);
    }
    if first == "synchronized" && s[first.len()..].trim_start().starts_with('(') {
        return Some(Kind::Block);
    }
    if s.contains("->") || has_word(s, "new") {
        return None;
    }
    if s == "static" {
        return Some(Kind::Method);
    }
    if is_type_decl(s) {
        return Some(Kind::Type);
    }
    if matches!(first, "return" | "throw" | "case" | "default" | "assert" | "yield") {
        return None;
    }
    let paren = s.find('(')?;
    if s[..paren].contains('=') {
        return None;
    }
    Some(Kind::Method)
}

fn starts_with_binary_op(t: &str) -> bool {
    t.starts_with("&&") || t.starts_with("||") || t.starts_with("+ ") || t.starts_with("? ") || t.starts_with(": ")
}

fn starts_with_continuation(t: &str) -> bool {
    starts_with_binary_op(t) || (t.starts_with('.') && !t.starts_with(".."))
}

/// `Some(true)`: the line ends with an operator whose placement is a style choice (`&& || + ? :`).
/// `Some(false)`: another operator / `=` / `.` that still continues the statement. `None`: neither.
fn trailing_operator(t: &str, stmt: &str) -> Option<bool> {
    if t.ends_with("&&") || t.ends_with("||") {
        return Some(true);
    }
    if t.ends_with('+') && !t.ends_with("++") {
        return Some(true);
    }
    if t.ends_with('?') {
        return Some(true);
    }
    if t.ends_with(':') {
        let label_or_case = !t.contains(' ') || starts_with_word(t, "case") || starts_with_word(t, "default");
        return (!label_or_case && stmt.contains(" ? ")).then_some(true);
    }
    if t.ends_with('-') && !t.ends_with("--") {
        return Some(false);
    }
    if t.ends_with(&['*', '/', '%', '=', '.', '&', '|', '^'][..]) {
        return Some(false);
    }
    None
}

struct PrevLine {
    indent: usize,
    spaces_only: bool,
    opens_block: bool,
    started_statement: bool,
}

fn scan_code(orig: &[&str], code: &[&str], acc: &mut Acc) {
    let mut paren: i32 = 0;
    let mut paren_stack: Vec<i32> = Vec::new();
    let mut depth: i32 = 0;
    let mut prev_continues = false;
    let mut prev_ends_with_style_op = false;
    let mut stmt = String::new();
    let mut stmt_start = 0usize;
    let mut stmt_indent: Option<usize> = None;
    let mut continuation_seen = false;
    let mut prev_stmt = String::new();
    let mut prev_stmt_start = 0usize;
    let mut prev: Option<PrevLine> = None;
    let mut prev_trimmed: &str = "";

    for (i, &line) in code.iter().enumerate() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        let ws = &line[..line.len() - line.trim_start().len()];
        let spaces_only = !ws.contains('\t');
        let indent = ws.len();
        if ws.starts_with('\t') {
            acc.tabs.add(true);
        } else if ws.starts_with(' ') {
            acc.tabs.add(false);
        }
        let depth_before = depth;
        let continues = prev_continues || starts_with_continuation(t);

        if continues {
            stmt.push(' ');
            stmt.push_str(t);
            if !continuation_seen {
                continuation_seen = true;
                if let (Some(base), true) = (stmt_indent, spaces_only) {
                    if indent > base {
                        let extra = indent - base;
                        acc.cont.add(if extra <= 16 { extra as u32 } else { 0 });
                    }
                }
            }
        } else {
            prev_stmt = std::mem::take(&mut stmt);
            prev_stmt_start = stmt_start;
            stmt.push_str(t);
            stmt_start = i;
            stmt_indent = spaces_only.then_some(indent);
            continuation_seen = false;
        }

        // Indentation step: the first statement line inside a block that the previous line opened.
        if let Some(p) = &prev {
            if p.opens_block
                && p.started_statement
                && !continues
                && p.spaces_only
                && spaces_only
                && indent > p.indent
                && !t.starts_with('}')
            {
                let step = indent - p.indent;
                acc.step.add(if step <= 8 { step as u32 } else { 0 });
            }
        }

        // Braces.
        if t == "{" {
            if !continues {
                if let Some(kind) = classify(&prev_stmt) {
                    acc.brace(kind, false);
                    if kind != Kind::Block {
                        sample_member_gap(orig, code, prev_stmt_start, depth_before, acc);
                    }
                }
            }
        } else if t.ends_with('{') {
            if let Some(kind) = classify(&stmt) {
                acc.brace(kind, true);
                if kind != Kind::Block {
                    sample_member_gap(orig, code, stmt_start, depth_before, acc);
                }
            }
        }

        // `} else {` against `}` / `else`.
        if let Some(rest) = t.strip_prefix('}') {
            let rest = rest.trim_start();
            let continuation_keyword = starts_with_word(rest, "else")
                || starts_with_word(rest, "catch")
                || starts_with_word(rest, "finally")
                || (starts_with_word(rest, "while") && rest.ends_with(';'));
            if continuation_keyword {
                acc.brace_cont.add(true);
            }
        } else if (starts_with_word(t, "else") || starts_with_word(t, "catch") || starts_with_word(t, "finally"))
            && prev_trimmed == "}"
        {
            acc.brace_cont.add(false);
        }

        // Spacing.
        control_paren_votes(t, &mut acc.control_space);
        lbrace_votes(t, &mut acc.lbrace_space);
        comma_votes(t, &mut acc.comma_space);
        cast_votes(t, &mut acc.cast_space);
        binary_votes(t, &mut acc.binary_space);

        // Where a wrapped binary operator goes.
        if continues {
            if starts_with_binary_op(t) {
                acc.op_next.add(true);
            } else if prev_ends_with_style_op {
                acc.op_next.add(false);
            }
        }

        // Structure bookkeeping for the next line.
        for ch in t.chars() {
            match ch {
                '(' | '[' => paren += 1,
                ')' | ']' => paren = (paren - 1).max(0),
                '{' => {
                    paren_stack.push(paren);
                    paren = 0;
                    depth += 1;
                }
                '}' => {
                    paren = paren_stack.pop().unwrap_or(0);
                    depth = (depth - 1).max(0);
                }
                _ => {}
            }
        }
        let terminated = t.ends_with('{') || t.ends_with('}') || t.ends_with(';');
        let operator = trailing_operator(t, &stmt);
        prev_continues = !terminated && (paren > 0 || operator.is_some());
        prev_ends_with_style_op = operator == Some(true);
        prev = Some(PrevLine {
            indent,
            spaces_only,
            opens_block: t.ends_with('{'),
            started_statement: !continues,
        });
        prev_trimmed = t;
    }
}

/// Blank lines above the member whose statement starts on line `start`, when it follows another
/// member (a `}` or `;`). Annotations and comments directly above belong to the member.
fn sample_member_gap(orig: &[&str], code: &[&str], start: usize, depth_before: i32, acc: &mut Acc) {
    if depth_before < 1 || start >= orig.len() {
        return;
    }
    let mut top = start;
    while top > 0 {
        let above = orig[top - 1].trim();
        if above.starts_with('@') || above.starts_with("//") || above.starts_with("/*") || above.starts_with('*') {
            top -= 1;
        } else {
            break;
        }
    }
    let mut blanks = 0usize;
    while top > blanks && orig[top - blanks - 1].trim().is_empty() {
        blanks += 1;
    }
    if top == blanks {
        return;
    }
    let previous = code[top - blanks - 1].trim();
    if previous.ends_with('}') || previous.ends_with(';') {
        acc.member_gap.add(blanks.min(2) as u32);
    }
}

// ── Spacing votes (all on masked, trimmed lines) ────────────────────────────────────────────

fn control_paren_votes(t: &str, votes: &mut Votes<bool>) {
    for keyword in ["if", "for", "while", "switch", "catch", "synchronized"] {
        let mut from = 0;
        while let Some(found) = t[from..].find(keyword) {
            let at = from + found;
            from = at + keyword.len();
            let before = t[..at].as_bytes().last().copied();
            if before.is_some_and(|b| is_ident(b) || b == b'.') {
                continue;
            }
            let rest = &t[at + keyword.len()..];
            if rest.starts_with('(') {
                votes.add(false);
            } else if rest.starts_with(" (") {
                votes.add(true);
            }
        }
    }
}

fn lbrace_votes(t: &str, votes: &mut Votes<bool>) {
    let b = t.as_bytes();
    for i in 0..b.len() {
        if b[i] != b')' {
            continue;
        }
        match (b.get(i + 1), b.get(i + 2)) {
            (Some(b'{'), _) => votes.add(false),
            (Some(b' '), Some(b'{')) => votes.add(true),
            _ => {}
        }
    }
}

fn comma_votes(t: &str, votes: &mut Votes<bool>) {
    let b = t.as_bytes();
    for i in 0..b.len() {
        if b[i] != b',' {
            continue;
        }
        match b.get(i + 1) {
            None => {}
            Some(b' ') => votes.add(true),
            Some(_) => votes.add(false),
        }
    }
}

fn is_operand_start(b: u8) -> bool {
    is_ident(b) || matches!(b, b'(' | b'"' | b'\'')
}

fn is_cast_type(inner: &str) -> bool {
    if inner.is_empty()
        || !inner.bytes().all(|b| is_ident(b) || matches!(b, b'.' | b'[' | b']' | b'<' | b'>'))
    {
        return false;
    }
    let base = inner.trim_end_matches("[]");
    if ["int", "long", "short", "byte", "char", "float", "double", "boolean"].contains(&base) {
        return true;
    }
    let first_upper = inner.as_bytes()[0].is_ascii_uppercase();
    let constant_like = inner.len() > 1
        && inner.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_');
    first_upper && !constant_like
}

fn ends_with_control_keyword(prefix: &str) -> bool {
    let trimmed = prefix.trim_end();
    word_spans(trimmed).last().is_some_and(|&(a, b)| {
        b == trimmed.len()
            && matches!(&trimmed[a..b], "if" | "while" | "for" | "switch" | "synchronized" | "catch")
    })
}

fn cast_votes(t: &str, votes: &mut Votes<bool>) {
    let b = t.as_bytes();
    for i in 0..b.len() {
        if b[i] != b'(' {
            continue;
        }
        let prev = if i > 0 { b[i - 1] } else { b' ' };
        if is_ident(prev) || matches!(prev, b')' | b']' | b'>') || ends_with_control_keyword(&t[..i]) {
            continue;
        }
        let Some(close) = t[i + 1..].find(')') else { continue };
        if !is_cast_type(&t[i + 1..i + 1 + close]) {
            continue;
        }
        let rest = &b[i + 1 + close + 1..];
        match rest.first().copied() {
            Some(b' ') => {
                if rest.get(1).is_some_and(|&c| is_operand_start(c)) {
                    votes.add(true);
                }
            }
            Some(c) if is_operand_start(c) => votes.add(false),
            _ => {}
        }
    }
}

/// `a+b` against `a + b`, on plain identifier / number operands only.
fn binary_votes(t: &str, votes: &mut Votes<bool>) {
    let b = t.as_bytes();
    for op in ["&&", "||", "==", "!=", "<=", ">=", "+", "-", "*", "/", "%"] {
        let ob = op.as_bytes();
        let n = ob.len();
        let mut i = 0;
        while i + n <= b.len() {
            if &b[i..i + n] != ob {
                i += 1;
                continue;
            }
            let before = if i > 0 { Some(b[i - 1]) } else { None };
            let after = b.get(i + n).copied();
            let exponent = n == 1
                && matches!(op, "+" | "-")
                && i >= 2
                && matches!(b[i - 1], b'e' | b'E')
                && b[i - 2].is_ascii_digit();
            if let (Some(x), Some(y)) = (before, after) {
                if is_ident(x) && is_ident(y) && !exponent {
                    votes.add(false);
                } else if x == b' '
                    && y == b' '
                    && i >= 2
                    && is_ident(b[i - 2])
                    && b.get(i + n + 1).is_some_and(|&c| is_ident(c))
                {
                    votes.add(true);
                }
            }
            i += n;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn src(lines: &[&str]) -> String {
        let mut text = lines.join("\n");
        text.push('\n');
        text
    }

    /// Detect over `n` copies of `source`, as if they were `n` files.
    fn detect_copies(source: &str, n: usize) -> Detection {
        let files: Vec<String> = vec![source.to_string(); n];
        let refs: Vec<&str> = files.iter().map(String::as_str).collect();
        detect_style(&refs)
    }

    fn detect(source: &str) -> Detection {
        detect_copies(source, 40)
    }

    fn kr() -> String {
        src(&[
            "package p;",
            "",
            "import java.util.List;",
            "",
            "public class Foo {",
            "",
            "    private int count;",
            "",
            "    @Override",
            "    public String toString() {",
            "        if (count > 0) {",
            "            return \"a\";",
            "        } else {",
            "            return \"b\";",
            "        }",
            "    }",
            "",
            "    public int add(int a, int b) {",
            "        for (int i = 0; i < a; i++) {",
            "            count = count + i;",
            "        }",
            "        return a + b;",
            "    }",
            "}",
        ])
    }

    fn allman() -> String {
        src(&[
            "package p;",
            "",
            "public class Foo",
            "{",
            "    public int add(int a, int b)",
            "    {",
            "        if (a > b)",
            "        {",
            "            return a;",
            "        }",
            "        else",
            "        {",
            "            return b;",
            "        }",
            "    }",
            "}",
        ])
    }

    #[test]
    fn a_k_and_r_project() {
        let d = detect(&kr());
        let p = &d.patch;
        assert_eq!(d.files, 40);
        assert_eq!(p.get("use_tabs"), Some(&json!(false)));
        assert_eq!(p.get("indent_width"), Some(&json!(4)));
        assert_eq!(p.get("brace_type"), Some(&json!("same_line")));
        assert_eq!(p.get("brace_method"), Some(&json!("same_line")));
        assert_eq!(p.get("brace_block"), Some(&json!("same_line")));
        assert_eq!(p.get("brace_continuation"), Some(&json!("same_line")));
        assert_eq!(p.get("space_before_control_paren"), Some(&json!(true)));
        assert_eq!(p.get("space_before_lbrace"), Some(&json!(true)));
        assert_eq!(p.get("space_after_comma"), Some(&json!(true)));
        assert_eq!(p.get("space_around_binary"), Some(&json!(true)));
        assert_eq!(p.get("max_blank_lines"), Some(&json!(1)));
        assert_eq!(p.get("blank_lines_before_member"), Some(&json!(1)));
        assert_eq!(p.get("annotations_method"), Some(&json!("own_line")));
        assert_eq!(p.get("ensure_final_newline"), Some(&json!(true)));
        assert_eq!(p.get("line_ending"), Some(&json!("lf")));
        // Never inferred.
        assert!(!p.contains_key("organize_imports"));
        assert!(!p.contains_key("wrap_chain"));
    }

    #[test]
    fn evidence_matches_the_patch() {
        let d = detect(&kr());
        assert_eq!(d.evidence.len(), d.patch.len());
        for e in &d.evidence {
            assert_eq!(d.patch.get(&e.key), Some(&e.value));
            assert!(e.share >= MIN_SHARE, "{} share {}", e.key, e.share);
            assert!(e.samples >= MIN_SAMPLES || e.key == "line_width", "{} samples {}", e.key, e.samples);
        }
    }

    #[test]
    fn every_detected_key_exists_in_the_style_with_the_same_kind() {
        let known = crate::format::style::FormatStyle::default().to_map();
        for source in [kr(), allman()] {
            for (key, value) in &detect(&source).patch {
                let default = known.get(key).unwrap_or_else(|| panic!("unknown style key {key}"));
                assert_eq!(std::mem::discriminant(default), std::mem::discriminant(value), "kind of {key}");
            }
        }
        let layered = crate::format::style::FormatStyle::layered(&[&detect(&allman()).patch]);
        assert_eq!(layered.brace_method, crate::format::style::BraceStyle::NextLine);
    }

    #[test]
    fn an_allman_project() {
        let d = detect(&allman());
        let p = &d.patch;
        assert_eq!(p.get("brace_type"), Some(&json!("next_line")));
        assert_eq!(p.get("brace_method"), Some(&json!("next_line")));
        assert_eq!(p.get("brace_block"), Some(&json!("next_line")));
        assert_eq!(p.get("brace_continuation"), Some(&json!("next_line")));
        assert_eq!(p.get("indent_width"), Some(&json!(4)));
    }

    #[test]
    fn a_tab_project_reports_tabs_and_no_indent_width() {
        let d = detect(&kr().replace("    ", "\t"));
        assert_eq!(d.patch.get("use_tabs"), Some(&json!(true)));
        assert!(!d.patch.contains_key("indent_width"));
        assert_eq!(d.patch.get("brace_method"), Some(&json!("same_line")));
    }

    #[test]
    fn a_two_space_project() {
        let d = detect(&kr().replace("    ", "  "));
        assert_eq!(d.patch.get("use_tabs"), Some(&json!(false)));
        assert_eq!(d.patch.get("indent_width"), Some(&json!(2)));
    }

    #[test]
    fn compact_spacing_is_detected() {
        let compact = kr()
            .replace("if (", "if(")
            .replace("for (", "for(")
            .replace(") {", "){")
            .replace(", ", ",")
            .replace(" + ", "+");
        let p = detect(&compact).patch;
        assert_eq!(p.get("space_before_control_paren"), Some(&json!(false)));
        assert_eq!(p.get("space_before_lbrace"), Some(&json!(false)));
        assert_eq!(p.get("space_after_comma"), Some(&json!(false)));
        assert_eq!(p.get("space_around_binary"), Some(&json!(false)));
    }

    #[test]
    fn casts_are_measured() {
        let spaced = src(&["class A {", "    void m(Object o) {", "        String s = (String) o;", "    }", "}"]);
        let tight = spaced.replace("(String) o", "(String)o");
        assert_eq!(detect(&spaced).patch.get("space_after_cast"), Some(&json!(true)));
        assert_eq!(detect(&tight).patch.get("space_after_cast"), Some(&json!(false)));
    }

    #[test]
    fn a_condition_in_parens_is_not_a_cast() {
        let source = src(&["class A {", "    void m() {", "        if (Ready) go();", "        while (Busy) wait();", "    }", "}"]);
        assert!(!detect(&source).patch.contains_key("space_after_cast"));
    }

    #[test]
    fn continuation_indent_and_leading_operators() {
        let source = src(&[
            "public class Foo {",
            "    void m() {",
            "        String s = compute(first,",
            "                second);",
            "        boolean b = a",
            "                && c;",
            "    }",
            "}",
        ]);
        let p = detect(&source).patch;
        assert_eq!(p.get("continuation_indent"), Some(&json!(8)));
        assert_eq!(p.get("operator_on_next_line"), Some(&json!(true)));
    }

    #[test]
    fn trailing_operators_are_detected() {
        let source = src(&[
            "public class Foo {",
            "    boolean m(boolean a, boolean c) {",
            "        boolean b = a &&",
            "                c;",
            "        return b;",
            "    }",
            "}",
        ]);
        let p = detect(&source).patch;
        assert_eq!(p.get("operator_on_next_line"), Some(&json!(false)));
        assert_eq!(p.get("continuation_indent"), Some(&json!(8)));
    }

    #[test]
    fn a_lambda_body_is_not_a_continuation() {
        let source = src(&[
            "class A {",
            "    void m() {",
            "        run(() -> {",
            "            work();",
            "        });",
            "    }",
            "}",
        ]);
        assert!(!detect(&source).patch.contains_key("continuation_indent"));
    }

    #[test]
    fn same_line_annotations() {
        let source = src(&[
            "class A {",
            "    @Autowired private Foo foo;",
            "    @Override public String toString() { return \"\"; }",
            "}",
        ]);
        let p = detect(&source).patch;
        assert_eq!(p.get("annotations_field"), Some(&json!("same_line")));
        assert_eq!(p.get("annotations_method"), Some(&json!("same_line")));
    }

    #[test]
    fn annotations_with_arguments_are_not_markers() {
        let source = src(&["class A {", "    @RequestMapping(\"/x\") public void m() {}", "}"]);
        assert!(!detect(&source).patch.contains_key("annotations_method"));
    }

    #[test]
    fn crlf_files_are_reported() {
        let d = detect(&kr().replace('\n', "\r\n"));
        assert_eq!(d.patch.get("line_ending"), Some(&json!("crlf")));
        // The structure is still read correctly through the `\r`.
        assert_eq!(d.patch.get("brace_method"), Some(&json!("same_line")));
        assert_eq!(d.patch.get("indent_width"), Some(&json!(4)));
    }

    #[test]
    fn missing_final_newline_is_reported() {
        let source = kr();
        let d = detect(source.trim_end_matches('\n'));
        assert_eq!(d.patch.get("ensure_final_newline"), Some(&json!(false)));
    }

    #[test]
    fn double_blank_lines_are_routine_or_rare() {
        let rare = detect(&kr());
        assert_eq!(rare.patch.get("max_blank_lines"), Some(&json!(1)));
        let doubled = src(&["class A {", "    int a;", "", "", "    int b;", "", "", "    int c;", "}"]);
        assert_eq!(detect(&doubled).patch.get("max_blank_lines"), Some(&json!(2)));
    }

    #[test]
    fn member_gap_of_zero_and_two() {
        let tight = src(&["class A {", "    void a() {", "    }", "    void b() {", "    }", "}"]);
        assert_eq!(detect(&tight).patch.get("blank_lines_before_member"), Some(&json!(0)));
        let roomy = src(&["class A {", "    void a() {", "    }", "", "", "    void b() {", "    }", "}"]);
        assert_eq!(detect(&roomy).patch.get("blank_lines_before_member"), Some(&json!(2)));
    }

    #[test]
    fn javadoc_belongs_to_its_member() {
        let source = src(&[
            "class A {",
            "    int x;",
            "",
            "    /** Doc. */",
            "    void a() {",
            "    }",
            "}",
        ]);
        assert_eq!(detect(&source).patch.get("blank_lines_before_member"), Some(&json!(1)));
    }

    fn import_file(static_last: bool) -> String {
        let mut lines = vec!["package p;", ""];
        if !static_last {
            lines.extend(["import static org.junit.Assert.assertTrue;", ""]);
        }
        lines.extend([
            "import java.util.List;",
            "import java.util.Map;",
            "import java.io.File;",
            "import java.nio.file.Path;",
            "",
            "import javax.inject.Inject;",
            "",
            "import org.acme.A;",
            "import com.acme.B;",
        ]);
        if static_last {
            lines.extend(["", "import static org.junit.Assert.assertTrue;"]);
        }
        lines.extend(["", "class C {}"]);
        src(&lines)
    }

    #[test]
    fn import_layout_statics_first() {
        let d = detect(&import_file(false));
        assert_eq!(d.patch.get("static_imports"), Some(&json!("first")));
        assert_eq!(d.patch.get("import_groups"), Some(&json!(["java", "javax", "*"])));
        assert_eq!(d.patch.get("blank_line_between_import_groups"), Some(&json!(true)));
        assert!(!d.patch.contains_key("organize_imports"));
    }

    #[test]
    fn import_layout_statics_last() {
        let d = detect(&import_file(true));
        assert_eq!(d.patch.get("static_imports"), Some(&json!("last")));
        assert_eq!(d.patch.get("import_groups"), Some(&json!(["java", "javax", "*"])));
    }

    #[test]
    fn imports_without_blank_lines_between_groups() {
        let source = src(&[
            "package p;",
            "",
            "import java.util.List;",
            "import java.util.Map;",
            "import javax.inject.Inject;",
            "import org.acme.A;",
            "import org.acme.B;",
            "import org.acme.C;",
            "",
            "class C {}",
        ]);
        let d = detect(&source);
        assert_eq!(d.patch.get("blank_line_between_import_groups"), Some(&json!(false)));
        assert!(!d.patch.contains_key("import_groups"));
    }

    #[test]
    fn inconsistent_import_orders_are_not_reported() {
        let a = import_file(false);
        let b = src(&[
            "package p;",
            "",
            "import org.acme.A;",
            "import com.acme.B;",
            "import com.acme.C;",
            "",
            "import javax.inject.Inject;",
            "",
            "import java.util.List;",
            "import java.util.Map;",
            "import java.io.File;",
            "",
            "class C {}",
        ]);
        let mut files: Vec<String> = vec![a; 20];
        files.extend(vec![b; 20]);
        let refs: Vec<&str> = files.iter().map(String::as_str).collect();
        assert!(!detect_style(&refs).patch.contains_key("import_groups"));
    }

    #[test]
    fn line_width_is_the_smallest_margin_holding_98_percent() {
        let long: Vec<String> = (0..300).map(|i| format!("    int value{i} = {};", "1".repeat(95))).collect();
        let wide = long.join("\n") + "\n";
        assert_eq!(detect_style(&[wide.as_str()]).patch.get("line_width"), Some(&json!(120)));

        let short: Vec<String> = (0..300).map(|i| format!("    int v{i} = 1;")).collect();
        let narrow = short.join("\n") + "\n";
        assert_eq!(detect_style(&[narrow.as_str()]).patch.get("line_width"), Some(&json!(80)));
    }

    #[test]
    fn line_width_needs_enough_lines() {
        let few: Vec<String> = (0..50).map(|i| format!("    int v{i} = 1;")).collect();
        let text = few.join("\n") + "\n";
        assert!(!detect_style(&[text.as_str()]).patch.contains_key("line_width"));
    }

    #[test]
    fn tabs_count_as_the_indent_width_in_line_width() {
        // 1 tab + 70 chars: 74 columns at width 4 -> 80; at width 8 it would be 78 -> still 80.
        let rows: Vec<String> = (0..300).map(|_| format!("\t{}", "x".repeat(70))).collect();
        let text = rows.join("\n") + "\n";
        assert_eq!(detect_style(&[text.as_str()]).patch.get("line_width"), Some(&json!(80)));
    }

    #[test]
    fn too_little_evidence_yields_nothing() {
        let d = detect_style(&["class A {}\n"]);
        assert!(d.patch.is_empty(), "{:?}", d.patch);
        assert_eq!(d.files, 1);
        assert!(detect_style(&[]).patch.is_empty());
    }

    #[test]
    fn a_split_project_yields_nothing() {
        // Half Allman, half K&R methods: no majority at MIN_SHARE.
        let mut files: Vec<String> = vec![kr(); 20];
        files.extend(vec![allman(); 20]);
        let refs: Vec<&str> = files.iter().map(String::as_str).collect();
        let d = detect_style(&refs);
        assert!(!d.patch.contains_key("brace_method"), "{:?}", d.patch);
        assert!(!d.patch.contains_key("brace_type"));
    }

    #[test]
    fn strings_and_comments_cannot_vote() {
        let source = src(&[
            "class A {",
            "    // if(x){ , a+b }",
            "    String s = \"if(x){ y,z a+b\";",
            "    /* for(;;){ */",
            "}",
        ]);
        let p = detect(&source).patch;
        assert!(!p.contains_key("space_before_control_paren"));
        assert!(!p.contains_key("space_before_lbrace"));
        assert!(!p.contains_key("space_after_comma"));
        assert!(!p.contains_key("space_around_binary"));
    }

    #[test]
    fn anonymous_classes_and_lambdas_do_not_vote_on_braces() {
        let source = src(&[
            "class A {",
            "    void m() {",
            "        Runnable r = new Runnable()",
            "        {",
            "        };",
            "        run(() ->",
            "        {",
            "        });",
            "    }",
            "}",
        ]);
        // Only the class and method braces are classifiable; the others abstain.
        let d = detect(&source);
        assert!(!d.patch.contains_key("brace_block"));
    }

    #[test]
    fn mask_blanks_comments_and_literals_but_keeps_layout() {
        let source = "int a = 1; // note { \nString s = \"x { // y\"; /* c\n { */ char c = '{';\n";
        let masked = mask(source);
        assert_eq!(masked.len(), source.len());
        assert_eq!(masked.matches('\n').count(), source.matches('\n').count());
        assert!(!masked.contains("note"));
        assert!(!masked.contains('{'));
        assert!(masked.contains("int a = 1;"));
        assert!(masked.contains("String s = \""));
    }

    #[test]
    fn mask_handles_escapes_and_text_blocks() {
        let source = "String s = \"a\\\"{\";\nString t = \"\"\"\n  { \"quoted\" }\n  \"\"\";\nint x = 1;\n";
        let masked = mask(source);
        assert!(!masked.contains('{'), "{masked}");
        assert!(masked.contains("int x = 1;"));
        assert_eq!(masked.matches('\n').count(), source.matches('\n').count());
    }

    #[test]
    fn classify_recognises_constructs() {
        assert!(classify("public class Foo {") == Some(Kind::Type));
        assert!(classify("public record Pair(int a, int b) {") == Some(Kind::Type));
        assert!(classify("@interface Marker {") == Some(Kind::Type));
        assert!(classify("public static <T> List<T> of(T... items) {") == Some(Kind::Method));
        assert!(classify("Foo(int x) {") == Some(Kind::Method));
        assert!(classify("static {") == Some(Kind::Method));
        assert!(classify("} else if (x) {") == Some(Kind::Block));
        assert!(classify("try (Reader r = open()) {") == Some(Kind::Block));
        assert!(classify("synchronized (this) {") == Some(Kind::Block));
        assert!(classify("synchronized void m() {") == Some(Kind::Method));
        assert!(classify("Runnable r = new Runnable() {").is_none());
        assert!(classify("run(x -> {").is_none());
        assert!(classify("int[] a = {").is_none());
        assert!(classify("case 1 -> {").is_none());
    }

    #[test]
    fn subsequence_helper() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert!(is_subsequence(&s(&["java", "*"]), &s(&["java", "javax", "*"])));
        assert!(!is_subsequence(&s(&["*", "java"]), &s(&["java", "javax", "*"])));
        assert!(is_subsequence(&s(&[]), &s(&["a"])));
    }
}
