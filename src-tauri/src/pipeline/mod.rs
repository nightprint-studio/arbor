// `ci_client` was relocated to `crate::git_provider::ci_impl` in Phase 5 of
// the GitProvider refactor. This module is now just the Tauri-coupled
// orchestrator: the live per-run thread, event emission, shell-process
// spawning, and Lua-op dispatch.
//
// Everything host-free moved out in round-2 M2:
//   · the model + expression engine (defs, run state, vars, conditions,
//     builtins, if-blocks) → `corvus-pipeline-api`;
//   · the run registry, JSON persistence, and the pure orchestration helpers
//     (resume-cursor, step-tree lookup, output chunking, log inference)
//     → `corvus-pipeline-core`.
// They're re-exported here so existing `crate::pipeline::*` call sites keep
// resolving.
pub use corvus_pipeline_api::{builtin, vars};
pub use corvus_pipeline_api::prelude::{
    parse_log_level, parse_stage_mode,
    BuiltinSpec, CaptureSource, CaptureSpec, IfBlock,
    LogEvent, LogLevel, LuaOpSpec, PipelineDef, PipelineRun, ResumeCursor, RunContext, RunStatus,
    StageDef, StageMode, StepDef, StepRun, VarValue,
};
pub use corvus_pipeline_core::prelude::{
    compute_resume_cursor, drain_partial_line, find_step_mut, infer_step_log_level,
    now_ms, persist_run, registry_from_disk, resumable_step_indices,
    split_chunk_lines, step_preview, PipelineRegistry, RUN_LOG_CAP,
};

mod engine;
pub use engine::{PipelineEngine, PipelineRuntime};

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;
use crate::process_ext::NoWindowExt;

/// Type alias for the per-run variable context shared between every step
/// execution. Wrapped in `Arc<Mutex<…>>` so parallel stages can both read
/// and write without aliasing rules tripping us — see `vars::RunContext`
/// for the concurrency tradeoffs.
pub type RunCtx = Arc<Mutex<RunContext>>;

// ===========================================================================
// Orchestrator — one background thread per pipeline run
// ===========================================================================

pub fn start_pipeline_run(
    def:        PipelineDef,
    run_id:     String,
    repo_path:  Option<String>,
    cancel:     Arc<AtomicBool>,
    rt:         Arc<PipelineRuntime>,
) {
    if let Err(e) = std::thread::Builder::new()
        .name(format!("arbor-pipe-{run_id}"))
        .spawn(move || orchestrate(def, run_id, repo_path, cancel, rt))
    {
        tracing::error!("failed to spawn pipeline orchestrator thread: {e}");
    }
}

fn emit(rt: &PipelineRuntime, run: &PipelineRun) {
    if let Ok(value) = serde_json::to_value(run) {
        rt.sink.emit("arbor://pipeline-update", value);
    }
}

fn snapshot(pipelines: &Mutex<PipelineRegistry>, run_id: &str) -> Option<PipelineRun> {
    pipelines.lock().ok().and_then(|r| r.get_run(run_id).cloned())
}

/// Push a log event on the given run (filtered by its `log_level`, capped at
/// RUN_LOG_CAP entries) and broadcast it to the frontend for live streaming.
fn log_event(
    rt:         &PipelineRuntime,
    run_id:     &str,
    level:      LogLevel,
    scope:      impl Into<String>,
    message:    impl Into<String>,
) {
    let scope_s = scope.into();
    let msg_s   = message.into();
    let ts      = now_ms();

    // Mutate the run's log buffer (filtered by its configured min level).
    let should_emit = {
        let Ok(mut reg) = rt.engine.registry.lock() else { return; };
        let Some(run)   = reg.runs.iter_mut().find(|r| r.id == run_id) else { return; };
        if level.rank() < run.log_level.rank() { return; }
        run.log.push(LogEvent { ts, level, scope: scope_s.clone(), message: msg_s.clone() });
        if run.log.len() > RUN_LOG_CAP {
            let drop = run.log.len() - RUN_LOG_CAP;
            run.log.drain(0..drop);
        }
        true
    };

    if should_emit {
        rt.sink.emit("arbor://pipeline-log", serde_json::json!({
            "run_id":  run_id,
            "ts":      ts,
            "level":   level.tag(),
            "scope":   scope_s,
            "message": msg_s,
        }));
    }
}

fn fire_hook(rt: &PipelineRuntime, hook: &str, ctx: &serde_json::Value) {
    rt.fire_hook(hook, ctx.clone());
}

/// Execution result of a single step — used internally to merge parallel
/// outcomes before persisting them on the run.
///
/// Children of an `if_block` step are NOT carried here: they're written
/// directly into the run state's tree as they execute (so live streaming
/// hits them via `find_step_mut`), and the merge loop leaves the parent's
/// existing `children` Vec untouched.
struct StepOutcome {
    step_idx:   usize,
    status:     RunStatus,
    exit_code:  Option<i32>,
    output:     Vec<String>,
    started_at: i64,
    finished_at: i64,
    /// `if` / `elif #N` / `else` — populated only for `if_block` steps.
    branch:     String,
}

