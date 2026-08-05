//! `find` domain — `bennu_find_in_files`, powering the project-wide text search.
//!
//! A **fresh**, line-oriented scan of the project's text files under `root` (no
//! persisted index needed), mirroring [`crate::todos`] / [`crate::class_index`]'s walk:
//! recurse the tree, skip `target/`, `.git/`, `node_modules/`, `.idea/`, skip anything
//! that isn't valid UTF-8, and match a caller-supplied `query` per line.
//!
//! Matching modes ([`FindInFilesArgs`]):
//!   * `regex` — **fallback** to a case-insensitive substring match. The `regex` crate is
//!     NOT a dependency of `bennu-be` (adding it needs approval), so `regex == true` is
//!     honoured as "loose, case-insensitive substring" rather than a true engine. The FE
//!     is free to surface this as a degraded mode.
//!   * plain substring — respects `case_sensitive`.
//!   * `whole_word` — bounds the match on `[A-Za-z0-9_]` word boundaries (so `Foo` does
//!     not match inside `FooBar`).
//!
//! ## Streaming contract (progressive search)
//!
//! `bennu_find_in_files` is **fire-and-forget**: it validates the args, spawns a
//! **background `std::thread`** to walk the tree, and returns `Ok(())` immediately, so the
//! IPC dispatcher never blocks on a long scan of a huge legacy tree (a plain thread is the
//! right fit — the walk does no reverse-channel round-trips, mirroring
//! [`crate::index_service`]'s background build). The thread emits results as it finds them
//! on the [`EVT_FIND_PROGRESS`] topic, keyed by the caller's `search_id`:
//!
//!   * `{ "id": <search_id>, "hits": FindHit[] }` — one or more **batches** as matches are
//!     found. A batch is flushed per-scanned-file and whenever it reaches
//!     [`BATCH_SIZE`] hits, so the FE list fills incrementally instead of waiting for the
//!     whole scan.
//!   * `{ "id": <search_id>, "done": true, "capped": <bool> }` — exactly **one** terminal
//!     event when the scan finishes. `capped` is whether the [`MAX_HITS`] cap stopped the
//!     walk early.
//!
//! One [`FindHit`] is emitted per matched line (first match on the line drives `col`). The
//! walk caps at [`MAX_HITS`] results (reported in the terminal `capped` flag; also logged
//! to stderr, never erroring).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use arbor_ipc::prelude::EventSink;
use bennu_core::prelude::BennuState;
use bennu_proto::prelude::FindHit;
use serde::Deserialize;
use serde_json::json;

/// File extensions scanned for text matches. Files with no extension are scanned too when
/// their name starts with `.` (dotfiles like `.gitignore` / `.editorconfig`), handled in
/// [`is_scannable`].
///
/// An allow-list rather than a deny-list of binaries, so a `.jar` / `.png` is never read
/// into memory to be searched. `rs` / `toml` / `ron` / `dig` are here for the Rust side:
/// a project-wide search that silently skipped every `.rs` file would look like a broken
/// search, not a narrow one.
const SCAN_EXTS: [&str; 19] = [
    "java", "xml", "jsp", "jspf", "tag", "properties", "js", "css", "html", "sql", "yml",
    "yaml", "md", "txt", "jspx",
    // Rust projects: sources, manifests, RON game data, and geode's `.dig` scripts.
    "rs", "toml", "ron", "dig",
];

/// Directory names never descended into during the scan (mirrors [`crate::todos`]).
const SKIP_DIRS: [&str; 4] = ["target", ".git", "node_modules", ".idea"];

/// Upper bound on returned hits — a project-wide search on a huge legacy tree can match a
/// lot; capping keeps the payload bounded. Reported in the terminal event's `capped` flag
/// (and logged, never errored) when hit.
const MAX_HITS: usize = 5000;

/// Max length of the captured `preview` per hit (chars, not bytes).
const MAX_PREVIEW_LEN: usize = 300;

/// Flush a `find-progress` batch once this many hits have accumulated (a batch is also
/// flushed at each file boundary), so results appear promptly on the FE rather than in one
/// end-of-scan dump. Small on purpose — the FE appends batches as they arrive.
const BATCH_SIZE: usize = 40;

