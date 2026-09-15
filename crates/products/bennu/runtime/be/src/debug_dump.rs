//! A whole value at once, as RON-shaped text.
//!
//! The variables tree is lazy for a good reason — a stopped program has an object graph, and walking
//! it eagerly would be a round trip per node for rows nobody looked at. But laziness has a cost that
//! only shows up on real data: a struct with fifteen fields, four of which are structs, is **nineteen
//! disclosure triangles** before you can read the thing, and by the time it is open it does not fit on
//! screen. That is a bad way to answer "what is in this value".
//!
//! So: one request, the whole subtree, rendered as text you can read top to bottom, search, and copy
//! into a bug report.
//!
//! ## Why RON and not JSON
//!
//! Because the value is a Rust value. RON keeps the three distinctions JSON throws away and that are
//! exactly the ones you are looking at a debugger to see: a **named struct** (`Order(id: 7)`) reads
//! differently from a **map**, a **tuple** `(1, "x")` differently from a list, and an **enum variant**
//! is a name rather than a tag field somebody invented. Pasting a `HashMap<String, u32>` into JSON
//! makes it an object and its keys strings; RON does not have to lie about it. And it colours: Bennu
//! already has a `.ron` mode, so the modal is the real editor rather than a `<pre>`.
//!
//! **RON-shaped, not RON-exact.** What is rendered comes from a debugger, which reports a name, a
//! rendered value and a list of children — not a type system. So the shape is inferred from the
//! children's names (`[0]`… is a sequence, `0`… a tuple, anything else a struct) and the container's
//! name from the variant the debugger printed. Round-tripping the output through a RON parser is not a
//! promise; reading it is.
//!
//! ## Protocol-blind, on purpose
//!
//! This walks [`DebugBackend::expand`] and nothing else, so **it works on both debuggers for free** —
//! the same modal on a Java object graph and on a Rust one. That the trait was worth having is the
//! reason this module is fifty lines of walk rather than two implementations.
//!
//! ## The three caps, and why a dump says when it hit one
//!
//! Every node is a round trip against a suspended program. So the walk stops on whichever of these
//! comes first — a node count, a depth, and a **wall-clock budget** — and the result says so, because
//! a dump silently cut at 2000 nodes reads as a complete answer and would be quoted as one.

use std::time::{Duration, Instant};

use bennu_proto::prelude::{DebugDump, DebugValue};

use crate::debug_backend::DebugBackend;

/// How many nodes to visit. Each is a round trip, and past a couple of thousand the text is not
/// something anybody reads anyway.
const MAX_NODES: u32 = 2_000;

/// How deep to follow. Deep enough for any real data model; a hard stop rather than trusting a graph
/// to be acyclic, which `Rc` cycles and doubly-linked lists are not.
const MAX_DEPTH: usize = 16;

/// How many children of one container to render. Below `expand`'s own cap, because a wall of five
/// hundred elements buries the fields around it.
const MAX_CHILDREN: usize = 200;

/// How long the whole walk may take. The guard that actually fires in practice: the node cap assumes
/// every request is fast, and a debuggee stopped inside something unhappy makes that false.
const BUDGET: Duration = Duration::from_secs(4);

/// How long a container's one-line summary may be before it is cut. It rides on the opening line as a
/// comment, and a 200-character one pushes the structure it is annotating off the right of the screen.
const MAX_SUMMARY: usize = 96;

/// One indent level.
const INDENT: &str = "    ";

/// Dump `root` and everything under it.
pub(crate) fn dump(backend: &dyn DebugBackend, root: &DebugValue) -> DebugDump {
    render(root, &mut |handle| backend.expand(handle))
}

