//! Streaming construction of a [`CallTree`]: stacks go in one at a time, the tree grows, nothing
//! per-sample is retained.

use std::collections::HashMap;

use crate::model::*;

/// Default node budget. A real application's profile folds into well under this; a pathological
/// one (deep recursion with distinct frames, generated code) is truncated rather than allowed to
/// eat memory, and the tree says so.
pub const DEFAULT_MAX_NODES: usize = 400_000;

/// A frame as a parser sees it, leaf-first in a stack.
#[derive(Debug, Clone, Copy)]
pub struct RawFrame<'a> {
    pub class: &'a str,
    pub method: &'a str,
    pub descriptor: Option<&'a str>,
    pub file: Option<&'a str>,
    /// The line *executing* in this frame (sample line), for the gutter heat.
    pub line: Option<u32>,
    /// The line the function is declared on, if the runtime says.
    pub decl_line: Option<u32>,
    pub kind: FrameKind,
}

impl<'a> RawFrame<'a> {
    pub fn method(class: &'a str, method: &'a str) -> Self {
        RawFrame { class, method, descriptor: None, file: None, line: None, decl_line: None, kind: FrameKind::Method }
    }
}

/// Accumulates stacks into a tree.
pub struct TreeBuilder {
    frames: Vec<FrameInfo>,
    frame_ix: HashMap<String, u32>,
    nodes: Vec<Node>,
    child_ix: HashMap<(u32, u32), u32>,
    lines: HashMap<(u32, u32), u64>,
    max_nodes: usize,
    capped: bool,
    key_buf: String,
}

impl Default for TreeBuilder {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_NODES)
    }
}

impl TreeBuilder {
    pub fn new(max_nodes: usize) -> Self {
        let root = FrameInfo {
            class: String::new(),
            method: "all".into(),
            descriptor: None,
            file: None,
            decl_line: None,
            kind: FrameKind::Root,
        };
        TreeBuilder {
            frames: vec![root],
            frame_ix: HashMap::new(),
            nodes: vec![Node { frame: 0, parent: NO_PARENT, self_w: 0, total: 0, children: Vec::new() }],
            child_ix: HashMap::new(),
            lines: HashMap::new(),
            max_nodes: max_nodes.max(2),
            capped: false,
            key_buf: String::new(),
        }
    }

    pub fn total(&self) -> u64 {
        self.nodes[0].total
    }

    pub fn capped(&self) -> bool {
        self.capped
    }

    fn intern(&mut self, f: &RawFrame) -> u32 {
        self.key_buf.clear();
        self.key_buf.push_str(f.class);
        self.key_buf.push('\u{0}');
        self.key_buf.push_str(f.method);
        self.key_buf.push('\u{0}');
        self.key_buf.push_str(f.descriptor.unwrap_or(""));
        self.key_buf.push('\u{0}');
        self.key_buf.push_str(f.file.unwrap_or(""));
        if let Some(&id) = self.frame_ix.get(&self.key_buf) {
            return id;
        }
        let id = self.frames.len() as u32;
        self.frames.push(FrameInfo {
            class: f.class.to_string(),
            method: f.method.to_string(),
            descriptor: f.descriptor.map(str::to_string),
            file: f.file.map(str::to_string),
            decl_line: f.decl_line,
            kind: f.kind,
        });
        self.frame_ix.insert(self.key_buf.clone(), id);
        id
    }

    fn child(&mut self, parent: u32, frame: u32) -> Option<u32> {
        if let Some(&n) = self.child_ix.get(&(parent, frame)) {
            return Some(n);
        }
        if self.nodes.len() >= self.max_nodes {
            self.capped = true;
            return None;
        }
        let n = self.nodes.len() as u32;
        self.nodes.push(Node { frame, parent, self_w: 0, total: 0, children: Vec::new() });
        self.nodes[parent as usize].children.push(n);
        self.child_ix.insert((parent, frame), n);
        Some(n)
    }

    /// Add one stack, **leaf first** (the order JFR and V8 report), optionally under a thread
    /// layer. `weight` is how many samples (or time units) it stands for.
    pub fn add_stack(&mut self, thread: Option<&str>, leaf_first: &[RawFrame], weight: u64) {
        if weight == 0 {
            return;
        }
        let mut cur = 0u32;
        self.nodes[0].total += weight;
        if let Some(t) = thread {
            let tf = self.intern(&RawFrame {
                class: "",
                method: t,
                descriptor: None,
                file: None,
                line: None,
                decl_line: None,
                kind: FrameKind::Thread,
            });
            match self.child(cur, tf) {
                Some(n) => {
                    cur = n;
                    self.nodes[n as usize].total += weight;
                }
                None => {
                    self.nodes[0].self_w += weight;
                    return;
                }
            }
        }
        for f in leaf_first.iter().rev() {
            let fid = self.intern(f);
            let Some(n) = self.child(cur, fid) else {
                // Budget spent: the remainder of this stack is attributed to the last node kept.
                self.nodes[cur as usize].self_w += weight;
                return;
            };
            cur = n;
            self.nodes[n as usize].total += weight;
            // Every frame's line counts: the leaf's is where time is spent, a caller's is the
            // call site the time went through — both are worth shading in the gutter.
            if let Some(line) = f.line {
                *self.lines.entry((fid, line)).or_insert(0) += weight;
            }
        }
        self.nodes[cur as usize].self_w += weight;
    }