fn execute_step(
    step_def:   &StepDef,
    cwd:        &str,
    cancel:     &Arc<AtomicBool>,
    step_idx:   usize,
    rt:         &Arc<PipelineRuntime>,
    default_plugin: &str,
    pipeline_name:  &str,
    run_id:         &str,
    stage_id:       &str,
    ctx:            &RunCtx,
    parent_path:    &str,
) -> StepOutcome {
    let started = now_ms();
    // Effective id for log scoping + StepRun lookup. Top-level steps use
    // their plain `def.id`; nested children carry a slash-separated path
    // (`<parent>/<child>`) so siblings under different `if_block` parents
    // never collide in `find_step_mut`.
    let effective_id: String = if parent_path.is_empty() {
        step_def.id.clone()
    } else {
        format!("{parent_path}{}", step_def.id)
    };
    let sink = StepLogSink::new(
        rt, default_plugin, pipeline_name, run_id,
        stage_id, &effective_id, &step_def.name,
    );
    if cancel.load(Ordering::Relaxed) {
        let line = "[cancelled before start]";
        sink.emit(line);
        return StepOutcome {
            step_idx,
            status:       RunStatus::Cancelled,
            exit_code:    None,
            output:       vec![line.into()],
            started_at:   started,
            finished_at:  now_ms(),
            branch:       String::new(),
        };
    }
    // Resolve ${var} on the cwd override (step-level cwd may reference vars
    // captured by an earlier step). The caller already picked the effective
    // dir so we only run substitution if it looks templated.
    let cwd_resolved = if cwd.contains('$') {
        let c = ctx.lock().ok();
        match c {
            Some(g) => vars::resolve_vars(cwd, &g),
            None    => cwd.to_string(),
        }
    } else {
        cwd.to_string()
    };

    // Dispatch order: if_block > builtin > lua_op > shell command.
    let (exit_code, output, return_value, branch) =
        if let Some(block) = &step_def.if_block {
            execute_if_block(
                block, step_def, &cwd_resolved, cancel, rt,
                default_plugin, pipeline_name, run_id, stage_id, ctx,
                &effective_id, &sink,
            )
        } else if let Some(spec) = &step_def.builtin {
            let resolved_spec = match resolve_builtin_spec(spec, ctx) {
                Ok(v)  => v,
                Err(e) => {
                    let msg = format!("⚠ builtin: spec resolve failed: {e}");
                    sink.emit(&msg);
                    return StepOutcome {
                        step_idx,
                        status:       RunStatus::Failed,
                        exit_code:    Some(1),
                        output:       vec![msg],
                        started_at:   started,
                        finished_at: now_ms(),
                        branch:       String::new(),
                    };
                }
            };
            let outcome = {
                let g = ctx.lock().expect("run-ctx mutex");
                builtin::run_builtin(&resolved_spec, &cwd_resolved, &g)
            };
            for line in &outcome.lines { sink.emit(line); }
            (
                Some(outcome.exit_code),
                outcome.lines,
                Some(outcome.value),
                String::new(),
            )
        } else if let Some(op) = &step_def.lua_op {
            let resolved_params = {
                let g = ctx.lock().expect("run-ctx mutex");
                vars::resolve_vars_in_json(&op.params, &g)
            };
            let resolved_op = LuaOpSpec {
                plugin: op.plugin.clone(),
                op:     op.op.clone(),
                params: resolved_params,
            };
            let (exit, lines) = run_lua_op(
                &resolved_op, &cwd_resolved, rt, default_plugin, &sink,
            );
            let joined = lines.iter().filter(|l| !l.starts_with("[stderr]"))
                .cloned().collect::<Vec<_>>().join("\n");
            (exit, lines, Some(VarValue::String(joined)), String::new())
        } else {
            let (cmd, env) = {
                let g = ctx.lock().expect("run-ctx mutex");
                let c = vars::resolve_vars(&step_def.command, &g);
                let e: HashMap<String, String> = step_def.env.iter()
                    .map(|(k, v)| (k.clone(), vars::resolve_vars(v, &g)))
                    .collect();
                (c, e)
            };
            let (exit, lines) = run_command(&cmd, &cwd_resolved, &env, cancel, &sink);
            let joined = lines.iter().filter(|l| !l.starts_with("[stderr]"))
                .cloned().collect::<Vec<_>>().join("\n");
            (exit, lines, Some(VarValue::String(joined)), String::new())
        };

    // A cancel that landed mid-step makes us treat the run as Cancelled even
    // if the killed process happened to flush a 0 exit code first — without
    // this an `mvn clean package` killed by taskkill on Windows would
    // sometimes still surface as Failed/Success depending on the moment the
    // tree died, hiding the user's intent.
    //
    // For if_block steps, the "fail" semantics flow up from any failed child
    // (already encoded in the aggregated exit_code).
    let status = if cancel.load(Ordering::Relaxed) {
        RunStatus::Cancelled
    } else {
        match exit_code {
            Some(0) => RunStatus::Success,
            Some(_) | None => RunStatus::Failed,
        }
    };

    // Apply capture spec (if any). Failures inside the chain are non-fatal —
    // we set the var to Null and log the trace so the user can debug, but
    // don't promote the step to Failed because of it.
    if let Some(spec) = &step_def.capture {
        apply_capture(spec, &output, exit_code, return_value.as_ref(), ctx, &sink);
    }

    StepOutcome {
        step_idx,
        status,
        exit_code,
        output,
        started_at:  started,
        finished_at: now_ms(),
        branch,
    }
}

/// Resolve `${var}` substitutions inside a `BuiltinSpec` via a round-trip
/// through JSON. This keeps the dispatch agnostic of which fields are
/// strings — any future variant gains substitution for free.
fn resolve_builtin_spec(spec: &BuiltinSpec, ctx: &RunCtx)
    -> std::result::Result<BuiltinSpec, String>
{
    let json = serde_json::to_value(spec).map_err(|e| e.to_string())?;
    let resolved = {
        let g = ctx.lock().map_err(|_| "ctx mutex poisoned".to_string())?;
        vars::resolve_vars_in_json(&json, &g)
    };
    serde_json::from_value(resolved).map_err(|e| e.to_string())
}

/// Run an `if_block` step: pick the first matching branch, then sequentially
/// execute every nested step. Children are written directly into the parent
/// `StepRun.children` Vec (in run state) as they execute — that way live
/// streaming through `StepLogSink::emit_batch` lands on the right child via
/// the recursive `find_step_mut` lookup, and the UI can render in-flight
/// nested progress without waiting for the if_block to finish.
///
/// Returns the same four-tuple the leaf dispatch uses, so the caller can
/// merge it uniformly into the parent's `StepOutcome`.
#[allow(clippy::too_many_arguments)]
fn execute_if_block(
    block:          &IfBlock,
    parent:         &StepDef,
    cwd:            &str,
    cancel:         &Arc<AtomicBool>,
    rt:             &Arc<PipelineRuntime>,
    default_plugin: &str,
    pipeline_name:  &str,
    run_id:         &str,
    stage_id:       &str,
    ctx:            &RunCtx,
    parent_id:      &str,   // effective id of the parent (no trailing `/`)
    sink:           &StepLogSink,
) -> (Option<i32>, Vec<String>, Option<VarValue>, String) {
    let (selection, steps) = {
        let g = ctx.lock().expect("run-ctx mutex");
        let (sel, sts) = block.select(&g);
        (sel, sts.to_vec())
    };
    let label = selection.label();
    let mut log = vec![format!("[if] selected branch: {}", label)];
    sink.emit(&log[0]);

    let mut overall = RunStatus::Success;

    for (i, child) in steps.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            overall = RunStatus::Cancelled;
            break;
        }
        let child_def_id = format!("{parent_id}/{}", child.id);

        // Pre-create the child StepRun (status=Running) so live stdout
        // streaming finds it via `find_step_mut` while the step is in
        // flight.
        push_child_step_running(
            rt, run_id, stage_id, parent_id,
            &child_def_id, &child.name,
        );

        let child_outcome = execute_step(
            child, cwd, cancel, i, rt,
            default_plugin, pipeline_name, run_id, stage_id, ctx,
            // Prefix used by execute_step to build effective_id for the
            // grandchild dispatch. Trailing `/` is intentional.
            &format!("{parent_id}/"),
        );

        finalize_child_step(
            rt, run_id, stage_id, &child_def_id, &child_outcome,
        );

        let line = format!(
            "[if/{label}] step '{}' → {:?} (exit={:?})",
            child.name, child_outcome.status, child_outcome.exit_code,
        );
        log.push(line.clone());
        sink.emit(&line);

        let allow = child.allow_failure;
        let broke = child_outcome.status == RunStatus::Failed && !allow;
        if child_outcome.status == RunStatus::Cancelled {
            overall = RunStatus::Cancelled;
            break;
        }
        if broke { overall = RunStatus::Failed; break; }
    }

    let exit = match overall {
        RunStatus::Success => Some(0),
        RunStatus::Failed | RunStatus::Cancelled => Some(1),
        _ => None,
    };
    let return_value = Some(VarValue::String(label.clone()));
    let _ = parent; // currently unused; kept for future per-parent log scoping
    (exit, log, return_value, label)
}

