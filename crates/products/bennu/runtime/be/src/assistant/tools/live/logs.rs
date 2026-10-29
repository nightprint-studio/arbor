//! Reading logs without paying for them line by line: filters (time, level, pattern, context), a
//! summary, FACETS (exceptions, queries, requests, warnings), one record in full, waiting for a line —
//! over a managed process, the user's own Run console, or a log file on disk.
//!
//! All of it is read-only and goes through the same code: the source is turned into lines, the lines
//! into records (a log line with its stack trace), and `bennu-live` does the rest. This file is the
//! model-facing layer: specs, argument parsing, and where each source's lines come from.
//!
//! Every answer states what it left out and how to ask for more, so a cut is never mistaken for the
//! whole log.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use arbor_logscan::prelude::Level;
use bennu_live::prelude::{
    build_records, digest, iso_time, now_ms, parse_level, parse_where, plan_file_read, render_records, split_lines, summarize_records, Facet, FacetQuery,
    LogLine, LogQuery, RawLine, ReadOptions, Record, Stream, TimeWindow, FACET_DEFAULT_LIMIT, FACET_MAX_LIMIT, MAX_CONTEXT, MAX_FRAMES, MAX_TAIL,
};
use regex::Regex;
use serde_json::{json, Map, Value};

use super::{clip, duration_text, proc_line, spec, state_word, str_arg, ToolHost};
use crate::log::ClassMap;
use bennu_assistant::prelude::ToolSpec;

pub(super) const NAMES: [&str; 5] = ["console_logs", "tail_file", "wait_log", "log_facets", "log_record"];

/// Distinct errors a summary lists.
const SUMMARY_TOP: usize = 5;
const FILE_KB_DEFAULT: usize = 512;
const FILE_KB_MAX: usize = 4096;
/// How much of a file `log_record` reads from the record's offset.
const RECORD_FILE_BYTES: u64 = 256 * 1024;

// ── Specs ────────────────────────────────────────────────────────────────────────────────────────

/// The filters every log reader takes, merged into a tool's own properties (the `properties` object).
pub(super) fn filter_props(own: Value) -> Value {
    let mut props: Map<String, Value> = own.as_object().cloned().unwrap_or_default();
    let shared = json!({
        "tail":{"type":"integer","description":"How many of the newest matching records. Default 80, at most 400. A record is a log line with its stack trace."},
        "pattern":{"type":"string","description":"Regex a record must match (anywhere in it, stack trace included). Add (?i) for case-insensitive."},
        "level":{"type":"string","enum":["ERROR","WARN","INFO","DEBUG","TRACE"],"description":"At least this severity (WARN gives WARN and ERROR). Read from logback/log4j/Spring, env_logger/tracing, logfmt and JSON logs."},
        "after":{"type":"string","description":"Only records at or after this time: ISO-8601 (no offset = UTC) or epoch milliseconds."},
        "before":{"type":"string","description":"Only records at or before this time."},
        "last":{"type":"string","description":"Only the last span of time: 30s, 5m, 2h, 1d."},
        "context":{"type":"integer","description":"Records shown before and after each match (at most 10); overlapping blocks merge, gaps are marked `--`."},
        "frames":{"type":"integer","description":"Stack-trace lines shown per record. Default 12, at most 200; the last `Caused by` is always kept."},
        "summary":{"type":"boolean","description":"Instead of lines: counts per level, the first distinct errors with their first time and count, the last error. Start here."}
    });
    props.extend(shared.as_object().cloned().unwrap_or_default());
    Value::Object(props)
}

/// A whole input schema: the tool's own properties plus the shared filters.
pub(super) fn schema_with_filters(own: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":filter_props(own),"required":required})
}

