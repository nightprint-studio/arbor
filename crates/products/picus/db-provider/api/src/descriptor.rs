//! [`DbProviderDescriptor`] — everything the UI needs to know about an engine,
//! served as **data** rather than hardcoded per component.
//!
//! The model is `corvus/provider-descriptor` for git hosts. The payoff is the same:
//! the create-connection form, the schema tree's groups, the chip labels and the
//! emitter's dialect traits all read one document, so adding an engine is writing
//! a descriptor and a crate — not editing a `match` in five places.
//!
//! Note what lives here and what does not. **Shape** lives here (which fields the
//! form asks for, which groups the tree shows, what a block opens and closes with).
//! **Behaviour** stays in the provider crate. A descriptor that started carrying
//! SQL would be a template engine wearing a hat.

use serde::{Deserialize, Serialize};

use crate::capability::{EngineCapabilities, SchemaGroup};
use crate::kind::EngineKind;

/// What sort of control a connection field is, and therefore how it is rendered
/// and validated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum FieldKind {
    /// A single-line string.
    Text,
    /// An integer with optional bounds (a port, a timeout).
    Number { min: Option<i64>, max: Option<i64> },
    /// A password. **Never** stored in the project or in any config: the value goes
    /// straight to Arbor's keychain and the backend asks for it at the moment of
    /// use. A descriptor declaring this field is declaring "ask, then forget".
    Secret,
    /// One of a fixed set. `options` is `(value, label)`.
    Select { options: Vec<SelectOption> },
    /// A boolean.
    Toggle,
}

/// One choice of a [`FieldKind::Select`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
}

impl SelectOption {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self { value: value.into(), label: label.into() }
    }
}

/// One field of the create-connection form.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionField {
    /// Stable key. Well-known ids (`host`, `port`, `database`, `user`, `schema`,
    /// `password`) map onto [`ConnectionSpec`](crate::connection::ConnectionSpec)'s
    /// named fields; anything else lands in its `params` map, so an engine can ask
    /// for something no other engine has without changing the spec type.
    pub id: String,
    pub label: String,
    #[serde(flatten)]
    pub kind: FieldKind,
    /// Prefilled value for a new connection.
    pub default: Option<String>,
    /// Ghost text — an example, never a repetition of the label.
    pub placeholder: Option<String>,
    pub required: bool,
    /// One line under the field, for the case where the label alone would leave a
    /// reasonable person guessing (`service name` vs `SID`).
    pub help: Option<String>,
}

impl ConnectionField {
    /// A required text field — the common case, kept short at call sites.
    pub fn text(id: &str, label: &str) -> Self {
        Self {
            id: id.to_string(),
            label: label.to_string(),
            kind: FieldKind::Text,
            default: None,
            placeholder: None,
            required: true,
            help: None,
        }
    }

    pub fn optional(mut self) -> Self {
        self.required = false;
        self
    }

    pub fn with_default(mut self, v: &str) -> Self {
        self.default = Some(v.to_string());
        self
    }

    pub fn with_placeholder(mut self, v: &str) -> Self {
        self.placeholder = Some(v.to_string());
        self
    }

    pub fn with_help(mut self, v: &str) -> Self {
        self.help = Some(v.to_string());
        self
    }

    pub fn with_kind(mut self, kind: FieldKind) -> Self {
        self.kind = kind;
        self
    }
}

/// How identifiers are folded when the server is given an unquoted name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IdentifierCase {
    /// Unquoted identifiers fold to upper case (Oracle).
    Upper,
    /// Unquoted identifiers fold to lower case (PostgreSQL).
    Lower,
}

/// The dialect differences the emitter needs, as data.
///
/// These are *already* encoded in the emitter as branches; moving them here is what
/// makes a third engine additive. Kept to the shapes that genuinely differ — a
/// field lands here only when two engines disagree about it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmissionTraits {
    /// Opens an anonymous procedural block (`DECLARE` / `DO $$`).
    pub block_open: String,
    /// Closes it (`END;\n/` / `END $$;`).
    pub block_close: String,
    /// Statement terminator inside a script.
    pub statement_terminator: String,
    /// Function returning the current timestamp (`SYSDATE` / `NOW()`).
    pub now_function: String,
    /// How an "insert if missing" is written, as a human label the preview shows
    /// (`MERGE … FROM DUAL` / `INSERT … ON CONFLICT DO NOTHING`).
    pub upsert_form: String,
    /// Catalogue expression used to check an object exists, with `{object}` as the
    /// placeholder for the (already quoted/cased) object name.
    pub object_exists_check: String,
    pub identifier_case: IdentifierCase,
    /// True when a DDL statement commits implicitly, so a "transactional" target
    /// rule cannot actually roll one back. The generator warns instead of lying.
    pub ddl_commits_implicitly: bool,
}

/// The full per-engine document the frontend renders from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DbProviderDescriptor {
    pub kind: EngineKind,
    /// Full product label (`PostgreSQL 16`).
    pub label: String,
    /// Short label for chips (`PostgreSQL`).
    pub short_label: String,
    /// Theme token holding the engine's identity colour. A token, never a hex
    /// literal — dialect colours belong to the theme.
    pub color_var: String,
    pub default_port: u16,
    pub fields: Vec<ConnectionField>,
    pub capabilities: EngineCapabilities,
    pub emission: EmissionTraits,
    /// The schema-tree groups this engine offers, in display order.
    pub schema_groups: Vec<SchemaGroup>,
}