/// Push a fresh `StepRun(Running)` onto a parent's `children` Vec. Idempotent
/// against re-entry (resume / nested if-block re-evaluation): if a child
/// with that `def_id` already exists in the tree it's reset in place.
fn push_child_step_running(
    rt:           &PipelineRuntime,
    run_id:       &str,
    stage_id:     &str,
    parent_id:    &str,
    child_def_id: &str,
    child_name:   &str,
) {
    {
        let Ok(mut reg) = rt.engine.registry.lock() else { return; };
        if let Some(r) = reg.runs.iter_mut().find(|r| r.id == run_id) {
            if let Some(s) = r.stages.iter_mut().find(|s| s.def_id == stage_id) {
                if let Some(parent) = find_step_mut(&mut s.steps, parent_id) {
                    // Reset existing or push new.
                    if let Some(existing) = parent.children.iter_mut()
                        .find(|c| c.def_id == child_def_id)
                    {
                        existing.status      = RunStatus::Running;
                        existing.output.clear();
                        existing.exit_code   = None;
                        existing.started_at  = Some(now_ms());
                        existing.finished_at = None;
                        existing.children.clear();
                        existing.branch.clear();
                    } else {
                        parent.children.push(StepRun {
                            def_id:      child_def_id.to_string(),
                            name:        child_name.to_string(),
                            status:      RunStatus::Running,
                            output:      Vec::new(),
                            started_at:  Some(now_ms()),
                            finished_at: None,
                            exit_code:   None,
                            children:    Vec::new(),
                            branch:      String::new(),
                        });
                    }
                }
            }
        }
    }
    if let Some(snap) = snapshot(&rt.engine.registry, run_id) {
        emit(rt, &snap);
    }
}

/// Write the final outcome of a child step (status, exit_code, output,
/// timing, branch label) into its already-pushed `StepRun`.
fn finalize_child_step(
    rt:           &PipelineRuntime,
    run_id:       &str,
    stage_id:     &str,
    child_def_id: &str,
    outcome:      &StepOutcome,
) {
    {
        let Ok(mut reg) = rt.engine.registry.lock() else { return; };
        if let Some(r) = reg.runs.iter_mut().find(|r| r.id == run_id) {
            if let Some(s) = r.stages.iter_mut().find(|s| s.def_id == stage_id) {
                if let Some(child) = find_step_mut(&mut s.steps, child_def_id) {
                    child.status      = outcome.status.clone();
                    child.exit_code   = outcome.exit_code;
                    child.started_at  = Some(outcome.started_at);
                    child.finished_at = Some(outcome.finished_at);
                    child.output      = outcome.output.clone();
                    child.branch      = outcome.branch.clone();
                }
            }
        }
    }
    if let Some(snap) = snapshot(&rt.engine.registry, run_id) {
        emit(rt, &snap);
    }
}

/// Apply a `CaptureSpec` after a step completes. Writes the resulting
/// `VarValue` under `spec.var` in `ctx`. Failures are reported through
/// `sink` (debug log) but never promoted to step failure — capture is a
/// best-effort side channel.
fn apply_capture(
    spec:         &CaptureSpec,
    output_lines: &[String],
    exit_code:    Option<i32>,
    return_value: Option<&VarValue>,
    ctx:          &RunCtx,
    sink:         &StepLogSink,
) {
    let src = match spec.source {
        CaptureSource::Stdout => {
            let stdout: Vec<&String> = output_lines.iter()
                .filter(|l| !l.starts_with("[stderr]"))
                .collect();
            VarValue::String(stdout.iter().map(|s| s.as_str())
                .collect::<Vec<_>>().join("\n"))
        }
        CaptureSource::Stderr => {
            let stderr: Vec<String> = output_lines.iter()
                .filter_map(|l| l.strip_prefix("[stderr] ").map(str::to_string))
                .collect();
            VarValue::String(stderr.join("\n"))
        }
        CaptureSource::ExitCode    => match exit_code {
            Some(c) => VarValue::Number(c as f64),
            None    => VarValue::Null,
        },
        CaptureSource::Success     => VarValue::Bool(matches!(exit_code, Some(0))),
        CaptureSource::ReturnValue => return_value.cloned().unwrap_or(VarValue::Null),
    };
    let (final_value, trace) = vars::apply_transforms(src, &spec.transforms);
    {
        if let Ok(mut g) = ctx.lock() {
            g.set(spec.var.clone(), final_value.clone());
        }
    }
    let preview = final_value.as_string();
    let preview = if preview.len() > 80 {
        format!("{}…", &preview[..80])
    } else {
        preview
    };
    sink.emit(&format!("[capture] ${} = {:?}", spec.var, preview));
    for t in &trace { sink.emit(t); }
}

/// Live sink for a single step's captured output. Cloning is cheap (all
/// fields are owned strings + the `PipelineRuntime` Arc) so the stderr reader
/// thread can take its own copy. Each `emit` call streams the line both
/// to the global Plugin Logs panel (via `arbor://plugin-log`) and to the
/// run's own log buffer (via `log_event` → `arbor://pipeline-log`), so
/// the UI sees output as it's produced rather than in one batch when the
/// step finishes. The caller still appends every emitted line to the
/// `StepRun.output` Vec for persistence and post-mortem replay.
#[derive(Clone)]
struct StepLogSink {
    rt:            Arc<PipelineRuntime>,
    plugin:        String,
    pipeline_name: String,
    run_id:        String,
    stage_id:      String,
    step_id:       String,
    step_name:     String,
    scope:         String,
}

impl StepLogSink {
    fn new(
        rt:            &Arc<PipelineRuntime>,
        plugin:        &str,
        pipeline_name: &str,
        run_id:        &str,
        stage_id:      &str,
        step_id:       &str,
        step_name:     &str,
    ) -> Self {
        Self {
            rt:            rt.clone(),
            plugin:        plugin.to_string(),
            pipeline_name: pipeline_name.to_string(),
            run_id:        run_id.to_string(),
            stage_id:      stage_id.to_string(),
            step_id:       step_id.to_string(),
            step_name:     step_name.to_string(),
            scope:         format!("step:{stage_id}.{step_id}"),
        }
    }

    /// Per-line side effects: Plugin Logs ring buffer + run-log event.
    /// CHEAP — local mutex on `rt.plugin_logs` + short level check on
    /// `rt.engine.registry` (returns early when below the run's `log_level`).
    /// Does **not** push to `StepRun.output` and does **not** emit
    /// `arbor://pipeline-step-output*`; both of those are batched by
    /// [`Self::emit_batch`] which the chunk reader calls once per drained
    /// pipe read instead of once per line.
    ///
    /// `[stderr]` / `WARN` / etc. prefixes are honored via
    /// [`infer_step_log_level`].
    fn record_line(&self, line: &str) {
        let level    = infer_step_log_level(line);
        let prefixed = if self.step_name.is_empty() {
            line.to_string()
        } else {
            format!("[{}] {line}", self.step_name)
        };
        crate::plugin_logs::record_with_pipeline_via(
            &self.rt.plugin_logs, &self.rt.sink,
            level, &self.plugin, prefixed,
            &self.pipeline_name, &self.run_id,
        );
        log_event(
            &self.rt, &self.run_id,
            LogLevel::Debug, self.scope.clone(), line.to_string(),
        );
    }

    /// Batch flush from the chunk reader. ONE `rt.engine.registry` lock for
    /// the whole batch + ONE `arbor://pipeline-step-output` IPC event
    /// carrying every line read in the latest pipe drain. This is the
    /// counterpart to the integrated terminal's `read(buf) → emit`
    /// model — instead of N events for N lines, the frontend gets one
    /// event with `lines: string[]` and applies them in a single Svelte
    /// reactivity tick.
    ///
    /// No-op on empty batch.
    fn emit_batch(&self, lines: &[String]) {
        if lines.is_empty() { return; }
        if let Ok(mut reg) = self.rt.engine.registry.lock() {
            if let Some(r) = reg.runs.iter_mut().find(|r| r.id == self.run_id) {
                if let Some(s) = r.stages.iter_mut().find(|s| s.def_id == self.stage_id) {
                    // Recursive lookup: top-level step OR any nested child
                    // produced by an `if_block` parent.
                    if let Some(st) = find_step_mut(&mut s.steps, &self.step_id) {
                        st.output.extend(lines.iter().cloned());
                    }
                }
            }
        }
        self.rt.sink.emit("arbor://pipeline-step-output", serde_json::json!({
            "run_id":   self.run_id,
            "stage_id": self.stage_id,
            "step_id":  self.step_id,
            "lines":    lines,
        }));
    }

