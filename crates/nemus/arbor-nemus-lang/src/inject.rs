//! Host-injected capabilities, so the language layer stays free of the
//! filesystem and any console (`design/nemus/host-language.md`).
//!
//! - [`SourceLoader`] resolves `import` paths to source text — the shell wires a
//!   real filesystem; tests use an in-memory map.
//! - [`LogSink`] receives eval-time and per-hap log messages, **gated** by
//!   level: nothing below the threshold is ever produced (no flood).

/// Log severity, ascending (`trace` is the most verbose, `error` the loudest).
/// Ordering matters: a sink emits a message only when its level is `>=` the
/// current threshold.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    /// The lowercase keyword for this level (used by the emitter and parsing).
    pub fn as_str(self) -> &'static str {
        match self {
            LogLevel::Trace => "trace",
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
        }
    }

    /// Parse a level keyword, if it is one.
    pub fn parse(name: &str) -> Option<LogLevel> {
        Some(match name {
            "trace" => LogLevel::Trace,
            "debug" => LogLevel::Debug,
            "info" => LogLevel::Info,
            "warn" => LogLevel::Warn,
            "error" => LogLevel::Error,
            _ => return None,
        })
    }
}

/// Resolves an `import` path (relative to the importing file) to source text.
/// Returns an error message on failure; the evaluator wraps it with a span.
pub trait SourceLoader {
    fn load(&self, path: &str) -> std::result::Result<String, String>;
}

/// Receives log output. Implementations decide formatting/destination; the
/// evaluator only calls [`log`](LogSink::log) for messages that pass
/// [`enabled`](LogSink::enabled), so sub-threshold work is never done.
///
/// `Send + Sync`: the per-hap `.log(level)` transform captures the sink inside a
/// `Pattern` query closure, which the pattern crate requires to be thread-safe.
pub trait LogSink: Send + Sync {
    fn enabled(&self, level: LogLevel) -> bool;
    fn log(&self, level: LogLevel, message: &str);
}

/// A loader that rejects every import — the default when none is provided.
#[derive(Debug, Default)]
pub struct NoImports;

impl SourceLoader for NoImports {
    fn load(&self, path: &str) -> std::result::Result<String, String> {
        Err(format!("imports are not available (tried to load {path:?})"))
    }
}

/// A sink that drops everything — the default when no logging is wired.
#[derive(Debug, Default)]
pub struct SilentLog;

impl LogSink for SilentLog {
    fn enabled(&self, _level: LogLevel) -> bool {
        false
    }
    fn log(&self, _level: LogLevel, _message: &str) {}
}
