//! How good the whole ghost-text pipeline is on a **real project**, measured through the same `suggest` the editor
//! calls — the index, the header, the log profile, the project's repeated lines, the model, the checks.
//!
//! The lab can only measure the model with a stand-in index. This opens a project the way Bennu does, replays lines of
//! its own files as if somebody were typing them, and compares what comes back with what was written. Numbers only:
//! no line of code is printed unless `BENNU_QUALITY_SHOW=1`.
//!
//! Skipped unless `BENNU_QUALITY_PROJECT` names a project root. It is slow (the index has to build) and, run on a
//! project the personal adapter was trained on, it measures memory more than skill — pick a project it never saw.
//!
//! ```text
//! BENNU_QUALITY_PROJECT=/path/to/project            # required
//! BENNU_FLC_MODEL=/path/to/model-folder             # optional: without it only logs and repeated lines answer
//! BENNU_QUALITY_CUT=word|mid|start                  # where the caret is in the line (default word)
//! BENNU_QUALITY_N=300  BENNU_QUALITY_MIN=70         # samples; confidence threshold in percent (default 70)
//! BENNU_QUALITY_JDK=21                              # the project's Java level (default 21)
//! BENNU_QUALITY_ALTERNATIVES=1                      # the three-candidates setting
//! BENNU_QUALITY_COMPILE=1                           # also: how many of the shown answers get a new 'invented' error
//! BENNU_QUALITY_DUMP=/path/rows.tsv                 # append one row of numbers per answer (no text): for offline fitting
//! BENNU_QUALITY_PREFS=/path/pairs.jsonl             # collect preference pairs: token ids of the prompt + the line as written
//!                                                   # (chosen) against a candidate the checker calls invented (rejected). Private:
//!                                                   # write it under the lab's private folder, never into the repo
//! BENNU_QUALITY_NOCHECK=1                           # switch the pipeline's own check off, to see what it removes
//! cargo test -p bennu-be flc_quality -- --nocapture --ignored
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use arbor_ipc::prelude::EventSink;
use bennu_core::prelude::BennuConfig;
use serde_json::Value;

use crate::flc::{apply_flc_settings, prepare_now, status_now, suggest, FlcArgs};
use crate::index_service::IndexService;

/// What the model was asked and answered on the last `suggest` of this thread — the harness reads it for preference pairs.
pub(crate) struct Tap {
    pub prompt: Vec<u16>,
    pub typed: String,
    pub chain_start: bool,
    pub candidates: Vec<(String, f32)>,
}

thread_local! {
    /// Lets the model answer where the gate would keep it silent (the harness asks what it would have written).
    pub(crate) static BYPASS_GATE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static TAP: std::cell::RefCell<Option<Tap>> = const { std::cell::RefCell::new(None) };
}

pub(crate) fn record(prompt: &[u16], typed: &str, chain_start: bool, candidates: &[bennu_flc::prelude::Candidate]) {
    TAP.with(|t| {
        *t.borrow_mut() = Some(Tap { prompt: prompt.to_vec(), typed: typed.to_string(), chain_start, candidates: candidates.iter().map(|c| (c.text.clone(), c.confidence)).collect() })
    });
}

struct Quiet;
impl EventSink for Quiet {
    fn emit(&self, _topic: &str, _payload: Value) {}
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

fn java_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let path = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            if ![".git", "target", "build", "node_modules", "test"].contains(&name.as_str()) {
                java_files(&path, out);
            }
        } else if name.ends_with(".java") {
            out.push(path);
        }
    }
}

/// A line worth typing again: code, not a comment, import, annotation or brace.
fn eligible(line: &str) -> bool {
    let t = line.trim();
    t.len() >= 16 && !["*", "//", "/*", "import ", "package ", "@", "}", "{"].iter().any(|p| t.starts_with(p))
}

