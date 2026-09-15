//! The markdown report written to `CARGO_TARGET_TMPDIR`, the one-line summaries, and the listing
//! the test fails with.

use std::collections::BTreeMap;
use std::fmt::Write;

use super::clean::CleanResult;
use super::golden::DisagreementKind;
use super::module::ModuleResult;
use super::verdict::{category, coverage_label, BennuError, Status};

pub fn summary_line(result: &ModuleResult) -> String {
    let t = Tally::of(result.verdicts.expectations.iter().map(|e| e.status));
    format!(
        "{}: {} files, javac errors {} (mapped {}, hit {}, miss {}, not covered {}), false positives {}, mislabeled {}, marker disagreements {}",
        result.name,
        result.files,
        t.total(),
        t.mapped(),
        t.hit,
        t.miss,
        t.not_covered,
        result.verdicts.false_positives.len(),
        result.verdicts.mislabeled.len(),
        result.disagreements.len(),
    )
}

pub fn clean_summary_line(result: &CleanResult) -> String {
    let corpus_bug = if result.javac_diagnostics.is_empty() {
        ""
    } else {
        " — CLEAN MODULE DOES NOT COMPILE, corpus bug"
    };
    format!(
        "{}: clean, {} files, {} lines, false positives {}, javac diagnostics {}{corpus_bug}",
        result.name,
        result.files,
        result.lines,
        result.false_positives.len(),
        result.javac_diagnostics.len(),
    )
}

/// One line per false positive (error modules, then clean modules), empty when there is none.
pub fn false_positive_listing(results: &[ModuleResult], clean: &[CleanResult]) -> String {
    let on_errors = results.iter().flat_map(|r| r.verdicts.false_positives.iter().map(move |fp| (r.name, fp)));
    let on_clean = clean.iter().flat_map(|c| c.false_positives.iter().map(move |fp| (c.name, fp)));
    let mut out = String::new();
    for (module, fp) in on_errors.chain(on_clean) {
        let _ = writeln!(out, "  [{module}] {} [{}] {}", location(fp), fp.code, fp.message);
    }
    out
}

pub fn render(results: &[ModuleResult], clean: &[CleanResult]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# bennu-check vs javac — differential corpus\n");
    let _ = writeln!(out, "javac is the oracle. Lines of a multi-line statement count as one (see `anchor.rs`).\n");
    push_summary(&mut out, results);
    push_false_positives(&mut out, results);
    push_clean(&mut out, clean);
    push_mislabeled(&mut out, results);
    push_by_category(&mut out, results);
    push_by_key(&mut out, results);
    push_misses(&mut out, results);
    push_disagreements(&mut out, results);
    out
}

#[derive(Default, Clone, Copy)]
struct Tally {
    hit: usize,
    miss: usize,
    not_covered: usize,
}

impl Tally {
    fn of(statuses: impl Iterator<Item = Status>) -> Self {
        let mut tally = Tally::default();
        statuses.for_each(|s| tally.add(s));
        tally
    }

    fn add(&mut self, status: Status) {
        match status {
            Status::Hit => self.hit += 1,
            Status::Miss => self.miss += 1,
            Status::NotCovered => self.not_covered += 1,
        }
    }

    fn mapped(&self) -> usize {
        self.hit + self.miss
    }

    fn total(&self) -> usize {
        self.mapped() + self.not_covered
    }
}

fn push_summary(out: &mut String, results: &[ModuleResult]) {
    let _ = writeln!(out, "## Summary\n");
    let _ = writeln!(out, "| module | files | javac errors | hit / mapped | not covered | false positives | mislabeled | marker disagreements |");
    let _ = writeln!(out, "|---|---:|---:|---:|---:|---:|---:|---:|");
    for r in results {
        let t = Tally::of(r.verdicts.expectations.iter().map(|e| e.status));
        let _ = writeln!(
            out,
            "| {} | {} | {} | {}/{} | {} | {} | {} | {} |",
            r.name, r.files, t.total(), t.hit, t.mapped(), t.not_covered,
            r.verdicts.false_positives.len(), r.verdicts.mislabeled.len(), r.disagreements.len()
        );
    }
    let _ = writeln!(out);
}

fn push_false_positives(out: &mut String, results: &[ModuleResult]) {
    let rows: Vec<_> = results.iter().flat_map(|r| r.verdicts.false_positives.iter().map(move |e| (r.name, e))).collect();
    let _ = writeln!(out, "## False positives ({}) — the test fails on these\n", rows.len());
    if rows.is_empty() {
        let _ = writeln!(out, "None.\n");
        return;
    }
    let _ = writeln!(out, "| module | location | bennu code | message |\n|---|---|---|---|");
    for (module, e) in rows {
        let _ = writeln!(out, "| {module} | `{}` | `{}` | {} |", location(e), e.code, cell(&e.message));
    }
    let _ = writeln!(out);
}

