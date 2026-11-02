//! M5 — a stack with no neural model: how much of what the ONNX model gets right does a token n-gram copy model get right?
//!
//! (a) the last n tokens before the caret → what followed them, counted over the public training projects and the project's
//! own lines (the sampled line itself is taken out of the counts it is asked about). (b) the *form* of the line — identifiers
//! → `I`, literals → `L` — predicted the same way; filling the holes is not built here, so (b) is a ceiling on form agreement.
//!
//! The samples are M1's (same draw); the cascade's and the model's answers come from `results/<tag>_m1_rows.tsv`, so run
//! M1 first. `cargo test -p bennu-be mlbench::m5_ngram -- --ignored --nocapture` (`BENNU_MLB_M1ROWS=<tsv>`).

use std::collections::HashMap;
use std::path::Path;

use super::common::*;
use super::m1_cascade::{eligible, prefix_share, split_point};

const KEYWORDS: &[&str] = &[
    "abstract", "assert", "boolean", "break", "byte", "case", "catch", "char", "class", "continue", "default", "do", "double", "else", "enum", "extends", "final", "finally", "float", "for", "if", "implements", "import", "instanceof", "int", "interface", "long", "new", "null", "package", "private", "protected", "public", "return", "short", "static", "super", "switch", "synchronized", "this", "throw", "throws", "try", "void", "volatile", "while", "true", "false", "var",
];

/// Tokens of a line: identifiers, numbers, string/char literals (one token), and single punctuation characters.
pub(super) fn tokens(line: &str) -> Vec<&str> {
    let b = line.as_bytes();
    let (mut out, mut i) = (Vec::new(), 0);
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_whitespace() {
            i += 1;
        } else if c.is_ascii_alphabetic() || c == b'_' || c == b'$' {
            let s = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'$') { i += 1 }
            out.push(&line[s..i]);
        } else if c.is_ascii_digit() {
            let s = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'.' || b[i] == b'_') { i += 1 }
            out.push(&line[s..i]);
        } else if c == b'"' || c == b'\'' {
            let s = i;
            i += 1;
            while i < b.len() && b[i] != c {
                i += if b[i] == b'\\' { 2 } else { 1 };
            }
            i = (i + 1).min(b.len());
            out.push(&line[s..i]);
        } else {
            // a non-ascii byte run is one token; ascii punctuation is one character
            let s = i;
            i += 1;
            while i < b.len() && !b[i].is_ascii() { i += 1 }
            out.push(&line[s..i]);
        }
    }
    out
}

/// Identifiers → `I`, numbers and strings → `L`, keywords and punctuation as they are.
pub(super) fn abstract_token(t: &str) -> &str {
    let c = t.as_bytes()[0];
    if c == b'"' || c == b'\'' || c.is_ascii_digit() {
        "L"
    } else if (c.is_ascii_alphabetic() || c == b'_' || c == b'$') && !KEYWORDS.contains(&t) {
        "I"
    } else {
        t
    }
}

