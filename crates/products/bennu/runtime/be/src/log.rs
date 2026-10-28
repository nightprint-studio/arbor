//! Turning a line of a child process's output into something a console can read.
//!
//! The interpretation itself is [`arbor_logscan`] — levels, timestamps, threads, URLs,
//! paths, qualified names, exceptions and stack frames — and it is deliberately ignorant of
//! Java projects. What it produces for a frame is a [`Link::Source`]: the *class* the frame
//! named. This module is where that becomes something openable, because turning
//! `com.acme.Order` into `C:/…/Order.java` needs the project's class index, and the class
//! index is exactly the thing a general-purpose log interpreter must not know about.
//!
//! A frame whose class the index does not know — the JDK, a dependency jar — keeps its
//! `Source` link and is resolved **when it is clicked** ([`bennu_frame_source`]): that answer
//! means reading jars, and it is worth paying for the one frame someone clicks rather than
//! for all forty as they stream past. A frame naming something made at runtime (a lambda
//! carrier, a proxy) carries no link at all — there is no source anywhere, and a link that
//! always fails teaches you not to click any of them.
//!
//! ## One annotator per stream
//!
//! Level inheritance is what makes a stack trace read as one error instead of one red line
//! and twenty grey ones, and it depends on the line *before*. stdout and stderr are
//! interleaved by the operating system in an order neither agreed to, so they get one
//! annotator each — otherwise a chatty stdout would break up stderr's trace. The class map
//! behind them is shared (an `Arc`), so that costs two `Option<Level>` and nothing else.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use arbor_logscan::prelude::{Level, Link, LogReader, PatternLayout, RuleSet};
use bennu_core::prelude::BennuState;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::index_service::IndexService;
use crate::intel::DecompiledLocation;

/// What the annotators of one run know about the project: its classes (fully-qualified name →
/// the absolute path of the file declaring it) and the layout its logs are written in. Shared by
/// the annotators of one run; built once, from the index the project already has and from the
/// logging configuration it already has.
pub(crate) type ClassMap = Arc<ClassIndex>;

pub(crate) struct ClassIndex {
    files: HashMap<String, String>,
    /// Simple name → every fully-qualified name that declares it, for a logger written as
    /// `OrderService` (`%logger{1}`) or abbreviated (`c.a.o.OrderService`).
    by_simple: HashMap<String, Vec<String>>,
    layout: Option<PatternLayout>,
}

impl ClassIndex {
    pub(crate) fn new(files: HashMap<String, String>) -> Self {
        let mut by_simple: HashMap<String, Vec<String>> = HashMap::new();
        for fqcn in files.keys() {
            let simple = fqcn.rsplit('.').next().unwrap_or(fqcn);
            by_simple.entry(simple.to_string()).or_default().push(fqcn.clone());
        }
        ClassIndex { files, by_simple, layout: None }
    }

    pub(crate) fn with_layout(mut self, layout: Option<PatternLayout>) -> Self {
        self.layout = layout;
        self
    }

    /// The source file declaring `class`. A nested class (`com.acme.Foo$Inner`) is looked up by
    /// its outer name, which is the one with a file.
    fn file_of(&self, class: &str) -> Option<&String> {
        let outer = arbor_logscan::prelude::outer_class(class);
        self.files.get(outer).or_else(|| self.files.get(class))
    }

    /// What a NAME in a log line points at — `com.acme.Order`, `com.acme.Order.total`,
    /// `Order#total`, a bare or abbreviated logger — as the file to open and the method to land
    /// on. `None` when no class of this project answers to it, or more than one does: a click
    /// that opens the wrong class is worse than a word that is not a link.
    fn resolve(&self, class: &str, method: Option<&str>) -> Option<(String, Option<String>)> {
        if let Some(path) = self.file_of(class) {
            return Some((path.clone(), method.map(str::to_string)));
        }
        if method.is_none() {
            // `com.acme.Order.total`: the last segment is a method when what precedes it is a class.
            if let Some((parent, tail)) = class.rsplit_once('.') {
                if tail.starts_with(char::is_lowercase) {
                    if let Some(path) = self.file_of(parent) {
                        return Some((path.clone(), Some(tail.to_string())));
                    }
                }
            }
        }
        let path = self.by_shape(class)?;
        Some((path, method.map(str::to_string)))
    }

