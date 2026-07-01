//! Structured TOML mutations — the cursor walker + `toml_edit` ops,
//! lifted verbatim from the launcher's `toml_studio/mod.rs` (only the
//! error type changed: `AppError::Other(..)` → `StudioError::App(..)`).
//!
//! The public entry point is [`mutate`]: it lowers one
//! [`SimpleMutation`] to the matching `toml_edit` op against a cloned
//! `DocumentMut`, re-emits, and re-parse-validates (rejecting a mutation
//! that produced invalid TOML — exactly the pre-extraction `mutate_with`
//! contract). All decor (comments / whitespace / quote style) survives
//! because `toml_edit` re-emits untouched spans verbatim.

use arbor_studio_core::prelude::{SimpleMutation, StudioError, StudioResult};
use serde_json::Value;
use toml_edit::{DocumentMut, InlineTable, Item, Table, Value as TomlValue};

use crate::project;

/// Apply one structured mutation to `text` and return the new text.
pub fn mutate(text: &str, mutation: SimpleMutation) -> StudioResult<String> {
    match mutation {
        SimpleMutation::SetPrimitive { path, value } => {
            mutate_with(text, |doc| {
                let target = resolve_cursor_mut(CursorMut::Item(doc.as_item_mut()), &path)
                    .ok_or_else(|| err(format!("Path not found: {path:?}")))?;
                let new_val = json_value_to_toml_value(&value).ok_or_else(|| {
                    err("Cannot set primitive — value is not a scalar TOML type")
                })?;
                set_primitive_at(target, new_val)
            })
        }
        SimpleMutation::ReplaceAt { path, text: snippet } => {
            mutate_with(text, |doc| {
                let new_item = parse_snippet_item(&snippet)?;
                let target = resolve_cursor_mut(CursorMut::Item(doc.as_item_mut()), &path)
                    .ok_or_else(|| err(format!("Path not found: {path:?}")))?;
                replace_at_cursor(target, new_item)
            })
        }
        SimpleMutation::RemoveAt { path } => {
            if path.is_empty() {
                return Err(err("Cannot remove document root"));
            }
            mutate_with(text, |doc| {
                let (parent_path, last) = path.split_at(path.len() - 1);
                let parent = resolve_cursor_mut(CursorMut::Item(doc.as_item_mut()), parent_path)
                    .ok_or_else(|| err(format!("Parent path not found: {parent_path:?}")))?;
                remove_child_at(parent, &last[0])
            })
        }
        SimpleMutation::InsertField { path, name, text: snippet } => {
            mutate_with(text, |doc| {
                let new_item = parse_snippet_item(&snippet)?;
                let target = resolve_cursor_mut(CursorMut::Item(doc.as_item_mut()), &path)
                    .ok_or_else(|| err(format!("Path not found: {path:?}")))?;
                insert_field_at(target, &name, new_item)
            })
        }
        SimpleMutation::InsertItem { path, text: snippet } => {
            mutate_with(text, |doc| {
                let new_item = parse_snippet_item(&snippet)?;
                let target = resolve_cursor_mut(CursorMut::Item(doc.as_item_mut()), &path)
                    .ok_or_else(|| err(format!("Path not found: {path:?}")))?;
                insert_item_at(target, new_item)
            })
        }
        // TOML maps and tables are the same construct — `insert_map_entry`
        // delegates to `insert_field` semantics (the key is the literal
        // string; `toml_edit` quotes it on serialisation when necessary).
        SimpleMutation::InsertMapEntry { path, key_text, val_text } => {
            mutate_with(text, |doc| {
                let new_item = parse_snippet_item(&val_text)?;
                let target = resolve_cursor_mut(CursorMut::Item(doc.as_item_mut()), &path)
                    .ok_or_else(|| err(format!("Path not found: {path:?}")))?;
                insert_field_at(target, &key_text, new_item)
            })
        }
        SimpleMutation::DuplicateAt { path } => {
            if path.is_empty() {
                return Err(err("Cannot duplicate document root"));
            }
            mutate_with(text, |doc| {
                let src_cursor = resolve_cursor(Cursor::Item(doc.as_item()), &path)
                    .ok_or_else(|| err(format!("Path not found: {path:?}")))?;
                let src_item = cursor_to_owned_item(src_cursor);
                let (parent_path, last) = path.split_at(path.len() - 1);
                let parent = resolve_cursor_mut(CursorMut::Item(doc.as_item_mut()), parent_path)
                    .ok_or_else(|| err(format!("Parent path not found: {parent_path:?}")))?;
                duplicate_at_cursor(parent, &last[0], src_item)
            })
        }
        SimpleMutation::MoveItem { path, delta } => {
            if path.is_empty() {
                return Err(err("Cannot move document root"));
            }
            mutate_with(text, |doc| {
                let (parent_path, last) = path.split_at(path.len() - 1);
                let key = &last[0];
                let parent = resolve_cursor_mut(CursorMut::Item(doc.as_item_mut()), parent_path)
                    .ok_or_else(|| err(format!("Parent path not found: {parent_path:?}")))?;
                move_at_cursor(parent, key, delta)
            })
        }
    }
}

