//! M7 — predicting the next statement of a method from the *structure already written*.
//!
//! A held-out method (the 2400 of M4) is cut after k = 1, 2, 3, … statements. The query is its signature plus the shapes of
//! those k statements (identifiers → `I`, literals → `L`). Retrieval takes the methods of the project and of the public
//! training projects that share the longest structural prefix *from the start of the method* (ties: the more similar signature,
//! M4's weighted Jaccard) and reads the shape of *their* statement k+1. Compared with
//!
//! - **solo-sopra** — what `flc_sequence` does with a key made only of the statement just above (here at the level of shapes: the
//!   most frequent successor of the last shape, counted at every position of the corpus);
//! - **n-gram of shapes** — the same table with the last 1, 2, 3 shapes, backing off from the longest context that was seen;
//!
//! and with the prefix capped at 1, 2, 3 statements, so that "how much comes from the long prefix" is read off directly.
//! The exact statement is then produced from the best neighbour with M4's `adapt` (names filled by parameter position, derived
//! property, `flc_rank` for free names) and compared to the real one. The baselines predict shapes only: no text is measured for them.
//! The query's own method never contributes to any table.
//!
//! `cargo test --release -p bennu-be mlbench::m7_prefix -- --ignored --nocapture`  (`BENNU_MLB_N` methods per project, default 800)

use std::collections::{BTreeMap, HashMap};

use super::common::*;
use super::m4_fill::{adapt, class_fields};
use super::m4_methods::{class_of, features, normalize, Corpus};
use super::m78_shared::*;

const BOS: u32 = 0;
type Key = (u8, [u32; 3]);

#[derive(Default)]
struct Interner(HashMap<String, u32>);

impl Interner {
    fn id(&mut self, s: &str) -> u32 {
        let next = self.0.len() as u32 + 1;
        *self.0.entry(s.to_string()).or_insert(next)
    }
    fn seq(&mut self, stmts: &[String]) -> Vec<u32> {
        stmts.iter().map(|s| self.id(&shape(s))).collect()
    }
}

/// Every (context, next shape) pair of a sequence, for the orders 1..=3, padded with `BOS` at the start.
fn pairs(seq: &[u32]) -> Vec<(Key, u32)> {
    let mut out = Vec::new();
    for p in 0..seq.len() {
        for order in 1..=3usize {
            let mut key = [BOS; 3];
            for j in 0..order {
                let at = p as isize - (order - j) as isize;
                key[3 - order + j] = if at < 0 { BOS } else { seq[at as usize] };
            }
            out.push(((order as u8, key), seq[p]));
        }
    }
    out
}

#[derive(Default)]
struct Table(HashMap<Key, HashMap<u32, u32>>);

impl Table {
    fn add(&mut self, seq: &[u32]) {
        for (k, next) in pairs(seq) { *self.0.entry(k).or_default().entry(next).or_default() += 1 }
    }

    /// Successors of the last `order` shapes of `prefix`, most frequent first, without what `own` contributed.
    /// `orders` is tried longest first; the first one with something left answers.
    fn predict(&self, prefix: &[u32], own: &HashMap<(Key, u32), u32>, orders: &[usize]) -> Vec<u32> {
        for &order in orders {
            let mut key = [BOS; 3];
            for j in 0..order {
                let at = prefix.len() as isize - (order - j) as isize;
                key[3 - order + j] = if at < 0 { BOS } else { prefix[at as usize] };
            }
            let key = (order as u8, key);
            let Some(m) = self.0.get(&key) else { continue };
            let mut v: Vec<(u32, u32)> = m.iter().map(|(&s, &n)| (s, n.saturating_sub(own.get(&(key, s)).copied().unwrap_or(0)))).filter(|x| x.1 > 0).collect();
            if v.is_empty() { continue }
            v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            return v.into_iter().map(|x| x.0).collect();
        }
        Vec::new()
    }
}

/// Neighbours ranked by (common prefix, capped; signature similarity). Returns the distinct next shapes (up to 5) and the best id.
fn rank(cands: &[(usize, usize, f32)], cap: usize, k: usize, seqs: &[Vec<u32>]) -> (Vec<u32>, Option<usize>, usize) {
    let mut v: Vec<(usize, usize, f32)> = cands.iter().map(|&(id, l, j)| (id, l.min(cap), j)).collect();
    let cmp = |a: &(usize, usize, f32), b: &(usize, usize, f32)| b.1.cmp(&a.1).then(b.2.total_cmp(&a.2)).then(a.0.cmp(&b.0));
    if v.len() > 300 {
        v.select_nth_unstable_by(299, cmp);
        v.truncate(300);
    }
    v.sort_by(cmp);
    let mut shapes: Vec<u32> = Vec::new();
    for &(id, ..) in &v {
        let s = seqs[id][k];
        if !shapes.contains(&s) { shapes.push(s) }
        if shapes.len() == 5 { break }
    }
    (shapes, v.first().map(|x| x.0), v.first().map_or(0, |x| x.1))
}