/// Where in `line` the caret is. `word`: just after a space, `(`, `.` or `,`; `mid`: two characters into a word;
/// `start`: before the statement.
fn cut_of(line: &str, mode: &str, pick: usize) -> Option<usize> {
    let b = line.as_bytes();
    let indent = line.len() - line.trim_start().len();
    let ident = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let cands: Vec<usize> = match mode {
        "start" => vec![indent],
        // right after the `=` of a declaration: what the variable is given decides how the rest of the method can use it
        "decl" => {
            let eq = line.find(" = ").filter(|&e| !line[..e].contains(['(', '.', '=']));
            match eq {
                Some(e) if line[..e].trim().split_whitespace().count() >= 2 => vec![e + 3],
                _ => Vec::new(),
            }
        }
        "mid" => (indent + 4..line.len().saturating_sub(8))
            .filter(|&i| matches!(b[i - 1], b' ' | b'(' | b'.' | b',') && ident(b[i]) && b[i..].iter().take_while(|&&c| ident(c)).count() >= 5)
            .map(|i| i + 2)
            .collect(),
        _ => (indent + 8..line.len().saturating_sub(8)).filter(|&i| matches!(b[i - 1], b' ' | b'(' | b'.' | b',') && b[i] != b' ').collect(),
    };
    let cands: Vec<usize> = cands.into_iter().filter(|&i| line.is_char_boundary(i)).collect();
    (!cands.is_empty()).then(|| cands[pick % cands.len()])
}

/// 0..1: how much of the longer string the other covers, by edit distance.
fn similarity(a: &str, b: &str) -> f32 {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i; b.len() + 1];
        for j in 1..=b.len() {
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + usize::from(a[i - 1] != b[j - 1]));
        }
        prev = cur;
    }
    1.0 - prev[b.len()] as f32 / a.len().max(b.len()) as f32
}

#[derive(Default)]
struct Tally {
    n: usize,
    exact: usize,
    /// Not the whole line, but a correct start of it.
    start_of_it: usize,
    similar: f32,
    broken: usize,
    checked: usize,
}

impl Tally {
    fn line(&self, label: &str, asked: usize) -> String {
        let pct = |x: usize, of: usize| if of == 0 { 0.0 } else { 100.0 * x as f64 / of as f64 };
        format!(
            "{label:<10} answered {:>4} ({:>4.1}% of {asked}), exact {:>4.1}%, a correct start {:>4.1}%, similar {:>4.1}%{}",
            self.n,
            pct(self.n, asked),
            pct(self.exact, self.n),
            pct(self.start_of_it, self.n),
            if self.n == 0 { 0.0 } else { 100.0 * self.similar as f64 / self.n as f64 },
            if self.checked > 0 { format!(", answers with an invented error {:>4.1}% ({} checked)", pct(self.broken, self.checked), self.checked) } else { String::new() }
        )
    }
}

