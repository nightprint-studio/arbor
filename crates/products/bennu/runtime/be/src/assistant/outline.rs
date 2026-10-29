//! A file's outline as a few lines of text, for when the language server has none to give.
//!
//! `bennu_outline` answers from a language server (Rust, TypeScript, Svelte …), as verbose JSON, and
//! says nothing for Java, which has none. The model's cheapest way to open a big file is its outline —
//! thirty lines instead of two thousand — so the assistant's own `file_outline` tries the server first and
//! falls back to what this module can read straight from the text: Java through Bennu's own symbol
//! extractor, everything else by the lines that start a declaration. A fallback is labelled as one in the
//! header, so the model knows how much to trust it.

use serde_json::Value;

/// One line of an outline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Entry {
    pub depth: usize,
    pub from: u32,
    /// 0 when only the first line is known.
    pub to: u32,
    pub text: String,
}

/// The most entries printed; the rest are counted.
const MAX_ENTRIES: usize = 160;
const MAX_TEXT: usize = 110;

fn line_of(source: &str, byte: usize) -> u32 {
    let byte = byte.min(source.len());
    source.as_bytes()[..byte].iter().filter(|b| **b == b'\n').count() as u32 + 1
}

fn clip(text: &str) -> String {
    let t = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.chars().count() <= MAX_TEXT { t } else { format!("{}…", t.chars().take(MAX_TEXT).collect::<String>()) }
}

// ── Java, from the symbol extractor ──────────────────────────────────────────────────────────────

pub(super) fn java_outline(source: &str) -> Vec<Entry> {
    use bennu_java::prelude::extract_symbols;
    let symbols = extract_symbols(source);
    let mut types: Vec<_> = symbols.types.iter().filter(|t| !t.is_anonymous && t.span.is_some()).collect();
    types.sort_by_key(|t| t.span.map_or(0, |s| s.start));
    let mut out: Vec<Entry> = Vec::new();
    // Spans of the types still open, to know how deep the next one sits.
    let mut open: Vec<usize> = Vec::new();
    for t in types {
        let span = t.span.expect("filtered above");
        while open.last().is_some_and(|end| span.start >= *end) {
            open.pop();
        }
        let depth = open.len();
        open.push(span.end);
        let mut head = format!("{} {}", t.kind.slug(), t.name);
        if let Some(e) = &t.extends {
            head.push_str(&format!(" extends {e}"));
        }
        if !t.implements.is_empty() {
            head.push_str(&format!(" implements {}", t.implements.join(", ")));
        }
        out.push(Entry { depth, from: line_of(source, span.start), to: line_of(source, span.end), text: clip(&head) });
        let mut members: Vec<Entry> = Vec::new();
        for f in t.fields.iter().filter(|f| f.span.is_some()) {
            let s = f.span.expect("filtered above");
            members.push(Entry { depth: depth + 1, from: line_of(source, s.start), to: 0, text: clip(&format!("field {} {}", f.type_text, f.name)) });
        }
        for m in t.methods.iter().filter(|m| m.span.is_some()) {
            let s = m.span.expect("filtered above");
            let params: Vec<String> = m.params.iter().map(|p| format!("{} {}", p.type_text, p.name)).collect();
            let vis = format!("{:?}", m.visibility).to_lowercase();
            let vis = if vis == "package" { String::new() } else { format!("{vis} ") };
            // A constructor is `<init>` to the extractor; it reads as the type's name, with no return type.
            let text = if m.name == "<init>" {
                format!("{vis}{}({})", t.name, params.join(", "))
            } else {
                format!("{vis}{}{} {}({})", if m.is_static { "static " } else { "" }, m.return_type_text, m.name, params.join(", "))
            };
            members.push(Entry { depth: depth + 1, from: line_of(source, s.start), to: line_of(source, s.end), text: clip(&text) });
        }
        members.sort_by_key(|e| e.from);
        out.extend(members);
    }
    out
}

// ── Anything else, from the lines that open a declaration ────────────────────────────────────────

/// Words a declaration starts with, in the languages Bennu opens (Rust, TypeScript/JavaScript, Python, Go, Svelte).
const OPENERS: &[&str] = &[
    "fn ", "pub fn ", "pub(crate) fn ", "pub(super) fn ", "async fn ", "pub async fn ", "struct ", "pub struct ", "enum ", "pub enum ", "trait ", "pub trait ", "impl ", "impl<", "mod ",
    "pub mod ", "function ", "export function ", "export default function ", "export async function ", "async function ", "class ", "export class ", "export default class ",
    "interface ", "export interface ", "export type ", "export const ", "def ", "async def ", "func ", "type ", "abstract class ", "export abstract class ",
];

pub(super) fn text_outline(source: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    for (i, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if !OPENERS.iter().any(|o| trimmed.starts_with(o)) {
            continue;
        }
        let indent = line.len() - trimmed.len();
        // Up to the opening brace or colon, not the body.
        let head = trimmed.split(['{', ';']).next().unwrap_or(trimmed).trim_end_matches(|c: char| c == ':' || c.is_whitespace());
        out.push(Entry { depth: (indent / 4).min(3), from: i as u32 + 1, to: 0, text: clip(head) });
    }
    out
}

