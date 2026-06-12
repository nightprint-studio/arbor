//! Re-eval orchestration: `.nemus` source → `Tracks`, plus the host-injected
//! filesystem loader and log sink the language layer needs.
//!
//! `evaluate` is single-threaded (it builds `Rc`/`RefCell` internally), so the
//! whole parse+eval runs synchronously on the command thread; only the resulting
//! `Tracks` (which *is* `Send`) crosses to the audio thread. Language errors are
//! turned into [`NemusDiagnostics`] with spans — they are *diagnostics*, not
//! command failures, so `nemus_eval` still returns `Ok`.

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use tauri::AppHandle;

use arbor_nemus::prelude::{
    evaluate, parse, EvalConfig, EvalOutput, LangError, LogLevel, LogSink, SourceLoader,
};

use super::events::{emit, Diagnostic, NemusDiagnostics, LogLine, EVT_LOG};

/// A filesystem [`SourceLoader`]: resolves `import` paths against the `.nemus`
/// file's directory. Lives only for the duration of one evaluation (never
/// captured into the output), so `Rc` is fine.
struct FsLoader {
    base: PathBuf,
}

impl SourceLoader for FsLoader {
    fn load(&self, path: &str) -> Result<String, String> {
        let resolved = self.base.join(path);
        std::fs::read_to_string(&resolved)
            .map_err(|e| format!("cannot read import {}: {e}", resolved.display()))
    }
}

/// A [`LogSink`] that forwards (gated) log lines to the nemus window as
/// `nemus:log`. `Send + Sync` because the per-hap `.log` transform captures it
/// inside a `Pattern` query closure that runs on the audio thread.
struct AppLogSink {
    app: AppHandle,
    threshold: LogLevel,
}

impl LogSink for AppLogSink {
    fn enabled(&self, level: LogLevel) -> bool {
        level >= self.threshold
    }

    fn log(&self, level: LogLevel, message: &str) {
        emit(
            &self.app,
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
    app: &AppHandle,
    source: &str,
    base_dir: PathBuf,
    cfg: EvalConfig,
) -> Result<EvalOutput, NemusDiagnostics> {
    let program = parse(source).map_err(|e| NemusDiagnostics::one(to_diagnostic(&e)))?;

    let loader: Rc<dyn SourceLoader> = Rc::new(FsLoader { base: base_dir });
    let log: Arc<dyn LogSink> = Arc::new(AppLogSink {
        app: app.clone(),
        threshold: cfg.log_threshold,
    });

    evaluate(&program, loader, log, cfg).map_err(|e| NemusDiagnostics::one(to_diagnostic(&e)))
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