/// The walk and the rendering, over any source of children.
///
/// Separated from [`dump`] so the formatting, the caps and the cycle guard are testable against a
/// hand-built graph — which is the whole of this module's logic, since the protocol part is one call.
fn render(
    root: &DebugValue,
    expand: &mut impl FnMut(&str) -> Result<Vec<DebugValue>, String>,
) -> DebugDump {
    let mut out = String::new();
    let mut state = Walk {
        nodes: 0,
        truncated: false,
        deadline: Instant::now() + BUDGET,
        // The handles already on the path from the root — a cycle, not a repetition. The same object
        // appearing twice in two different branches is a fact worth printing twice; the same object
        // inside itself is infinite.
        path: Vec::new(),
    };

    // A header line rather than a bare value: the name and the *full* type are the two things the
    // tree row was truncating, and a `//` comment is legal RON.
    let head = match root.type_name.trim() {
        "" => format!("// {}\n", root.name),
        type_name => format!("// {}: {}\n", root.name, type_name),
    };
    out.push_str(&head);

    write_value(&mut out, root, 0, &mut state, expand);
    out.push('\n');

    DebugDump { text: out, nodes: state.nodes, truncated: state.truncated }
}

struct Walk {
    nodes: u32,
    truncated: bool,
    deadline: Instant,
    path: Vec<String>,
}

impl Walk {
    /// Whether there is room for one more node. Sets `truncated` the moment there is not, so the
    /// answer says it was cut rather than looking complete.
    fn may_descend(&mut self, depth: usize) -> bool {
        if depth >= MAX_DEPTH || self.nodes >= MAX_NODES || Instant::now() >= self.deadline {
            self.truncated = true;
            return false;
        }
        true
    }
}

fn write_value(
    out: &mut String,
    value: &DebugValue,
    depth: usize,
    state: &mut Walk,
    expand: &mut impl FnMut(&str) -> Result<Vec<DebugValue>, String>,
) {
    state.nodes += 1;

    let Some(handle) = value.object.as_deref().filter(|h| !h.is_empty()) else {
        out.push_str(&leaf(value));
        return;
    };
    // A value the debugger rendered as a **literal** is the value, children or not — see [`is_literal`]
    // for why this is the difference between reading a path and reading ninety characters of one.
    if is_literal(&value.value) {
        out.push_str(&leaf(value));
        return;
    }
    if state.path.iter().any(|seen| seen == handle) {
        // The value is genuinely inside itself. Said, because the alternative is a stack overflow.
        out.push_str(&format!("{} /* already above: a cycle */", opener_name(value)));
        return;
    }
    if !state.may_descend(depth) {
        out.push_str(&format!("{} /* not expanded */", opener_name(value)));
        return;
    }

    let children = match expand(handle) {
        Ok(children) => children,
        // A handle the session refuses — stale, or a field the debugger cannot read. The reason is
        // the interesting part and it goes where the value would have been.
        Err(e) => {
            out.push_str(&format!("/* {} */", e.replace("*/", "* /")));
            return;
        }
    };
    if children.is_empty() {
        out.push_str(&leaf(value));
        return;
    }

    let shape = Shape::of(&children);
    let (open, close) = shape.delimiters(value);
    out.push_str(&open);
    // The debugger's own one-line rendering, when it says something the children do not — an enum's
    // variant, a smart pointer's target. Dropped when it is boilerplate, because `// size=3` above
    // three visible elements is noise.
    if let Some(summary) = informative_summary(value, &open) {
        out.push_str(&format!(" // {summary}"));
    }
    out.push('\n');

    state.path.push(handle.to_string());
    let shown = children.len().min(MAX_CHILDREN);
    for child in &children[..shown] {
        out.push_str(&INDENT.repeat(depth + 1));
        if let Some(label) = shape.label(child) {
            out.push_str(&label);
        }
        write_value(out, child, depth + 1, state, expand);
        out.push_str(",\n");
    }
    if children.len() > shown {
        // Said, not silently dropped: a list that stops at 200 without saying so reads as a list of
        // 200.
        out.push_str(&INDENT.repeat(depth + 1));
        out.push_str(&format!("// … {} more\n", children.len() - shown));
        state.truncated = true;
    }
    state.path.pop();

    out.push_str(&INDENT.repeat(depth));
    out.push_str(close);
}

