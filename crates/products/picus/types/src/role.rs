//! [`FolderRole`] — what a folder of scripts is FOR.
//!
//! It lives in the leaf crate because both halves of Picus need it and neither
//! owns it. The script half decides a folder's role when it discovers a project;
//! the generator half reads that role to pick a destination's default rules. If
//! the two ever disagreed about what "update" means, one generation would write a
//! guarded block into a folder that is installed on a fresh database — which is
//! the exact class of mistake Picus exists to prevent.

use serde::{Deserialize, Serialize};

/// What a folder of scripts is FOR. Drives which rules a target defaults to.
///
/// Ordered, in the order declared below — which is [`FolderRole::ALL`], the order
/// the interface lists them in. The ordering earns its keep as a map key: the
/// rules group folders by `(dialect, role)`, and a lane that sorted differently
/// from run to run would reorder the report for no reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FolderRole {
    /// Runs on a fresh install. Bare statements, no guards.
    Init,
    /// Runs on an existing database. Guarded, and carries the version forward.
    Update,
    /// Packages, procedures, functions, triggers.
    Routines,
    /// Reference rows loaded alongside the schema.
    Data,
    /// Not part of the installation. Read, never written into.
    ///
    /// Also the honest answer for a folder nobody recognised: a folder Picus does
    /// not understand must not receive generated SQL until a human says it should.
    Ignored,
}

impl FolderRole {
    /// Every role, in the order the UI lists them.
    pub const ALL: [FolderRole; 5] = [
        FolderRole::Init,
        FolderRole::Update,
        FolderRole::Routines,
        FolderRole::Data,
        FolderRole::Ignored,
    ];

    /// The wire word — the same string the frontend's `FolderRole` union uses.
    pub fn as_str(self) -> &'static str {
        match self {
            FolderRole::Init => "init",
            FolderRole::Update => "update",
            FolderRole::Routines => "routines",
            FolderRole::Data => "data",
            FolderRole::Ignored => "ignored",
        }
    }

    /// Parse a wire word; `None` for anything unrecognised.
    ///
    /// The counterpart of [`EngineKind::from_wire`](crate::kind::EngineKind::from_wire),
    /// and it exists for the same reason: settings that a human types into a TOML
    /// file are stored as plain strings so a typo degrades to the default and is
    /// reported, rather than failing the parse and resetting the rest of the file.
    pub fn from_wire(s: &str) -> Option<Self> {
        FolderRole::ALL.iter().copied().find(|r| r.as_str() == s)
    }

    /// Can a generation be written into a folder with this role?
    ///
    /// `Ignored` is the only no, and it is a hard no rather than a UI hint: it is
    /// what a folder gets when nobody could tell what it was for.
    pub fn accepts_generation(self) -> bool {
        !matches!(self, FolderRole::Ignored)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wire_words_match_the_frontend_union() {
        // `src/lib/types/picus/index.ts` declares exactly these five. A rename on
        // either side has to break something, and this is the something.
        let words: Vec<&str> = FolderRole::ALL.iter().map(|r| r.as_str()).collect();
        assert_eq!(words, ["init", "update", "routines", "data", "ignored"]);
    }

    #[test]
    fn as_str_and_serde_agree() {
        for role in FolderRole::ALL {
            let json = serde_json::to_string(&role).unwrap();
            assert_eq!(json, format!("\"{}\"", role.as_str()));
        }
    }

    #[test]
    fn wire_words_round_trip() {
        for role in FolderRole::ALL {
            assert_eq!(FolderRole::from_wire(role.as_str()), Some(role));
        }
        assert_eq!(FolderRole::from_wire("initialisation"), None);
        assert_eq!(FolderRole::from_wire(""), None);
    }

    #[test]
    fn only_an_unrecognised_folder_refuses_generation() {
        assert!(!FolderRole::Ignored.accepts_generation());
        for role in FolderRole::ALL.iter().filter(|r| **r != FolderRole::Ignored) {
            assert!(role.accepts_generation());
        }
    }
}
