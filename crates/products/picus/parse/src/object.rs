//! Named database objects: what a statement defines, and what it references.
//!
//! The distinction is fixed here once so every consumer means the same thing:
//!
//! * **defines** — the statement creates the object or changes its definition
//!   (`CREATE …`, `ALTER …`). This is what `picus-inventory` lists.
//! * **references** — every other object the statement names: the FROM tables,
//!   the target of an INSERT, the table a foreign key points at, the object a
//!   DROP removes. This is what a dependency graph follows.
//!
//! A DROP is a reference, not a definition. It names an object that must already
//! exist; treating it as a definition would put a removed table into the
//! inventory.

use serde::{Deserialize, Serialize};

use crate::range::ByteRange;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ObjectKind {
    Table,
    View,
    MaterializedView,
    Index,
    Sequence,
    Trigger,
    Function,
    Procedure,
    Package,
    PackageBody,
    Type,
    Schema,
    Synonym,
    Column,
    Constraint,
    Role,
    Database,
    Tablespace,
    Domain,
    Extension,
    /// Named in the source but of a kind this crate does not model.
    Unknown,
}

/// One named object, as written.
///
/// The name is kept **verbatim** — quotes, case and all — because the rewriter
/// has to be able to put back exactly what was there. Comparison goes through
/// [`ObjectRef::folded_name`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectRef {
    pub kind: ObjectKind,
    /// Schema qualifier as written, when the name had one.
    pub schema: Option<String>,
    pub name: String,
    /// The whole dotted name, including the qualifier.
    pub range: ByteRange,
}

impl ObjectRef {
    /// The name in comparison form.
    ///
    /// Unquoted names fold to UPPER CASE, quoted names keep their contents
    /// exactly. This is the one form in which an Oracle `PARAMETRI` and a
    /// PostgreSQL `parametri` are the same object — which is the whole point of
    /// a cross-dialect diff. Uppercase (rather than lower) because that is what
    /// Oracle's own catalogue stores, and Oracle is the branch whose names are
    /// written by hand.
    pub fn folded_name(&self) -> String {
        fold(&self.name)
    }

    pub fn folded_schema(&self) -> Option<String> {
        self.schema.as_deref().map(fold)
    }

    /// `SCHEMA.NAME` in folded form, or just `NAME` when unqualified.
    pub fn folded_qualified(&self) -> String {
        match self.folded_schema() {
            Some(s) => format!("{s}.{}", self.folded_name()),
            None => self.folded_name(),
        }
    }
}

/// `"Mixed Case"` → `Mixed Case`; `parametri` → `PARAMETRI`.
pub(crate) fn fold(written: &str) -> String {
    if written.len() >= 2 && written.starts_with('"') && written.ends_with('"') {
        // Quoted: contents verbatim, with `""` collapsed back to one quote.
        written[1..written.len() - 1].replace("\"\"", "\"")
    } else {
        written.to_uppercase()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj(name: &str) -> ObjectRef {
        ObjectRef {
            kind: ObjectKind::Table,
            schema: None,
            name: name.to_string(),
            range: ByteRange::new(0, name.len()),
        }
    }

    #[test]
    fn unquoted_names_fold_across_dialects() {
        assert_eq!(obj("parametri").folded_name(), "PARAMETRI");
        assert_eq!(obj("PARAMETRI").folded_name(), "PARAMETRI");
        assert_eq!(obj("Parametri").folded_name(), "PARAMETRI");
    }

    #[test]
    fn quoted_names_keep_their_case_and_unescape() {
        assert_eq!(obj("\"Mixed Case\"").folded_name(), "Mixed Case");
        assert_eq!(obj("\"He said \"\"hi\"\"\"").folded_name(), "He said \"hi\"");
        // A quoted lowercase name is NOT the same object as an unquoted one.
        assert_ne!(obj("\"parametri\"").folded_name(), obj("parametri").folded_name());
    }

    #[test]
    fn qualified_form_folds_both_halves() {
        let mut o = obj("t");
        o.schema = Some("app".into());
        assert_eq!(o.folded_qualified(), "APP.T");
    }
}
