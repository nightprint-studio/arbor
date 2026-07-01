//! [`YamlRefactor`] — a standalone [`RefactorOps`] for the project-wide
//! F12/F13 orchestration the CALLER drives.
//!
//! `DefaultBackend` already implements `RefactorOps` for the active-doc +
//! FS-only apply paths, but the project-wide preview/apply orchestration
//! lives at the api/launcher layer (it needs the repo scanner / index,
//! which `core` and this crate don't name). That orchestrator needs a
//! `RefactorOps` to build + apply sites against YAML files on disk; this is
//! it.
//!
//! It mirrors `DefaultBackend`'s own `RefactorOps` impl, expressed directly
//! against the crate's `mutate` / `project` / `kind` primitives so the
//! launcher can use it without owning a `DefaultBackend` doc registry.
//! Unlike TOML, YAML's `null_handling = Native`, so a `null` set stays a
//! literal write (not routed to a delete).

use arbor_studio_core::prelude::{
    refactor, BulkOp, CoerceOutcome, CoerceSkip, RefactorOps, SetValue, SimpleMutation,
    StudioError, StudioResult,
};
use serde_json::Value;

use crate::{kind, mutate, project};

/// Stateless YAML leaf operations for `core::refactor`'s project-wide
/// flows.
pub struct YamlRefactor;

impl RefactorOps for YamlRefactor {
    fn parse_to_value(&self, text: &str) -> Option<Value> {
        project::parse_to_value(text)
    }

    fn apply_string_rename(
        &self,
        text:  &str,
        paths: &[Vec<String>],
        new:   &str,
    ) -> StudioResult<String> {
        // Pre-flush validation: every site must resolve to a string leaf in
        // the projection before any mutation (atomic-by-file, FROZEN F12).
        let root = project::parse_to_value(text)
            .ok_or_else(|| StudioError::App("YAML parse failed during rename".into()))?;
        for path in paths {
            match resolve(&root, path) {
                Some(Value::String(_)) => {}
                Some(_) => {
                    return Err(StudioError::App(format!(
                        "Rename site at {path:?} is not a string leaf",
                    )))
                }
                None => {
                    return Err(StudioError::App(format!(
                        "Rename site path not found: {}",
                        path.join("/"),
                    )))
                }
            }
        }
        let mut current = text.to_string();
        for path in paths {
            current = mutate::mutate(
                &current,
                SimpleMutation::SetPrimitive {
                    path:  path.clone(),
                    value: Value::String(new.to_string()),
                },
            )?;
        }
        Ok(current)
    }

    fn apply_bulk_ops(&self, text: &str, ops: &[BulkOp]) -> StudioResult<String> {
        let mut current = text.to_string();
        // Phase A — sets (order irrelevant).
        for op in ops {
            if let BulkOp::Set { path, value } = op {
                current = mutate::mutate(
                    &current,
                    SimpleMutation::SetPrimitive {
                        path:  path.clone(),
                        value: set_value_to_json(value),
                    },
                )?;
            }
        }
        // Phase B — deletes, reverse-sorted so array-index removes don't
        // shift earlier indices.
        let mut delete_paths: Vec<Vec<String>> = ops
            .iter()
            .filter_map(|op| match op {
                BulkOp::Delete { path } => Some(path.clone()),
                _ => None,
            })
            .collect();
        delete_paths.sort_by(|a, b| b.cmp(a));
        delete_paths.dedup();
        for path in delete_paths {
            current = mutate::mutate(&current, SimpleMutation::RemoveAt { path })?;
        }
        Ok(current)
    }

    fn node_kind(&self, v: &Value) -> String {
        kind::node_kind(v)
    }

    fn preview_for(&self, v: &Value) -> String {
        kind::preview_for(v)
    }

    fn coerce_set_value(
        &self,
        _target_kind: &str,
        raw:          &arbor_studio_core::prelude::edit_expr::Value,
    ) -> Result<CoerceOutcome, CoerceSkip> {
        use arbor_studio_core::prelude::edit_expr::Value as ExprValue;
        // YAML `null_handling = Native` → null stays a literal write.
        Ok(match raw {
            ExprValue::Null      => CoerceOutcome::Set(SetValue::Null),
            ExprValue::Bool(b)   => CoerceOutcome::Set(SetValue::Bool(*b)),
            ExprValue::Number(n) => CoerceOutcome::Set(refactor::coerce_number_default(*n)),
            ExprValue::String(s) => CoerceOutcome::Set(SetValue::String(s.clone())),
        })
    }
}

fn resolve<'a>(root: &'a Value, path: &[String]) -> Option<&'a Value> {
    let mut cur = root;
    for seg in path {
        cur = match cur {
            Value::Object(m) => m.get(seg)?,
            Value::Array(a) => {
                let i: usize = seg.parse().ok()?;
                a.get(i)?
            }
            _ => return None,
        };
    }
    Some(cur)
}

fn set_value_to_json(v: &SetValue) -> Value {
    match v {
        SetValue::Null      => Value::Null,
        SetValue::Bool(b)   => Value::Bool(*b),
        SetValue::Int(i)    => Value::Number((*i).into()),
        SetValue::Float(f)  => serde_json::Number::from_f64(*f)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        SetValue::String(s) => Value::String(s.clone()),
    }
}