/// The BE→FE find-progress topic. Payloads (keyed by the caller's `search_id`):
/// `{ "id": <string>, "hits": FindHit[] }` for each result batch, then exactly one terminal
/// `{ "id": <string>, "done": true, "capped": <bool> }` when the scan finishes.
const EVT_FIND_PROGRESS: &str = "arbor://bennu/find-progress";

/// Args for [`bennu_find_in_files`].
#[derive(Deserialize)]
pub struct FindInFilesArgs {
    /// Absolute path to the (active) project root to scan.
    pub root: String,
    /// Additional project roots to scan too (the other workspace projects, when the search is
    /// scoped to the whole workspace). Empty for a single-project / active-project-only search.
    #[serde(default)]
    pub extra_roots: Vec<String>,
    /// The text (or, in `regex` fallback mode, the substring) to find.
    pub query: String,
    /// Regex mode. NOTE: the `regex` crate isn't a dependency, so this falls back to a
    /// case-insensitive substring match (see the module doc).
    #[serde(default)]
    pub regex: bool,
    /// Case-sensitive matching (ignored in `regex` fallback mode, which is always
    /// case-insensitive).
    #[serde(default)]
    pub case_sensitive: bool,
    /// Bound the match on `[A-Za-z0-9_]` word boundaries.
    #[serde(default)]
    pub whole_word: bool,
    /// Also search the **text entries of the project's dependency jars** — the `struts-default.xml`
    /// that declares an interceptor stack, a schema, a bundled `.properties`.
    ///
    /// Opt-in, and off by default, because it is a different order of cost from walking the
    /// project tree: every candidate entry is decompressed to be read (see [`scan_jars`]). It is
    /// also a different kind of answer — a hit in a dependency is something to understand, never
    /// something to go and change — so it runs **after** the project's own files and its results
    /// arrive underneath them.
    #[serde(default)]
    pub include_dependencies: bool,
    /// The FE-minted id correlating this search's `find-progress` events. Every batch and
    /// the terminal `done` carry it under `"id"`, so the FE ignores events from a
    /// superseded (older) scan.
    pub search_id: String,
}

/// The compiled matching policy for one search (derived once from the args, applied per
/// line — avoids re-lowercasing the needle for every line).
struct Matcher {
    /// The needle as searched: lowered when the match is case-insensitive.
    needle: String,
    /// Whether the haystack line must be lowered before searching.
    ci: bool,
    /// Whether to bound the match on word boundaries.
    whole_word: bool,
}

impl Matcher {
    fn new(args: &FindInFilesArgs) -> Self {
        // regex fallback is always case-insensitive; otherwise honour `case_sensitive`.
        let ci = args.regex || !args.case_sensitive;
        let needle = if ci { args.query.to_lowercase() } else { args.query.clone() };
        Self { needle, ci, whole_word: args.whole_word }
    }

    /// The byte offset of the first match of `needle` in `line`, or `None`. When
    /// case-insensitive, the search runs over a lowered copy of the line, but the returned
    /// offset is valid on the ORIGINAL line only when the lowering is length-preserving —
    /// which it is for the ASCII identifiers/keywords this search targets. For a
    /// non-ASCII line whose lowering changes length we fall back to reporting offset 0
    /// (the hit is still surfaced; only the column is approximate).
    fn find(&self, line: &str) -> Option<usize> {
        if self.needle.is_empty() {
            return None;
        }
        if self.ci {
            let lowered = line.to_lowercase();
            let pos = self.find_in(&lowered)?;
            // Column is byte-accurate only when lowering didn't shift byte lengths.
            if lowered.len() == line.len() { Some(pos) } else { Some(0) }
        } else {
            self.find_in(line)
        }
    }

