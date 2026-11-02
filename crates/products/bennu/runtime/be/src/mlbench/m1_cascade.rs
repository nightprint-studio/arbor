//! M1 — ablation of the ghost-text cascade on held-out public lines.
//!
//! One pass through the real `suggest` answers both configurations: the model is the LAST layer, so "the cascade
//! without the model" is exactly the samples whose answer did not come from it (the others would have been silent).
//! `name` is counted apart: it lives inside the model branch and disappears with the model in the product.
//!
//! `cargo test -p bennu-be mlbench::m1_cascade -- --ignored --nocapture`
//! (`BENNU_MLB_N` samples per project, 0 = every drawn line; `BENNU_MLB_ONLY=commons-lang` to run one project;
//! `BENNU_MLB_MIN` confidence gate in percent, default 70; `BENNU_MLB_MODEL_ALONE=0` skips the model-only pass)

use std::collections::BTreeMap;

use bennu_core::prelude::BennuConfig;

use super::common::*;
use crate::flc::{apply_flc_settings, prepare_now, status_now, suggest, FlcArgs};

/// A line worth completing: the lab's own rule (`stub/src/eval.rs::eligible`), so the sample matches its reference numbers.
pub(super) fn eligible(line: &str) -> bool {
    let t = line.trim();
    t.len() >= 25 && line.is_ascii() && !["*", "//", "/*", "import ", "package ", "@"].iter().any(|p| t.starts_with(p))
}

/// Cuts at a word start after the indent, leaving at least 8 bytes of tail (the lab's `split_point`).
pub(super) fn split_point(line: &str, rng: &mut u64) -> Option<usize> {
    let indent = line.len() - line.trim_start().len();
    let b = line.as_bytes();
    let cands: Vec<usize> = (indent + 8..line.len().saturating_sub(8)).filter(|&i| matches!(b[i - 1], b' ' | b'(' | b'.' | b',') && b[i] != b' ').collect();
    if cands.is_empty() {
        return None;
    }
    *rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    Some(cands[(*rng >> 33) as usize % cands.len()])
}

/// Characters of the expected text that coincide from the start, over its length.
pub(super) fn prefix_share(got: &str, expected: &str) -> f64 {
    let total = expected.chars().count();
    if total == 0 {
        return 1.0;
    }
    got.chars().zip(expected.chars()).take_while(|(a, b)| a == b).count() as f64 / total as f64
}

/// The suggestion repeats itself: the whole line is one of the five above, or one word of 4+ letters comes back 3+ times.
pub(super) fn repeats(prefix: &str, got: &str, above: &[&str]) -> bool {
    let line = format!("{}{}", prefix.trim_start(), got.trim_end());
    if got.trim().len() >= 6 && above.iter().any(|l| l.trim() == line) {
        return true;
    }
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    for w in got.split(|c: char| !c.is_alphanumeric() && c != '_').filter(|w| w.len() >= 4) {
        *seen.entry(w).or_default() += 1;
    }
    seen.values().any(|&n| n >= 3)
}

/// Which of the layers sharing an `origin` string wrote `text` — they are called again in the cascade's order.
fn sublayer(origin: &str, file: &str, source: &str, offset: usize, line_prefix: &str, text: &str) -> String {
    let same = |t: Option<String>| t.is_some_and(|t| t.trim() == text.trim());
    match origin {
        "habit" => {
            if same(crate::flc_created::suggest(line_prefix).map(|c| c.text)) { "created" }
            else if same(crate::flc_habits::suggest(file, source, offset).map(|h| h.text)) { "habits" }
            else if same(crate::flc_member_args::suggest(file, source, offset).map(|m| m.text)) { "member_args" }
            else { "habit?" }
        }
        "arguments" => {
            if same(crate::flc_arglist::array_constructor(file, source, offset).map(|a| a.text)) { "array_ctor" }
            else if same(crate::flc_arglist::class_literal(file, source, offset).map(|a| a.text)) { "class_literal" }
            else if same(crate::flc_arglist::creation(file, source, offset).map(|a| a.text)) { "creation" }
            else if same(crate::flc_arglist::rest(file, source, offset).map(|a| a.text)) { "arglist_rest" }
            else if same(crate::flc_arglist::suggest(file, source, offset).map(|a| a.text)) { "arglist" }
            else { "arguments?" }
        }
        other => return other.to_string(),
    }
    .to_string()
}