fn err(message: impl Into<String>) -> StudioError {
    StudioError::App(message.into())
}

/// Parse + re-emit harness: parse `text`, run `op` on the doc, emit, then
/// re-parse the result to reject a mutation that produced invalid TOML.
fn mutate_with<F>(text: &str, op: F) -> StudioResult<String>
where
    F: FnOnce(&mut DocumentMut) -> StudioResult<()>,
{
    let mut working: DocumentMut = text
        .parse()
        .map_err(|e| err(format!("Document has parse errors — cannot edit tree: {e}")))?;
    op(&mut working)?;
    let new_text = working.to_string();
    // Re-parse so a mutation that produced invalid TOML never escapes.
    let (_doc, _value, parse_error) = project::parse_pair(&new_text);
    if let Some(e) = parse_error {
        return Err(err(format!("Mutation produced invalid TOML: {e}")));
    }
    Ok(new_text)
}

/// Parse a snippet (TOML value RHS) into an owned `Item` by wrapping it in
/// a throwaway assignment. Lets the user paste a struct / array / inline
/// table without learning a new mini-grammar.
fn parse_snippet_item(snippet: &str) -> StudioResult<Item> {
    let parsed: DocumentMut = format!("__arbor_tmp__ = {snippet}\n")
        .parse()
        .map_err(|e| err(format!("Invalid TOML snippet: {e}")))?;
    parsed
        .get("__arbor_tmp__")
        .cloned()
        .ok_or_else(|| err("Snippet parse: missing value"))
}

// ── Cursor — navigates Items + nested Values + tables-in-AoT ───────────

#[derive(Clone, Copy)]
enum Cursor<'a> {
    Item(&'a Item),
    Table(&'a Table),
    Value(&'a TomlValue),
}

fn resolve_cursor<'a>(start: Cursor<'a>, path: &[String]) -> Option<Cursor<'a>> {
    let mut cur = start;
    for seg in path {
        cur = step_cursor(cur, seg)?;
    }
    Some(cur)
}

fn step_cursor<'a>(c: Cursor<'a>, seg: &str) -> Option<Cursor<'a>> {
    match c {
        Cursor::Item(Item::Table(t)) => step_table(t, seg),
        Cursor::Item(Item::ArrayOfTables(arr)) => {
            let i: usize = seg.parse().ok()?;
            arr.get(i).map(Cursor::Table)
        }
        Cursor::Item(Item::Value(v)) => step_value(v, seg),
        Cursor::Item(Item::None) => None,
        Cursor::Table(t) => step_table(t, seg),
        Cursor::Value(v) => step_value(v, seg),
    }
}

fn step_table<'a>(t: &'a Table, seg: &str) -> Option<Cursor<'a>> {
    t.get(seg).map(Cursor::Item)
}