/// A value with nothing inside it, as one token.
fn leaf(value: &DebugValue) -> String {
    let text = value.value.trim();
    if text.is_empty() {
        // A row the debugger rendered as nothing at all and that has no children either. `()` is
        // RON's unit and is the honest reading.
        return "()".to_string();
    }
    text.to_string()
}

/// Whether the debugger's own rendering already **is** the whole value, so opening it can only make
/// things worse.
///
/// Load-bearing, and the reason is specific: the Rust formatters render a `String`, a `&str`, an
/// `OsString`, a `PathBuf`, a `Path` and a `CString` as their text — *and still hand out children*,
/// because underneath every one of them is a byte buffer. So a walk that expands whenever there are
/// children turns one path into four levels of `Buf` → `inner` → `inner` and then ninety rows reading
/// `'/'`, `'U'`, `'s'`, `'e'`, `'r'`, `'s'` — which is the value nobody wanted, spelled out. A summary
/// provider saying "this is the value" is exactly the signal to stop.
///
/// Only quoted forms count. A `size=3` or a `strong=2, weak=0` describes a container without being it,
/// and those still open.
fn is_literal(rendered: &str) -> bool {
    let text = rendered.trim();
    for quote in ['"', '\''] {
        if !text.starts_with(quote) {
            continue;
        }
        // A long string the debugger cut keeps its opening quote and marks the cut. Still the whole
        // answer as far as opening it would help — the characters underneath are not the missing part.
        let body = text
            .strip_suffix("...")
            .or_else(|| text.strip_suffix('\u{2026}'))
            .unwrap_or(text)
            .trim_end();
        if body.len() >= 2 && body.ends_with(quote) {
            return true;
        }
    }
    false
}

/// What a container's children say about its shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// `[0]`, `[1]`, … — a `Vec`, an array, a slice, a Java array.
    Seq,
    /// `0`, `1`, … — a tuple, or a tuple struct.
    Tuple,
    /// Named fields.
    Struct,
}

impl Shape {
    fn of(children: &[DebugValue]) -> Shape {
        let bracketed = |name: &str| {
            name.strip_prefix('[')
                .and_then(|n| n.strip_suffix(']'))
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        };
        if children.iter().all(|c| bracketed(&c.name)) {
            return Shape::Seq;
        }
        if children
            .iter()
            .all(|c| !c.name.is_empty() && c.name.bytes().all(|b| b.is_ascii_digit()))
        {
            return Shape::Tuple;
        }
        Shape::Struct
    }

    /// The opening and closing tokens. A struct carries a name where one can be had, which is what
    /// makes the output readable rather than a nest of anonymous braces.
    fn delimiters(self, value: &DebugValue) -> (String, &'static str) {
        match self {
            // A sequence is bare in RON, and its type is on the header line anyway.
            Shape::Seq => ("[".to_string(), "]"),
            // A named tuple is RON's tuple-struct form — `Some(3)`, `Wrapper(7)` — and an anonymous
            // one falls back to a plain `(…)` because `opener_name` gives nothing for a type that is
            // not an identifier.
            Shape::Tuple | Shape::Struct => (format!("{}(", opener_name(value)), ")"),
        }
    }

    /// How a child is introduced. A sequence's index is noise — position says it — and a tuple's
    /// field number likewise.
    fn label(self, child: &DebugValue) -> Option<String> {
        match self {
            Shape::Seq | Shape::Tuple => None,
            Shape::Struct => Some(format!("{}: ", child.name)),
        }
    }
}

/// The name to put in front of a struct's parenthesis.
///
/// The **variant the debugger printed** wins over the declared type, because that is the more specific
/// truth and the one RON wants: an `Option<u32>` holding `Some(3)` should read `Some(…)`, not
/// `Option(…)`. Otherwise the type's own name with the module path taken off — `geode::mine::Order`
/// is `Order`, and `Vec<geode::Order>` keeps its parameter because that is the informative half.
fn opener_name(value: &DebugValue) -> String {
    if let Some(variant) = variant_of(&value.value) {
        return variant;
    }
    simple_type(&value.type_name)
}

