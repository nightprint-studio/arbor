//! Digests: the answer a big tool result deserves, with the rest one call away.
//!
//! A build with sixty errors, a test run with forty failures, a search with four hundred hits: the model
//! needs the shape first — how many, which kinds, which files — and the detail only for the part it is
//! about to act on. Reading all of it costs tokens on this request and, because the conversation is
//! re-sent, on every request after it.
//!
//! Each formatter here turns one tool's result into a [`Digest`]: a short `head` (counts, the distinct
//! problems grouped with how often they occur, the files most affected, and a plain statement of what was
//! left out) and the `full` text that backs it, which the host keeps under an id so the model can ask for
//! any part of it (`expand_result`). Nothing is lost, and nothing is cut silently.
//!
//! Pure functions over JSON (the tools' own result shapes) or text; they know nothing of the files or the
//! network. A result that is already small is not worth a digest: [`MIN_CHARS`] is the host's threshold.

use std::collections::BTreeMap;

use serde_json::Value;

/// A result shorter than this is returned whole: a digest of a small thing is just a smaller thing.
pub const MIN_CHARS: usize = 6_000;

/// How many distinct problems of each severity the head names.
const ERROR_GROUPS: usize = 10;
const WARNING_GROUPS: usize = 4;
const FILES_NAMED: usize = 6;
const FAILURES_NAMED: usize = 10;
const SEARCH_FILES: usize = 12;
const SEARCH_PER_FILE: usize = 2;
const LINE_CHARS: usize = 160;

/// A big result, summarised.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Digest {
    /// What the model reads now.
    pub head: String,
    /// Everything, one item per line, for the model to filter and page through.
    pub full: String,
}

/// The digest of a tool's JSON result, for the tools that have a formatter; `None` for the others (their
/// result goes through as it is).
pub fn digest_value(tool: &str, value: &Value) -> Option<Digest> {
    match tool {
        "bennu_problems" | "bennu_check_file" => problems(value),
        "bennu_build" => build(value),
        "bennu_test_run" => tests(value),
        "bennu_references" => references(value),
        _ => None,
    }
}

/// The digest of a tool's text result (`search_text`).
pub fn digest_text(tool: &str, text: &str) -> Option<Digest> {
    (tool == "search_text").then(|| search(text)).flatten()
}

// ── shared pieces ─────────────────────────────────────────────────────────────────────────────────

fn clip(text: &str, max: usize) -> String {
    let one_line = text.lines().next().unwrap_or("").trim();
    if one_line.chars().count() <= max { one_line.to_string() } else { format!("{}…", one_line.chars().take(max).collect::<String>()) }
}

fn str_of<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

fn num_of(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// One reported problem, whatever tool reported it.
#[derive(Debug, Clone)]
struct Item {
    severity: String,
    file: String,
    line: u64,
    message: String,
}

impl Item {
    fn at(&self) -> String {
        if self.line > 0 { format!("{}:{}", self.file, self.line) } else { self.file.clone() }
    }
    fn full_line(&self) -> String {
        format!("{} {} {}", self.severity, self.at(), self.message.trim().replace('\n', " ⏎ "))
    }
}

/// The distinct messages of one severity, most frequent first (ties keep the order they were reported in).
fn groups<'a>(items: &'a [Item], severity: &str) -> Vec<(&'a str, Vec<&'a Item>)> {
    let mut order: Vec<&str> = Vec::new();
    let mut by_message: BTreeMap<&str, Vec<&Item>> = BTreeMap::new();
    for i in items.iter().filter(|i| i.severity == severity) {
        let key = i.message.lines().next().unwrap_or("").trim();
        if !by_message.contains_key(key) {
            order.push(key);
        }
        by_message.entry(key).or_default().push(i);
    }
    let mut out: Vec<(&str, Vec<&Item>)> = order.into_iter().map(|k| (k, by_message.remove(k).unwrap_or_default())).collect();
    out.sort_by_key(|(_, v)| std::cmp::Reverse(v.len()));
    out
}

fn files_of(items: &[&Item]) -> usize {
    let mut f: Vec<&str> = items.iter().map(|i| i.file.as_str()).collect();
    f.sort_unstable();
    f.dedup();
    f.len()
}

