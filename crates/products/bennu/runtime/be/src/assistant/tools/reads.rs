//! Reading files cheaply: what the model reads most, and what a naive read wastes.
//!
//! Four tools share one idea — a read should cost what is *new*:
//!
//!  * `bennu_read_file` (the catalogue's) is wrapped so that a file already read in this conversation comes
//!    back as one line when unchanged and as a diff when changed (`read_memory`), and so that the text is
//!    plain text with a one-line header instead of a JSON string full of escapes;
//!  * `read_files` reads several in one call — every call re-sends the whole conversation — under a total cap;
//!  * `file_outline` shows a file's declarations with line ranges, so a big file is opened by the part that
//!    matters (`outline`);
//!  * `expand_result` pages through the detail behind a digest (`digests`).

use std::collections::HashSet;

use super::*;
use crate::assistant::read_memory::{self, Read, Reply};
use crate::assistant::{digest_store, outline};

pub(super) fn is_read_tool(name: &str) -> bool {
    matches!(name, "read_files" | "file_outline" | "expand_result")
}

/// The default total a multi-file read may return, in characters (about twelve thousand tokens).
const DEFAULT_CAP: usize = 48_000;
const MAX_FILES: usize = 20;

pub(super) fn specs() -> Vec<ToolSpec> {
    vec![
        spec(
            "read_files",
            "Read several files, or parts of them, in one call. Every call re-sends the whole conversation, so three reads in one call cost much less than three calls. Each item is `{file, from_line?, to_line?, symbol?}` exactly like bennu_read_file. The total is capped (`max_chars`, default 48000): a file that does not fit is listed as not read, with its size — ask for it alone, or for a range. A file you already read and that has not changed comes back as one line; one that changed comes back as the lines that differ.",
            json!({"type":"object","properties":{
                "files":{"type":"array","minItems":1,"maxItems":MAX_FILES,"items":{"type":"object","properties":{
                    "file":{"type":"string","description":"Absolute path, or relative to the project."},
                    "from_line":{"type":"integer"},
                    "to_line":{"type":"integer"},
                    "symbol":{"type":"string","description":"Read one declaration instead of a range, e.g. `Order.total`."}
                },"required":["file"]}},
                "max_chars":{"type":"integer","description":"Total characters to return, 4000-120000. Default 48000."}
            },"required":["files"]}),
        ),
        spec(
            "file_outline",
            "The outline of one file: its declarations with their line ranges, in a few lines. Call it before opening a file you do not know, then read only the part you need with bennu_read_file (`symbol` or `from_line`/`to_line`) — a two-thousand-line file is thirty lines of outline. Uses the language server where there is one, and the file's text otherwise (Java and anything else).",
            json!({"type":"object","properties":{"file":{"type":"string","description":"Absolute path, or relative to the project."}},"required":["file"]}),
        ),
        spec(
            "expand_result",
            "Get more of a result that came back as a digest (a summary that ends by naming an id). `match` keeps only the lines that contain some text — a file name, a test name, a message; `offset` and `limit` page through what is left.",
            json!({"type":"object","properties":{
                "id":{"type":"string","description":"The id the digest named, e.g. `d3`."},
                "match":{"type":"string","description":"Only lines containing this text (case-insensitive)."},
                "offset":{"type":"integer","description":"Skip this many matching lines. Default 0."},
                "limit":{"type":"integer","description":"At most this many lines, 1-400. Default 80."}
            },"required":["id"]}),
        ),
    ]
}

/// The pieces of a `bennu_read_file` answer.
struct FileText<'a> {
    text: &'a str,
    encoding: &'a str,
    stamp: &'a str,
    from_line: u32,
    total_lines: u32,
}

impl<'a> FileText<'a> {
    fn of(value: &'a Value) -> Self {
        let n = |k: &str| value.get(k).and_then(Value::as_u64).unwrap_or(0) as u32;
        Self {
            text: value.get("text").and_then(Value::as_str).unwrap_or(""),
            encoding: value.get("encoding").and_then(Value::as_str).unwrap_or(""),
            stamp: value.get("stamp").and_then(Value::as_str).unwrap_or(""),
            from_line: n("from_line").max(1),
            total_lines: n("total_lines"),
        }
    }

    fn read(&self) -> Read<'a> {
        Read { stamp: self.stamp, text: self.text, from_line: self.from_line, total_lines: self.total_lines }
    }

    /// The text with a one-line header saying what and where it is.
    fn rendered(&self, shown: &str) -> String {
        let lines = self.text.lines().count() as u32;
        let to = self.from_line + lines.saturating_sub(1);
        let enc = if self.encoding.is_empty() { String::new() } else { format!(" · {}", self.encoding) };
        if self.text.is_empty() {
            return format!("{shown} — empty{enc}");
        }
        format!("{shown} — lines {}-{to} of {}{enc}\n{}", self.from_line, self.total_lines, self.text)
    }
}