    /// First match offset within an already case-normalised `hay`, honouring `whole_word`.
    fn find_in(&self, hay: &str) -> Option<usize> {
        let hb = hay.as_bytes();
        let nb = self.needle.as_bytes();
        if nb.is_empty() || hb.len() < nb.len() {
            return None;
        }
        let mut i = 0;
        while i + nb.len() <= hb.len() {
            if &hb[i..i + nb.len()] == nb {
                if !self.whole_word || word_bounded(hb, i, nb.len()) {
                    return Some(i);
                }
            }
            i += 1;
        }
        None
    }
}

/// Whether the match at `[start, start+len)` in `hay` is bounded by non-word chars on both
/// sides (word chars: `[A-Za-z0-9_]`).
fn word_bounded(hay: &[u8], start: usize, len: usize) -> bool {
    let before_ok = start == 0 || !is_word_byte(hay[start - 1]);
    let end = start + len;
    let after_ok = end >= hay.len() || !is_word_byte(hay[end]);
    before_ok && after_ok
}

/// Whether `b` is part of an ASCII identifier (`[A-Za-z0-9_]`).
fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Project-wide text search (progressive). Validates the args, then spawns a background
/// thread that walks `root`'s text files for `query` and streams matches back as
/// `find-progress` batches keyed by `search_id`, ending with one terminal `done` event.
/// Returns `Ok(())` immediately — the IPC dispatcher never blocks on the scan.
#[arbor_rpc::handler]
fn bennu_find_in_files(ctx: &BennuState, args: FindInFilesArgs) -> Result<(), String> {
    let sink = ctx.event_sink();
    // An empty query never matches: emit an immediate terminal `done` (uncapped) so the FE
    // spinner ends cleanly, without spinning up a thread for nothing.
    if args.query.is_empty() {
        emit_done(&sink, &args.search_id, false);
        return Ok(());
    }

    let matcher = Matcher::new(&args);
    let root = args.root.clone();
    let extra_roots = args.extra_roots.clone();
    let search_id = args.search_id.clone();
    let query = args.query.clone();
    // Resolved on the calling thread: it reads the project slot's jar list, which is cheap, and
    // doing it here keeps the scan thread from touching the index service at all.
    let jars = if args.include_dependencies {
        crate::index_service::IndexService::global().dep_jars_of(&args.root)
    } else {
        Vec::new()
    };

    // A plain background std thread (not a tokio worker): the scan does no reverse-channel
    // round-trips, so this mirrors `index_service`'s background build — it just walks the
    // tree and emits on the cloned sink.
    std::thread::Builder::new()
        .name(format!("bennu-find-{search_id}"))
        .spawn(move || {
            let mut batch = BatchSink::new(sink.clone(), search_id.clone());
            scan_dir(Path::new(&root), &matcher, &mut batch);
            // Workspace scope: scan the other projects too (same stream / cap / search_id).
            for r in &extra_roots {
                if batch.is_full() {
                    break;
                }
                scan_dir(Path::new(r), &matcher, &mut batch);
            }
            // Dependencies last, so the files you can actually edit arrive first.
            scan_jars(&jars, &matcher, &mut batch);
            let capped = batch.capped;
            batch.finish(); // flush any trailing hits before the terminal event
            if capped {
                eprintln!(
                    "bennu-be: bennu_find_in_files capped at {MAX_HITS} hits for {root} (query {query:?})"
                );
            }
            emit_done(&sink, &search_id, capped);
        })
        .map_err(|e| format!("spawn find thread: {e}"))?;

    Ok(())
}

/// Emit the terminal `{ id, done: true, capped }` event for `search_id`.
fn emit_done(sink: &Arc<dyn EventSink>, search_id: &str, capped: bool) {
    sink.emit(EVT_FIND_PROGRESS, json!({ "id": search_id, "done": true, "capped": capped }));
}

/// Accumulates hits and flushes them as `find-progress` batches, tracking the `MAX_HITS`
/// cap. The walk pushes hits and calls [`flush_file`](Self::flush_file) at each file
/// boundary; a batch is also auto-flushed once it reaches [`BATCH_SIZE`]. `finish` flushes
/// the trailing partial batch (the terminal `done` is emitted by the handler thread).
struct BatchSink {
    sink: Arc<dyn EventSink>,
    search_id: String,
    /// Pending hits not yet emitted.
    buf: Vec<FindHit>,
    /// Total hits seen so far (across all flushed batches + `buf`), gating the cap.
    total: usize,
    /// Whether the `MAX_HITS` cap stopped the walk.
    capped: bool,
}