/// The head for a list of problems: grouped, with what was left out said.
fn render_items(counts: String, items: &[Item], note: &str) -> Digest {
    let mut head = counts;
    for (severity, shown) in [("error", ERROR_GROUPS), ("warning", WARNING_GROUPS)] {
        let g = groups(items, severity);
        for (message, at) in g.iter().take(shown) {
            let first = at[0];
            if at.len() == 1 {
                head.push_str(&format!("\n- {severity} {}: {}", first.at(), clip(message, LINE_CHARS)));
            } else {
                head.push_str(&format!("\n- {severity} ×{} in {}, first {}: {}", at.len(), plural(files_of(at), "file", "files"), first.at(), clip(message, LINE_CHARS)));
            }
        }
        if g.len() > shown {
            let rest: usize = g.iter().skip(shown).map(|(_, v)| v.len()).sum();
            head.push_str(&format!("\n- … {} more distinct {severity} messages ({rest} occurrences) not listed", g.len() - shown));
        }
    }
    let mut per_file: BTreeMap<&str, usize> = BTreeMap::new();
    for i in items {
        *per_file.entry(i.file.as_str()).or_default() += 1;
    }
    let mut files: Vec<(&str, usize)> = per_file.into_iter().collect();
    files.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    if files.len() > 1 {
        let named: Vec<String> = files.iter().take(FILES_NAMED).map(|(f, n)| format!("{f} ({n})")).collect();
        let more = if files.len() > FILES_NAMED { format!(", … {} more files", files.len() - FILES_NAMED) } else { String::new() };
        head.push_str(&format!("\nMost affected: {}{more}", named.join(", ")));
    }
    if !note.is_empty() {
        head.push_str(&format!("\n{note}"));
    }
    Digest { head, full: items.iter().map(Item::full_line).collect::<Vec<_>>().join("\n") }
}

// ── problems and build output ─────────────────────────────────────────────────────────────────────

fn problems(v: &Value) -> Option<Digest> {
    let files = v.get("files")?.as_array()?;
    let mut items = Vec::new();
    for f in files {
        let file = str_of(f, "file");
        for p in f.get("problems").and_then(Value::as_array).into_iter().flatten() {
            items.push(Item { severity: str_of(p, "severity").to_string(), file: file.to_string(), line: num_of(p, "line"), message: str_of(p, "message").to_string() });
        }
    }
    let counts = format!("{} errors, {} warnings across {} (of {} reported).", num_of(v, "errors"), num_of(v, "warnings"), plural(files.len(), "file", "files"), num_of(v, "total"));
    Some(render_items(counts, &items, v.get("note").and_then(Value::as_str).unwrap_or("")))
}

fn build(v: &Value) -> Option<Digest> {
    let list = v.get("problems")?.as_array()?;
    let items: Vec<Item> = list
        .iter()
        .map(|p| Item { severity: str_of(p, "severity").to_string(), file: if str_of(p, "file").is_empty() { "(no file)".into() } else { str_of(p, "file").to_string() }, line: num_of(p, "line"), message: str_of(p, "message").to_string() })
        .collect();
    let ok = v.get("ok").and_then(Value::as_bool).unwrap_or(false);
    let cut = num_of(v, "truncated");
    let counts = format!("{} ({}): {} errors, {} warnings.", if ok { "Build passed" } else { "Build failed" }, str_of(v, "tool"), num_of(v, "errors"), num_of(v, "warnings"));
    let mut note = String::new();
    for key in ["note", "attribution"] {
        if let Some(t) = v.get(key).and_then(Value::as_str).filter(|t| !t.is_empty()) {
            note.push_str(&format!("{t}\n"));
        }
    }
    if cut > 0 {
        note.push_str(&format!("The tool itself left out {cut} further problems."));
    }
    Some(render_items(counts, &items, note.trim_end()))
}

// ── tests ─────────────────────────────────────────────────────────────────────────────────────────