/// A path as the read memory keys it: one spelling per file.
fn memory_key(path: &Path) -> String {
    let s = path.to_string_lossy().replace('\\', "/");
    if cfg!(windows) { s.to_lowercase() } else { s }
}

impl ToolHost {
    fn file_tool(&self) -> Result<&ToolDescriptor, String> {
        self.descriptor("bennu_read_file").ok_or_else(|| "bennu_read_file is not available".to_string())
    }

    /// The files a read tool names, for the access policy.
    pub(super) fn read_touched(&self, call: &ToolCall) -> Vec<PathBuf> {
        let input = &call.input;
        match call.name.as_str() {
            "read_files" => input
                .get("files")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|f| f.get("file").and_then(Value::as_str).or_else(|| f.as_str()))
                .map(|p| self.resolve(p))
                .collect(),
            "file_outline" => input.get("file").and_then(Value::as_str).map(|p| self.resolve(p)).into_iter().collect(),
            _ => Vec::new(),
        }
    }

    /// One read, planned against the conversation's memory: the text to send, and what to remember if it is.
    fn plan_read(&self, call_id: &str, args: &Value) -> Result<(String, Option<read_memory::Pending>, bool), String> {
        let value = self.catalog_call(self.file_tool()?, args)?;
        let file = args.get("file").and_then(Value::as_str).unwrap_or("");
        let abs = self.resolve(file);
        let shown = self.display_path(&abs.to_string_lossy());
        let content = FileText::of(&value);
        let (reply, pending) = read_memory::with(&self.session.conversation_id, |m| m.plan(call_id, &memory_key(&abs), &content.read()));
        Ok(match reply {
            Reply::Unchanged(line) => (format!("{shown}: {line}"), pending, false),
            Reply::Diff(diff) => (format!("{shown}: {diff}"), pending, false),
            Reply::Full => (content.rendered(&shown), pending, true),
        })
    }

    fn remember(&self, file: &str, pending: Option<read_memory::Pending>) {
        if let Some(p) = pending {
            read_memory::with(&self.session.conversation_id, |m| m.commit(&memory_key(&self.resolve(file)), p));
        }
    }

    /// `bennu_read_file`: the same read, answered from the conversation's memory where that is true.
    pub(super) fn run_read_file(&self, call_id: &str, input: &Value) -> Result<String, String> {
        let (text, pending, _) = self.plan_read(call_id, input)?;
        self.remember(input.get("file").and_then(Value::as_str).unwrap_or(""), pending);
        Ok(clip(text))
    }

    /// `read_files`: several reads, one answer, under a total cap.
    pub(super) fn run_read_files(&self, call_id: &str, input: &Value) -> Result<String, String> {
        let items = input.get("files").and_then(Value::as_array).ok_or("`files` is required")?;
        if items.is_empty() || items.len() > MAX_FILES {
            return Err(format!("name between 1 and {MAX_FILES} files"));
        }
        let cap = input.get("max_chars").and_then(Value::as_u64).map_or(DEFAULT_CAP, |c| (c as usize).clamp(4_000, 120_000));
        let mut sections: Vec<String> = Vec::new();
        let mut left: Vec<String> = Vec::new();
        let mut used = 0usize;
        for item in items {
            // A bare path is accepted as `{file: path}`.
            let args = if item.is_string() { json!({ "file": item }) } else { item.clone() };
            let name = args.get("file").and_then(Value::as_str).unwrap_or("?").to_string();
            match self.plan_read(call_id, &args) {
                Err(e) => left.push(format!("- {name}: {e}")),
                Ok((text, pending, full)) => {
                    if full && used + text.len() > cap {
                        left.push(format!("- {name}: {} characters would pass the {cap} cap — read it alone, or ask for a range or a `symbol`", text.len()));
                        continue;
                    }
                    used += text.len();
                    self.remember(&name, pending);
                    sections.push(text);
                }
            }
        }
        if !left.is_empty() {
            sections.push(format!("Not read:\n{}", left.join("\n")));
        }
        Ok(sections.join("\n\n"))
    }

    /// `file_outline`: the language server's outline if it has one, else what the text itself shows.
    pub(super) fn run_outline(&self, input: &Value) -> Result<String, String> {
        let file = input.get("file").and_then(Value::as_str).filter(|f| !f.is_empty()).ok_or("`file` is required")?;
        let abs = self.resolve(file);
        let shown = self.display_path(&abs.to_string_lossy());
        if let Some(d) = self.descriptor("bennu_outline") {
            if let Ok(v) = self.catalog_call(d, &json!({ "file": abs.to_string_lossy() })) {
                let entries = outline::from_server(&v);
                if !entries.is_empty() {
                    let total = v.get("total_lines").and_then(Value::as_u64).unwrap_or(0) as u32;
                    return Ok(outline::render(&shown, total, "the language server", &entries));
                }
            }
        }
        let (text, _) = self.read_text(&abs)?;
        let total = text.lines().count() as u32;
        let is_java = abs.extension().is_some_and(|e| e.eq_ignore_ascii_case("java"));
        let (entries, source) = if is_java {
            (outline::java_outline(&text), "Bennu's Java symbols")
        } else {
            (outline::text_outline(&text), "the file's text (no language server answered; declarations found by their first line)")
        };
        if entries.is_empty() {
            return Ok(format!("{shown} — {total} lines — no declarations found. Read it with bennu_read_file, or search it with search_text."));
        }
        Ok(outline::render(&shown, total, source, &entries))
    }

    /// Replace old, bulky, re-fetchable tool results by stubs — all at once, and only when that is worth one rewrite
    /// of the provider's cache — and say so in the thread.
    pub(super) fn stub_stale_results(&self, request: &mut Request) {
        if self.chat {
            return;
        }
        let policy = bennu_assistant::prelude::StalePolicy::default();
        if let Some((messages, report)) = bennu_assistant::prelude::maybe_stub_stale(&request.messages, &policy) {
            request.messages = messages;
            self.note(
                "info",
                &format!("{} older file reads and search results were replaced by short stubs (about {} tokens freed); the assistant fetches them again if it needs them.", report.stubbed.len(), report.freed_tokens),
            );
        }
    }

    /// Forget the files the read memory believes the conversation still holds but no longer does.
    pub(super) fn sync_read_memory(&self, messages: &[bennu_assistant::prelude::Message]) {
        let intact = intact_result_ids(messages);
        read_memory::with(&self.session.conversation_id, |m| m.retain_holders(&intact));
    }

    /// `expand_result`: the part of a digest's full text that was asked for.
    pub(super) fn run_expand(&self, input: &Value) -> Result<String, String> {
        let id = input.get("id").and_then(Value::as_str).unwrap_or("");
        let full = digest_store::get(&self.session.conversation_id, id)
            .ok_or_else(|| format!("no digest `{id}` is kept: it is from an earlier session, or newer digests pushed it out. Run the tool again."))?;
        let needle = input.get("match").and_then(Value::as_str).unwrap_or("");
        let offset = input.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
        let limit = input.get("limit").and_then(Value::as_u64).map_or(80, |l| l as usize);
        Ok(clip(bennu_assistant::prelude::expand_digest(&full, needle, offset, limit)))
    }
}

