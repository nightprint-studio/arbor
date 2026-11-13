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
    ),
    spec(
        "structural_search",
        "Search Java or JSP code by its shape, across the project, and count it — nothing is changed. The query is code with placeholders: `log.debug($msg$)`, `catch ($t$ $e$) { }`, `@Autowired private $t$ $n$;`. Placeholders: `$x$` is one node (an expression, a name, a type, a whole statement or member when it stands alone in a body); `$xs...$` is a run — arguments, statements in a block (`{ $body...$ }`), members of a class body (`class $c$ { $members...$ }`), parameters (`m($params...$)`), or, in front of a declaration, zero or more modifiers and annotations (`$mods...$ final $t$ $n$ = $v$;`). A constraint follows a colon: `$x: Order$` (that type), `$x: Order+$` (or a subtype), `$m: ~get*$` (a name glob), `$x: #string_literal$` (a grammar node kind), `$x: @type$` / `$x: @value$` (a class name or a variable), `!` negates, `&` joins. Clauses go on their own lines: `or <pattern>` (another shape of the same question), `in src/main/java/x` (a path), `group $m$` / `group file` / `group module` / `group enclosing` (count by that instead of listing), `as local|param|field|member|statement|expression|class|catch` (how to read the pattern text — the same `final $t$ $n$ = $v$;` is a field or a local; `as` picks one and applies to the alternative it follows), `inside method|constructor|initializer|lambda|class|interface|enum|record|anonymous` (where the match must live; `body` = the first four, `type` = the last five; comma-separated for several), and `use of $m$ on com.acme.Type` (every use of a member). Returns the number of matches and files, then either the `group` table or a sample of places as file:line with the enclosing method. Use it to find or count before changing, then `structural_replace` to rewrite. `dialect` is `java` (the default), `jsp` (page markup) or `jspjava` (Java inside the pages' scriptlets).",
        json!({"type":"object","properties":{
            "query":{"type":"string","description":"The pattern and its clauses, one per line."},
            "dialect":{"type":"string","enum":["java","jsp","jspjava"]},
            "limit":{"type":"integer","description":"How many places to list (default 20, at most 100). The counts and a `group` table always cover every match."}
        },"required":["query"]}),
    )]
}

/// The line that says files were left out for want of access. A count and nothing else: naming the files, or
/// what was in them, would hand over the thing the rule keeps back.
fn skipped_note(skipped: u64) -> String {
    match skipped {
        0 => String::new(),
        1 => "\n1 file was skipped: Imber has no access to it.".to_string(),
        n => format!("\n{n} files were skipped: Imber has no access to them."),
    }
}

