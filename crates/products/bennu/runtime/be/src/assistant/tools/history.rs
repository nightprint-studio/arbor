//! Local History as a tool: what a file used to say, what was deleted, and putting it back.
//!
//! Bennu records every save and every change the assistant makes, so "undo what you just did" and "bring
//! back the file I deleted" have an answer that does not depend on git. Reading is plain. **Restoring is
//! an edit**: the old text goes through the same door as any other change — proposed for review or applied
//! by the user's mode, and recorded again — rather than a direct write of bytes the user never saw.

use std::path::PathBuf;

use bennu_assistant::prelude::ToolSpec;
use serde_json::{json, Value};

use super::{clip, spec, ToolHost};

pub(super) fn specs() -> Vec<ToolSpec> {
    vec![spec(
        "local_history",
        "Bennu's Local History records every save of a file and every change you make. `list` shows a file's revisions, newest first; `deleted` lists the files that were deleted or moved away; `diff` shows what changed between a revision and the file now; `restore` puts a revision back (the newest one when `revision` is left out — which is how a deleted file comes back). A restore is an edit like any other: proposed or applied by the user's mode, so say what you restored and why.",
        json!({"type":"object","properties":{
            "action":{"type":"string","enum":["list","deleted","diff","restore"]},
            "file":{"type":"string","description":"Absolute path, or relative to the project. Not needed for `deleted`."},
            "project":{"type":"string","description":"For `deleted` with several projects open: the project to look in."},
            "revision":{"type":"string","description":"A revision id from `list`."}
        },"required":["action"]}),
    )]
}

impl ToolHost {
    /// The path a history call touches and how, for the access policy.
    pub(super) fn history_touched(&self, input: &Value) -> Vec<(PathBuf, bool)> {
        let action = input.get("action").and_then(Value::as_str).unwrap_or("");
        match (action, input.get("file").and_then(Value::as_str)) {
            ("deleted", _) | (_, None) => vec![(self.project().to_path_buf(), false)],
            ("restore", Some(f)) => vec![(self.resolve(f), true)],
            (_, Some(f)) => vec![(self.resolve(f), false)],
        }
    }

    pub(super) fn run_history(&self, input: &Value) -> Result<String, String> {
        let action = input.get("action").and_then(Value::as_str).ok_or("`action` is required")?;
        // Each project keeps its own history: the one that owns the file, or the one named for `deleted`.
        let root_for = |file: &std::path::Path| self.root_of(file);
        let revision = input.get("revision").and_then(Value::as_str);
        let file = || -> Result<PathBuf, String> {
            Ok(self.resolve(input.get("file").and_then(Value::as_str).ok_or("`file` is required")?))
        };
        let show = |value: Value| clip(serde_json::to_string_pretty(&value).unwrap_or_default());
        match action {
            "list" => {
                let f = file()?;
                let v = self.call_handler("bennu_history_file", json!({"args": {"root": root_for(&f), "file": f.display().to_string()}}))?;
                Ok(show(v))
            }
            "deleted" => {
                let root = match input.get("project").and_then(Value::as_str).filter(|p| !p.is_empty()) {
                    Some(name) => self.resolve(name).display().to_string(),
                    None => self.session.root.clone(),
                };
                Ok(show(self.call_handler("bennu_history_deleted", json!({"args": {"root": root}}))?))
            }
            "diff" => {
                let revision = revision.ok_or("`revision` is required for a diff — take one from `list`")?;
                let f = file()?;
                let v = self.call_handler(
                    "bennu_history_diff",
                    json!({"args": {"root": root_for(&f), "file": f.display().to_string(), "revision": revision}}),
                )?;
                Ok(show(v))
            }
            "restore" => self.restore(&file()?, revision),
            other => Err(format!("`{other}` is not an action — use list, deleted, diff or restore")),
        }
    }

    fn restore(&self, file: &std::path::Path, revision: Option<&str>) -> Result<String, String> {
        let mut args = json!({"root": self.root_of(file), "file": file.display().to_string()});
        if let Some(r) = revision {
            args["revision"] = json!(r);
        }
        let old = self.call_handler("bennu_history_content", json!({ "args": args }))?;
        if old.get("is_text").and_then(Value::as_bool) == Some(false) {
            return Err("that revision is not text; restore it from Bennu's Local History dialog".into());
        }
        let restored = old.get("text").and_then(Value::as_str).ok_or("the revision could not be read as text")?.to_string();

        // A file that is gone is created back, like `create_file`; one that exists is edited.
        if file.exists() {
            let (current, stamp) = self.read_text(file)?;
            return self.submit(file, &current, restored, Some(stamp), None);
        }
        if self.session.applies_edits() {
            if let Some(parent) = file.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
        }
        self.submit(file, "", restored, None, None)
    }
}