pub(super) fn specs() -> Vec<ToolSpec> {
    let source = json!({"type":"string","description":"What to read: a process id or name (`p1`, `api`), `console` (the user's latest Run console run) or `console:r3`, or `file:<path>` for a log file."});
    vec![
        spec(
            "console_logs",
            "Read what the USER's own Run console printed (the Run button, not your processes): lists the recent runs (id, configuration, start, status) and reads one — the newest unless `run` names another (`r3`, the number, or its name). Same filters as run_logs. Read-only; the output is kept for a few runs per project.",
            schema_with_filters(json!({"run":{"type":"string","description":"`r3`, `3`, or the run's name. Default: the newest."},"stream":{"type":"string","enum":["stdout","stderr"]}}), &[]),
        ),
        spec(
            "tail_file",
            "Read a log FILE on disk (logs/app.log, a rolling file): the end of it, whole records only, with the run_logs filters. Returns `next_offset`: pass it as `since_offset` to get only what was appended since; a file that shrank (rotated, truncated) is read from its end again and says so. Encoding is detected. Goes through the file-access rules like any read.",
            schema_with_filters(json!({
                "path":{"type":"string","description":"The log file, relative to the project or absolute."},
                "since_offset":{"type":"integer","description":"The `next_offset` of the previous call."},
                "max_kb":{"type":"integer","description":"The most bytes read from the end of the file, in KB. Default 512, at most 4096."}
            }), &["path"]),
        ),
        spec(
            "wait_log",
            "Wait until a NEW line matching `pattern` appears in a managed process's output, then return it with 3 records of context before and after. Use it instead of polling run_logs. Returns early if the process exits, with its last output; gives up after `timeout_s`. `since` takes a `next_since` (0 = also look at what is already buffered).",
            json!({"type":"object","properties":{
                "id":{"type":"string","description":"A process id or name."},
                "pattern":{"type":"string","description":"Regex the line must match."},
                "timeout_s":{"type":"integer","description":"Default 30, at most 300."},
                "since":{"type":"integer","description":"Only lines after this sequence number. Default: only lines written after this call starts."}
            },"required":["id","pattern"]}),
        ),
        spec(
            "log_facets",
            "A DIGEST of a log instead of its lines — far cheaper to read. Without `facet`: counts per level plus every facet that has something. EXCEPTIONS grouped by class, root cause and the first frame in the project (as file:line), with count, first/last time and a sample record id; QUERIES (SQL from Hibernate, p6spy, JdbcTemplate, MyBatis, sqlx) with literals replaced, slowest first when the log has times; REQUESTS (access logs, tower/axum traces) grouped by endpoint with statuses, failing endpoints first; WARNINGS grouped by message template. Then open one thing with log_record. Never read raw logs when a facet answers.",
            with_filters_time(json!({
                "source":source.clone(),
                "facet":{"type":"string","enum":["exceptions","queries","requests","warnings"],"description":"Only this facet, in more depth."},
                "limit":{"type":"integer","description":"Groups per facet. Default 8, at most 30."},
                "where":{"type":"string","description":"Filter the occurrences of the chosen facet (needs `facet`): `field op value`, joined with `and`. Fields — exceptions: class, message, frame; queries: sql, duration, logger; requests: method, url, status, duration; warnings: message, logger; any: contains. Ops: contains = != > >= < <= ~ (regex). Examples: `status >= 500`, `duration > 200ms and sql contains orders`, `class contains Order`."},
                "since":{"type":"integer","description":"For a process or console: only lines after this sequence number."},
                "max_kb":{"type":"integer","description":"For a file: the most bytes read from its end, in KB. Default 512, at most 4096."}
            })),
        ),
        spec(
            "log_record",
            "One record in full — the log line with its whole stack trace and `Caused by` chain (capped at 200 lines) — by the id a digest or run_logs printed (`#1842`). `context` adds that many records before and after. Use it to open a sample from log_facets; do not use it to read a log.",
            json!({"type":"object","properties":{
                "source":source,
                "id":{"type":"integer","description":"The record id (the number after `#`)."},
                "context":{"type":"integer","description":"Records before and after (at most 5; for a file, after only)."},
                "frames":{"type":"integer","description":"Lines of the record shown. Default 200."}
            },"required":["source","id"]}),
        ),
    ]
}

/// The time filters only (a digest has no use for tail / pattern).
fn with_filters_time(own: Value) -> Value {
    let all = filter_props(json!({}));
    let mut props: Map<String, Value> = own.as_object().cloned().unwrap_or_default();
    for key in ["after", "before", "last"] {
        if let Some(v) = all.get(key) {
            props.insert(key.into(), v.clone());
        }
    }
    json!({"type":"object","properties":Value::Object(props),"required":["source"]})
}

