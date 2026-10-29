//! Structural search and replace as a tool.
//!
//! Bennu already matches code by its shape — `log.debug($msg$)` finds that call however it is spaced or
//! wrapped — and rewrites it with the captured parts. This is that engine given to the assistant, with one
//! difference: what the engine would write goes through the same door as every other change. The preview
//! is computed (nothing is written by it), and each file's result is proposed for review or applied by the
//! user's mode, never written behind it.

use bennu_assistant::prelude::ToolSpec;
use serde_json::{json, Value};

use super::{spec, ToolHost};

pub(super) fn specs() -> Vec<ToolSpec> {
    vec![spec(
        "structural_replace",
        "Rewrite Java or JSP code by its shape rather than its text, across the project. The query is code with placeholders — `log.debug($msg$)`, `new SimpleDateFormat($p$)`, `if ($c$) { return $v$; }` — and a placeholder matches whatever stands in that place, however the code is spaced or wrapped. The replacement is code that may reuse the placeholders: `LOG.debug($msg$)`. Far safer than a regular expression for code. Run it first with `dry_run: true` to see which files would change; then for real. The changes are proposed or applied by the user's mode, like an edit. `dialect` is `java` (the default), `jsp` (page markup) or `jspjava` (Java inside the pages' scriptlets).",
        json!({"type":"object","properties":{
            "query":{"type":"string","description":"The pattern, in the language it searches, with `$name$` placeholders."},
            "replacement":{"type":"string","description":"What replaces each match; may use the query's placeholders."},
            "dialect":{"type":"string","enum":["java","jsp","jspjava"]},
            "dry_run":{"type":"boolean","description":"Only say which files would change and how many matches each has."}
        },"required":["query","replacement"]}),
    )]
}

impl ToolHost {
    pub(super) fn run_structural(&self, input: &Value) -> Result<String, String> {
        let text = |k: &str| input.get(k).and_then(Value::as_str).ok_or_else(|| format!("`{k}` is required"));
        let (query, replacement) = (text("query")?, text("replacement")?);
        let dry_run = input.get("dry_run").and_then(Value::as_bool).unwrap_or(false);
        if self.session.projects.first().is_some_and(|p| !p.modules.is_empty() && !p.has("Maven") && !p.has("Gradle")) {
            return Err("structural_replace matches Java and JSP code, and this project has none. Use replace_in_files or edit_files here.".into());
        }
        let mut args = json!({"root": self.session.root, "query": query, "replacement": replacement});
        if let Some(d) = input.get("dialect").and_then(Value::as_str) {
            args["dialect"] = json!(d);
        }

        let plan = self.call_handler("bennu_ssr_preview", json!({ "args": args }))?;
        let files = plan.get("files").and_then(Value::as_array).cloned().unwrap_or_default();
        if files.is_empty() {
            return Ok("The pattern matches nothing that would change. Check it with a narrower sample; the index may also still be building.".into());
        }
        let total = plan.get("hits").and_then(Value::as_u64).unwrap_or(0);
        if dry_run {
            let list: Vec<String> = files
                .iter()
                .map(|f| format!("{} ({})", f.get("rel").and_then(Value::as_str).unwrap_or("?"), f.get("hits").and_then(Value::as_u64).unwrap_or(0)))
                .collect();
            return Ok(format!("Dry run — {total} matches in {} files, nothing changed:\n{}", files.len(), list.join("\n")));
        }

        let mut report = Vec::new();
        for f in &files {
            let (Some(file), Some(before), Some(after)) =
                (f.get("file").and_then(Value::as_str), f.get("before").and_then(Value::as_str), f.get("after").and_then(Value::as_str))
            else {
                continue;
            };
            let path = std::path::PathBuf::from(file);
            let shown = self.display_path(file);
            // The plan was computed from the file as it was; a file that moved on since is not rewritten.
            match self.read_text(&path) {
                Ok((now, stamp)) if now.replace("
", "
") == before.replace("
", "
") => match self.submit(&path, before, after.to_string(), Some(stamp), None) {
                    Ok(m) => report.push(m),
                    Err(m) => report.push(format!("{shown}: {m}")),
                },
                Ok(_) => report.push(format!("{shown}: skipped — it changed since the plan was made")),
                Err(e) => report.push(format!("{shown}: skipped ({e})")),
            }
        }
        Ok(format!("{total} matches in {} files:\n{}", files.len(), report.join("\n")))
    }
}
