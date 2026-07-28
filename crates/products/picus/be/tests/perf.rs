//! Phase-by-phase timing harness for the Picus script pipeline.
//!
//! ## What this is for
//!
//! A real repository — ~500 SQL scripts, ~11 MB — took over five minutes to
//! index. That is not "slow", it is an algorithm with the wrong exponent, and the
//! only way to find which one is to time each phase separately at two sizes and
//! watch which number grows faster than the input.
//!
//! So this file builds a **synthetic repository on disk** that resembles the real
//! one (Italian folder names, `AGGIORNAMENTO/<version>/<ORA|POS>`, windows-1252
//! bytes, CRLF, a few very large files among many small ones) and runs the same
//! six phases `picus_be::scripts` runs, one `Instant` each:
//!
//! ```text
//! discover → decode → parse_all → ParsedProject::new → Inventory::build → analyze
//! ```
//!
//! Then it does it again at 4× the size. **A phase that quadruples when the input
//! quadruples is linear; one that grows 16× is quadratic**, and quadratic is the
//! answer.
//!
//! Two things are deliberately re-implemented here rather than called:
//!
//! * `picus-be` is a binary, so `scripts::read` / `scripts::parse_all` are not
//!   reachable from an integration test. The bodies below are transcriptions of
//!   them — if either changes shape, this file has to follow, and the comment on
//!   each phase says which function it mirrors.
//! * `picus-analyze` runs its fourteen rules behind one `analyze()`, so per-rule
//!   timing is not available from outside the crate. Instead there are **probes**:
//!   small replicas, built only from public API, of the two loops the code review
//!   flagged (`CONS001`'s lane sum, and `line_col`). A probe measures the shape of
//!   a cost, not the cost itself, and it is labelled as such in the output.
//!
//! ## This is a measurement, not a gate
//!
//! There are no timing assertions and there never should be: the numbers depend
//! on the machine, and a perf test that fails on a loaded CI box is a perf test
//! everybody learns to ignore. The only assertions are that the fixture is the
//! shape it claims to be. Run it deliberately:
//!
//! ```text
//! cargo test --release -p picus-be --test perf -- --nocapture
//! ```
//!
//! (`--release` matters: a debug tree-sitter parse is an order of magnitude
//! slower than the real thing and would drown the phase it is being compared to.)

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use arbor_fs::prelude::encoding::{decode_in_context, EncodingContext};
use picus_analyze::prelude::{analyze, Context};
use picus_analyze::suppress;
use picus_inventory::prelude::{Inventory, ParsedProject, ParsedScript};
use picus_parse::prelude::{line_col, DialectScope, EngineKind, ParsedFile, SqlParser};
use picus_project::prelude::{
    discover, label_to_encoding, FolderRole, Project, ProjectConfig, Proposal,
};

// ── the tests ─────────────────────────────────────────────────────────────────

/// The headline: the same repository at 125 and 500 files, phase by phase.
///
/// The scaling column is what to read. Everything about the fixture scales
/// together — files, bytes, folders, distinct object names — so a phase that is
/// linear in *any* of them lands near 4.0 and a phase quadratic in any pair of
/// them lands near 16.0.
#[test]
#[ignore = "a measurement, not a gate: minutes in debug. cargo test --release -p picus-be --test perf -- --ignored --nocapture"]
fn every_phase_of_the_pipeline_timed_at_two_repository_sizes() {
    let small = measure("picus-perf-125", 125);
    let large = measure("picus-perf-500", 500);

    eprintln!();
    eprintln!("═══ Picus pipeline — phase timings ═══════════════════════════════");
    eprintln!("{}", Fixture::header());
    eprintln!("{}", small.fixture.row());
    eprintln!("{}", large.fixture.row());
    eprintln!();
    eprintln!(
        "{:<26} {:>12} {:>12} {:>9}  {}",
        "phase", "125 files", "500 files", "×", "(4.0 = linear, 16.0 = quadratic)"
    );
    eprintln!("{}", "-".repeat(80));
    for (name, a, b) in Timings::rows(&small.timings, &large.timings) {
        eprintln!("{:<26} {:>12} {:>12} {:>9.1}", name, millis(a), millis(b), factor(a, b));
    }
    eprintln!("{}", "-".repeat(80));
    let (total_small, total_large) = (small.timings.total(), large.timings.total());
    eprintln!(
        "{:<26} {:>12} {:>12} {:>9.1}",
        "TOTAL",
        millis(total_small),
        millis(total_large),
        factor(total_small, total_large)
    );
    eprintln!();

    // The fixture is what it claims to be. These are the only real assertions:
    // if the generator drifts, the timings above stop meaning anything.
    assert_eq!(small.fixture.files, 125);
    assert_eq!(large.fixture.files, 500);
    assert!(large.fixture.bytes > 9_000_000, "the large fixture should be ~11 MB");
    assert!(large.fixture.objects > 50, "the analysis needs a real inventory to chew on");
}

