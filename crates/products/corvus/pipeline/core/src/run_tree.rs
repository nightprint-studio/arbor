//! Pure orchestration helpers — step-tree navigation, resume planning,
//! output chunking, and log-level inference.
//!
//! All host-free: the live orchestrator (which owns the registry mutex, the
//! `AppHandle`, and the spawned processes) calls these to decide *what* to do;
//! they never emit, lock, or spawn anything themselves.

use corvus_pipeline_api::prelude::{
    describe, PipelineDef, PipelineRun, ResumeCursor, RunStatus, StageDef, StepDef, StepRun,
};

/// Locate a `StepRun` anywhere inside a stage's step tree (top-level or
/// nested under any `if_block`). Used by the host's `set_step_running`,
/// `emit_step_done`, and the live-streaming sink so they all work uniformly
/// regardless of nesting depth.
pub fn find_step_mut<'a>(steps: &'a mut [StepRun], target_id: &str) -> Option<&'a mut StepRun> {
    for s in steps.iter_mut() {
        if s.def_id == target_id { return Some(s); }
        if let Some(found) = find_step_mut(&mut s.children, target_id) {
            return Some(found);
        }
    }
    None
}

/// Index of steps within `stage` that actually need to run, given the run's
/// optional resume cursor. Returns `None` when the entire stage should be
/// skipped (only possible when the cursor points beyond this stage).
pub fn resumable_step_indices(
    stage: &StageDef,
    stage_idx: usize,
    cursor: &Option<ResumeCursor>,
) -> Option<Vec<usize>> {
    match cursor {
        None => Some((0..stage.steps.len()).collect()),
        Some(c) if c.stage_idx > stage_idx => None, // earlier stage, already succeeded
        Some(c) if c.stage_idx < stage_idx => Some((0..stage.steps.len()).collect()),
        Some(c) => {
            // Same stage as the cursor: execute only the listed step_ids.
            let wanted: std::collections::HashSet<&str> =
                c.step_ids.iter().map(|s| s.as_str()).collect();
            Some(stage.steps.iter().enumerate()
                .filter_map(|(i, s)| if wanted.contains(s.id.as_str()) { Some(i) } else { None })
                .collect())
        }
    }
}

/// Build a `ResumeCursor` for a terminal-but-incomplete run by walking its
/// stages in order and picking the first one that contains any step which
/// did NOT finish in `Success`. The cursor's `step_ids` lists every such
/// step in that stage so resume re-executes:
///   · the failing step,
///   · steps that came after it in sequential mode and never ran (Pending),
///   · steps cancelled by the host's `mark_remaining_cancelled` (Cancelled),
///   · cancelled steps from a parallel stage.
/// Steps explicitly marked `allow_failure = true` that ended in `Failed` are
/// excluded — the original run already accepted them as non-fatal.
/// Returns `None` when every step succeeded (nothing to resume).
pub fn compute_resume_cursor(run: &PipelineRun, def: &PipelineDef) -> Option<ResumeCursor> {
    for (si, stage) in run.stages.iter().enumerate() {
        let stage_def = def.stages.iter().find(|sd| sd.id == stage.def_id);
        let pending: Vec<String> = stage.steps.iter()
            .filter(|st| match st.status {
                RunStatus::Success => false,
                RunStatus::Failed  => !stage_def
                    .and_then(|sd| sd.steps.iter().find(|s| s.id == st.def_id))
                    .map(|s| s.allow_failure)
                    .unwrap_or(false),
                _ => true, // Pending / Running / Cancelled / Paused → re-run
            })
            .map(|st| st.def_id.clone())
            .collect();
        if !pending.is_empty() {
            return Some(ResumeCursor { stage_idx: si, step_ids: pending });
        }
    }
    None
}

/// Short preview of a step's "what does it do" used in run logs and in the
/// "step started" lines. Prefers the most specific kind: if_block > builtin >
/// lua_op > shell command.
pub fn step_preview(step: &StepDef) -> String {
    if step.if_block.is_some() { return "if-block".to_string(); }
    if let Some(b) = &step.builtin { return describe(b); }
    if let Some(op) = &step.lua_op { return format!("lua_op {}", op.op); }
    step.command.clone()
}