fn tests(v: &Value) -> Option<Digest> {
    let failures = v.get("failures")?.as_array()?;
    let mut head = format!(
        "{}: {} passed, {} failed, {} skipped{}.",
        str_of(v, "label"),
        num_of(v, "passed"),
        num_of(v, "failed"),
        num_of(v, "skipped"),
        if v.get("cancelled").and_then(Value::as_bool).unwrap_or(false) { " (stopped before the end)" } else { "" }
    );
    for f in failures.iter().take(FAILURES_NAMED) {
        let kind = f.get("kind").and_then(Value::as_str).map(|k| format!("{k}: ")).unwrap_or_default();
        head.push_str(&format!("\n- FAILED {} — {kind}{}", str_of(f, "test"), clip(str_of(f, "message"), LINE_CHARS)));
    }
    if failures.len() > FAILURES_NAMED {
        let names: Vec<&str> = failures.iter().skip(FAILURES_NAMED).take(15).map(|f| str_of(f, "test")).collect();
        let more = if failures.len() - FAILURES_NAMED > names.len() { ", …" } else { "" };
        head.push_str(&format!("\n- … {} more failing: {}{more}", failures.len() - FAILURES_NAMED, names.join(", ")));
    }
    if let Some(note) = v.get("note").and_then(Value::as_str).filter(|n| !n.is_empty()) {
        head.push_str(&format!("\n{note}"));
    }
    let full = failures.iter().map(|f| format!("FAILED {}\n{}", str_of(f, "test"), str_of(f, "message"))).collect::<Vec<_>>().join("\n\n");
    Some(Digest { head, full })
}

// ── usages ────────────────────────────────────────────────────────────────────────────────────────

fn references(v: &Value) -> Option<Digest> {
    let files = v.get("files")?.as_array()?;
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    let mut full = Vec::new();
    for f in files {
        for u in f.get("usages").and_then(Value::as_array).into_iter().flatten() {
            *kinds.entry(str_of(u, "kind").to_string()).or_default() += 1;
            full.push(format!("{}:{} {} {}", str_of(f, "file"), num_of(u, "line"), str_of(u, "kind"), clip(str_of(u, "preview"), LINE_CHARS)));
        }
    }
    let by_kind: Vec<String> = kinds.iter().map(|(k, n)| format!("{n} {k}")).collect();
    let mut head = format!("{}: {} sites in {} ({}).", str_of(v, "target"), num_of(v, "total"), plural(files.len(), "file", "files"), by_kind.join(", "));
    for f in files.iter().take(SEARCH_FILES) {
        let sites: Vec<String> = f
            .get("usages")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .take(SEARCH_PER_FILE)
            .map(|u| format!("{} {}: {}", num_of(u, "line"), str_of(u, "kind"), clip(str_of(u, "preview"), 100)))
            .collect();
        head.push_str(&format!("\n- {} ({}): {}", str_of(f, "file"), num_of(f, "count"), sites.join(" | ")));
    }
    if files.len() > SEARCH_FILES {
        let rest: u64 = files.iter().skip(SEARCH_FILES).map(|f| num_of(f, "count")).sum();
        head.push_str(&format!("\n- … {} more files ({rest} sites) not listed", files.len() - SEARCH_FILES));
    }
    if let Some(note) = v.get("note").and_then(Value::as_str).filter(|n| !n.is_empty()) {
        head.push_str(&format!("\n{note}"));
    }
    Some(Digest { head, full: full.join("\n") })
}

// ── text search ───────────────────────────────────────────────────────────────────────────────────

/// `file:LINE: text` split into its three parts; context lines (`file-LINE- text`) are not matches.
fn match_line(line: &str) -> Option<(&str, u64, &str)> {
    for (at, _) in line.match_indices(':') {
        let rest = &line[at + 1..];
        let digits = rest.chars().take_while(char::is_ascii_digit).count();
        if digits > 0 && rest[digits..].starts_with(": ") {
            return Some((&line[..at], rest[..digits].parse().ok()?, &rest[digits + 2..]));
        }
    }
    None
}

