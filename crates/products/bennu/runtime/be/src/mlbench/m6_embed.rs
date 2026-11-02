//! M6 — does a name similarity that *generalizes* (sub-word vectors learned from co-occurrences, no network) raise the
//! retrieval of M4's skeleton over the lexical one (weighted Jaccard)?
//!
//! The vectors: PPMI of (sub-word of a method name or of a signature type) × (context: sub-words of the methods it calls,
//! of the other identifiers it uses, of the types in its signature), reduced by a randomized SVD written by hand. They are
//! learned on the public TRAINING projects only — never on the held-out methods, so a query's own body cannot leak into
//! its vector. A method is the mean of its sub-word vectors (name ×2, signature types ×1) plus an arity component; the
//! similarity is the cosine. Three rankers are compared on the very same 2400 held-out methods as M4: lexical, vectors, and
//! `0.5·lexical + 0.5·cosine`.
//!
//! `cargo test --release -p bennu-be mlbench::m6_embed -- --ignored --nocapture`  (`BENNU_MLB_N`, `BENNU_MLB_DIM`)

use std::collections::{BTreeMap, HashMap};

use super::common::*;
use super::m4_methods::*;
use super::m5_ngram::tokens;

const KEYWORDS: &[&str] = &["return", "if", "else", "for", "while", "new", "this", "super", "null", "true", "false", "try", "catch", "throw", "throws", "final", "static", "int", "long", "boolean", "void", "char", "double", "float", "byte", "short", "instanceof", "case", "switch", "break", "continue", "do", "finally", "class", "default"];

fn simple_type(t: &str) -> &str {
    t.split('<').next().unwrap_or(t).trim_end_matches("[]").trim_end_matches("...").rsplit('.').next().unwrap_or("")
}

/// Words a method is *described by*: its name's sub-words and the sub-words of its signature types.
fn signature_words(m: &Method) -> (Vec<String>, Vec<String>) {
    let name = subwords(&m.name);
    let mut types = subwords(simple_type(&m.ret));
    for (t, _) in &m.params { types.extend(subwords(simple_type(t))) }
    (name, types)
}

/// The contexts a method offers to every word describing it.
fn contexts(m: &Method) -> Vec<String> {
    let mut out = Vec::new();
    for s in &m.statements {
        let toks = tokens(s);
        for (i, t) in toks.iter().enumerate() {
            let first = t.as_bytes()[0];
            if !(first.is_ascii_alphabetic() || first == b'_') || KEYWORDS.contains(t) { continue }
            let prefix = if toks.get(i + 1) == Some(&"(") { "c" } else { "f" };
            out.extend(subwords(t).into_iter().map(|w| format!("{prefix}:{w}")));
        }
    }
    for w in signature_words(m).1 { out.push(format!("t:{w}")) }
    out
}

pub(super) struct Embedding {
    index: HashMap<String, usize>,
    vectors: Vec<Vec<f32>>,
    dim: usize,
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    fn gauss(&mut self) -> f64 {
        let (a, b) = (self.next().max(1e-12), self.next());
        (-2.0 * a.ln()).sqrt() * (std::f64::consts::TAU * b).cos()
    }
}

/// Modified Gram-Schmidt on the columns of a row-major `rows × k` matrix.
fn orthonormalize(m: &mut [Vec<f64>], k: usize) {
    for j in 0..k {
        for p in 0..j {
            let dot: f64 = m.iter().map(|r| r[j] * r[p]).sum();
            for r in m.iter_mut() { r[j] -= dot * r[p] }
        }
        let norm = m.iter().map(|r| r[j] * r[j]).sum::<f64>().sqrt().max(1e-12);
        for r in m.iter_mut() { r[j] /= norm }
    }
}