#[derive(Default, Clone)]
struct Acc {
    n: usize,
    prefix: [usize; 3],
    cap1: [usize; 3],
    cap2: [usize; 3],
    cap3: [usize; 3],
    above: [usize; 3],
    ngram: [usize; 3],
    exact: usize,
    exact_cap1: usize,
}

fn hit(list: &[u32], truth: u32, out: &mut [usize; 3]) {
    for (slot, k) in out.iter_mut().zip([1usize, 3, 5]) {
        *slot += usize::from(list.iter().take(k).any(|&s| s == truth));
    }
}

impl Acc {
    fn merge(&mut self, o: &Acc) {
        self.n += o.n;
        for (a, b) in [(&mut self.prefix, &o.prefix), (&mut self.cap1, &o.cap1), (&mut self.cap2, &o.cap2), (&mut self.cap3, &o.cap3), (&mut self.above, &o.above), (&mut self.ngram, &o.ngram)] {
            for i in 0..3 { a[i] += b[i] }
        }
        self.exact += o.exact;
        self.exact_cap1 += o.exact_cap1;
    }
}

fn triple(h: &[usize; 3], n: usize) -> String {
    format!("{:.1} / {:.1} / {:.1}", pct(h[0], n), pct(h[1], n), pct(h[2], n))
}

