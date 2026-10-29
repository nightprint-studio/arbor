//! Reading records the way a person skims a log: filter by time, level and pattern, show the neighbours
//! of each match, fold repeats, keep stack traces attached but short — and say, every time, what was left
//! out and how to ask for it.
//!
//! Pure over [`Record`]s, so a process buffer, a run console and a file on disk are read by the same code.
//! The caller supplies how a record is introduced (`[+3 s #42] `) because that depends on the source.

use std::collections::HashMap;
use std::sync::LazyLock;

use arbor_logscan::prelude::Level;
use regex::Regex;

use crate::logrecord::{fold, level_name, Record};
use crate::logtime::{iso, TimeWindow};

/// What a read asks for.
#[derive(Debug, Clone)]
pub struct ReadOptions {
    /// The newest N matching records.
    pub tail: usize,
    pub pattern: Option<Regex>,
    /// At least this severity.
    pub level: Option<Level>,
    pub window: TimeWindow,
    /// Records shown before and after each match.
    pub context: usize,
    /// Continuation lines shown per record.
    pub frames: usize,
    /// Fold identical consecutive records into one with a count.
    pub fold: bool,
}

impl Default for ReadOptions {
    fn default() -> Self {
        Self { tail: 80, pattern: None, level: None, window: TimeWindow::default(), context: 0, frames: 12, fold: true }
    }
}

pub const MAX_TAIL: usize = 400;
pub const MAX_CONTEXT: usize = 10;
pub const MAX_FRAMES: usize = 200;

/// What a read produced.
#[derive(Debug, Default, PartialEq)]
pub struct Rendered {
    pub text: String,
    /// Records that matched the filters, before `tail` cut them.
    pub matched: usize,
    /// Records printed (matches and context, repeats counted once).
    pub shown: usize,
    /// One sentence per thing left out, each saying how to get it.
    pub cut: Vec<String>,
    /// The id of the newest record printed, 0 if none.
    pub newest_id: u64,
}

fn passes(rec: &Record, o: &ReadOptions) -> bool {
    if !o.window.is_open() && !rec.ts_ms.is_some_and(|t| o.window.contains(t)) {
        return false;
    }
    if let Some(min) = o.level {
        if !rec.level.is_some_and(|l| fold(l) >= min) {
            return false;
        }
    }
    o.pattern.as_ref().is_none_or(|re| re.is_match(&rec.head) || rec.rest.iter().any(|l| re.is_match(l)))
}

/// Which records to print: `(index, is_match)` in order, `None` marking a gap between non-adjacent runs.
fn select(records: &[Record], o: &ReadOptions) -> (Vec<Option<(usize, bool)>>, usize) {
    let matches: Vec<usize> = (0..records.len()).filter(|&i| passes(&records[i], o)).collect();
    let matched = matches.len();
    let kept = &matches[matches.len().saturating_sub(o.tail.clamp(1, MAX_TAIL))..];
    let ctx = o.context.min(MAX_CONTEXT);
    let mut rows: Vec<Option<(usize, bool)>> = Vec::new();
    let mut next_free = 0usize; // first index not yet emitted
    for &m in kept {
        let want_from = m.saturating_sub(ctx);
        if !rows.is_empty() && want_from > next_free {
            rows.push(None);
        }
        let to = (m + ctx).min(records.len().saturating_sub(1));
        for i in want_from.max(next_free)..=to {
            rows.push(Some((i, kept.binary_search(&i).is_ok())));
        }
        next_free = to + 1;
    }
    (rows, matched)
}

/// What makes two records "the same line again": the message and the lines under it, not the clock.
fn fingerprint(r: &Record) -> (Option<Level>, &str, &[String]) {
    (r.level, if r.message.is_empty() { r.head.as_str() } else { r.message.as_str() }, r.rest.as_slice())
}