fn search(text: &str) -> Option<Digest> {
    let mut per_file: Vec<(&str, Vec<(u64, &str)>)> = Vec::new();
    let mut tail: Vec<&str> = Vec::new();
    for line in text.lines() {
        match match_line(line) {
            Some((file, n, body)) => match per_file.last_mut() {
                Some((f, v)) if *f == file => v.push((n, body)),
                _ => per_file.push((file, vec![(n, body)])),
            },
            None if line.starts_with('…') || line.contains(" matches in ") => tail.push(line),
            None => {}
        }
    }
    if per_file.is_empty() {
        return None;
    }
    let total: usize = per_file.iter().map(|(_, v)| v.len()).sum();
    let mut head = format!("{total} matching lines in {}.", plural(per_file.len(), "file", "files"));
    for (file, hits) in per_file.iter().take(SEARCH_FILES) {
        let shown: Vec<String> = hits.iter().take(SEARCH_PER_FILE).map(|(n, b)| format!("{n}: {}", clip(b.trim(), 100))).collect();
        let more = if hits.len() > SEARCH_PER_FILE { format!(" (+{} more)", hits.len() - SEARCH_PER_FILE) } else { String::new() };
        head.push_str(&format!("\n- {file} [{}]: {}{more}", hits.len(), shown.join(" | ")));
    }
    if per_file.len() > SEARCH_FILES {
        let rest: usize = per_file.iter().skip(SEARCH_FILES).map(|(_, v)| v.len()).sum();
        head.push_str(&format!("\n- … {} more files ({rest} lines) not listed", per_file.len() - SEARCH_FILES));
    }
    for t in tail {
        head.push_str(&format!("\n{t}"));
    }
    Some(Digest { head, full: text.to_string() })
}

// ── drill-down ────────────────────────────────────────────────────────────────────────────────────