fn hash_ctx<'a>(toks: impl Iterator<Item = &'a str>) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for t in toks {
        for b in t.bytes().chain(std::iter::once(1u8)) {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    h
}

const ORDERS: [usize; 2] = [5, 3];

/// `context hash → [(continuation id, count)]`, one table per order; continuations are interned strings.
#[derive(Default)]
pub(super) struct Ngram {
    tables: [HashMap<u64, Vec<(u32, u32)>>; 2],
    ids: HashMap<String, u32>,
    texts: Vec<String>,
    /// Same, over abstract tokens: the continuation is the abstract form of the rest of the line.
    form_tables: [HashMap<u64, Vec<(u32, u32)>>; 2],
}

fn bump(table: &mut HashMap<u64, Vec<(u32, u32)>>, key: u64, cont: u32) {
    let v = table.entry(key).or_default();
    match v.iter_mut().find(|(c, _)| *c == cont) {
        Some(e) => e.1 += 1,
        None => v.push((cont, 1)),
    }
}

impl Ngram {
    fn intern(&mut self, s: &str) -> u32 {
        if let Some(&i) = self.ids.get(s) { return i }
        let i = self.texts.len() as u32;
        self.texts.push(s.to_string());
        self.ids.insert(s.to_string(), i);
        i
    }

    /// Counts every word-start of `line` the way a caret lands there (`cut_points` of M1).
    pub(super) fn add_line(&mut self, line: &str) {
        let b = line.as_bytes();
        let indent = line.len() - line.trim_start().len();
        for i in indent + 8..line.len().saturating_sub(8) {
            if !(matches!(b[i - 1], b' ' | b'(' | b'.' | b',') && b[i] != b' ') || !line.is_char_boundary(i) {
                continue;
            }
            self.add_at(&line[..i], line[i..].trim_end());
        }
    }

    fn add_at(&mut self, prefix: &str, rest: &str) {
        let toks = tokens(prefix);
        let abs: Vec<&str> = toks.iter().map(|t| abstract_token(t)).collect();
        let abs_rest = tokens(rest).iter().map(|t| abstract_token(t)).collect::<Vec<_>>().join(" ");
        let (cont, form) = (self.intern(rest), self.intern(&abs_rest));
        for (k, n) in ORDERS.iter().enumerate() {
            if toks.len() >= *n {
                bump(&mut self.tables[k], hash_ctx(toks[toks.len() - n..].iter().copied()), cont);
                bump(&mut self.form_tables[k], hash_ctx(abs[abs.len() - n..].iter().copied()), form);
            }
        }
    }

    /// The most frequent continuation of `prefix`: (text, share of the context's sightings, sightings). `own` takes out the
    /// sighting that the asked line itself contributed.
    pub(super) fn predict(&self, prefix: &str, own: Option<&str>, form: bool) -> Option<(String, f32, u32)> {
        let toks = tokens(prefix);
        let abs: Vec<&str> = toks.iter().map(|t| abstract_token(t)).collect();
        let own_id = own.and_then(|o| {
            let key = if form { tokens(o).iter().map(|t| abstract_token(t)).collect::<Vec<_>>().join(" ") } else { o.to_string() };
            self.ids.get(&key).copied()
        });
        for (k, n) in ORDERS.iter().enumerate() {
            if toks.len() < *n {
                continue;
            }
            let (table, seq) = if form { (&self.form_tables[k], &abs) } else { (&self.tables[k], &toks) };
            let Some(list) = table.get(&hash_ctx(seq[seq.len() - n..].iter().copied())) else { continue };
            let adjusted: Vec<(u32, u32)> = list.iter().map(|&(c, cnt)| (c, if Some(c) == own_id { cnt.saturating_sub(1) } else { cnt })).filter(|&(_, cnt)| cnt > 0).collect();
            let total: u32 = adjusted.iter().map(|x| x.1).sum();
            if total < 2 {
                continue;
            }
            // the most frequent; the continuation's text breaks a tie so the answer does not depend on hash order
            let &(best, count) = adjusted.iter().max_by(|a, b| a.1.cmp(&b.1).then_with(|| self.texts[b.0 as usize].cmp(&self.texts[a.0 as usize]))).unwrap();
            return Some((self.texts[best as usize].clone(), count as f32 / total as f32, total));
        }
        None
    }
}

fn category(line: &str) -> &'static str {
    let calls = line.matches('.').count().min(line.matches('(').count());
    if line.contains("->") || line.contains("::") { "lambda / method ref" } else if calls >= 2 { "chain" } else if line.contains('(') { "call" } else { "other" }
}

fn java_lines(dir: &Path, out: &mut Vec<String>, cap: usize) {
    let mut files = Vec::new();
    fn walk(d: &Path, out: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() { walk(&p, out) } else if p.extension().is_some_and(|x| x == "java") { out.push(p) }
        }
    }
    walk(dir, &mut files);
    files.sort();
    for f in files {
        for l in std::fs::read_to_string(&f).unwrap_or_default().lines() {
            if out.len() >= cap { return }
            if eligible(l) { out.push(l.to_string()) }
        }
    }
}

struct M1Row {
    layer: String,
    exact: bool,
    got: String,
}

fn read_m1_rows() -> HashMap<String, M1Row> {
    let path = env("BENNU_MLB_M1ROWS").map(std::path::PathBuf::from).unwrap_or_else(|| work_dir().join("results/full_m1_rows.tsv"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("run M1 first with BENNU_MLB_TAG=full ({})", path.display()));
    text.lines()
        .skip(1)
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            (f.len() >= 10).then(|| (format!("{}|{}", f[0], f[7]), M1Row { layer: f[1].to_string(), exact: f[3] == "1", got: f[9].to_string() }))
        })
        .collect()
}