#[test]
#[ignore = "needs the public corpus: see the module docs"]
fn m7_next_statement_from_the_prefix() {
    let wanted = env_num("BENNU_MLB_N", 800);
    let mut shapes = Interner::default();
    let train = train_bodies(8);
    let train_seqs: Vec<Vec<u32>> = train.iter().map(|b| shapes.seq(&b.m.statements)).collect();
    eprintln!("M7: {} training methods", train.len());
    let mut by_k: BTreeMap<usize, Acc> = BTreeMap::new();
    let mut by_class: BTreeMap<&'static str, Acc> = BTreeMap::new();
    let mut by_l: BTreeMap<&'static str, (usize, usize)> = BTreeMap::new();
    let (mut all, mut late) = (Acc::default(), Acc::default());
    for spec in PROJECTS {
        let (_, held) = assemble(spec);
        let mut own = Corpus::empty();
        let mut seqs = train_seqs.clone();
        for b in &train { own.add(b.m.clone()) }
        let mut queries = Vec::new();
        for (hi, h) in held.iter().enumerate() {
            for b in bodies_of(&h.text, 1_000_000 + hi, 8) {
                seqs.push(shapes.seq(&b.m.statements));
                own.add(b.m.clone());
                queries.push((hi, b));
            }
        }
        let first_of: HashMap<u32, Vec<usize>> = seqs.iter().enumerate().fold(HashMap::new(), |mut m, (i, s)| { m.entry(s[0]).or_default().push(i); m });
        let mut table = Table::default();
        for s in &seqs { table.add(s) }
        let stride = (queries.len() / wanted.max(1)).max(1);
        for (qi, (hi, qb)) in queries.iter().enumerate().step_by(stride).take(wanted) {
            let q = &qb.m;
            let n = q.statements.len();
            if n < 2 { continue }
            let qseq = shapes.seq(&q.statements);
            let me = train.len() + qi;
            debug_assert_eq!(own.methods[me].start, q.start);
            let mut own_pairs: HashMap<(Key, u32), u32> = HashMap::new();
            for p in pairs(&qseq) { *own_pairs.entry(p).or_default() += 1 }
            // signature similarity, M4's weighted Jaccard
            let qf = features(q);
            let qsize: f32 = qf.iter().map(|f| f.1).sum();
            let mut inter: HashMap<u32, f32> = HashMap::new();
            for (f, w) in &qf {
                for &(id, cw) in own.inverted.get(f).map(Vec::as_slice).unwrap_or(&[]) { *inter.entry(id).or_default() += w.min(cw) }
            }
            let jac = |id: usize| inter.get(&(id as u32)).map_or(0.0, |x| x / (qsize + own.sizes[id] - x));
            let mut ids: Vec<usize> = inter.keys().map(|&i| i as usize).collect();
            ids.extend(first_of.get(&qseq[0]).into_iter().flatten().copied().filter(|i| !inter.contains_key(&(*i as u32))));
            let fields = class_fields(&held[*hi].text, q.start);
            let class = class_of(q);
            for k in 1..n {
                let truth = qseq[k];
                let cands: Vec<(usize, usize, f32)> = ids
                    .iter()
                    .copied()
                    .filter(|&i| i != me && seqs[i].len() > k)
                    .map(|i| (i, seqs[i].iter().zip(&qseq[..k]).take_while(|(a, b)| a == b).count(), jac(i)))
                    .collect();
                let mut a = Acc { n: 1, ..Acc::default() };
                let (full, best, best_l) = rank(&cands, usize::MAX, k, &seqs);
                hit(&full, truth, &mut a.prefix);
                hit(&rank(&cands, 1, k, &seqs).0, truth, &mut a.cap1);
                hit(&rank(&cands, 2, k, &seqs).0, truth, &mut a.cap2);
                hit(&rank(&cands, 3, k, &seqs).0, truth, &mut a.cap3);
                hit(&table.predict(&qseq[..k], &own_pairs, &[1]), truth, &mut a.above);
                hit(&table.predict(&qseq[..k], &own_pairs, &[3, 2, 1]), truth, &mut a.ngram);
                let exact_of = |id: Option<usize>| id.is_some_and(|id| adapt(&own.methods[id], q, &fields).get(k).is_some_and(|s| normalize(s) == normalize(&q.statements[k])));
                a.exact = usize::from(exact_of(best));
                a.exact_cap1 = usize::from(exact_of(rank(&cands, 1, k, &seqs).1));
                let bucket = if best.is_none() { "no neighbour" } else if best_l >= k { "whole prefix shared" } else if best_l >= 1 { "part of the prefix shared" } else { "nothing shared" };
                let e = by_l.entry(bucket).or_default();
                e.0 += 1;
                e.1 += usize::from(full.first() == Some(&truth));
                by_k.entry(k).or_default().merge(&a);
                by_class.entry(class).or_default().merge(&a);
                all.merge(&a);
                if k >= 3 { late.merge(&a) }
            }
        }
    }
    let header = "| | cases | long prefix top-1 / 3 / 5 | statement above only | n-gram of shapes | exact statement after filling (best neighbour) |\n|---|---|---|---|---|---|\n";
    let row = |name: &str, a: &Acc| format!("| {name} | {} | {} | {} | {} | {:.1}% |\n", a.n, triple(&a.prefix, a.n), triple(&a.above, a.n), triple(&a.ngram, a.n), pct(a.exact, a.n));
    let mut out = String::from("## M7 — next statement from the structural prefix\n\nCorrect *shape* of statement k+1: first / within 3 / within 5 (%).\n\n### By number of statements already written\n\n");
    out.push_str(header);
    for (k, a) in &by_k { out.push_str(&row(&format!("k = {k}"), a)) }
    out.push_str(&row("**all**", &all));
    out.push_str("\n### By class of method (all k)\n\n");
    out.push_str(header);
    for (c, a) in &by_class { out.push_str(&row(c, a)) }
    out.push_str("\n### How much comes from the long prefix (top-1 shape / top-5 shape, the prefix used for ranking capped at …)\n\n| prefix used | all k | k ≥ 3 |\n|---|---|---|\n");
    let cap_row = |name: &str, f: fn(&Acc) -> [usize; 3]| format!("| {name} | {:.1} / {:.1} | {:.1} / {:.1} |\n", pct(f(&all)[0], all.n), pct(f(&all)[2], all.n), pct(f(&late)[0], late.n), pct(f(&late)[2], late.n));
    out.push_str(&cap_row("statement above only (table)", |a| a.above));
    out.push_str(&cap_row("n-gram of shapes (table)", |a| a.ngram));
    out.push_str(&cap_row("first statement only", |a| a.cap1));
    out.push_str(&cap_row("first 2 statements", |a| a.cap2));
    out.push_str(&cap_row("first 3 statements", |a| a.cap3));
    out.push_str(&cap_row("whole prefix", |a| a.prefix));
    out.push_str(&format!("\nExact statement after filling, best neighbour: whole prefix {:.1}%, first statement only {:.1}%.\n", pct(all.exact, all.n), pct(all.exact_cap1, all.n)));
    out.push_str("\n### Hit rate of the top-1 shape by how much of the prefix the best neighbour shares\n\n| best neighbour | cases | top-1 shape |\n|---|---|---|\n");
    for (b, (n, h)) in &by_l { out.push_str(&format!("| {b} | {n} | {:.1}% |\n", pct(*h, *n))) }
    let path = write_result("m7_report.md", &out);
    eprintln!("{out}\nreport: {}", path.display());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairs_cover_the_three_orders_with_padding() {
        let p = pairs(&[5, 6]);
        assert_eq!(p.len(), 6);
        assert!(p.contains(&((3, [BOS, BOS, 5]), 6)));
        assert!(p.contains(&((1, [BOS, BOS, BOS]), 5)));
    }

    #[test]
    fn a_table_forgets_what_the_query_itself_taught_it() {
        let mut t = Table::default();
        t.add(&[1, 2]);
        t.add(&[1, 3]);
        t.add(&[1, 3]);
        assert_eq!(t.predict(&[1], &HashMap::new(), &[1]), vec![3, 2]);
        let own2: HashMap<(Key, u32), u32> = pairs(&[1, 2]).into_iter().fold(HashMap::new(), |mut m, p| { *m.entry(p).or_default() += 1; m });
        assert_eq!(t.predict(&[1], &own2, &[1]), vec![3]);
    }

    #[test]
    fn the_neighbour_with_the_longer_shared_prefix_wins_and_the_cap_flattens_it() {
        let seqs = vec![vec![1, 2, 9], vec![1, 7, 8], vec![4, 4, 4]];
        let cands = vec![(0usize, 2usize, 0.1f32), (1, 1, 0.9)];
        assert_eq!(rank(&cands, usize::MAX, 2, &seqs).0[0], 9);
        assert_eq!(rank(&cands, 1, 2, &seqs).0[0], 8);
    }
}