#[test]
#[ignore = "needs a real project: see the module docs"]
fn the_pipeline_on_a_real_project() {
    let Some(root) = env("BENNU_QUALITY_PROJECT") else {
        eprintln!("BENNU_QUALITY_PROJECT not set: skipping");
        return;
    };
    let wanted: usize = env("BENNU_QUALITY_N").and_then(|v| v.parse().ok()).unwrap_or(300);
    let mode = env("BENNU_QUALITY_CUT").unwrap_or_else(|| "word".into());
    let min_confidence: u32 = env("BENNU_QUALITY_MIN").and_then(|v| v.parse().ok()).unwrap_or(70);
    let compile = env("BENNU_QUALITY_COMPILE").is_some();
    let show = env("BENNU_QUALITY_SHOW").is_some();
    let check = env("BENNU_QUALITY_NOCHECK").is_none();
    let dump_path = env("BENNU_QUALITY_DUMP");
    let prefs_path = env("BENNU_QUALITY_PREFS");
    let mut prefs = String::new();
    let mut pairs = 0usize;
    let mut dump = String::new();
    let label = Path::new(&root).file_name().and_then(|n| n.to_str()).unwrap_or("?").to_string();

    // the index, built the way opening a project builds it
    let svc = IndexService::global();
    svc.open(&root, &env("BENNU_QUALITY_JDK").unwrap_or_else(|| "21".into()), "UTF-8", Arc::new(Quiet));
    let started = Instant::now();
    while !svc.has_resolver(&root) {
        assert!(started.elapsed() < Duration::from_secs(600), "the index did not come up in 10 minutes");
        std::thread::sleep(Duration::from_millis(500));
    }
    // the semantic engine is what types the project's own classes: wait for it too, but not forever
    while !svc.has_semantic_engine(&root) && started.elapsed() < Duration::from_secs(180) {
        std::thread::sleep(Duration::from_millis(500));
    }
    eprintln!("index ready after {:.0}s", started.elapsed().as_secs_f32());

    let mut config = BennuConfig::default();
    config.flc_logs = true;
    config.flc_min_confidence = min_confidence;
    config.flc_alternatives = env("BENNU_QUALITY_ALTERNATIVES").is_some() || env("BENNU_QUALITY_PREFS").is_some();
    if let Some(dir) = env("BENNU_FLC_MODEL") {
        config.flc_enabled = true;
        config.flc_model_dir = dir;
    }
    apply_flc_settings(&config);

    let mut files = Vec::new();
    java_files(Path::new(&root), &mut files);
    files.sort();
    assert!(!files.is_empty(), "no .java file under {root}");

    // the first request starts the log scan and the model load; wait for both
    prepare_now(&root);
    let waited = Instant::now();
    loop {
        let s = status_now();
        let model_ready = !config.flc_enabled || s.model == "ready" || s.model.starts_with("failed");
        if model_ready && !s.log_scanning {
            eprintln!("model: {}, {} log calls in {} patterns, ready after {:.0}s", s.model, s.log_calls, s.log_patterns, waited.elapsed().as_secs_f32());
            break;
        }
        assert!(waited.elapsed() < Duration::from_secs(600), "scan/model not ready in 10 minutes: {} scanning={}", s.model, s.log_scanning);
        std::thread::sleep(Duration::from_millis(500));
    }

    // every eligible line gets one caret; a stride spreads the sample over the whole project
    let mut candidates: Vec<(usize, usize)> = Vec::new(); // (file index, line index)
    let texts: Vec<String> = files.iter().map(|f| std::fs::read_to_string(f).unwrap_or_default()).collect();
    for (fi, text) in texts.iter().enumerate() {
        for (li, line) in text.lines().enumerate() {
            if eligible(line) && (mode != "decl" || cut_of(line, "decl", 0).is_some()) {
                candidates.push((fi, li));
            }
        }
    }
    let stride = (candidates.len() / wanted.max(1)).max(1);
    let (mut all, mut by_origin): (Tally, BTreeMap<String, Tally>) = (Tally::default(), BTreeMap::new());
    let (mut asked, mut micros) = (0usize, Vec::new());
    for (k, &(fi, li)) in candidates.iter().step_by(stride).take(wanted).enumerate() {
        let text = &texts[fi];
        let line = text.lines().nth(li).unwrap_or("");
        let start = text.lines().take(li).map(|l| l.len() + 1).sum::<usize>();
        let Some(rel) = cut_of(line, &mode, k.wrapping_mul(7919)) else { continue };
        if mode == "decl" {
            let name = line[..rel].trim_end().trim_end_matches('=').trim_end().rsplit(|c: char| !(c.is_alphanumeric() || c == '_')).next().unwrap_or("");
            let later: String = text.lines().skip(li + 1).take(40).collect::<Vec<_>>().join("\n");
            if name.is_empty() || !later.split(|c: char| !(c.is_alphanumeric() || c == '_')).any(|w| w == name) {
                continue;
            }
        }
        let expected = line[rel..].trim_end();
        // the buffer as the editor holds it: the typed start, the rest of the line gone, the rest of the file as it is
        let eol = start + line.len();
        let source = format!("{}{}", &text[..start + rel], &text[eol..]);
        asked += 1;
        let t = Instant::now();
        crate::flc_check::set_enabled(check);
        if std::env::var("BENNU_QUALITY_TRACE").is_ok() {
            eprintln!("TRACE {} line {}", files[fi].display(), li + 1);
        }
        let reply = suggest(FlcArgs { file: files[fi].to_string_lossy().into_owned(), offset: start + rel, source: Some(source.clone()) }).unwrap_or(None);
        micros.push(t.elapsed().as_micros());
        // the verdict below is the measurement itself: it is always on
        crate::flc_check::set_enabled(true);
        if prefs_path.is_some() {
            if let Some(tap) = TAP.with(|t| t.borrow_mut().take()) {
                let file = files[fi].to_string_lossy();
                let written = format!("{}{}", &tap.typed, expected);
                let mut taken = 0;
                for (text, _) in tap.candidates.iter().take(3) {
                    let mut got = text.get(tap.typed.len()..).unwrap_or("").to_string();
                    if tap.chain_start {
                        got.insert(0, '.');
                    }
                    // a candidate the cheap guards already change or drop (a looping list, a repeated annotation element, a ternary, a
                    // declaration where an argument goes) is as good a negative as one the checker calls invented
                    let guarded = {
                        let line_prefix = &source[start..start + rel];
                        let flat = |t: &str| t.replace(' ', "");
                        bennu_flc::prelude::vet(line_prefix, got.trim_end(), &|_| true).map_or(true, |v| flat(&v) != flat(got.trim_end()))
                    };
                    if taken < 2 && !got.is_empty() && got.trim_end() != expected && (guarded || crate::flc_check::invents(&file, &source, start + rel, got.trim_end())) {
                        let ids = serde_json::to_string(&tap.prompt).unwrap_or_default();
                        prefs.push_str(&format!("{{\"prompt\":{ids},\"chosen\":{},\"rejected\":{},\"weight\":1.0}}\n", serde_json::to_string(&written).unwrap_or_default(), serde_json::to_string(text).unwrap_or_default()));
                        taken += 1;
                        pairs += 1;
                    } else if taken == 0 && !got.is_empty() && similarity(got.trim_end(), expected) < 0.5 {
                        // valid, but not what was written and nothing like it: a weak negative, worth a third of the others
                        let ids = serde_json::to_string(&tap.prompt).unwrap_or_default();
                        prefs.push_str(&format!("{{\"prompt\":{ids},\"chosen\":{},\"rejected\":{},\"weight\":0.3}}\n", serde_json::to_string(&written).unwrap_or_default(), serde_json::to_string(text).unwrap_or_default()));
                        taken += 1;
                        pairs += 1;
                    }
                }
            }
        }
        let Some(reply) = reply else { continue };
        let got = reply.text.trim_end();
        let (exact, sim) = (got == expected, similarity(got, expected));
        let start_of_it = !exact && !got.is_empty() && expected.starts_with(got);
        let mut broken = None;
        // a right answer, or the right start of one, is not the model's invention: only the others are asked to compile
        if compile && !exact && !start_of_it {
            // the same verdict the pipeline gives: does the line get a new error of the kind that means invention?
            broken = Some(crate::flc_check::invents(&files[fi].to_string_lossy(), &source, start + rel, got));
        }
        if dump_path.is_some() {
            // numbers only: where it came from, how sure it was, how long, and how it turned out
            let invented = broken.unwrap_or_else(|| crate::flc_check::invents(&files[fi].to_string_lossy(), &source, start + rel, got));
            let category = if got.contains("->") || got.contains("::") { "lambda" } else if got.matches('.').count().min(got.matches('(').count()) >= 2 { "chain" } else if got.contains('(') { "call" } else { "other" };
            dump.push_str(&format!("{label}\t{}\t{:.4}\t{}\t{}\t{}\t{}\t{:.3}\t{}\t{category}\n", reply.origin, reply.confidence, got.len(), line[..rel].trim_start().len(), u8::from(exact), u8::from(start_of_it), sim, u8::from(invented)));
        }
        if show {
            eprintln!("[{}] {:<8} {:.2}  {}| want {:?} got {:?}", if exact { "=" } else { "x" }, reply.origin, reply.confidence, &line[..rel].trim_start(), expected, got);
        }
        for t in [&mut all, by_origin.entry(reply.origin.clone()).or_default()] {
            t.n += 1;
            t.exact += usize::from(exact);
            t.start_of_it += usize::from(start_of_it);
            t.similar += sim;
            if let Some(b) = broken {
                t.checked += 1;
                t.broken += usize::from(b);
            }
        }
    }
    // positions where the editor stays silent (after `;`, `{`, `}`, a finished annotation): the right answer is nothing
    if let (Some(_), Some(n)) = (&prefs_path, env("BENNU_QUALITY_SILENCE").and_then(|v| v.parse::<usize>().ok())) {
        let mut silent: Vec<(usize, usize)> = Vec::new();
        for (fi, text) in texts.iter().enumerate() {
            for (li, line) in text.lines().enumerate() {
                let t = line.trim_end();
                let tt = t.trim_start();
                let annotation = tt.starts_with('@') && tt.matches('(').count() == tt.matches(')').count();
                if tt.len() >= 8 && (t.ends_with(';') || t.ends_with('{') || t.ends_with('}') || annotation) && !tt.starts_with("//") && !tt.starts_with('*') && !tt.starts_with("import ") {
                    silent.push((fi, li));
                }
            }
        }
        let stride = (silent.len() / n.max(1)).max(1);
        crate::flc_check::set_enabled(false);
        BYPASS_GATE.with(|b| b.set(true));
        let mut wrote = 0usize;
        for &(fi, li) in silent.iter().step_by(stride).take(n) {
            let text = &texts[fi];
            let line = text.lines().nth(li).unwrap_or("").trim_end();
            let start = text.lines().take(li).map(|l| l.len() + 1).sum::<usize>();
            let offset = start + line.len();
            let source = text.clone();
            let args = FlcArgs { file: files[fi].to_string_lossy().into_owned(), offset, source: Some(source) };
            if crate::flc::model_only(&args).is_none() {
                continue;
            }
            if let Some(tap) = TAP.with(|t| t.borrow_mut().take()) {
                if let Some((cand, _)) = tap.candidates.first().filter(|(c, _)| !c.trim().is_empty()) {
                    let ids = serde_json::to_string(&tap.prompt).unwrap_or_default();
                    prefs.push_str(&format!("{{\"prompt\":{ids},\"chosen\":\"\",\"rejected\":{},\"weight\":0.7}}\n", serde_json::to_string(cand).unwrap_or_default()));
                    wrote += 1;
                    pairs += 1;
                }
            }
        }
        BYPASS_GATE.with(|b| b.set(false));
        crate::flc_check::set_enabled(true);
        eprintln!("silence pairs: {wrote}");
    }
    if let Some(path) = &dump_path {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = f.write_all(dump.as_bytes());
        }
    }
    if let Some(path) = &prefs_path {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = f.write_all(prefs.as_bytes());
        }
        eprintln!("preference pairs written: {pairs} (from {asked} samples)");
    }
    micros.sort();
    eprintln!("\n== {root_name} / cut {mode} / confidence >= {min_confidence}%{alt}{chk}", root_name = Path::new(&root).file_name().and_then(|n| n.to_str()).unwrap_or("?"), alt = if config.flc_alternatives { " / three candidates" } else { "" }, chk = if check { "" } else { " / NO check" });
    eprintln!("{}", all.line("ALL", asked));
    for (origin, t) in &by_origin {
        eprintln!("{}", t.line(origin, asked));
    }
    if !micros.is_empty() {
        eprintln!("latency: avg {:.0} ms, p50 {} ms, p95 {} ms", micros.iter().sum::<u128>() as f64 / micros.len() as f64 / 1000.0, micros[micros.len() / 2] / 1000, micros[micros.len() * 95 / 100] / 1000);
    }
    assert!(asked > 0, "no line to replay");
}