/// The part of a digest's full text asked for: the lines containing `needle` (case-insensitive; every line
/// when empty), from the `offset`-th, at most `limit`. Says how many there are and how to get the next page.
pub fn expand(full: &str, needle: &str, offset: usize, limit: usize) -> String {
    let needle = needle.trim().to_lowercase();
    let lines: Vec<&str> = full.lines().filter(|l| needle.is_empty() || l.to_lowercase().contains(&needle)).collect();
    if lines.is_empty() {
        return if needle.is_empty() { "The result is empty.".into() } else { format!("No line contains `{needle}`.") };
    }
    let limit = limit.clamp(1, 400);
    let end = (offset + limit).min(lines.len());
    if offset >= lines.len() {
        return format!("Only {} lines match; `offset` {offset} is past the end.", lines.len());
    }
    let mut out = lines[offset..end].join("\n");
    out.push_str(&format!("\n[lines {}–{} of {}{}]", offset + 1, end, lines.len(), if end < lines.len() { format!("; next page: offset {end}") } else { String::new() }));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn problem(sev: &str, line: u32, msg: &str) -> Value {
        json!({"severity": sev, "message": msg, "line": line, "column": 1})
    }

    #[test]
    fn many_problems_become_distinct_messages_with_their_counts() {
        let files: Vec<Value> = (0..30)
            .map(|i| json!({"file": format!("src/m{i}.rs"), "count": 2, "problems": [problem("error", 10, "cannot find value `x` in this scope"), problem("warning", 3, "unused import")]}))
            .collect();
        let v = json!({"files": files, "errors": 30, "warnings": 30, "total": 60, "note": null});
        let d = digest_value("bennu_problems", &v).unwrap();
        assert!(d.head.starts_with("30 errors, 30 warnings across 30 files (of 60 reported)."));
        assert!(d.head.contains("error ×30 in 30 files, first src/m0.rs:10: cannot find value `x` in this scope"), "{}", d.head);
        assert!(d.head.contains("warning ×30") && d.head.contains("Most affected:") && d.head.contains("24 more files"));
        assert_eq!(d.full.lines().count(), 60, "everything is kept for the drill-down");
        assert!(d.head.len() < d.full.len() / 3, "{} vs {}", d.head.len(), d.full.len());
    }

    #[test]
    fn what_is_cut_is_always_said() {
        let items: Vec<Value> = (0..25).map(|i| json!({"severity": "error", "file": "a.java", "line": i, "message": format!("problem number {i}")})).collect();
        let v = json!({"tool": "mvn", "ok": false, "errors": 25, "warnings": 0, "problems": items, "truncated": 7});
        let d = digest_value("bennu_build", &v).unwrap();
        assert!(d.head.starts_with("Build failed (mvn): 25 errors"));
        assert!(d.head.contains("15 more distinct error messages (15 occurrences) not listed"), "{}", d.head);
        assert!(d.head.contains("left out 7 further problems"));
    }

    #[test]
    fn a_test_run_names_the_failing_tests_and_the_first_line_of_why() {
        let failures: Vec<Value> = (0..14).map(|i| json!({"test": format!("orders::t{i}"), "kind": null, "message": format!("assertion failed: left == right\nleft: {i}\nright: 0")})).collect();
        let v = json!({"label": "cargo test", "passed": 100, "failed": 14, "skipped": 2, "failures": failures, "cancelled": false, "note": null});
        let d = digest_value("bennu_test_run", &v).unwrap();
        assert!(d.head.starts_with("cargo test: 100 passed, 14 failed, 2 skipped."));
        assert!(d.head.contains("FAILED orders::t0 — assertion failed: left == right") && !d.head.contains("left: 0"), "only the first line of the message");
        assert!(d.head.contains("4 more failing: orders::t10, orders::t11, orders::t12, orders::t13"));
        assert!(d.full.contains("left: 3"), "the whole message is one expand away");
    }

    #[test]
    fn usages_are_counted_by_kind_and_listed_per_file() {
        let usage = |l: u32, k: &str| json!({"line": l, "column": 1, "preview": "foo.bar();", "kind": k});
        let files: Vec<Value> = (0..20).map(|i| json!({"file": format!("F{i}.java"), "count": 3, "usages": [usage(1, "call"), usage(2, "call"), usage(3, "import")]})).collect();
        let v = json!({"target": "method Foo.bar()", "files": files, "total": 60});
        let d = digest_value("bennu_references", &v).unwrap();
        assert!(d.head.starts_with("method Foo.bar(): 60 sites in 20 files (40 call, 20 import)."), "{}", d.head);
        assert!(d.head.contains("8 more files (24 sites) not listed"));
        assert_eq!(d.full.lines().count(), 60);
    }

    #[test]
    fn a_search_is_grouped_by_file_with_a_cap_per_file() {
        let mut text = String::new();
        for f in 0..20 {
            for n in 1..=5 {
                text.push_str(&format!("src/f{f}.rs:{}: let value = compute({n});\n", n * 10));
            }
        }
        text.push_str("100 matches in 20 files.");
        let d = digest_text("search_text", &text).unwrap();
        assert!(d.head.starts_with("100 matching lines in 20 files."));
        assert!(d.head.contains("src/f0.rs [5]: 10: let value") && d.head.contains("(+3 more)"));
        assert!(d.head.contains("8 more files (40 lines) not listed") && d.head.ends_with("100 matches in 20 files."));
        assert_eq!(d.full, text);
        assert!(digest_text("find_files", &text).is_none() && digest_text("search_text", "No matches in 3 files.").is_none());
    }

    #[test]
    fn windows_paths_and_context_lines_do_not_confuse_the_search_parser() {
        assert_eq!(match_line("C:/work/a.rs:12: fn main()"), Some(("C:/work/a.rs", 12, "fn main()")));
        assert_eq!(match_line("src/a.rs-12- context"), None);
        assert_eq!(match_line("a.rs:x: no"), None);
    }

    #[test]
    fn expanding_filters_and_pages() {
        let full = (1..=10).map(|i| format!("error a.rs:{i} boom {}", if i % 2 == 0 { "even" } else { "odd" })).collect::<Vec<_>>().join("\n");
        let page = expand(&full, "EVEN", 0, 3);
        assert_eq!(page.lines().count(), 4);
        assert!(page.ends_with("[lines 1–3 of 5; next page: offset 3]"), "{page}");
        assert!(expand(&full, "even", 3, 3).ends_with("[lines 4–5 of 5]"));
        assert_eq!(expand(&full, "nothing", 0, 3), "No line contains `nothing`.");
        assert!(expand(&full, "", 50, 3).contains("past the end"));
    }

    #[test]
    fn a_tool_without_a_formatter_gets_none() {
        assert!(digest_value("bennu_read_file", &json!({"text": "x"})).is_none());
        assert!(digest_value("bennu_problems", &json!({"nope": 1})).is_none());
    }
}
