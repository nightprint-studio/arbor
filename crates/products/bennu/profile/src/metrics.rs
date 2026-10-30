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
    /// Bytes allocated from this method's own code — outside any other project method it called. From the
    /// recording's allocation stacks when it kept them, otherwise from the allocation site alone.
    pub alloc_bytes: u64,
    pub alloc_count: u64,
    /// `alloc_bytes` as a share of everything the recording allocated, in percent.
    pub alloc_pct: f64,
    /// The type most of `alloc_bytes` went to (`byte[]`), when the recording kept allocation stacks; empty otherwise.
    pub alloc_top_type: String,
    /// Milliseconds threads spent blocked in this method or what it called, off the CPU: waiting on a socket, a file,
    /// a park. The execution sampler is blind to it, which is why a method that waits on the database shows no CPU.
    pub wait_ms: f64,
    /// What it waited on longest (`socket db.internal:5432`); empty when it did not wait.
    pub wait_on: String,
    /// Whether the recording caught the method at all. No CPU sample, no wait and no allocation reads as "not
    /// sampled" — which is not the same as fast: a sampler taking a stack every ~10 ms does not see a method shorter
    /// than that, nor one that only ever waits.
    pub sampled: bool,
    /// `self_pct` relative to the hottest method of the whole recording, 0..1 — what the gutter
    /// shading scales by.
    pub heat: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct LineMetric {
    pub line: u32,
    /// Share of the CPU samples on this line.
    pub pct: f64,
    /// 0..1, the strongest of the line's CPU, allocation and wait shares against the hottest line of each — what the
    /// gutter bar scales by.
    pub heat: f64,
    /// Bytes allocated from this line, including what it calls.
    pub alloc_bytes: u64,
    /// Share of everything the recording allocated.
    pub alloc_pct: f64,
    /// Milliseconds threads were blocked on this line, including what it calls.
    pub wait_ms: f64,
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

/// What a declaration is matched by: dotted class, name, and arity — `None` for a lambda body, whose arity says
/// nothing about the enclosing method's.
type MethodKey = (String, String, Option<u32>);

fn method_key(class: &str, method: &str, arity: Option<u32>) -> MethodKey {
    let class = dotted_class(class);
    if let Some(name) = unlambda(method) {
        return (class, name.to_string(), None);
    }
    if method == "<init>" {
        let name = simple(&class).to_string();
        return (class, name, arity);
    }
    (class, method.to_string(), arity)
}

/// Weight given to one method by a side tree, and what it went to (the tips: allocated types, wait targets).
#[derive(Default)]
struct Attr {
    weight: u64,
    by_tip: HashMap<String, u64>,
}

fn credit(out: &mut HashMap<MethodKey, Attr>, key: &MethodKey, weight: u64, tip: &str) {
    let a = out.entry(key.clone()).or_default();
    a.weight += weight;
    *a.by_tip.entry(tip.to_string()).or_insert(0) += weight;
}

/// Walk every tip of a side tree up its stack and give the tip's weight to the methods on the way.
///
/// With `nearest` set the weight goes to the **first** method (nearest the tip) the predicate accepts and stops there:
/// that is "whose code asked for this allocation", skipping the JDK and the framework in between. Without it every
/// distinct method on the stack is credited, which is a total — a method waits as long as anything it calls waits.
fn attribute(tree: &CallTree, nearest: Option<&dyn Fn(&str) -> bool>) -> HashMap<MethodKey, Attr> {
    let keys: Vec<MethodKey> = tree.frames.iter().map(|f| method_key(&f.class, &f.method, f.arity())).collect();
    let mut out: HashMap<MethodKey, Attr> = HashMap::new();
    for node in &tree.nodes {
        if node.self_w == 0 {
            continue;
        }
        let tip = &tree.frames[node.frame as usize];
        if tip.kind != FrameKind::Native || !tip.class.is_empty() {
            continue;
        }
        let mut seen: Vec<u32> = Vec::new();
        let mut cur = node.parent;
        while cur != NO_PARENT {
            let n = &tree.nodes[cur as usize];
            let f = &tree.frames[n.frame as usize];
            let frame_ix = n.frame;
            cur = n.parent;
            if f.kind != FrameKind::Method {
                continue;
            }
            match nearest {
                Some(is_project) => {
                    if is_project(&f.class) {
                        credit(&mut out, &keys[frame_ix as usize], node.self_w, &tip.method);
                        break;
                    }
                }
                None => {
                    if !seen.iter().any(|s| keys[*s as usize] == keys[frame_ix as usize]) {
                        seen.push(frame_ix);
                        credit(&mut out, &keys[frame_ix as usize], node.self_w, &tip.method);
                    }
                }
            }
        }
    }
    out
}

/// The weight a declaration earned from a side tree (its exact arity plus its lambda bodies) and the tip most of it went to.
fn lookup(m: &HashMap<MethodKey, Attr>, exact: &MethodKey, lambda: &MethodKey) -> (u64, String) {
    let mut weight = 0;
    let mut tips: HashMap<&str, u64> = HashMap::new();
    for key in [exact, lambda] {
        if let Some(a) = m.get(key) {
            weight += a.weight;
            for (tip, n) in &a.by_tip {
                *tips.entry(tip.as_str()).or_insert(0) += n;
            }
        }
    }
    let top = tips.into_iter().max_by_key(|(_, n)| *n).map(|(t, _)| t.to_string()).unwrap_or_default();
    (weight, top)
}

/// Weight per source line of the frames whose class is declared in this file.
fn lines_of(tree: &CallTree, classes: &std::collections::HashSet<&str>) -> HashMap<u32, u64> {
    let mut out: HashMap<u32, u64> = HashMap::new();
    for h in &tree.hot_lines {
        let f = &tree.frames[h.frame as usize];
        if classes.contains(dotted_class(&f.class).as_str()) {
            *out.entry(h.line).or_insert(0) += h.weight;
        }
    }
    out
}

/// Metrics of a Java file given its declared methods.
///
/// Every declared method gets an entry: one the recording never caught is marked `sampled: false` rather than left
/// out, so the editor can say "not sampled" instead of being silent about it. `is_project` says whether a class (a
/// binary name, nested ones with `$`) is the project's own; it is how an allocation is credited to the nearest
/// method of the project rather than to the `Arrays.copyOf` that performed it.
pub fn java_file_metrics(
    p: &Profile,
    declared: &[DeclaredMethod],
    min_line_pct: f64,
    is_project: &dyn Fn(&str) -> bool,
) -> FileMetrics {
    let nothing = p.tree.total == 0 && p.alloc_tree.total == 0 && p.wait_tree.total == 0 && p.alloc_sites.is_empty();
    if nothing || declared.is_empty() {
        return FileMetrics::default();
    }
    let mut by_arity: HashMap<MethodKey, Acc> = HashMap::new();
    for m in &p.methods {
        let e = by_arity.entry(method_key(&m.class, &m.method, m.arity)).or_default();
        e.self_w += m.self_w;
        e.total_w += m.total_w;
    }
    let alloc_attr = (p.alloc_tree.total > 0).then(|| attribute(&p.alloc_tree, Some(is_project)));
    let wait_attr = (p.wait_tree.total > 0).then(|| attribute(&p.wait_tree, None));

    // A recording from before allocation stacks were kept has only the site that did the allocating — no
    // descriptor — so with overloads declared in the same file the bytes cannot be told apart and are credited only
    // to an unambiguous name.
    let mut legacy_alloc: HashMap<(String, String), (u64, u64)> = HashMap::new();
    if alloc_attr.is_none() {
        for a in &p.alloc_sites {
            if let Some((c, m)) = a.name.rsplit_once('.') {
                let e = legacy_alloc.entry((dotted_class(c), m.to_string())).or_default();
                e.0 += a.bytes;
                e.1 += a.count;
            }
        }
    }
    let mut overloads: HashMap<(&str, &str), u32> = HashMap::new();
    for d in declared {
        *overloads.entry((d.class.as_str(), d.name.as_str())).or_insert(0) += 1;
    }

    let max_self = p.methods.iter().map(|m| m.self_w).max().unwrap_or(1).max(1) as f64;
    let total = p.tree.total.max(1) as f64;
    let alloc_total = if p.alloc_tree.total > 0 { p.alloc_tree.total } else { p.alloc_sites.iter().map(|a| a.bytes).sum() };

    let mut methods = Vec::new();
    for d in declared {
        let name = if d.is_constructor { simple(&d.class).to_string() } else { d.name.clone() };
        let exact_key: MethodKey = (d.class.clone(), name.clone(), Some(d.arity));
        let lambda_key: MethodKey = (d.class.clone(), name, None);
        let exact = by_arity.get(&exact_key).copied().unwrap_or_default();
        let lambdas = by_arity.get(&lambda_key).copied().unwrap_or_default();
        // A lambda body's time is the enclosing method's own (its self adds up); its total is
        // normally already inside the method's total, so the larger of the two stands.
        let acc = Acc { self_w: exact.self_w + lambdas.self_w, total_w: exact.total_w.max(lambdas.total_w) };

        let (alloc_bytes, alloc_count, alloc_top_type) = match &alloc_attr {
            Some(attr) => {
                let (bytes, top) = lookup(attr, &exact_key, &lambda_key);
                (bytes, 0, top)
            }
            None => {
                let key_name = if d.is_constructor { "<init>".to_string() } else { d.name.clone() };
                let unambiguous = overloads.get(&(d.class.as_str(), d.name.as_str())).copied() == Some(1);
                let (b, c) = if unambiguous { legacy_alloc.get(&(d.class.clone(), key_name)).copied().unwrap_or((0, 0)) } else { (0, 0) };
                (b, c, String::new())
            }
        };
        let (wait_w, wait_on) = match &wait_attr {
            Some(attr) => lookup(attr, &exact_key, &lambda_key),
            None => (0, String::new()),
        };
        methods.push(MethodMetric {
            line: d.line,
            name: d.name.clone(),
            self_pct: acc.self_w as f64 * 100.0 / total,
            total_pct: acc.total_w as f64 * 100.0 / total,
            self_ms: p.ms(acc.self_w),
            total_ms: p.ms(acc.total_w),
            alloc_bytes,
            alloc_count,
            alloc_pct: alloc_bytes as f64 * 100.0 / alloc_total.max(1) as f64,
            alloc_top_type,
            // The wait tree weighs milliseconds directly.
            wait_ms: wait_w as f64,
            wait_on,
            sampled: acc.total_w > 0 || alloc_bytes > 0 || wait_w > 0,
            heat: acc.self_w as f64 / max_self,
        });
    }

    let classes: std::collections::HashSet<&str> = declared.iter().map(|d| d.class.as_str()).collect();
    let lines = merge_lines(
        lines_of(&p.tree, &classes),
        lines_of(&p.alloc_tree, &classes),
        lines_of(&p.wait_tree, &classes),
        total,
        alloc_total.max(1) as f64,
        min_line_pct,
    );
    FileMetrics { methods, lines }
}

/// One entry per line that stands out in CPU, allocation or waiting. A line is kept when any of the three clears
/// the threshold (a wait of 10 ms or more always does: JFR reports no shorter ones).
fn merge_lines(
    exec: HashMap<u32, u64>,
    alloc: HashMap<u32, u64>,
    wait: HashMap<u32, u64>,
    exec_total: f64,
    alloc_total: f64,
    min_pct: f64,
) -> Vec<LineMetric> {
    let max_exec = exec.values().copied().max().unwrap_or(1).max(1) as f64;
    let max_alloc = alloc.values().copied().max().unwrap_or(1).max(1) as f64;
    let max_wait = wait.values().copied().max().unwrap_or(1).max(1) as f64;
    let mut lines: Vec<u32> = exec.keys().chain(alloc.keys()).chain(wait.keys()).copied().collect();
    lines.sort_unstable();
    lines.dedup();
    lines
        .into_iter()
        .filter_map(|line| {
            let e = exec.get(&line).copied().unwrap_or(0);
            let a = alloc.get(&line).copied().unwrap_or(0);
            let w = wait.get(&line).copied().unwrap_or(0);
            let pct = e as f64 * 100.0 / exec_total;
            let alloc_pct = a as f64 * 100.0 / alloc_total;
            if pct < min_pct && alloc_pct < min_pct && w < 10 {
                return None;
            }
            let heat = (e as f64 / max_exec).max(a as f64 / max_alloc).max(w as f64 / max_wait);
            Some(LineMetric { line, pct, heat, alloc_bytes: a, alloc_pct, wait_ms: w as f64 })
        })
        .collect()
}

fn line_metrics(line_w: HashMap<u32, u64>, total: f64, min_pct: f64) -> Vec<LineMetric> {
    merge_lines(line_w, HashMap::new(), HashMap::new(), total, 1.0, min_pct)
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
                alloc_pct: 0.0,
                alloc_top_type: String::new(),
                wait_ms: 0.0,
                wait_on: String::new(),
                sampled: true,
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
            alloc_tree: Default::default(),
            wait_tree: Default::default(),
            waits: vec![],
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
            &|_| false,
        );
        let overloaded = java_file_metrics(
            &p,
            &[decl("a.b.Outer.Inner", "spin", 1, 40), decl("a.b.Outer.Inner", "spin", 2, 60)],
            0.0,
            &|_| false,
        );
        assert_eq!(overloaded.methods.len(), 2, "every declared method has an entry");
        assert!(overloaded.methods[0].sampled);
        assert!(!overloaded.methods[1].sampled, "the 2-arg overload was never called");
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
    fn a_foreign_file_is_all_unsampled() {
        let m = java_file_metrics(&profile(), &[decl("x.Y", "z", 0, 1)], 0.0, &|_| false);
        assert_eq!(m.methods.len(), 1);
        assert!(!m.methods[0].sampled);
        assert!(m.lines.is_empty());
    }

    #[test]
    fn allocations_go_to_the_nearest_project_method_and_waits_to_every_caller() {
        let mut p = profile();
        // Service.load (project) → Arrays.copyOf (JDK) → [byte[]]: 4096 bytes.
        let mut a = TreeBuilder::default();
        let tip = RawFrame { kind: FrameKind::Native, ..RawFrame::method("", "byte[]") };
        let copy = RawFrame::method("java.util.Arrays", "copyOf");
        let load = RawFrame { descriptor: Some("()V"), line: Some(33), ..RawFrame::method("a.b.Service", "load") };
        a.add_stack(None, &[tip, copy, load], 4096);
        p.alloc_tree = a.finish();
        // Controller.get → Service.load → Socket.read → [socket db:5432]: 250 ms.
        let mut w = TreeBuilder::default();
        let wait_tip = RawFrame { kind: FrameKind::Native, ..RawFrame::method("", "socket db:5432") };
        let read = RawFrame::method("java.net.Socket", "read");
        let load = RawFrame { descriptor: Some("()V"), line: Some(31), ..RawFrame::method("a.b.Service", "load") };
        let get = RawFrame { descriptor: Some("()V"), line: Some(12), ..RawFrame::method("a.b.Controller", "get") };
        w.add_stack(Some("t"), &[wait_tip, read, load, get], 250);
        p.wait_tree = w.finish();

        let is_project = |c: &str| c.starts_with("a.b.");
        let m = java_file_metrics(
            &p,
            &[decl("a.b.Service", "load", 0, 30), decl("a.b.Controller", "get", 0, 10)],
            0.0,
            &is_project,
        );
        let load = m.methods.iter().find(|x| x.name == "load").unwrap();
        assert_eq!(load.alloc_bytes, 4096, "credited to the project method, not to Arrays.copyOf");
        assert_eq!(load.alloc_top_type, "byte[]");
        assert!((load.wait_ms - 250.0).abs() < 1e-9);
        assert_eq!(load.wait_on, "socket db:5432");
        let get = m.methods.iter().find(|x| x.name == "get").unwrap();
        assert!((get.wait_ms - 250.0).abs() < 1e-9, "a caller waits as long as what it calls");
        assert_eq!(get.alloc_bytes, 0, "allocation stops at the nearest project method");
        assert!(get.sampled, "a method that only waits is still seen");
        assert!(m.lines.iter().any(|l| l.line == 31 && (l.wait_ms - 250.0).abs() < 1e-9));
        assert!(m.lines.iter().any(|l| l.line == 33 && l.alloc_bytes == 4096));
    }
}
