//! The aggregated profile — what every parser reduces a recording to, and what crosses the RPC
//! seam, the disk and the editor.
//!
//! A recording holds millions of samples; the profile holds the *call tree* they fall into plus
//! per-method and per-site tables. Nothing here keeps a raw sample.

use serde::{Deserialize, Serialize};

/// Which runtime produced the recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    Java,
    Node,
    Rust,
}

/// What a frame is. Decides whether it is a "method" for the hot-methods table and the editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FrameKind {
    /// Synthetic top of the tree.
    Root,
    /// A thread (or process) layer under the root.
    Thread,
    /// A source-level function: a Java method, a JS function, a Rust fn.
    Method,
    /// Runtime internals with no source: `(garbage collector)`, `(idle)`, unsymbolicated addresses.
    Native,
}

/// One distinct function, interned once per profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameInfo {
    /// Fully-qualified class (Java: dotted binary name, `a.b.Outer$Inner`), module/script path
    /// (JS) or crate/module path (Rust). Empty for a synthetic frame.
    #[serde(rename = "c")]
    pub class: String,
    /// Method / function name. Java constructors are `<init>`.
    #[serde(rename = "m")]
    pub method: String,
    /// JVM descriptor (`(ILjava/lang/String;)V`) when known — the only thing telling overloads apart.
    #[serde(rename = "d", default, skip_serializing_if = "Option::is_none")]
    pub descriptor: Option<String>,
    /// Source file when the runtime knows it (JS script path, Rust file).
    #[serde(rename = "f", default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// Line the function is declared on, when the runtime reports it (JS does; JFR does not).
    #[serde(rename = "l", default, skip_serializing_if = "Option::is_none")]
    pub decl_line: Option<u32>,
    #[serde(rename = "k")]
    pub kind: FrameKind,
}

impl FrameInfo {
    /// `class#method` — the key the editor matches a declaration against.
    pub fn key(&self) -> String {
        format!("{}#{}", self.class, self.method)
    }

    /// Display name: `Class.method` for Java, the bare name otherwise.
    pub fn display(&self) -> String {
        if self.class.is_empty() {
            self.method.clone()
        } else {
            format!("{}.{}", self.class, self.method)
        }
    }

    /// Number of parameters, from a JVM descriptor.
    pub fn arity(&self) -> Option<u32> {
        self.descriptor.as_deref().and_then(descriptor_arity)
    }
}

/// Parameter count of a JVM method descriptor: `(I[JLjava/lang/String;)V` has 3.
pub fn descriptor_arity(desc: &str) -> Option<u32> {
    let inner = desc.strip_prefix('(')?;
    let end = inner.find(')')?;
    let b = inner[..end].as_bytes();
    let (mut i, mut n) = (0, 0);
    while i < b.len() {
        while b[i] == b'[' {
            i += 1;
            if i >= b.len() {
                return None;
            }
        }
        if b[i] == b'L' {
            while i < b.len() && b[i] != b';' {
                i += 1;
            }
        }
        i += 1;
        n += 1;
    }
    Some(n)
}

/// A node of the call tree. Index 0 is the root.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    #[serde(rename = "f")]
    pub frame: u32,
    #[serde(rename = "p")]
    pub parent: u32,
    /// Weight of samples whose leaf is exactly this node.
    #[serde(rename = "s")]
    pub self_w: u64,
    /// Weight of samples passing through this node.
    #[serde(rename = "t")]
    pub total: u64,
    #[serde(rename = "k", default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<u32>,
}

/// Parent marker of the root.
pub const NO_PARENT: u32 = u32::MAX;

/// Samples that fell on one source line of one function (the editor's gutter heat).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotLine {
    pub frame: u32,
    pub line: u32,
    pub weight: u64,
}

/// The aggregated call tree of one recording.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CallTree {
    pub frames: Vec<FrameInfo>,
    pub nodes: Vec<Node>,
    pub hot_lines: Vec<HotLine>,
    /// Total weight (samples).
    pub total: u64,
    /// The node budget ran out: deeper stacks were folded into their parent.
    pub capped: bool,
}

/// One row of the hot-methods table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodStat {
    pub class: String,
    pub method: String,
    pub arity: Option<u32>,
    pub kind: FrameKind,
    pub file: Option<String>,
    pub decl_line: Option<u32>,
    pub self_w: u64,
    pub total_w: u64,
    /// `(line, weight)`, heaviest first, at most a handful.
    pub hot_lines: Vec<(u32, u64)>,
}

impl MethodStat {
    pub fn key(&self) -> String {
        format!("{}#{}", self.class, self.method)
    }
}

/// Allocation attributed to a type or to a site.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllocStat {
    pub name: String,
    pub count: u64,
    pub bytes: u64,
}

/// Lock contention on one monitor class.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockStat {
    pub monitor: String,
    /// The method that blocked most often on it.
    pub site: String,
    pub count: u64,
    pub total_ms: f64,
    pub max_ms: f64,
}

/// Garbage collection, one row per collector.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GcStat {
    pub collector: String,
    pub count: u64,
    pub total_pause_ms: f64,
    pub max_pause_ms: f64,
    /// Most frequent cause.
    pub top_cause: String,
}

/// Samples per thread.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadStat {
    pub name: String,
    pub weight: u64,
}

/// Everything one recording reduces to.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub lang: Lang,
    pub label: String,
    /// Unix milliseconds the recording was finished (and parsed).
    pub created_ms: u64,
    /// Wall-clock length of the recording, when known.
    pub duration_ms: Option<f64>,
    /// How many milliseconds one unit of tree weight stands for.
    pub ms_per_unit: f64,
    pub tree: CallTree,
    /// Every method with samples, heaviest self first (capped).
    pub methods: Vec<MethodStat>,
    pub alloc_sites: Vec<AllocStat>,
    pub alloc_types: Vec<AllocStat>,
    pub locks: Vec<LockStat>,
    pub gc: Vec<GcStat>,
    pub threads: Vec<ThreadStat>,
    /// Plain-language caveats shown with the recording (caps hit, unsymbolicated frames, …).
    pub notes: Vec<String>,
}

impl Profile {
    /// Self time of a method, in milliseconds.
    pub fn ms(&self, weight: u64) -> f64 {
        weight as f64 * self.ms_per_unit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arity_counts_objects_arrays_and_primitives() {
        assert_eq!(descriptor_arity("()V"), Some(0));
        assert_eq!(descriptor_arity("(I)V"), Some(1));
        assert_eq!(descriptor_arity("(ILjava/lang/String;[J[[Ljava/util/List;)Z"), Some(4));
        assert_eq!(descriptor_arity("garbage"), None);
    }
}
