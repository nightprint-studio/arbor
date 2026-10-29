//! The assistant's message as the panel draws it, built up while a run goes on.
//!
//! The run produces events in the order things happen; the panel wants one message with blocks in the
//! order a person reads them. This is the translation, kept in one place and in the panel's own JSON
//! shape, so that what is streamed to the screen and what is saved to disk are the same value — there
//! is no second reducer on the frontend to disagree with it.

use serde_json::{json, Value};

/// How a tool is shown: the icon family the thread uses for it.
pub(super) fn tool_kind(name: &str) -> &'static str {
    match name {
        "edit_file" | "create_file" | "delete_file" | "bennu_write_file" => "edit",
        "search_text" | "find_files" => "search",
        "ask_user" => "search",
        n if n.contains("rename") || n.contains("refactor") || n.contains("parameter") || n.contains("safe_delete") => "refactor",
        n if n.contains("problems") || n.contains("check") || n.contains("compile") || n.contains("test") || n.contains("validate") => "validate",
        n if n.contains("read") || n.contains("contents") => "read",
        _ => "search",
    }
}

/// The key argument worth showing next to a tool's name: the file it touches, or what it searches for.
pub(super) fn subject(input: &Value) -> Option<String> {
    for key in ["file", "path", "pattern", "name", "symbol", "query"] {
        if let Some(s) = input.get(key).and_then(Value::as_str).filter(|s| !s.is_empty()) {
            // A path reads better as its file name; the full one is in the tool's input.
            let short = s.rsplit(['/', '\\']).next().unwrap_or(s);
            return Some(if key == "pattern" { format!("“{short}”") } else { short.to_string() });
        }
    }
    None
}

/// A result boiled down to a few words for the receipt line.
pub(super) fn detail_of(result: &Result<String, String>) -> String {
    let (text, failed) = match result {
        Ok(t) => (t.as_str(), false),
        Err(e) => (e.as_str(), true),
    };
    let first = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
    let clipped: String = first.chars().take(80).collect();
    match (failed, clipped.is_empty()) {
        (true, true) => "failed".into(),
        (true, false) => clipped,
        (false, true) => "done".into(),
        (false, false) => clipped,
    }
}

/// The message being built.
pub(super) struct View {
    id: String,
    model: String,
    at: u64,
    blocks: Vec<Value>,
}

impl View {
    pub(super) fn new(id: String, model: String, at: u64) -> Self {
        Self { id, model, at, blocks: Vec::new() }
    }

    /// Append streamed text, extending the last block when it is text too.
    pub(super) fn text(&mut self, delta: &str) {
        if let Some(last) = self.blocks.last_mut() {
            if last.get("type").and_then(Value::as_str) == Some("text") {
                let joined = format!("{}{}", last["text"].as_str().unwrap_or(""), delta);
                last["text"] = Value::String(joined);
                return;
            }
        }
        self.blocks.push(json!({ "type": "text", "text": delta }));
    }

    pub(super) fn push(&mut self, block: Value) {
        self.blocks.push(block);
    }

    pub(super) fn tool(&mut self, id: &str, name: &str, input: &Value, detail: &str, ok: bool) {
        let label = match subject(input) {
            Some(s) => format!("{} {s}", title_of(name)),
            None => title_of(name),
        };
        self.push(json!({
            "type": "tool",
            "call": { "id": id, "kind": tool_kind(name), "label": label, "detail": detail, "ok": ok },
        }));
    }

    /// Change a block in place, found by the id held in `key` (`call.id`, `request.id`, `set.id`,
    /// `proposal.id`). Returns whether it was found.
    pub(super) fn update(&mut self, pointer: &str, id: &str, change: impl FnOnce(&mut Value)) -> bool {
        for b in &mut self.blocks {
            if b.pointer(pointer).and_then(Value::as_str) == Some(id) {
                change(b);
                return true;
            }
        }
        false
    }

    pub(super) fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    pub(super) fn snapshot(&self) -> Value {
        json!({ "id": self.id, "role": "assistant", "at": self.at, "model": self.model, "blocks": self.blocks })
    }
}

/// `search_text` → `Search text`, `bennu_find_usages` → `Find usages`.
fn title_of(name: &str) -> String {
    let words = name.strip_prefix("bennu_").unwrap_or(name).replace('_', " ");
    let mut chars = words.chars();
    match chars.next() {
        Some(f) => f.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streamed_text_extends_one_block_until_something_else_comes() {
        let mut v = View::new("m".into(), "x".into(), 0);
        v.text("Hel");
        v.text("lo");
        v.tool("t", "search_text", &json!({"pattern": "total"}), "3 matches", true);
        v.text("Done");
        let s = v.snapshot();
        let blocks = s["blocks"].as_array().unwrap();
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0]["text"], "Hello");
        assert_eq!(blocks[2]["text"], "Done");
    }

    #[test]
    fn a_tool_receipt_names_what_it_touched_and_what_came_back() {
        let mut v = View::new("m".into(), "x".into(), 0);
        v.tool("t1", "bennu_read_file", &json!({"file": "C:\\p\\src\\Order.java"}), "412 lines", true);
        let call = &v.snapshot()["blocks"][0]["call"];
        assert_eq!(call["kind"], "read");
        assert_eq!(call["label"], "Read file Order.java");
    }

    #[test]
    fn a_failed_tool_shows_the_reason() {
        assert_eq!(detail_of(&Err("outside the project\nmore".into())), "outside the project");
        assert_eq!(detail_of(&Err(String::new())), "failed");
        assert_eq!(detail_of(&Ok(String::new())), "done");
    }

    #[test]
    fn a_block_is_found_and_changed_by_its_id() {
        let mut v = View::new("m".into(), "x".into(), 0);
        v.push(json!({"type": "access", "request": {"id": "a1"}}));
        assert!(v.update("/request/id", "a1", |b| b["request"]["answer"] = json!("read")));
        assert!(!v.update("/request/id", "nope", |_| {}));
        assert_eq!(v.snapshot()["blocks"][0]["request"]["answer"], "read");
    }

    #[test]
    fn tools_are_sorted_into_the_icon_families() {
        assert_eq!(tool_kind("edit_file"), "edit");
        assert_eq!(tool_kind("search_text"), "search");
        assert_eq!(tool_kind("bennu_rename_plan"), "refactor");
        assert_eq!(tool_kind("bennu_check_file"), "validate");
        assert_eq!(tool_kind("bennu_read_file"), "read");
    }
}