/// The leading identifier of a rendered value, when it looks like an enum variant carrying
/// something: `Some(3)`, `Ok(())`, `Kind::Mine { depth: 3 }`. `None` for `42`, `"text"`, `{...}`,
/// `0x7ff…` — and for a bare `Pending`, which has no children and is therefore a leaf that never
/// reaches here.
fn variant_of(rendered: &str) -> Option<String> {
    let text = rendered.trim();
    let head: String =
        text.chars().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == ':').collect();
    if head.is_empty() || head.len() == text.len() && !is_identifier(&head) {
        return None;
    }
    // What follows the identifier has to be an opening parenthesis or brace — otherwise this is a
    // number with a suffix, or prose.
    let rest = text[head.len()..].trim_start();
    if !rest.starts_with('(') && !rest.starts_with('{') {
        return None;
    }
    is_identifier(&head).then(|| simple_type(&head))
}

fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars.next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && text.chars().all(|c| c.is_alphanumeric() || c == '_' || c == ':')
}

/// A type without its module path, keeping its generic parameters: `alloc::vec::Vec<geode::Order>`
/// becomes `Vec<geode::Order>`.
///
/// Only the **outermost** path is stripped — the parameters are left alone, because a
/// `HashMap<String, Order>` whose parameters had been simplified reads the same and one whose had been
/// mangled would not.
///
/// A type that is not an identifier path — `(i32, &str)`, `&[u8]`, `*mut c_void` — has no name that
/// could go in front of a parenthesis, so it returns nothing and the container renders anonymously.
/// Emitting `(i32, &str)(…)` would be worse than emitting `(…)`.
fn simple_type(type_name: &str) -> String {
    let text = type_name.trim();
    let head_end = text.find('<').unwrap_or(text.len());
    let head = &text[..head_end];
    let simple = head.rsplit("::").next().unwrap_or(head);
    if !simple.starts_with(|c: char| c.is_alphabetic() || c == '_') {
        return String::new();
    }
    format!("{simple}{}", &text[head_end..])
}