    /// Single-line convenience used by `run_lua_op` and by the cancel /
    /// spawn-error paths, where the producer does not have a chunk-reader
    /// to amortise over. Internally a 1-line batch — semantically
    /// identical to the old per-line emit.
    fn emit(&self, line: &str) {
        self.record_line(line);
        self.emit_batch(&[line.to_string()]);
    }
}

/// Outcome of `acquire_run_slot`.
enum SlotAcquire {
    /// A slot was reserved (`running_count` incremented). Caller MUST pair
    /// with `release_run_slot` exactly once when the run leaves Running.
    Acquired,
    /// The cancel token fired while the orchestrator was parked waiting for
    /// a slot — no slot was reserved. Caller should mark the run Cancelled,
    /// release the lock_key and bail out.
    Cancelled,
}

/// Wait for a free concurrency slot, then increment `running_count`.
/// While parked the run stays `Pending` with `queued = true` — the panel
/// renders that as a "Queued" badge so the user can tell it apart from a
/// run that's about to start. `cap == 0` short-circuits to "unlimited".
///
/// Wakes on `rt.engine.cv.notify_*` (terminal release) AND on a 250 ms
/// poll timeout so a cancel signal lands within a quarter second even if no
/// other run is changing state.
fn acquire_run_slot(
    rt:         &PipelineRuntime,
    run_id:     &str,
    cancel:     &Arc<AtomicBool>,
) -> SlotAcquire {
    // Cap snapshotted at run start (`rt.max_concurrent_runs`). BEHAVIOR DELTA:
    // the wait loop no longer re-reads `config.pipelines.max_concurrent_runs`
    // each iteration, so a mid-run config change to the cap no longer affects
    // an already-queued run. Everything else is identical.
    let cap = rt.max_concurrent_runs;
    // Fast path: try without ever surfacing as "queued".
    {
        let Ok(mut reg) = rt.engine.registry.lock() else { return SlotAcquire::Cancelled; };
        if cancel.load(Ordering::Relaxed) {
            return SlotAcquire::Cancelled;
        }
        if cap == 0 || reg.running_count < cap as usize {
            reg.running_count += 1;
            return SlotAcquire::Acquired;
        }
        // Cap reached — surface the queued state to the UI before waiting.
        if let Some(r) = reg.runs.iter_mut().find(|r| r.id == run_id) {
            r.queued = true;
        }
    }
    // Emit queued snapshot outside the lock so listeners can update.
    if let Some(snap) = snapshot(&rt.engine.registry, run_id) {
        emit(rt, &snap);
        persist_run(&snap);
    }
    let mut guard = match rt.engine.registry.lock() {
        Ok(g)  => g,
        Err(_) => return SlotAcquire::Cancelled,
    };
    loop {
        if cancel.load(Ordering::Relaxed) {
            if let Some(r) = guard.runs.iter_mut().find(|r| r.id == run_id) {
                r.queued = false;
            }
            return SlotAcquire::Cancelled;
        }
        if cap == 0 || guard.running_count < cap as usize {
            guard.running_count += 1;
            if let Some(r) = guard.runs.iter_mut().find(|r| r.id == run_id) {
                r.queued = false;
            }
            return SlotAcquire::Acquired;
        }
        let res = rt.engine.cv
            .wait_timeout(guard, Duration::from_millis(250))
            .ok();
        match res {
            Some((g, _)) => guard = g,
            None         => return SlotAcquire::Cancelled,
        }
    }
}

/// Decrement the running counter and wake the next queued orchestrator.
/// Idempotent against an already-zero counter (defensive — saturating
/// arithmetic so a misuse cannot underflow into a near-`usize::MAX`
/// queue-stall state).
fn release_run_slot(rt: &PipelineRuntime) {
    if let Ok(mut reg) = rt.engine.registry.lock() {
        reg.running_count = reg.running_count.saturating_sub(1);
    }
    rt.engine.cv.notify_one();
}

