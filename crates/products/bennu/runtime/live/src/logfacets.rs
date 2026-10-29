//! A log as FACETS instead of lines: the exceptions (grouped by class, root cause and the first frame in
//! the project), the SQL statements (literals normalised, slowest first), the HTTP requests (grouped by
//! endpoint, error statuses first) and the warnings (grouped by message template) — each with a count,
//! first and last time and one record id to drill into.
//!
//! A digest is small by design: groups are capped, templates are cut, and every cut is announced with
//! how to widen it. The per-line reading is `arbor-logscan` (through [`crate::logrecord`]); the class of a
//! stack frame is turned into a project file by the caller's resolver, which knows the index.

use std::collections::HashMap;
use std::sync::LazyLock;

use arbor_logscan::prelude::{class_of, outer_class, Level};
use regex::Regex;

use crate::logaccess;
use crate::logread::{message_of, template};
use crate::logrecord::{starts_with_throwable, Record};
use crate::logsql;
use crate::logtime::{iso, parse_duration, TimeWindow};

pub const DEFAULT_LIMIT: usize = 8;
pub const MAX_LIMIT: usize = 30;
const TEMPLATE_CHARS: usize = 160;
const SQL_CHARS: usize = 220;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Facet {
    Exceptions,
    Queries,
    Requests,
    Warnings,
}

impl Facet {
    pub const ALL: [Facet; 4] = [Facet::Exceptions, Facet::Queries, Facet::Requests, Facet::Warnings];

    pub fn parse(word: &str) -> Result<Facet, String> {
        match word.trim().to_ascii_lowercase().as_str() {
            "exceptions" | "exception" | "errors" => Ok(Facet::Exceptions),
            "queries" | "query" | "sql" => Ok(Facet::Queries),
            "requests" | "request" | "http" => Ok(Facet::Requests),
            "warnings" | "warning" | "warns" => Ok(Facet::Warnings),
            other => Err(format!("`{other}` is not a facet: use exceptions, queries, requests or warnings")),
        }
    }

    fn title(self) -> &'static str {
        match self {
            Facet::Exceptions => "EXCEPTIONS",
            Facet::Queries => "QUERIES",
            Facet::Requests => "REQUESTS",
            Facet::Warnings => "WARNINGS",
        }
    }

    /// The fields a `where` term may name for this facet.
    fn fields(self) -> &'static [&'static str] {
        match self {
            Facet::Exceptions => &["class", "message", "frame", "contains"],
            Facet::Queries => &["sql", "duration", "logger", "contains"],
            Facet::Requests => &["method", "url", "status", "duration", "contains"],
            Facet::Warnings => &["message", "logger", "contains"],
        }
    }
}

// ── `where` ──────────────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum Op {
    Contains,
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
    Matches,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cond {
    field: String,
    op: Op,
    value: String,
}

#[derive(Debug, Clone)]
enum Val {
    Text(String),
    Num(f64),
}

static COND: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\w+)\s*(contains|>=|<=|!=|==|=|>|<|~)\s*(.+)$").expect("cond regex"));
static AND: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\s+and\s+|;").expect("and regex"));

/// `class contains Foo and status >= 500` (also `;`-separated).
pub fn parse_where(text: &str) -> Result<Vec<Cond>, String> {
    let mut out = Vec::new();
    for term in AND.split(text).map(str::trim).filter(|t| !t.is_empty()) {
        let c = COND.captures(term).ok_or_else(|| format!("`{term}` is not a condition: write `field op value`, e.g. `status >= 500`, `class contains Order`, `duration > 200ms`"))?;
        let op = match &c[2] {
            "contains" => Op::Contains,
            "=" | "==" => Op::Eq,
            "!=" => Op::Ne,
            ">" => Op::Gt,
            ">=" => Op::Ge,
            "<" => Op::Lt,
            "<=" => Op::Le,
            _ => Op::Matches,
        };
        let value = c[3].trim().trim_matches(['"', '\'']).to_string();
        if op == Op::Matches {
            Regex::new(&value).map_err(|e| format!("`{value}` is not a valid regex: {e}"))?;
        }
        out.push(Cond { field: c[1].to_ascii_lowercase(), op, value });
    }
    Ok(out)
}