/// Eigen-decomposition of a symmetric matrix by cyclic Jacobi: (eigenvalues, eigenvectors as columns of a row-major matrix).
fn jacobi(mut a: Vec<Vec<f64>>) -> (Vec<f64>, Vec<Vec<f64>>) {
    let n = a.len();
    let mut v: Vec<Vec<f64>> = (0..n).map(|i| (0..n).map(|j| f64::from(i == j)).collect()).collect();
    for _ in 0..40 {
        let off: f64 = (0..n).flat_map(|i| (0..n).filter(move |&j| j != i).map(move |j| (i, j))).map(|(i, j)| a[i][j] * a[i][j]).sum();
        if off < 1e-18 { break }
        for p in 0..n {
            for q in p + 1..n {
                if a[p][q].abs() < 1e-30 { continue }
                let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
                let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
                let (c, s) = (1.0 / (t * t + 1.0).sqrt(), t / (t * t + 1.0).sqrt());
                for k in 0..n {
                    let (akp, akq) = (a[k][p], a[k][q]);
                    a[k][p] = c * akp - s * akq;
                    a[k][q] = s * akp + c * akq;
                }
                for k in 0..n {
                    let (apk, aqk) = (a[p][k], a[q][k]);
                    a[p][k] = c * apk - s * aqk;
                    a[q][k] = s * apk + c * aqk;
                }
                for row in v.iter_mut() {
                    let (vkp, vkq) = (row[p], row[q]);
                    row[p] = c * vkp - s * vkq;
                    row[q] = s * vkp + c * vkq;
                }
            }
        }
    }
    ((0..n).map(|i| a[i][i]).collect(), v)
}

