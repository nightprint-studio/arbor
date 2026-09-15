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

use arbor_logscan::prelude::{Level, Link, LogReader, RuleSet};
use bennu_core::prelude::BennuState;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::index_service::IndexService;
use crate::intel::DecompiledLocation;

/// Fully-qualified class name → the absolute path of the file declaring it. Shared by the
/// annotators of one run; built once, from the index the project already has.
pub(crate) type ClassMap = Arc<HashMap<String, String>>;

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
    Arc::new(map)
}

/// One output stream's interpreter: the reader (which remembers the previous line) plus the
/// class map (which resolves this project's frames).
pub(crate) struct LogAnnotator {
    reader: LogReader,
    classes: ClassMap,
}

impl LogAnnotator {
    pub(crate) fn new(classes: ClassMap) -> Self {
        LogAnnotator { reader: LogReader::new(RuleSet::java()), classes }
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
            let resolved = match &span.link {
                Some(Link::Source { class, line: number, .. }) => {
                    self.file_of(class).map(|path| Link::File { path, line: *number })
                }
                _ => None,
            };
            if let Some(link) = resolved {
                span.link = Some(link);
            }
        }
        let pieces = serde_json::to_value(line.pieces()).unwrap_or_else(|_| Value::Array(Vec::new()));
        json!({
            "text": line.text,
            "level": line.level.map(Level::as_str),
            "pieces": pieces,
        })
    }

    /// The source file declaring `class`, if this project declares it. A nested class
    /// (`com.acme.Foo$Inner`) is looked up by its outer name, which is the one with a file.
    fn file_of(&self, class: &str) -> Option<String> {
        let outer = arbor_logscan::prelude::outer_class(class);
        self.classes.get(outer).or_else(|| self.classes.get(class)).cloned()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn annotator(pairs: &[(&str, &str)]) -> LogAnnotator {
        let map: HashMap<String, String> =
            pairs.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect();
        LogAnnotator::new(Arc::new(map))
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