/// Is a phase quadratic in the **size of one file**, independent of file count?
///
/// The 125→500 run above cannot tell: it holds the size distribution fixed while
/// scaling the count, so a per-file `O(bytes²)` cost shows up there as a merely
/// linear 4×. Here the count is fixed at one and only the file grows, so an `×`
/// near 4.0 for a 2× size step is a per-file quadratic caught red-handed.
#[test]
#[ignore = "a measurement, not a gate: minutes in debug. cargo test --release -p picus-be --test perf -- --ignored --nocapture"]
fn the_per_file_cost_of_one_growing_file() {
    eprintln!();
    eprintln!("═══ One file, growing — per-file scaling ═════════════════════════");
    eprintln!(
        "{:<12} {:>10} {:>12} {:>12} {:>12} {:>12}",
        "file size", "stmts", "parse", "inventory", "analyze", "suppress-scan"
    );
    eprintln!("{}", "-".repeat(76));

    let mut previous: Option<(usize, Duration, Duration, Duration, Duration)> = None;
    for chunks in [75usize, 150, 300, 600, 1200] {
        let root = repository(&format!("picus-perf-one-{chunks}"), OneFile { chunks });
        let mut read = phase_read(&root);
        let t = Instant::now();
        let parses = phase_parse(&read.snapshot);
        read.parse_hint = t.elapsed();

        let scripts = join(&read.snapshot, &parses);
        let joined = ParsedProject::new(&read.snapshot.project, scripts);

        let t = Instant::now();
        let inventory = Inventory::build(&joined);
        let inventory_time = t.elapsed();

        let t = Instant::now();
        let _ = analyze(&joined, &read.snapshot.config, &inventory);
        let analyze_time = t.elapsed();

        // The suppression pass on its own — it walks every `--` comment in the
        // file and asks `line_col` where each one is.
        let t = Instant::now();
        for (path, source, parsed) in triples(&read.snapshot, &parses) {
            let _ = suppress::scan(path, source, parsed);
        }
        let suppress_time = t.elapsed();

        let bytes = read.snapshot.total_bytes();
        let statements: usize = parses.iter().map(|(_, p)| p.statements.len()).sum();
        eprintln!(
            "{:<12} {:>10} {:>12} {:>12} {:>12} {:>12}",
            kib(bytes),
            statements,
            millis(read.parse_hint),
            millis(inventory_time),
            millis(analyze_time),
            millis(suppress_time)
        );
        if let Some((_, _, prev_inv, prev_analyze, prev_suppress)) = previous {
            eprintln!(
                "{:<12} {:>10} {:>12} {:>12.1} {:>12.1} {:>12.1}",
                "", "(× vs prev)", "", factor(prev_inv, inventory_time),
                factor(prev_analyze, analyze_time), factor(prev_suppress, suppress_time)
            );
        }
        previous = Some((bytes, read.parse_hint, inventory_time, analyze_time, suppress_time));
        let _ = std::fs::remove_dir_all(&root);
    }
    eprintln!("{}", "-".repeat(76));
    eprintln!("A 2× size step costing 4× is quadratic in the size of a single file.");
    eprintln!();
}