#[test]
#[ignore = "needs M1's rows and the public corpus: see the module docs"]
fn m5_ngram_instead_of_the_model() {
    let m1 = read_m1_rows();
    let train: Vec<&str> = ["commons-io", "commons-collections", "mockito", "junit5", "jackson-databind"].to_vec();
    let mut index = Ngram::default();
    let mut lines = Vec::new();
    for t in &train {
        let root = super::m3_rank::assemble_training(t);
        java_lines(&root, &mut lines, usize::MAX);
    }
    let per_project_cap = env_num("BENNU_MLB_NGRAM_LINES", 200_000);
    lines.truncate(per_project_cap * 3 / 2);
    eprintln!("n-gram: {} public training lines", lines.len());
    for l in &lines {
        index.add_line(l);
    }
    // the project's own lines: held-out files of every project, and the dependency trees of the project asked about
    let mut samples = Vec::new(); // (project, place, prefix, expected, category)
    for spec in PROJECTS {
        let (root, held) = assemble(spec);
        let mut dep_lines = Vec::new();
        java_lines(&root.join("deps"), &mut dep_lines, 150_000);
        for l in &dep_lines {
            index.add_line(l);
        }
        for h in &held {
            let fname = h.path.file_name().unwrap().to_string_lossy().into_owned();
            let (mut taken, mut offset) = (0usize, 0usize);
            for (line_no, line) in h.text.split('\n').enumerate() {
                let start = offset;
                offset += line.len() + 1;
                index.add_line(line);
                let mut rng = line_seed(&format!("{}/{fname}", h.source), line_no);
                if taken >= 4 || (rng >> 33) % 11 != 0 || !eligible(line) {
                    continue;
                }
                let Some(cut) = split_point(line, &mut rng) else { continue };
                let _ = start;
                taken += 1;
                samples.push((spec.name.to_string(), format!("{fname}:{line_no}:{cut}"), line[..cut].to_string(), line[cut..].trim_end().to_string()));
            }
        }
    }
    eprintln!("n-gram index built; {} samples", samples.len());

    #[derive(Clone)]
    struct Ans { shown: bool, exact: bool, prefix: f64, conf: f32 }
    let predict = |s: &(String, String, String, String)| -> Ans {
        match index.predict(&s.2, Some(&s.3), false) {
            Some((t, conf, _)) => Ans { shown: true, exact: t == s.3, prefix: prefix_share(&t, &s.3), conf },
            None => Ans { shown: false, exact: false, prefix: 0.0, conf: 0.0 },
        }
    };
    let answers: Vec<Ans> = samples.iter().map(predict).collect();
    let rows: Vec<Option<&M1Row>> = samples.iter().map(|s| m1.get(&format!("{}|{}", s.0, s.1))).collect();
    let model_exact: Vec<usize> = (0..samples.len()).filter(|&i| rows[i].is_some_and(|r| r.layer == "model" && r.exact)).collect();
    let model_shown: usize = rows.iter().flatten().filter(|r| r.layer == "model").count();
    let model_precision = pct(model_exact.len(), model_shown);

    let n = samples.len();
    let mut out = format!("## M5 — token n-gram instead of the ONNX model ({n} samples)\n\nThe model's slice in M1: {model_shown} answers, {} exact ({model_precision:.1}%).\n\n", model_exact.len());
    out.push_str("### (a) n-gram copy model alone, by confidence threshold\n\n| confidence ≥ | shown | coverage | exact of shown | prefix of asked | covers of the model's exact lines | exact lines the model did not get |\n|---|---|---|---|---|---|---|\n");
    let mut parity: Option<f32> = None;
    for th in [0.3f32, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9] {
        let sel: Vec<usize> = (0..n).filter(|&i| answers[i].shown && answers[i].conf >= th).collect();
        let ex = sel.iter().filter(|&&i| answers[i].exact).count();
        let covered = model_exact.iter().filter(|&&i| answers[i].shown && answers[i].conf >= th && answers[i].exact).count();
        let extra = sel.iter().filter(|&&i| answers[i].exact && !model_exact.contains(&i)).count();
        let pf: f64 = sel.iter().map(|&i| answers[i].prefix).sum::<f64>() / n as f64 * 100.0;
        if parity.is_none() && pct(ex, sel.len()) >= model_precision { parity = Some(th) }
        out.push_str(&format!("| {th:.1} | {} | {:.1}% | {:.1}% | {pf:.1}% | {covered} / {} ({:.1}%) | {extra} |\n", sel.len(), pct(sel.len(), n), pct(ex, sel.len()), model_exact.len(), pct(covered, model_exact.len())));
    }
    out.push_str(&format!("\nThe first threshold at which the n-gram's precision reaches the model's ({model_precision:.1}%): {}.\n", parity.map_or("none of those tried".to_string(), |t| format!("{t:.1}"))));

    // the stack: the cascade's non-model layers, then the n-gram where they are silent
    out.push_str("\n### A stack with no neural model: deterministic layers, then the n-gram\n\n| configuration | shown | coverage | exact | exact of shown |\n|---|---|---|---|---|\n");
    let cascade_full: (usize, usize) = rows.iter().flatten().fold((0, 0), |a, r| (a.0 + 1, a.1 + usize::from(r.exact)));
    let det: (usize, usize) = rows.iter().flatten().filter(|r| r.layer != "model").fold((0, 0), |a, r| (a.0 + 1, a.1 + usize::from(r.exact)));
    out.push_str(&format!("| full cascade with the model | {} | {:.1}% | {} | {:.1}% |\n| deterministic layers only | {} | {:.1}% | {} | {:.1}% |\n", cascade_full.0, pct(cascade_full.0, n), cascade_full.1, pct(cascade_full.1, cascade_full.0), det.0, pct(det.0, n), det.1, pct(det.1, det.0)));
    for th in [0.5f32, 0.7, 0.9] {
        let (mut shown, mut exact) = (0usize, 0usize);
        for i in 0..n {
            match rows[i] {
                Some(r) if r.layer != "model" => { shown += 1; exact += usize::from(r.exact) }
                _ if answers[i].shown && answers[i].conf >= th => { shown += 1; exact += usize::from(answers[i].exact) }
                _ => {}
            }
        }
        out.push_str(&format!("| deterministic + n-gram ≥ {th:.1} | {shown} | {:.1}% | {exact} | {:.1}% |\n", pct(shown, n), pct(exact, shown)));
    }

    // what stays with the model alone
    out.push_str("\n### The model's exact lines the n-gram (at parity) does not reproduce\n\n| kind of line | model-exact lines | reproduced by the n-gram | left to the model |\n|---|---|---|---|\n");
    let th = parity.unwrap_or(0.7);
    let mut kinds: std::collections::BTreeMap<&str, (usize, usize)> = Default::default();
    for &i in &model_exact {
        let e = kinds.entry(category(&samples[i].3)).or_default();
        e.0 += 1;
        e.1 += usize::from(answers[i].shown && answers[i].conf >= th && answers[i].exact);
    }
    for (k, (all, cov)) in &kinds {
        out.push_str(&format!("| {k} | {all} | {cov} ({:.1}%) | {} |\n", pct(*cov, *all), all - cov));
    }
    let lit = model_exact.iter().filter(|&&i| samples[i].3.contains('"')).count();
    let long = model_exact.iter().filter(|&&i| samples[i].3.len() >= 40).count();
    out.push_str(&format!("\nOf the model's exact lines, {lit} contain a string literal and {long} are 40+ characters after the caret.\n"));

    // (b) form
    out.push_str("\n### (b) form only: does the abstract n-gram predict the shape of the rest of the line?\n\n");
    let (mut asked_form, mut hit_form) = (0usize, 0usize);
    for &i in &model_exact {
        let abs_expected = tokens(&samples[i].3).iter().map(|t| abstract_token(t)).collect::<Vec<_>>().join(" ");
        if let Some((form, _, _)) = index.predict(&samples[i].2, Some(&samples[i].3), true) {
            asked_form += 1;
            hit_form += usize::from(form == abs_expected);
        }
    }
    out.push_str(&format!("For the model's {} exact lines the abstract n-gram answers on {asked_form} and names the right shape on {hit_form} ({:.1}% of them). Filling the holes with the type-driven enumerator of M2 is not built, so this is the ceiling of form + fill, not a measured stack.\n", model_exact.len(), pct(hit_form, model_exact.len())));

    // the same copy model with nothing but the project's own held-out files behind it (no public training lines, no dependency
    // sources): what a freshly opened project has — and the setting the production layer (`flc_ngram`) runs in
    out.push_str("\n### The same, learning only from the project's own files\n\n| confidence ≥ | shown | exact of shown | prefix of asked |\n|---|---|---|---|\n");
    let mut own_answers: Vec<Ans> = Vec::new();
    for spec in PROJECTS {
        let (_, held) = assemble(spec);
        let mut own = Ngram::default();
        for h in &held {
            for line in h.text.split('\n') {
                own.add_line(line);
            }
        }
        for sm in samples.iter().filter(|x| spec.sources.iter().any(|src| x.0 == spec.name || x.1.starts_with(src))) {
            if sm.0 != spec.name { continue }
            own_answers.push(match own.predict(&sm.2, Some(&sm.3), false) {
                Some((t, conf, _)) => Ans { shown: true, exact: t == sm.3, prefix: prefix_share(&t, &sm.3), conf },
                None => Ans { shown: false, exact: false, prefix: 0.0, conf: 0.0 },
            });
        }
    }
    // one row per sample, to join with the cascade's rows: which layer answered the places where the copy model is right
    let mut dump = String::from("project\tplace\tshown\tconf\texact\n");
    let mut k = 0usize;
    for spec in PROJECTS {
        for sm in samples.iter().filter(|x| x.0 == spec.name) {
            let a = &own_answers[k];
            k += 1;
            dump.push_str(&format!("{}\t{}\t{}\t{:.3}\t{}\n", sm.0, sm.1, u8::from(a.shown), a.conf, u8::from(a.exact)));
        }
    }
    write_result("m5_own_answers.tsv", &dump);
    for th in [0.5f32, 0.7, 0.9, 0.99] {
        let sel: Vec<&Ans> = own_answers.iter().filter(|a| a.shown && a.conf >= th).collect();
        let ex = sel.iter().filter(|a| a.exact).count();
        let pf: f64 = sel.iter().map(|a| a.prefix).sum::<f64>() / own_answers.len().max(1) as f64 * 100.0;
        out.push_str(&format!("| {th:.2} | {} | {:.1}% | {pf:.1}% |\n", sel.len(), pct(ex, sel.len())));
    }
    let path = write_result("m5_report.md", &out);
    eprintln!("{out}\nreport: {}", path.display());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_keep_strings_whole_and_drop_blanks() {
        assert_eq!(tokens("foo.bar(\"a b\", 12);"), vec!["foo", ".", "bar", "(", "\"a b\"", ",", "12", ")", ";"]);
        assert_eq!(abstract_token("foo"), "I");
        assert_eq!(abstract_token("return"), "return");
        assert_eq!(abstract_token("\"x\""), "L");
        assert_eq!(abstract_token("42"), "L");
    }

    #[test]
    fn it_copies_what_followed_the_same_context_and_takes_its_own_sighting_out() {
        let mut g = Ngram::default();
        for _ in 0..3 {
            g.add_line("        final Foo value = repository.findById(identifier).orElseThrow();");
        }
        let prefix = "        final Foo value = repository.";
        let (text, share, seen) = g.predict(prefix, None, false).unwrap();
        assert_eq!(text, "findById(identifier).orElseThrow();");
        assert_eq!((share, seen), (1.0, 3));
        // asked about one of the three lines: two sightings remain, still enough
        assert_eq!(g.predict(prefix, Some("findById(identifier).orElseThrow();"), false).unwrap().2, 2);
        // a single sighting is not enough
        let mut once = Ngram::default();
        once.add_line("        final Foo value = repository.findById(identifier).orElseThrow();");
        assert!(once.predict(prefix, None, false).is_none());
    }

    #[test]
    fn the_form_ignores_names_and_literals() {
        let mut g = Ngram::default();
        g.add_line("        final Foo alpha = repository.findById(first, \"x\");");
        g.add_line("        final Bar beta = repository.findById(second, \"y\");");
        let (form, _, _) = g.predict("        final Baz gamma = repository.", None, true).unwrap();
        assert_eq!(form, "I ( I , L ) ;");
    }
}