/// Print `records` per `o`. `prefix` introduces each record (time, id, stream...).
pub fn render(records: &[Record], o: &ReadOptions, prefix: &dyn Fn(&Record) -> String) -> Rendered {
    let (rows, matched) = select(records, o);
    let mut out = Rendered { matched, ..Default::default() };
    let mut hidden_lines = 0usize;
    let mut hidden_records = 0usize;
    let frames = o.frames.min(MAX_FRAMES);
    let mut i = 0;
    while i < rows.len() {
        let Some((idx, _)) = rows[i] else {
            out.text.push_str("--\n");
            i += 1;
            continue;
        };
        let rec = &records[idx];
        // Fold the identical records that follow directly (same run of rows, no gap between).
        let mut repeats = 1;
        let mut last = idx;
        while let Some(Some((n, _))) = rows.get(i + repeats).filter(|_| o.fold) {
            if fingerprint(&records[*n]) != fingerprint(rec) {
                break;
            }
            last = *n;
            repeats += 1;
        }
        out.text.push_str(&prefix(rec));
        out.text.push_str(&rec.head);
        if repeats > 1 {
            let until = records[last].ts_ms.map(|t| format!(", last at {}", iso(t))).unwrap_or_default();
            out.text.push_str(&format!("  [x{repeats}{until}]"));
        }
        out.text.push('\n');
        let shown_rest = rec.rest.len().min(frames);
        for l in &rec.rest[..shown_rest] {
            out.text.push_str("    ");
            out.text.push_str(l);
            out.text.push('\n');
        }
        let hidden = rec.lines() - 1 - shown_rest;
        if hidden > 0 {
            // The root cause is the line people read first: keep the last `Caused by:` even when frames cut the middle.
            if let Some(cause) = rec.rest[shown_rest..].iter().rev().find(|l| l.trim_start().starts_with("Caused by")) {
                out.text.push_str(&format!("    … {hidden} more line(s), last cause: {}\n", cause.trim()));
            } else {
                out.text.push_str(&format!("    … {hidden} more line(s) of this record\n"));
            }
            hidden_lines += hidden;
            hidden_records += 1;
        }
        out.shown += repeats;
        out.newest_id = out.newest_id.max(records[last].id);
        i += repeats;
    }
    if matched > o.tail.clamp(1, MAX_TAIL) {
        out.cut.push(format!("{} older matching record(s) are not shown: raise `tail` (up to {MAX_TAIL}), or narrow with `pattern`, `level`, `after`/`last`.", matched - o.tail.clamp(1, MAX_TAIL)));
    }
    if hidden_lines > 0 {
        out.cut.push(format!("{hidden_lines} line(s) of {hidden_records} record(s) (stack traces) are hidden: raise `frames` (up to {MAX_FRAMES}), or read one record whole with log_record."));
    }
    if o.level.is_some() {
        let unleveled = records.iter().filter(|r| r.level.is_none()).count();
        if unleveled > 0 {
            out.cut.push(format!("{unleveled} record(s) have no recognisable level and are never matched by `level`; search them with `pattern`."));
        }
    }
    if !o.window.is_open() {
        let undated = records.iter().filter(|r| r.ts_ms.is_none()).count();
        if undated > 0 {
            out.cut.push(format!("{undated} record(s) carry no timestamp and are left out by the time filter."));
        }
    }
    out
}

static UUID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b").expect("uuid regex"));
static HEXNUM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\b0x[0-9a-f]+\b|\b[0-9a-f]{12,}\b").expect("hex regex"));
static NUMBER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d+(?:\.\d+)?").expect("number regex"));

/// A message with what varies between occurrences (ids, numbers, uuids) replaced, cut at `max` chars:
/// the key under which "the same thing happened 200 times" is counted.
pub fn template(message: &str, max: usize) -> String {
    let flat = message.split_whitespace().collect::<Vec<_>>().join(" ");
    let s = UUID.replace_all(&flat, "<uuid>");
    let s = HEXNUM.replace_all(&s, "<hex>");
    let s = NUMBER.replace_all(&s, "#");
    if s.chars().count() <= max {
        return s.into_owned();
    }
    format!("{}…", s.chars().take(max).collect::<String>())
}

/// The message of a record for grouping: the head's own text, or the whole head when it had none.
pub fn message_of(rec: &Record) -> &str {
    if rec.message.is_empty() { rec.head.as_str() } else { rec.message.as_str() }
}