/// The same 11 MB, cut two different ways.
///
/// Both repositories hold ~500 files and ~11 MB of the same generated SQL. One
/// has the size distribution a real legacy repository has — four files of ~1 MB,
/// then a long tail of small ones; the other has every file the same 21 KB.
/// **The work is identical**: the same statements, the same objects, the same
/// rules over the same bytes.
///
/// So any difference between the two rows below is not work, it is the file-size
/// term of a quadratic — and it is what a fix would give back.
#[test]
#[ignore = "a measurement, not a gate: minutes in debug. cargo test --release -p picus-be --test perf -- --ignored --nocapture"]
fn the_same_bytes_cost_far_more_when_they_sit_in_a_few_large_files() {
    let tailed = measure("picus-perf-tailed", 500);
    let flat = measure_flat("picus-perf-flat", 500, 26);

    eprintln!();
    eprintln!("═══ Same bytes, different file sizes ═════════════════════════════");
    eprintln!("{}", Fixture::header());
    eprintln!("{}   long tail (≈1 MB monsters)", tailed.fixture.row());
    eprintln!("{}   flat (every file ≈21 KB)", flat.fixture.row());
    eprintln!();
    eprintln!("{:<26} {:>12} {:>12} {:>9}", "phase", "long tail", "flat", "×");
    eprintln!("{}", "-".repeat(64));
    for (name, a, b) in Timings::rows(&tailed.timings, &flat.timings) {
        eprintln!("{:<26} {:>12} {:>12} {:>9.1}", name, millis(a), millis(b), factor(b, a));
    }
    eprintln!("{}", "-".repeat(64));
    eprintln!(
        "{:<26} {:>12} {:>12} {:>9.1}",
        "TOTAL",
        millis(tailed.timings.total()),
        millis(flat.timings.total()),
        factor(flat.timings.total(), tailed.timings.total())
    );
    eprintln!();
    eprintln!("The `×` column is how many times more the long-tailed repository costs");
    eprintln!("for the same work. Anything above ~1 is the per-file quadratic.");
    eprintln!();
}

/// Probes for the two loops flagged by review, measured through public API only.
///
/// Neither of these is the real code — `CONS001`'s inner loop and `line_col`'s
/// call sites live behind `pub(crate)` — but both are transcriptions of it, and
/// what they establish is the *shape* of the cost, which is the question.
#[test]
#[ignore = "a measurement, not a gate: minutes in debug. cargo test --release -p picus-be --test perf -- --ignored --nocapture"]
fn probes_for_the_two_loops_that_look_quadratic() {
    eprintln!();
    eprintln!("═══ Probes ═══════════════════════════════════════════════════════");

    for files in [125usize, 500] {
        let root = repository(&format!("picus-perf-probe-{files}"), Legacy { files });
        let read = phase_read(&root);
        let parses = phase_parse(&read.snapshot);
        let scripts = join(&read.snapshot, &parses);
        let joined = ParsedProject::new(&read.snapshot.project, scripts);
        let inventory = Inventory::build(&joined);
        let context = Context::new(&joined, &read.snapshot.config, &inventory);

        // Probe 1 — `consistency::coverage_of`, which is
        // `context.lane(dialect, role).iter().map(|f| entry.coverage_in(&f.path)).sum()`
        // called from inside a loop over every inventory object, for four roles
        // and every dialect. `Context::lane` walks the whole tree and allocates a
        // Vec on every call.
        let dialects = context.dialects();
        let roles =
            [FolderRole::Init, FolderRole::Data, FolderRole::Update, FolderRole::Routines];
        let t = Instant::now();
        let mut sink = 0usize;
        let mut lane_calls = 0usize;
        for entry in &inventory.objects {
            for role in roles {
                for dialect in &dialects {
                    let lane = context.lane(*dialect, role);
                    lane_calls += 1;
                    sink += lane.iter().map(|f| entry.coverage_in(&f.path)).sum::<usize>();
                }
            }
        }
        let lane_probe = t.elapsed();

        // Probe 2 — `line_col`, which counts newlines from byte 0 every time it
        // is asked. Called once per inventory site, once per INSERT row, once per
        // finding and up to four times per suppression comment.
        let sites: usize = inventory.objects.iter().map(|o| o.sites.len()).sum();
        let t = Instant::now();
        let mut lines = 0usize;
        for object in &inventory.objects {
            for site in &object.sites {
                let source = read
                    .snapshot
                    .sources
                    .get(&site.path)
                    .map(|s| s.text.as_str())
                    .unwrap_or("");
                lines += line_col(source, site.range.start).0;
            }
        }
        let line_col_probe = t.elapsed();

        // Probe 3 — `Project::scope_of`, which `scripts::parse_one` calls once
        // per file. It is `folder_of`, which walks every folder and scans every
        // folder's file list looking for the path: a linear walk per file, over
        // the same tree `unparsable_paths` was careful to walk only once.
        let t = Instant::now();
        let mut scopes = 0usize;
        for path in read.snapshot.sources.keys() {
            scopes += usize::from(read.snapshot.project.scope_of(path).is_some());
        }
        let scope_of_probe = t.elapsed();

        eprintln!();
        eprintln!(
            "--- {files} files, {} folders, {} objects ---",
            folder_count(&read.snapshot.project),
            inventory.objects.len()
        );
        eprintln!(
            "  lane()/coverage_in probe : {:>10}  ({lane_calls} lane() calls, each a full tree walk)",
            millis(lane_probe)
        );
        eprintln!(
            "  line_col probe           : {:>10}  ({sites} sites re-scanned from byte 0)",
            millis(line_col_probe)
        );
        eprintln!(
            "  Project::scope_of probe  : {:>10}  ({files} files × one full tree walk each)",
            millis(scope_of_probe)
        );
        let _ = (sink, lines, scopes);
        let _ = std::fs::remove_dir_all(&root);
    }
    eprintln!();
}