impl ToolHost {
    /// `structural_search`: run the query to the end and say what it found. Reads, never writes.
    pub(super) fn run_structural_search(&self, input: &Value) -> Result<String, String> {
        let query = input.get("query").and_then(Value::as_str).ok_or("`query` is required")?;
        if self.session.projects().first().is_some_and(|p| !p.modules.is_empty() && !p.has("Maven") && !p.has("Gradle")) {
            return Err("structural_search matches Java and JSP code, and this project has none. Use search_text here.".into());
        }
        let limit = input.get("limit").and_then(Value::as_u64).unwrap_or(20).clamp(1, 100);
        // What this conversation may reach, applied by the walk itself: a file it may not read is never
        // opened, so it is in no count, no table and no sample.
        let access = self.session.policy().scan_filter(false);
        let mut args = json!({"root": self.session.root, "query": query, "sample": limit, "access": access});
        if let Some(d) = input.get("dialect").and_then(Value::as_str) {
            args["dialect"] = json!(d);
        }
        let found = self.call_handler("bennu_ssr_run", json!({ "args": args }))?;
        if let Some(why) = found.get("error").and_then(Value::as_str) {
            return Err(format!("The query could not be run: {why}"));
        }
        let count = |k: &str| found.get(k).and_then(Value::as_u64).unwrap_or(0);
        let (total, files) = (count("total"), count("files"));
        let report = found.get("report");
        let mut out = String::new();
        let capped = found.get("capped").and_then(Value::as_bool).unwrap_or(false);
        let note = skipped_note(count("skipped"));
        if total == 0 {
            return Ok(format!(
                "No matches ({} files read, {} parsed). If the code is certainly there, check the pattern against one real example, the `dialect`, an `in`/`inside` clause, or whether the index is still building.{note}",
                count("scanned"),
                count("parsed")
            ));
        }
        out.push_str(&format!("{total} matches in {files} files ({} files read, {} parsed){}\n", count("scanned"), count("parsed"), if capped { " — stopped at the search's cap, there are more" } else { "" }));
        let undecided = report.and_then(|r| r.get("unresolved")).and_then(Value::as_u64).unwrap_or(0);
        if undecided > 0 {
            out.push_str(&format!("{undecided} of them could not have a type constraint decided (kept, not dropped).\n"));
        }
        let grouped = report.and_then(|r| r.get("groupedBy")).and_then(Value::as_str);
        if let (Some(by), Some(rows)) = (grouped, report.and_then(|r| r.get("rows")).and_then(Value::as_array)) {
            out.push_str(&format!("By {by}:\n"));
            for row in rows.iter().take(60) {
                let key = row.get("key").and_then(Value::as_str).unwrap_or("?");
                let n = row.get("count").and_then(Value::as_u64).unwrap_or(0);
                let f = row.get("files").and_then(Value::as_u64).unwrap_or(0);
                out.push_str(&format!("{n:>6}  {key}  ({f} files)\n"));
            }
            if rows.len() > 60 {
                out.push_str(&format!("… and {} more rows\n", rows.len() - 60));
            }
            out.push_str(&note);
            return Ok(out);
        }
        let sample = found.get("sample").and_then(Value::as_array).cloned().unwrap_or_default();
        out.push_str(&format!("First {}:\n", sample.len()));
        for hit in &sample {
            let rel = hit.get("rel").and_then(Value::as_str).unwrap_or("?");
            let line = hit.get("line").and_then(Value::as_u64).unwrap_or(0);
            let text = hit.get("preview").and_then(Value::as_str).unwrap_or("");
            let within = hit.get("enclosing").and_then(Value::as_str).map(|e| format!("  [in {e}]")).unwrap_or_default();
            out.push_str(&format!("{rel}:{line}  {text}{within}\n"));
        }
        if total as usize > sample.len() {
            out.push_str(&format!("… {} more. Narrow it with `in`, `inside` or a constraint, or ask for a larger `limit`.\n", total as usize - sample.len()));
        }
        out.push_str(&note);
        Ok(out)
    }

    pub(super) fn run_structural(&self, input: &Value) -> Result<String, String> {
        let text = |k: &str| input.get(k).and_then(Value::as_str).ok_or_else(|| format!("`{k}` is required"));
        let (query, replacement) = (text("query")?, text("replacement")?);
        let dry_run = input.get("dry_run").and_then(Value::as_bool).unwrap_or(false);
        if self.session.projects().first().is_some_and(|p| !p.modules.is_empty() && !p.has("Maven") && !p.has("Gradle")) {
            return Err("structural_replace matches Java and JSP code, and this project has none. Use replace_in_files or edit_files here.".into());
        }
        // A rewrite reaches only what could also be changed: read-only folders, secrets and `.git` are out,
        // and with editing off nothing is admitted at all. Skipped files are counted, never named.
        let access = self.session.policy().scan_filter(true);
        let mut args = json!({"root": self.session.root, "query": query, "replacement": replacement, "access": access});
        if let Some(d) = input.get("dialect").and_then(Value::as_str) {
            args["dialect"] = json!(d);
        }

        let plan = self.call_handler("bennu_ssr_preview", json!({ "args": args }))?;
        let files = plan.get("files").and_then(Value::as_array).cloned().unwrap_or_default();
        let note = skipped_note(plan.get("skipped").and_then(Value::as_u64).unwrap_or(0));
        if files.is_empty() {
            return Ok(format!("The pattern matches nothing that would change. Check it with a narrower sample; the index may also still be building.{note}"));
        }
        let total = plan.get("hits").and_then(Value::as_u64).unwrap_or(0);
        if dry_run {
            let list: Vec<String> = files
                .iter()
                .map(|f| format!("{} ({})", f.get("rel").and_then(Value::as_str).unwrap_or("?"), f.get("hits").and_then(Value::as_u64).unwrap_or(0)))
                .collect();
            return Ok(format!("Dry run — {total} matches in {} files, nothing changed:\n{}{note}", files.len(), list.join("\n")));
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
        Ok(format!("{total} matches in {} files:\n{}{note}", files.len(), report.join("\n")))
    }
}