/// Counts and the few distinct errors: what to read first before reading anything else.
pub fn summarize(records: &[Record], window: &TimeWindow, top: usize) -> String {
    let in_window: Vec<&Record> = records.iter().filter(|r| window.is_open() || r.ts_ms.is_some_and(|t| window.contains(t))).collect();
    let mut out = String::new();
    let count = |l: Level| in_window.iter().filter(|r| r.level == Some(l)).count();
    let unleveled = in_window.iter().filter(|r| r.level.is_none()).count();
    out.push_str(&format!(
        "{} record(s): ERROR {}, WARN {}, INFO {}, DEBUG {}, TRACE {}, no level {}\n",
        in_window.len(),
        count(Level::Error),
        count(Level::Warn),
        count(Level::Info),
        count(Level::Debug),
        count(Level::Trace),
        unleveled
    ));
    let times: Vec<u64> = in_window.iter().filter_map(|r| r.ts_ms).collect();
    if let (Some(a), Some(b)) = (times.iter().min(), times.iter().max()) {
        out.push_str(&format!("covers {} .. {}\n", iso(*a), iso(*b)));
    }
    let errors: Vec<&&Record> = in_window.iter().filter(|r| r.level == Some(Level::Error)).collect();
    if errors.is_empty() {
        out.push_str("no errors.\n");
        return out;
    }
    let mut groups: Vec<(String, usize, &Record)> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for r in &errors {
        let key = template(message_of(r), 160);
        match index.get(&key) {
            Some(&g) => groups[g].1 += 1,
            None => {
                index.insert(key.clone(), groups.len());
                groups.push((key, 1, r));
            }
        }
    }
    out.push_str(&format!("{} distinct error(s){}:\n", groups.len(), if groups.len() > top { format!(", first {top} shown") } else { String::new() }));
    for (key, n, first) in groups.iter().take(top) {
        let when = first.ts_ms.map(|t| format!(" first at {}", iso(t))).unwrap_or_default();
        out.push_str(&format!("  x{n}{when} #{}: {key}\n", first.id));
        if let Some(cause) = first.rest.iter().rev().find(|l| l.trim_start().starts_with("Caused by")).or_else(|| first.rest.first()) {
            out.push_str(&format!("      {}\n", template_keep(cause.trim(), 200)));
        }
    }
    if let Some(last) = errors.last() {
        let when = last.ts_ms.map(|t| format!(" at {}", iso(t))).unwrap_or_default();
        out.push_str(&format!("last error{when} #{}: {}\n", last.id, template_keep(message_of(last), 200)));
    }
    if groups.len() > top {
        out.push_str(&format!("{} more distinct error(s) not listed: narrow with `after`/`last` or `pattern`, or use log_facets facet=exceptions.\n", groups.len() - top));
    }
    out
}

fn template_keep(text: &str, max: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max { flat } else { format!("{}…", flat.chars().take(max).collect::<String>()) }
}