fn step_value<'a>(v: &'a TomlValue, seg: &str) -> Option<Cursor<'a>> {
    match v {
        TomlValue::Array(arr) => {
            let i: usize = seg.parse().ok()?;
            arr.get(i).map(Cursor::Value)
        }
        TomlValue::InlineTable(t) => t.get(seg).map(Cursor::Value),
        _ => None,
    }
}

/// Clone the AST node a `Cursor` points at into an owned `Item`.
fn cursor_to_owned_item(c: Cursor<'_>) -> Item {
    match c {
        Cursor::Item(item) => item.clone(),
        Cursor::Table(t)   => Item::Table(t.clone()),
        Cursor::Value(v)   => Item::Value(v.clone()),
    }
}

// ── Mutable cursor ─────────────────────────────────────────────────────

enum CursorMut<'a> {
    Item(&'a mut Item),
    Table(&'a mut Table),
    Value(&'a mut TomlValue),
}

fn resolve_cursor_mut<'a>(start: CursorMut<'a>, path: &[String]) -> Option<CursorMut<'a>> {
    let mut cur = start;
    for seg in path {
        cur = step_cursor_mut(cur, seg)?;
    }
    Some(cur)
}

fn step_cursor_mut<'a>(c: CursorMut<'a>, seg: &str) -> Option<CursorMut<'a>> {
    match c {
        CursorMut::Item(item) => match item {
            Item::Table(t)         => t.get_mut(seg).map(CursorMut::Item),
            Item::ArrayOfTables(a) => {
                let i: usize = seg.parse().ok()?;
                a.get_mut(i).map(CursorMut::Table)
            }
            Item::Value(v) => step_value_mut(v, seg),
            Item::None     => None,
        },
        CursorMut::Table(t) => t.get_mut(seg).map(CursorMut::Item),
        CursorMut::Value(v) => step_value_mut(v, seg),
    }
}

fn step_value_mut<'a>(v: &'a mut TomlValue, seg: &str) -> Option<CursorMut<'a>> {
    match v {
        TomlValue::Array(arr) => {
            let i: usize = seg.parse().ok()?;
            arr.get_mut(i).map(CursorMut::Value)
        }
        TomlValue::InlineTable(t) => t.get_mut(seg).map(CursorMut::Value),
        _ => None,
    }
}

// ── Mutation helpers ───────────────────────────────────────────────────

/// Convert a `serde_json::Value` into a `toml_edit::Value`. Returns
/// `None` for nulls and containers — `set_primitive` is for scalar leaves
/// only. Unwraps the FE-tagged `{type, value}` form first.
fn json_value_to_toml_value(v: &Value) -> Option<TomlValue> {
    let unwrapped: Value;
    let v = if let Value::Object(map) = v {
        if map.len() == 2 && map.contains_key("type") && map.contains_key("value") {
            unwrapped = map.get("value").cloned().unwrap_or(Value::Null);
            &unwrapped
        } else {
            v
        }
    } else {
        v
    };
    match v {
        Value::Bool(b)   => Some(TomlValue::from(*b)),
        Value::String(s) => Some(TomlValue::from(s.as_str())),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Some(TomlValue::from(i))
            } else {
                n.as_f64().map(TomlValue::from)
            }
        }
        // TOML has no null; `null_handling = AsDelete` lives at the
        // descriptor level. Mutating a scalar to null is rejected here.
        Value::Null => None,
        Value::Array(_) | Value::Object(_) => None,
    }
}

fn set_primitive_at(target: CursorMut<'_>, new_val: TomlValue) -> StudioResult<()> {
    match target {
        CursorMut::Item(item) => {
            let decor = match item {
                Item::Value(v) => Some(v.decor().clone()),
                _ => None,
            };
            let mut nv = new_val;
            if let Some(d) = decor {
                *nv.decor_mut() = d;
            }
            *item = Item::Value(nv);
            Ok(())
        }
        CursorMut::Value(v) => {
            let decor = v.decor().clone();
            let mut nv = new_val;
            *nv.decor_mut() = decor;
            *v = nv;
            Ok(())
        }
        CursorMut::Table(_) => Err(err("Cannot set a primitive on a table node")),
    }
}

