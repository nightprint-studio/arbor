//! What a project creates under a name: `bao` is a `ByteArrayOutputStream` in dozens of places, so `val bao = new B|` is
//! offered `ByteArrayOutputStream()` — in the ghost text, and at the top of the popup, whatever the typed start says.
//!
//! Counts only, per file so a saved file replaces its own: for every `name = new Type(…)` (a declaration, with a type, `var`
//! or Lombok's `val`, or a plain assignment) the pair `(name, Type)`. A type is offered for a name when it holds a clear
//! majority of the sightings, and never for a name that has been seen creating several types about equally. Only simple,
//! unqualified types, with their generics left off: `new ArrayList<>()` counts as `ArrayList`.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};

use bennu_java::prelude::parse_java;
use bennu_proto::prelude::CompletionItem;
use tree_sitter::Node;

use crate::flc_memory::{text as bytes_of, two_level, Sizes};

const MIN_SIGHTINGS: u32 = 4;
const MIN_SHARE: f32 = 0.7;
const MIN_NAME: usize = 2;
/// Names kept; past it a file read later is not counted.
const MAX_NAMES: usize = 20_000;

/// A creation to draw after the caret, and how sure that is.
pub(crate) struct Created {
    pub text: String,
    pub confidence: f32,
}

#[derive(Default)]
struct Held {
    by_file: HashMap<String, Vec<(String, String)>>,
    /// variable name → type created → how often
    table: HashMap<String, HashMap<String, u32>>,
}

fn lock() -> MutexGuard<'static, Held> {
    static HELD: OnceLock<Mutex<Held>> = OnceLock::new();
    HELD.get_or_init(Mutex::default).lock().unwrap_or_else(|p| p.into_inner())
}

fn is_ident(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'$')
}

fn text_of<'a>(node: Node<'_>, source: &'a str) -> Option<&'a str> {
    node.utf8_text(source.as_bytes()).ok()
}

/// The type a `new` expression makes, simple and without generics; `None` for an anonymous class or a qualified name.
fn created_type(node: Node<'_>, source: &str) -> Option<String> {
    if node.kind() != "object_creation_expression" {
        return None;
    }
    let mut cursor = node.walk();
    if node.children(&mut cursor).any(|c| c.kind() == "class_body") {
        return None;
    }
    let ty = text_of(node.child_by_field_name("type")?, source)?;
    let base = ty.split('<').next().unwrap_or(ty).trim();
    (base.starts_with(|c: char| c.is_ascii_uppercase()) && is_ident(base)).then(|| base.to_string())
}

/// Every `(name, Type)` where a variable is made with `new Type(…)` in `source`.
fn creations(source: &str) -> Vec<(String, String)> {
    let Some(tree) = parse_java(source) else { return Vec::new() };
    let mut out = Vec::new();
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        match node.kind() {
            "variable_declarator" => {
                let name = node.child_by_field_name("name").and_then(|n| text_of(n, source));
                let made = node.child_by_field_name("value").and_then(|v| created_type(v, source));
                if let (Some(name), Some(made)) = (name, made) {
                    out.push((name.to_string(), made));
                }
            }
            "assignment_expression" => {
                let left = node.child_by_field_name("left").filter(|l| l.kind() == "identifier").and_then(|l| text_of(l, source));
                let made = node.child_by_field_name("right").and_then(|v| created_type(v, source));
                if let (Some(name), Some(made)) = (left, made) {
                    out.push((name.to_string(), made));
                }
            }
            _ => {}
        }
        let mut cursor = node.walk();
        stack.extend(node.children(&mut cursor));
    }
    out.retain(|(name, _)| name.len() >= MIN_NAME && name.starts_with(|c: char| c.is_ascii_lowercase()) && is_ident(name));
    // counted under the words of the name, not its spelling: `jaxbProcessor` and `jaxb_processor` are one name
    out.into_iter().map(|(name, made)| (crate::flc_words::canon(&name), made)).collect()
}

pub(crate) fn add_file(file: &str, source: &str) {
    let found = creations(source);
    let mut h = lock();
    remove_locked(&mut h, file);
    for (name, made) in &found {
        if h.table.len() >= MAX_NAMES && !h.table.contains_key(name) {
            continue;
        }
        *h.table.entry(name.clone()).or_default().entry(made.clone()).or_insert(0) += 1;
    }
    h.by_file.insert(file.to_string(), found);
}