impl Embedding {
    /// Learn from `methods`: words seen at least `min_word` times, contexts at least `min_ctx` times.
    pub(super) fn train(methods: &[Method], dim: usize) -> Embedding {
        let (min_word, min_ctx) = (4usize, 3usize);
        let mut wc: HashMap<String, usize> = HashMap::new();
        let mut cc: HashMap<String, usize> = HashMap::new();
        let mut pairs: Vec<(String, Vec<String>)> = Vec::new();
        for m in methods {
            let (name, types) = signature_words(m);
            let mut words: Vec<String> = name.into_iter().chain(types).collect();
            words.sort();
            words.dedup();
            let ctx = contexts(m);
            for c in &ctx { *cc.entry(c.clone()).or_default() += 1 }
            for w in &words { *wc.entry(w.clone()).or_default() += 1 }
            for w in words { pairs.push((w, ctx.clone())) }
        }
        let mut vocab: Vec<&String> = wc.iter().filter(|(_, &n)| n >= min_word).map(|(w, _)| w).collect();
        vocab.sort();
        let index: HashMap<String, usize> = vocab.iter().enumerate().map(|(i, w)| ((*w).clone(), i)).collect();
        let mut cvocab: Vec<&String> = cc.iter().filter(|(_, &n)| n >= min_ctx).map(|(c, _)| c).collect();
        cvocab.sort();
        let cindex: HashMap<&String, usize> = cvocab.iter().enumerate().map(|(i, c)| (*c, i)).collect();
        let mut cells: Vec<HashMap<usize, f64>> = vec![HashMap::new(); index.len()];
        let (mut total, mut wtot) = (0.0f64, vec![0.0f64; index.len()]);
        let mut ctot = vec![0.0f64; cindex.len()];
        for (w, ctx) in &pairs {
            let Some(&wi) = index.get(w) else { continue };
            for c in ctx {
                if let Some(&ci) = cindex.get(c) {
                    *cells[wi].entry(ci).or_default() += 1.0;
                    wtot[wi] += 1.0;
                    ctot[ci] += 1.0;
                    total += 1.0;
                }
            }
        }
        // PPMI with the context distribution smoothed to the 0.75 power
        let smooth: f64 = ctot.iter().map(|c| c.powf(0.75)).sum();
        let ppmi: Vec<Vec<(usize, f64)>> = cells.iter().enumerate().map(|(wi, row)| {
            let mut r: Vec<(usize, f64)> = row.iter().filter_map(|(&ci, &n)| {
                let pmi = ((n / total) / ((wtot[wi] / total) * (ctot[ci].powf(0.75) / smooth))).ln();
                (pmi > 0.0).then_some((ci, pmi))
            }).collect();
            r.sort_by_key(|x| x.0);
            r
        }).collect();
        let (nw, nc) = (index.len(), cindex.len());
        let k = (dim + 8).min(nw).min(nc).max(1);
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        let omega: Vec<Vec<f64>> = (0..nc).map(|_| (0..k).map(|_| rng.gauss()).collect()).collect();
        let times = |x: &[Vec<f64>]| -> Vec<Vec<f64>> { ppmi.iter().map(|row| { let mut y = vec![0.0; k]; for &(c, v) in row { for j in 0..k { y[j] += v * x[c][j] } } y }).collect() };
        let times_t = |y: &[Vec<f64>]| -> Vec<Vec<f64>> { let mut z = vec![vec![0.0; k]; nc]; for (w, row) in ppmi.iter().enumerate() { for &(c, v) in row { for j in 0..k { z[c][j] += v * y[w][j] } } } z };
        let mut y = times(&omega);
        orthonormalize(&mut y, k);
        for _ in 0..3 {
            let mut z = times_t(&y);
            orthonormalize(&mut z, k);
            y = times(&z);
            orthonormalize(&mut y, k);
        }
        // B = Qᵀ M  (k × nc); G = B Bᵀ; the eigenvectors of G give the left singular vectors in Q's basis
        let b = times_t(&y); // nc × k, that is Bᵀ
        let mut g = vec![vec![0.0f64; k]; k];
        for row in &b { for i in 0..k { if row[i] != 0.0 { for j in 0..k { g[i][j] += row[i] * row[j] } } } }
        let (vals, vecs) = jacobi(g);
        let mut order: Vec<usize> = (0..k).collect();
        order.sort_by(|&a, &c| vals[c].total_cmp(&vals[a]));
        let d = dim.min(k);
        let vectors: Vec<Vec<f32>> = (0..nw).map(|w| {
            let mut e: Vec<f64> = order.iter().take(d).map(|&o| {
                let u: f64 = (0..k).map(|j| y[w][j] * vecs[j][o]).sum();
                u * vals[o].max(0.0).powf(0.25)
            }).collect();
            let norm = e.iter().map(|x| x * x).sum::<f64>().sqrt();
            if norm > 1e-12 { for x in e.iter_mut() { *x /= norm } }
            e.into_iter().map(|x| x as f32).collect()
        }).collect();
        eprintln!("M6: {nw} words × {nc} contexts, dim {d}");
        Embedding { index, vectors, dim: d }
    }

    pub(super) fn word(&self, w: &str) -> Option<&[f32]> {
        self.index.get(w).map(|&i| self.vectors[i].as_slice())
    }

    /// The method vector: unit-length, `dim` embedding components then a one-hot arity block (weight chosen so arity
    /// agreement is worth a quarter of the similarity).
    pub(super) fn method(&self, m: &Method) -> Vec<f32> {
        let (name, types) = signature_words(m);
        let mean = |ws: &[String]| -> Option<Vec<f32>> {
            let found: Vec<&[f32]> = ws.iter().filter_map(|w| self.word(w)).collect();
            if found.is_empty() { return None }
            let mut v = vec![0.0f32; self.dim];
            for f in &found { for (a, b) in v.iter_mut().zip(*f) { *a += b / found.len() as f32 } }
            Some(v)
        };
        let mut e = vec![0.0f32; self.dim];
        for (part, w) in [(mean(&name), 2.0f32), (mean(&types), 1.0)] {
            if let Some(p) = part { for (a, b) in e.iter_mut().zip(&p) { *a += w * b } }
        }
        let norm = e.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 1e-9 { for x in e.iter_mut() { *x /= norm } }
        for x in e.iter_mut() { *x *= 0.866 }
        let mut v = e;
        let mut arity = [0.0f32; 6];
        arity[m.params.len().min(5)] = 0.5;
        v.extend(arity);
        v
    }
}

