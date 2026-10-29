//! V8 / Node CPU profiles (`.cpuprofile`, what `node --cpu-prof`, the inspector's
//! `Profiler.stop` and Chrome DevTools write) → call tree.
//!
//! Frames carry the function's *declaration* line (`lineNumber`, 0-based) and its script URL, so
//! the editor can pin hints to the right function without an index. Source maps are not applied:
//! a bundled or transpiled script is reported at the line of the file V8 ran.

use std::collections::HashMap;

use serde_json::Value;

use crate::builder::{RawFrame, TreeBuilder};
use crate::model::*;

/// `file:///C:/x/y.js` → `C:/x/y.js`; `file:///home/x.js` → `/home/x.js`; other URLs unchanged.
pub fn url_to_path(url: &str) -> String {
    let Some(rest) = url.strip_prefix("file://") else { return url.to_string() };
    let rest = percent_decode(rest);
    // `/C:/…` → `C:/…`
    let b = rest.as_bytes();
    if b.len() > 2 && b[0] == b'/' && b[2] == b':' && b[1].is_ascii_alphabetic() {
        return rest[1..].to_string();
    }
    rest
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// What a parsed `.cpuprofile` contributes.
pub struct CpuProfileResult {
    pub ms_per_sample: f64,
    pub duration_ms: f64,
}

/// Fold one `.cpuprofile` into `builder` under a thread layer named `layer` (e.g. the process
/// file name, so several profiles of one dev-server run stay apart).
pub fn add_cpuprofile(builder: &mut TreeBuilder, layer: &str, json: &Value) -> Result<CpuProfileResult, String> {
    let nodes = json["nodes"].as_array().ok_or("not a .cpuprofile: no `nodes` array")?;
    let samples = json["samples"].as_array().ok_or("not a .cpuprofile: no `samples` array")?;

    let mut index: HashMap<i64, usize> = HashMap::with_capacity(nodes.len());
    let mut parent: Vec<Option<usize>> = vec![None; nodes.len()];
    for (i, n) in nodes.iter().enumerate() {
        if let Some(id) = n["id"].as_i64() {
            index.insert(id, i);
        }
    }
    for (i, n) in nodes.iter().enumerate() {
        if let Some(ch) = n["children"].as_array() {
            for c in ch.iter().filter_map(Value::as_i64) {
                if let Some(&ci) = index.get(&c) {
                    parent[ci] = Some(i);
                }
            }
        }
    }

    // Weight per leaf node: the time delta *preceding* each sample is the standard attribution.
    let deltas = json["timeDeltas"].as_array();
    let mut weight: HashMap<usize, u64> = HashMap::new();
    for s in samples {
        let Some(id) = s.as_i64() else { continue };
        if let Some(&ni) = index.get(&id) {
            *weight.entry(ni).or_insert(0) += 1;
        }
    }
    let n_samples: u64 = weight.values().sum();
    let start = json["startTime"].as_f64().unwrap_or(0.0);
    let end = json["endTime"].as_f64().unwrap_or(start);
    let duration_ms = ((end - start) / 1000.0).max(0.0);
    let ms_per_sample = if n_samples > 0 {
        if let Some(d) = deltas.filter(|d| !d.is_empty()) {
            let sum: f64 = d.iter().filter_map(Value::as_f64).filter(|x| *x > 0.0).sum();
            (sum / d.len() as f64) / 1000.0
        } else {
            duration_ms / n_samples as f64
        }
    } else {
        0.0
    };

    let mut leaves: Vec<(usize, u64)> = weight.into_iter().collect();
    leaves.sort_unstable();
    for (leaf, w) in leaves {
        // Walk to the root collecting frames leaf-first.
        let mut chain: Vec<usize> = Vec::new();
        let mut cur = Some(leaf);
        while let Some(c) = cur {
            chain.push(c);
            cur = parent[c];
            if chain.len() > 4096 {
                break;
            }
        }
        let owned: Vec<(String, String, Option<String>, Option<u32>, FrameKind)> = chain
            .iter()
            .filter_map(|&i| {
                let cf = &nodes[i]["callFrame"];
                let name = cf["functionName"].as_str().unwrap_or("");
                if name == "(root)" {
                    return None;
                }
                let url = cf["url"].as_str().unwrap_or("");
                let path = url_to_path(url);
                let line = cf["lineNumber"].as_i64().filter(|l| *l >= 0).map(|l| l as u32 + 1);
                let (kind, display) = if name.starts_with('(') {
                    (FrameKind::Native, name.to_string())
                } else if name.is_empty() {
                    (FrameKind::Method, "(anonymous)".to_string())
                } else {
                    (FrameKind::Method, name.to_string())
                };
                let file = (!path.is_empty()).then_some(path.clone());
                Some((path, display, file, line, kind))
            })
            .collect();
        let raws: Vec<RawFrame> = owned
            .iter()
            .map(|(class, method, file, line, kind)| RawFrame {
                class,
                method,
                descriptor: None,
                file: file.as_deref(),
                // The function's own line doubles as the sample line: hints sit on declarations.
                line: *line,
                decl_line: *line,
                kind: *kind,
            })
            .collect();
        builder.add_stack(Some(layer), &raws, w);
    }
    Ok(CpuProfileResult { ms_per_sample, duration_ms })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample() -> Value {
        json!({
          "nodes":[
            {"id":1,"callFrame":{"functionName":"(root)","url":"","lineNumber":-1},"children":[2,5]},
            {"id":2,"callFrame":{"functionName":"main","url":"file:///C:/app/server.js","lineNumber":9},"children":[3]},
            {"id":3,"callFrame":{"functionName":"render","url":"file:///C:/app/view%20x.js","lineNumber":19},"children":[]},
            {"id":5,"callFrame":{"functionName":"(idle)","url":"","lineNumber":-1},"children":[]}
          ],
          "startTime":1000000,"endTime":1040000,
          "samples":[3,3,3,2,5],
          "timeDeltas":[8000,8000,8000,8000,8000]
        })
    }

    #[test]
    fn builds_tree_with_declaration_lines_and_paths() {
        let mut b = TreeBuilder::default();
        let r = add_cpuprofile(&mut b, "server.cpuprofile", &sample()).unwrap();
        assert!((r.ms_per_sample - 8.0).abs() < 1e-9);
        assert!((r.duration_ms - 40.0).abs() < 1e-9);
        let t = b.finish();
        assert_eq!(t.total, 5);
        let stats = t.method_stats(10);
        let render = stats.iter().find(|s| s.method == "render").unwrap();
        assert_eq!((render.self_w, render.total_w), (3, 3));
        assert_eq!(render.file.as_deref(), Some("C:/app/view x.js"));
        assert_eq!(render.decl_line, Some(20));
        let main = stats.iter().find(|s| s.method == "main").unwrap();
        assert_eq!((main.self_w, main.total_w), (1, 4));
    }

    #[test]
    fn rejects_other_json() {
        let mut b = TreeBuilder::default();
        assert!(add_cpuprofile(&mut b, "x", &json!({"a":1})).is_err());
    }

    #[test]
    fn file_urls_become_paths() {
        assert_eq!(url_to_path("file:///home/me/a.js"), "/home/me/a.js");
        assert_eq!(url_to_path("file:///C:/a/b.js"), "C:/a/b.js");
        assert_eq!(url_to_path("http://localhost:4200/main.js"), "http://localhost:4200/main.js");
    }
}