fn orchestrate(
    def:        PipelineDef,
    run_id:     String,
    repo_path:  Option<String>,
    cancel:     Arc<AtomicBool>,
    rt:         Arc<PipelineRuntime>,
) {
    // ── Acquire the lock. Failure here aborts the run before it starts. ──
    let lock_key = def.effective_lock_key();
    {
        let Ok(mut reg) = rt.engine.registry.lock() else {
            tracing::error!("pipeline mutex poisoned acquiring lock for run {run_id}");
            return;
        };
        if let Err(owner) = reg.try_acquire_lock(&lock_key, &run_id) {
            // Mark this run Failed immediately with a descriptive log entry.
            if let Some(r) = reg.runs.iter_mut().find(|r| r.id == run_id) {
                r.status      = RunStatus::Failed;
                r.started_at  = Some(now_ms());
                r.finished_at = Some(now_ms());
                r.log.push(LogEvent {
                    ts:      now_ms(),
                    level:   LogLevel::Error,
                    scope:   "pipeline".into(),
                    message: format!(
                        "cannot start: lock '{lock_key}' is already held by run '{owner}'"
                    ),
                });
            }
            let snap = reg.get_run(&run_id).cloned();
            drop(reg);
            if let Some(s) = snap {
                persist_run(&s);
                emit(&rt, &s);
            }
            return;
        }
    }

    // ── Wait for a global concurrency slot (config.pipelines.max_concurrent_runs).
    //    The lock_key is already held above, so a queued run still blocks
    //    other runs of the same pipeline from starting in parallel — that
    //    keeps the lock_key collision semantics exactly as documented.
    if let SlotAcquire::Cancelled = acquire_run_slot(&rt, &run_id, &cancel) {
        // The user cancelled while we were parked. Mark Cancelled, release
        // the lock_key, snapshot + emit, and bail without ever transitioning
        // through Running. No slot was reserved → no release_run_slot call.
        {
            let Ok(mut reg) = rt.engine.registry.lock() else { return; };
            if let Some(r) = reg.runs.iter_mut().find(|r| r.id == run_id) {
                r.status      = RunStatus::Cancelled;
                r.queued      = false;
                r.started_at  = r.started_at.or(Some(now_ms()));
                r.finished_at = Some(now_ms());
                r.log.push(LogEvent {
                    ts:      now_ms(),
                    level:   LogLevel::Warn,
                    scope:   "pipeline".into(),
                    message: "cancelled while waiting for a concurrency slot"
                        .into(),
                });
            }
            reg.release_lock_of(&run_id);
        }
        if let Some(snap) = snapshot(&rt.engine.registry, &run_id) {
            persist_run(&snap);
            emit(&rt, &snap);
        }
        return;
    }

    // ── Mark pipeline Running + first snapshot ───────────────────────────
    {
        let Ok(mut reg) = rt.engine.registry.lock() else { return; };
        if let Some(r) = reg.runs.iter_mut().find(|r| r.id == run_id) {
            r.status     = RunStatus::Running;
            if r.started_at.is_none() { r.started_at = Some(now_ms()); }
        }
    }
    let resume_cursor_taken: Option<ResumeCursor> = {
        let Ok(mut reg) = rt.engine.registry.lock() else { return; };
        reg.runs.iter_mut()
            .find(|r| r.id == run_id)
            .and_then(|r| r.resume_cursor.take())
    };
    if let Some(snap) = snapshot(&rt.engine.registry, &run_id) {
        emit(&rt, &snap);
        persist_run(&snap);
        fire_hook(&rt, "on_pipeline_started", &serde_json::json!({
            "run_id":      &run_id,
            "pipeline_id": &snap.pipeline_id,
            "plugin":      &snap.plugin,
        }));
        log_event(&rt, &run_id, LogLevel::Info, "pipeline",
            if resume_cursor_taken.is_some() {
                format!("pipeline '{}' resumed", def.name)
            } else {
                format!("pipeline '{}' started", def.name)
            });
    }

    let mut overall_failed = false;

    // ── Per-run variable context ─────────────────────────────────────────
    // Empty at run start. Steps populate it via `CaptureSpec`; downstream
    // steps + `if_block` conditions read it via `${var}` interpolation.
    // Cleared on every fresh run (resume re-creates a context too — vars
    // captured by previously-Successful steps are NOT re-applied, since
    // we currently don't persist them; in practice that means a resumed
    // run re-runs only Failed/Pending steps and any vars they need must
    // come from those steps themselves).
    let ctx: RunCtx = Arc::new(Mutex::new(RunContext::new()));

    'stages: for (si, stage_def) in def.stages.iter().enumerate() {
        let step_indices = match resumable_step_indices(stage_def, si, &resume_cursor_taken) {
            Some(v) => v,
            None    => continue 'stages, // skip stages already succeeded
        };

        // Nothing to do for this stage? move on.
        if step_indices.is_empty() {
            continue 'stages;
        }

        // Cancel check (pre-stage).
        if cancel.load(Ordering::Relaxed) {
            mark_remaining_cancelled(&rt.engine.registry, &run_id, si);
            break 'stages;
        }

        // Mark stage Running.
        {
            let Ok(mut reg) = rt.engine.registry.lock() else { return; };
            if let Some(r) = reg.runs.iter_mut().find(|r| r.id == run_id) {
                if let Some(s) = r.stages.get_mut(si) { s.status = RunStatus::Running; }
            }
        }
        if let Some(snap) = snapshot(&rt.engine.registry, &run_id) {
            emit(&rt, &snap);
            persist_run(&snap);
        }
        log_event(&rt, &run_id, LogLevel::Info,
            format!("stage:{}", stage_def.id),
            format!("stage '{}' started (mode={:?}, steps={})",
                stage_def.name, stage_def.mode, step_indices.len()));

        // ── Execute the stage's steps in the correct mode ────────────────
        let outcomes = match stage_def.mode {
            StageMode::Sequential => execute_stage_sequential(
                &def, stage_def, si, &step_indices,
                &repo_path, &cancel, &run_id, &rt, &ctx,
            ),
            StageMode::Parallel => execute_stage_parallel(
                &def, stage_def, si, &step_indices,
                &repo_path, &cancel, &run_id, &rt, &ctx,
            ),
        };

        // Merge outcomes into the run. `children` is INTENTIONALLY left
        // untouched — for `if_block` steps it's been mutated incrementally
        // by `execute_if_block` while the children ran, and overwriting it
        // with the (empty) outcome would erase that nested progress.
        let mut had_fatal_failure = false;
        {
            let Ok(mut reg) = rt.engine.registry.lock() else { return; };
            if let Some(r) = reg.runs.iter_mut().find(|r| r.id == run_id) {
                if let Some(s) = r.stages.get_mut(si) {
                    for o in &outcomes {
                        if let Some(st) = s.steps.get_mut(o.step_idx) {
                            st.status      = o.status.clone();
                            st.exit_code   = o.exit_code;
                            st.started_at  = Some(o.started_at);
                            st.finished_at = Some(o.finished_at);
                            st.output      = o.output.clone();
                            st.branch      = o.branch.clone();
                        }
                        // Honor allow_failure when deciding "fatal".
                        let allow = stage_def.steps.get(o.step_idx)
                            .map(|sd| sd.allow_failure)
                            .unwrap_or(false);
                        if o.status == RunStatus::Failed && !allow {
                            had_fatal_failure = true;
                        }
                    }
                }
            }
        }

        // Finalize stage status.
        let stage_status = if cancel.load(Ordering::Relaxed) {
            RunStatus::Cancelled
        } else if had_fatal_failure {
            RunStatus::Failed
        } else {
            RunStatus::Success
        };
        {
            let Ok(mut reg) = rt.engine.registry.lock() else { return; };
            if let Some(r) = reg.runs.iter_mut().find(|r| r.id == run_id) {
                if let Some(s) = r.stages.get_mut(si) { s.status = stage_status.clone(); }
            }
        }
        if let Some(snap) = snapshot(&rt.engine.registry, &run_id) {
            emit(&rt, &snap);
            persist_run(&snap);
        }
        log_event(&rt, &run_id,
            match stage_status {
                RunStatus::Success  => LogLevel::Info,
                RunStatus::Failed   => LogLevel::Error,
                _                   => LogLevel::Warn,
            },
            format!("stage:{}", stage_def.id),
            format!("stage '{}' finished with status={:?}", stage_def.name, stage_status));

        if stage_status == RunStatus::Failed {
            overall_failed = true;
            break 'stages;
        }
    }

    // ── Finalize pipeline ────────────────────────────────────────────────
    let final_status = if cancel.load(Ordering::Relaxed) {
        RunStatus::Cancelled
    } else if overall_failed {
        RunStatus::Failed
    } else {
        RunStatus::Success
    };
    {
        let Ok(mut reg) = rt.engine.registry.lock() else { return; };
        if let Some(r) = reg.runs.iter_mut().find(|r| r.id == run_id) {
            r.status      = final_status.clone();
            r.finished_at = Some(now_ms());
            // Compute the resume cursor from the run's CURRENT step statuses
            // (after all outcome merges) for both Failed and Cancelled. This
            // captures not only the failing step but also subsequent Pending
            // steps in sequential mode + Cancelled steps from
            // `mark_remaining_cancelled`, all of which need to re-run.
            r.resume_cursor = match final_status {
                RunStatus::Failed | RunStatus::Cancelled => compute_resume_cursor(r, &def),
                _ => None,
            };
        }
        // Release the lock regardless of terminal outcome.
        reg.release_lock_of(&run_id);
    }
    // Release the concurrency slot and wake the next queued orchestrator.
    // Done OUTSIDE the registry lock above so the wake-up doesn't race
    // against `wait_timeout` re-acquiring it.
    release_run_slot(&rt);

    if let Some(snap) = snapshot(&rt.engine.registry, &run_id) {
        emit(&rt, &snap);
        persist_run(&snap);
        fire_hook(&rt, "on_pipeline_done", &serde_json::json!({
            "run_id":      &run_id,
            "pipeline_id": &snap.pipeline_id,
            "plugin":      &snap.plugin,
            "status":      &final_status,
        }));
        log_event(&rt, &run_id,
            if final_status == RunStatus::Success { LogLevel::Info } else { LogLevel::Error },
            "pipeline",
            format!("pipeline '{}' finished with status={:?}", def.name, final_status));
    }
}

