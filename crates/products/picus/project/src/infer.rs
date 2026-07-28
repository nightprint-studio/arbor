//! Guessing what a folder is, from what it is called — and saying so.
//!
//! Every guess here is a **proposal**. Nothing inferred reaches the disk until the
//! user has seen it and agreed, which is why each answer carries the evidence that
//! produced it: "AGGIORNAMENTO → update" is a sentence someone can disagree with,
//! whereas a role appearing in a dropdown is not.
//!
//! The vocabulary is Italian and English together, on purpose. These repositories
//! are Italian in practice (`INIZIALIZZAZIONE`, `AGGIORNAMENTO`) while the codebase
//! and its documentation are English; matching only one of the two would make the
//! feature useless in exactly the case it was built for.

use picus_types::prelude::{EngineKind, FolderRole};

/// A guess plus the word that produced it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Guess<T> {
    pub value: T,
    /// The keyword that matched, for the proposal UI to show. `None` when the
    /// value is a fallback rather than a match.
    pub matched: Option<&'static str>,
}

impl<T> Guess<T> {
    fn matched(value: T, keyword: &'static str) -> Self {
        Guess { value, matched: Some(keyword) }
    }

    fn fallback(value: T) -> Self {
        Guess { value, matched: None }
    }

    /// Did a keyword actually match, or is this the fallback?
    pub fn is_confident(&self) -> bool {
        self.matched.is_some()
    }
}

/// Keywords per role, longest-first within each group so `inizializzazione` is
/// tested before `init` and the reported evidence is the specific word.
const ROLE_KEYWORDS: &[(FolderRole, &[&str])] = &[
    (
        FolderRole::Init,
        &["inizializzazione", "installazione", "initialisation", "initialization", "creazione", "install", "schema", "setup", "init"],
    ),
    (
        FolderRole::Update,
        &["aggiornamento", "aggiornamenti", "migrazione", "migrazioni", "migrations", "migration", "upgrade", "update", "patch", "delta"],
    ),
    (
        FolderRole::Routines,
        &["storedprocedures", "procedures", "procedure", "functions", "function", "packages", "package", "routines", "triggers", "trigger", "plsql", "views", "vista", "viste"],
    ),
    (
        FolderRole::Data,
        &["datibase", "reference", "anagrafiche", "fixtures", "seeds", "dati", "seed", "data", "lookup"],
    ),
    (
        FolderRole::Ignored,
        &["documentazione", "documentation", "backup", "archivio", "archive", "esempi", "samples", "sample", "docs", "doc", "old", "tmp", "temp"],
    ),
];

/// Keywords per engine, same rule.
const DIALECT_KEYWORDS: &[(EngineKind, &[&str])] = &[
    (EngineKind::Postgres, &["postgresql", "postgres", "pgsql", "psql", "pg"]),
    (EngineKind::Oracle, &["oracle", "plsql", "ora"]),
];

/// What a folder called this is probably for.
///
/// The fallback is [`FolderRole::Ignored`] and that is the safe answer, not a
/// dismissive one: a folder nobody recognised must not receive generated SQL until
/// a human has said what it is. `is_confident()` is `false` there, so the proposal
/// can single those folders out for attention instead of burying them.
pub fn infer_role(folder_name: &str) -> Guess<FolderRole> {
    match match_keyword(folder_name, ROLE_KEYWORDS) {
        Some((role, keyword)) => Guess::matched(role, keyword),
        None => Guess::fallback(FolderRole::Ignored),
    }
}

/// Which engine a branch folder is written in.
///
/// `None`, not a default: guessing wrong here writes Oracle syntax into a
/// PostgreSQL file, which is precisely the failure Picus exists to catch. An
/// unrecognised branch is shown to the user with the question asked out loud.
pub fn infer_dialect(branch_name: &str) -> Option<Guess<EngineKind>> {
    match_keyword(branch_name, DIALECT_KEYWORDS).map(|(kind, keyword)| Guess::matched(kind, keyword))
}

