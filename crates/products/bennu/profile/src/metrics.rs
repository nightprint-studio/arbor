//! Per-file metrics for the editor: which declaration in *this buffer* is which method of the
//! recording.
//!
//! Matching is by fully-qualified name, not by line, so a recording stays useful while the file is
//! edited: the declarations come from the buffer as it is now (the caller parses it), and the
//! numbers come from the recording. Whether the file changed after the recording is the caller's
//! to judge (it knows modification times); this module only matches.

use std::collections::HashMap;

use serde::Serialize;

use crate::model::*;

/// A method declared in the buffer.
#[derive(Debug, Clone)]
pub struct DeclaredMethod {
    /// Dotted fully-qualified class (`a.b.Outer.Inner`).
    pub class: String,
    pub name: String,
    pub arity: u32,
    /// 1-based declaration line.
    pub line: u32,
    pub is_constructor: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MethodMetric {
    pub line: u32,
    pub name: String,
    pub self_pct: f64,
    pub total_pct: f64,
    pub self_ms: f64,
    pub total_ms: f64,
    pub alloc_bytes: u64,
    pub alloc_count: u64,
    /// `self_pct` relative to the hottest method of the whole recording, 0..1 — what the gutter
    /// shading scales by.
    pub heat: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct LineMetric {
    pub line: u32,
    pub pct: f64,
    pub heat: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct FileMetrics {
    pub methods: Vec<MethodMetric>,
    pub lines: Vec<LineMetric>,
}

/// `a.b.Outer$Inner` → `a.b.Outer.Inner`.
pub fn dotted_class(binary: &str) -> String {
    binary.replace('$', ".")
}

/// `lambda$main$0` → `main`; anything else unchanged.
fn unlambda(method: &str) -> Option<&str> {
    method.strip_prefix("lambda$")?.rsplit_once('$').map(|(n, _)| n)
}

fn simple(class: &str) -> &str {
    class.rsplit('.').next().unwrap_or(class)
}

#[derive(Default, Clone, Copy)]
struct Acc {
    self_w: u64,
    total_w: u64,
}

/// Metrics of a Java file given its declared methods.
pub fn java_file_metrics(p: &Profile, declared: &[DeclaredMethod], min_line_pct: f64) -> FileMetrics {
    if p.tree.total == 0 || declared.is_empty() {
        return FileMetrics::default();
    }
    // (class, name, arity) with arity None = lambda bodies (their arity says nothing about the
    // enclosing method's).
    let mut by_arity: HashMap<(String, String, Option<u32>), Acc> = HashMap::new();
    for m in &p.methods {
        let class = dotted_class(&m.class);
        let (name, arity) = match unlambda(&m.method) {
            Some(n) => (n.to_string(), None),
            None if m.method == "<init>" => (simple(&class).to_string(), m.arity),
            None => (m.method.clone(), m.arity),
        };
        let e = by_arity.entry((class, name, arity)).or_default();
        e.self_w += m.self_w;
        e.total_w += m.total_w;
    }
    let mut alloc: HashMap<(String, String), (u64, u64)> = HashMap::new();
    for a in &p.alloc_sites {
        if let Some((c, m)) = a.name.rsplit_once('.') {
            let e = alloc.entry((dotted_class(c), m.to_string())).or_default();
            e.0 += a.bytes;
            e.1 += a.count;
        }
    }
    let max_self = p.methods.iter().map(|m| m.self_w).max().unwrap_or(1).max(1) as f64;
    let total = p.tree.total as f64;

    // Allocation sites carry no descriptor: with overloads declared in the same file the bytes
    // cannot be told apart, so they are attributed only to an unambiguous name.
    let mut overloads: HashMap<(&str, &str), u32> = HashMap::new();
    for d in declared {
        *overloads.entry((d.class.as_str(), d.name.as_str())).or_insert(0) += 1;
    }

    let mut methods = Vec::new();
    for d in declared {
        let name = if d.is_constructor { simple(&d.class).to_string() } else { d.name.clone() };
        let exact = by_arity.get(&(d.class.clone(), name.clone(), Some(d.arity))).copied().unwrap_or_default();
        let lambdas = by_arity.get(&(d.class.clone(), name.clone(), None)).copied().unwrap_or_default();
        // A lambda body's time is the enclosing method's own (its self adds up); its total is
        // normally already inside the method's total, so the larger of the two stands.
        let acc = Acc { self_w: exact.self_w + lambdas.self_w, total_w: exact.total_w.max(lambdas.total_w) };
        let alloc_key_name = if d.is_constructor { "<init>".to_string() } else { d.name.clone() };
        let unambiguous = overloads.get(&(d.class.as_str(), d.name.as_str())).copied() == Some(1);
        let (alloc_bytes, alloc_count) = if unambiguous {
            alloc.get(&(d.class.clone(), alloc_key_name)).copied().unwrap_or((0, 0))
        } else {
            (0, 0)
        };
        if acc.total_w == 0 && alloc_bytes == 0 {
            continue;
        }
        methods.push(MethodMetric {
            line: d.line,
            name: d.name.clone(),
            self_pct: acc.self_w as f64 * 100.0 / total,
            total_pct: acc.total_w as f64 * 100.0 / total,
            self_ms: p.ms(acc.self_w),
            total_ms: p.ms(acc.total_w),
            alloc_bytes,
            alloc_count,
            heat: acc.self_w as f64 / max_self,
        });
    }

    let classes: std::collections::HashSet<&str> = declared.iter().map(|d| d.class.as_str()).collect();
    let mut line_w: HashMap<u32, u64> = HashMap::new();
    for h in &p.tree.hot_lines {
        let f = &p.tree.frames[h.frame as usize];
        if classes.contains(dotted_class(&f.class).as_str()) {
            *line_w.entry(h.line).or_insert(0) += h.weight;
        }
    }
    FileMetrics { methods, lines: line_metrics(line_w, total, min_line_pct) }
}

fn line_metrics(line_w: HashMap<u32, u64>, total: f64, min_pct: f64) -> Vec<LineMetric> {
    let max = line_w.values().copied().max().unwrap_or(1).max(1) as f64;
    let mut v: Vec<LineMetric> = line_w
        .into_iter()
        .map(|(line, w)| LineMetric { line, pct: w as f64 * 100.0 / total, heat: w as f64 / max })
        .filter(|l| l.pct >= min_pct)
        .collect();
    v.sort_by_key(|l| l.line);
    v
}

fn norm_path(p: &str) -> String {
    let s = p.replace('\\', "/");
    if cfg!(windows) { s.to_ascii_lowercase() } else { s }
}

/// Metrics of a file whose frames carry their own file and declaration line (Node, Rust).
pub fn path_file_metrics(p: &Profile, path: &str, min_line_pct: f64) -> FileMetrics {
    if p.tree.total == 0 {
        return FileMetrics::default();
    }
    let want = norm_path(path);
    let total = p.tree.total as f64;
    let max_self = p.methods.iter().map(|m| m.self_w).max().unwrap_or(1).max(1) as f64;
    let methods = p
        .methods
        .iter()
        .filter(|m| m.file.as_deref().map(norm_path).as_deref() == Some(want.as_str()))
        .filter_map(|m| {
            Some(MethodMetric {
                line: m.decl_line.or_else(|| m.hot_lines.first().map(|h| h.0))?,
                name: m.method.clone(),
                self_pct: m.self_w as f64 * 100.0 / total,
                total_pct: m.total_w as f64 * 100.0 / total,
                self_ms: p.ms(m.self_w),
                total_ms: p.ms(m.total_w),
                alloc_bytes: 0,
                alloc_count: 0,
                heat: m.self_w as f64 / max_self,
            })
        })
        .collect();
    let mut line_w: HashMap<u32, u64> = HashMap::new();
    for h in &p.tree.hot_lines {
        let f = &p.tree.frames[h.frame as usize];
        if f.file.as_deref().map(norm_path).as_deref() == Some(want.as_str()) {
            *line_w.entry(h.line).or_insert(0) += h.weight;
        }
    }
    FileMetrics { methods, lines: line_metrics(line_w, total, min_line_pct) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::{RawFrame, TreeBuilder};

    fn profile() -> Profile {
        let mut b = TreeBuilder::default();
        let hot = RawFrame { descriptor: Some("(I)V"), line: Some(42), ..RawFrame::method("a.b.Outer$Inner", "spin") };
        let lam = RawFrame { line: Some(50), ..RawFrame::method("a.b.Outer", "lambda$run$0") };
        let run = RawFrame { descriptor: Some("()V"), line: Some(49), ..RawFrame::method("a.b.Outer", "run") };
        b.add_stack(Some("t"), &[hot, run], 6);
        b.add_stack(Some("t"), &[lam, run], 2);
        let tree = b.finish();
        Profile {
            id: "p".into(),
            lang: Lang::Java,
            label: "p".into(),
            created_ms: 0,
            duration_ms: None,
            ms_per_unit: 10.0,
            methods: tree.method_stats(100),
            tree,
            alloc_sites: vec![AllocStat { name: "a.b.Outer$Inner.spin".into(), count: 3, bytes: 300 }],
            alloc_types: vec![],
            locks: vec![],
            gc: vec![],
            threads: vec![],
            notes: vec![],
        }
    }

    fn decl(class: &str, name: &str, arity: u32, line: u32) -> DeclaredMethod {
        DeclaredMethod { class: class.into(), name: name.into(), arity, line, is_constructor: false }
    }

    #[test]
    fn declarations_match_by_fqn_name_and_arity_across_nesting_and_lambdas() {
        let p = profile();
        let m = java_file_metrics(
            &p,
            &[decl("a.b.Outer", "run", 0, 48), decl("a.b.Outer.Inner", "spin", 1, 40)],
            0.0,
        );
        let overloaded = java_file_metrics(
            &p,
            &[decl("a.b.Outer.Inner", "spin", 1, 40), decl("a.b.Outer.Inner", "spin", 2, 60)],
            0.0,
        );
        assert_eq!(overloaded.methods.len(), 1, "the 2-arg overload was never called");
        assert_eq!(overloaded.methods[0].alloc_bytes, 0, "ambiguous allocation is not guessed");
        let run = m.methods.iter().find(|x| x.name == "run").unwrap();
        // 2 self samples from the lambda body (attributed to `run`), all 8 in total.
        assert!((run.total_pct - 100.0).abs() < 1e-9);
        assert!((run.self_pct - 25.0).abs() < 1e-9);
        let spins: Vec<_> = m.methods.iter().filter(|x| x.name == "spin").collect();
        assert_eq!(spins.len(), 1, "the 2-arg overload was never called");
        assert_eq!(spins[0].line, 40);
        assert_eq!(spins[0].alloc_bytes, 300);
        assert!((spins[0].heat - 1.0).abs() < 1e-9);
        assert!(m.lines.iter().any(|l| l.line == 42 && (l.heat - 0.75).abs() < 1e-9));
    }

    #[test]
    fn nothing_matches_a_foreign_file() {
        assert!(java_file_metrics(&profile(), &[decl("x.Y", "z", 0, 1)], 0.0).methods.is_empty());
    }
}