fn replace_at_cursor(target: CursorMut<'_>, new_item: Item) -> StudioResult<()> {
    match target {
        CursorMut::Item(item) => {
            *item = new_item;
            Ok(())
        }
        CursorMut::Value(v) => {
            *v = item_to_inline_value(new_item)?;
            Ok(())
        }
        CursorMut::Table(_) => Err(err(
            "Cannot replace an array-of-tables entry as a whole — descend into a field instead",
        )),
    }
}

/// Convert an `Item` into a `Value` for placement inside an inline table
/// or value array. Block tables are demoted to inline.
fn item_to_inline_value(item: Item) -> StudioResult<TomlValue> {
    match item {
        Item::Value(v) => Ok(v),
        Item::Table(t) => {
            let mut inline = InlineTable::new();
            for (k, v) in t.iter() {
                if let Item::Value(val) = v {
                    inline.insert(k, val.clone());
                }
            }
            Ok(TomlValue::InlineTable(inline))
        }
        Item::ArrayOfTables(_) => {
            Err(err("Cannot place an array-of-tables inside a value container"))
        }
        Item::None => Err(err("Empty item — nothing to place")),
    }
}

fn insert_field_at(target: CursorMut<'_>, key: &str, value: Item) -> StudioResult<()> {
    match target {
        CursorMut::Item(Item::Table(t)) | CursorMut::Table(t) => {
            if t.contains_key(key) {
                return Err(err(format!("Key already exists: {key}")));
            }
            t.insert(key, value);
            Ok(())
        }
        CursorMut::Item(Item::Value(TomlValue::InlineTable(t)))
        | CursorMut::Value(TomlValue::InlineTable(t)) => {
            if t.contains_key(key) {
                return Err(err(format!("Key already exists: {key}")));
            }
            let v = item_to_inline_value(value)?;
            t.insert(key, v);
            Ok(())
        }
        _ => Err(err("Cannot add a field — target is not a table or inline table")),
    }
}

fn insert_item_at(target: CursorMut<'_>, value: Item) -> StudioResult<()> {
    match target {
        CursorMut::Item(Item::Value(TomlValue::Array(arr)))
        | CursorMut::Value(TomlValue::Array(arr)) => {
            let v = item_to_inline_value(value)?;
            arr.push(v);
            Ok(())
        }
        CursorMut::Item(Item::ArrayOfTables(arr)) => {
            let tbl = match value {
                Item::Table(t) => t,
                Item::Value(TomlValue::InlineTable(t)) => {
                    let mut block = Table::new();
                    for (k, v) in t.iter() {
                        block.insert(k, Item::Value(v.clone()));
                    }
                    block
                }
                _ => return Err(err("Cannot push a non-table into an array-of-tables")),
            };
            arr.push(tbl);
            Ok(())
        }
        _ => Err(err("Cannot add an item — target is not an array")),
    }
}

fn remove_child_at(parent: CursorMut<'_>, key: &str) -> StudioResult<()> {
    match parent {
        CursorMut::Item(Item::Table(t)) | CursorMut::Table(t) => t
            .remove(key)
            .ok_or_else(|| err(format!("Key not found: {key}")))
            .map(|_| ()),
        CursorMut::Item(Item::Value(TomlValue::InlineTable(t)))
        | CursorMut::Value(TomlValue::InlineTable(t)) => t
            .remove(key)
            .ok_or_else(|| err(format!("Key not found: {key}")))
            .map(|_| ()),
        CursorMut::Item(Item::Value(TomlValue::Array(arr)))
        | CursorMut::Value(TomlValue::Array(arr)) => {
            let i: usize = key
                .parse()
                .map_err(|_| err(format!("Invalid array index: {key}")))?;
            if i >= arr.len() {
                return Err(err(format!("Array index out of bounds: {i}")));
            }
            arr.remove(i);
            Ok(())
        }
        CursorMut::Item(Item::ArrayOfTables(arr)) => {
            let i: usize = key
                .parse()
                .map_err(|_| err(format!("Invalid array index: {key}")))?;
            if i >= arr.len() {
                return Err(err(format!("Array index out of bounds: {i}")));
            }
            arr.remove(i);
            Ok(())
        }
        _ => Err(err("Parent is not a container")),
    }
}

