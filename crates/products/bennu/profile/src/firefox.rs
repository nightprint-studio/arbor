//! ADAPTER (format not verified against a live `samply` run): the Firefox Profiler "processed"
//! format that `samply record --save-only` writes, reduced to the call tree.
//!
//! Written from the documented shape of that format — `threads[]` each with a string array,
//! `funcTable`, `frameTable`, `stackTable` and `samples` — and exercised only by a hand-made
//! fixture. What is NOT known without running samply on a real binary:
//!
//! * whether function names arrive resolved or as hex addresses (samply leaves symbolication to
//!   its viewer unless `--presymbolicate` is given). Unresolved frames are detected and reported
//!   as a note on the profile instead of being presented as if they were readable;
//! * the exact spelling of optional fields across samply versions (both `stringArray` and the
//!   older `stringTable` are tolerated).

use serde_json::Value;

use crate::builder::{RawFrame, TreeBuilder};
use crate::model::*;

/// Result of folding a processed profile in.
pub struct FirefoxResult {
    pub ms_per_sample: f64,
    pub unresolved_fraction: f64,
    pub threads: usize,
}

fn strings(thread: &Value) -> Vec<String> {
    let arr = thread["stringArray"].as_array().or_else(|| thread["stringTable"].as_array());
    arr.map(|a| a.iter().map(|s| s.as_str().unwrap_or("").to_string()).collect()).unwrap_or_default()
}

fn col<'a>(table: &'a Value, name: &str) -> &'a [Value] {
    table[name].as_array().map(Vec::as_slice).unwrap_or(&[])
}

fn looks_like_address(name: &str) -> bool {
    let n = name.strip_prefix("0x").unwrap_or(name);
    !n.is_empty() && n.len() >= 6 && n.bytes().all(|b| b.is_ascii_hexdigit()) && name.starts_with("0x")
}

/// Split `crate::module::func` → (`crate::module`, `func`). A leading `<T as Trait>::` is kept in
/// the module part.
fn split_rust_path(full: &str) -> (&str, &str) {
    match full.rfind("::") {
        Some(i) => (&full[..i], &full[i + 2..]),
        None => ("", full),
    }
}