    /// A class named by its simple name or with its packages abbreviated to initials — Logback's
    /// `%logger{1}` and `%logger{20}` — when exactly one class fits.
    fn by_shape(&self, written: &str) -> Option<String> {
        let mut segments: Vec<&str> = written.split('.').collect();
        let simple = segments.pop()?;
        let candidates = self.by_simple.get(simple)?;
        let fits: Vec<&String> = candidates
            .iter()
            .filter(|fqcn| {
                let package: Vec<&str> = fqcn.split('.').collect();
                let package = &package[..package.len() - 1];
                // No packages written: the simple name alone. Otherwise each written segment is
                // the start of the package segment it stands for, as many as were kept.
                segments.is_empty()
                    || (segments.len() <= package.len()
                        && segments.iter().rev().zip(package.iter().rev()).all(|(w, p)| p.starts_with(w)))
            })
            .collect();
        match fits.as_slice() {
            [only] => self.files.get(*only).cloned(),
            _ => None,
        }
    }
}

/// What to do with a span's link on its way out — see [`LogAnnotator::line`].
enum Resolution {
    Keep,
    Replace(Link),
    Drop,
}

/// The longest line the console carries, in characters.
///
/// Nothing stops a Java program from printing a line that is megabytes long — the `toString`
/// of a loaded collection, a response body logged whole, a stack trace someone concatenated —
/// and every stage downstream pays for it in full: the scanner walks it, the frame carries it
/// across the seam, the store keeps it, and the webview lays it out as one row that cannot be
/// broken. The tail of such a line has never been the part anyone was reading.
///
/// Generous on purpose. This is a guard against the pathological line, not a formatting
/// policy: a cap low enough to cut ordinary output would be the console lying about what the
/// program said.
const MAX_LINE_CHARS: usize = 4000;

/// `raw`, shortened to something a console can carry, saying so where it was cut.
///
/// Free on the overwhelming majority of lines: the byte length is an upper bound on the
/// character count, so anything short enough in bytes is returned without a scan at all.
fn clamp(raw: &str) -> Cow<'_, str> {
    if raw.len() <= MAX_LINE_CHARS {
        return Cow::Borrowed(raw);
    }
    // Character-indexed, not byte-indexed: a cut in the middle of a UTF-8 sequence would panic,
    // and the first accented log line is not when you want to find that out.
    match raw.char_indices().nth(MAX_LINE_CHARS) {
        None => Cow::Borrowed(raw),
        Some((cut, _)) => {
            let dropped = raw[cut..].chars().count();
            Cow::Owned(format!("{}… (+{dropped} characters)", &raw[..cut]))
        }
    }
}

/// The project's classes as a lookup. Empty (never an error) when the index has not landed
/// yet — a run whose frames are not clickable is worse than one whose frames are, and much
/// better than one that waits.
pub(crate) fn class_map(root: &str) -> ClassMap {
    let mut map = HashMap::new();
    if let Some(entries) = IndexService::global().class_index(root) {
        for entry in entries {
            map.entry(entry.fqcn).or_insert(entry.file);
        }
    }
    // The layout the project's logs are written in, read from its logging configuration — a
    // few small files, once per run, never on the output path.
    let layout = crate::log_pattern::detect(std::path::Path::new(root));
    Arc::new(ClassIndex::new(map).with_layout(layout))
}

/// One output stream's interpreter: the reader (which remembers the previous line) plus the
/// class map (which resolves this project's frames).
pub(crate) struct LogAnnotator {
    reader: LogReader,
    classes: ClassMap,
}

impl LogAnnotator {
    pub(crate) fn new(classes: ClassMap) -> Self {
        let rules = match &classes.layout {
            Some(layout) => layout.clone().over_java(),
            None => RuleSet::java(),
        };
        LogAnnotator { reader: LogReader::new(rules), classes }
    }

    /// The annotator for a project, resolving its own class map.
    pub(crate) fn for_root(root: &str) -> Self {
        LogAnnotator::new(class_map(root))
    }