// ── the six phases, mirroring picus-be ────────────────────────────────────────

#[derive(Default)]
struct Timings {
    discover: Duration,
    decode: Duration,
    parse: Duration,
    join: Duration,
    inventory: Duration,
    analyze: Duration,
    /// Not a phase — a slice of `analyze`, measured a second time on its own.
    suppress_slice: Duration,
}

impl Timings {
    fn total(&self) -> Duration {
        self.discover + self.decode + self.parse + self.join + self.inventory + self.analyze
    }

    fn rows(a: &Timings, b: &Timings) -> Vec<(&'static str, Duration, Duration)> {
        vec![
            ("discover (scan+sample)", a.discover, b.discover),
            ("decode (read every file)", a.decode, b.decode),
            ("parse_all (tree-sitter)", a.parse, b.parse),
            ("ParsedProject::new", a.join, b.join),
            ("Inventory::build", a.inventory, b.inventory),
            ("analyze (14 rules)", a.analyze, b.analyze),
            ("  └ suppress::scan only", a.suppress_slice, b.suppress_slice),
        ]
    }
}

struct Run {
    fixture: Fixture,
    timings: Timings,
}

/// Build a repository of `files` files and time the whole pipeline over it.
fn measure(name: &str, files: usize) -> Run {
    measure_shape(name, Legacy { files })
}

/// The same, with every file the same size.
fn measure_flat(name: &str, files: usize, chunks: usize) -> Run {
    measure_shape(name, Flat { files, chunks })
}

fn measure_shape(name: &str, shape: impl Shape) -> Run {
    let root = repository(name, shape);
    let mut timings = Timings::default();

    // ── 1 + 2. discover, then decode — `scripts::read`. ───────────────────────
    let read = phase_read(&root);
    timings.discover = read.discover;
    timings.decode = read.decode;
    let snapshot = read.snapshot;

    // ── 3. parse_all. ─────────────────────────────────────────────────────────
    let t = Instant::now();
    let parses = phase_parse(&snapshot);
    timings.parse = t.elapsed();

    // ── 4. ParsedProject::new. ────────────────────────────────────────────────
    let scripts = join(&snapshot, &parses);
    let t = Instant::now();
    let joined = ParsedProject::new(&snapshot.project, scripts);
    timings.join = t.elapsed();

    // ── 5. Inventory::build. ──────────────────────────────────────────────────
    let t = Instant::now();
    let inventory = Inventory::build(&joined);
    timings.inventory = t.elapsed();

    // ── 6. analyze. ───────────────────────────────────────────────────────────
    let t = Instant::now();
    let report = analyze(&joined, &snapshot.config, &inventory);
    timings.analyze = t.elapsed();

    // …and the suppression pass on its own, re-run, so its share of `analyze` is
    // visible without needing the crate to be instrumented.
    let t = Instant::now();
    for (path, source, parsed) in triples(&snapshot, &parses) {
        let _ = suppress::scan(path, source, parsed);
    }
    timings.suppress_slice = t.elapsed();

    let fixture = Fixture {
        files: snapshot.sources.len(),
        bytes: snapshot.total_bytes(),
        folders: folder_count(&snapshot.project),
        parsed: parses.len(),
        statements: parses.iter().map(|(_, p)| p.statements.len()).sum(),
        objects: inventory.objects.len(),
        columns: inventory.keys.len(),
        findings: report.findings.len(),
    };
    drop(joined);
    drop(parses);
    drop(snapshot);
    let _ = std::fs::remove_dir_all(&root);
    Run { fixture, timings }
}

