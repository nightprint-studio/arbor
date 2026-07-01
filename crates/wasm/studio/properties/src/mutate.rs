//! Structured `.properties` mutations — lowers one [`SimpleMutation`] to
//! the line-edit primitives in [`crate::line_model`], re-emitting the
//! buffer.
//!
//! Text-in / text-out (the `SimpleFormat` mutation seam): parse the line
//! view, apply the op, re-emit. Lifted from the launcher's
//! `properties_studio/mod.rs` `mutate_*` registry methods, minus the doc
//! bookkeeping (which `DefaultBackend` owns).

use arbor_studio_core::prelude::{SimpleMutation, StudioError, StudioResult};

use crate::line_model::{
    self as line, emit_lines, parse_lines, primitive_to_string,
};

/// Apply one structured mutation to `text` and return the new text.
pub fn mutate(text: &str, mutation: SimpleMutation) -> StudioResult<String> {
    let mut lines = parse_lines(text);
    match mutation {
        SimpleMutation::SetPrimitive { path, value } => {
            let new_str = primitive_to_string(&value);
            line::set_value_at_path(&mut lines, &path, &new_str)?;
        }
        SimpleMutation::ReplaceAt { path, text: snippet } => {
            // `.properties` has no nested snippet syntax; replace_at is
            // identical to set_primitive with the snippet as raw string.
            line::set_value_at_path(&mut lines, &path, &snippet)?;
        }
        SimpleMutation::RemoveAt { path } => {
            if path.is_empty() {
                return Err(StudioError::App("Cannot remove document root".into()));
            }
            line::remove_at_path(&mut lines, &path)?;
        }
        SimpleMutation::InsertField { path, name, text: snippet } => {
            let mut full = path.clone();
            full.push(name);
            line::insert_or_set(&mut lines, &full, &snippet)?;
        }
        SimpleMutation::InsertItem { path, text: snippet } => {
            // Append at the next index under `path`. Use Spring `[N]`
            // bracket notation so the assembled tree treats it as an
            // array index.
            let mut next = path.clone();
            let n = line::next_array_index_under(&lines, &path);
            // Encode array index into the LAST existing segment when
            // present (Spring style: `path = ["servers"]` + idx 0
            // → key `servers[0]`); otherwise create a synthetic
            // bracketed segment at root level.
            if let Some(last) = next.last_mut() {
                *last = format!("{last}[{n}]");
            } else {
                next.push(format!("[{n}]"));
            }
            line::insert_or_set(&mut lines, &next, &snippet)?;
        }
        SimpleMutation::InsertMapEntry { path, key_text, val_text } => {
            // Maps and "fields" are interchangeable in `.properties`.
            let mut full = path.clone();
            full.push(key_text);
            line::insert_or_set(&mut lines, &full, &val_text)?;
        }
        SimpleMutation::DuplicateAt { path } => {
            if path.is_empty() {
                return Err(StudioError::App("Cannot duplicate document root".into()));
            }
            line::duplicate_at_path(&mut lines, &path)?;
        }
        SimpleMutation::MoveItem { path, delta } => {
            if path.is_empty() {
                return Err(StudioError::App("Cannot move document root".into()));
            }
            line::move_at_path(&mut lines, &path, delta)?;
        }
    }
    Ok(emit_lines(&lines))
}