    /// One line, as the wire carries it: `{ text, level, pieces }`.
    ///
    /// `text` is the line with the escapes gone — what a copy or a search should see.
    /// `pieces` is the same text already cut up, so the frontend renders it without doing
    /// offset arithmetic: this side counts UTF-8 bytes and that side counts UTF-16 code
    /// units, and a range crossing the seam would be a bug waiting for the first accented
    /// log line.
    pub(crate) fn line(&mut self, raw: &str) -> Value {
        // Before the scanner rather than after: the point is not to send less, it is not to
        // *walk* a megabyte to produce pieces nobody will read (see [`MAX_LINE_CHARS`]).
        let mut line = self.reader.read(&clamp(raw));
        for span in &mut line.spans {
            // `Link::Source` is the interpreter saying "a class was named here". A class of
            // THIS project resolves now, from a map already in memory. Anything else — the
            // JDK, a dependency — stays a `source` link: resolving it means opening jars, and
            // that is worth doing on the one frame that gets clicked, not on all forty
            // (`bennu_frame_source`).
            let resolution = match &span.link {
                // A frame: it always carries its line.
                Some(Link::Source { class, line: Some(number), .. }) => {
                    match self.classes.file_of(class) {
                        Some(path) => Resolution::Replace(Link::File {
                            path: path.clone(),
                            line: Some(*number),
                            method: None,
                        }),
                        None => Resolution::Keep,
                    }
                }
                // A NAME (a qualified class, `Order#total`, a logger): this project's class or
                // nothing. A library name is not resolved while streaming, and offering it as a
                // link would promise a click that mostly leads nowhere.
                Some(Link::Source { class, method, line: None, .. }) => {
                    match self.classes.resolve(class, method.as_deref()) {
                        Some((path, method)) => {
                            Resolution::Replace(Link::File { path, line: None, method })
                        }
                        None => Resolution::Drop,
                    }
                }
                _ => Resolution::Keep,
            };
            match resolution {
                Resolution::Keep => {}
                Resolution::Replace(link) => span.link = Some(link),
                Resolution::Drop => span.link = None,
            }
        }
        let pieces = serde_json::to_value(line.pieces()).unwrap_or_else(|_| Value::Array(Vec::new()));
        json!({
            "text": line.text,
            "level": line.level.map(Level::as_str),
            "pieces": pieces,
        })
    }
}

// ── clicking a frame we could not resolve while streaming ───────────────────────

/// Args for [`bennu_frame_source`].
#[derive(Deserialize)]
pub struct FrameSourceArgs {
    /// The open project's root (which picks the classpath resolver).
    pub root: String,
    /// The class the frame named, as written in it (`java.lang.Thread`, `com.acme.Foo$Inner`).
    pub class: String,
    /// The method the frame named — where to land when the view has no usable line numbers.
    #[serde(default)]
    pub method: Option<String>,
    /// The frame's line, meaningful only against real source.
    #[serde(default)]
    pub line: Option<u32>,
}

/// Open the source view for a stack-trace frame naming a **library or JDK** class: the real
/// `.java` from the JDK's `src.zip` or a downloaded `-sources.jar` when there is one, else a
/// stub decompiled from the bytecode.
///
/// Asked on the click and not while streaming, which is the point: reading jars for every
/// frame of every trace would cost the console its speed to answer a question nobody asked
/// of thirty-nine of the forty frames. A frame in a class this project declares never gets
/// here — it was resolved from the class index as the line went past.
///
/// `None`-shaped empty result when nothing resolves (a project type, an undecodable class, no
/// open project) — the frontend then leaves the click as a no-op rather than opening
/// something wrong.
#[arbor_rpc::handler]
fn bennu_frame_source(
    _ctx: &BennuState,
    args: FrameSourceArgs,
) -> Result<Option<DecompiledLocation>, String> {
    Ok(IndexService::global()
        .frame_source(&args.root, &args.class, args.method.as_deref(), args.line)
        .map(|v| DecompiledLocation {
            file: v.file,
            offset: v.offset,
            can_download: v.can_download,
        }))
}

// ── landing on a method named in a log ──────────────────────────────────────────

/// Args for [`bennu_method_line`].
#[derive(Deserialize)]
pub struct MethodLineArgs {
    /// The source file a log link resolved to.
    pub path: String,
    /// The method the log named (`Order#total`, `com.acme.Order.total`).
    pub method: String,
}

/// The line a method is declared on in a project source file — what a log link that names a method
/// but no line (`com.acme.Order#total`) lands on.
///
/// Asked on the click and not while streaming: it parses the file, which is worth doing for the one
/// link somebody follows and not for every name that goes past. The first declaration wins when the
/// name is overloaded; `None` (the click then just opens the file) when there is no such method.
#[arbor_rpc::handler]
fn bennu_method_line(_ctx: &BennuState, args: MethodLineArgs) -> Result<Option<u32>, String> {
    // Lossy: a legacy project's sources are not always UTF-8, and a replacement character in a
    // comment must not stop a click from finding the method below it.
    let Ok(bytes) = std::fs::read(&args.path) else {
        return Ok(None);
    };
    Ok(method_line(&String::from_utf8_lossy(&bytes), &args.method))
}

