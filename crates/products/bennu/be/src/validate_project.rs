//! Whole-project "validation without compiling" — two entry points sharing one engine
//! ([`IndexService::validate_project_collect`]: read → parallel validate against the incremental
//! diagnostic cache → fold):
//!
//! - [`bennu_validate_project`] — the explicit action (the split-button alternative to
//!   `mvn compile`): it takes the shared [`BuildGuard`](crate::build::BuildGuard) (so a validation
//!   and a Maven build can't run at once), streams `arbor://bennu/validate-*` progress events for
//!   the Build tool window, and returns full timing statistics + the diagnostics grouped by file.
//! - [`bennu_project_diagnostics`] — the SILENT on-save refresh: no build guard, no events, no
//!   stats. It just re-validates (cheap, thanks to the cache) and returns the current diagnostics
//!   grouped by file, so the Problems panel reflects cross-file effects of a save without the user
//!   re-running validation. Returns `None` when the project's index isn't ready (nothing to report
//!   yet → the FE leaves the panel as-is).
//!
//! Threading: both are synchronous handlers the serve loop runs on their own thread, so validating
//! thousands of files never stalls the IPC loop; the validation itself runs across CPU cores
//! (leaving ~2 free for the interactive path).

use std::path::Path;

use bennu_core::prelude::BennuState;
use bennu_project::prelude::source_encoding_label;
use bennu_proto::prelude::{FileDiagnostics, ProjectValidationResult};
use serde::Deserialize;
use serde_json::json;

use crate::build::{BuildGuard, BUSY_MSG};
use crate::index_service::IndexService;

/// Progress ticks while a project validation runs (`{ root, done, total }`).
const EVT_VALIDATE_PROGRESS: &str = "arbor://bennu/validate-progress";
/// The validation finished (`{ root, total_files, errors, warnings, total_ms }`).
const EVT_VALIDATE_DONE: &str = "arbor://bennu/validate-done";

/// Cap on the per-file stat rows returned (the slowest files). Aggregates still cover every file;
/// this only bounds the detail table's payload on a huge project.
const MAX_STAT_ROWS: usize = 200;

/// Cap on how many files-with-diagnostics carry their diagnostics back (for the Problems panel).
/// The aggregate `error_count` / `warning_count` still reflect **every** file — this only bounds the
/// payload + DOM when a legacy project has thousands of problem files.
const MAX_DIAG_FILES: usize = 500;

/// Args for [`bennu_validate_project`] / [`bennu_project_diagnostics`].
#[derive(Deserialize)]
pub struct ValidateProjectArgs {
    /// Absolute path to the project root.
    pub root: String,
}

/// Validate every `.java` file in the project and return timing stats + diagnostics. A concurrent
/// build/validation is refused (`Err`) via the shared [`BuildGuard`].
#[arbor_rpc::handler]
fn bennu_validate_project(
    ctx: &BennuState,
    args: ValidateProjectArgs,
) -> Result<ProjectValidationResult, String> {
    let _guard = BuildGuard::acquire().ok_or_else(|| BUSY_MSG.to_string())?;
    let sink = ctx.event_sink();
    let label = source_encoding_label(Path::new(&args.root), "UTF-8");
    let svc = IndexService::global();

    // Validate the whole project (parallel, cache-backed), streaming progress to the Build window.
    let on_progress = |done: usize, total: usize| {
        sink.emit(EVT_VALIDATE_PROGRESS, json!({ "root": &args.root, "done": done, "total": total }));
    };
    let mut out = svc.validate_project_collect(&args.root, &label, &on_progress);
    eprintln!(
        "bennu-be: validated {} file(s) in {}ms, {} served from the diagnostic cache",
        out.validated, out.wall_ms, out.cached_hits
    );

    let total_ms = out.wall_ms; // real wall-clock of the parallel run
    let total_diagnostics = out.error_count + out.warning_count;
    let avg_ms = if out.validated > 0 { out.sum_ms as f64 / out.validated as f64 } else { 0.0 };

    // Slowest files first, capped — the detail table wants the outliers, not every file.
    out.stats.sort_by(|a, b| b.ms.cmp(&a.ms));
    out.stats.truncate(MAX_STAT_ROWS);
    out.diagnostics.truncate(MAX_DIAG_FILES);

    sink.emit(EVT_VALIDATE_DONE, json!({
        "root": &args.root,
        "total_files": out.validated,
        "errors": out.error_count,
        "warnings": out.warning_count,
        "total_ms": total_ms,
    }));

    Ok(ProjectValidationResult {
        total_files: out.validated,
        total_ms,
        avg_ms,
        max_ms: out.max_ms,
        max_file: out.max_file,
        total_diagnostics,
        error_count: out.error_count,
        warning_count: out.warning_count,
        files: out.stats,
        diagnostics: out.diagnostics,
    })
}

/// SILENT whole-project re-validation for the live Problems panel — the on-save refresh. No build
/// guard, no events, no stats: it re-validates (cheap, cache-backed) and returns the diagnostics
/// grouped by file so the panel reflects the cross-file effects of the save. Returns `None` when the
/// project's index isn't ready yet (the FE then leaves the panel unchanged rather than clearing it).
#[arbor_rpc::handler]
fn bennu_project_diagnostics(
    _ctx: &BennuState,
    args: ValidateProjectArgs,
) -> Result<Option<Vec<FileDiagnostics>>, String> {
    let svc = IndexService::global();
    // Not ready → don't touch the panel (a pure-AST pass would show a misleading partial picture).
    if !svc.has_resolver(&args.root) {
        return Ok(None);
    }
    let label = source_encoding_label(Path::new(&args.root), "UTF-8");
    let mut out = svc.validate_project_collect(&args.root, &label, &|_done, _total| {});
    out.diagnostics.truncate(MAX_DIAG_FILES);
    Ok(Some(out.diagnostics))
}