/// Heuristic level for a captured step-output line. Conservative — anything
/// we don't recognise stays at info, since that's the safe default for
/// arbitrary shell stdout. Mirrored on the frontend in
/// `src/lib/utils/log-highlight.ts::inferLogLevel` — keep both in sync.
///
/// `[stderr]` is NOT treated as an error signal: git/cargo/npm and most CLI
/// tools write progress and informational output to stderr by convention
/// ("Cloning into …", "Compiling foo v0.1", "Receiving objects: 42%"), so
/// flagging every stderr line as error floods the global log panel with
/// false positives. We strip the prefix and inspect the actual message
/// instead, escalating only when the body looks like a real diagnostic.
pub fn infer_step_log_level(line: &str) -> &'static str {
    let trimmed = line.trim_start();
    let body = trimmed.strip_prefix("[stderr]").unwrap_or(trimmed).trim_start();
    if body.starts_with('⚠')
        || body.starts_with("FAIL")
        || body.starts_with("error")
        || body.starts_with("ERROR")
        || body.starts_with("Error")
        || body.starts_with("fatal:")
        || body.starts_with("Fatal")
        || body.starts_with("panic")
    {
        "error"
    } else if body.starts_with("WARN")
        || body.starts_with("WARNING")
        || body.starts_with("warning:")
        || body.starts_with("Warning")
    {
        "warn"
    } else if body.starts_with("DEBUG") {
        "debug"
    } else {
        "info"
    }
}

/// Drain `\n`-terminated lines out of an accumulating byte buffer.
///
/// `leftover` is the per-pipe state owned by the reader thread:
///   · on entry it holds bytes received in previous chunks that did not
///     yet contain a complete line;
///   · `new_data` is appended to it;
///   · every `\n` produces a `String` line (with trailing `\r` stripped
///     to handle CRLF cleanly), which is removed from the front of
///     `leftover`;
///   · bytes after the last `\n` stay in `leftover` for the next call.
///
/// At pipe EOF the caller is expected to call [`drain_partial_line`] to
/// emit any remaining tail that never received a `\n`.
pub fn split_chunk_lines(leftover: &mut Vec<u8>, new_data: &[u8]) -> Vec<String> {
    leftover.extend_from_slice(new_data);
    let mut lines = Vec::new();
    let mut consumed = 0usize;
    let mut i = consumed;
    while i < leftover.len() {
        if leftover[i] == b'\n' {
            let mut end = i;
            if end > consumed && leftover[end - 1] == b'\r' { end -= 1; }
            let bytes = &leftover[consumed..end];
            lines.push(String::from_utf8_lossy(bytes).into_owned());
            consumed = i + 1;
        }
        i += 1;
    }
    if consumed > 0 { leftover.drain(0..consumed); }
    lines
}

