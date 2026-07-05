//! TEMPORARY real-project profiler. Builds the REAL index + resolver (project fst index + JDK member
//! index over the installed JDK's bytecode) for a checked-out Maven project, then validates every file
//! and reports total / average / slowest, with the per-check breakdown (set `BENNU_PROFILE=1`) for the
//! slow files. This is the only way to see where validation time REALLY goes on real project types —
//! a mock resolver can't reproduce the bytecode/fst/wildcard cost.
//!
//! Run (PowerShell):
//!   $env:BENNU_PROFILE=1; $env:BENNU_TEST_PROJECT="C:\Sviluppo\Mio\temp\disposable-projects\e-procurement-Appalti"
//!   cargo test -p bennu-intel --test real_project_profile --release -- --ignored --nocapture
//!
//! `#[ignore]` so it never runs in the normal suite (needs a JDK + a big checkout on disk).

use std::path::{Path, PathBuf};
use std::time::Instant;

use bennu_check::prelude::FileContext;
use bennu_intel::prelude::{
    build_project_index_from_sources, parallel_map, read_java_sources, source_hash, DiagCache,
    FileDeps, NativeJavaProvider,
};

#[test]
#[ignore]
fn profile_real_project() {
    let root = std::env::var("BENNU_TEST_PROJECT")
        .unwrap_or_else(|_| r"C:\Sviluppo\Mio\temp\disposable-projects\e-procurement-Appalti".to_string());
    let root = PathBuf::from(root);
    assert!(root.exists(), "set BENNU_TEST_PROJECT to a checked-out project ({})", root.display());

    // The resolved JDK to validate against (JAVA_HOME is 21 on this box). Version-gating uses the
    // project's own target (1.5 here) but that's cosmetic for timing.
    let jdk_version = std::env::var("BENNU_TEST_JDK").unwrap_or_else(|_| "21".to_string());

    // ── build the real index (parse all sources, persist fst) ─────────────────────────────────
    let t = Instant::now();
    let sources = read_java_sources(&root, "UTF-8").sources;
    eprintln!("read+decoded {} sources in {:?}", sources.len(), t.elapsed());

    let index_dir = std::env::temp_dir().join(format!("bennu-real-profile-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&index_dir);
    let t = Instant::now();
    let built = build_project_index_from_sources(&sources, &index_dir);
    built.builder.persist().expect("persist index");
    let pairs: Vec<(String, String)> = built.type_map.into_iter().collect();
    eprintln!("built+persisted index ({} types) in {:?}", pairs.len(), t.elapsed());

    // ── build the real provider (project index + JDK bytecode resolver) ───────────────────────
    let t = Instant::now();
    let provider = NativeJavaProvider::for_project(&index_dir, &jdk_version, &pairs, None)
        .expect("provider (is the JDK resolvable?)");
    eprintln!("built provider in {:?}", t.elapsed());

    // ── validate every file, timing each ──────────────────────────────────────────────────────
    let mut stats: Vec<(String, u128, usize, usize)> = Vec::new(); // (file, ms, lines, diags)
    let run = Instant::now();
    for (path, src) in &sources {
        let file_stem = path.file_stem().and_then(|s| s.to_str()).map(str::to_string);
        let ctx = FileContext { file_stem, expected_package: None, java_major: Some(5) };
        let t = Instant::now();
        let diags = provider.validate(src, &ctx, true);
        let ms = t.elapsed().as_millis();
        stats.push((path.to_string_lossy().into_owned(), ms, src.lines().count(), diags.len()));
    }
    let total = run.elapsed();

    let total_ms: u128 = stats.iter().map(|s| s.1).sum();
    let n = stats.len().max(1);
    eprintln!("\n════════ validated {} files in {:?} (sum {}ms, avg {:.1}ms) ════════", stats.len(), total, total_ms, total_ms as f64 / n as f64);
    stats.sort_by(|a, b| b.1.cmp(&a.1));
    eprintln!("  slowest 15 files:");
    for (file, ms, lines, diags) in stats.iter().take(15) {
        let short = file.rsplit(['/', '\\']).next().unwrap_or(file);
        eprintln!("    {ms:>6} ms  {lines:>6} lines  {diags:>4} diags  {short}");
    }
    eprintln!("════════════════════════════════════════════════════════════════════\n");

    let _ = std::fs::remove_dir_all(&index_dir);
}

/// INCREMENTAL diagnostic-cache profile on the real project: measure a COLD full validation
/// (recording every file's project dependencies into the cache), then a WARM one (served from the
/// cache), then a single-file edit (only the changed file re-validates). This is the "instant
/// re-build" the symbol-table cache buys — the warm pass should be a small fraction of the cold one.
///
/// Run (PowerShell):
///   $env:BENNU_TEST_PROJECT="C:\Sviluppo\Mio\temp\disposable-projects\e-procurement-Appalti"
///   cargo test -p bennu-intel --test real_project_profile --release -- --ignored --nocapture profile_diag_cache
#[test]
#[ignore]
fn profile_diag_cache() {
    let root = std::env::var("BENNU_TEST_PROJECT")
        .unwrap_or_else(|_| r"C:\Sviluppo\Mio\temp\disposable-projects\e-procurement-Appalti".to_string());
    let root = PathBuf::from(root);
    assert!(root.exists(), "set BENNU_TEST_PROJECT to a checked-out project ({})", root.display());
    let jdk_version = std::env::var("BENNU_TEST_JDK").unwrap_or_else(|_| "21".to_string());

    let mut sources = read_java_sources(&root, "UTF-8").sources;
    eprintln!("read {} sources", sources.len());
    let index_dir = std::env::temp_dir().join(format!("bennu-real-diagcache-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&index_dir);
    let built = build_project_index_from_sources(&sources, &index_dir);
    built.builder.persist().expect("persist index");
    let pairs: Vec<(String, String)> = built.type_map.into_iter().collect();
    let provider = NativeJavaProvider::for_project(&index_dir, &jdk_version, &pairs, None)
        .expect("provider (is the JDK resolvable?)");

    // One validation pass over `sources`, consulting + filling `cache` the way the be layer's
    // whole-project validation does per file. Returns (wall-clock, cache-hits, total-diagnostics).
    let pass = |provider: &NativeJavaProvider, sources: &[(PathBuf, String)], cache: &mut DiagCache| {
        let mut hits = 0usize;
        let mut diags_total = 0usize;
        let run = Instant::now();
        for (path, src) in sources {
            let file = path.to_string_lossy().replace('\\', "/");
            let ctx = FileContext {
                file_stem: path.file_stem().and_then(|s| s.to_str()).map(str::to_string),
                expected_package: None,
                java_major: Some(5),
            };
            let own = source_hash(src);
            if let Some(view) = provider.project_view() {
                if let Some(cached) = cache.get_fresh(&file, own, view) {
                    hits += 1;
                    diags_total += cached.len();
                    continue;
                }
            }
            let (diags, recorded) = provider.validate_recording(src, &ctx, true);
            diags_total += diags.len();
            cache.put(&file, FileDeps::from_recorded(own, &recorded), diags);
        }
        (run.elapsed(), hits, diags_total)
    };

    // COLD, single-thread (baseline) — into its own cache.
    let mut cache_seq = DiagCache::new(1);
    let (cold, cold_hits, cold_diags) = pass(&provider, &sources, &mut cache_seq);

    // COLD, PARALLEL (the be layer's real path: a read-only pass producing per-file results, folded
    // into the cache afterwards) — the same work spread over the work-stealing pool.
    let empty = DiagCache::new(1);
    let par_run = Instant::now();
    let par_results: Vec<(String, Vec<_>, FileDeps)> = parallel_map(&sources, |(path, src)| {
        let file = path.to_string_lossy().replace('\\', "/");
        let ctx = FileContext {
            file_stem: path.file_stem().and_then(|s| s.to_str()).map(str::to_string),
            expected_package: None,
            java_major: Some(5),
        };
        let own = source_hash(src);
        let (diags, recorded) = provider.validate_recording(src, &ctx, true);
        (file, diags, FileDeps::from_recorded(own, &recorded))
    });
    let cold_par = par_run.elapsed();
    let mut cache = DiagCache::new(1);
    let mut par_diags = 0usize;
    for (file, diags, deps) in par_results {
        par_diags += diags.len();
        cache.put(&file, deps, diags);
    }
    let _ = empty;

    let (warm, warm_hits, warm_diags) = pass(&provider, &sources, &mut cache);

    // Single-file edit: append a comment to the first source (changes its bytes, NOT any type's
    // members → only that file re-validates; dependents stay cached).
    if let Some((_p, first)) = sources.first_mut() {
        first.push_str("\n// incremental-cache probe\n");
    }
    let (incr, incr_hits, _incr_diags) = pass(&provider, &sources, &mut cache);

    let n = sources.len();
    let par_speedup = cold.as_secs_f64() / cold_par.as_secs_f64().max(1e-9);
    eprintln!("\n════════ diagnostic-cache profile ({n} files) ════════");
    eprintln!("  COLD single-thread : {cold:>10.2?}  hits {cold_hits:>5}/{n}  diags {cold_diags}");
    eprintln!("  COLD parallel      : {cold_par:>10.2?}  diags {par_diags}   ({par_speedup:.1}× vs single-thread)");
    eprintln!("  WARM  (all cached) : {warm:>10.2?}  hits {warm_hits:>5}/{n}  diags {warm_diags}");
    eprintln!("  EDIT 1 file        : {incr:>10.2?}  hits {incr_hits:>5}/{n}  (misses {})", n - incr_hits);
    let speedup = cold_par.as_secs_f64() / warm.as_secs_f64().max(1e-9);
    eprintln!("  warm speedup (vs parallel cold): {speedup:>6.1}×");
    eprintln!("  cold vs warm diags identical: {}", cold_diags == warm_diags);
    eprintln!("  parallel == single-thread diags: {}", cold_diags == par_diags);
    eprintln!("════════════════════════════════════════════════════\n");

    // Correctness sanity: warm serves the SAME total diagnostics as cold (no drift), the parallel
    // pass agrees with single-thread (no races), and re-editing one file re-validates only a tiny
    // fraction of the project.
    assert_eq!(cold_diags, par_diags, "parallel validation must agree with single-thread");
    assert_eq!(cold_diags, warm_diags, "warm cache must serve identical diagnostics");
    assert_eq!(warm_hits, n, "every file cached on the warm pass");
    assert!(n - incr_hits <= 25, "a one-file edit re-validates only that file + a few dependents");

    let _ = std::fs::remove_dir_all(&index_dir);
}
