//! Re-eval orchestration: `.merula` source -> `Tracks`, plus the host-injected
//! filesystem loader and log sink the language layer needs.
//!
//! `evaluate` is single-threaded (it builds `Rc`/`RefCell` internally), so the
//! whole parse+eval runs synchronously on the command thread; only the resulting
//! `Tracks` (which *is* `Send`) crosses to the audio thread. Language errors are
//! turned into [`MerulaDiagnostics`] with spans — they are *diagnostics*, not
//! command failures, so `merula_eval` still returns `Ok`.
//!
//! Ported from the shell's `src-tauri/src/merula/eval.rs`, with the [`LogSink`]
//! changed to capture an `Arc<dyn EventSink>` instead of the Tauri `AppHandle`.
//! The semantic validator (`validate`) lives here as a submodule rather than a
//! top-level domain — it is the eval layer's own semantic pass, shared with the
//! query / config / audio-command handlers.

pub(crate) mod validate;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use arbor_ipc::prelude::EventSink;
use merula::prelude::{
    evaluate, parse, EvalConfig, EvalOutput, LangError, LogLevel, LogSink, SourceLoader,
};

use merula_core::events::{emit, Diagnostic, LogLine, MerulaDiagnostics, EVT_LOG};

/// A filesystem [`SourceLoader`]. A `$lib/<name>/<file>` import resolves against
/// the synced external-library cache (`libs`, name -> cache dir); every other path
/// resolves against the project directory (`base`). Lives only for one evaluation
/// (never captured into the output), so `Rc` is fine.
struct FsLoader {
    base: PathBuf,
    libs: BTreeMap<String, PathBuf>,
}

impl SourceLoader for FsLoader {
    fn load(&self, path: &str) -> Result<String, String> {
        if let Some(rest) = path.strip_prefix("$lib/") {
            let (name, file) = rest
                .split_once('/')
                .ok_or_else(|| format!("import {path}: expected $lib/<name>/<file>"))?;
            let dir = self.libs.get(name).ok_or_else(|| {
                format!("library `{name}` is not synced — run Sync libraries (import {path})")
            })?;
            let resolved = dir.join(file);
            return std::fs::read_to_string(&resolved)
                .map_err(|e| format!("cannot read import {path}: {e}"));
        }
        let resolved = self.base.join(path);
        std::fs::read_to_string(&resolved)
            .map_err(|e| format!("cannot read import {}: {e}", resolved.display()))
    }
}

/// A [`LogSink`] that forwards (gated) log lines to the front end as `merula:log`.
/// `Send + Sync` because the per-hap `.log` transform captures it inside a
/// `Pattern` query closure that runs on the audio thread. Holds the backend event
/// sink (the shell re-emits to the merula window) instead of the Tauri `AppHandle`
/// the shell carried.
struct SinkLog {
    sink: Arc<dyn EventSink>,
    threshold: LogLevel,
}

impl LogSink for SinkLog {
    fn enabled(&self, level: LogLevel) -> bool {
        level >= self.threshold
    }

    fn log(&self, level: LogLevel, message: &str) {
        emit(
            &*self.sink,
            EVT_LOG,
            LogLine {
                level: level.as_str().to_string(),
                message: message.to_string(),
            },
        );
    }
}

/// Parse + evaluate `source`, resolving imports against `base_dir`.
///
/// On success returns the evaluated [`EvalOutput`] (cps + tracks). On a language
/// error returns the diagnostics (with span) so the caller can both emit them and
/// return them inline. The injected log sink is `Arc`-shared into the output's
/// patterns, so it stays alive as long as the `Tracks` it logs from.
pub fn evaluate_source(
    sink: Arc<dyn EventSink>,
    source: &str,
    base_dir: PathBuf,
    cfg: EvalConfig,
) -> Result<EvalOutput, MerulaDiagnostics> {
    let program = parse(source).map_err(|e| MerulaDiagnostics::one(to_diagnostic(&e)))?;

    // The synced external libraries (`$lib/<name>/…`) resolve from the project's
    // lock; a normal relative import still resolves against `base_dir`.
    let libs = crate::libraries::resolve_dirs(&base_dir);
    let loader: Rc<dyn SourceLoader> = Rc::new(FsLoader { base: base_dir, libs });
    let log: Arc<dyn LogSink> = Arc::new(SinkLog {
        sink,
        threshold: cfg.log_threshold,
    });

    evaluate(&program, loader, log, cfg).map_err(|e| MerulaDiagnostics::one(to_diagnostic(&e)))
}

/// Map a language error to a wire diagnostic. The message is the error kind
/// (the byte-range is carried in `start`/`end`, not duplicated in the text).
fn to_diagnostic(e: &LangError) -> Diagnostic {
    Diagnostic {
        message: e.kind.to_string(),
        severity: "error",
        start: e.span.map(|s| s.start),
        end: e.span.map(|s| s.end),
    }
}