struct Row {
    project: String,
    /// Where the sample is, and what was asked and answered (public code): lets other benches join on the same samples.
    place: String,
    expected: String,
    got: String,
    layer: String,
    confidence: f32,
    exact: bool,
    prefix: f64,
    invented: bool,
    repeated: bool,
}

#[derive(Default, Clone)]
struct Agg {
    n: usize,
    exact: usize,
    prefix: f64,
    invented: usize,
    repeated: usize,
}

impl Agg {
    fn add(&mut self, r: &Row) {
        self.n += 1;
        self.exact += usize::from(r.exact);
        self.prefix += r.prefix;
        self.invented += usize::from(r.invented);
        self.repeated += usize::from(r.repeated);
    }
}

fn config_line(label: &str, a: &Agg, asked: usize) -> String {
    format!(
        "| {label} | {} | {:.1}% | {} | {:.1}% ±{:.1} | {:.1}% | {:.1}% | {:.1}% | {:.1}% |",
        a.n,
        pct(a.n, asked),
        a.exact,
        pct(a.exact, a.n),
        ci95(a.exact, a.n),
        100.0 * a.prefix / a.n.max(1) as f64,
        100.0 * a.prefix / asked.max(1) as f64,
        pct(a.invented, a.n),
        pct(a.repeated, a.n),
    )
}