struct ReadPhases {
    snapshot: Snapshot,
    discover: Duration,
    decode: Duration,
    /// Only filled in by the one-file test, which parses inside this helper.
    parse_hint: Duration,
}

/// A transcription of `picus_be::scripts::read` with the two halves timed apart.
fn phase_read(root: &Path) -> ReadPhases {
    let t = Instant::now();
    let proposal: Proposal = discover(root).expect("the fixture is a directory");
    let discover_time = t.elapsed();

    let t = Instant::now();
    let mut sources: BTreeMap<String, Source> = BTreeMap::new();
    for file in proposal.project.all_files() {
        let path = root.join(&file.path);
        let bytes = std::fs::read(&path).expect("the fixture was just written");
        let encoding = label_to_encoding(&file.encoding);
        let context = EncodingContext::new().with_legacy(encoding).with_dominant(encoding);
        let (text, _) = decode_in_context(&bytes, &context);
        sources.insert(file.path.clone(), Source { bytes: bytes.len(), text });
    }
    let decode_time = t.elapsed();

    ReadPhases {
        snapshot: Snapshot { project: proposal.project, config: proposal.config, sources },
        discover: discover_time,
        decode: decode_time,
        parse_hint: Duration::ZERO,
    }
}

/// A transcription of `picus_be::scripts::parse_all`, threading included.
fn phase_parse(snapshot: &Snapshot) -> Vec<(String, ParsedFile)> {
    let skip: HashSet<&str> = snapshot
        .project
        .walk()
        .filter(|folder| folder.engine_is_unsupported())
        .flat_map(|folder| folder.files.iter().map(|f| f.path.as_str()))
        .collect();
    let sources: Vec<(&String, &Source)> =
        snapshot.sources.iter().filter(|(path, _)| !skip.contains(path.as_str())).collect();

    let parse_one = |parser: &mut SqlParser, path: &String, source: &Source| {
        // The same fallback `scripts::scope_of` uses — and the same per-file
        // `Project::scope_of` call, which is itself a full tree walk.
        let scope =
            snapshot.project.scope_of(path).unwrap_or(DialectScope::One(EngineKind::Postgres));
        (path.clone(), parser.parse(&source.text, scope))
    };

    if sources.len() < 24 {
        let mut parser = SqlParser::new();
        return sources.iter().map(|(p, s)| parse_one(&mut parser, p, s)).collect();
    }
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .saturating_sub(1)
        .clamp(1, 8)
        .min(sources.len());
    let chunk = sources.len().div_ceil(threads);
    let mut parsed = Vec::with_capacity(sources.len());
    std::thread::scope(|scope| {
        let handles: Vec<_> = sources
            .chunks(chunk)
            .map(|slice| {
                scope.spawn(|| {
                    let mut parser = SqlParser::new();
                    slice
                        .iter()
                        .map(|(p, s)| parse_one(&mut parser, p, s))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        for handle in handles {
            parsed.extend(handle.join().expect("a parse thread panicked"));
        }
    });
    parsed
}

fn join<'a>(snapshot: &'a Snapshot, parses: &'a [(String, ParsedFile)]) -> Vec<ParsedScript<'a>> {
    parses
        .iter()
        .filter_map(|(path, parsed)| {
            snapshot
                .sources
                .get(path)
                .map(|source| ParsedScript { path: path.as_str(), source: &source.text, parsed })
        })
        .collect()
}

fn triples<'a>(
    snapshot: &'a Snapshot,
    parses: &'a [(String, ParsedFile)],
) -> Vec<(&'a str, &'a str, &'a ParsedFile)> {
    parses
        .iter()
        .filter_map(|(path, parsed)| {
            snapshot.sources.get(path).map(|s| (path.as_str(), s.text.as_str(), parsed))
        })
        .collect()
}

/// The bits of `ScriptSnapshot` this harness needs. `picus-core`'s own type is
/// not reachable without a running `PicusState`, and none of that is being timed.
struct Snapshot {
    project: Project,
    config: ProjectConfig,
    sources: BTreeMap<String, Source>,
}

impl Snapshot {
    fn total_bytes(&self) -> usize {
        self.sources.values().map(|s| s.bytes).sum()
    }
}

struct Source {
    bytes: usize,
    text: String,
}

fn folder_count(project: &Project) -> usize {
    project.walk().count()
}

// ── the fixture ───────────────────────────────────────────────────────────────

/// What was actually generated, printed above the timings so a number can always
/// be tied back to the workload that produced it.
struct Fixture {
    files: usize,
    bytes: usize,
    folders: usize,
    parsed: usize,
    statements: usize,
    objects: usize,
    columns: usize,
    findings: usize,
}

impl Fixture {
    fn header() -> String {
        format!(
            "{:>7} {:>10} {:>9} {:>8} {:>11} {:>9} {:>9} {:>10}",
            "files", "bytes", "folders", "parsed", "statements", "objects", "columns", "findings"
        )
    }

    fn row(&self) -> String {
        format!(
            "{:>7} {:>10} {:>9} {:>8} {:>11} {:>9} {:>9} {:>10}",
            self.files,
            self.bytes,
            self.folders,
            self.parsed,
            self.statements,
            self.objects,
            self.columns,
            self.findings
        )
    }
}

/// How to lay a synthetic repository out.
trait Shape {
    /// `(project-relative path, number of content chunks)` for every file.
    fn plan(&self) -> Vec<(String, usize)>;
}

/// The real thing, in miniature: Italian folder names, both dialects, a version
/// folder per delivered release, and a size distribution with a long tail —
/// three files of several hundred KB, a dozen of ~120 KB, the rest small.
struct Legacy {
    files: usize,
}

/// The folder layout both shapes use, so a `Legacy` and a `Flat` repository of
/// the same file count differ in **nothing but the sizes of the files**.
///
/// 20% initialisation, 10% reference data, 10% routines, 60% updates — roughly
/// the split of the repository this product was built for — with three files per
/// `(version, engine)` folder, which is what produces the hundred-odd folders a
/// real repository of this size has.
fn layout(n: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(n);
    let init = n / 5;
    let data = n / 10;
    let routines = n / 10;
    let updates = n - init - data - routines;

    for (i, engine) in (0..init).map(|i| (i, dialect_dir(i))) {
        out.push(format!("INIZIALIZZAZIONE/{engine}/{:03}_TABELLE.sql", i / 2));
    }
    for (i, engine) in (0..data).map(|i| (i, dialect_dir(i))) {
        out.push(format!("DATI/{engine}/{:03}_ANAGRAFICHE.sql", i / 2));
    }
    for (i, engine) in (0..routines).map(|i| (i, dialect_dir(i))) {
        out.push(format!("PROCEDURE/{engine}/{:03}_PKG.sql", i / 2));
    }
    for i in 0..updates {
        let engine = dialect_dir(i);
        let version = i / 6;
        let step = i % 3;
        out.push(format!(
            "AGGIORNAMENTO/4_{version:02}/{engine}/4_{version:02}__4_{:02}_{step}.sql",
            version + 1
        ));
    }
    out
}

impl Shape for Legacy {
    fn plan(&self) -> Vec<(String, usize)> {
        let n = self.files;
        let mut out: Vec<(String, usize)> =
            layout(n).into_iter().map(|path| (path, 0usize)).collect();

        // The long tail, which is the part that matters. A real repository of
        // this kind is mostly small files with a handful of monsters in it — the
        // consolidated "tutte le tabelle" script, the reference-data dump — and
        // the monsters are where a per-file quadratic hides. The tiers scale with
        // the file count so the two runs stay proportional; the totals land near
        // 22 KB × count, i.e. ~11 MB at 500 files.
        let huge = (n / 125).max(1); //  ≈ 990 KB each
        let big = huge + (n / 50).max(1); //  ≈ 290 KB each
        let medium = big + (n / 25).max(1); //  ≈  82 KB each
        for (i, entry) in out.iter_mut().enumerate() {
            entry.1 = if i < huge {
                1200
            } else if i < big {
                350
            } else if i < medium {
                100
            } else {
                6
            };
        }
        out
    }
}

/// The same repository with the tail flattened: every file identical in size.
/// The control for [`Legacy`].
struct Flat {
    files: usize,
    chunks: usize,
}

impl Shape for Flat {
    fn plan(&self) -> Vec<(String, usize)> {
        layout(self.files).into_iter().map(|path| (path, self.chunks)).collect()
    }
}

/// One file, of a size the caller chooses — for the per-file scaling test.
struct OneFile {
    chunks: usize,
}

impl Shape for OneFile {
    fn plan(&self) -> Vec<(String, usize)> {
        vec![("AGGIORNAMENTO/4_00/ORA/4_00__4_01.sql".to_string(), self.chunks)]
    }
}

/// `ORA` and `POS` alternating — the two leaf folder names the real repository
/// uses. `ORA` is in the built-in vocabulary; `POS` is declared by the alias
/// written into `project.toml` below, exactly as a configured project would.
fn dialect_dir(i: usize) -> &'static str {
    if i % 2 == 0 {
        "ORA"
    } else {
        "POS"
    }
}

/// Write a synthetic repository under the system temp dir and return its root.
fn repository(name: &str, shape: impl Shape) -> PathBuf {
    let root = std::env::temp_dir().join(name);
    let _ = std::fs::remove_dir_all(&root);

    // A configured project, so both dialects resolve and every cross-dialect rule
    // actually runs. Without the alias, `POS` is unclassified, the consistency
    // rules stand down, and the analysis measured would be the cheap one.
    let config = root.join(".arbor/picus/project.toml");
    std::fs::create_dir_all(config.parent().expect("has a parent")).expect("mkdir .arbor");
    std::fs::write(
        &config,
        "version = 2\nname = \"PROD_CORE\"\n\n\
         [[alias]]\nname = \"POS\"\nengine = \"postgres\"\n",
    )
    .expect("write project.toml");

    let plan = shape.plan();
    // How many distinct tables the repository names. Proportional to its size,
    // because a bigger repository really does have more objects in it — and the
    // inventory's row count is one of the two factors in the cross-dialect rules.
    let names = (plan.len() * 2).max(40);
    for (index, (relative, chunks)) in plan.into_iter().enumerate() {
        let path = root.join(&relative);
        std::fs::create_dir_all(path.parent().expect("has a parent")).expect("mkdir");
        let oracle = relative.contains("/ORA/");
        let text = legacy_sql(index, chunks, oracle, names);
        // windows-1252 on disk, like the real repository — so the decode phase is
        // doing the transcoding it does in production rather than a memcpy.
        let bytes = label_to_encoding("windows-1252").encode(&text).0.into_owned();
        std::fs::write(&path, &bytes).expect("write");
    }
    root
}

/// One synthetic script: a header, then `chunks` blocks of the kind of SQL these
/// repositories are actually full of.
///
/// The proportions matter more than the prose. Legacy install scripts are
/// **comment-heavy** — a banner above every statement — and comments are what the
/// suppression pass walks; they are dense in INSERTs, which is what the
/// consistency and duplicate rules compare; and they carry at least one PL/SQL
/// block, which is what makes the parse non-trivial.
fn legacy_sql(seed: usize, chunks: usize, oracle: bool, names: usize) -> String {
    let mut out = String::with_capacity(chunks * 2200 + 512);
    out.push_str("-- ============================================================\r\n");
    out.push_str("-- Script di manutenzione — generato per la verifica di coerenza\r\n");
    out.push_str("-- Autore: manutenzione applicativa (città di riferimento: Città)\r\n");
    out.push_str("-- Nota: già applicato in produzione, però va riverificato\r\n");
    out.push_str("-- ============================================================\r\n\r\n");

    let mut rng = Lcg::new(seed as u64 + 1);
    for chunk in 0..chunks {
        // Drawn from a pool, so objects recur across files and across dialects —
        // an inventory where every row had exactly one site would not exercise
        // the cross-dialect rules at all.
        let table = format!("T_{:04}", rng.next_below(names));
        let other = format!("T_{:04}", rng.next_below(names));

        out.push_str(&format!(
            "-- ------------------------------------------------------------\r\n\
             -- Blocco {chunk}: aggiornamento di {table}\r\n\
             -- Motivo: la soglia precedente non era più valida perché il\r\n\
             --         calcolo veniva eseguito prima della verifica\r\n\
             -- ------------------------------------------------------------\r\n"
        ));

        if chunk % 7 == 0 {
            out.push_str(&format!(
                "CREATE TABLE {table} (\r\n\
                 \x20 COD          {},\r\n\
                 \x20 DESCRIZIONE  {},\r\n\
                 \x20 VALORE       {},\r\n\
                 \x20 ATTIVO       {}\r\n);\r\n",
                if oracle { "VARCHAR2(30)" } else { "varchar(30)" },
                if oracle { "VARCHAR2(200)" } else { "varchar(200)" },
                if oracle { "NUMBER(10,2)" } else { "numeric(10,2)" },
                if oracle { "CHAR(1)" } else { "char(1)" },
            ));
        }

        out.push_str(&format!(
            "INSERT INTO {table} (COD, DESCRIZIONE, VALORE, ATTIVO) VALUES \
             ('COD_{:05}', 'Descrizione qualità {:03}', {}, 'S');\r\n",
            rng.next_below(9000),
            rng.next_below(999),
            rng.next_below(500)
        ));
        out.push_str(&format!(
            "INSERT INTO {table} (COD, DESCRIZIONE, VALORE, ATTIVO) VALUES \
             ('COD_{:05}', 'Voce già presente', {}, 'N');\r\n",
            rng.next_below(9000),
            rng.next_below(500)
        ));
        out.push_str("-- la riga sopra è stata corretta il mese scorso\r\n");
        out.push_str(&format!(
            "UPDATE {table} SET VALORE = VALORE + {}, DESCRIZIONE = 'Rivisto perché errato' \
             WHERE COD LIKE 'COD_1%';\r\n",
            rng.next_below(20)
        ));
        out.push_str(&format!(
            "DELETE FROM {other} WHERE COD IN (SELECT COD FROM {table} WHERE ATTIVO = 'N');\r\n"
        ));

        if chunk % 5 == 0 {
            if oracle {
                out.push_str(&format!(
                    "DECLARE\r\n\
                     \x20 v_conta NUMBER;\r\n\
                     BEGIN\r\n\
                     \x20 SELECT COUNT(*) INTO v_conta FROM {table} WHERE ATTIVO = 'S';\r\n\
                     \x20 IF v_conta > 0 THEN\r\n\
                     \x20   -- si aggiorna solo se c'è già qualcosa\r\n\
                     \x20   UPDATE {other} SET VALORE = NVL(VALORE, 0) + 1 WHERE COD = 'COD_00001';\r\n\
                     \x20 END IF;\r\n\
                     END;\r\n/\r\n"
                ));
            } else {
                out.push_str(&format!(
                    "DO $$\r\n\
                     BEGIN\r\n\
                     \x20 IF EXISTS (SELECT 1 FROM {table} WHERE ATTIVO = 'S') THEN\r\n\
                     \x20   -- si aggiorna solo se c'è già qualcosa\r\n\
                     \x20   UPDATE {other} SET VALORE = COALESCE(VALORE, 0) + 1 WHERE COD = 'COD_00001';\r\n\
                     \x20 END IF;\r\n\
                     END $$;\r\n"
                ));
            }
        }
        out.push_str("\r\n");
    }

    // The version bump every real update script ends with, so `VER002` is not
    // firing on every file in the fixture and drowning the finding list.
    out.push_str("UPDATE VERSIONE_DB SET VERSIONE = '4.12' WHERE VERSIONE = '4.11';\r\n");
    out
}

/// A tiny deterministic generator, so two runs of this harness build byte-for-byte
/// the same repository and two timings are comparable.
struct Lcg(u64);

impl Lcg {
    fn new(seed: u64) -> Lcg {
        Lcg(seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407))
    }

    fn next_below(&mut self, bound: usize) -> usize {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 33) as usize) % bound.max(1)
    }
}

// ── printing ──────────────────────────────────────────────────────────────────

fn millis(d: Duration) -> String {
    format!("{:.1} ms", d.as_secs_f64() * 1000.0)
}

fn kib(bytes: usize) -> String {
    format!("{} KiB", bytes / 1024)
}

/// How much slower `b` is than `a`. Guarded against a zero baseline so a phase
/// that is genuinely instantaneous prints `0.0` rather than `inf`.
fn factor(a: Duration, b: Duration) -> f64 {
    let a = a.as_secs_f64();
    if a <= 0.0 {
        return 0.0;
    }
    b.as_secs_f64() / a
}