fn dot(a: &[f32], b: &[f32]) -> f32 { a.iter().zip(b).map(|(x, y)| x * y).sum() }

fn render(m: &Method) -> String {
    let params = m.params.iter().map(|(t, n)| format!("{t} {n}")).collect::<Vec<_>>().join(", ");
    format!("{} {}({params}) {{ {} }}", m.ret, m.name, m.statements.join(" "))
}

#[derive(Default, Clone)]
struct Hits { n: usize, s1: usize, s5: usize, s20: usize, b20: usize }

impl Hits {
    fn add(&mut self, top: &[usize], c: &Corpus, sk: &str, bd: &str) {
        self.n += 1;
        self.s1 += usize::from(top.first().is_some_and(|&i| c.skeletons[i] == sk));
        self.s5 += usize::from(top.iter().take(5).any(|&i| c.skeletons[i] == sk));
        self.s20 += usize::from(top.iter().any(|&i| c.skeletons[i] == sk));
        self.b20 += usize::from(top.iter().any(|&i| c.bodies[i] == bd));
    }
    fn merge(&mut self, o: &Hits) { self.n += o.n; self.s1 += o.s1; self.s5 += o.s5; self.s20 += o.s20; self.b20 += o.b20 }
}

fn top_by(scores: &mut Vec<(usize, f32)>, k: usize) -> Vec<usize> {
    let k = k.min(scores.len());
    if k == 0 { return Vec::new() }
    scores.select_nth_unstable_by(k - 1, |a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    scores.truncate(k);
    scores.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    scores.iter().map(|x| x.0).collect()
}

struct Example { class: &'static str, size: usize, query: String, found: String, lexical_first: String }

#[test]
#[ignore = "needs the public corpus: see the module docs"]
fn m6_name_vectors_against_jaccard() {
    let wanted = env_num("BENNU_MLB_N", 800);
    let dim = env_num("BENNU_MLB_DIM", 64);
    let mut train = Corpus::empty();
    let mut file_ids = 0usize;
    for t in ["commons-io", "commons-collections", "mockito", "junit5", "jackson-databind"] {
        let root = super::m3_rank::assemble_training(t);
        let mut files = Vec::new();
        rust_files(&root, &mut files);
        files.sort();
        for f in files {
            file_ids += 1;
            for m in methods_of(&std::fs::read_to_string(&f).unwrap_or_default(), file_ids) { train.add(m) }
        }
    }
    let t0 = std::time::Instant::now();
    let emb = Embedding::train(&train.methods, dim);
    eprintln!("M6: {} training methods, embedding in {:.1}s", train.methods.len(), t0.elapsed().as_secs_f64());
    let train_vecs: Vec<Vec<f32>> = train.methods.iter().map(|m| emb.method(m)).collect();
    // sanity: neighbours of a few words
    let mut probe = String::new();
    for w in ["find", "customer", "load", "validate", "name"] {
        if let Some(v) = emb.word(w) {
            let mut near: Vec<(f32, &String)> = emb.index.iter().map(|(o, &i)| (dot(v, &emb.vectors[i]), o)).filter(|(_, o)| *o != w).collect();
            near.sort_by(|a, b| b.0.total_cmp(&a.0));
            probe.push_str(&format!("- `{w}` → {}\n", near.iter().take(6).map(|(s, o)| format!("{o} ({s:.2})")).collect::<Vec<_>>().join(", ")));
        }
    }
    eprintln!("{probe}");

    let names = ["lexical", "vectors", "combined"];
    let mut cells: BTreeMap<&'static str, [Hits; 3]> = BTreeMap::new();
    let mut examples: Vec<Example> = Vec::new();
    for spec in PROJECTS {
        let (_, held) = assemble(spec);
        let mut own = Corpus { methods: train.methods.clone(), skeletons: train.skeletons.clone(), bodies: train.bodies.clone(), inverted: train.inverted.clone(), sizes: train.sizes.clone() };
        let mut vecs = train_vecs.clone();
        let mut queries = Vec::new();
        for (hi, h) in held.iter().enumerate() {
            for m in methods_of(&h.text, 1_000_000 + hi) {
                queries.push(m.clone());
                vecs.push(emb.method(&m));
                own.add(m);
            }
        }
        let stride = (queries.len() / wanted.max(1)).max(1);
        for q in queries.iter().step_by(stride).take(wanted) {
            let qv = emb.method(q);
            let alive = |i: usize| { let m = &own.methods[i]; !(m.file == q.file && m.start == q.start) };
            // lexical: the same scoring as M4, kept as scores so the combination can use it
            let qf = features(q);
            let qsize: f32 = qf.iter().map(|f| f.1).sum();
            let mut inter: HashMap<u32, f32> = HashMap::new();
            for (f, w) in &qf {
                for &(id, cw) in own.inverted.get(f).map(Vec::as_slice).unwrap_or(&[]) { *inter.entry(id).or_default() += w.min(cw) }
            }
            let jac: HashMap<usize, f32> = inter.into_iter().map(|(id, x)| (id as usize, x / (qsize + own.sizes[id as usize] - x))).collect();
            let mut lex: Vec<(usize, f32)> = jac.iter().filter(|(&i, _)| alive(i)).map(|(&i, &s)| (i, s)).collect();
            let mut cos: Vec<(usize, f32)> = Vec::with_capacity(vecs.len());
            let mut com: Vec<(usize, f32)> = Vec::with_capacity(vecs.len());
            for (i, v) in vecs.iter().enumerate().filter(|(i, _)| alive(*i)) {
                let c = dot(&qv, v);
                cos.push((i, c));
                com.push((i, 0.5 * c + 0.5 * jac.get(&i).copied().unwrap_or(0.0)));
            }
            let tops = [top_by(&mut lex, 20), top_by(&mut cos, 20), top_by(&mut com, 20)];
            let (sk, bd) = (skeleton(q), body_key(q));
            let class = class_of(q);
            let row = cells.entry(class).or_default();
            for (h, t) in row.iter_mut().zip(&tops) { h.add(t, &own, &sk, &bd) }
            // an example: the vectors find the skeleton, the lexical top-20 does not, and the finder shares no name word
            let hit = tops[1].iter().copied().find(|&i| own.skeletons[i] == sk);
            if let Some(i) = hit {
                let lexical_hit = tops[0].iter().any(|&j| own.skeletons[j] == sk);
                let shared = subwords(&own.methods[i].name).iter().any(|w| subwords(&q.name).contains(w));
                if !lexical_hit && !shared && class != "getter" && class != "setter" {
                    examples.push(Example { class, size: render(q).len() + render(&own.methods[i]).len(), query: render(q), found: render(&own.methods[i]), lexical_first: tops[0].first().map(|&j| render(&own.methods[j])).unwrap_or_else(|| "(nothing)".into()) });
                }
            }
        }
    }

    let mut out = String::from("## M6 — name vectors against the lexical similarity (retrieval of M4's skeleton)\n\nWord neighbours learned (sanity):\n\n");
    out.push_str(&probe);
    out.push_str("\n| class | methods | skeleton top-1 lex / vec / comb | top-5 lex / vec / comb | top-20 lex / vec / comb | Δ top-20 vec−lex | Δ top-20 comb−lex | body verbatim top-20 lex / vec / comb |\n|---|---|---|---|---|---|---|---|\n");
    let line = |name: &str, c: &[Hits; 3]| {
        let p = |f: fn(&Hits) -> usize| format!("{:.1} / {:.1} / {:.1}", pct(f(&c[0]), c[0].n), pct(f(&c[1]), c[1].n), pct(f(&c[2]), c[2].n));
        format!("| {name} | {} | {} | {} | {} | {:+.1} | {:+.1} | {} |\n", c[0].n, p(|h| h.s1), p(|h| h.s5), p(|h| h.s20), pct(c[1].s20, c[1].n) - pct(c[0].s20, c[0].n), pct(c[2].s20, c[2].n) - pct(c[0].s20, c[0].n), p(|h| h.b20))
    };
    let (mut all, mut hard) = (<[Hits; 3]>::default(), <[Hits; 3]>::default());
    for (k, c) in &cells {
        out.push_str(&line(k, c));
        for i in 0..3 {
            all[i].merge(&c[i]);
            if ["conversion", "validation", "other"].contains(k) { hard[i].merge(&c[i]) }
        }
    }
    out.push_str(&line("**non-trivial (conversion, validation, other)**", &hard));
    out.push_str(&line("**all**", &all));
    let (d_all, d_hard) = (pct(all[2].s20, all[2].n).max(pct(all[1].s20, all[1].n)) - pct(all[0].s20, all[0].n), pct(hard[2].s20, hard[2].n).max(pct(hard[1].s20, hard[1].n)) - pct(hard[0].s20, hard[0].n));
    out.push_str(&format!("\nBest of vectors/combined minus lexical, skeleton in top-20: {d_all:+.1} points overall, {d_hard:+.1} on the non-trivial. Threshold: +5 / +10 → **{}**.\n", if d_all >= 5.0 || d_hard >= 10.0 { "worth training a real network" } else { "not worth it yet" }));
    let _ = names;
    examples.sort_by_key(|e| e.size);
    let mut seen = Vec::new();
    out.push_str("\n### Examples: vectors find a skeleton the lexical top-20 misses (no name word in common)\n\n");
    for e in &examples {
        if seen.contains(&e.class) { continue }
        seen.push(e.class);
        out.push_str(&format!("**{}**\n\n```java\n// query\n{}\n// vectors found\n{}\n// lexical top-1 was\n{}\n```\n\n", e.class, e.query, e.found, e.lexical_first));
        if seen.len() == 3 { break }
    }
    let path = write_result("m6_report.md", &out);
    eprintln!("{out}\nreport: {}", path.display());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jacobi_recovers_a_known_spectrum() {
        let (vals, _) = jacobi(vec![vec![2.0, 1.0], vec![1.0, 2.0]]);
        let mut v = vals.clone();
        v.sort_by(f64::total_cmp);
        assert!((v[0] - 1.0).abs() < 1e-9 && (v[1] - 3.0).abs() < 1e-9);
    }

    #[test]
    fn words_that_share_contexts_get_close_vectors() {
        // `find` and `load` call `query`/`row`; `render` calls `draw`/`paint`: find~load must beat find~render
        let methods: Vec<Method> = (0..12).flat_map(|i| methods_of(&format!("class A{i} {{ Object find(String k) {{ return query(k).row(); }} Object load(String k) {{ return query(k).row(); }} void render(String k) {{ draw(k); paint(k); }} }}"), i)).collect();
        assert_eq!(methods.len(), 36);
        let e = Embedding::train(&methods, 8);
        let (f, l, r) = (e.word("find").unwrap(), e.word("load").unwrap(), e.word("render").unwrap());
        assert!(dot(f, l) > dot(f, r), "find~load {} vs find~render {}", dot(f, l), dot(f, r));
    }

    #[test]
    fn a_method_vector_has_unit_length() {
        let methods: Vec<Method> = (0..12).flat_map(|i| methods_of(&format!("class A{i} {{ Object find(String k) {{ return query(k).row(); }} Object load(String k) {{ return query(k).row(); }} void render(String k) {{ draw(k); paint(k); }} }}"), i)).collect();
        let e = Embedding::train(&methods, 4);
        let v = e.method(&methods[0]);
        assert!((dot(&v, &v) - 1.0).abs() < 0.02, "{}", dot(&v, &v));
    }
}