impl BatchSink {
    fn new(sink: Arc<dyn EventSink>, search_id: String) -> Self {
        Self { sink, search_id, buf: Vec::new(), total: 0, capped: false }
    }

    /// Whether the cap has been reached (the walk should stop and mark `capped`).
    fn is_full(&mut self) -> bool {
        if self.total >= MAX_HITS {
            self.capped = true;
            return true;
        }
        false
    }

    /// Record one hit, auto-flushing when the batch reaches [`BATCH_SIZE`].
    fn push(&mut self, hit: FindHit) {
        self.buf.push(hit);
        self.total += 1;
        if self.buf.len() >= BATCH_SIZE {
            self.flush();
        }
    }

    /// Flush the pending batch at a file boundary (so a file's matches land together and
    /// promptly, even if it produced fewer than [`BATCH_SIZE`] hits).
    fn flush_file(&mut self) {
        self.flush();
    }

    /// Emit any pending hits as one batch (no-op when empty).
    fn flush(&mut self) {
        if self.buf.is_empty() {
            return;
        }
        let hits = std::mem::take(&mut self.buf);
        self.sink.emit(EVT_FIND_PROGRESS, json!({ "id": self.search_id, "hits": hits }));
    }

    /// Flush the trailing partial batch. Called once the walk is done, before the handler
    /// emits the terminal `done`.
    fn finish(&mut self) {
        self.flush();
    }
}

/// Recursively walk `dir`, scanning eligible files. Stops once `MAX_HITS` is reached
/// (marking `sink.capped`). Mirrors [`crate::todos::scan_dir`].
fn scan_dir(dir: &Path, matcher: &Matcher, sink: &mut BatchSink) {
    if sink.is_full() {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for entry in rd.flatten() {
        if sink.is_full() {
            return;
        }
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if SKIP_DIRS.contains(&name) {
                continue;
            }
            scan_dir(&path, matcher, sink);
        } else if is_scannable(&path) {
            scan_file(&path, matcher, sink);
        }
    }
}

/// Collect the absolute paths of every scannable text file under `root` — the same walk as
/// [`scan_dir`] (skips `SKIP_DIRS`, [`is_scannable`] extensions), WITHOUT reading them. Shared by
/// whole-project text passes that want the file set up front (e.g. the project mojibake scan) and
/// decode each file themselves (in the project's resolved encoding, in parallel). Paths use native
/// separators — normalise at the call site.
pub(crate) fn collect_text_paths(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_text_paths_into(root, &mut out);
    out
}

fn collect_text_paths_into(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for entry in rd.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if SKIP_DIRS.contains(&name) {
                continue;
            }
            collect_text_paths_into(&path, out);
        } else if is_scannable(&path) {
            out.push(path);
        }
    }
}

/// Cap on the text entries taken from any ONE dependency jar. A jar of ten thousand generated
/// schemas must not turn a search into a stall; a real artifact is far under this.
const MAX_JAR_ENTRIES: usize = 500;

/// Search the **text entries of dependency jars**, streaming hits into the same batch as the
/// project's own files.
///
/// Only the entries the project scan would have read ([`is_scannable_entry`]) — the bytecode is
/// most of a jar and searching it for text would produce nothing but noise from the constant
/// pool.
///
/// A hit's `file` is `<jar file name>!/<entry>` and not a path, because there is no path: the
/// thing matched lives inside a zip. That is the same identity `bennu_library_files` hands out,
/// so the frontend opens one exactly as it opens the other.
///
/// One archive open per jar rather than one per entry — [`read_jar_entries_matching`] walks the
/// central directory once and reads everything wanted from it, which is the difference between
/// a few hundred file opens and a few tens of thousands.
fn scan_jars(jars: &[String], matcher: &Matcher, sink: &mut BatchSink) {
    for jar in jars {
        if sink.is_full() {
            return;
        }
        let path = PathBuf::from(jar);
        let entries = bennu_classpath::prelude::read_jar_entries_matching(
            std::slice::from_ref(&path),
            |name| is_scannable_entry(name),
            MAX_JAR_ENTRIES,
        );
        for resource in entries {
            if sink.is_full() {
                sink.flush_file();
                return;
            }
            // Decoded by the rule every jar entry is read with — a Latin-1 `.properties` scanned
            // as lossy UTF-8 would match on a line whose preview shows `U+FFFD` where the accent
            // was, and would MISS a query containing that accent entirely.
            let text = crate::dep_classpath::jar_entry_text(&resource.bytes);
            scan_text(&resource.id, &text, matcher, sink);
        }
    }
}