/// Flush any trailing bytes in `leftover` as a final partial line.
/// Call once at pipe EOF — the child may have written a non-newline-
/// terminated tail (e.g. progress dot, ANSI sequence with no `\n`)
/// which would otherwise be lost.
pub fn drain_partial_line(leftover: &mut Vec<u8>) -> Option<String> {
    if leftover.is_empty() { return None; }
    let s = String::from_utf8_lossy(leftover).into_owned();
    leftover.clear();
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use corvus_pipeline_api::prelude::{LogLevel, StageMode, StageRun};

    #[test]
    fn split_handles_crlf_and_keeps_leftover() {
        let mut buf = Vec::new();
        let lines = split_chunk_lines(&mut buf, b"a\r\nb\npartial");
        assert_eq!(lines, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(buf, b"partial");
        // Next chunk completes the partial line.
        let lines = split_chunk_lines(&mut buf, b" done\n");
        assert_eq!(lines, vec!["partial done".to_string()]);
        assert!(buf.is_empty());
    }

    #[test]
    fn drain_partial_emits_then_clears() {
        let mut buf = b"tail".to_vec();
        assert_eq!(drain_partial_line(&mut buf), Some("tail".to_string()));
        assert!(buf.is_empty());
        assert_eq!(drain_partial_line(&mut buf), None);
    }

    #[test]
    fn infer_level_strips_stderr_prefix_before_classifying() {
        assert_eq!(infer_step_log_level("error: boom"), "error");
        assert_eq!(infer_step_log_level("[stderr] fatal: nope"), "error");
        assert_eq!(infer_step_log_level("[stderr] Compiling foo v0.1"), "info");
        assert_eq!(infer_step_log_level("warning: deprecated"), "warn");
        assert_eq!(infer_step_log_level("DEBUG details"), "debug");
        assert_eq!(infer_step_log_level("just output"), "info");
    }

    fn def_with_allow_failure(allow_b: bool) -> PipelineDef {
        PipelineDef {
            id: "p".into(), name: "P".into(), plugin: "x".into(),
            description: None, icon: None, lock_key: None,
            log_level: LogLevel::Info, silent: false,
            stages: vec![StageDef {
                id: "s1".into(), name: "S1".into(), mode: StageMode::Sequential,
                max_parallel: None,
                steps: vec![
                    StepDef { id: "a".into(), name: "A".into(), command: "x".into(),
                        lua_op: None, builtin: None, if_block: None, cwd: None,
                        allow_failure: false, env: Default::default(), capture: None },
                    StepDef { id: "b".into(), name: "B".into(), command: "x".into(),
                        lua_op: None, builtin: None, if_block: None, cwd: None,
                        allow_failure: allow_b, env: Default::default(), capture: None },
                ],
            }],
        }
    }

    fn run_with_statuses(a: RunStatus, b: RunStatus) -> PipelineRun {
        PipelineRun {
            id: "r".into(), pipeline_id: "p".into(), plugin: "x".into(), name: "P".into(),
            status: RunStatus::Failed, started_at: Some(0), finished_at: Some(1),
            lock_key: "x:p".into(), log_level: LogLevel::Info, log: Vec::new(),
            resume_cursor: None, repo_path: None, silent: false, queued: false,
            stages: vec![StageRun {
                def_id: "s1".into(), name: "S1".into(), status: RunStatus::Failed,
                steps: vec![
                    StepRun { def_id: "a".into(), name: "A".into(), status: a,
                        output: Vec::new(), started_at: None, finished_at: None,
                        exit_code: None, children: Vec::new(), branch: String::new() },
                    StepRun { def_id: "b".into(), name: "B".into(), status: b,
                        output: Vec::new(), started_at: None, finished_at: None,
                        exit_code: None, children: Vec::new(), branch: String::new() },
                ],
            }],
        }
    }

    #[test]
    fn resume_cursor_lists_non_success_steps() {
        let def = def_with_allow_failure(false);
        let run = run_with_statuses(RunStatus::Success, RunStatus::Failed);
        let cursor = compute_resume_cursor(&run, &def).unwrap();
        assert_eq!(cursor.stage_idx, 0);
        assert_eq!(cursor.step_ids, vec!["b".to_string()]);
    }

    #[test]
    fn resume_cursor_skips_allow_failure_steps() {
        let def = def_with_allow_failure(true); // b allows failure
        let run = run_with_statuses(RunStatus::Success, RunStatus::Failed);
        // b failed but is allow_failure → not resumable → whole run is done.
        assert!(compute_resume_cursor(&run, &def).is_none());
    }

    #[test]
    fn resumable_indices_honor_cursor_position() {
        let def = def_with_allow_failure(false);
        let stage = &def.stages[0];
        // No cursor → all steps.
        assert_eq!(resumable_step_indices(stage, 0, &None), Some(vec![0, 1]));
        // Cursor on a later stage → skip this one.
        let later = Some(ResumeCursor { stage_idx: 1, step_ids: vec![] });
        assert_eq!(resumable_step_indices(stage, 0, &later), None);
        // Cursor on this stage → only the listed ids.
        let here = Some(ResumeCursor { stage_idx: 0, step_ids: vec!["b".into()] });
        assert_eq!(resumable_step_indices(stage, 0, &here), Some(vec![1]));
    }

    #[test]
    fn find_step_mut_descends_into_children() {
        let mut steps = vec![StepRun {
            def_id: "parent".into(), name: "P".into(), status: RunStatus::Running,
            output: Vec::new(), started_at: None, finished_at: None, exit_code: None,
            branch: String::new(),
            children: vec![StepRun {
                def_id: "parent/child".into(), name: "C".into(), status: RunStatus::Pending,
                output: Vec::new(), started_at: None, finished_at: None, exit_code: None,
                children: Vec::new(), branch: String::new(),
            }],
        }];
        let found = find_step_mut(&mut steps, "parent/child").unwrap();
        assert_eq!(found.name, "C");
    }

    #[test]
    fn step_preview_prefers_most_specific_kind() {
        let mut step = StepDef {
            id: "s".into(), name: "S".into(), command: "echo hi".into(),
            lua_op: None, builtin: None, if_block: None, cwd: None,
            allow_failure: false, env: Default::default(), capture: None,
        };
        assert_eq!(step_preview(&step), "echo hi");
        step.lua_op = Some(corvus_pipeline_api::prelude::LuaOpSpec {
            plugin: None, op: "build".into(), params: serde_json::Value::Null,
        });
        assert_eq!(step_preview(&step), "lua_op build");
    }
}
