//! Turning a raw recording into a [`Profile`]: spawn the reader (`jfr`, the Java helper) or read
//! the file, stream it through the aggregator. Blocking — callers run it on their own thread.

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use bennu_profile::prelude::*;
use serde_json::Value;

use super::store::now_ms;

/// Events folded before the reader is cut off. A 30-minute recording of a busy server is a few
/// million; past this the tables stop being more informative and memory/time are still bounded.
pub const MAX_EVENTS: u64 = 8_000_000;
/// A `.cpuprofile` / samply file larger than this on disk (compressed or not) is refused unread; the
/// decoded size is capped separately by `MAX_DECODED_BYTES`.
const MAX_JSON_BYTES: u64 = 600 * 1024 * 1024;
/// JFR `profile` settings sample Java threads every 10 ms.
const JFR_PERIOD_MS: f64 = 10.0;
/// Frames kept per stack when asking `jfr print`.
const STACK_DEPTH: &str = "96";

fn command(program: &Path) -> Command {
    let mut c = Command::new(program);
    c.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use arbor_process_ext::prelude::NoWindowExt;
        c.no_window();
    }
    c
}

/// Drain stderr on a thread (a full pipe would block the child) and keep its tail.
fn drain_stderr(child: &mut Child) -> std::thread::JoinHandle<String> {
    let err = child.stderr.take();
    std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(mut e) = err {
            let _ = e.read_to_string(&mut s);
        }
        let keep = s.len().saturating_sub(2000);
        s[keep..].to_string()
    })
}

fn empty_profile(id: &str, lang: Lang, label: &str, ms_per_unit: f64) -> Profile {
    Profile {
        id: id.to_string(),
        lang,
        label: label.to_string(),
        created_ms: now_ms(),
        duration_ms: None,
        ms_per_unit,
        tree: CallTree::default(),
        methods: vec![],
        alloc_sites: vec![],
        alloc_tree: Default::default(),
        wait_tree: Default::default(),
        waits: vec![],
        alloc_types: vec![],
        locks: vec![],
        gc: vec![],
        threads: vec![],
        notes: vec![],
    }
}

/// Parse a `.jfr` file with the route the detector chose.
pub fn parse_jfr(how: &ParseHow, jfr_file: &Path, scratch: &Path, id: &str, label: &str, progress: &dyn Fn(u64)) -> Result<Profile, String> {
    let mut agg = JfrAggregator::default();
    let mut note_cut = false;
    match how {
        ParseHow::JfrCli { jfr } => {
            let mut child = command(jfr)
                .args(["print", "--json", "--stack-depth", STACK_DEPTH, "--events", WANTED_EVENTS])
                .arg(jfr_file)
                .spawn()
                .map_err(|e| format!("cannot run {}: {e}", jfr.display()))?;
            let err = drain_stderr(&mut child);
            let mut out = child.stdout.take().ok_or("jfr produced no output stream")?;
            let mut splitter = EventSplitter::default();
            let mut buf = vec![0u8; 256 * 1024];
            'read: loop {
                let n = out.read(&mut buf).map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                let mut cut = false;
                splitter.push(&buf[..n], &mut |obj| {
                    if cut {
                        return;
                    }
                    if let Ok(v) = serde_json::from_slice::<Value>(obj) {
                        if let Some(ev) = parse_print_event(&v) {
                            agg.feed(ev);
                        }
                    }
                    if agg.events % 20_000 == 0 {
                        progress(agg.events);
                    }
                    if agg.events >= MAX_EVENTS {
                        cut = true;
                    }
                });
                if cut {
                    note_cut = true;
                    let _ = child.kill();
                    break 'read;
                }
            }
            let status = child.wait().map_err(|e| e.to_string())?;
            let stderr = err.join().unwrap_or_default();
            if !note_cut && !status.success() && agg.events == 0 {
                return Err(format!("`jfr print` failed: {}", stderr.trim()));
            }
        }
        ParseHow::HelperSource { java } => {
            let src = write_helper(scratch)?;
            let mut cmd = command(java);
            cmd.arg(&src).arg(jfr_file);
            note_cut = run_helper(cmd, &mut agg, progress)?;
        }
        ParseHow::HelperCompiled { java, javac } => {
            let src = write_helper(scratch)?;
            let out = command(javac)
                .arg("-d")
                .arg(scratch)
                .arg(&src)
                .output()
                .map_err(|e| format!("cannot run javac: {e}"))?;
            if !out.status.success() {
                return Err(format!("the JFR reader failed to compile: {}", String::from_utf8_lossy(&out.stderr).trim()));
            }
            let mut cmd = command(java);
            cmd.arg("-cp").arg(scratch).arg(HELPER_CLASS).arg(jfr_file);
            note_cut = run_helper(cmd, &mut agg, progress)?;
        }
    }
    let mut p = empty_profile(id, Lang::Java, label, JFR_PERIOD_MS);
    if note_cut {
        p.notes.push(format!("The recording was cut after {MAX_EVENTS} events; the rest is not included."));
    }
    agg.finish_into(&mut p);
    Ok(p)
}