/// Whether a jar entry is one of the text files a search should read. The same extension
/// allow-list as the project walk, minus the dotfile rule — a jar has no `.gitignore` worth
/// finding, and `META-INF/` is full of names that begin with nothing useful.
fn is_scannable_entry(name: &str) -> bool {
    let file = name.rsplit('/').next().unwrap_or(name);
    match file.rsplit_once('.') {
        Some((_, ext)) => SCAN_EXTS.contains(&ext) || ext == "dtd" || ext == "xsd" || ext == "tld",
        None => false,
    }
}

/// Whether `path` is a text file we scan: a known extension, or an extension-less dotfile
/// (`.gitignore`, `.editorconfig`, …).
fn is_scannable(path: &Path) -> bool {
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        return SCAN_EXTS.contains(&ext);
    }
    // No extension: scan only if it's a dotfile (a plain `Makefile`/binary is skipped).
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.starts_with('.') && n.len() > 1)
        .unwrap_or(false)
}

/// Scan one file line-by-line for the matcher's needle, pushing each match into `sink` and
/// flushing the batch at the end of the file (so its matches stream out together).
fn scan_file(path: &Path, matcher: &Matcher, sink: &mut BatchSink) {
    let Ok(source) = std::fs::read_to_string(path) else {
        return; // unreadable / non-UTF-8 — skip
    };
    scan_text(&path.to_string_lossy().replace('\\', "/"), &source, matcher, sink);
}