/// The 1-based line of the first method called `method` in `source`.
pub(crate) fn method_line(source: &str, method: &str) -> Option<u32> {
    let symbols = bennu_java::prelude::extract_symbols(source);
    let span = symbols
        .types
        .iter()
        .flat_map(|t| t.methods.iter())
        .filter(|m| m.name == method)
        .find_map(|m| m.span.as_ref())?;
    let newlines = source.as_bytes()[..span.start.min(source.len())].iter().filter(|b| **b == b'\n').count();
    Some(newlines as u32 + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = "package a;\n\npublic class Order {\n    int total() { return 1; }\n\n    void other() {}\n    void other(int x) {}\n}\n";

    #[test]
    fn a_method_is_found_on_the_line_it_is_declared() {
        assert_eq!(method_line(SOURCE, "total"), Some(4));
        assert_eq!(method_line(SOURCE, "other"), Some(6), "the first overload");
    }

    #[test]
    fn a_method_that_is_not_there_has_no_line() {
        assert_eq!(method_line(SOURCE, "missing"), None);
        assert_eq!(method_line("", "total"), None);
    }

    fn annotator(pairs: &[(&str, &str)]) -> LogAnnotator {
        let map: HashMap<String, String> =
            pairs.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect();
        LogAnnotator::new(Arc::new(ClassIndex::new(map)))
    }

    /// An annotator whose project logs in `pattern`.
    fn annotator_with_layout(pairs: &[(&str, &str)], pattern: &str) -> LogAnnotator {
        let map: HashMap<String, String> =
            pairs.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect();
        let layout = PatternLayout::parse(pattern).expect("a layout this understands");
        LogAnnotator::new(Arc::new(ClassIndex::new(map).with_layout(Some(layout))))
    }

    fn link_of<'a>(value: &'a Value, token: &str) -> Option<&'a Value> {
        value["pieces"].as_array()?.iter().find(|p| p["token"] == token).and_then(|p| p.get("link"))
    }

    const LAYOUT: &str = "%d{yyyy-MM-dd HH:mm:ss} | %-5level | %logger{1} | MDC:[tenant=%X{tenant.id}] | %msg%n";

    #[test]
    fn a_qualified_class_in_a_message_opens_its_file() {
        let mut log = annotator(&[("com.acme.Order", "/p/Order.java")]);
        let value = log.line("INFO started com.acme.Order ok");
        let link = link_of(&value, "package").expect("a link");
        assert_eq!(link["kind"], "file");
        assert_eq!(link["path"], "/p/Order.java");
        assert!(link.get("line").is_none() && link.get("method").is_none());
    }

    #[test]
    fn a_hash_names_the_method_to_land_on() {
        let mut log = annotator(&[("com.acme.Order", "/p/Order.java")]);
        let link = link_of(&log.line("failed in com.acme.Order#total now"), "package").cloned().unwrap();
        assert_eq!((link["path"].as_str(), link["method"].as_str()), (Some("/p/Order.java"), Some("total")));
    }

    #[test]
    fn a_method_written_after_a_dot_is_told_from_a_package() {
        let mut log = annotator(&[("com.acme.Order", "/p/Order.java")]);
        let link = link_of(&log.line("calling com.acme.Order.total now"), "package").cloned().unwrap();
        assert_eq!(link["method"], "total");
    }

    /// A name no class of the project answers to is a styled word, not a link.
    #[test]
    fn an_unknown_name_is_not_clickable() {
        let mut log = annotator(&[("com.acme.Order", "/p/Order.java")]);
        let value = log.line("see org.springframework.boot.SpringApplication here");
        assert!(link_of(&value, "package").is_none());
        assert!(value["pieces"].as_array().unwrap().iter().any(|p| p["token"] == "package"));
    }

    #[test]
    fn a_bare_logger_resolves_when_one_class_has_that_name() {
        let mut log = annotator_with_layout(&[("com.acme.order.OrderService", "/p/OrderService.java")], LAYOUT);
        let value = log.line(
            "2026-10-28 14:03:22 | ERROR | OrderService | MDC:[tenant=acme] | Could not place order",
        );
        let link = link_of(&value, "logger").expect("the logger links to its class");
        assert_eq!(link["path"], "/p/OrderService.java");
        assert_eq!(value["level"], "error");
        assert!(value["pieces"].as_array().unwrap().iter().any(|p| p["token"] == "mdc" && p["text"] == "acme"));
    }

    #[test]
    fn a_logger_two_classes_share_is_not_guessed() {
        let mut log = annotator_with_layout(
            &[("a.b.OrderService", "/p/a/OrderService.java"), ("c.d.OrderService", "/p/c/OrderService.java")],
            LAYOUT,
        );
        let value = log.line("2026-10-28 14:03:22 | INFO  | OrderService | MDC:[tenant=x] | hi");
        assert!(link_of(&value, "logger").is_none());
    }

    #[test]
    fn an_abbreviated_logger_is_matched_by_its_initials() {
        let mut log = annotator_with_layout(
            &[
                ("com.acme.order.OrderService", "/p/order/OrderService.java"),
                ("com.acme.billing.OrderService", "/p/billing/OrderService.java"),
            ],
            "%d{HH:mm:ss} %-5level %logger{20} - %msg%n",
        );
        let value = log.line("14:03:22 INFO  c.a.o.OrderService - hi");
        assert_eq!(link_of(&value, "logger").expect("unique by initials")["path"], "/p/order/OrderService.java");
    }

    #[test]
    fn a_layout_does_not_disturb_a_stack_trace() {
        let mut log = annotator_with_layout(&[("com.acme.Order", "/p/Order.java")], LAYOUT);
        log.line("2026-10-28 14:03:22 | ERROR | OrderService | MDC:[tenant=x] | boom");
        let value = log.line("\tat com.acme.Order.total(Order.java:118)");
        assert_eq!(value["level"], "error");
        assert!(value["pieces"].as_array().unwrap().iter().any(|p| p["link"]["line"] == 118));
    }

    #[test]
    fn a_frame_in_the_project_becomes_an_openable_file() {
        let mut log = annotator(&[("com.acme.Order", "C:/p/src/main/java/com/acme/Order.java")]);
        let value = log.line("\tat com.acme.Order.total(Order.java:118)");
        let pieces = value["pieces"].as_array().unwrap();
        let link = pieces.iter().find_map(|p| p.get("link")).expect("a linked piece");
        assert_eq!(link["kind"], "file");
        assert_eq!(link["path"], "C:/p/src/main/java/com/acme/Order.java");
        assert_eq!(link["line"], 118);
    }

    #[test]
    fn a_nested_class_resolves_through_its_outer_one() {
        let mut log = annotator(&[("com.acme.Order", "/p/Order.java")]);
        let value = log.line("\tat com.acme.Order$Line.price(Order.java:204)");
        let pieces = value["pieces"].as_array().unwrap();
        assert!(pieces.iter().any(|p| p.get("link").is_some()));
    }

    #[test]
    fn a_library_frame_stays_a_frame_the_host_can_resolve_later() {
        let mut log = annotator(&[]);
        let value = log.line("\tat java.base/java.lang.Thread.run(Thread.java:840)");
        let pieces = value["pieces"].as_array().unwrap();
        let link = pieces.iter().find_map(|p| p.get("link")).expect("an unresolved frame link");
        // Still a `source`: which file holds `java.lang.Thread` is a question about jars, and
        // it is asked when the frame is clicked — see `bennu_frame_source`.
        assert_eq!(link["kind"], "source");
        assert_eq!(link["class"], "java.lang.Thread");
        assert_eq!(link["method"], "run");
        assert_eq!(link["line"], 840);
    }

    #[test]
    fn a_trace_inherits_the_level_of_the_error_above_it() {
        let mut log = annotator(&[]);
        assert_eq!(log.line("ERROR could not place the order")["level"], "error");
        assert_eq!(log.line("\tat com.acme.Order.total(Order.java:118)")["level"], "error");
    }

    #[test]
    fn an_ordinary_line_is_carried_whole() {
        let long = "x".repeat(MAX_LINE_CHARS);
        assert!(matches!(clamp(&long), Cow::Borrowed(_)), "no copy when nothing is cut");
        assert_eq!(clamp(&long), long);
    }

    #[test]
    fn a_line_nobody_could_read_is_cut_and_says_so() {
        let huge = "y".repeat(MAX_LINE_CHARS + 500);
        let cut = clamp(&huge);
        assert!(cut.ends_with("… (+500 characters)"));
        assert_eq!(cut.chars().count(), MAX_LINE_CHARS + "… (+500 characters)".chars().count());
    }

    /// The cap counts characters, and the cut lands on a character boundary — a byte-indexed
    /// slice through a multi-byte sequence panics, and log lines are exactly where accented
    /// text turns up.
    #[test]
    fn cutting_a_line_of_accents_does_not_split_a_character() {
        let accents = "à".repeat(MAX_LINE_CHARS + 10);
        let cut = clamp(&accents);
        assert!(cut.starts_with('à'));
        assert!(cut.ends_with("… (+10 characters)"));
    }

    #[test]
    fn an_ordinary_line_costs_one_piece_and_no_level() {
        let mut log = annotator(&[]);
        let value = log.line("Hello, world!");
        assert_eq!(value["text"], "Hello, world!");
        assert!(value["level"].is_null());
        assert_eq!(value["pieces"].as_array().unwrap().len(), 1);
    }
}