// ── Arguments ────────────────────────────────────────────────────────────────────────────────────

/// What a read asks for, parsed from the tool's input.
#[derive(Debug)]
pub(super) struct Filters {
    pub pattern: Option<Regex>,
    pub level: Option<Level>,
    pub after: Option<String>,
    pub before: Option<String>,
    pub last: Option<String>,
    pub tail: usize,
    pub context: usize,
    pub frames: usize,
    pub summary: bool,
}

fn opt_str(input: &Value, key: &str) -> Option<String> {
    input.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

fn opt_usize(input: &Value, key: &str) -> Option<usize> {
    input.get(key).and_then(Value::as_u64).map(|n| n as usize)
}

impl Filters {
    pub(super) fn from_input(input: &Value) -> Result<Self, String> {
        let pattern = match opt_str(input, "pattern") {
            Some(p) => Some(Regex::new(&p).map_err(|e| format!("`pattern` is not a valid regex: {e}"))?),
            None => None,
        };
        let level = opt_str(input, "level").map(|l| parse_level(&l)).transpose()?;
        Ok(Filters {
            pattern,
            level,
            after: opt_str(input, "after"),
            before: opt_str(input, "before"),
            last: opt_str(input, "last"),
            tail: opt_usize(input, "tail").unwrap_or(80).clamp(1, MAX_TAIL),
            context: opt_usize(input, "context").unwrap_or(0).min(MAX_CONTEXT),
            frames: opt_usize(input, "frames").unwrap_or(12).clamp(0, MAX_FRAMES),
            summary: input.get("summary").and_then(Value::as_bool).unwrap_or(false),
        })
    }

    fn window(&self, reference_ms: u64) -> Result<TimeWindow, String> {
        TimeWindow::from_args(self.after.as_deref(), self.before.as_deref(), self.last.as_deref(), reference_ms)
    }

    fn options(&self, window: TimeWindow) -> ReadOptions {
        ReadOptions { tail: self.tail, pattern: self.pattern.clone(), level: self.level, window, context: self.context, frames: self.frames, fold: true }
    }
}

/// Where a log comes from.
#[derive(Debug, PartialEq)]
pub(super) enum Src {
    Process(String),
    Console(Option<String>),
    File(String),
}

pub(super) fn parse_source(text: &str) -> Result<Src, String> {
    let t = text.trim();
    if t.is_empty() {
        return Err("`source` is required: a process id or name, `console`, `console:r3` or `file:<path>`".into());
    }
    if t.eq_ignore_ascii_case("console") {
        return Ok(Src::Console(None));
    }
    if let Some(run) = t.strip_prefix("console:") {
        return Ok(Src::Console(Some(run.trim().to_string())));
    }
    if let Some(path) = t.strip_prefix("file:") {
        return Ok(Src::File(path.trim().to_string()));
    }
    Ok(Src::Process(t.to_string()))
}

/// `path` under `root`, as a `/` path relative to it; unchanged when it is elsewhere.
pub(super) fn project_relative(root: &str, path: &str) -> String {
    let norm = |s: &str| s.replace('\\', "/");
    let (root, path) = (norm(root), norm(path));
    match path.strip_prefix(root.trim_end_matches('/')) {
        Some(rest) if rest.starts_with('/') => rest.trim_start_matches('/').to_string(),
        _ => path,
    }
}

// ── Loading ──────────────────────────────────────────────────────────────────────────────────────

/// A source read into lines, with what a reader needs to introduce and bound them.
pub(super) struct Loaded {
    pub title: String,
    pub lines: Vec<RawLine>,
    /// "next_since=…" / "next_offset=…": how to ask for only what comes after.
    pub cursor: String,
    pub notes: Vec<String>,
    /// When the source began (a `[+3 s]` prefix is relative to it); `None` for a file.
    pub started_ms: Option<u64>,
    /// The "now" of `last`: the clock for a live source, `None` for a file (its own newest line).
    pub clock: Option<u64>,
    pub classes: ClassMap,
}

fn raw_lines(lines: Vec<LogLine>, stream: Option<Stream>) -> Vec<RawLine> {
    lines.into_iter().filter(|l| stream.is_none_or(|s| l.stream == s)).map(|l| RawLine { seq: l.seq, ts_ms: Some(l.ts_ms), stream: l.stream, text: l.text }).collect()
}

fn stream_arg(input: &Value) -> Option<Stream> {
    match input.get("stream").and_then(Value::as_str) {
        Some("stdout") => Some(Stream::Stdout),
        Some("stderr") => Some(Stream::Stderr),
        _ => None,
    }
}

impl ToolHost {
    fn log_classes(&self) -> ClassMap {
        crate::log::class_map(&self.session.root)
    }

    fn load_process(&self, id: &str, since: Option<u64>, stream: Option<Stream>) -> Result<Loaded, String> {
        let (info, lines) = self.live().manager.logs(id, &LogQuery { since, ..Default::default() })?;
        let next = lines.last().map(|l| l.seq).or(since).unwrap_or(0);
        let mut notes = Vec::new();
        if info.log_dropped > 0 {
            notes.push(format!("{} older line(s) were dropped by the process's ring buffer and cannot be read any more.", info.log_dropped));
        }
        Ok(Loaded {
            title: proc_line(&info, self.me()),
            lines: raw_lines(lines, stream),
            cursor: format!("next_since={next}"),
            notes,
            started_ms: Some(info.started_ms),
            clock: Some(now_ms()),
            classes: self.log_classes(),
        })
    }

    fn load_console(&self, run: Option<&str>, since: Option<u64>, stream: Option<Stream>) -> Result<Loaded, String> {
        let (info, lines) = crate::run_console::read(&self.session.root, run, &LogQuery { since, ..Default::default() })?;
        let next = lines.last().map(|l| l.seq).or(since).unwrap_or(0);
        let mut notes = Vec::new();
        if info.dropped > 0 {
            notes.push(format!("{} older line(s) of this run were dropped by the console buffer.", info.dropped));
        }
        Ok(Loaded {
            title: format!("Run console {} `{}` — {} — {} line(s) kept", info.short, info.label, info.status(), info.lines),
            lines: raw_lines(lines, stream),
            cursor: format!("next_since={next}"),
            notes,
            started_ms: Some(info.started_ms),
            clock: Some(now_ms()),
            classes: self.log_classes(),
        })
    }

    /// The end of a file (or what was appended since `since_offset`), whole lines only.
    fn load_file(&self, raw_path: &str, since_offset: Option<u64>, max_kb: usize, from_record: Option<u64>) -> Result<Loaded, String> {
        let path = self.resolve(raw_path);
        let meta = std::fs::metadata(&path).map_err(|e| format!("cannot read `{}`: {e}", path.display()))?;
        if !meta.is_file() {
            return Err(format!("`{}` is not a file", path.display()));
        }
        let size = meta.len();
        let max_bytes = if from_record.is_some() { RECORD_FILE_BYTES } else { (max_kb.clamp(16, FILE_KB_MAX) as u64) * 1024 };
        let plan = plan_file_read(size, from_record.or(since_offset), max_bytes);
        let bytes = read_range(&path, plan.start, size - plan.start)?;
        let chunk = split_lines(&bytes, plan.start, plan.mid_line);
        let encoding = arbor_fs::prelude::encoding::detect(&bytes);
        let lines: Vec<RawLine> = chunk
            .lines
            .iter()
            .map(|l| RawLine { seq: l.offset, ts_ms: None, stream: Stream::Stdout, text: arbor_fs::prelude::encoding::decode_with(l.bytes, encoding) })
            .collect();
        let mut notes = Vec::new();
        if plan.rotated {
            notes.push(format!("the file is smaller than `since_offset` ({size} bytes): it was rotated or truncated, so it is read from its end."));
        }
        if plan.skipped > 0 {
            notes.push(format!("{} byte(s) appended since `since_offset` were not read (more than max_kb): raise `max_kb` or narrow the question.", plan.skipped));
        } else if plan.start > 0 && since_offset.is_none() && from_record.is_none() {
            notes.push(format!("only the last {} KB of a {} KB file were read; earlier content is not covered (raise `max_kb`, up to {FILE_KB_MAX}).", (size - plan.start) / 1024, size / 1024));
        }
        Ok(Loaded {
            title: format!("{} — {} KB", path.display(), size / 1024),
            lines,
            cursor: format!("next_offset={}", chunk.next_offset),
            notes,
            started_ms: None,
            clock: None,
            classes: crate::log::class_map(&self.root_of(&path)),
        })
    }

    fn load(&self, src: &Src, input: &Value) -> Result<Loaded, String> {
        let since = input.get("since").and_then(Value::as_u64);
        match src {
            Src::Process(id) => self.load_process(id, since, stream_arg(input)),
            Src::Console(run) => self.load_console(run.as_deref(), since, stream_arg(input)),
            Src::File(path) => self.load_file(path, input.get("since_offset").and_then(Value::as_u64), opt_usize(input, "max_kb").unwrap_or(FILE_KB_DEFAULT), None),
        }
    }

    /// The paths a log tool touches, for the access policy: a file is a Read of that file, a console run
    /// is read at project scope, a process is already ours.
    pub(super) fn log_touched(&self, tool: &str, input: &Value) -> Vec<(PathBuf, bennu_assistant::prelude::Op)> {
        use bennu_assistant::prelude::Op;
        let file = match tool {
            "tail_file" => input.get("path").and_then(Value::as_str).map(str::to_string),
            "log_facets" | "log_record" => match input.get("source").and_then(Value::as_str).map(parse_source) {
                Some(Ok(Src::File(p))) => Some(p),
                Some(Ok(Src::Console(_))) => return vec![(self.project().to_path_buf(), Op::List)],
                _ => None,
            },
            "console_logs" => return vec![(self.project().to_path_buf(), Op::List)],
            _ => None,
        };
        file.map(|p| vec![(self.resolve(&p), Op::Read)]).unwrap_or_default()
    }

    // ── The tools ────────────────────────────────────────────────────────────────────────────────

    pub(super) fn run_log_tool(&self, tool: &str, input: &Value) -> Result<String, String> {
        match tool {
            "console_logs" => self.console_logs(input),
            "tail_file" => self.tail_file(input),
            "wait_log" => self.wait_log(input),
            "log_facets" => self.log_facets(input),
            "log_record" => self.log_record(input),
            other => Err(format!("`{other}` is not a log tool")),
        }
    }

    /// `run_logs`: a managed process's output, filtered.
    pub(super) fn live_logs(&self, input: &Value) -> Result<String, String> {
        let filters = Filters::from_input(input)?;
        let loaded = self.load_process(str_arg(input, "id")?, input.get("since").and_then(Value::as_u64), stream_arg(input))?;
        self.report(loaded, &filters)
    }

    fn console_logs(&self, input: &Value) -> Result<String, String> {
        let filters = Filters::from_input(input)?;
        let runs = crate::run_console::list(&self.session.root);
        let mut listing = String::new();
        for r in &runs {
            listing.push_str(&format!("  {} `{}` — started {} — {}, {} line(s)\n", r.short, r.label, iso_time(r.started_ms), r.status(), r.lines));
        }
        let loaded = self.load_console(opt_str(input, "run").as_deref(), input.get("since").and_then(Value::as_u64), stream_arg(input))?;
        Ok(format!("recent runs of the Run console (newest first):\n{listing}\n{}", self.report(loaded, &filters)?))
    }

    fn tail_file(&self, input: &Value) -> Result<String, String> {
        let filters = Filters::from_input(input)?;
        let loaded = self.load_file(str_arg(input, "path")?, input.get("since_offset").and_then(Value::as_u64), opt_usize(input, "max_kb").unwrap_or(FILE_KB_DEFAULT), None)?;
        self.report(loaded, &filters)
    }

    /// Lines → records → the filtered view or the summary, introduced by the source and closed by what was cut.
    fn report(&self, loaded: Loaded, f: &Filters) -> Result<String, String> {
        let rules = loaded.classes.rules();
        let records = build_records(&rules, &loaded.lines);
        let reference = loaded.clock.or_else(|| records.iter().filter_map(|r| r.ts_ms).max()).unwrap_or_else(now_ms);
        let window = f.window(reference)?;
        let mut out = format!("{}\n", loaded.title);
        let mut cut = loaded.notes.clone();
        if f.summary {
            out.push_str(&format!("{} line(s) read, {}.\n", loaded.lines.len(), loaded.cursor));
            out.push_str(&summarize_records(&records, &window, SUMMARY_TOP));
        } else {
            let started = loaded.started_ms;
            let prefix = move |r: &Record| record_prefix(r, started);
            let rendered = render_records(&records, &f.options(window), &prefix);
            out.push_str(&format!("{} of {} matching record(s) shown ({} line(s) read); {}\n", rendered.shown, rendered.matched, loaded.lines.len(), loaded.cursor));
            if rendered.text.is_empty() {
                out.push_str("no matching records.\n");
                if !window.is_open() {
                    if let Some(newest) = records.iter().filter_map(|r| r.ts_ms).max() {
                        out.push_str(&format!("(the newest record is from {}; the window is {})\n", iso_time(newest), window.describe()));
                    }
                }
            } else {
                out.push_str(&rendered.text);
            }
            cut.extend(rendered.cut);
        }
        Ok(clip(append_cut(out, &cut)))
    }

    fn wait_log(&self, input: &Value) -> Result<String, String> {
        let live = self.live();
        let id = str_arg(input, "id")?;
        let pattern = str_arg(input, "pattern")?;
        let re = Regex::new(pattern).map_err(|e| format!("`pattern` is not a valid regex: {e}"))?;
        let timeout = Duration::from_secs(input.get("timeout_s").and_then(Value::as_u64).unwrap_or(30).clamp(1, 300));
        let cursor = match input.get("since").and_then(Value::as_u64) {
            Some(s) => s,
            // Only what is written after this call begins.
            None => live.manager.logs(id, &LogQuery { tail: Some(1), ..Default::default() })?.1.last().map(|l| l.seq).unwrap_or(0),
        };
        let begun = Instant::now();
        loop {
            if self.session.cancelled() {
                return Err("cancelled".into());
            }
            let (info, found) = live.manager.logs(id, &LogQuery { since: Some(cursor), pattern: Some(re.clone()), ..Default::default() })?;
            if let Some(hit) = found.first() {
                // A moment for the lines that follow (the rest of a stack trace) to be written.
                std::thread::sleep(Duration::from_millis(150));
                return self.matched_with_context(id, hit, info.started_ms, begun.elapsed());
            }
            if info.phase.is_finished() {
                let tail = live.manager.logs(id, &LogQuery { tail: Some(10), ..Default::default() })?.1;
                return Ok(format!("`{}` {} without a line matching /{pattern}/.\nLast output:\n{}", info.name, state_word(&info), plain_lines(&tail, info.started_ms)));
            }
            if begun.elapsed() >= timeout {
                let tail = live.manager.logs(id, &LogQuery { tail: Some(5), ..Default::default() })?.1;
                let next = tail.last().map(|l| l.seq).unwrap_or(cursor);
                return Ok(format!(
                    "no new line matching /{pattern}/ in {} s; `{}` is still {}.\nLast output:\n{}\nnext_since={next}. Wait again, or read what it printed with run_logs / log_facets.",
                    timeout.as_secs(),
                    info.name,
                    state_word(&info),
                    plain_lines(&tail, info.started_ms)
                ));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn matched_with_context(&self, id: &str, hit: &LogLine, started_ms: u64, waited: Duration) -> Result<String, String> {
        let (info, around) = self.live().manager.logs(id, &LogQuery { since: Some(hit.seq.saturating_sub(4)), ..Default::default() })?;
        let shown: Vec<&LogLine> = around.iter().filter(|l| l.seq + 3 >= hit.seq && l.seq <= hit.seq + 3).collect();
        let mut out = format!("`{}` printed a matching line after {}:\n", info.name, duration_text(waited.as_millis() as u64));
        for l in &shown {
            out.push_str(&format!("{} [+{} #{}] {}\n", if l.seq == hit.seq { ">" } else { " " }, duration_text(l.ts_ms.saturating_sub(started_ms)), l.seq, l.text));
        }
        out.push_str(&format!("next_since={}", shown.last().map(|l| l.seq).unwrap_or(hit.seq)));
        Ok(clip(out))
    }

    fn log_facets(&self, input: &Value) -> Result<String, String> {
        let src = parse_source(str_arg(input, "source")?)?;
        let loaded = self.load(&src, input)?;
        let records = build_records(&loaded.classes.rules(), &loaded.lines);
        let reference = loaded.clock.or_else(|| records.iter().filter_map(|r| r.ts_ms).max()).unwrap_or_else(now_ms);
        let window = Filters::from_input(input)?.window(reference)?;
        let facet = opt_str(input, "facet").map(|f| Facet::parse(&f)).transpose()?;
        let conds = match opt_str(input, "where") {
            Some(w) => parse_where(&w)?,
            None => Vec::new(),
        };
        let limit = opt_usize(input, "limit").unwrap_or(FACET_DEFAULT_LIMIT).clamp(1, FACET_MAX_LIMIT);
        let classes = loaded.classes.clone();
        let root = self.session.root.clone();
        let resolve = move |class: &str| classes.project_file(class).map(|p| project_relative(&root, p));
        let d = digest(&records, &FacetQuery { facet, limit, window, conds }, &resolve)?;
        let mut cut = loaded.notes.clone();
        cut.extend(d.cut);
        Ok(clip(append_cut(format!("{}\n{}; {}\n{}", loaded.title, format_lines_read(&loaded), loaded.cursor, d.text), &cut)))
    }

    fn log_record(&self, input: &Value) -> Result<String, String> {
        let src = parse_source(str_arg(input, "source")?)?;
        let id = input.get("id").and_then(Value::as_u64).ok_or("`id` is required: the number after `#` in a digest or in run_logs")?;
        let context = opt_usize(input, "context").unwrap_or(0).min(5);
        let frames = opt_usize(input, "frames").unwrap_or(200).clamp(1, MAX_FRAMES);
        let loaded = match &src {
            Src::File(path) => self.load_file(path, None, FILE_KB_DEFAULT, Some(id))?,
            other => self.load(other, &json!({}))?,
        };
        let records = build_records(&loaded.classes.rules(), &loaded.lines);
        let at = records
            .iter()
            .position(|r| r.id == id || (r.id <= id && id <= r.last_seq))
            .ok_or_else(|| not_found(id, &records))?;
        let from = if matches!(src, Src::File(_)) { at } else { at.saturating_sub(context) };
        let to = (at + context).min(records.len() - 1);
        let started = loaded.started_ms;
        let prefix = move |r: &Record| record_prefix(r, started);
        let options = ReadOptions { tail: MAX_TAIL, frames, fold: false, ..Default::default() };
        let rendered = render_records(&records[from..=to], &options, &prefix);
        let mut cut = loaded.notes.clone();
        cut.extend(rendered.cut);
        Ok(clip(append_cut(format!("{}\n{}", loaded.title, rendered.text), &cut)))
    }
}

fn format_lines_read(l: &Loaded) -> String {
    format!("{} line(s) read", l.lines.len())
}

fn not_found(id: u64, records: &[Record]) -> String {
    match records.first() {
        Some(first) => format!("no record #{id} in this source (the oldest still kept is #{}); it may have been dropped by the ring buffer", first.id),
        None => format!("no record #{id}: the source has no lines"),
    }
}

/// `[+3.2 s #42 err] ` for a live source, `[#42] ` for a file (its id is a byte offset).
fn record_prefix(r: &Record, started_ms: Option<u64>) -> String {
    let tag = match r.stream {
        Stream::Stderr => " err",
        Stream::System => " sys",
        Stream::Stdout => "",
    };
    match (started_ms, r.ts_ms) {
        (Some(s), Some(t)) => format!("[+{} #{}{tag}] ", duration_text(t.saturating_sub(s)), r.id),
        _ => format!("[#{}] ", r.id),
    }
}

fn plain_lines(lines: &[LogLine], started_ms: u64) -> String {
    lines.iter().map(|l| format!("[+{} #{}] {}", duration_text(l.ts_ms.saturating_sub(started_ms)), l.seq, l.text)).collect::<Vec<_>>().join("\n")
}

/// The "cut" block: one bullet per thing left out, each saying how to get it.
fn append_cut(mut out: String, cut: &[String]) -> String {
    if !out.ends_with('\n') {
        out.push('\n');
    }
    if !cut.is_empty() {
        out.push_str("left out:\n");
        for c in cut {
            out.push_str(&format!("- {c}\n"));
        }
    }
    out
}

fn read_range(path: &Path, start: u64, len: u64) -> Result<Vec<u8>, String> {
    let mut file = std::fs::File::open(path).map_err(|e| format!("cannot open `{}`: {e}", path.display()))?;
    file.seek(SeekFrom::Start(start)).map_err(|e| e.to_string())?;
    let mut bytes = Vec::with_capacity(len.min(8 * 1024 * 1024) as usize);
    file.take(len).read_to_end(&mut bytes).map_err(|e| format!("cannot read `{}`: {e}", path.display()))?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_source_is_a_process_the_console_or_a_file() {
        assert_eq!(parse_source("api"), Ok(Src::Process("api".into())));
        assert_eq!(parse_source("console"), Ok(Src::Console(None)));
        assert_eq!(parse_source("console:r3"), Ok(Src::Console(Some("r3".into()))));
        assert_eq!(parse_source("file: logs/app.log"), Ok(Src::File("logs/app.log".into())));
        assert!(parse_source(" ").is_err());
    }

    #[test]
    fn paths_are_shown_relative_to_the_project() {
        assert_eq!(project_relative("C:\\work\\shop", "C:/work/shop/src/main/java/A.java"), "src/main/java/A.java");
        assert_eq!(project_relative("/work/shop/", "/work/shop/src/A.java"), "src/A.java");
        assert_eq!(project_relative("/work/shop", "/work/shop2/A.java"), "/work/shop2/A.java");
    }

    #[test]
    fn filters_have_defaults_caps_and_clear_errors() {
        let f = Filters::from_input(&json!({})).unwrap();
        assert_eq!((f.tail, f.context, f.frames, f.summary), (80, 0, 12, false));
        let f = Filters::from_input(&json!({"tail": 9999, "context": 99, "frames": 9999, "level": "warning", "pattern": "x", "summary": true})).unwrap();
        assert_eq!((f.tail, f.context, f.frames, f.level, f.summary), (MAX_TAIL, MAX_CONTEXT, MAX_FRAMES, Some(Level::Warn), true));
        assert!(Filters::from_input(&json!({"pattern": "("})).unwrap_err().contains("not a valid regex"));
        assert!(Filters::from_input(&json!({"level": "loud"})).is_err());
    }

    #[test]
    fn the_prefix_is_relative_for_live_sources_and_an_offset_for_files() {
        let mut r = Record { id: 42, last_seq: 42, ts_ms: Some(5_000), level: None, stream: Stream::Stderr, logger: None, message: String::new(), head: String::new(), rest: vec![], dropped_rest: 0 };
        assert_eq!(record_prefix(&r, Some(2_000)), "[+3.0 s #42 err] ");
        r.ts_ms = None;
        assert_eq!(record_prefix(&r, None), "[#42] ");
    }

    #[test]
    fn the_cut_block_lists_what_was_left_out() {
        assert_eq!(append_cut("body".into(), &[]), "body\n");
        assert_eq!(append_cut("body\n".into(), &["a".into(), "b".into()]), "body\nleft out:\n- a\n- b\n");
    }

    #[test]
    fn every_log_spec_is_declared_and_the_filters_merge_into_a_tools_own_properties() {
        let names: Vec<String> = specs().into_iter().map(|s| s.name).collect();
        assert_eq!(names, NAMES.iter().map(|n| n.to_string()).collect::<Vec<_>>());
        let merged = filter_props(json!({"id": {"type": "string"}}));
        assert!(merged.get("id").is_some() && merged.get("level").is_some() && merged.get("summary").is_some());
    }
}