// ---------------------------------------------------------------------------
// Sequential / parallel stage execution
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn execute_stage_sequential(
    def:        &PipelineDef,
    stage_def:  &StageDef,
    _si:        usize,
    indices:    &[usize],
    repo_path:  &Option<String>,
    cancel:     &Arc<AtomicBool>,
    run_id:     &str,
    rt:         &Arc<PipelineRuntime>,
    ctx:        &RunCtx,
) -> Vec<StepOutcome> {
    let mut outcomes = Vec::with_capacity(indices.len());
    for &step_idx in indices {
        if cancel.load(Ordering::Relaxed) { break; }
        let step_def = &stage_def.steps[step_idx];
        let cwd = step_def.cwd.clone()
            .or_else(|| repo_path.clone())
            .unwrap_or_else(|| ".".to_string());

        // Mark step Running.
        set_step_running(rt, run_id, stage_def.id.as_str(), step_def.id.as_str());
        if let Some(snap) = snapshot(&rt.engine.registry, run_id) { emit(rt, &snap); }
        // Log preview: prefer op name / builtin label / "if-block" when the
        // step isn't a shell command so the debug log carries something
        // meaningful.
        let preview = step_preview(step_def);
        log_event(rt, run_id, LogLevel::Info,
            format!("step:{}.{}", stage_def.id, step_def.id),
            format!("step '{}' started: {}", step_def.name, preview));
        log_event(rt, run_id, LogLevel::Debug,
            format!("step:{}.{}", stage_def.id, step_def.id),
            format!("cwd={cwd}"));

        let outcome = execute_step(
            step_def, &cwd, cancel, step_idx, rt,
            &def.plugin, &def.name, run_id, &stage_def.id, ctx, "",
        );
        emit_step_done(rt, run_id, stage_def, step_def, &outcome);

        let allow_failure = step_def.allow_failure;
        let broke_stage   = outcome.status == RunStatus::Failed && !allow_failure;
        outcomes.push(outcome);
        if broke_stage { break; }
    }
    outcomes
}

#[allow(clippy::too_many_arguments)]
fn execute_stage_parallel(
    def:        &PipelineDef,
    stage_def:  &StageDef,
    _si:        usize,
    indices:    &[usize],
    repo_path:  &Option<String>,
    cancel:     &Arc<AtomicBool>,
    run_id:     &str,
    rt:         &Arc<PipelineRuntime>,
    ctx:        &RunCtx,
) -> Vec<StepOutcome> {
    let total = indices.len();
    let cap = stage_def.max_parallel.unwrap_or(total).max(1);
    let (tx, rx) = mpsc::channel::<StepOutcome>();
    let mut in_flight: Vec<std::thread::JoinHandle<()>> = Vec::new();
    let mut pending = indices.to_vec();
    let mut collected: Vec<StepOutcome> = Vec::with_capacity(total);

    // Mark all steps Running upfront so the UI shows them spinning.
    for &step_idx in indices {
        let step_def = &stage_def.steps[step_idx];
        set_step_running(rt, run_id, stage_def.id.as_str(), step_def.id.as_str());
        let preview = step_preview(step_def);
        log_event(rt, run_id, LogLevel::Info,
            format!("step:{}.{}", stage_def.id, step_def.id),
            format!("step '{}' started (parallel): {}", step_def.name, preview));
    }
    if let Some(snap) = snapshot(&rt.engine.registry, run_id) { emit(rt, &snap); }

    let mut spawned = 0usize;
    // Spawn up to `cap` workers, refill as they finish. Drop our own sender
    // at the end of the block so lingering `rx.recv()` doesn't deadlock even
    // if a worker panics before sending.
    while collected.len() < total {
        // Fill the pool.
        while spawned - collected.len() < cap && !pending.is_empty() {
            let step_idx = pending.remove(0);
            let step_def = stage_def.steps[step_idx].clone();
            let cwd = step_def.cwd.clone()
                .or_else(|| repo_path.clone())
                .unwrap_or_else(|| ".".to_string());
            let cancel_c = cancel.clone();
            let tx_c = tx.clone();
            let rt_c = rt.clone();
            let plugin_c   = def.plugin.clone();
            let pipeline_c = def.name.clone();
            let run_id_c   = run_id.to_string();
            let stage_id_c = stage_def.id.clone();
            let ctx_c      = ctx.clone();
            let h = std::thread::spawn(move || {
                let outcome = execute_step(
                    &step_def, &cwd, &cancel_c, step_idx, &rt_c,
                    &plugin_c, &pipeline_c, &run_id_c, &stage_id_c,
                    &ctx_c, "",
                );
                let _ = tx_c.send(outcome);
            });
            in_flight.push(h);
            spawned += 1;
        }

        match rx.recv() {
            Ok(outcome) => {
                let step_def = &stage_def.steps[outcome.step_idx];
                emit_step_done(rt, run_id, stage_def, step_def, &outcome);
                collected.push(outcome);
            }
            Err(_) => break, // all senders dropped — should only happen on worker panic
        }
    }
    drop(tx);
    for h in in_flight { let _ = h.join(); }
    collected
}

fn set_step_running(
    rt:       &PipelineRuntime,
    run_id:   &str,
    stage_id: &str,
    step_id:  &str,
) {
    let Ok(mut reg) = rt.engine.registry.lock() else { return; };
    if let Some(r) = reg.runs.iter_mut().find(|r| r.id == run_id) {
        if let Some(s) = r.stages.iter_mut().find(|s| s.def_id == stage_id) {
            if let Some(st) = find_step_mut(&mut s.steps, step_id) {
                st.status      = RunStatus::Running;
                st.started_at  = Some(now_ms());
                // Wipe previous-attempt artefacts so a resumed step doesn't
                // show stale output / exit code / finished_at / nested
                // children while the new attempt streams in.
                st.output.clear();
                st.exit_code   = None;
                st.finished_at = None;
                st.children.clear();
                st.branch.clear();
            }
        }
    }
}

fn emit_step_done(
    rt:         &PipelineRuntime,
    run_id:     &str,
    stage_def:  &StageDef,
    step_def:   &StepDef,
    outcome:    &StepOutcome,
) {
    let scope = format!("step:{}.{}", stage_def.id, step_def.id);
    let level = match outcome.status {
        RunStatus::Success   => LogLevel::Info,
        RunStatus::Failed    => if step_def.allow_failure { LogLevel::Warn } else { LogLevel::Error },
        RunStatus::Cancelled => LogLevel::Warn,
        _                    => LogLevel::Info,
    };
    log_event(rt, run_id, level, scope,
        format!("step '{}' finished: {:?} (exit={:?}, elapsed={}ms)",
            step_def.name,
            outcome.status,
            outcome.exit_code,
            outcome.finished_at - outcome.started_at));

    // Captured stdout/stderr lines are NOT replayed here — they were already
    // streamed live through `StepLogSink` while the step was running, both
    // into the global Plugin Logs panel (`arbor://plugin-log`) and into the
    // run's own log (`arbor://pipeline-log` + `run.log`).

    // Push the step update into the run and snapshot+emit. Children are
    // mutated separately by `execute_if_block` while the if-block runs, so
    // we deliberately do NOT touch `st.children` here — overwriting it
    // would erase the nested progress the UI already received.
    {
        let Ok(mut reg) = rt.engine.registry.lock() else { return; };
        if let Some(r) = reg.runs.iter_mut().find(|r| r.id == run_id) {
            if let Some(s) = r.stages.iter_mut().find(|s| s.def_id == stage_def.id) {
                if let Some(st) = find_step_mut(&mut s.steps, &step_def.id) {
                    st.status      = outcome.status.clone();
                    st.exit_code   = outcome.exit_code;
                    st.started_at  = Some(outcome.started_at);
                    st.finished_at = Some(outcome.finished_at);
                    st.output      = outcome.output.clone();
                    st.branch      = outcome.branch.clone();
                }
            }
        }
    }
    if let Some(snap) = snapshot(&rt.engine.registry, run_id) {
        emit(rt, &snap);
        persist_run(&snap);
        fire_hook(rt, "on_pipeline_step_done", &serde_json::json!({
            "run_id":    run_id,
            "plugin":    &snap.plugin,
            "stage_id":  &stage_def.id,
            "step_id":   &step_def.id,
            "step_name": &step_def.name,
            "status":    &outcome.status,
            "exit_code": outcome.exit_code,
        }));
    }
}