fn duplicate_at_cursor(parent: CursorMut<'_>, key: &str, source: Item) -> StudioResult<()> {
    match parent {
        CursorMut::Item(Item::Table(t)) | CursorMut::Table(t) => {
            let next_key = next_copy_key(key, |k| t.contains_key(k));
            t.insert(&next_key, source);
            Ok(())
        }
        CursorMut::Item(Item::Value(TomlValue::InlineTable(t)))
        | CursorMut::Value(TomlValue::InlineTable(t)) => {
            let next_key = next_copy_key(key, |k| t.contains_key(k));
            let v = item_to_inline_value(source)?;
            t.insert(&next_key, v);
            Ok(())
        }
        CursorMut::Item(Item::Value(TomlValue::Array(arr)))
        | CursorMut::Value(TomlValue::Array(arr)) => {
            let i: usize = key
                .parse()
                .map_err(|_| err(format!("Invalid array index: {key}")))?;
            if i >= arr.len() {
                return Err(err(format!("Array index out of bounds: {i}")));
            }
            let v = item_to_inline_value(source)?;
            arr.insert(i + 1, v);
            Ok(())
        }
        CursorMut::Item(Item::ArrayOfTables(arr)) => {
            let i: usize = key
                .parse()
                .map_err(|_| err(format!("Invalid array index: {key}")))?;
            if i >= arr.len() {
                return Err(err(format!("Array index out of bounds: {i}")));
            }
            let tbl = match source {
                Item::Table(t) => t,
                _ => return Err(err("Cannot duplicate non-table entry in array-of-tables")),
            };
            // `ArrayOfTables` exposes no `insert(usize, Table)`; clone the
            // contents, clear, splice the duplicate, push everything back.
            let mut all: Vec<Table> = arr.iter().cloned().collect();
            while !arr.is_empty() {
                arr.remove(0);
            }
            all.insert(i + 1, tbl);
            for t in all {
                arr.push(t);
            }
            Ok(())
        }
        _ => Err(err("Parent is not a container")),
    }
}

fn next_copy_key(key: &str, exists: impl Fn(&str) -> bool) -> String {
    let mut next_key = format!("{key}_copy");
    let mut n = 2;
    while exists(&next_key) {
        next_key = format!("{key}_copy{n}");
        n += 1;
    }
    next_key
}

fn move_at_cursor(parent: CursorMut<'_>, key: &str, delta: i32) -> StudioResult<()> {
    match parent {
        CursorMut::Item(Item::Value(TomlValue::Array(arr)))
        | CursorMut::Value(TomlValue::Array(arr)) => {
            let i: usize = key
                .parse()
                .map_err(|_| err(format!("Invalid array index: {key}")))?;
            let new_i = (i as i32 + delta).max(0) as usize;
            let new_i = new_i.min(arr.len().saturating_sub(1));
            if new_i == i {
                return Ok(());
            }
            let item = arr.remove(i);
            arr.insert(new_i, item);
            Ok(())
        }
        CursorMut::Item(Item::ArrayOfTables(arr)) => {
            let i: usize = key
                .parse()
                .map_err(|_| err(format!("Invalid array index: {key}")))?;
            let new_i = (i as i32 + delta).max(0) as usize;
            let new_i = new_i.min(arr.len().saturating_sub(1));
            if new_i == i {
                return Ok(());
            }
            let mut all: Vec<Table> = arr.iter().cloned().collect();
            while !arr.is_empty() {
                arr.remove(0);
            }
            let item = all.remove(i);
            all.insert(new_i, item);
            for t in all {
                arr.push(t);
            }
            Ok(())
        }
        _ => Err(err("Cannot move — parent is not an ordered container")),
    }
}
