//! Scale profiler: validate THOUSANDS of files through ONE shared resolver (exactly how
//! `bennu_validate_project` runs a whole project — the slot's provider/resolver is reused for every
//! file) and report the average per-file time BY POSITION BUCKET. If per-file time climbs from the
//! first bucket to the last, there is cross-file state accumulating (the "the more files, the slower
//! each file" symptom the user reported); if it stays flat, per-file cost is O(1) in the run length
//! and the total is just N × constant.
//!
//! Run: `cargo test -p bennu-check --test perf_scale --release -- --nocapture`
//!
//! The shared resolver here MEMOIZES like the real `IndexResolver` (a `RwLock<HashMap>` that grows as
//! new binary names are queried), and each file references a few UNIQUE types so the cache grows
//! unboundedly across the run — the worst case for "cache size slows lookups".

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Instant;

use bennu_check::prelude::{check_file_resolved, FileContext};
use bennu_java::prelude::{ClassFlags, ClassMembers, Import, Member, TypeRef, TypeResolver};

/// A resolver that memoizes every queried binary name (positive AND negative), exactly like the real
/// `IndexResolver` — so the shared cache grows monotonically across the whole run.
struct AccumResolver {
    cache: RwLock<HashMap<String, Option<Arc<ClassMembers>>>>,
    /// The shared framework type `p/P` (method `m()->P`, field `f:int`) every file uses — warms fast.
    p: Arc<ClassMembers>,
    /// Template members for the per-file UNIQUE types (`p/U…`) that make the cache grow without bound.
    unique: Arc<ClassMembers>,
    /// Count of `members_of` MISSES that had to compute (not a cache hit) — a cheap proxy for whether
    /// later files really are hitting the cache rather than recomputing.
    computed: RwLock<usize>,
}
impl AccumResolver {
    fn new() -> Self {
        let obj = Some(TypeRef::simple("java/lang/Object"));
        Self {
            cache: RwLock::new(HashMap::new()),
            p: Arc::new(ClassMembers {
                type_params: Vec::new(),
                superclass: obj.clone(),
                interfaces: Vec::new(),
                methods: vec![Member::method("m", TypeRef::simple("p/P"), Vec::new())],
                fields: vec![Member::field("f", TypeRef::simple("int"))],
                flags: ClassFlags::default(),
            }),
            unique: Arc::new(ClassMembers {
                type_params: Vec::new(),
                superclass: obj,
                interfaces: Vec::new(),
                methods: vec![Member::method("go", TypeRef::simple("int"), Vec::new())],
                fields: Vec::new(),
                flags: ClassFlags::default(),
            }),
        computed: RwLock::new(0),
        }
    }
    fn compute(&self, binary: &str) -> Option<Arc<ClassMembers>> {
        *self.computed.write().unwrap() += 1;
        if binary == "p/P" {
            Some(self.p.clone())
        } else if binary.starts_with("p/U") {
            Some(self.unique.clone())
        } else {
            None // deps / java.lang.Object → conservative miss (negatively cached)
        }
    }
}
impl TypeResolver for AccumResolver {
    fn members_of(&self, binary: &str) -> Option<Arc<ClassMembers>> {
        if let Some(hit) = self.cache.read().unwrap().get(binary) {
            return hit.clone();
        }
        let computed = self.compute(binary);
        self.cache.write().unwrap().insert(binary.to_string(), computed.clone());
        computed
    }
    fn resolve_simple_name(&self, name: &str, _imports: &[Import]) -> Option<String> {
        if name == "P" {
            Some("p/P".to_string())
        } else if let Some(rest) = name.strip_prefix('U') {
            // A per-file unique type name `U<idx>_<k>` → its own binary name (grows the cache).
            Some(format!("p/U{rest}"))
        } else {
            None
        }
    }
}

/// A medium, realistically-shaped file: `methods` methods, each using the shared `P` type (chained
/// calls, warm cache) plus a couple of file-UNIQUE `U<idx>_k` types (cold — grow the cache).
fn medium_file(idx: usize, methods: usize) -> String {
    let mut s = String::from("package p;\npublic class F {\n");
    s.push_str("  private static P make() { return null; }\n");
    for mth in 0..methods {
        s.push_str(&format!("  void m{mth}() {{\n"));
        s.push_str("    P p = make();\n");
        s.push_str("    int a = p.m().m().f;\n");
        s.push_str("    int b = p.m().f + a;\n");
        // Two file-unique types → two new binary names cached per method.
        s.push_str(&format!("    U{idx}_{mth} u = null;\n"));
        s.push_str(&format!("    int c = u.go() + b;\n"));
        s.push_str("  }\n");
    }
    s.push_str("}\n");
    s
}

// A PROFILER, not a regression guard: its assertions check that this file's own generator produced
// the shape it meant to, never that the code under test is fast. Left un-ignored it ran on every
// `cargo test`, in DEBUG — which is both the slowest way to run it and the least meaningful, since
// the numbers it prints only mean something optimised. Run it deliberately:
//
//     cargo test -p bennu-check --release --test perf_scale -- --ignored --nocapture
#[ignore = "profiler — run explicitly, in release; see the note above"]
#[test]
fn validate_thousands_of_files_no_temporal_drift() {
    const FILES: usize = 3000;
    const BUCKETS: usize = 10;
    const PER_BUCKET: usize = FILES / BUCKETS;

    let ctx = FileContext { java_major: Some(8), ..Default::default() };
    let resolver = AccumResolver::new();

    // Pre-generate the sources so file generation isn't inside the timed loop.
    let sources: Vec<String> = (0..FILES).map(|i| medium_file(i, 15)).collect();
    let lines = sources[0].lines().count();

    let mut bucket_ms = vec![0.0f64; BUCKETS];
    let run_start = Instant::now();
    for (i, src) in sources.iter().enumerate() {
        let t = Instant::now();
        let _ = check_file_resolved(src, &ctx, &resolver, true);
        bucket_ms[(i / PER_BUCKET).min(BUCKETS - 1)] += t.elapsed().as_secs_f64() * 1000.0;
    }
    let total = run_start.elapsed().as_secs_f64() * 1000.0;

    let cache_size = resolver.cache.read().unwrap().len();
    let computed = *resolver.computed.read().unwrap();

    eprintln!("\n════════ perf_scale: {FILES} files × ~{lines} lines, ONE shared resolver ════════");
    eprintln!("  total {total:.0} ms  ({:.3} ms/file avg)", total / FILES as f64);
    eprintln!("  resolver cache ended at {cache_size} entries, {computed} compute-misses total");
    eprintln!("  per-bucket average ms/file (drift check):");
    let first = bucket_ms[0] / PER_BUCKET as f64;
    for (b, sum) in bucket_ms.iter().enumerate() {
        let avg = sum / PER_BUCKET as f64;
        let lo = b * PER_BUCKET;
        let hi = (b + 1) * PER_BUCKET;
        eprintln!("    files {lo:>5}..{hi:<5}  {avg:>6.3} ms/file   ({:+.0}% vs bucket 0)", (avg / first - 1.0) * 100.0);
    }
    let last = bucket_ms[BUCKETS - 1] / PER_BUCKET as f64;
    eprintln!("  ⇒ last bucket vs first: {:+.1}%", (last / first - 1.0) * 100.0);
    eprintln!("════════════════════════════════════════════════════════════════════\n");

    // Guard: this is a report, not a strict benchmark — only assert the run completed and the cache
    // actually grew (so the drift check is meaningful).
    assert!(cache_size > FILES, "each file should add unique types to the shared cache");
}