fn holds(cond: &Cond, got: &Val) -> bool {
    let text = |v: &Val| match v {
        Val::Text(t) => t.clone(),
        Val::Num(n) => n.to_string(),
    };
    match cond.op {
        Op::Contains => text(got).to_lowercase().contains(&cond.value.to_lowercase()),
        Op::Matches => Regex::new(&cond.value).is_ok_and(|re| re.is_match(&text(got))),
        Op::Eq => match got {
            Val::Num(n) => cond.value.parse::<f64>().is_ok_and(|v| (v - n).abs() < 1e-9),
            Val::Text(t) => t.eq_ignore_ascii_case(&cond.value),
        },
        Op::Ne => !holds(&Cond { op: Op::Eq, ..cond.clone() }, got),
        Op::Gt | Op::Ge | Op::Lt | Op::Le => {
            let Val::Num(n) = got else { return false };
            let Some(v) = number_of(&cond.field, &cond.value) else { return false };
            match cond.op {
                Op::Gt => *n > v,
                Op::Ge => *n >= v,
                Op::Lt => *n < v,
                _ => *n <= v,
            }
        }
    }
}

/// A duration may be written with a unit (`200ms`, `2s`); every other number is plain.
fn number_of(field: &str, value: &str) -> Option<f64> {
    if field == "duration" && value.bytes().any(|b| b.is_ascii_alphabetic()) {
        return parse_duration(value).ok().map(|ms| ms as f64);
    }
    value.parse().ok()
}

fn check_fields(facet: Facet, conds: &[Cond]) -> Result<(), String> {
    for c in conds {
        if !facet.fields().contains(&c.field.as_str()) {
            return Err(format!("`{}` is not a field of the {} facet; it has: {}", c.field, facet.title().to_lowercase(), facet.fields().join(", ")));
        }
    }
    Ok(())
}

/// A record passes when every condition holds on the fields `facts` gives it.
fn matches_all(conds: &[Cond], rec: &Record, facts: &[(&str, Val)]) -> bool {
    conds.iter().all(|c| {
        if c.field == "contains" {
            return holds(c, &Val::Text(rec.text()));
        }
        facts.iter().find(|(name, _)| *name == c.field).is_some_and(|(_, v)| holds(c, v))
    })
}

// ── Groups ───────────────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Default, Clone)]
struct Agg {
    label: String,
    detail: Vec<String>,
    count: usize,
    first_ts: Option<u64>,
    last_ts: Option<u64>,
    sample_id: u64,
    /// Time statistics for the facets that have them.
    timed: usize,
    total_ms: f64,
    max_ms: f64,
    /// Request statuses seen, for the endpoint rows.
    statuses: Vec<(u16, usize)>,
    server_errors: usize,
    client_errors: usize,
}

/// Collects occurrences under a key, in first-seen order.
#[derive(Default)]
struct Groups {
    order: Vec<String>,
    by_key: HashMap<String, Agg>,
}

impl Groups {
    fn add(&mut self, key: String, rec: &Record, make: impl FnOnce() -> (String, Vec<String>)) -> &mut Agg {
        if !self.by_key.contains_key(&key) {
            let (label, detail) = make();
            self.order.push(key.clone());
            self.by_key.insert(key.clone(), Agg { label, detail, sample_id: rec.id, ..Default::default() });
        }
        let agg = self.by_key.get_mut(&key).expect("just inserted");
        agg.count += 1;
        agg.first_ts = match (agg.first_ts, rec.ts_ms) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        agg.last_ts = agg.last_ts.max(rec.ts_ms);
        agg
    }

    fn into_vec(self) -> Vec<Agg> {
        let mut by_key = self.by_key;
        self.order.into_iter().filter_map(|k| by_key.remove(&k)).collect()
    }
}

fn time_stats(agg: &mut Agg, ms: Option<f64>, rec: &Record, slowest_is_sample: bool) {
    if let Some(ms) = ms {
        agg.timed += 1;
        agg.total_ms += ms;
        if ms > agg.max_ms {
            agg.max_ms = ms;
            if slowest_is_sample {
                agg.sample_id = rec.id;
            }
        }
    }
}

// ── Exceptions ───────────────────────────────────────────────────────────────────────────────────

static THROWABLE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[A-Za-z_][\w$]*(?:\.[A-Za-z_$][\w$]*)*(?:Exception|Error|Throwable)\b").expect("throwable regex"));
static FRAME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*at\s+(\S+?)\(([^)]*)\)").expect("frame regex"));
static RUST_AT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*at\s+(\S+?):(\d+)(?::\d+)?\s*$").expect("rust at regex"));
static PANIC_AT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"panicked at\s+(\S+?):(\d+)(?::\d+)?").expect("panic regex"));

