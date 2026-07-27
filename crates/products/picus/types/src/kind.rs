//! [`EngineKind`] — which database engine a connection, a folder or a generated
//! statement belongs to.
//!
//! This is the same vocabulary the product calls a **dialect**, and the two are
//! deliberately one type: the engine that a live connection speaks and the dialect
//! a script folder is written in must never drift apart. What must NOT follow from
//! that is an ambient value — see the note on [`EngineKind`].

use serde::{Deserialize, Serialize};

/// A database engine / SQL dialect.
///
/// **Never store this as global state.** It is a property of the *thing* being
/// acted on — the connection, the folder, the target — and travels as an explicit
/// parameter through every parse / emit / rewrite call. A backend-wide "current
/// engine" would break the product's single reason to exist (`docs/picus-design.md`
/// §1).
///
/// `Oracle` is a first-class member here from day one even though no Oracle
/// *driver* exists: the script half — reading, parsing, analysing, generating and
/// rewriting Oracle SQL — is pure text and needs none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EngineKind {
    Postgres,
    Oracle,
}

impl EngineKind {
    /// The stable wire string — also the value the frontend's `Dialect` uses.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Postgres => "postgres",
            Self::Oracle => "oracle",
        }
    }

    /// Parse a wire string; `None` for anything unrecognised.
    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "postgres" => Some(Self::Postgres),
            "oracle" => Some(Self::Oracle),
            _ => None,
        }
    }

    /// Every engine Picus knows about, in display order. Whether one can be
    /// *connected to* is a separate question — ask the registry.
    pub const ALL: &'static [EngineKind] = &[EngineKind::Postgres, EngineKind::Oracle];
}

impl std::fmt::Display for EngineKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_strings_round_trip() {
        for k in EngineKind::ALL {
            assert_eq!(EngineKind::from_wire(k.as_str()), Some(*k));
        }
        assert_eq!(EngineKind::from_wire("mysql"), None);
    }

    #[test]
    fn serde_matches_the_frontend_dialect_strings() {
        assert_eq!(serde_json::to_string(&EngineKind::Postgres).unwrap(), "\"postgres\"");
        assert_eq!(serde_json::to_string(&EngineKind::Oracle).unwrap(), "\"oracle\"");
    }
}
