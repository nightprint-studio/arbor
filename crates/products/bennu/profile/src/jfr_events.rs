//! JFR events → aggregate. Streaming: bytes in, one event at a time, nothing retained but the
//! aggregates. Two input shapes feed the same [`JfrAggregator`]:
//!
//! * the output of `jfr print --json` ([`EventSplitter`] cuts it into event objects without ever
//!   holding the whole document — a profile is gigabytes of JSON),
//! * the compact lines [`crate::jfr_helper`]'s Java reader emits.

use std::collections::HashMap;

use serde_json::Value;

use crate::builder::{RawFrame, TreeBuilder};
use crate::duration::duration_ms;
use crate::model::*;

/// The events worth asking `jfr print` for.
pub const WANTED_EVENTS: &str =
    "jdk.ExecutionSample,jdk.ObjectAllocationSample,jdk.JavaMonitorEnter,jdk.GarbageCollection";

/// One decoded frame, owned (the aggregator interns it right away).
#[derive(Debug, Clone, PartialEq)]
pub struct JfrFrame {
    pub class: String,
    pub method: String,
    pub descriptor: String,
    pub line: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum JfrEvent {
    Exec { thread: String, frames: Vec<JfrFrame> },
    Alloc { class: String, bytes: u64, frames: Vec<JfrFrame> },
    Lock { monitor: String, ms: f64, frames: Vec<JfrFrame> },
    Gc { collector: String, cause: String, pause_ms: f64, longest_ms: f64 },
}

/// Cuts the `{"recording":{"events":[ {…}, {…} ]}}` document of `jfr print --json` into event
/// objects. Depth-tracking only (strings and escapes respected); an event is the object that
/// opens while the depth is 3.
#[derive(Default)]
pub struct EventSplitter {
    depth: u32,
    in_string: bool,
    escape: bool,
    buf: Vec<u8>,
    collecting: bool,
}

impl EventSplitter {
    /// Feed a chunk; `emit` receives the bytes of every complete event object.
    pub fn push(&mut self, chunk: &[u8], emit: &mut dyn FnMut(&[u8])) {
        for &b in chunk {
            if self.in_string {
                if self.collecting {
                    self.buf.push(b);
                }
                if self.escape {
                    self.escape = false;
                } else if b == b'\\' {
                    self.escape = true;
                } else if b == b'"' {
                    self.in_string = false;
                }
                continue;
            }
            match b {
                b'"' => {
                    self.in_string = true;
                    if self.collecting {
                        self.buf.push(b);
                    }
                }
                b'{' | b'[' => {
                    if b == b'{' && self.depth == 3 && !self.collecting {
                        self.collecting = true;
                        self.buf.clear();
                    }
                    if self.collecting {
                        self.buf.push(b);
                    }
                    self.depth += 1;
                }
                b'}' | b']' => {
                    self.depth = self.depth.saturating_sub(1);
                    if self.collecting {
                        self.buf.push(b);
                        if b == b'}' && self.depth == 3 {
                            self.collecting = false;
                            emit(&self.buf);
                            self.buf.clear();
                        }
                    }
                }
                _ => {
                    if self.collecting {
                        self.buf.push(b);
                    }
                }
            }
        }
    }
}

fn norm_class(s: &str) -> String {
    s.replace('/', ".")
}

fn frames_of(values: &Value) -> Vec<JfrFrame> {
    let Some(arr) = values.pointer("/stackTrace/frames").and_then(Value::as_array) else {
        return Vec::new();
    };
    arr.iter()
        .map(|f| {
            let m = &f["method"];
            let class = m
                .pointer("/type/name")
                .and_then(Value::as_str)
                .map(norm_class)
                .unwrap_or_default();
            JfrFrame {
                class,
                method: m["name"].as_str().unwrap_or("?").to_string(),
                descriptor: m["descriptor"].as_str().unwrap_or("").to_string(),
                line: f["lineNumber"].as_i64().filter(|l| *l > 0).map(|l| l as u32),
            }
        })
        .collect()
}

fn class_name(v: &Value) -> String {
    match v {
        Value::String(s) => norm_class(s),
        Value::Object(_) => v["name"].as_str().map(norm_class).unwrap_or_default(),
        _ => String::new(),
    }
}

fn thread_name(v: &Value) -> String {
    v["javaName"]
        .as_str()
        .or_else(|| v["osName"].as_str())
        .unwrap_or("")
        .to_string()
}

/// Decode one `jfr print --json` event object; `None` for events we do not aggregate.
pub fn parse_print_event(ev: &Value) -> Option<JfrEvent> {
    let values = &ev["values"];
    match ev["type"].as_str()? {
        "jdk.ExecutionSample" => Some(JfrEvent::Exec {
            thread: thread_name(&values["sampledThread"]),
            frames: frames_of(values),
        }),
        "jdk.ObjectAllocationSample" => Some(JfrEvent::Alloc {
            class: class_name(&values["objectClass"]),
            bytes: values["weight"].as_u64().or_else(|| values["weight"].as_f64().map(|f| f as u64)).unwrap_or(0),
            frames: frames_of(values),
        }),
        "jdk.JavaMonitorEnter" => Some(JfrEvent::Lock {
            monitor: class_name(&values["monitorClass"]),
            ms: duration_ms(&values["duration"]),
            frames: frames_of(values),
        }),
        "jdk.GarbageCollection" => Some(JfrEvent::Gc {
            collector: values["name"].as_str().unwrap_or("GC").to_string(),
            cause: values["cause"].as_str().unwrap_or("").to_string(),
            pause_ms: duration_ms(&values["sumOfPauses"]),
            longest_ms: duration_ms(&values["longestPause"]),
        }),
        _ => None,
    }
}

/// Decode one line of the Java helper's output.
pub fn parse_helper_line(line: &str) -> Option<JfrEvent> {
    let v: Value = serde_json::from_str(line).ok()?;
    let frames = || -> Vec<JfrFrame> {
        v["f"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|f| JfrFrame {
                        class: f[0].as_str().unwrap_or("").to_string(),
                        method: f[1].as_str().unwrap_or("?").to_string(),
                        descriptor: f[2].as_str().unwrap_or("").to_string(),
                        line: f[3].as_i64().filter(|l| *l > 0).map(|l| l as u32),
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let n = v["n"].as_i64().unwrap_or(0).max(0) as u64;
    match v["e"].as_str()? {
        "x" => Some(JfrEvent::Exec { thread: v["t"].as_str().unwrap_or("").into(), frames: frames() }),
        "a" => Some(JfrEvent::Alloc { class: v["c"].as_str().unwrap_or("").into(), bytes: n, frames: frames() }),
        "l" => Some(JfrEvent::Lock {
            monitor: v["c"].as_str().unwrap_or("").into(),
            ms: n as f64 / 1e6,
            frames: frames(),
        }),
        "g" => Some(JfrEvent::Gc {
            collector: v["c"].as_str().unwrap_or("GC").into(),
            cause: v["cause"].as_str().unwrap_or("").into(),
            pause_ms: n as f64 / 1e6,
            longest_ms: v["max"].as_i64().unwrap_or(0) as f64 / 1e6,
        }),
        _ => None,
    }
}

#[derive(Default)]
struct AllocAcc {
    count: u64,
    bytes: u64,
}

#[derive(Default)]
struct LockAcc {
    count: u64,
    total_ms: f64,
    max_ms: f64,
    sites: HashMap<String, u64>,
}

#[derive(Default)]
struct GcAcc {
    count: u64,
    total: f64,
    max: f64,
    causes: HashMap<String, u64>,
}

/// Number of distinct allocation sites / lock monitors kept; the rest is dropped (and noted).
const TABLE_CAP: usize = 20_000;

/// Folds events into the profile's tables.
pub struct JfrAggregator {
    tree: TreeBuilder,
    alloc_sites: HashMap<String, AllocAcc>,
    alloc_types: HashMap<String, AllocAcc>,
    locks: HashMap<String, LockAcc>,
    gc: HashMap<String, GcAcc>,
    pub events: u64,
    exec_samples: u64,
    table_overflow: bool,
}

impl Default for JfrAggregator {
    fn default() -> Self {
        JfrAggregator {
            tree: TreeBuilder::default(),
            alloc_sites: HashMap::new(),
            alloc_types: HashMap::new(),
            locks: HashMap::new(),
            gc: HashMap::new(),
            events: 0,
            exec_samples: 0,
            table_overflow: false,
        }
    }
}

fn raw<'a>(f: &'a JfrFrame) -> RawFrame<'a> {
    RawFrame {
        class: &f.class,
        method: &f.method,
        descriptor: (!f.descriptor.is_empty()).then_some(f.descriptor.as_str()),
        file: None,
        line: f.line,
        decl_line: None,
        kind: FrameKind::Method,
    }
}

fn site_of(frames: &[JfrFrame]) -> String {
    frames
        .first()
        .map(|f| if f.class.is_empty() { f.method.clone() } else { format!("{}.{}", f.class, f.method) })
        .unwrap_or_else(|| "(no stack)".into())
}

impl JfrAggregator {
    pub fn exec_samples(&self) -> u64 {
        self.exec_samples
    }

    pub fn feed(&mut self, ev: JfrEvent) {
        self.events += 1;
        match ev {
            JfrEvent::Exec { thread, frames } => {
                self.exec_samples += 1;
                let raws: Vec<RawFrame> = frames.iter().map(raw).collect();
                let name = if thread.is_empty() { "(unknown thread)" } else { thread.as_str() };
                self.tree.add_stack(Some(name), &raws, 1);
            }
            JfrEvent::Alloc { class, bytes, frames } => {
                let site = site_of(&frames);
                self.bump_alloc(true, site, bytes);
                self.bump_alloc(false, class, bytes);
            }
            JfrEvent::Lock { monitor, ms, frames } => {
                if !self.locks.contains_key(&monitor) && self.locks.len() >= TABLE_CAP {
                    self.table_overflow = true;
                    return;
                }
                let a = self.locks.entry(monitor).or_default();
                a.count += 1;
                a.total_ms += ms;
                a.max_ms = a.max_ms.max(ms);
                *a.sites.entry(site_of(&frames)).or_insert(0) += 1;
            }
            JfrEvent::Gc { collector, cause, pause_ms, longest_ms } => {
                let a = self.gc.entry(collector).or_default();
                a.count += 1;
                a.total += pause_ms;
                a.max = a.max.max(longest_ms.max(pause_ms));
                if !cause.is_empty() {
                    *a.causes.entry(cause).or_insert(0) += 1;
                }
            }
        }
    }

    fn bump_alloc(&mut self, site: bool, key: String, bytes: u64) {
        let map = if site { &mut self.alloc_sites } else { &mut self.alloc_types };
        if !map.contains_key(&key) && map.len() >= TABLE_CAP {
            self.table_overflow = true;
            return;
        }
        let a = map.entry(key).or_default();
        a.count += 1;
        a.bytes += bytes;
    }

    /// Fill `p` with the aggregated tables. `p.ms_per_unit` must already be set.
    pub fn finish_into(self, p: &mut Profile) {
        let capped = self.tree.capped();
        let tree = self.tree.finish();
        p.methods = tree.method_stats(5_000);
        p.threads = tree.thread_stats();
        p.tree = tree;
        p.alloc_sites = top_alloc(self.alloc_sites);
        p.alloc_types = top_alloc(self.alloc_types);
        p.locks = {
            let mut v: Vec<LockStat> = self
                .locks
                .into_iter()
                .map(|(monitor, a)| LockStat {
                    site: a.sites.into_iter().max_by_key(|(_, n)| *n).map(|(s, _)| s).unwrap_or_default(),
                    monitor,
                    count: a.count,
                    total_ms: a.total_ms,
                    max_ms: a.max_ms,
                })
                .collect();
            v.sort_by(|a, b| b.total_ms.total_cmp(&a.total_ms));
            v.truncate(500);
            v
        };
        p.gc = {
            let mut v: Vec<GcStat> = self
                .gc
                .into_iter()
                .map(|(collector, a)| GcStat {
                    collector,
                    count: a.count,
                    total_pause_ms: a.total,
                    max_pause_ms: a.max,
                    top_cause: a.causes.into_iter().max_by_key(|(_, n)| *n).map(|(c, _)| c).unwrap_or_default(),
                })
                .collect();
            v.sort_by(|a, b| b.total_pause_ms.total_cmp(&a.total_pause_ms));
            v
        };
        if capped {
            p.notes.push("The call tree hit its size limit; the deepest frames of rare stacks were folded into their callers.".into());
        }
        if self.table_overflow {
            p.notes.push("Very many distinct allocation sites or monitors: the tables keep the first 20 000 of each.".into());
        }
        if self.exec_samples == 0 {
            p.notes.push(
                "The recording holds no execution samples. The program may have run for less than the sampling period (10 ms), \
                 or exited before the recording was dumped."
                    .into(),
            );
        }
    }
}

fn top_alloc(m: HashMap<String, AllocAcc>) -> Vec<AllocStat> {
    let mut v: Vec<AllocStat> = m.into_iter().map(|(name, a)| AllocStat { name, count: a.count, bytes: a.bytes }).collect();
    v.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    v.truncate(2_000);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRINT_JSON: &str = r#"{"recording":{"events":[
      {"type":"jdk.ExecutionSample","values":{"startTime":"2024-01-01T00:00:00Z","sampledThread":{"osName":"main","javaName":"main","javaThreadId":1},
        "stackTrace":{"truncated":false,"frames":[
          {"method":{"type":{"name":"com/acme/Hot"},"name":"spin","descriptor":"(I)V"},"lineNumber":42,"type":"JIT compiled"},
          {"method":{"type":{"name":"com.acme.App"},"name":"main","descriptor":"([Ljava/lang/String;)V"},"lineNumber":7,"type":"Interpreted"}]},
        "state":"STATE_RUNNABLE"}},
      {"type":"jdk.ObjectAllocationSample","values":{"objectClass":{"name":"byte[]"},"weight":1048576,
        "stackTrace":{"frames":[{"method":{"type":{"name":"com.acme.App"},"name":"load","descriptor":"()V"},"lineNumber":9}]}}},
      {"type":"jdk.JavaMonitorEnter","values":{"duration":"PT0.0123S","monitorClass":{"name":"java.lang.Object"},
        "stackTrace":{"frames":[{"method":{"type":{"name":"com.acme.App"},"name":"sync","descriptor":"()V"},"lineNumber":3}]}}},
      {"type":"jdk.GarbageCollection","values":{"name":"G1New","cause":"G1 Evacuation Pause","sumOfPauses":"PT0.005S","longestPause":"PT0.004S"}},
      {"type":"jdk.Other","values":{"a":"} \" ] {"}}
    ]}}"#;

    fn collect(chunk: usize) -> Vec<JfrEvent> {
        let mut sp = EventSplitter::default();
        let mut out = Vec::new();
        for part in PRINT_JSON.as_bytes().chunks(chunk) {
            sp.push(part, &mut |obj| {
                let v: Value = serde_json::from_slice(obj).expect("valid event json");
                if let Some(e) = parse_print_event(&v) {
                    out.push(e);
                }
            });
        }
        out
    }

    #[test]
    fn splitter_is_chunk_size_independent_and_survives_tricky_strings() {
        let whole = collect(PRINT_JSON.len());
        assert_eq!(whole.len(), 4);
        assert_eq!(whole, collect(7));
        assert_eq!(whole, collect(1));
    }

    #[test]
    fn aggregates_samples_allocations_locks_and_gc() {
        let mut agg = JfrAggregator::default();
        for e in collect(64) {
            agg.feed(e);
        }
        let mut p = Profile {
            id: "t".into(),
            lang: Lang::Java,
            label: "t".into(),
            created_ms: 0,
            duration_ms: None,
            ms_per_unit: 10.0,
            tree: CallTree::default(),
            methods: vec![],
            alloc_sites: vec![],
            alloc_types: vec![],
            locks: vec![],
            gc: vec![],
            threads: vec![],
            notes: vec![],
        };
        agg.finish_into(&mut p);
        let spin = p.methods.iter().find(|m| m.method == "spin").unwrap();
        assert_eq!(spin.class, "com.acme.Hot", "slashes normalised");
        assert_eq!((spin.self_w, spin.total_w, spin.arity), (1, 1, Some(1)));
        assert_eq!(spin.hot_lines, vec![(42, 1)]);
        assert_eq!(p.alloc_types[0].name, "byte[]");
        assert_eq!(p.alloc_sites[0].name, "com.acme.App.load");
        assert_eq!(p.alloc_sites[0].bytes, 1_048_576);
        assert!((p.locks[0].total_ms - 12.3).abs() < 1e-6);
        assert_eq!(p.gc[0].collector, "G1New");
        assert!((p.gc[0].total_pause_ms - 5.0).abs() < 1e-6);
        assert_eq!(p.threads[0].name, "main");
    }

    #[test]
    fn helper_lines_decode() {
        let l = r#"{"e":"x","t":"main","f":[["a.B","m","(I)V",5]]}"#;
        match parse_helper_line(l).unwrap() {
            JfrEvent::Exec { thread, frames } => {
                assert_eq!(thread, "main");
                assert_eq!(frames[0].line, Some(5));
            }
            e => panic!("{e:?}"),
        }
        assert!(matches!(
            parse_helper_line(r#"{"e":"g","c":"G1","n":5000000,"cause":"x","max":4000000}"#),
            Some(JfrEvent::Gc { pause_ms, .. }) if (pause_ms - 5.0).abs() < 1e-9
        ));
    }
}