const LIBRARY_PREFIXES: [&str; 16] = [
    "java.", "javax.", "jakarta.", "jdk.", "sun.", "com.sun.", "org.springframework.", "org.apache.", "org.hibernate.", "io.netty.", "kotlin.", "scala.",
    "org.junit.", "com.zaxxer.", "reactor.", "org.eclipse.",
];

#[derive(Debug, Clone, PartialEq)]
pub struct ExceptionFact {
    pub class: String,
    pub root: Option<String>,
    pub message: String,
    pub frame: Option<String>,
}

fn throwable_in(line: &str) -> Option<&str> {
    THROWABLE.find(line).map(|m| m.as_str())
}

fn is_library(class: &str) -> bool {
    LIBRARY_PREFIXES.iter().any(|p| class.starts_with(p))
}

/// The frame to blame: the first one in the project (per the resolver), else the first outside the well-known
/// libraries, else the first. `section` is a slice of the record's lines (one `Caused by` chain element).
fn pick_frame(lines: &[&str], resolve: &dyn Fn(&str) -> Option<String>) -> Option<(String, bool)> {
    struct F {
        class: String,
        method: String,
        file: String,
        line: String,
    }
    let frames: Vec<F> = lines
        .iter()
        .filter_map(|l| FRAME.captures(l))
        .map(|c| {
            let qualified = c[1].to_string();
            let class = class_of(&qualified);
            let method = qualified.rsplit('.').next().unwrap_or("").to_string();
            let (file, line) = c[2].rsplit_once(':').map(|(f, n)| (f.to_string(), n.to_string())).unwrap_or((c[2].to_string(), String::new()));
            F { class, method, file, line }
        })
        .collect();
    let show = |f: &F, path: Option<String>| {
        let simple = f.class.rsplit('.').next().unwrap_or(&f.class);
        match path {
            Some(p) if !f.line.is_empty() => format!("{p}:{} ({simple}.{})", f.line, f.method),
            Some(p) => format!("{p} ({simple}.{})", f.method),
            None if !f.line.is_empty() => format!("{}.{} ({}:{})", f.class, f.method, f.file, f.line),
            None => format!("{}.{}", f.class, f.method),
        }
    };
    if let Some((f, p)) = frames.iter().find_map(|f| resolve(outer_class(&f.class)).map(|p| (f, p))) {
        return Some((show(f, Some(p)), true));
    }
    frames.iter().find(|f| !is_library(&f.class)).or(frames.first()).map(|f| (show(f, None), false))
}

/// The first of the user's own Rust locations in a backtrace (not the standard library, not a dependency).
fn rust_location(rec: &Record) -> Option<String> {
    let own = |path: &str| !(path.contains("/rustc/") || path.contains("/.cargo/") || path.contains("\\.cargo\\") || path.contains("/library/"));
    rec.rest.iter().filter_map(|l| RUST_AT.captures(l)).find(|c| own(&c[1])).map(|c| format!("{}:{}", c[1].trim_start_matches("./"), &c[2])).or_else(|| {
        let c = PANIC_AT.captures(&rec.head)?;
        Some(format!("{}:{}", &c[1], &c[2]))
    })
}