/// Case-insensitive, separator-insensitive keyword search.
///
/// Matching on the normalised *whole* name rather than on tokens is deliberate:
/// `01_INIZIALIZZAZIONE` and `db-updates` both have to work, and splitting on
/// separators first would miss `dbupdates`.
fn match_keyword<T: Copy>(
    name: &str,
    table: &[(T, &'static [&'static str])],
) -> Option<(T, &'static str)> {
    let normalised = normalise(name);
    // Longest keyword across the whole table wins, so a folder called
    // `DATI_AGGIORNAMENTO` reads as an update rather than as data purely because
    // `dati` happened to be tested first. Keywords are stored separator-free
    // because the name is normalised the same way before matching.
    let mut best: Option<(T, &'static str)> = None;
    for (value, keywords) in table {
        for keyword in *keywords {
            if normalised.contains(keyword) {
                let better = best.map(|(_, current)| keyword.len() > current.len()).unwrap_or(true);
                if better {
                    best = Some((*value, keyword));
                }
            }
        }
    }
    best
}

/// Lowercase, and every separator removed — `01_Aggiornamento-DB` → `01aggiornamentodb`.
fn normalise(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_italian_folder_names_these_repositories_actually_use() {
        assert_eq!(infer_role("INIZIALIZZAZIONE").value, FolderRole::Init);
        assert_eq!(infer_role("AGGIORNAMENTO").value, FolderRole::Update);
        assert_eq!(infer_role("PROCEDURE").value, FolderRole::Routines);
        assert_eq!(infer_role("DATI").value, FolderRole::Data);
        assert_eq!(infer_role("DOCUMENTAZIONE").value, FolderRole::Ignored);
    }

    #[test]
    fn and_the_english_ones() {
        assert_eq!(infer_role("init").value, FolderRole::Init);
        assert_eq!(infer_role("migrations").value, FolderRole::Update);
        assert_eq!(infer_role("functions").value, FolderRole::Routines);
        assert_eq!(infer_role("seed-data").value, FolderRole::Data);
    }

    #[test]
    fn numbering_and_separators_do_not_hide_the_word() {
        assert_eq!(infer_role("01_INIZIALIZZAZIONE").value, FolderRole::Init);
        assert_eq!(infer_role("02-Aggiornamento DB").value, FolderRole::Update);
        assert_eq!(infer_role("dbupdates").value, FolderRole::Update);
    }

    #[test]
    fn an_unrecognised_folder_is_ignored_and_says_it_is_guessing() {
        let guess = infer_role("MISCELLANEA");
        assert_eq!(guess.value, FolderRole::Ignored);
        assert!(!guess.is_confident());

        // A recognised one is confident and can name the word it matched.
        let guess = infer_role("AGGIORNAMENTO");
        assert!(guess.is_confident());
        assert_eq!(guess.matched, Some("aggiornamento"));
    }

    #[test]
    fn the_evidence_is_the_specific_word_not_the_shortest_one() {
        // "inizializzazione" contains "init"; the reported evidence must be the
        // word a human would point at.
        assert_eq!(infer_role("INIZIALIZZAZIONE").matched, Some("inizializzazione"));
    }

    #[test]
    fn dialects_are_recognised_from_the_branch_folder() {
        assert_eq!(infer_dialect("ORACLE").unwrap().value, EngineKind::Oracle);
        assert_eq!(infer_dialect("POSTGRES").unwrap().value, EngineKind::Postgres);
        assert_eq!(infer_dialect("PostgreSQL").unwrap().value, EngineKind::Postgres);
        assert_eq!(infer_dialect("db_pg").unwrap().value, EngineKind::Postgres);
    }

    #[test]
    fn an_unrecognised_branch_gets_no_dialect_at_all() {
        // Never a default: guessing wrong writes Oracle syntax into a PostgreSQL
        // file, which is the failure this product exists to catch.
        assert!(infer_dialect("COMMON").is_none());
        assert!(infer_dialect("database").is_none());
    }
}