pub fn add_firefox_profile(builder: &mut TreeBuilder, json: &Value) -> Result<FirefoxResult, String> {
    let threads = json["threads"].as_array().ok_or("not a Firefox-profiler profile: no `threads`")?;
    let interval_ms = json["meta"]["interval"].as_f64().filter(|i| *i > 0.0).unwrap_or(1.0);
    let (mut resolved, mut unresolved, mut used) = (0u64, 0u64, 0usize);

    for th in threads {
        let strs = strings(th);
        let s = |ix: &Value| -> String { ix.as_u64().and_then(|i| strs.get(i as usize)).cloned().unwrap_or_default() };
        let func_name = col(&th["funcTable"], "name");
        let func_file = col(&th["funcTable"], "fileName");
        let func_line = col(&th["funcTable"], "lineNumber");
        let frame_func = col(&th["frameTable"], "func");
        let frame_line = col(&th["frameTable"], "line");
        let stack_frame = col(&th["stackTable"], "frame");
        let stack_prefix = col(&th["stackTable"], "prefix");
        let sample_stack = col(&th["samples"], "stack");
        let sample_weight = col(&th["samples"], "weight");
        if sample_stack.is_empty() || stack_frame.is_empty() {
            continue;
        }
        used += 1;
        let tname = {
            let n = th["name"].as_str().unwrap_or("thread");
            match th["processName"].as_str() {
                Some(p) if !p.is_empty() && p != n => format!("{p} / {n}"),
                _ => n.to_string(),
            }
        };

        // Per stack index → weight, so each distinct stack is walked once.
        let mut weights: std::collections::BTreeMap<usize, u64> = std::collections::BTreeMap::new();
        for (i, st) in sample_stack.iter().enumerate() {
            let Some(si) = st.as_u64() else { continue };
            let w = sample_weight.get(i).and_then(Value::as_u64).unwrap_or(1).max(1);
            *weights.entry(si as usize).or_insert(0) += w;
        }
        for (si, w) in weights {
            // Walk prefixes: leaf → root, i.e. already leaf-first.
            let mut chain: Vec<(String, String, Option<String>, Option<u32>)> = Vec::new();
            let mut cur = Some(si);
            while let Some(c) = cur {
                if chain.len() > 4096 {
                    break;
                }
                let Some(fr) = stack_frame.get(c).and_then(Value::as_u64) else { break };
                let func = frame_func.get(fr as usize).and_then(Value::as_u64).unwrap_or(0) as usize;
                let name = func_name.get(func).map(&s).unwrap_or_default();
                let file = func_file.get(func).map(&s).filter(|f| !f.is_empty());
                let line = frame_line
                    .get(fr as usize)
                    .and_then(Value::as_u64)
                    .or_else(|| func_line.get(func).and_then(Value::as_u64))
                    .filter(|l| *l > 0)
                    .map(|l| l as u32);
                // Only the leaf decides whether a sample is readable: that is the frame whose
                // name the hot-methods table leads with.
                if chain.is_empty() {
                    if looks_like_address(&name) {
                        unresolved += w;
                    } else {
                        resolved += w;
                    }
                }
                let (module, func_only) = split_rust_path(&name);
                chain.push((module.to_string(), func_only.to_string(), file, line));
                cur = stack_prefix.get(c).and_then(Value::as_u64).map(|p| p as usize);
            }
            let raws: Vec<RawFrame> = chain
                .iter()
                .map(|(module, f, file, line)| RawFrame {
                    class: module,
                    method: f,
                    descriptor: None,
                    file: file.as_deref(),
                    line: *line,
                    decl_line: None,
                    kind: if looks_like_address(f) { FrameKind::Native } else { FrameKind::Method },
                })
                .collect();
            builder.add_stack(Some(&tname), &raws, w);
        }
    }
    if used == 0 {
        return Err("the profile contains no samples".into());
    }
    let total = (resolved + unresolved).max(1) as f64;
    Ok(FirefoxResult { ms_per_sample: interval_ms, unresolved_fraction: unresolved as f64 / total, threads: used })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture() -> Value {
        json!({
          "meta": {"interval": 1.0},
          "threads": [{
            "name": "main", "processName": "app.exe",
            "stringArray": ["app::work::hash", "src/work.rs", "app::main", "0xdeadbeef"],
            "funcTable": {"name":[0,2,3], "fileName":[1,null,null], "lineNumber":[10,null,null], "length":3},
            "frameTable": {"func":[0,1,2], "line":[12,null,null], "length":3},
            "stackTable": {"frame":[1,0,2], "prefix":[null,0,0], "length":3},
            "samples": {"stack":[1,1,1,2], "length":4}
          }]
        })
    }

    #[test]
    fn reads_the_processed_shape_and_reports_unresolved_addresses() {
        let mut b = TreeBuilder::default();
        let r = add_firefox_profile(&mut b, &fixture()).unwrap();
        assert_eq!(r.threads, 1);
        assert!((r.unresolved_fraction - 0.25).abs() < 1e-9);
        let t = b.finish();
        assert_eq!(t.total, 4);
        let stats = t.method_stats(10);
        let hash = stats.iter().find(|s| s.method == "hash").unwrap();
        assert_eq!(hash.class, "app::work");
        assert_eq!(hash.self_w, 3);
        assert_eq!(hash.file.as_deref(), Some("src/work.rs"));
        assert_eq!(hash.hot_lines[0], (12, 3));
        let main = stats.iter().find(|s| s.method == "main").unwrap();
        assert_eq!(main.total_w, 4);
    }

    #[test]
    fn empty_or_foreign_json_is_an_error() {
        let mut b = TreeBuilder::default();
        assert!(add_firefox_profile(&mut b, &json!({})).is_err());
        assert!(add_firefox_profile(&mut b, &json!({"threads":[{"name":"x"}]})).is_err());
    }
}