fn push_mislabeled(out: &mut String, results: &[ModuleResult]) {
    let rows: Vec<_> = results.iter().flat_map(|r| r.verdicts.mislabeled.iter().map(move |m| (r.name, m))).collect();
    let _ = writeln!(out, "## Mislabeled ({}) — javac errors there too, but under a key not mapped to this check\n", rows.len());
    if rows.is_empty() {
        let _ = writeln!(out, "None.\n");
        return;
    }
    let _ = writeln!(out, "| module | location | bennu code | javac keys | message |\n|---|---|---|---|---|");
    for (module, m) in rows {
        let keys = m.javac_keys.join(", ");
        let _ = writeln!(out, "| {module} | `{}` | `{}` | `{keys}` | {} |", location(&m.error), m.error.code, cell(&m.error.message));
    }
    let _ = writeln!(out);
}

fn push_by_category(out: &mut String, results: &[ModuleResult]) {
    let _ = writeln!(out, "## Coverage by category\n");
    let _ = writeln!(out, "| module | category | hit / mapped | not covered |\n|---|---|---:|---:|");
    for r in results {
        let mut tallies: BTreeMap<&str, Tally> = BTreeMap::new();
        for e in &r.verdicts.expectations {
            tallies.entry(category(&e.file)).or_default().add(e.status);
        }
        for (name, t) in tallies {
            let _ = writeln!(out, "| {} | {name} | {}/{} | {} |", r.name, t.hit, t.mapped(), t.not_covered);
        }
    }
    let _ = writeln!(out);
}

fn push_by_key(out: &mut String, results: &[ModuleResult]) {
    let mut tallies: BTreeMap<&str, Tally> = BTreeMap::new();
    for e in results.iter().flat_map(|r| &r.verdicts.expectations) {
        tallies.entry(e.key.as_str()).or_default().add(e.status);
    }
    let _ = writeln!(out, "## Coverage by javac key (all modules)\n");
    let _ = writeln!(out, "| javac key | table | hit / occurrences |\n|---|---|---:|");
    for (key, t) in tallies {
        let _ = writeln!(out, "| `{key}` | {} | {}/{} |", coverage_label(key), t.hit, t.total());
    }
    let _ = writeln!(out);
}

fn push_misses(out: &mut String, results: &[ModuleResult]) {
    let _ = writeln!(out, "## Misses — mapped javac errors Bennu did not report\n");
    let mut any = false;
    for r in results {
        for e in r.verdicts.expectations.iter().filter(|e| e.status == Status::Miss) {
            let _ = writeln!(out, "- [{}] `{}:{}` `{}`", r.name, e.file, e.line, e.key);
            any = true;
        }
    }
    let _ = writeln!(out, "{}", if any { "" } else { "None.\n" });
}

fn push_disagreements(out: &mut String, results: &[ModuleResult]) {
    let _ = writeln!(out, "## Marker / golden disagreements — corpus case or marker is wrong, or the golden file is stale\n");
    let mut any = false;
    for r in results {
        for d in &r.disagreements {
            let what = match d.kind {
                DisagreementKind::Unreported => "marker, javac did not report it",
                DisagreementKind::Unmarked => "javac reports it, no marker",
            };
            let _ = writeln!(out, "- [{}] `{}:{}` `{}` — {what}", r.name, d.marker.file, d.marker.line, d.marker.key);
            any = true;
        }
    }
    let _ = writeln!(out, "{}", if any { "" } else { "None.\n" });
}

fn push_clean(out: &mut String, clean: &[CleanResult]) {
    let _ = writeln!(out, "## Clean modules — legal code: every Bennu diagnostic (error or warning) is a false positive\n");
    if clean.is_empty() {
        let _ = writeln!(out, "No clean module ran.\n");
        return;
    }
    let _ = writeln!(out, "| module | files | lines | false positives | javac diagnostics (must be 0) |\n|---|---:|---:|---:|---:|");
    for c in clean {
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} | {} |",
            c.name, c.files, c.lines, c.false_positives.len(), c.javac_diagnostics.len()
        );
    }
    let _ = writeln!(out);
    for c in clean {
        for d in &c.javac_diagnostics {
            let _ = writeln!(out, "- [{}] CLEAN MODULE DOES NOT COMPILE (corpus bug): `{}:{}` `{}`", c.name, d.file, d.line, d.key);
        }
    }
    let rows: Vec<_> = clean.iter().flat_map(|c| c.false_positives.iter().map(move |e| (c.name, e))).collect();
    if rows.is_empty() {
        let _ = writeln!(out, "\nNo false positives.\n");
        return;
    }
    let _ = writeln!(out, "\n| module | location | bennu code | message |\n|---|---|---|---|");
    for (module, e) in rows {
        let _ = writeln!(out, "| {module} | `{}` | `{}` | {} |", location(e), e.code, cell(&e.message));
    }
    let _ = writeln!(out);
}

fn location(error: &BennuError) -> String {
    if error.first_line == error.last_line {
        format!("{}:{}", error.file, error.first_line)
    } else {
        format!("{}:{}-{}", error.file, error.first_line, error.last_line)
    }
}

/// A message made safe for a markdown table cell.
fn cell(text: &str) -> String {
    text.replace('|', "\\|").replace('\n', " ")
}