#[cfg(test)]
mod pure {
    use super::*;

    #[test]
    fn the_caret_lands_where_the_mode_says() {
        let line = "        return repository.findById(id).orElseThrow(NotFound::new);";
        let word = cut_of(line, "word", 3).unwrap();
        assert!(matches!(line.as_bytes()[word - 1], b' ' | b'(' | b'.' | b','));
        let mid = cut_of(line, "mid", 0).unwrap();
        assert!(line.as_bytes()[mid - 1].is_ascii_alphanumeric() && line.as_bytes()[mid].is_ascii_alphanumeric());
        assert_eq!(cut_of(line, "start", 0), Some(8));
    }

    #[test]
    fn similarity_is_one_for_equal_and_falls_with_distance() {
        assert_eq!(similarity("abc", "abc"), 1.0);
        assert!(similarity("abcd", "abxd") > similarity("abcd", "wxyz"));
    }

    #[test]
    fn comments_imports_and_braces_are_not_replayed() {
        for l in ["    // a long comment line", "import java.util.List;", "    @Override", "    }", "short;"] {
            assert!(!eligible(l), "{l}");
        }
        assert!(eligible("        return repository.findById(id);"));
    }
}

/// How long the classic completion popup takes to answer on a real project: the same `IndexService::completion` the
/// editor calls, at the positions where it is opened while typing — after a `.` (members) and in the middle of a word.
/// `BENNU_QUALITY_PROJECT=<root> cargo test -p bennu-be flc_quality::the_popup -- --ignored --nocapture`
#[test]
#[ignore = "needs a real project: see the module docs"]
fn the_popup_latency_on_a_real_project() {
    let Some(root) = env("BENNU_QUALITY_PROJECT") else {
        eprintln!("BENNU_QUALITY_PROJECT not set: skipping");
        return;
    };
    let wanted: usize = env("BENNU_QUALITY_N").and_then(|v| v.parse().ok()).unwrap_or(300);
    let svc = IndexService::global();
    svc.open(&root, &env("BENNU_QUALITY_JDK").unwrap_or_else(|| "21".into()), "UTF-8", Arc::new(Quiet));
    let started = Instant::now();
    while !svc.has_resolver(&root) || !svc.has_semantic_engine(&root) {
        assert!(started.elapsed() < Duration::from_secs(600), "the index did not come up in 10 minutes");
        std::thread::sleep(Duration::from_millis(500));
    }
    let mut files = Vec::new();
    java_files(Path::new(&root), &mut files);
    files.sort();
    let mut samples: Vec<(usize, usize, bool)> = Vec::new(); // (file, offset, member)
    let texts: Vec<String> = files.iter().map(|f| std::fs::read_to_string(f).unwrap_or_default()).collect();
    for (fi, text) in texts.iter().enumerate() {
        let mut start = 0;
        for line in text.lines() {
            if eligible(line) {
                // after a dot, one typed letter in; and two letters into the first word after a space or `(`
                if let Some(d) = line.find('.').filter(|&d| d + 2 < line.len() && line.is_char_boundary(d + 2)) {
                    samples.push((fi, start + d + 2, true));
                }
                if let Some(w) = line.trim_start().find(' ').map(|w| line.len() - line.trim_start().len() + w + 3).filter(|&w| w < line.len() && line.is_char_boundary(w)) {
                    samples.push((fi, start + w, false));
                }
            }
            start += line.len() + 1;
        }
    }
    let stride = (samples.len() / wanted.max(1)).max(1);
    let (mut member, mut bare): (Vec<u128>, Vec<u128>) = (Vec::new(), Vec::new());
    for (fi, offset, is_member) in samples.iter().step_by(stride).take(wanted) {
        let source = texts[*fi].clone();
        let t = Instant::now();
        let _ = svc.completion(&files[*fi].to_string_lossy(), *offset, Some(&source), Default::default());
        let us = t.elapsed().as_micros();
        if *is_member { member.push(us) } else { bare.push(us) }
    }
    for (label, mut v) in [("after a dot", member), ("in a word", bare)] {
        v.sort();
        if v.is_empty() {
            continue;
        }
        eprintln!("{label:<12} n={} avg {:.1} ms, p50 {:.1} ms, p95 {:.1} ms, max {:.1} ms", v.len(), v.iter().sum::<u128>() as f64 / v.len() as f64 / 1000.0, v[v.len() / 2] as f64 / 1000.0, v[v.len() * 95 / 100] as f64 / 1000.0, *v.last().unwrap() as f64 / 1000.0);
    }
}
