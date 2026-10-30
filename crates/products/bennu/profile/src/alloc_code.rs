//! Who allocates: the recording's allocated bytes credited to the **project's own methods**.
//!
//! JFR reports an allocation at the code that performed it — `Arrays.copyOf`, `HashMap.resize` — which is true and
//! no use to anyone who wants to know which of *their* methods to change. The allocation tree keeps the whole stack,
//! with the allocated type as its tip, so each allocation can be walked up to the nearest frame the project owns:
//! `Service.load → ArrayList.<init> → Arrays.copyOf → [byte[]]` is `Service.load`'s.
//!
//! What no project frame reaches (a framework thread that never called into the project) is returned as one row of
//! its own rather than dropped, so the shares still add up to everything the recording allocated.

use std::collections::HashMap;

use serde::Serialize;

use crate::model::*;

/// One project method and what it allocated.
#[derive(Debug, Clone, Serialize)]
pub struct CodeAlloc {
    /// Binary class name (`a.b.Outer$Inner`), empty for the row of allocations no project frame reached.
    pub class: String,
    pub method: String,
    pub arity: Option<u32>,
    pub bytes: u64,
    /// The types the bytes went to, biggest first (at most five).
    pub types: Vec<(String, u64)>,
}

/// Name of the row for allocations made outside the project's code.
pub const OUTSIDE_PROJECT: &str = "(no project code on the stack)";

pub fn allocations_by_code(tree: &CallTree, is_project: &dyn Fn(&str) -> bool, limit: usize) -> Vec<CodeAlloc> {
    let mut by_frame: HashMap<Option<u32>, (u64, HashMap<&str, u64>)> = HashMap::new();
    for node in &tree.nodes {
        if node.self_w == 0 {
            continue;
        }
        let tip = &tree.frames[node.frame as usize];
        // The allocated type is a synthetic native frame with no class.
        if tip.kind != FrameKind::Native || !tip.class.is_empty() {
            continue;
        }
        let mut owner: Option<u32> = None;
        let mut cur = node.parent;
        while cur != NO_PARENT {
            let n = &tree.nodes[cur as usize];
            let f = &tree.frames[n.frame as usize];
            if f.kind == FrameKind::Method && is_project(&f.class) {
                owner = Some(n.frame);
                break;
            }
            cur = n.parent;
        }
        let e = by_frame.entry(owner).or_default();
        e.0 += node.self_w;
        *e.1.entry(tip.method.as_str()).or_insert(0) += node.self_w;
    }
    let mut rows: Vec<CodeAlloc> = by_frame
        .into_iter()
        .map(|(owner, (bytes, types))| {
            let mut types: Vec<(String, u64)> = types.into_iter().map(|(t, n)| (t.to_string(), n)).collect();
            types.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            types.truncate(5);
            match owner {
                Some(ix) => {
                    let f = &tree.frames[ix as usize];
                    CodeAlloc { class: f.class.clone(), method: f.method.clone(), arity: f.arity(), bytes, types }
                }
                None => CodeAlloc { class: String::new(), method: OUTSIDE_PROJECT.to_string(), arity: None, bytes, types },
            }
        })
        .collect();
    rows.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.method.cmp(&b.method)));
    rows.truncate(limit);
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::{RawFrame, TreeBuilder};

    fn stack(b: &mut TreeBuilder, ty: &str, frames: &[(&str, &str)], bytes: u64) {
        let tip = RawFrame { kind: FrameKind::Native, ..RawFrame::method("", ty) };
        let mut raws = vec![tip];
        raws.extend(frames.iter().map(|(c, m)| RawFrame { descriptor: Some("()V"), ..RawFrame::method(c, m) }));
        b.add_stack(None, &raws, bytes);
    }

    #[test]
    fn bytes_go_to_the_nearest_project_frame_not_to_the_jdk_that_allocated_them() {
        let mut b = TreeBuilder::default();
        // Leaf-first, as JFR reports it: the JDK frames, then the project method, then the framework above it.
        stack(&mut b, "byte[]", &[("java.util.Arrays", "copyOf"), ("a.b.Service", "load"), ("org.spring.Proxy", "invoke")], 3000);
        stack(&mut b, "String", &[("java.lang.String", "concat"), ("a.b.Service", "load")], 1000);
        stack(&mut b, "Node", &[("java.util.HashMap", "put"), ("a.b.Repo", "save"), ("a.b.Service", "load")], 500);
        stack(&mut b, "byte[]", &[("org.tomcat.Buffer", "grow")], 200);
        let tree = b.finish();

        let rows = allocations_by_code(&tree, &|c| c.starts_with("a.b."), 10);
        let load = rows.iter().find(|r| r.method == "load").unwrap();
        assert_eq!(load.bytes, 4000);
        assert_eq!(load.types[0], ("byte[]".to_string(), 3000));
        // The nearest project frame wins: Repo.save allocated the HashMap node, not the Service above it.
        assert_eq!(rows.iter().find(|r| r.method == "save").unwrap().bytes, 500);
        // What no project frame reached is kept, so the shares still add up.
        let outside = rows.iter().find(|r| r.method == OUTSIDE_PROJECT).unwrap();
        assert_eq!((outside.class.as_str(), outside.bytes), ("", 200));
        assert_eq!(rows.iter().map(|r| r.bytes).sum::<u64>(), tree.total);
        assert_eq!(rows[0].method, "load", "biggest first");
    }
}