fn mark_remaining_cancelled(
    pipelines: &Mutex<PipelineRegistry>,
    run_id:    &str,
    from_si:   usize,
) {
    fn cascade(steps: &mut [StepRun]) {
        for step in steps.iter_mut() {
            if matches!(step.status, RunStatus::Pending | RunStatus::Running) {
                step.status = RunStatus::Cancelled;
            }
            cascade(&mut step.children);
        }
    }
    let Ok(mut reg) = pipelines.lock() else { return; };
    if let Some(r) = reg.runs.iter_mut().find(|r| r.id == run_id) {
        for (si, stage) in r.stages.iter_mut().enumerate() {
            if si < from_si { continue; }
            stage.status = RunStatus::Cancelled;
            cascade(&mut stage.steps);
        }
    }
}

// ===========================================================================
// Resume / discard entry points (called from Tauri commands)
// ===========================================================================

/// Resume a terminal-but-incomplete run by re-executing its non-Success
/// steps (and any later stages that never ran).
/// Returns an error when:
/// - the run does not exist
/// - the run is not in a resumable state (only `Failed`, `Cancelled`, or
///   `Paused` qualify — `Success` has nothing to resume)
/// - the lock is held by a different run (another run is active)
pub fn resume_run(
    run_id:     &str,
    rt:         Arc<PipelineRuntime>,
) -> std::result::Result<(), String> {
    // Validate + clone the data we need outside the mutex.
    let (def, repo_path) = {
        let mut reg = rt.engine.registry.lock()
            .map_err(|_| "pipeline mutex poisoned".to_string())?;
        let run = reg.runs.iter().find(|r| r.id == run_id)
            .ok_or_else(|| format!("run '{run_id}' not found"))?;
        match run.status {
            RunStatus::Failed | RunStatus::Paused | RunStatus::Cancelled => {},
            ref s => return Err(format!("run '{run_id}' is not resumable (status={s:?})")),
        }
        if run.resume_cursor.is_none() {
            return Err(format!("run '{run_id}' has no resume cursor"));
        }
        let def = reg.defs.iter()
            .find(|d| d.plugin == run.plugin && d.id == run.pipeline_id)
            .cloned()
            .ok_or_else(|| format!(
                "pipeline definition '{}:{}' not found (plugin unloaded?)",
                run.plugin, run.pipeline_id))?;
        // Verify the lock is free (or already ours).
        let lock_key = run.lock_key.clone();
        if let Some(owner) = reg.locked_by(&lock_key) {
            if owner != run_id {
                return Err(format!(
                    "cannot resume: lock '{lock_key}' is held by run '{owner}'"));
            }
        }
        let repo_path = run.repo_path.clone();
        // Refresh state to Pending so the orchestrator's Running transition
        // fires properly. Keep the resume_cursor intact — the orchestrator
        // will consume it.
        if let Some(r) = reg.runs.iter_mut().find(|r| r.id == run_id) {
            r.status      = RunStatus::Pending;
            r.finished_at = None;
        }
        (def, repo_path)
    };

    // Fresh cancel token.
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut reg = rt.engine.registry.lock()
            .map_err(|_| "pipeline mutex poisoned".to_string())?;
        reg.cancel_tokens.insert(run_id.to_string(), cancel.clone());
    }

    start_pipeline_run(def, run_id.to_string(), repo_path, cancel, rt);
    Ok(())
}

/// Drop a failed/cancelled run — removes the on-disk file and the in-memory
/// entry. Refuses to discard a run that is currently Running.
pub fn discard_run(run_id: &str, rt: Arc<PipelineRuntime>) -> std::result::Result<(), String> {
    let mut reg = rt.engine.registry.lock()
        .map_err(|_| "pipeline mutex poisoned".to_string())?;
    let status = reg.runs.iter().find(|r| r.id == run_id).map(|r| r.status.clone());
    match status {
        None => Err(format!("run '{run_id}' not found")),
        Some(RunStatus::Running) => Err(format!("cannot discard a Running run — cancel it first")),
        Some(_) => {
            reg.discard(run_id);
            drop(reg);
            rt.sink.emit("arbor://pipeline-discarded",
                serde_json::json!({ "run_id": run_id }));
            Ok(())
        }
    }
}

// ===========================================================================
// Command execution (unchanged behavior)
// ===========================================================================