/// `ERROR` / `WARN` ... of a record, `-` when it has none.
pub fn level_word(rec: &Record) -> &'static str {
    rec.level.map(level_name).unwrap_or("-")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logbuf::Stream;
    use crate::logrecord::{build_records, RawLine};
    use arbor_logscan::prelude::RuleSet;

    fn records(texts: &[&str]) -> Vec<Record> {
        let lines: Vec<RawLine> = texts.iter().enumerate().map(|(i, t)| RawLine { seq: i as u64 + 1, ts_ms: Some(1_000 * (i as u64 + 1)), stream: Stream::Stdout, text: t.to_string() }).collect();
        build_records(&RuleSet::java(), &lines)
    }

    fn pre(r: &Record) -> String {
        format!("#{} ", r.id)
    }

    #[test]
    fn level_and_pattern_filter_whole_records_including_their_traces() {
        let r = records(&["INFO a", "ERROR b failed", "\tat x.Y.z(Y.java:1)", "Caused by: java.io.IOException: disk", "WARN c"]);
        let o = ReadOptions { level: Some(Level::Error), ..Default::default() };
        let out = render(&r, &o, &pre);
        assert!(out.text.contains("#2 ERROR b failed") && out.text.contains("Caused by: java.io.IOException"));
        assert!(!out.text.contains("WARN c"));
        let o = ReadOptions { pattern: Some(Regex::new("IOException").unwrap()), ..Default::default() };
        assert_eq!(render(&r, &o, &pre).shown, 1);
        let o = ReadOptions { level: Some(Level::Warn), ..Default::default() };
        assert_eq!(render(&r, &o, &pre).shown, 2);
    }

    #[test]
    fn tail_keeps_the_newest_and_says_how_many_were_cut() {
        let r = records(&["INFO 1", "INFO 2", "INFO 3", "INFO 4"]);
        let out = render(&r, &ReadOptions { tail: 2, ..Default::default() }, &pre);
        assert_eq!((out.matched, out.shown), (4, 2));
        assert!(out.text.contains("INFO 4") && !out.text.contains("INFO 2"));
        assert!(out.cut[0].contains("2 older"));
    }

    #[test]
    fn context_surrounds_matches_merges_and_separates() {
        let texts: Vec<String> = (1..=12).map(|i| if i == 3 || i == 4 || i == 11 { format!("ERROR hit {i}") } else { format!("INFO line {i}") }).collect();
        let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
        let r = records(&refs);
        let o = ReadOptions { pattern: Some(Regex::new("hit").unwrap()), context: 1, ..Default::default() };
        let out = render(&r, &o, &pre);
        // 2..5 merged into one block, then a gap, then 10..12.
        assert!(out.text.contains("line 2") && out.text.contains("line 5") && !out.text.contains("line 6"));
        assert_eq!(out.text.matches("--\n").count(), 1);
        assert!(out.text.contains("line 10") && out.text.contains("hit 11") && out.text.contains("line 12"));
    }

    #[test]
    fn identical_consecutive_records_are_folded_with_a_count() {
        let r = records(&["2026-08-05 12:00:01 WARN pool low", "2026-08-05 12:00:02 WARN pool low", "2026-08-05 12:00:03 WARN pool low", "INFO other"]);
        let out = render(&r, &ReadOptions::default(), &pre);
        assert!(out.text.contains("[x3"), "{}", out.text);
        assert_eq!(out.text.matches("pool low").count(), 1);
        assert_eq!(out.shown, 4);
    }

    #[test]
    fn long_traces_are_cut_but_the_last_cause_survives() {
        let mut texts = vec!["ERROR boom".to_string()];
        texts.extend((0..30).map(|i| format!("\tat a.B.c{i}(B.java:{i})")));
        texts.push("Caused by: java.lang.IllegalStateException: root".into());
        let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
        let r = records(&refs);
        let out = render(&r, &ReadOptions { frames: 3, ..Default::default() }, &pre);
        assert!(out.text.contains("last cause: Caused by: java.lang.IllegalStateException: root"));
        assert!(out.cut.iter().any(|c| c.contains("`frames`")));
        assert!(!out.text.contains("c10("));
    }

    #[test]
    fn a_time_window_drops_records_outside_it_and_undated_ones_are_reported() {
        let r = records(&["INFO a", "INFO b", "INFO c"]);
        let o = ReadOptions { window: TimeWindow { after_ms: Some(2_000), before_ms: Some(2_500) }, ..Default::default() };
        let out = render(&r, &o, &pre);
        assert_eq!(out.shown, 1);
        assert!(out.text.contains("INFO b"));
    }

    #[test]
    fn templates_mask_what_varies() {
        assert_eq!(template("order 17 for 6f1c2a3e-1111-2222-3333-444455556666 failed in 3.5s", 100), "order # for <uuid> failed in #s");
        assert_eq!(template("a   b", 100), "a b");
    }

    #[test]
    fn the_summary_counts_levels_and_groups_distinct_errors() {
        let r = records(&[
            "ERROR order 1 failed",
            "\tat a.B.c(B.java:1)",
            "Caused by: java.sql.SQLException: closed",
            "INFO fine",
            "ERROR order 2 failed",
            "WARN careful",
            "ERROR other problem",
        ]);
        let s = summarize(&r, &TimeWindow::default(), 5);
        assert!(s.starts_with("5 record(s): ERROR 3, WARN 1, INFO 1"), "{s}");
        assert!(s.contains("2 distinct error(s)"));
        assert!(s.contains("x2") && s.contains("order # failed"));
        assert!(s.contains("Caused by: java.sql.SQLException: closed"), "{s}");
        assert!(s.contains("last error"));
    }

    #[test]
    fn a_clean_log_says_so() {
        let r = records(&["INFO a"]);
        assert!(summarize(&r, &TimeWindow::default(), 5).contains("no errors."));
    }
}