fn write_helper(dir: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let p = dir.join(HELPER_FILE);
    std::fs::write(&p, HELPER_SOURCE).map_err(|e| e.to_string())?;
    Ok(p)
}

fn run_helper(mut cmd: Command, agg: &mut JfrAggregator, progress: &dyn Fn(u64)) -> Result<bool, String> {
    let mut child = cmd.spawn().map_err(|e| format!("cannot run java: {e}"))?;
    let err = drain_stderr(&mut child);
    let out = child.stdout.take().ok_or("the reader produced no output stream")?;
    let mut reader = BufReader::with_capacity(256 * 1024, out);
    let mut line = String::new();
    let mut cut = false;
    loop {
        line.clear();
        if reader.read_line(&mut line).map_err(|e| e.to_string())? == 0 {
            break;
        }
        if let Some(ev) = parse_helper_line(line.trim_end()) {
            agg.feed(ev);
            if agg.events % 20_000 == 0 {
                progress(agg.events);
            }
            if agg.events >= MAX_EVENTS {
                cut = true;
                let _ = child.kill();
                break;
            }
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let stderr = err.join().unwrap_or_default();
    if !cut && !status.success() && agg.events == 0 {
        return Err(format!("the JFR reader failed: {}", stderr.trim()));
    }
    Ok(cut)
}

fn read_json(path: &Path) -> Result<Value, String> {
    let size = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?.len();
    if size > MAX_JSON_BYTES {
        return Err(format!("{} is {} MB; profiles over {} MB are not loaded", path.display(), size >> 20, MAX_JSON_BYTES >> 20));
    }
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    // Plain or gzip (samply writes `.json.gz`), told apart by the magic bytes, decoded size capped.
    let bytes = read_maybe_gzip(file, MAX_DECODED_BYTES).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("{} is not valid JSON: {e}", path.display()))
}

/// Merge every `.cpuprofile` given (one per Node process).
pub fn parse_cpuprofiles(files: &[PathBuf], id: &str, label: &str) -> Result<Profile, String> {
    let mut b = TreeBuilder::default();
    let (mut ms_sum, mut counted, mut duration) = (0.0, 0u32, 0.0f64);
    let mut notes = Vec::new();
    for f in files {
        let layer = f.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "node".into());
        match read_json(f).and_then(|v| add_cpuprofile(&mut b, &layer, &v)) {
            Ok(r) => {
                if r.ms_per_sample > 0.0 {
                    ms_sum += r.ms_per_sample;
                    counted += 1;
                }
                duration = duration.max(r.duration_ms);
            }
            Err(e) => notes.push(format!("{}: {e}", f.display())),
        }
    }
    if b.total() == 0 {
        return Err(if notes.is_empty() { "the profile holds no samples".into() } else { notes.join("; ") });
    }
    let mut p = empty_profile(id, Lang::Node, label, if counted > 0 { ms_sum / counted as f64 } else { 1.0 });
    p.duration_ms = Some(duration);
    p.notes = notes;
    p.notes.push("Frames are reported at the file V8 ran; source maps are not applied.".into());
    finish_tree(&mut p, b);
    Ok(p)
}

/// A `samply` profile (Firefox-profiler JSON).
pub fn parse_samply(file: &Path, id: &str, label: &str) -> Result<Profile, String> {
    let v = read_json(file)?;
    let mut b = TreeBuilder::default();
    let r = add_firefox_profile(&mut b, &v)?;
    let mut p = empty_profile(id, Lang::Rust, label, r.ms_per_sample);
    if r.unresolved_fraction > 0.25 {
        p.notes.push(format!(
            "{:.0}% of the samples have no function name (raw addresses). Build with debug info \
             (`[profile.release] debug = true`) and keep the binary in place so samply can resolve symbols.",
            r.unresolved_fraction * 100.0
        ));
    }
    p.notes.push("Imported from samply's profile format; verify names and lines against your binary.".into());
    finish_tree(&mut p, b);
    Ok(p)
}

fn finish_tree(p: &mut Profile, b: TreeBuilder) {
    if b.capped() {
        p.notes.push("The call tree hit its size limit; the deepest frames of rare stacks were folded into their callers.".into());
    }
    let tree = b.finish();
    p.methods = tree.method_stats(5_000);
    p.threads = tree.thread_stats();
    p.tree = tree;
}