// ── Rendering ────────────────────────────────────────────────────────────────────────────────────

/// The outline as text: one line per declaration, `from-to` or `from`, indented by depth.
pub(super) fn render(path: &str, total_lines: u32, source_kind: &str, entries: &[Entry]) -> String {
    let mut out = format!("{path} — {total_lines} lines — outline from {source_kind}\n");
    for e in entries.iter().take(MAX_ENTRIES) {
        let span = if e.to > e.from { format!("{}-{}", e.from, e.to) } else { e.from.to_string() };
        out.push_str(&format!("{}{span} {}\n", "  ".repeat(e.depth), e.text));
    }
    if entries.len() > MAX_ENTRIES {
        out.push_str(&format!("… {} more declarations not listed; read a range or a `symbol` for the part you need.\n", entries.len() - MAX_ENTRIES));
    }
    out.push_str("Read one declaration with bennu_read_file `symbol` or the line range above.");
    out
}

/// The entries of a `bennu_outline` result (JSON from the language server), in the same shape.
pub(super) fn from_server(value: &Value) -> Vec<Entry> {
    value
        .get("entries")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|e| {
            let name = e.get("name").and_then(Value::as_str).unwrap_or("");
            let kind = e.get("kind").and_then(Value::as_str).unwrap_or("");
            let detail = e.get("detail").and_then(Value::as_str).filter(|d| !d.is_empty()).map(|d| format!(" {d}")).unwrap_or_default();
            let n = |k: &str| e.get(k).and_then(Value::as_u64).unwrap_or(0) as u32;
            // The declaration's own first line (its doc comment included) when the server gave one, else the name's.
            let from = if n("from_line") > 0 { n("from_line") } else { n("line") };
            Entry { depth: e.get("depth").and_then(Value::as_u64).unwrap_or(0) as usize, from, to: n("to_line"), text: clip(&format!("{kind} {name}{detail}")) }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const JAVA: &str = "package demo;\n\npublic class Order extends Base implements Comparable<Order> {\n    private int total;\n    private final String id = \"x\";\n\n    public Order(String id) {\n        this.id = id;\n    }\n\n    public static int sum(int a, int b) {\n        return a + b;\n    }\n\n    class Line {\n        void print() {}\n    }\n}\n";

    #[test]
    fn a_java_file_is_outlined_from_its_symbols_with_line_ranges() {
        let o = java_outline(JAVA);
        let text: Vec<String> = o.iter().map(|e| format!("{}{}-{} {}", "  ".repeat(e.depth), e.from, e.to, e.text)).collect();
        assert!(text[0].starts_with("3-18 class Order extends Base implements Comparable<Order>"), "{text:?}");
        assert!(o.iter().any(|e| e.depth == 1 && e.from == 4 && e.text == "field int total"), "{text:?}");
        assert!(o.iter().any(|e| e.depth == 1 && e.from == 11 && e.to == 13 && e.text == "public static int sum(int a, int b)"), "{text:?}");
        assert!(o.iter().any(|e| e.depth == 1 && e.text.starts_with("class Line")), "a nested type is a member of its parent: {text:?}");
        assert!(o.iter().any(|e| e.depth == 2 && e.text.contains("print()")), "{text:?}");
    }

    #[test]
    fn other_files_are_outlined_by_the_lines_that_open_a_declaration() {
        let ts = "import x from 'y';\n\nexport class Store {\n    load(id: string) {\n        return 1;\n    }\n}\n\nexport function build(a: number): Store {\n    return new Store();\n}\n";
        let o = text_outline(ts);
        assert_eq!(o.iter().map(|e| (e.from, e.text.as_str())).collect::<Vec<_>>(), [(3, "export class Store"), (9, "export function build(a: number): Store")]);
        let rs = "pub struct A {\n    x: i32,\n}\n\nimpl A {\n    pub fn new() -> Self {\n        A { x: 0 }\n    }\n}\n";
        let o = text_outline(rs);
        assert_eq!(o.iter().map(|e| (e.depth, e.from)).collect::<Vec<_>>(), [(0, 1), (0, 5), (1, 6)]);
    }

    #[test]
    fn the_server_entries_are_rendered_the_same_way() {
        let v = json!({"total_lines": 900, "entries": [
            {"name": "Store", "kind": "struct", "line": 3, "column": 1, "from_line": 2, "to_line": 40, "depth": 0},
            {"name": "load", "kind": "method", "detail": "fn(&self)", "line": 10, "from_line": 9, "to_line": 20, "depth": 1},
        ]});
        let text = render("src/store.rs", 900, "the language server", &from_server(&v));
        assert!(text.starts_with("src/store.rs — 900 lines — outline from the language server\n2-40 struct Store\n  9-20 method load fn(&self)\n"), "{text}");
        assert!(text.ends_with("or the line range above."));
    }

    #[test]
    fn a_long_outline_is_cut_and_says_so() {
        let many: Vec<Entry> = (1..=200).map(|i| Entry { depth: 0, from: i, to: 0, text: format!("fn f{i}()") }).collect();
        let text = render("a.rs", 400, "its text", &many);
        assert!(text.contains("… 40 more declarations not listed") && !text.contains("fn f161()"));
    }
}