/// The debugger's own one-liner, when it adds something the expansion does not.
///
/// Dropped for the forms that repeat what is about to be printed anyway: the name we just used, a
/// `size=`/`len=` count above the elements themselves, and the `{…}` / `{n fields}` placeholders the
/// backend substitutes for a value the debugger rendered as nothing.
fn informative_summary(value: &DebugValue, opener: &str) -> Option<String> {
    let text = value.value.trim();
    if text.is_empty() {
        return None;
    }
    let name = opener.trim_end_matches('(');
    if !name.is_empty() && (text == name || text.starts_with(&format!("{name}("))) {
        return None;
    }
    // An anonymous container whose one-liner is the same brackets we are about to print.
    if text.starts_with(opener) {
        return None;
    }
    if text.starts_with('{') && text.ends_with('}') {
        return None;
    }
    let counted = text
        .split_once('=')
        .is_some_and(|(k, v)| matches!(k, "size" | "len" | "length" | "count") && !v.is_empty());
    if counted {
        return None;
    }
    let one_line = text.replace("*/", "* /").replace('\n', " ");
    let cut: String = one_line.chars().take(MAX_SUMMARY).collect();
    Some(if cut.chars().count() < one_line.chars().count() {
        format!("{cut}\u{2026}")
    } else {
        cut
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// A tree by handle, so the walk can be driven without a debugger.
    struct Fake(HashMap<String, Vec<DebugValue>>);

    impl Fake {
        fn expander(&self) -> impl FnMut(&str) -> Result<Vec<DebugValue>, String> + '_ {
            move |handle: &str| {
                self.0.get(handle).cloned().ok_or_else(|| format!("no such handle {handle}"))
            }
        }
    }

    fn leaf_value(name: &str, type_name: &str, rendered: &str) -> DebugValue {
        DebugValue {
            name: name.to_string(),
            kind: "field".to_string(),
            type_name: type_name.to_string(),
            value: rendered.to_string(),
            object: None,
        }
    }

    fn parent(name: &str, type_name: &str, rendered: &str, handle: &str) -> DebugValue {
        DebugValue { object: Some(handle.to_string()), ..leaf_value(name, type_name, rendered) }
    }

    /// The shape the screenshot was of: a struct whose fields are structs, read top to bottom
    /// instead of through nineteen disclosure triangles.
    #[test]
    fn a_struct_of_structs_reads_as_named_ron() {
        let fake = Fake(HashMap::from([
            (
                "1".to_string(),
                vec![
                    leaf_value("id", "alloc::string::String", "\"geode-core\""),
                    parent("root", "std::path::PathBuf", "", "2"),
                ],
            ),
            ("2".to_string(), vec![leaf_value("inner", "std::ffi::OsString", "\"/Users/c\"")]),
        ]));
        let root = parent("sorgente", "nd_stub_generator::Cartella", "", "1");
        let dump = render(&root, &mut fake.expander());

        assert_eq!(
            dump.text,
            "// sorgente: nd_stub_generator::Cartella\n\
             Cartella(\n\
             \x20   id: \"geode-core\",\n\
             \x20   root: PathBuf(\n\
             \x20       inner: \"/Users/c\",\n\
             \x20   ),\n\
             )\n"
        );
        assert!(!dump.truncated);
        assert_eq!(dump.nodes, 4);
    }

    /// The one that produced this rule. A `PathBuf` is rendered as its text by the formatters **and**
    /// still hands out children, because underneath it is a `Vec<u8>`: expanding it gave four levels of
    /// `Buf` → `inner` → `inner` and then one row per character of the path.
    #[test]
    fn a_path_is_its_text_and_not_its_bytes() {
        let fake = Fake(HashMap::from([
            ("root".to_string(), vec![parent("inner", "std::ffi::OsString", "\"/Users/c\"", "os")]),
            ("os".to_string(), vec![parent("inner", "Buf", "", "buf")]),
            (
                "buf".to_string(),
                vec![leaf_value("[0]", "u8", "'/'"), leaf_value("[1]", "u8", "'U'")],
            ),
        ]));
        let dump =
            render(&parent("root", "std::path::PathBuf", "\"/Users/c\"", "root"), &mut fake.expander());
        assert_eq!(dump.text, "// root: std::path::PathBuf\n\"/Users/c\"\n");
        assert_eq!(dump.nodes, 1, "one value read, not the whole byte buffer");
        // Not one row of the buffer underneath, and not one of its characters.
        assert!(!dump.text.contains("inner"), "{}", dump.text);
        assert!(!dump.text.contains("'/'"), "{}", dump.text);
    }

    /// The same rule, on the type it bites hardest: a `String`'s children are its characters.
    #[test]
    fn a_string_is_its_text_and_not_its_characters() {
        let fake = Fake(HashMap::from([(
            "s".to_string(),
            vec![leaf_value("[0]", "char", "'c'"), leaf_value("[1]", "char", "'i'")],
        )]));
        let dump = render(&parent("s", "alloc::string::String", "\"ci\"", "s"), &mut fake.expander());
        assert_eq!(dump.text, "// s: alloc::string::String\n\"ci\"\n");
    }

    /// Which literals stop the walk, and — the half that matters more — which summaries do not. A
    /// `size=3` describes a container without being it, and stopping there would hide the elements.
    #[test]
    fn a_literal_is_told_from_a_summary_that_merely_describes() {
        for literal in ["\"text\"", "\"\"", "'c'", "  \"padded\"  ", "\"cut off\"...", "\"cut\"\u{2026}"] {
            assert!(is_literal(literal), "{literal:?}");
        }
        for container in ["size=3", "strong=2, weak=0", "{4 fields}", "Some(3)", "42", "", "\"", "'"] {
            assert!(!is_literal(container), "{container:?}");
        }
        // A string whose own last character is a full stop is not a truncation marker.
        assert!(is_literal("\"end.\""));
    }

    /// A long summary rides on the opening line as a comment, so it has to be bounded — an
    /// unbounded one pushes the structure it annotates off the right of the screen, which is half of
    /// what the path report was about.
    #[test]
    fn a_long_summary_comment_is_cut() {
        let long = format!("strong={}", "9".repeat(MAX_SUMMARY * 2));
        let text = informative_summary(&leaf_value("p", "", &long), "Rc(").unwrap();
        assert!(text.chars().count() <= MAX_SUMMARY + 1, "{text}");
        assert!(text.ends_with('\u{2026}'), "{text}");
    }

    /// The three distinctions JSON throws away and RON keeps.
    #[test]
    fn a_sequence_a_tuple_and_a_variant_are_each_shaped_as_themselves() {
        let fake = Fake(HashMap::from([
            (
                "v".to_string(),
                vec![leaf_value("[0]", "i32", "1"), leaf_value("[1]", "i32", "2")],
            ),
            (
                "t".to_string(),
                vec![leaf_value("0", "i32", "7"), leaf_value("1", "&str", "\"x\"")],
            ),
            ("o".to_string(), vec![leaf_value("0", "u32", "3")]),
        ]));

        // A sequence: brackets, no indices — position already says which is which.
        let seq = render(&parent("v", "alloc::vec::Vec<i32>", "size=2", "v"), &mut fake.expander());
        assert_eq!(seq.text, "// v: alloc::vec::Vec<i32>\n[\n    1,\n    2,\n]\n");

        // A tuple: parentheses, no field numbers.
        let tuple = render(&parent("t", "(i32, &str)", "", "t"), &mut fake.expander());
        assert_eq!(tuple.text, "// t: (i32, &str)\n(\n    7,\n    \"x\",\n)\n");

        // An enum: the VARIANT the debugger printed, not the declared type. `Option(…)` would be
        // the less specific of the two truths.
        let enumeration =
            render(&parent("o", "core::option::Option<u32>", "Some(3)", "o"), &mut fake.expander());
        assert!(enumeration.text.contains("Some(\n"), "{}", enumeration.text);
        assert!(!enumeration.text.contains("Option("), "{}", enumeration.text);
    }

    /// `// size=2` above two visible elements is noise; an enum's variant is not.
    #[test]
    fn a_summary_survives_only_when_it_adds_something() {
        assert_eq!(informative_summary(&leaf_value("v", "", "size=3"), "["), None);
        assert_eq!(informative_summary(&leaf_value("v", "", "{4 fields}"), "Order("), None);
        assert_eq!(informative_summary(&leaf_value("v", "", "Order("), "Order("), None);
        assert_eq!(informative_summary(&leaf_value("v", "", ""), "Order("), None);
        // A pointer's address, a smart pointer's strong count, a panic message — kept.
        assert_eq!(
            informative_summary(&leaf_value("p", "", "strong=2, weak=0"), "Rc("),
            Some("strong=2, weak=0".to_string())
        );
    }

    /// A value inside itself. Without the guard this is a stack overflow in the backend, which takes
    /// the whole editor's debugger with it.
    #[test]
    fn a_cycle_is_named_rather_than_followed() {
        let fake = Fake(HashMap::from([
            ("a".to_string(), vec![parent("next", "Node", "", "b")]),
            ("b".to_string(), vec![parent("prev", "Node", "", "a")]),
        ]));
        let dump = render(&parent("head", "Node", "", "a"), &mut fake.expander());
        assert!(dump.text.contains("a cycle"), "{}", dump.text);
        // …and it terminated.
        assert!(dump.nodes < 10, "{}", dump.nodes);
    }

    /// A container wider than the cap says so. A list cut at 200 without a word reads as a list of
    /// 200, and would be quoted as one.
    #[test]
    fn a_container_past_the_cap_says_how_much_was_left() {
        let wide: Vec<DebugValue> =
            (0..MAX_CHILDREN + 40).map(|i| leaf_value(&format!("[{i}]"), "u8", "0")).collect();
        let fake = Fake(HashMap::from([("w".to_string(), wide)]));
        let dump = render(&parent("w", "Vec<u8>", "", "w"), &mut fake.expander());
        assert!(dump.text.contains("… 40 more"), "{}", dump.text);
        assert!(dump.truncated);
    }

    /// Depth is a hard stop, and the row where it stopped says it was not expanded — rather than
    /// looking like a leaf, which would read as "this field is empty".
    #[test]
    fn the_depth_cap_marks_where_it_stopped() {
        // A chain longer than the cap, each link pointing at the next.
        let mut map = HashMap::new();
        for i in 0..MAX_DEPTH + 5 {
            map.insert(i.to_string(), vec![parent("next", "Link", "", &(i + 1).to_string())]);
        }
        let fake = Fake(map);
        let dump = render(&parent("head", "Link", "", "0"), &mut fake.expander());
        assert!(dump.truncated);
        assert!(dump.text.contains("not expanded"), "{}", dump.text);
    }

    /// A handle the session refuses — a stale one, or a field the debugger cannot read. The reason is
    /// the interesting part, so it goes where the value would have been instead of failing the dump.
    #[test]
    fn a_refused_handle_puts_its_reason_in_place_of_the_value() {
        let fake = Fake(HashMap::new());
        let dump = render(&parent("x", "Order", "", "stale"), &mut fake.expander());
        assert!(dump.text.contains("no such handle stale"), "{}", dump.text);
    }

    #[test]
    fn a_type_keeps_its_parameters_and_loses_its_module_path() {
        assert_eq!(simple_type("geode::mine::Order"), "Order");
        assert_eq!(simple_type("alloc::vec::Vec<geode::Order>"), "Vec<geode::Order>");
        assert_eq!(simple_type("i32"), "i32");
        assert_eq!(simple_type(""), "");
        assert_eq!(simple_type("java.util.ArrayList"), "java.util.ArrayList");
        // Not an identifier path: there is no name to put in front of a parenthesis, and
        // `(i32, &str)(…)` would be worse than `(…)`.
        assert_eq!(simple_type("(i32, &str)"), "");
        assert_eq!(simple_type("&[u8]"), "");
        assert_eq!(simple_type("*mut c_void"), "");
    }

    /// A rendered value is a variant only when it really looks like one. Getting this wrong in the
    /// other direction would put `42` or a quoted string where a struct name goes.
    #[test]
    fn a_variant_is_told_from_a_number_and_from_prose() {
        assert_eq!(variant_of("Some(3)"), Some("Some".to_string()));
        assert_eq!(variant_of("Ok(())"), Some("Ok".to_string()));
        assert_eq!(variant_of("geode::Kind::Mine { depth: 3 }"), Some("Mine".to_string()));
        assert_eq!(variant_of("42"), None);
        assert_eq!(variant_of("\"text\""), None);
        assert_eq!(variant_of("{4 fields}"), None);
        assert_eq!(variant_of("0x7ffee3a0"), None);
        assert_eq!(variant_of(""), None);
        assert_eq!(variant_of("size=3"), None);
    }

    /// A leaf the debugger rendered as nothing at all. `()` rather than an empty line, which would
    /// read as a missing field.
    #[test]
    fn a_value_the_debugger_could_not_render_is_the_unit() {
        let fake = Fake(HashMap::new());
        let dump = render(&leaf_value("x", "()", "   "), &mut fake.expander());
        assert!(dump.text.ends_with("()\n"), "{}", dump.text);
    }
}