/// The ids of the tool results a conversation still holds whole: not stubbed, not cleared. What the read memory
/// checks its "you already have it above" against.
pub(in crate::assistant) fn intact_result_ids(messages: &[bennu_assistant::prelude::Message]) -> HashSet<String> {
    use bennu_assistant::prelude::{is_stale_stub, Block};
    messages
        .iter()
        .flat_map(|m| m.blocks.iter())
        .filter_map(|b| match b {
            Block::ToolResult { tool_use_id, content, .. } if !is_stale_stub(content) && content != crate::assistant::compact::CLEARED => Some(tool_use_id.clone()),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_assistant::prelude::{Block, Message};

    #[test]
    fn the_rendered_text_says_what_and_where_it_is_without_escapes() {
        let v = json!({"text": "fn a() {\n    \"x\"\n}", "encoding": "UTF-8", "stamp": "1:2", "total_lines": 90, "from_line": 10});
        let t = FileText::of(&v).rendered("src/a.rs");
        assert_eq!(t, "src/a.rs — lines 10-12 of 90 · UTF-8\nfn a() {\n    \"x\"\n}");
        assert_eq!(FileText::of(&json!({"text": "", "total_lines": 0})).rendered("e.rs"), "e.rs — empty");
    }

    #[test]
    fn only_results_still_whole_can_back_an_already_read_line() {
        let ok = |id: &str, c: &str| Block::ToolResult { tool_use_id: id.into(), content: c.into(), is_error: false };
        let m = vec![Message::tool_results(vec![
            ok("a", "plenty of text"),
            ok("b", &format!("{}: x]", bennu_assistant::prelude::STUB_PREFIX)),
            ok("c", crate::assistant::compact::CLEARED),
        ])];
        let ids = intact_result_ids(&m);
        assert!(ids.contains("a") && !ids.contains("b") && !ids.contains("c"));
    }

    #[test]
    fn the_read_tools_are_recognised_by_name() {
        for t in ["read_files", "file_outline", "expand_result"] {
            assert!(is_read_tool(t));
        }
        assert!(!is_read_tool("bennu_read_file"), "the catalogue's own tool keeps its descriptor");
        let names: Vec<String> = specs().into_iter().map(|s| s.name).collect();
        assert_eq!(names, ["read_files", "file_outline", "expand_result"]);
    }

    #[test]
    fn memory_keys_ignore_the_slash_direction() {
        assert_eq!(memory_key(Path::new("a\\b.rs")), memory_key(Path::new("a/b.rs")));
    }
}