    /// Freeze into a tree.
    pub fn finish(self) -> CallTree {
        let total = self.nodes[0].total;
        let mut hot: Vec<HotLine> = self
            .lines
            .into_iter()
            .map(|((frame, line), weight)| HotLine { frame, line, weight })
            .collect();
        hot.sort_by(|a, b| b.weight.cmp(&a.weight));
        hot.truncate(20_000);
        CallTree { frames: self.frames, nodes: self.nodes, hot_lines: hot, total, capped: self.capped }
    }
}

impl CallTree {
    /// Per-method self/total weight. Total does not double-count recursion: a method's weight is
    /// added only at its outermost activation on each path.
    pub fn method_stats(&self, max_rows: usize) -> Vec<MethodStat> {
        let nf = self.frames.len();
        let mut self_w = vec![0u64; nf];
        let mut total_w = vec![0u64; nf];
        // Iterative DFS with an "active" counter per frame.
        let mut active = vec![0u32; nf];
        let mut stack: Vec<(u32, usize)> = vec![(0, 0)];
        while let Some((n, ci)) = stack.pop() {
            let node = &self.nodes[n as usize];
            let f = node.frame as usize;
            if ci == 0 {
                self_w[f] += node.self_w;
                if active[f] == 0 {
                    total_w[f] += node.total;
                }
                active[f] += 1;
            }
            if ci < node.children.len() {
                stack.push((n, ci + 1));
                stack.push((node.children[ci], 0));
            } else {
                active[f] -= 1;
            }
        }
        let mut lines: HashMap<u32, Vec<(u32, u64)>> = HashMap::new();
        for h in &self.hot_lines {
            lines.entry(h.frame).or_default().push((h.line, h.weight));
        }
        let mut out: Vec<MethodStat> = (0..nf)
            .filter(|&i| matches!(self.frames[i].kind, FrameKind::Method | FrameKind::Native))
            .filter(|&i| total_w[i] > 0)
            .map(|i| {
                let fr = &self.frames[i];
                let mut hl = lines.remove(&(i as u32)).unwrap_or_default();
                hl.sort_by(|a, b| b.1.cmp(&a.1));
                hl.truncate(8);
                MethodStat {
                    class: fr.class.clone(),
                    method: fr.method.clone(),
                    arity: fr.arity(),
                    kind: fr.kind,
                    file: fr.file.clone(),
                    decl_line: fr.decl_line,
                    self_w: self_w[i],
                    total_w: total_w[i],
                    hot_lines: hl,
                }
            })
            .collect();
        out.sort_by(|a, b| b.self_w.cmp(&a.self_w).then(b.total_w.cmp(&a.total_w)));
        out.truncate(max_rows);
        out
    }

    /// Per-thread weight (the root's children when the tree was built with a thread layer).
    pub fn thread_stats(&self) -> Vec<ThreadStat> {
        let mut v: Vec<ThreadStat> = self.nodes[0]
            .children
            .iter()
            .map(|&c| &self.nodes[c as usize])
            .filter(|n| self.frames[n.frame as usize].kind == FrameKind::Thread)
            .map(|n| ThreadStat { name: self.frames[n.frame as usize].method.clone(), weight: n.total })
            .collect();
        v.sort_by(|a, b| b.weight.cmp(&a.weight));
        v
    }
}

/// A node of the nested tree the flame graph draws.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FlameNode {
    pub name: String,
    pub value: u64,
    #[serde(rename = "self")]
    pub self_value: u64,
    pub kind: FrameKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    pub children: Vec<FlameNode>,
}

/// Options for [`CallTree::flame`].
#[derive(Debug, Clone, Copy)]
pub struct FlameOptions {
    /// Drop the thread layer (merge all threads); or keep only one thread when `thread` is set.
    pub merge_threads: bool,
    /// Subtrees lighter than this fraction of the total are folded into their parent's self.
    pub min_fraction: f64,
    pub max_nodes: usize,
}

impl Default for FlameOptions {
    fn default() -> Self {
        FlameOptions { merge_threads: true, min_fraction: 0.0005, max_nodes: 25_000 }
    }
}