pub(crate) fn remove_file(file: &str) {
    remove_locked(&mut lock(), file);
}

fn remove_locked(h: &mut Held, file: &str) {
    for (name, made) in h.by_file.remove(file).unwrap_or_default() {
        let Some(row) = h.table.get_mut(&name) else { continue };
        if let Some(n) = row.get_mut(&made) {
            *n -= 1;
            if *n == 0 {
                row.remove(&made);
            }
        }
        if row.is_empty() {
            h.table.remove(&name);
        }
    }
}

pub(crate) fn clear() {
    *lock() = Held::default();
}

/// The type this project creates under `name`, with the share of the sightings it holds and how many there were.
fn dominant(name: &str) -> Option<(String, f32, u32)> {
    let name = &crate::flc_words::canon(name);
    let h = lock();
    let row = h.table.get(name)?;
    let mut seen: Vec<(&String, u32)> = row.iter().filter(|(t, _)| !crate::learned_rules::hidden("created", name, t)).map(|(t, n)| (t, *n)).collect();
    seen.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    let total: u32 = seen.iter().map(|(_, n)| n).sum();
    let (top, count) = seen.first().map(|(t, n)| ((*t).clone(), *n))?;
    let share = count as f32 / total as f32;
    (count >= MIN_SIGHTINGS && share >= MIN_SHARE).then_some((top, share, count))
}

/// Where the caret stands in `name = new Typed|`: the target name, what of the type is typed, and the declared type when
/// there is one that is not `var`/`val`.
struct Site {
    name: String,
    typed: String,
    declared: Option<String>,
}