/// The exception a record reports, if it reports one.
pub fn exception_of(rec: &Record, resolve: &dyn Fn(&str) -> Option<String>) -> Option<ExceptionFact> {
    if rec.head.contains("panicked at") {
        let message = rec.rest.iter().find(|l| !l.trim_start().starts_with("stack backtrace") && !l.trim().is_empty()).map(|l| l.trim().to_string()).unwrap_or_default();
        return Some(ExceptionFact { class: "panic".into(), root: None, message, frame: rust_location(rec) });
    }
    let lines: Vec<&str> = std::iter::once(rec.head.as_str()).chain(rec.rest.iter().map(String::as_str)).collect();
    let thrown_line = rec.rest.iter().find(|l| starts_with_throwable(l.trim_start()) && !l.trim_start().starts_with("Caused by"));
    let top = thrown_line.and_then(|l| throwable_in(l)).or_else(|| throwable_in(&rec.head))?;
    let has_trace = rec.rest.iter().any(|l| FRAME.is_match(l) || l.trim_start().starts_with("Caused by"));
    if !(has_trace || rec.level.is_some_and(|l| l >= Level::Warn) || starts_with_throwable(&rec.head)) {
        return None;
    }
    // Sections of the chain: lines up to the first `Caused by`, then one section per `Caused by`.
    let cause_at: Vec<usize> = lines.iter().enumerate().filter(|(_, l)| l.trim_start().starts_with("Caused by:")).map(|(i, _)| i).collect();
    let root_line = cause_at.last().map(|&i| lines[i]);
    let root = root_line.and_then(|l| throwable_in(l)).map(str::to_string).filter(|r| r != top);
    let section = match cause_at.last() {
        Some(&i) => &lines[i..],
        None => &lines[..],
    };
    // The deepest cause's own trace is the most telling; fall back to the whole record when it has no project frame.
    let resolved = |lines: &[&str]| pick_frame(lines, resolve).filter(|(_, ok)| *ok).map(|(f, _)| f);
    let frame = resolved(section).or_else(|| resolved(&lines)).or_else(|| pick_frame(section, resolve).map(|(f, _)| f));
    let message_line = root_line.or(thrown_line.map(String::as_str)).unwrap_or(rec.head.as_str());
    let message_line = message_line.trim_start().strip_prefix("Caused by:").map(str::trim_start).unwrap_or(message_line.trim_start());
    let message = match message_line.split_once(": ").filter(|(head, _)| throwable_in(head).is_some()) {
        Some((_, m)) => m.to_string(),
        None => rec.message.clone(),
    };
    Some(ExceptionFact { class: top.to_string(), root, message, frame })
}

fn exception_groups(records: &[&Record], conds: &[Cond], resolve: &dyn Fn(&str) -> Option<String>) -> Vec<Agg> {
    let mut groups = Groups::default();
    for rec in records {
        let Some(fact) = exception_of(rec, resolve) else { continue };
        let facts = [
            ("class", Val::Text(format!("{} {}", fact.class, fact.root.as_deref().unwrap_or("")))),
            ("message", Val::Text(fact.message.clone())),
            ("frame", Val::Text(fact.frame.clone().unwrap_or_default())),
        ];
        if !matches_all(conds, rec, &facts) {
            continue;
        }
        let key = format!("{}|{}|{}", fact.class, fact.root.as_deref().unwrap_or(""), fact.frame.as_deref().unwrap_or(""));
        groups.add(key, rec, || {
            let label = match &fact.root {
                Some(root) => format!("{} (root cause {root})", fact.class),
                None => fact.class.clone(),
            };
            let mut detail = vec![format!("message: {}", template(&fact.message, TEMPLATE_CHARS))];
            detail.extend(fact.frame.as_ref().map(|f| format!("at {f}")));
            (label, detail)
        });
    }
    groups.into_vec()
}

// ── Queries, requests, warnings ──────────────────────────────────────────────────────────────────

fn query_groups(records: &[&Record], conds: &[Cond]) -> Vec<Agg> {
    let mut groups = Groups::default();
    for rec in records {
        let Some(st) = logsql::extract(rec) else { continue };
        let norm = logsql::normalize(&st.sql, SQL_CHARS);
        let facts = [
            ("sql", Val::Text(norm.clone())),
            ("duration", st.ms.map(Val::Num).unwrap_or(Val::Text(String::new()))),
            ("logger", Val::Text(rec.logger.clone().unwrap_or_default())),
        ];
        if !matches_all(conds, rec, &facts) {
            continue;
        }
        let agg = groups.add(norm.clone(), rec, || (norm, Vec::new()));
        time_stats(agg, st.ms, rec, true);
    }
    groups.into_vec()
}

fn request_groups(records: &[&Record], conds: &[Cond]) -> Vec<Agg> {
    let mut groups = Groups::default();
    for rec in records {
        let Some(a) = logaccess::extract(rec) else { continue };
        let facts = [
            ("method", Val::Text(a.method.clone())),
            ("url", Val::Text(a.endpoint.clone())),
            ("status", Val::Num(f64::from(a.status))),
            ("duration", a.ms.map(Val::Num).unwrap_or(Val::Text(String::new()))),
        ];
        if !matches_all(conds, rec, &facts) {
            continue;
        }
        let agg = groups.add(format!("{} {}", a.method, a.endpoint), rec, || (format!("{} {}", a.method, a.endpoint), Vec::new()));
        match agg.statuses.iter_mut().find(|(s, _)| *s == a.status) {
            Some((_, n)) => *n += 1,
            None => agg.statuses.push((a.status, 1)),
        }
        // The sample is the first failing call: that is the one worth opening.
        let first_failure = match a.status {
            500.. => agg.server_errors == 0,
            400..=499 => agg.server_errors == 0 && agg.client_errors == 0,
            _ => false,
        };
        if first_failure {
            agg.sample_id = rec.id;
        }
        match a.status {
            500.. => agg.server_errors += 1,
            400..=499 => agg.client_errors += 1,
            _ => {}
        }
        time_stats(agg, a.ms, rec, false);
    }
    groups.into_vec()
}

