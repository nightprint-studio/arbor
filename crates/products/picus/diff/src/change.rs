//! The two shapes every comparison in this crate is built out of: "this became
//! that", and "how bad is it".

use serde::{Deserialize, Serialize};

/// One property that differs, with both sides side by side.
///
/// Modelled as `Option<FieldChange<T>>` on every diff struct rather than as two
/// parallel `Option<T>` fields, because those admit a state that cannot happen —
/// a `type_a` without a `type_b` — and every renderer then has to decide what to
/// draw for it. Here the presence of the field *is* the claim that the property
/// changed, and both values are always there to draw.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldChange<T> {
    pub a: T,
    pub b: T,
}

impl<T: PartialEq> FieldChange<T> {
    /// `Some` when the two sides differ, `None` when they agree.
    ///
    /// The whole crate is written as `field: FieldChange::changed(x, y)`, so an
    /// unchanged property costs nothing on the wire and no renderer has to filter
    /// out equal pairs.
    pub fn changed(a: T, b: T) -> Option<Self> {
        if a == b {
            None
        } else {
            Some(Self { a, b })
        }
    }
}

impl<T> FieldChange<T> {
    pub fn new(a: T, b: T) -> Self {
        Self { a, b }
    }
}

/// How much a numeric difference matters, given the thresholds the run was
/// configured with.
///
/// Deliberately not called "status": it says how far outside the tolerance a
/// number is, not whether the check succeeded. A count that differs by one row
/// is still a difference — it is just an [`Severity::Ok`] one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    #[default]
    Ok,
    Warning,
    Error,
}

impl Severity {
    /// Classify a percentage against the two thresholds. `None` is the case where
    /// no percentage exists — growth away from zero — and is the worst of the
    /// three: it is not "unknown", it is "all of it".
    pub fn from_percent(percent: Option<f64>, warning: f64, error: f64) -> Self {
        match percent {
            None => Self::Error,
            Some(p) => {
                let p = p.abs();
                if p >= error {
                    Self::Error
                } else if p >= warning {
                    Self::Warning
                } else {
                    Self::Ok
                }
            }
        }
    }
}
