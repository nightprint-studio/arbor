//! One cell, and what it means for two cells to be equal.
//!
//! ## Why this is typed
//!
//! The obvious way to diff two result sets is to render every cell to a string
//! and compare the strings. It is also wrong in both directions at once, and the
//! direction it is wrong in depends on the driver:
//!
//! * two drivers that render the same `NUMBER(10,2)` as `1.5` and `1.50` produce
//!   a difference that does not exist;
//! * a driver that renders the integer `1`, the float `1.0` and the string `'1'`
//!   all as `1` hides two differences that do;
//! * `NULL` and the empty string become the same cell, which in a tool whose
//!   whole job is to help someone write an `UPDATE` is the single most expensive
//!   confusion available.
//!
//! So the caller hands over the value with its type intact and the comparison
//! never crosses variants: `Int(1)`, `Float(1.0)` and `Text("1")` are three
//! different values, and only `Null` equals `Null`.
//!
//! ## Why the shape is what it is
//!
//! Four variants, matching the driver contract's own cell type one for one, and
//! serialised untagged so the two are the same JSON. That is intentional: the
//! backend reads rows through the driver and feeds them here, and a shape that
//! needed a translation would be a place for the translation to be wrong.

use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

/// One cell as it came out of a driver.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DiffValue {
    /// Must stay the first variant: untagged deserialisation tries them in order
    /// and `null` has to land here.
    Null,
    Int(i64),
    Float(f64),
    /// Everything that is not a number — including dates, which arrive formatted
    /// by the server and are compared as the text the server chose.
    Text(String),
}

impl DiffValue {
    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// A short rendering for a message. Never used for comparison — see the
    /// module docs for why that would defeat the point of the type.
    pub fn render(&self) -> String {
        match self {
            Self::Null => "NULL".to_string(),
            Self::Int(i) => i.to_string(),
            Self::Float(f) => f.to_string(),
            Self::Text(t) => t.clone(),
        }
    }
}

/// Equality across two databases.
///
/// Hand-written for one reason: `f64` says `NaN != NaN`, and a stored `NaN` in
/// both databases is two databases that agree. A tool that reported that column
/// as "different" on every run, forever, with no edit that could make it stop,
/// would be a tool people learn to ignore. `-0.0` and `0.0` are likewise one
/// number here — no server distinguishes them and no user would thank us for it.
impl PartialEq for DiffValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Null, Self::Null) => true,
            (Self::Int(a), Self::Int(b)) => a == b,
            (Self::Float(a), Self::Float(b)) => a == b || (a.is_nan() && b.is_nan()),
            (Self::Text(a), Self::Text(b)) => a == b,
            // Deliberately no cross-variant arm: `1`, `1.0` and `'1'` are three
            // values, and a driver that hands over the wrong one is a bug worth
            // seeing rather than smoothing over.
            _ => false,
        }
    }
}

impl Eq for DiffValue {}

/// Consistent with the `PartialEq` above — which is a hard requirement here,
/// because keyed row matching puts these in a `HashMap` and a hash that
/// disagreed with equality would make two equal keys miss each other and be
/// reported as "only in A" and "only in B".
impl Hash for DiffValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Self::Null => {}
            Self::Int(i) => i.hash(state),
            Self::Float(f) => {
                if f.is_nan() {
                    // One bucket for every NaN bit pattern.
                    u64::MAX.hash(state);
                } else if *f == 0.0 {
                    // `-0.0` and `0.0` compare equal above, so they hash alike.
                    0u64.hash(state);
                } else {
                    f.to_bits().hash(state);
                }
            }
            Self::Text(t) => t.hash(state),
        }
    }
}

impl From<i64> for DiffValue {
    fn from(v: i64) -> Self {
        Self::Int(v)
    }
}

impl From<f64> for DiffValue {
    fn from(v: f64) -> Self {
        Self::Float(v)
    }
}

impl From<String> for DiffValue {
    fn from(v: String) -> Self {
        Self::Text(v)
    }
}

impl From<&str> for DiffValue {
    fn from(v: &str) -> Self {
        Self::Text(v.to_string())
    }
}

impl<T: Into<DiffValue>> From<Option<T>> for DiffValue {
    fn from(v: Option<T>) -> Self {
        match v {
            Some(inner) => inner.into(),
            None => Self::Null,
        }
    }
}