fn run_command(
    command: &str,
    cwd: &str,
    env: &std::collections::HashMap<String, String>,
    cancel: &Arc<AtomicBool>,
    sink: &StepLogSink,
) -> (Option<i32>, Vec<String>) {
    // On Windows, `Command::arg` / `args` auto-quotes arguments that contain
    // spaces or quotes — which mangles shell command strings like
    //   `git clone --progress -- "C:\path" "C:\target"`
    // because the inner quotes get doubled and cmd.exe receives them as
    // literal characters (we saw `fatal: could not create '"C:\…"'`).
    // `raw_arg` bypasses the quoting and passes bytes as-is to CreateProcessW,
    // which is exactly what we want for shell command strings.
    // stdin=null prevents the child from inheriting Arbor's stdin pipe; no
    // user-driven program sends data through there, and keeping it closed
    // means fewer handles shared across processes (smaller chance that an
    // open file in Arbor keeps a file "in use" for the child's siblings).
    #[cfg(windows)]
    let spawn_result = {
        use std::os::windows::process::CommandExt;
        let mut c = std::process::Command::new("cmd");
        c.raw_arg("/C").raw_arg(command)
            .current_dir(cwd)
            .no_window()
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        for (k, v) in env { c.env(k, v); }
        c.spawn()
    };
    #[cfg(not(windows))]
    let spawn_result = {
        let mut c = std::process::Command::new("sh");
        c.args(["-c", command])
            .current_dir(cwd)
            .no_window()
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        for (k, v) in env { c.env(k, v); }
        c.spawn()
    };

    let mut child = match spawn_result {
        Ok(c)  => c,
        Err(e) => {
            let msg = format!("⚠ failed to spawn: {e}");
            sink.emit(&msg);
            return (Some(1), vec![msg]);
        }
    };

    let pid = child.id();

    // Captured-output buffer shared between the stdout reader (this thread)
    // and the stderr reader (a worker thread) so both pipes' lines land in
    // a single chronologically-ordered Vec — matching the order in which
    // they were streamed live through the sink.
    let output: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    // Cancel watcher — without this, clicking "Stop" on a long-running step
    // (e.g. `mvn clean package`) would only take effect at the NEXT step
    // boundary, because run_command happily blocks on stdout for as long as
    // the child wants to talk. The watcher polls the cancel flag at 100ms
    // and forcibly terminates the entire process tree (`/T` on Windows,
    // SIGTERM on Unix) so Maven's spawned `java` child also dies — killing
    // just the cmd.exe wrapper would leave the build running. Exits as soon
    // as `done` is set by the main thread after `child.wait()`.
    let done = Arc::new(AtomicBool::new(false));
    let killer = {
        let cancel_c = cancel.clone();
        let done_c   = done.clone();
        let sink_c   = sink.clone();
        std::thread::spawn(move || {
            let mut announced = false;
            while !done_c.load(Ordering::Relaxed) {
                if cancel_c.load(Ordering::Relaxed) {
                    if !announced {
                        sink_c.emit("[cancel requested — terminating process tree]");
                        announced = true;
                    }
                    crate::jobs::kill_process(pid);
                    // Re-issue every tick: on Windows taskkill races with
                    // child startup; one shot can miss. Stops as soon as
                    // `done` flips after wait() returns.
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        })
    };

    // ── Chunk-based pipe reader ──────────────────────────────────────────
    //
    // Mirrors the integrated terminal's read loop: pull whatever bytes the
    // OS hands us in 4 KB chunks (`reader.read(buf)`), split them into
    // `\n`-terminated lines on the reader thread via [`split_chunk_lines`],
    // then call [`StepLogSink::emit_batch`] once per chunk with the lines
    // we found.  Earlier versions used `BufReader::lines() + take(500)`
    // which closed the read end of the pipe mid-build (silently dropping
    // output and forcing the child into BROKEN_PIPE handling) and emitted
    // one Tauri event per line — typically a couple of thousand events
    // for a `mvn clean package`.  The chunk model does ~5–20 events for
    // the same build and never closes the pipe early.
    //
    // `MAX_CAPTURED_LINES_PER_PIPE` caps the captured buffer so a runaway
    // step does not eat memory.  We KEEP READING the pipe past the cap —
    // just stop appending — so the child never blocks on a full pipe.
    const READ_BUF_SIZE: usize = 4096;
    const MAX_CAPTURED_LINES_PER_PIPE: usize = 5_000;

    fn drain_pipe<R: std::io::Read>(
        mut reader: R,
        sink: &StepLogSink,
        output: &Arc<Mutex<Vec<String>>>,
        stderr_prefix: bool,
    ) {
        let mut buf       = [0u8; READ_BUF_SIZE];
        let mut leftover  = Vec::<u8>::with_capacity(READ_BUF_SIZE);
        let mut captured  = 0usize;
        loop {
            let n = match reader.read(&mut buf) {
                Ok(0)  => 0,
                Ok(n)  => n,
                Err(_) => 0,
            };
            if n == 0 {
                if let Some(tail) = drain_partial_line(&mut leftover) {
                    let line = if stderr_prefix { format!("[stderr] {tail}") } else { tail };
                    if captured < MAX_CAPTURED_LINES_PER_PIPE {
                        sink.record_line(&line);
                        if let Ok(mut v) = output.lock() { v.push(line.clone()); }
                        sink.emit_batch(std::slice::from_ref(&line));
                    }
                }
                break;
            }
            let mut chunk_lines = split_chunk_lines(&mut leftover, &buf[..n]);
            if stderr_prefix {
                for l in chunk_lines.iter_mut() {
                    *l = format!("[stderr] {l}");
                }
            }
            if chunk_lines.is_empty() { continue; }

            // Apply the per-pipe capture cap. Past the cap we keep reading
            // (to drain the pipe so the child doesn't block on WriteFile)
            // but stop emitting / appending.
            let to_emit: &[String] = if captured >= MAX_CAPTURED_LINES_PER_PIPE {
                &[]
            } else {
                let take_n = chunk_lines.len()
                    .min(MAX_CAPTURED_LINES_PER_PIPE - captured);
                captured += take_n;
                &chunk_lines[..take_n]
            };

            if !to_emit.is_empty() {
                for line in to_emit { sink.record_line(line); }
                if let Ok(mut v) = output.lock() {
                    v.extend(to_emit.iter().cloned());
                }
                sink.emit_batch(to_emit);
            }
        }
    }

    let stderr_handle = child.stderr.take().map(|se| {
        let sink_c   = sink.clone();
        let output_c = output.clone();
        std::thread::spawn(move || {
            drain_pipe(se, &sink_c, &output_c, /* stderr_prefix = */ true);
        })
    });

    if let Some(so) = child.stdout.take() {
        drain_pipe(so, sink, &output, /* stderr_prefix = */ false);
    }

    if let Some(h) = stderr_handle { let _ = h.join(); }

    let exit_code = child.wait().ok().and_then(|s| s.code());
    done.store(true, Ordering::Relaxed);
    let _ = killer.join();
    let final_output = output.lock().map(|v| v.clone()).unwrap_or_default();
    (exit_code, final_output)
}

// ===========================================================================
// LuaOp execution — dispatch to a plugin-registered Lua handler
// ===========================================================================
//
// Return shape mirrors `run_command` so the caller can treat both uniformly:
//   · exit_code = None or Some(non-zero) → Failed
//   · exit_code = Some(0)                → Success
//   · output                            → lines captured (logged per step)
//
// Resolution order for the target plugin name:
//   1. `op.plugin` explicit override
//   2. `default_plugin` (the pipeline's `plugin` field — the common case)
//
// Handler contract (Lua side):
//   local handler = function(params, ctx)
//     -- `params` is the JSON payload the step def carried.
//     -- `ctx` has `{ cwd = "...", plugin = "..." }` for convenience.
//     return { exit_code = 0, stdout = "…", stderr = "…" }
//   end
//
// `stdout` / `stderr` are split on newlines and appended to the step output
// (stderr lines prefixed with `[stderr]` to match shell runs). If the handler
// raises, the step is marked Failed with the error message captured.
fn run_lua_op(
    op:             &LuaOpSpec,
    cwd:            &str,
    rt:             &PipelineRuntime,
    default_plugin: &str,
    sink:           &StepLogSink,
) -> (Option<i32>, Vec<String>) {
    let target_plugin = op.plugin.clone().unwrap_or_else(|| default_plugin.to_string());
    let host = match rt.plugin_host.lock() {
        Ok(h) => h,
        Err(_) => {
            let msg = "⚠ plugin host mutex poisoned".to_string();
            sink.emit(&msg);
            return (Some(1), vec![msg]);
        }
    };
    // Lua handlers return stdout/stderr as opaque blobs — the live stream is
    // therefore "burst" rather than truly per-line, but feeding each parsed
    // line through `sink` keeps the UX uniform with shell steps and ensures
    // every line lands in plugin-log + run.log as it does for `run_command`.
    match host.invoke_pipeline_op(&target_plugin, &op.op, &op.params, cwd) {
        Ok(result) => {
            let mut lines = Vec::new();
            if !result.stdout.is_empty() {
                for l in result.stdout.lines().take(500) {
                    sink.emit(l);
                    lines.push(l.to_string());
                }
            }
            if !result.stderr.is_empty() {
                for l in result.stderr.lines().take(500) {
                    let line = format!("[stderr] {l}");
                    sink.emit(&line);
                    lines.push(line);
                }
            }
            (Some(result.exit_code), lines)
        }
        Err(e) => {
            let msg = format!("⚠ lua_op '{}.{}' error: {e}", target_plugin, op.op);
            sink.emit(&msg);
            (Some(1), vec![msg])
        }
    }
}
