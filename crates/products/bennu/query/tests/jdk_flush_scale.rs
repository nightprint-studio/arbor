//! Reproduces the "the more files, the slower each file" drift in the PERSISTENT JDK member index.
//!
//! On a legacy project whose dependencies aren't indexed, every distinct library type is queried
//! through `JdkMemberIndex::members_of`, misses the JDK, and is memoized. The persistent index writes
//! the WHOLE memo back to disk every `FLUSH_EVERY` fresh entries — so as the memo grows, each flush
//! (clone + JSON-serialize + write the entire map) gets more expensive: O(K²) in the number of
//! distinct types K seen across the run. That K grows with the number of files validated, which is
//! why per-file validation slows down over a project.
//!
//! This test queries thousands of DISTINCT missing names through one persistent index and reports the
//! average time per query BY POSITION BUCKET. Rising buckets == the drift. After the fix (persist only
//! real JDK classes, not dependency misses) the buckets are flat.
//!
//! Run: `cargo test -p bennu-query --test jdk_flush_scale --release -- --nocapture`

use std::time::Instant;

use bennu_classpath::prelude::ClassSource;
use bennu_query::prelude::JdkMemberIndex;
use bennu_classpath::prelude::MemberIndex;

/// A source that has NO class — every lookup is a definitive miss, exactly like a dependency type
/// that isn't on the (unindexed) classpath. Cheap, so the measured cost is the index's own
/// bookkeeping (the persistent flush), not bytecode decoding.
struct MissSource;
impl ClassSource for MissSource {
    fn class_bytes(&self, _binary_name: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(None)
    }
}

#[test]
fn persistent_jdk_index_scales_flat_over_many_misses() {
    const QUERIES: usize = 20_000;
    const BUCKETS: usize = 10;
    const PER_BUCKET: usize = QUERIES / BUCKETS;

    let dir = std::env::temp_dir().join(format!("bennu-jdk-flush-scale-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("jdk-memo.json");
    let _ = std::fs::remove_file(&path);

    let idx = JdkMemberIndex::persistent(Box::new(MissSource), path.clone());

    let mut bucket_ms = vec![0.0f64; BUCKETS];
    let run = Instant::now();
    for i in 0..QUERIES {
        // A distinct, non-JDK binary name per query — the "unindexed dependency type" case.
        let name = format!("com/dep/pkg{i}/Type{i}");
        let t = Instant::now();
        let _ = idx.members_of(&name);
        bucket_ms[(i / PER_BUCKET).min(BUCKETS - 1)] += t.elapsed().as_secs_f64() * 1000.0;
    }
    let total = run.elapsed().as_secs_f64() * 1000.0;
    idx.flush();

    let file_bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    eprintln!("\n════════ jdk_flush_scale: {QUERIES} distinct missing lookups (persistent) ════════");
    eprintln!("  total {total:.0} ms   persisted file = {} KB", file_bytes / 1024);
    let first = bucket_ms[0] / PER_BUCKET as f64;
    for (b, sum) in bucket_ms.iter().enumerate() {
        let avg = sum / PER_BUCKET as f64;
        let lo = b * PER_BUCKET;
        let hi = (b + 1) * PER_BUCKET;
        eprintln!("    q {lo:>6}..{hi:<6}  {avg:>7.4} ms/lookup   ({:+.0}% vs bucket 0)", (avg / first - 1.0) * 100.0);
    }
    let last = bucket_ms[BUCKETS - 1] / PER_BUCKET as f64;
    let drift = (last / first - 1.0) * 100.0;
    eprintln!("  ⇒ last bucket vs first: {drift:+.0}%");
    eprintln!("════════════════════════════════════════════════════════════════════\n");

    let _ = std::fs::remove_dir_all(&dir);

    // After the fix (dependency misses no longer bloat the persisted memo / drive O(K²) flushes),
    // the last bucket must not be dramatically slower than the first. Generous ceiling so a loaded
    // box isn't flaky, but a quadratic regression (which was hundreds of %) trips it.
    assert!(drift < 150.0, "per-lookup time drifts upward over the run (persistent-flush O(K^2)?): {drift:+.0}%");
}