impl CallTree {
    /// The nested form for drawing. `thread` restricts to one thread's subtree.
    pub fn flame(&self, thread: Option<&str>, opt: FlameOptions) -> FlameNode {
        let mut budget = opt.max_nodes;
        let floor = (self.total as f64 * opt.min_fraction) as u64;
        let root = &self.nodes[0];
        let mut top = FlameNode {
            name: "all".into(),
            value: 0,
            self_value: root.self_w,
            kind: FrameKind::Root,
            class: None,
            file: None,
            line: None,
            children: Vec::new(),
        };
        let thread_nodes: Vec<u32> = root
            .children
            .iter()
            .copied()
            .filter(|&c| {
                let fr = &self.frames[self.nodes[c as usize].frame as usize];
                fr.kind != FrameKind::Thread || thread.map_or(true, |t| fr.method == t)
            })
            .collect();
        if thread.is_some() || opt.merge_threads {
            // Lift the children of the thread layer directly under the root, merging equal frames.
            let mut merged: Vec<FlameNode> = Vec::new();
            for tn in thread_nodes {
                let n = &self.nodes[tn as usize];
                if self.frames[n.frame as usize].kind == FrameKind::Thread {
                    top.self_value += n.self_w;
                    for &c in &n.children {
                        merge_into(&mut merged, self.flame_node(c, floor, &mut budget));
                    }
                } else {
                    merge_into(&mut merged, self.flame_node(tn, floor, &mut budget));
                }
            }
            top.children = merged;
        } else {
            for tn in thread_nodes {
                top.children.push(self.flame_node(tn, floor, &mut budget));
            }
        }
        top.value = top.self_value + top.children.iter().map(|c| c.value).sum::<u64>();
        top
    }

    fn flame_node(&self, n: u32, floor: u64, budget: &mut usize) -> FlameNode {
        let node = &self.nodes[n as usize];
        let fr = &self.frames[node.frame as usize];
        *budget = budget.saturating_sub(1);
        let mut out = FlameNode {
            name: fr.display(),
            value: node.total,
            self_value: node.self_w,
            kind: fr.kind,
            class: (!fr.class.is_empty()).then(|| fr.class.clone()),
            file: fr.file.clone(),
            line: fr.decl_line,
            children: Vec::new(),
        };
        let mut kids: Vec<u32> = node.children.clone();
        kids.sort_by(|a, b| self.nodes[*b as usize].total.cmp(&self.nodes[*a as usize].total));
        for c in kids {
            let cn = &self.nodes[c as usize];
            if cn.total < floor || *budget == 0 {
                out.self_value += cn.total;
                continue;
            }
            out.children.push(self.flame_node(c, floor, budget));
        }
        out
    }
}

fn merge_into(list: &mut Vec<FlameNode>, n: FlameNode) {
    if let Some(e) = list.iter_mut().find(|e| e.name == n.name && e.kind == n.kind) {
        e.value += n.value;
        e.self_value += n.self_value;
        for c in n.children {
            merge_into(&mut e.children, c);
        }
        e.children.sort_by(|a, b| b.value.cmp(&a.value));
    } else {
        list.push(n);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m<'a>(c: &'a str, n: &'a str) -> RawFrame<'a> {
        RawFrame::method(c, n)
    }

    #[test]
    fn stacks_fold_into_a_tree_and_stats_do_not_double_count_recursion() {
        let mut b = TreeBuilder::default();
        // leaf first: fib <- fib <- main
        b.add_stack(Some("main"), &[m("A", "fib"), m("A", "fib"), m("A", "main")], 3);
        b.add_stack(Some("main"), &[m("A", "main")], 1);
        let t = b.finish();
        assert_eq!(t.total, 4);
        let stats = t.method_stats(10);
        let fib = stats.iter().find(|s| s.method == "fib").unwrap();
        assert_eq!(fib.self_w, 3);
        assert_eq!(fib.total_w, 3, "recursive activations must not be summed twice");
        let main = stats.iter().find(|s| s.method == "main").unwrap();
        assert_eq!((main.self_w, main.total_w), (1, 4));
        assert_eq!(t.thread_stats()[0].weight, 4);
    }

    #[test]
    fn node_budget_caps_and_keeps_weight() {
        let mut b = TreeBuilder::new(4);
        b.add_stack(None, &[m("A", "d"), m("A", "c"), m("A", "b"), m("A", "a")], 5);
        let t = b.finish();
        assert!(t.capped);
        assert_eq!(t.total, 5);
        assert_eq!(t.nodes.iter().map(|n| n.self_w).sum::<u64>(), 5);
    }

    #[test]
    fn flame_merges_threads_and_prunes_light_subtrees() {
        let mut b = TreeBuilder::default();
        b.add_stack(Some("t1"), &[m("A", "x"), m("A", "main")], 9000);
        b.add_stack(Some("t2"), &[m("A", "x"), m("A", "main")], 1000);
        b.add_stack(Some("t2"), &[m("A", "rare"), m("A", "main")], 1);
        let t = b.finish();
        let f = t.flame(None, FlameOptions::default());
        assert_eq!(f.value, 10001);
        assert_eq!(f.children.len(), 1, "main merged across threads");
        let main = &f.children[0];
        assert_eq!(main.value, 10001);
        assert_eq!(main.children.len(), 1, "rare folded away");
        assert_eq!(main.children[0].value, 10000);
        let only_t1 = t.flame(Some("t1"), FlameOptions::default());
        assert_eq!(only_t1.value, 9000);
    }
}