/// Scan already-read text line by line, pushing each match into `sink` and flushing the batch at
/// the end (so one file's matches stream out together).
///
/// `id` is what a hit says it was found in — a forward-slashed path for a file on disk, or
/// `<jar>!/<entry>` for something inside a dependency. This half is separate from [`scan_file`]
/// precisely because the second kind never was a file and cannot be read as one.
fn scan_text(id: &str, source: &str, matcher: &Matcher, sink: &mut BatchSink) {
    for (idx, line) in source.lines().enumerate() {
        if sink.is_full() {
            sink.flush_file();
            return;
        }
        if let Some(byte_col) = matcher.find(line) {
            // Column is 1-based CHAR count up to the byte offset (so a preview containing
            // multi-byte chars reports a caret-friendly column).
            let col = line[..byte_col].chars().count() + 1;
            let preview: String = line.trim().chars().take(MAX_PREVIEW_LEN).collect();
            sink.push(FindHit { file: id.to_string(), line: idx + 1, col, preview });
        }
    }
    // Flush this file's matches promptly (a partial batch < BATCH_SIZE otherwise waits for
    // the next file to fill it).
    sink.flush_file();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(query: &str, regex: bool, case_sensitive: bool, whole_word: bool) -> FindInFilesArgs {
        FindInFilesArgs {
            root: String::new(),
            extra_roots: Vec::new(),
            query: query.to_string(),
            regex,
            case_sensitive,
            whole_word,
            include_dependencies: false,
            search_id: "test".to_string(),
        }
    }

    #[test]
    fn a_jar_entry_is_scanned_when_it_is_the_kind_of_text_a_project_is_configured_by() {
        assert!(is_scannable_entry("struts-default.xml"));
        assert!(is_scannable_entry("META-INF/spring-beans-4.3.xsd"));
        assert!(is_scannable_entry("struts-2.5.dtd"));
        assert!(is_scannable_entry("META-INF/c.tld"));
        assert!(is_scannable_entry("messages.properties"));
        // The bytecode is most of a jar, and searching it for text finds constant-pool noise.
        assert!(!is_scannable_entry("org/apache/struts2/Dispatcher.class"));
        // A dotfile inside a jar is not the `.gitignore` case the project walk covers.
        assert!(!is_scannable_entry("META-INF/MANIFEST.MF"));
        assert!(!is_scannable_entry("META-INF/no-extension"));
    }

    #[test]
    fn substring_case_insensitive_by_default() {
        let m = Matcher::new(&args("todo", false, false, false));
        assert_eq!(m.find("  // TODO: refactor"), Some("  // ".len()));
        assert!(m.find("nothing here").is_none());
    }

    #[test]
    fn substring_case_sensitive_when_asked() {
        let m = Matcher::new(&args("TODO", false, true, false));
        assert!(m.find("todo lowercase").is_none());
        assert_eq!(m.find("a TODO b"), Some(2));
    }

    #[test]
    fn whole_word_bounds_the_match() {
        let m = Matcher::new(&args("foo", false, true, true));
        assert!(m.find("foobar").is_none());
        assert!(m.find("a_foo_b").is_none()); // underscore is a word char
        assert_eq!(m.find("call foo();"), Some("call ".len()));
    }

    #[test]
    fn whole_word_case_insensitive_combo() {
        let m = Matcher::new(&args("Order", false, false, true));
        assert!(m.find("Reorder()").is_none());
        assert_eq!(m.find("new order(x)"), Some("new ".len()));
    }

    #[test]
    fn regex_flag_is_case_insensitive_fallback() {
        // regex==true ignores case_sensitive and matches as a loose substring.
        let m = Matcher::new(&args("HANDLER", true, true, false));
        assert_eq!(m.find("register a handler"), Some("register a ".len()));
    }

    #[test]
    fn empty_query_never_matches() {
        let m = Matcher::new(&args("", false, false, false));
        assert!(m.find("anything").is_none());
    }

    #[test]
    fn dotfiles_and_known_exts_are_scannable() {
        assert!(is_scannable(Path::new("/p/Foo.java")));
        assert!(is_scannable(Path::new("/p/page.jspf")));
        assert!(is_scannable(Path::new("/p/.gitignore")));
        assert!(!is_scannable(Path::new("/p/image.png")));
        assert!(!is_scannable(Path::new("/p/Makefile")));
    }

    #[test]
    fn rust_project_files_are_scannable() {
        assert!(is_scannable(Path::new("/p/src/lib.rs")));
        assert!(is_scannable(Path::new("/p/Cargo.toml")));
        assert!(is_scannable(Path::new("/p/content/core/crystals/geode.ron")));
        assert!(is_scannable(Path::new("/p/content/core/examples/01-tre-righe.dig")));
        // A lockfile is text, and deliberately out: it is machine-written noise that
        // would swamp a search for a crate name with hundreds of hits.
        assert!(!is_scannable(Path::new("/p/Cargo.lock")));
    }

    // ── streaming walk (BatchSink flushing + capping) ────────────────────────────

    use std::sync::Mutex;

    /// A test [`EventSink`] that records every emitted `(topic, payload)`.
    #[derive(Default)]
    struct RecordingSink {
        events: Mutex<Vec<(String, serde_json::Value)>>,
    }

    impl EventSink for RecordingSink {
        fn emit(&self, topic: &str, payload: serde_json::Value) {
            self.events.lock().unwrap().push((topic.to_string(), payload));
        }
    }

    /// A unique temp dir for a fixture tree, cleaned up by the caller.
    fn temp_tree(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "bennu-find-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Count the total hits across all `{ hits: [...] }` batch events on the topic.
    fn total_hits(events: &[(String, serde_json::Value)]) -> usize {
        events
            .iter()
            .filter_map(|(_, p)| p.get("hits").and_then(|h| h.as_array()).map(|a| a.len()))
            .sum()
    }

    #[test]
    fn walk_streams_batches_then_one_terminal_done() {
        let dir = temp_tree("stream");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        // Two scannable files with matches, one skipped dir, one non-scannable file.
        std::fs::write(dir.join("A.java"), "class Foo {}\n// no match\nFoo again\n").unwrap();
        std::fs::write(dir.join("sub").join("B.xml"), "<Foo/>\n").unwrap();
        std::fs::create_dir_all(dir.join("target")).unwrap();
        std::fs::write(dir.join("target").join("C.java"), "Foo skipped\n").unwrap();
        std::fs::write(dir.join("image.png"), "Foo binary skipped\n").unwrap();

        let rec = Arc::new(RecordingSink::default());
        let sink: Arc<dyn EventSink> = rec.clone();
        let matcher = Matcher::new(&args("foo", false, false, false));

        let mut batch = BatchSink::new(sink, "s1".to_string());
        scan_dir(&dir, &matcher, &mut batch);
        let capped = batch.capped;
        batch.finish();
        emit_done(&batch.sink, "s1", capped);

        let events = rec.events.lock().unwrap();
        // Every event is on the find-progress topic and carries our id.
        for (topic, p) in events.iter() {
            assert_eq!(topic, EVT_FIND_PROGRESS);
            assert_eq!(p.get("id").and_then(|v| v.as_str()), Some("s1"));
        }
        // 3 hits total: A.java lines 1 & 3, B.xml line 1 (target/ + png skipped).
        assert_eq!(total_hits(&events), 3, "events: {events:?}");
        // Exactly one terminal done, uncapped, and it's LAST.
        let done: Vec<_> =
            events.iter().filter(|(_, p)| p.get("done").is_some()).collect();
        assert_eq!(done.len(), 1, "exactly one terminal done");
        assert_eq!(done[0].1.get("capped").and_then(|v| v.as_bool()), Some(false));
        assert!(events.last().unwrap().1.get("done").is_some(), "done is terminal");
        // At least one hit batch was emitted before the done (progressive).
        assert!(events.len() >= 2, "a batch + a done at minimum");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn walk_caps_and_reports_capped_in_terminal() {
        let dir = temp_tree("cap");
        // Build a file with well over MAX_HITS matching lines so the cap trips.
        let mut body = String::new();
        for _ in 0..(MAX_HITS + 50) {
            body.push_str("foo\n");
        }
        std::fs::write(dir.join("Big.java"), body).unwrap();

        let rec = Arc::new(RecordingSink::default());
        let sink: Arc<dyn EventSink> = rec.clone();
        let matcher = Matcher::new(&args("foo", false, false, false));

        let mut batch = BatchSink::new(sink, "s2".to_string());
        scan_dir(&dir, &matcher, &mut batch);
        let capped = batch.capped;
        batch.finish();
        emit_done(&batch.sink, "s2", capped);

        let events = rec.events.lock().unwrap();
        // Never emits more than the cap.
        assert_eq!(total_hits(&events), MAX_HITS, "hits are capped at MAX_HITS");
        let done = events.iter().rev().find(|(_, p)| p.get("done").is_some()).unwrap();
        assert_eq!(done.1.get("capped").and_then(|v| v.as_bool()), Some(true));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn batch_flushes_at_batch_size_boundary() {
        // Pushing 2*BATCH_SIZE + a few hits yields >= 2 auto-flushed batches before finish.
        let rec = Arc::new(RecordingSink::default());
        let sink: Arc<dyn EventSink> = rec.clone();
        let mut batch = BatchSink::new(sink, "s3".to_string());
        for i in 0..(BATCH_SIZE * 2 + 3) {
            batch.push(FindHit {
                file: "f".into(),
                line: i + 1,
                col: 1,
                preview: "foo".into(),
            });
        }
        // Before finish: exactly two full batches auto-flushed (the remainder is pending).
        assert_eq!(rec.events.lock().unwrap().len(), 2, "two auto-flushed batches");
        batch.finish();
        // finish flushes the trailing partial batch → 3 batch events, all with our id.
        let events = rec.events.lock().unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(total_hits(&events), BATCH_SIZE * 2 + 3);
    }
}