#[test]
#[ignore = "needs the public corpus and the base model: see the module docs"]
fn m1_cascade_ablation() {
    let n_per_project = env_num("BENNU_MLB_N", 0);
    let min = env_num("BENNU_MLB_MIN", 70) as u32;
    let only = env("BENNU_MLB_ONLY");
    let model_alone = env("BENNU_MLB_MODEL_ALONE").is_none_or(|v| v != "0");
    let mut rows: Vec<Row> = Vec::new();
    let mut alone: Vec<Row> = Vec::new();
    let mut asked_by_project: BTreeMap<String, usize> = BTreeMap::new();
    let mut tsv = String::from("project\tlayer\tconfidence\texact\tprefix\tinvented\trepeated\tplace\texpected\tgot\n");

    for spec in PROJECTS.iter().filter(|p| only.as_deref().is_none_or(|o| o == p.name)) {
        let (root, held) = assemble(spec);
        eprintln!("== {} : {} held-out files", spec.name, held.len());
        open_and_wait(&root);

        let mut config = BennuConfig::default();
        config.flc_logs = true;
        config.flc_min_confidence = min;
        config.flc_enabled = true;
        config.flc_model_dir = model_dir().to_string_lossy().into_owned();
        apply_flc_settings(&config);
        prepare_now(&root.to_string_lossy());
        let waited = std::time::Instant::now();
        loop {
            let s = status_now();
            if (s.model == "ready" || s.model.starts_with("failed")) && !s.log_scanning {
                eprintln!("model: {}, ready after {:.0}s", s.model, waited.elapsed().as_secs_f32());
                break;
            }
            assert!(waited.elapsed().as_secs() < 900, "scan/model not ready: {} scanning={}", s.model, s.log_scanning);
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        eprintln!("copy layer holds (files, contexts): {:?} of {} held-out files", crate::flc_ngram::tracked(), held.len());
        assert_eq!(status_now().model, "ready", "the model did not load: give BENNU_FLC_MODEL a folder with flc.onnx");

        // the lab's draw: one line in eleven, at most four per file
        let (mut asked, mut taken_project) = (0usize, 0usize);
        for h in &held {
            let file = h.path.to_string_lossy().into_owned();
            let fname = h.path.file_name().unwrap().to_string_lossy().into_owned();
            let lines: Vec<&str> = h.text.split('\n').collect();
            let (mut offset, mut taken) = (0usize, 0usize);
            for (line_no, line) in lines.iter().enumerate() {
                let start = offset;
                offset += line.len() + 1;
                let mut rng = line_seed(&format!("{}/{fname}", h.source), line_no);
                if taken >= 4 || (rng >> 33) % 11 != 0 || !eligible(line) {
                    continue;
                }
                let Some(cut) = split_point(line, &mut rng) else { continue };
                if n_per_project > 0 && taken_project >= n_per_project {
                    break;
                }
                taken += 1;
                taken_project += 1;
                let expected = line[cut..].trim_end();
                let source = format!("{}{}", &h.text[..start + cut], &h.text[start + line.len()..]);
                let prefix = &line[..cut];
                let above: Vec<&str> = lines[line_no.saturating_sub(5)..line_no].to_vec();
                asked += 1;
                crate::flc_check::set_enabled(true);
                if model_alone {
                    crate::flc_quality::BYPASS_GATE.with(|b| b.set(true));
                    crate::flc_check::set_enabled(false);
                    let args = FlcArgs { file: file.clone(), offset: start + cut, source: Some(source.clone()) };
                    let mo = crate::flc::model_only(&args);
                    if env("BENNU_MLB_SHOW").is_some() { eprintln!("[model_only] {:?} -> {:?}", &line[..cut], mo.as_ref().map(|r| (&r.text, r.confidence))); }
                    if let Some(r) = mo {
                        let got = r.text.trim_end();
                        alone.push(Row { project: spec.name.into(), place: String::new(), expected: String::new(), got: String::new(), layer: "model_only".into(), confidence: r.confidence, exact: got == expected, prefix: prefix_share(got, expected), invented: false, repeated: repeats(prefix, got, &above) });
                    }
                    crate::flc_quality::BYPASS_GATE.with(|b| b.set(false));
                    crate::flc_check::set_enabled(true);
                }
                // the copy layers count every line of the project from disk, the asked one included: in the editor that line is not
                // written yet, so the file is counted again as the buffer the caret is in, without it
                if env("BENNU_MLB_COUNT_OWN").is_none() {
                    crate::flc_ngram::add_file(&file, &source);
                    crate::flc_catch::add_file(&file, &source);
                }
                let reply = suggest(FlcArgs { file: file.clone(), offset: start + cut, source: Some(source.clone()) }).unwrap_or(None);
                let Some(reply) = reply else { continue };
                let got = reply.text.trim_end();
                let exact = got == expected;
                let layer = sublayer(&reply.origin, &file, &source, start + cut, prefix, got);
                let start_of_it = !exact && !got.is_empty() && expected.starts_with(got);
                let invented = !exact && !start_of_it && crate::flc_check::invents(&file, &source, start + cut, got);
                let row = Row { project: spec.name.into(), place: format!("{fname}:{line_no}:{cut}"), expected: expected.to_string(), got: got.to_string(), layer, confidence: reply.confidence, exact, prefix: prefix_share(got, expected), invented, repeated: repeats(prefix, got, &above) };
                let flat = |t: &str| t.replace(['\t', '\n', '\r'], " ");
                tsv.push_str(&format!("{}\t{}\t{:.3}\t{}\t{:.3}\t{}\t{}\t{}\t{}\t{}\n", row.project, row.layer, row.confidence, u8::from(row.exact), row.prefix, u8::from(row.invented), u8::from(row.repeated), row.place, flat(&row.expected), flat(&row.got)));
                rows.push(row);
            }
        }
        asked_by_project.insert(spec.name.into(), asked);
        eprintln!("{}: {asked} asked, {} shown so far", spec.name, rows.len());
        IndexService_forget(&root);
    }
    let asked: usize = asked_by_project.values().sum();
    let report = report(&rows, &alone, &asked_by_project, asked, min);
    let path = write_result("m1_rows.tsv", &tsv);
    let md = write_result("m1_report.md", &report);
    eprintln!("{report}\nrows: {}\nreport: {}", path.display(), md.display());
    assert!(asked > 0);
}

#[allow(non_snake_case)]
fn IndexService_forget(root: &std::path::Path) {
    crate::index_service::IndexService::global().forget(&root.to_string_lossy());
}

fn report(rows: &[Row], alone: &[Row], per_project: &BTreeMap<String, usize>, asked: usize, min: u32) -> String {
    let mut out = format!("## M1 — {asked} held-out lines (gate {min}%)\n\n");
    out.push_str(&format!("Asked per project: {per_project:?}\n\n"));
    let agg = |f: &dyn Fn(&Row) -> bool| rows.iter().filter(|r| f(r)).fold(Agg::default(), |mut a, r| { a.add(r); a });
    let head = "| configuration | shown | coverage | exact | exact of shown | prefix of shown | prefix of asked | invented | repeated |\n|---|---|---|---|---|---|---|---|---|\n";
    out.push_str(head);
    out.push_str(&config_line("full cascade (with model)", &agg(&|_| true), asked));
    out.push('\n');
    out.push_str(&config_line("without the ONNX model", &agg(&|r| r.layer != "model"), asked));
    out.push('\n');
    out.push_str(&config_line("without model and without `name`", &agg(&|r| r.layer != "model" && r.layer != "name"), asked));
    out.push('\n');
    out.push_str(&config_line("the model's slice alone", &agg(&|r| r.layer == "model"), asked));
    out.push('\n');
    if !alone.is_empty() {
        let a = alone.iter().fold(Agg::default(), |mut a, r| { a.add(r); a });
        out.push_str(&config_line("model alone, every position, no gate", &a, asked));
        out.push('\n');
    }
    out.push_str("\n### By layer\n\n| layer | shown | share of shown | exact | prefix | invented | repeated | mean confidence |\n|---|---|---|---|---|---|---|---|\n");
    let mut layers: BTreeMap<&str, (Agg, f64)> = BTreeMap::new();
    for r in rows {
        let e = layers.entry(r.layer.as_str()).or_default();
        e.0.add(r);
        e.1 += r.confidence as f64;
    }
    let mut ordered: Vec<_> = layers.iter().collect();
    ordered.sort_by(|a, b| b.1 .0.n.cmp(&a.1 .0.n));
    for (name, (a, conf)) in ordered {
        out.push_str(&format!("| {name} | {} | {:.1}% | {:.1}% | {:.1}% | {:.1}% | {:.1}% | {:.2} |\n", a.n, pct(a.n, rows.len()), pct(a.exact, a.n), 100.0 * a.prefix / a.n as f64, pct(a.invented, a.n), pct(a.repeated, a.n), conf / a.n as f64));
    }
    out.push_str("\n### Marginal value of the model by its own confidence\n\nConfidence is the calibrated one the editor compares with the gate (the profile's own usage statistics scale it: run in an isolated HOME for a clean reading). Combined = cascade without the model + the model answers at or above the threshold.\n\n| model confidence ≥ | model answers | model exact | model prefix | combined shown | combined coverage | combined exact of shown | combined prefix of asked |\n|---|---|---|---|---|---|---|---|\n");
    let base = agg(&|r| r.layer != "model");
    for th in [0.0f32, 0.70, 0.80, 0.85, 0.90, 0.95] {
        let m = agg(&|r| r.layer == "model" && r.confidence >= th);
        let (n, ex, pf) = (base.n + m.n, base.exact + m.exact, base.prefix + m.prefix);
        out.push_str(&format!("| {th:.2} | {} | {:.1}% | {:.1}% | {n} | {:.1}% | {:.1}% | {:.1}% |\n", m.n, pct(m.exact, m.n), 100.0 * m.prefix / m.n.max(1) as f64, pct(n, asked), pct(ex, n), 100.0 * pf / asked as f64));
    }
    out.push_str("\n### By project\n\n| project | asked | shown | exact of shown | model share of shown |\n|---|---|---|---|---|\n");
    for (p, a) in per_project {
        let s = agg(&|r| &r.project == p);
        let m = agg(&|r| &r.project == p && r.layer == "model");
        out.push_str(&format!("| {p} | {a} | {} | {:.1}% | {:.1}% |\n", s.n, pct(s.exact, s.n), pct(m.n, s.n)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prefix_share_stops_at_the_first_wrong_character() {
        assert_eq!(prefix_share("abcX", "abcdef"), 0.5);
        assert_eq!(prefix_share("abcdef", "abcdef"), 1.0);
        assert_eq!(prefix_share("", "abc"), 0.0);
    }

    #[test]
    fn a_line_already_written_above_or_a_looping_word_is_a_repetition() {
        assert!(repeats("        return ", "foo.bar(x);", &["        return foo.bar(x);"]));
        assert!(repeats("x = ", "name + name + name", &[]));
        assert!(!repeats("x = ", "a.b(c)", &["y = z;"]));
    }

    #[test]
    fn the_draw_leaves_comments_and_short_lines_alone() {
        assert!(!eligible("    // a long comment line that is far too long to skip"));
        assert!(!eligible("    int x;"));
        assert!(eligible("        return repository.findById(id).orElse(null);"));
        let mut r = 7u64;
        let line = "        return repository.findById(id);";
        let cut = split_point(line, &mut r).unwrap();
        assert!(cut >= 16 && line.len() - cut >= 8);
    }
}