fn warning_groups(records: &[&Record], conds: &[Cond]) -> Vec<Agg> {
    let mut groups = Groups::default();
    for rec in records.iter().filter(|r| r.level == Some(Level::Warn)) {
        let key = template(message_of(rec), TEMPLATE_CHARS);
        let facts = [("message", Val::Text(message_of(rec).to_string())), ("logger", Val::Text(rec.logger.clone().unwrap_or_default()))];
        if !matches_all(conds, rec, &facts) {
            continue;
        }
        groups.add(key.clone(), rec, || (key, rec.logger.iter().map(|l| format!("logger: {l}")).collect()));
    }
    groups.into_vec()
}

// ── The digest ───────────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct FacetQuery {
    pub facet: Option<Facet>,
    pub limit: usize,
    pub window: TimeWindow,
    pub conds: Vec<Cond>,
}

impl Default for FacetQuery {
    fn default() -> Self {
        Self { facet: None, limit: DEFAULT_LIMIT, window: TimeWindow::default(), conds: Vec::new() }
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct Digest {
    pub text: String,
    pub cut: Vec<String>,
}

fn fmt_ms(ms: f64) -> String {
    if ms >= 1000.0 {
        format!("{:.1} s", ms / 1000.0)
    } else if ms >= 10.0 {
        format!("{ms:.0} ms")
    } else {
        format!("{ms:.1} ms")
    }
}

fn span_of(agg: &Agg) -> String {
    match (agg.first_ts, agg.last_ts) {
        (Some(a), Some(b)) if a != b => format!("{} .. {}", &iso(a)[5..19], &iso(b)[5..19]),
        (Some(a), _) => iso(a)[5..19].to_string(),
        _ => "no timestamp".into(),
    }
}

fn status_summary(agg: &Agg) -> String {
    let mut s = agg.statuses.clone();
    s.sort_by_key(|(code, _)| std::cmp::Reverse(*code));
    s.iter().map(|(code, n)| format!("{code}:{n}")).collect::<Vec<_>>().join(" ")
}

fn print_group(out: &mut String, facet: Facet, g: &Agg) {
    match facet {
        Facet::Exceptions | Facet::Warnings => {
            out.push_str(&format!("  x{} {}  [{}]  sample #{}\n", g.count, g.label, span_of(g), g.sample_id));
        }
        Facet::Queries => {
            let times = if g.timed > 0 { format!("total {}, avg {}, max {}  ", fmt_ms(g.total_ms), fmt_ms(g.total_ms / g.timed as f64), fmt_ms(g.max_ms)) } else { String::new() };
            out.push_str(&format!("  x{} {times}[{}]  sample #{}\n      {}\n", g.count, span_of(g), g.sample_id, g.label));
            return;
        }
        Facet::Requests => {
            let times = if g.timed > 0 { format!("  avg {}, max {}", fmt_ms(g.total_ms / g.timed as f64), fmt_ms(g.max_ms)) } else { String::new() };
            out.push_str(&format!("  {}  x{}  {}{times}  [{}]  sample #{}\n", g.label, g.count, status_summary(g), span_of(g), g.sample_id));
            return;
        }
    }
    for d in &g.detail {
        out.push_str(&format!("      {d}\n"));
    }
}

fn sort_groups(facet: Facet, groups: &mut [Agg]) {
    match facet {
        Facet::Exceptions | Facet::Warnings => groups.sort_by(|a, b| b.count.cmp(&a.count).then(b.last_ts.cmp(&a.last_ts))),
        // Slowest first; statements that never reported a time go after, by how often they ran.
        Facet::Queries => groups.sort_by(|a, b| {
            let (ta, tb) = (a.timed > 0, b.timed > 0);
            tb.cmp(&ta).then(b.max_ms.total_cmp(&a.max_ms)).then(b.count.cmp(&a.count))
        }),
        // Endpoints that fail first, server errors before client errors.
        Facet::Requests => groups.sort_by(|a, b| {
            b.server_errors.cmp(&a.server_errors).then(b.client_errors.cmp(&a.client_errors)).then(b.count.cmp(&a.count))
        }),
    }
}

fn level_counts(records: &[&Record]) -> String {
    let n = |l: Level| records.iter().filter(|r| r.level == Some(l)).count();
    format!(
        "ERROR {}, WARN {}, INFO {}, DEBUG {}, TRACE {}, no level {}",
        n(Level::Error),
        n(Level::Warn),
        n(Level::Info),
        n(Level::Debug),
        n(Level::Trace),
        records.iter().filter(|r| r.level.is_none()).count()
    )
}

/// The digest of `records`. `resolve` maps a class name to a project file (relative path), or `None`.
pub fn digest(records: &[Record], q: &FacetQuery, resolve: &dyn Fn(&str) -> Option<String>) -> Result<Digest, String> {
    if !q.conds.is_empty() {
        let facet = q.facet.ok_or("`where` needs a `facet`: it filters the occurrences of one facet")?;
        check_fields(facet, &q.conds)?;
    }
    let mut cut = Vec::new();
    let in_window: Vec<&Record> = records.iter().filter(|r| q.window.is_open() || r.ts_ms.is_some_and(|t| q.window.contains(t))).collect();
    if !q.window.is_open() {
        let undated = records.iter().filter(|r| r.ts_ms.is_none()).count();
        if undated > 0 {
            cut.push(format!("{undated} record(s) carry no timestamp and are left out by the time filter."));
        }
    }
    let limit = q.limit.clamp(1, MAX_LIMIT);
    let wanted: Vec<Facet> = q.facet.map(|f| vec![f]).unwrap_or_else(|| Facet::ALL.to_vec());
    let mut text = format!("{} record(s), {}.\nlevels: {}\n", in_window.len(), q.window.describe(), level_counts(&in_window));
    let mut any = false;
    for facet in wanted {
        let mut groups = match facet {
            Facet::Exceptions => exception_groups(&in_window, &q.conds, resolve),
            Facet::Queries => query_groups(&in_window, &q.conds),
            Facet::Requests => request_groups(&in_window, &q.conds),
            Facet::Warnings => warning_groups(&in_window, &q.conds),
        };
        if groups.is_empty() {
            if q.facet.is_some() {
                text.push_str(&format!("{}: none found{}.\n", facet.title(), if q.conds.is_empty() { "" } else { " matching `where`" }));
            }
            continue;
        }
        any = true;
        sort_groups(facet, &mut groups);
        let occurrences: usize = groups.iter().map(|g| g.count).sum();
        let order = match facet {
            Facet::Queries => ", slowest first",
            Facet::Requests => ", failing endpoints first",
            _ => ", most frequent first",
        };
        text.push_str(&format!("{} — {} group(s), {} occurrence(s){order}\n", facet.title(), groups.len(), occurrences));
        for g in groups.iter().take(limit) {
            print_group(&mut text, facet, g);
        }
        if groups.len() > limit {
            cut.push(format!("{}: {} more group(s) not listed — raise `limit` (up to {MAX_LIMIT}) or filter with `where` / `after` / `last`.", facet.title(), groups.len() - limit));
        }
    }
    if !any && q.facet.is_none() {
        text.push_str("nothing recognisable as an exception, query, request or warning in this window; read lines with run_logs / console_logs instead.\n");
    }
    cut.push(format!("messages are cut at {TEMPLATE_CHARS} characters and statements at {SQL_CHARS}; read one record whole with log_record {{source, id}}."));
    Ok(Digest { text, cut })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logbuf::Stream;
    use crate::logrecord::{build_records, RawLine};
    use arbor_logscan::prelude::RuleSet;

    fn records(texts: &[&str]) -> Vec<Record> {
        let lines: Vec<RawLine> = texts
            .iter()
            .enumerate()
            .map(|(i, t)| RawLine { seq: i as u64 + 1, ts_ms: Some(1_785_933_181_000 + 1000 * i as u64), stream: Stream::Stdout, text: t.to_string() })
            .collect();
        build_records(&RuleSet::java(), &lines)
    }

    fn project(class: &str) -> Option<String> {
        class.starts_with("com.acme.").then(|| format!("src/main/java/{}.java", class.replace('.', "/")))
    }

    const SPRING: &[&str] = &[
        "2026-08-05 12:33:01.100 DEBUG 77 --- [nio-8080-exec-1] org.hibernate.SQL : select o1_0.id from orders o1_0 where o1_0.id=17",
        "2026-08-05 12:33:01.200 DEBUG 77 --- [nio-8080-exec-1] org.hibernate.SQL : select o1_0.id from orders o1_0 where o1_0.id=18",
        "2026-08-05 12:33:01.300 ERROR 77 --- [nio-8080-exec-1] c.a.web.OrderController : Request failed",
        "org.springframework.web.util.NestedServletException: Request processing failed; nested exception is java.lang.IllegalStateException: no total",
        "\tat org.springframework.web.servlet.FrameworkServlet.processRequest(FrameworkServlet.java:1014)",
        "\tat com.acme.web.OrderController.show(OrderController.java:58)",
        "Caused by: java.lang.IllegalStateException: no total for order 17",
        "\tat java.base/java.util.Objects.requireNonNull(Objects.java:233)",
        "\tat com.acme.service.OrderService.total(OrderService.java:42)",
        "\t... 31 common frames omitted",
        "2026-08-05 12:33:02.000 ERROR 77 --- [nio-8080-exec-2] c.a.web.OrderController : Request failed",
        "org.springframework.web.util.NestedServletException: Request processing failed; nested exception is java.lang.IllegalStateException: no total",
        "Caused by: java.lang.IllegalStateException: no total for order 18",
        "\tat com.acme.service.OrderService.total(OrderService.java:42)",
        "2026-08-05 12:33:03.000 WARN 77 --- [main] c.z.h.p.PoolBase : Pool 3 low: 2 idle",
        "2026-08-05 12:33:04.000 WARN 77 --- [main] c.z.h.p.PoolBase : Pool 4 low: 1 idle",
    ];

    #[test]
    fn exceptions_group_by_class_root_cause_and_project_frame() {
        let r = records(SPRING);
        let refs: Vec<&Record> = r.iter().collect();
        let groups = exception_groups(&refs, &[], &project);
        assert_eq!(groups.len(), 1, "{groups:#?}");
        let g = &groups[0];
        assert_eq!(g.count, 2);
        assert!(g.label.contains("NestedServletException") && g.label.contains("root cause java.lang.IllegalStateException"), "{}", g.label);
        assert!(g.detail.iter().any(|d| d.contains("src/main/java/com/acme/service/OrderService.java:42 (OrderService.total)")), "{:?}", g.detail);
        assert!(g.detail[0].contains("no total for order #"));
        assert_eq!(g.sample_id, r[2].id);
    }

    #[test]
    fn queries_collapse_to_one_statement_with_a_count() {
        let r = records(SPRING);
        let refs: Vec<&Record> = r.iter().collect();
        let groups = query_groups(&refs, &[]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].count, 2);
        assert!(groups[0].label.ends_with("where o1_0.id=?"));
    }

    #[test]
    fn warnings_group_by_template() {
        let r = records(SPRING);
        let refs: Vec<&Record> = r.iter().collect();
        let groups = warning_groups(&refs, &[]);
        assert_eq!(groups.len(), 1);
        assert_eq!((groups[0].count, groups[0].label.as_str()), (2, "Pool # low: # idle"));
    }

    #[test]
    fn a_rust_panic_is_an_exception_pointing_at_the_user_code() {
        let r = records(&[
            "thread 'main' panicked at src/main.rs:12:5:",
            "called `Option::unwrap()` on a `None` value",
            "stack backtrace:",
            "   0: rust_begin_unwind",
            "             at /rustc/abc/library/std/src/panicking.rs:645:5",
            "   5: app::handler",
            "             at ./src/handler.rs:44:9",
        ]);
        let f = exception_of(&r[0], &|_| None).unwrap();
        assert_eq!(f.class, "panic");
        assert_eq!(f.frame.as_deref(), Some("src/handler.rs:44"));
        assert!(f.message.contains("unwrap()"));
    }

    #[test]
    fn an_axum_access_log_groups_by_endpoint_and_puts_failures_first() {
        let r = records(&[
            "2026-08-05T12:33:01.1Z  INFO request{method=GET uri=/api/orders/1 version=HTTP/1.1}: tower_http::trace::on_response: finished processing request latency=3 ms status=200",
            "2026-08-05T12:33:01.2Z  INFO request{method=GET uri=/api/orders/2 version=HTTP/1.1}: tower_http::trace::on_response: finished processing request latency=9 ms status=200",
            "2026-08-05T12:33:01.3Z  INFO request{method=POST uri=/api/pay version=HTTP/1.1}: tower_http::trace::on_response: finished processing request latency=410 ms status=500",
            "2026-08-05T12:33:01.4Z  INFO request{method=GET uri=/api/orders/3 version=HTTP/1.1}: tower_http::trace::on_response: finished processing request latency=5 ms status=404",
        ]);
        let d = digest(&r, &FacetQuery { facet: Some(Facet::Requests), ..Default::default() }, &|_| None).unwrap();
        let first = d.text.lines().find(|l| l.contains("/api/")).unwrap();
        assert!(first.contains("POST /api/pay") && first.contains("500:1"), "{}", d.text);
        assert!(d.text.contains("GET /api/orders/{id}  x3") && d.text.contains("404:1 200:2"), "{}", d.text);
    }

    #[test]
    fn where_filters_one_facet_by_field() {
        let r = records(&[
            "127.0.0.1 - - [05/Aug/2026:12:33:01 +0000] \"GET /a HTTP/1.1\" 200 2 5 ms",
            "127.0.0.1 - - [05/Aug/2026:12:33:02 +0000] \"GET /b HTTP/1.1\" 503 2 700 ms",
            "127.0.0.1 - - [05/Aug/2026:12:33:03 +0000] \"GET /c HTTP/1.1\" 200 2 900 ms",
        ]);
        let q = |w: &str| FacetQuery { facet: Some(Facet::Requests), conds: parse_where(w).unwrap(), ..Default::default() };
        let d = digest(&r, &q("status >= 500"), &|_| None).unwrap();
        assert!(d.text.contains("GET /b") && !d.text.contains("GET /a") && !d.text.contains("GET /c"));
        let d = digest(&r, &q("duration > 0.5s and url contains /c"), &|_| None).unwrap();
        assert!(d.text.contains("GET /c") && !d.text.contains("GET /b"));
        assert!(digest(&r, &q("class contains x"), &|_| None).is_err());
        assert!(digest(&r, &FacetQuery { conds: parse_where("status > 1").unwrap(), ..Default::default() }, &|_| None).is_err());
        assert!(parse_where("nonsense").is_err());
    }

    #[test]
    fn the_overview_lists_every_facet_that_has_something_and_states_its_cuts() {
        let r = records(SPRING);
        let d = digest(&r, &FacetQuery { limit: 1, ..Default::default() }, &project).unwrap();
        assert!(d.text.contains("levels: ERROR 2, WARN 2, INFO 0, DEBUG 2"), "{}", d.text);
        for title in ["EXCEPTIONS", "QUERIES", "WARNINGS"] {
            assert!(d.text.contains(title), "{title}: {}", d.text);
        }
        assert!(!d.text.contains("REQUESTS"));
        assert!(d.cut.iter().any(|c| c.contains("log_record")));
    }

    #[test]
    fn slow_queries_come_first() {
        let r = records(&[
            "1785933181000|3|statement|connection 1|select a from t where id=1|select a from t where id=1",
            "1785933182000|900|statement|connection 1|select b from big|select b from big",
            "1785933183000|4|statement|connection 1|select a from t where id=2|select a from t where id=2",
        ]);
        let d = digest(&r, &FacetQuery { facet: Some(Facet::Queries), ..Default::default() }, &|_| None).unwrap();
        let first = d.text.lines().find(|l| l.contains("select")).unwrap();
        assert!(first.contains("select b from big"), "{}", d.text);
        assert!(d.text.contains("x2 total 7.0 ms, avg 3.5 ms, max 4.0 ms"), "{}", d.text);
    }

    #[test]
    fn groups_past_the_limit_are_announced() {
        let texts: Vec<String> = (0..5).map(|i| format!("2026-08-05 12:33:0{i} WARN distinct warning kind{}", ["a", "b", "c", "d", "e"][i])).collect();
        let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
        let r = records(&refs);
        let d = digest(&r, &FacetQuery { facet: Some(Facet::Warnings), limit: 2, ..Default::default() }, &|_| None).unwrap();
        assert!(d.cut.iter().any(|c| c.contains("3 more group(s)")), "{:?}", d.cut);
    }
}
