//! Changing several things in one call.
//!
//! One exact edit per request is the right unit for a careful change and the wrong one for a mechanical
//! one: ten call sites cost ten round trips, each carrying the whole conversation again. These two tools
//! take the many-at-once shape — a list of edits, or a pattern over a tree — and keep everything that
//! makes the single edit safe: the text must be there, a change is read back through Bennu (encoding
//! and stamp), and it arrives as a proposal or is applied by the user's mode and recorded in Local
//! History, like any other.

use std::path::PathBuf;

use bennu_assistant::prelude::ToolSpec;
use serde_json::{json, Value};

use super::{spec, ToolHost};
use crate::assistant::text_tools::{self, ReplaceArgs};

pub(super) fn specs() -> Vec<ToolSpec> {
    vec![
        spec(
            "edit_files",
            "Make several exact edits, in one or more files, as one change. Each edit is `{file, old_text, new_text, expected_count?}` and follows the rule of edit_file: the text must appear exactly once unless `expected_count` says how many. Edits to the same file apply in order, each on the result of the one before. All or nothing — if one cannot be applied, nothing is changed and the reply names it. Use this instead of many edit_file calls: it is one request, not one per edit.",
            json!({"type":"object","properties":{
                "edits":{"type":"array","minItems":1,"items":{"type":"object","properties":{
                    "file":{"type":"string","description":"Absolute path, or relative to the project."},
                    "old_text":{"type":"string"},
                    "new_text":{"type":"string"},
                    "expected_count":{"type":"integer","description":"How many occurrences to replace. Default 1."}
                },"required":["file","old_text","new_text"]}}
            },"required":["edits"]}),
        ),
        spec(
            "replace_in_files",
            "Replace text — or a regular expression — in every file under a folder that contains it. `path` and `extensions` narrow it; build output and version-control folders are skipped. Run it first with `dry_run: true` to see which files would change and how many times, then for real. Refuses past `max_files` (default 100): a pattern that matches that much is more likely a mistake than a refactoring. For renaming a Java symbol use rename_symbol, which understands scope; this is for text.",
            json!({"type":"object","properties":{
                "pattern":{"type":"string","description":"The text to replace, or a regular expression with `regex`."},
                "replacement":{"type":"string","description":"With `regex`, `$1` and `${name}` stand for the pattern's groups."},
                "path":{"type":"string","description":"Folder (or file) to work in; defaults to the project root."},
                "regex":{"type":"boolean"},
                "ignore_case":{"type":"boolean"},
                "extensions":{"type":"array","items":{"type":"string"},"description":"Only files with these extensions, without the dot."},
                "dry_run":{"type":"boolean","description":"Only say what would change."},
                "max_files":{"type":"integer"}
            },"required":["pattern","replacement"]}),
        ),
    ]
}

/// One file's text as the list of edits has left it so far.
struct Draft {
    file: PathBuf,
    original: String,
    stamp: String,
    text: String,
}

impl ToolHost {
    /// The files a list of edits names, for the access policy.
    pub(super) fn edit_files_touched(&self, input: &Value) -> Vec<PathBuf> {
        input
            .get("edits")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|e| e.get("file").and_then(Value::as_str))
            .map(|f| self.resolve(f))
            .collect()
    }

    pub(super) fn run_edit_files(&self, input: &Value) -> Result<String, String> {
        let list = input.get("edits").and_then(Value::as_array).filter(|l| !l.is_empty()).ok_or("`edits` must be a non-empty list")?;
        let mut drafts: Vec<Draft> = Vec::new();
        for (i, edit) in list.iter().enumerate() {
            let n = i + 1;
            let field = |k: &str| edit.get(k).and_then(Value::as_str).ok_or_else(|| format!("edit {n}: `{k}` is required"));
            let file = self.resolve(field("file")?);
            let (old_text, new_text) = (field("old_text")?, field("new_text")?);
            let expected = edit.get("expected_count").and_then(Value::as_u64).map(|c| c as usize);
            let at = match drafts.iter().position(|d| d.file == file) {
                Some(at) => at,
                None => {
                    let (text, stamp) = self.read_text(&file).map_err(|m| format!("edit {n}: {m}. Nothing was changed."))?;
                    drafts.push(Draft { file: file.clone(), original: text.clone(), stamp, text });
                    drafts.len() - 1
                }
            };
            drafts[at].text = text_tools::replace(&drafts[at].text, old_text, new_text, expected)
                .map_err(|m| format!("edit {n} ({}): {m}. Nothing was changed.", self.display_path(&file.display().to_string())))?;
        }
        let mut report = Vec::new();
        for d in drafts {
            match self.submit(&d.file, &d.original, d.text, Some(d.stamp), None) {
                Ok(m) => report.push(m),
                Err(m) => report.push(format!("{}: {m}", self.display_path(&d.file.display().to_string()))),
            }
        }
        Ok(report.join("\n"))
    }

    pub(super) fn run_replace_in_files(&self, input: &Value) -> Result<String, String> {
        let args: ReplaceArgs = serde_json::from_value(input.clone()).map_err(|e| format!("bad arguments: {e}"))?;
        let re = text_tools::matcher(&args)?;
        let base = args.path.as_deref().map(|p| self.resolve(p)).unwrap_or_else(|| self.project().to_path_buf());
        let files = text_tools::files_matching(&base, &re, &args.extensions);
        if files.is_empty() {
            return Ok("No file contains the pattern; nothing to replace.".into());
        }
        let limit = args.max_files.unwrap_or(100);
        if files.len() > limit {
            return Err(format!(
                "{} files match, more than the {limit} allowed — narrow `path` or `extensions`, or raise `max_files` if that really is the intent.",
                files.len()
            ));
        }

        let mut report = Vec::new();
        let mut total = 0usize;
        for file in &files {
            let shown = self.display_path(&file.display().to_string());
            let (text, stamp) = match self.read_text(file) {
                Ok(read) => read,
                Err(e) => {
                    report.push(format!("{shown}: skipped ({e})"));
                    continue;
                }
            };
            let (next, count) = text_tools::replace_matches(&re, &text, &args.replacement, args.regex);
            if count == 0 {
                continue;
            }
            total += count;
            if args.dry_run {
                report.push(format!("{shown} ({count})"));
                continue;
            }
            match self.submit(file, &text, next, Some(stamp), None) {
                Ok(m) => report.push(m),
                Err(m) => report.push(format!("{shown}: {m}")),
            }
        }
        let head = if args.dry_run {
            format!("Dry run — {total} replacements in {} files, nothing changed:", report.len())
        } else {
            format!("{total} replacements in {} files:", files.len())
        };
        Ok(format!("{head}\n{}", report.join("\n")))
    }
}