fn site(line_prefix: &str) -> Option<Site> {
    let (head, typed) = line_prefix.rsplit_once("new ")?;
    if !(typed.is_empty() || is_ident(typed)) || head.ends_with(|c: char| c.is_alphanumeric() || matches!(c, '_' | '$' | '.')) {
        return None;
    }
    let lhs = head.trim_end().strip_suffix('=')?;
    if lhs.ends_with(['=', '!', '<', '>', '+', '-', '*', '/', '&', '|', '^', '%']) {
        return None;
    }
    let lhs = lhs.trim_end();
    let name_at = lhs.rfind(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$')).map_or(0, |i| i + 1);
    let name = &lhs[name_at..];
    if !is_ident(name) || lhs[..name_at].ends_with('.') {
        return None;
    }
    let before = lhs[..name_at].trim();
    let before = before.strip_prefix("final ").unwrap_or(before).trim();
    let declared = match before {
        "" | "var" | "val" => None,
        t => Some(t.split('<').next().unwrap_or(t).trim().to_string()),
    };
    Some(Site { name: name.to_string(), typed: typed.to_string(), declared })
}

/// The creation to draw after `val bao = new B|`: the rest of the type the project makes under that name, its parentheses
/// and the end of the statement.
pub(crate) fn suggest(line_prefix: &str) -> Option<Created> {
    let Some(at) = site(line_prefix) else { return declaration(line_prefix) };
    let (made, share, _) = dominant(&at.name)?;
    // a declared type has to be the one created: `OutputStream o = new …` is for the layers that know its subtypes
    if at.declared.as_deref().is_some_and(|d| d != made) || !made.starts_with(at.typed.as_str()) {
        return None;
    }
    let rest = &made[at.typed.len()..];
    Some(Created { text: format!("{rest}();"), confidence: share.min(0.95) })
}

/// The whole of ` = new Type();` after the name of a declaration that has just been written (`val bao|`, `var bao `,
/// `ByteArrayOutputStream bao|`): the name alone says what the project makes under it.
fn declaration(line_prefix: &str) -> Option<Created> {
    const MODIFIERS: [&str; 5] = ["private", "protected", "public", "static", "final"];
    let tokens: Vec<&str> = line_prefix.split_whitespace().skip_while(|t| MODIFIERS.contains(t)).collect();
    let [ty, name] = tokens[..] else { return None };
    if !is_ident(name) || !name.starts_with(|c: char| c.is_ascii_lowercase()) || !line_prefix.trim_end().ends_with(name) {
        return None;
    }
    let (made, share, _) = dominant(name)?;
    if !matches!(ty, "var" | "val") && ty.split('<').next() != Some(made.as_str()) {
        return None;
    }
    let lead = if line_prefix.ends_with(char::is_whitespace) { "" } else { " " };
    Some(Created { text: format!("{lead}= new {made}();"), confidence: share.min(0.95) })
}

/// Puts the type this project creates under the declared name at the head of the popup after `new`: the row the provider
/// already has, or — when the typed start is too short for it to be among the rows — the one it gives for the whole name.
/// `rows_for` answers the provider's rows for the buffer it is given.
pub(crate) fn lift(
    source: &str,
    offset: usize,
    items: &mut Vec<CompletionItem>,
    rows_for: &dyn Fn(&str, usize) -> Vec<CompletionItem>,
) {
    let line_start = source[..offset.min(source.len())].rfind('\n').map_or(0, |p| p + 1);
    let Some(at) = source.get(line_start..offset).and_then(site) else { return };
    let Some((made, _, _)) = dominant(&at.name) else { return };
    if at.declared.as_deref().is_some_and(|d| d != made) || !made.starts_with(at.typed.as_str()) {
        return;
    }
    let position = items.iter().position(|i| i.kind == "class" && i.label == made);
    let mut row = match position {
        Some(p) => items.remove(p),
        None => {
            let widened = format!("{}{}{}", &source[..offset], &made[at.typed.len()..], &source[offset..]);
            match rows_for(&widened, offset + made.len() - at.typed.len()).into_iter().find(|i| i.kind == "class" && i.label == made) {
                Some(r) => r,
                None => return,
            }
        }
    };
    row.preselect = true;
    items.insert(0, row);
}

/// What the page that shows it lists: per variable name, the types made under it and how often.
pub(crate) fn report() -> Vec<(String, Vec<(String, u32)>)> {
    let h = lock();
    let mut out: Vec<(String, Vec<(String, u32)>)> = h
        .table
        .iter()
        .map(|(name, row)| {
            let mut rows: Vec<(String, u32)> = row.iter().map(|(t, n)| (t.clone(), *n)).collect();
            rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            (name.clone(), rows)
        })
        .filter(|(_, rows)| rows.iter().map(|r| r.1).sum::<u32>() >= MIN_SIGHTINGS)
        .collect();
    out.sort_by_key(|(name, rows)| (std::cmp::Reverse(rows.iter().map(|r| r.1).sum::<u32>()), name.clone()));
    out
}

pub(crate) fn tracked() -> (usize, usize) {
    let h = lock();
    (h.by_file.len(), h.table.len())
}

pub(crate) fn sizes() -> Sizes {
    let h = lock();
    let mut s = Sizes::default();
    for (k, v) in &h.by_file {
        s.add_file(k, v.iter().map(|(a, b)| bytes_of(a) + bytes_of(b)).sum());
    }
    s.shared = two_level(&h.table, |k| bytes_of(k));
    s.shared_entries = h.table.values().map(|r| r.len()).sum();
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn method(body: &str) -> String {
        format!("class A {{\n    void f() {{\n{body}\n    }}\n}}\n")
    }

    fn project(name: &str, ty: &str, times: usize) {
        for i in 0..times {
            add_file(&format!("created/{name}{i}.java"), &method(&format!("        val {name} = new {ty}();\n        {name}.go();")));
        }
    }

    fn forget(name: &str, times: usize) {
        (0..times).for_each(|i| remove_file(&format!("created/{name}{i}.java")));
    }

    #[test]
    fn the_same_name_in_another_case_is_the_same_name() {
        // three sightings in one style and two in the other: only together do they make a habit
        for (i, spelled) in ["zzTmpBuf", "zz_tmp_buf", "zzTmpBuf", "zz_tmp_buf", "zzTmpBuf"].iter().enumerate() {
            add_file(&format!("created/case{i}.java"), &method(&format!("        val {spelled} = new ZzTmpBuffer();")));
        }
        assert_eq!(suggest("        val zzTmpBuf = new Zz").expect("camel").text, "TmpBuffer();");
        assert_eq!(suggest("        val zz_tmp_buf = new Zz").expect("snake").text, "TmpBuffer();");
        (0..5).for_each(|i| remove_file(&format!("created/case{i}.java")));
    }

    #[test]
    fn the_type_a_project_creates_under_a_name_completes_after_new() {
        project("zz_buf", "ZzBuffer", 5);
        let got = suggest("        val zz_buf = new Zz").expect("a creation");
        assert_eq!(got.text, "Buffer();");
        assert!(got.confidence >= 0.7);
        assert_eq!(suggest("        zz_buf = new ").expect("an empty start").text, "ZzBuffer();");
        assert_eq!(suggest("        var zz_buf = new ZzBuffer").expect("a full name").text, "();");
        // the name of the declaration is enough: the rest of the statement is drawn
        assert_eq!(suggest("        val zz_buf").expect("after the name").text, " = new ZzBuffer();");
        assert_eq!(suggest("        var zz_buf ").expect("after a space").text, "= new ZzBuffer();");
        assert_eq!(suggest("        private final ZzBuffer zz_buf").expect("with the type").text, " = new ZzBuffer();");
        assert!(suggest("        Other zz_buf").is_none(), "another declared type");
        assert!(suggest("        zz_buf").is_none(), "a bare name is no declaration");
        assert!(suggest("        val zz_bu").is_none(), "an unfinished name");
        forget("zz_buf", 5);
    }

    #[test]
    fn a_name_seen_with_two_types_or_too_few_times_offers_nothing() {
        for i in 0..3 {
            add_file(&format!("created/tie_a{i}.java"), &method("        val zz_tie = new ZzOne();"));
            add_file(&format!("created/tie_b{i}.java"), &method("        val zz_tie = new ZzTwo();"));
        }
        assert!(suggest("        val zz_tie = new ").is_none(), "a tie is no habit");
        project("zz_few", "ZzFew", 3);
        assert!(suggest("        val zz_few = new ").is_none(), "too few sightings");
        for i in 0..3 {
            remove_file(&format!("created/tie_a{i}.java"));
            remove_file(&format!("created/tie_b{i}.java"));
        }
        forget("zz_few", 3);
    }

    #[test]
    fn only_a_new_after_an_equals_on_a_plain_name_counts() {
        project("zz_plain", "ZzPlain", 5);
        assert!(suggest("        foo(new ").is_none());
        assert!(suggest("        this.zz_plain = new ").is_none());
        assert!(suggest("        if (zz_plain == new ").is_none());
        assert!(suggest("        Other zz_plain = new ").is_none(), "a declared type that is not the one created");
        assert!(suggest("        ZzPlain zz_plain = new ").is_some());
        forget("zz_plain", 5);
    }

    #[test]
    fn generics_are_left_off_and_anonymous_classes_do_not_count() {
        let found = creations(&method("        var zz_list = new ZzArray<String>();\n        var zz_anon = new ZzRunner() { };\n        zz_assigned = new ZzOther<>();"));
        let mut found = found;
        found.sort();
        assert_eq!(found, [("zz_assigned".to_string(), "ZzOther".to_string()), ("zz_list".to_string(), "ZzArray".to_string())]);
    }

    fn class_row(label: &str) -> CompletionItem {
        CompletionItem { label: label.to_string(), kind: "class".to_string(), ..Default::default() }
    }

    #[test]
    fn the_type_created_under_the_name_heads_the_popup_even_when_the_provider_cut_it() {
        project("zz_pop", "ZzPopped", 5);
        let source = "class A {\n    void f() {\n        val zz_pop = new Z\n    }\n}\n";
        let offset = source.find("new Z").unwrap() + "new Z".len();
        // present among the rows: moved to the front and preselected
        let mut items = vec![class_row("ZzOther"), class_row("ZzPopped")];
        lift(source, offset, &mut items, &|_, _| Vec::new());
        assert_eq!(items[0].label, "ZzPopped");
        assert!(items[0].preselect);
        // cut by the provider: asked again for the whole name, whose row is taken
        let mut items = vec![class_row("ZzOther")];
        lift(source, offset, &mut items, &|widened, _| {
            assert!(widened.contains("new ZzPopped"));
            vec![class_row("ZzPopped")]
        });
        assert_eq!(items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(), ["ZzPopped", "ZzOther"]);
        // another name: the rows are left as they are
        let mut items = vec![class_row("ZzOther")];
        lift(&source.replace("zz_pop", "zz_nothing"), offset + 3, &mut items, &|_, _| Vec::new());
        assert_eq!(items.len(), 1);
        forget("zz_pop", 5);
    }

    #[test]
    fn a_removed_file_is_forgotten() {
        add_file("created/gone.java", &method("        val zz_gone = new ZzGone();"));
        remove_file("created/gone.java");
        assert!(lock().table.get("zz_gone").is_none());
    }
}
