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
//!
//! ## Both questions are asked of **any** folder, at any depth
//!
//! Neither of these is about a top-level folder. A repository puts the role at the
//! top and the dialect at the bottom (`AGGIORNAMENTO/2024/ORA`) as readily as the
//! other way round, so both run against every folder's own name and inheritance
//! sorts out what applies where ([`crate::resolve`]).
//!
//! That is also why the two match differently. A **role** is matched as a
//! substring, because `dbupdates` and `01_INIZIALIZZAZIONE` both have to work and
//! a wrong role is visible and harmless — nothing is generated until a human
//! agrees. An **engine** is matched on whole words only: `ora` as a substring
//! appears inside `LAVORAZIONE`, and now that every folder in the tree is asked,
//! a substring rule would quietly declare an Oracle folder in the middle of
//! somebody's PostgreSQL repository. Wrong there is the failure this product
//! exists to catch, so it is whole words or nothing.
//!
//! ## What this module will and will not answer
//!
//! [`infer_engine`] answers with a [`FolderEngine`]: a dialect Picus reads, or an
//! engine it merely **recognises** — SQL Server, DB2, MySQL. Those are not the
//! same as "no engine": one is a question for the user, the other is an answer,
//! and a folder that has an answer must stop being asked about.
//!
//! It will **never** answer [`FolderEngine::Generic`]. Portable SQL is a promise
//! — *these scripts run on Oracle and on PostgreSQL* — and a promise is something
//! a person makes, not something a folder name implies. No keyword reaches it and
//! no heuristic produces it; the only way in is somebody declaring it, per path or
//! by name in [`crate::alias`]. Asserted below, because the day a `COMMON`
//! keyword looks tempting is the day this stops being true by accident.
//!
//! ## The global vocabulary is deliberately short
//!
//! It has to be right in *every* repository, so it only holds names that mean one
//! thing everywhere. A name that means something in one repository — `POS` for
//! PostgreSQL, `MSQ` for SQL Server, `CONSEGNE` for updates — belongs in that
//! repository's own vocabulary instead: [`crate::alias`], consulted first by
//! [`infer_role_in`] and [`infer_engine_in`].

use std::borrow::Cow;

use picus_types::prelude::{EngineKind, FolderEngine, FolderRole, ForeignEngine};

use crate::alias::AliasVocabulary;

/// A guess plus the word that produced it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Guess<T> {
    pub value: T,
    /// The keyword that matched, for the proposal UI to show. `None` when the
    /// value is a fallback rather than a match.
    ///
    /// `Cow` because the evidence is a `&'static str` from the tables below in
    /// the common case, and a name out of the project's own vocabulary — owned,
    /// read from a file — otherwise.
    pub matched: Option<Cow<'static, str>>,
}

impl<T> Guess<T> {
    pub(crate) fn matched(value: T, keyword: impl Into<Cow<'static, str>>) -> Self {
        Guess { value, matched: Some(keyword.into()) }
    }

    fn fallback(value: T) -> Self {
        Guess { value, matched: None }
    }

    /// Did a keyword actually match, or is this the fallback?
    pub fn is_confident(&self) -> bool {
        self.matched.is_some()
    }

    /// The evidence as a plain string, for a message.
    pub fn evidence(&self) -> Option<&str> {
        self.matched.as_deref()
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

/// Keywords per engine, matched as **whole words** rather than as substrings.
///
/// Two groups in one table because a folder has one engine and the answer is one
/// lookup: the supported dialects, and the engines Picus can only name.
///
/// Deliberately short and deliberately not extended: `pos` is not here even
/// though `POS` is what one real repository calls its PostgreSQL folder, and
/// neither are `msq` or `db` for that repository's SQL Server and DB2 folders.
/// Three letters that generic would declare an engine on somebody else's
/// `POSIZIONI`. Those belong in that project's own vocabulary — see
/// [`crate::alias`] — where they are a local fact rather than a global claim.
///
/// The unsupported half only holds names that are the product's actual name:
/// `sqlserver`, `mssql`, `db2`. A folder called `DB2` is DB2 in every repository
/// on earth; a folder called `DB` is not.
const ENGINE_KEYWORDS: &[(FolderEngine, &[&str])] = &[
    (
        FolderEngine::Supported(EngineKind::Postgres),
        &["postgresql", "postgres", "pgsql", "psql", "pg"],
    ),
    (FolderEngine::Supported(EngineKind::Oracle), &["oracle", "plsql", "ora"]),
    (
        FolderEngine::Unsupported(ForeignEngine::SqlServer),
        &["sqlserver", "mssql", "tsql"],
    ),
    (FolderEngine::Unsupported(ForeignEngine::Db2), &["db2"]),
    (FolderEngine::Unsupported(ForeignEngine::MySql), &["mysql"]),
    (FolderEngine::Unsupported(ForeignEngine::MariaDb), &["mariadb"]),
    (FolderEngine::Unsupported(ForeignEngine::Sqlite), &["sqlite"]),
];

/// What a folder called this is probably for, using the built-in vocabulary only.
///
/// The fallback is [`FolderRole::Ignored`] and that is the safe answer, not a
/// dismissive one: a folder nobody recognised must not receive generated SQL until
/// a human has said what it is. `is_confident()` is `false` there, so the proposal
/// can single those folders out for attention instead of burying them.
pub fn infer_role(folder_name: &str) -> Guess<FolderRole> {
    infer_role_in(folder_name, &AliasVocabulary::EMPTY)
}

/// What a folder called this is for, with **this project's** vocabulary consulted
/// first.
///
/// Precedence, and it is the whole point of the alias: a name the project declares
/// beats the built-in list. The built-in list has to be right everywhere, so it
/// only holds names that mean one thing everywhere; the project's own list is a
/// local fact its owner knows, and a local fact outranks a global heuristic.
pub fn infer_role_in(folder_name: &str, aliases: &AliasVocabulary) -> Guess<FolderRole> {
    if let Some(guess) = aliases.role(folder_name) {
        return guess;
    }
    match match_keyword(folder_name, ROLE_KEYWORDS) {
        Some((role, keyword)) => Guess::matched(role, keyword),
        None => Guess::fallback(FolderRole::Ignored),
    }
}

/// Which engine a folder's scripts are written in, using the built-in vocabulary
/// only.
///
/// `None`, not a default: guessing wrong here writes Oracle syntax into a
/// PostgreSQL file, which is precisely the failure Picus exists to catch. An
/// unrecognised folder is shown to the user with the question asked out loud.
///
/// A `Some(Unsupported(_))` answer is *not* that question — it is an answer, and
/// the folder is left alone from then on.
///
/// Matched on whole words — see the module note for why this one is stricter
/// than [`infer_role`].
pub fn infer_engine(folder_name: &str) -> Option<Guess<FolderEngine>> {
    infer_engine_in(folder_name, &AliasVocabulary::EMPTY)
}

/// Which engine a folder's scripts are written in, with **this project's**
/// vocabulary consulted first. Same precedence rule as [`infer_role_in`].
pub fn infer_engine_in(
    folder_name: &str,
    aliases: &AliasVocabulary,
) -> Option<Guess<FolderEngine>> {
    if let Some(guess) = aliases.engine(folder_name) {
        return Some(guess);
    }
    match_word(folder_name, ENGINE_KEYWORDS).map(|(kind, keyword)| Guess::matched(kind, keyword))
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

/// Whole-word keyword search: a keyword has to **be** one of the name's words,
/// or a run of them.
///
/// The words are what you would read out loud: separators split them, and so does
/// the boundary between letters and digits, so `ORACLE12` and `01_ORA` both say
/// what they look like they say while `LAVORAZIONE` says nothing. A keyword that
/// spans that boundary itself — `db2` reads as `db` then `2` — has to match the
/// run, which is why this compares sequences rather than single words.
fn match_word<T: Copy>(
    name: &str,
    table: &[(T, &'static [&'static str])],
) -> Option<(T, &'static str)> {
    let haystack = words(name);
    let mut best: Option<(T, &'static str)> = None;
    for (value, keywords) in table {
        for keyword in *keywords {
            if !contains_words(&haystack, &words(keyword)) {
                continue;
            }
            // The longest match wins, so `ORACLE_ORA` reports the word a human
            // would point at rather than whichever was tested first.
            let better = best.map(|(_, current)| keyword.len() > current.len()).unwrap_or(true);
            if better {
                best = Some((*value, keyword));
            }
        }
    }
    best
}

/// Does `needle` appear as a **contiguous run** of `haystack`'s words?
///
/// Shared with [`crate::alias`], so a project's own name matches exactly the way
/// a built-in keyword does. An empty needle never matches: an alias with no name
/// would otherwise claim every folder in the repository.
pub(crate) fn contains_words(haystack: &[String], needle: &[String]) -> bool {
    if needle.is_empty() || needle.len() > haystack.len() {
        return false;
    }
    haystack.windows(needle.len()).any(|window| window == needle)
}

/// `01_Aggiornamento-DB2` → `["01", "aggiornamento", "db", "2"]`.
pub(crate) fn words(name: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut digits = false;
    for c in name.chars() {
        if !c.is_alphanumeric() {
            push(&mut out, &mut current);
            continue;
        }
        if !current.is_empty() && c.is_numeric() != digits {
            push(&mut out, &mut current);
        }
        digits = c.is_numeric();
        current.extend(c.to_lowercase());
    }
    push(&mut out, &mut current);
    out
}

fn push(out: &mut Vec<String>, current: &mut String) {
    if !current.is_empty() {
        out.push(std::mem::take(current));
    }
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
    use crate::alias::InferenceAlias;

    fn engine_of(name: &str) -> Option<FolderEngine> {
        infer_engine(name).map(|g| g.value)
    }

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
        assert_eq!(guess.evidence(), Some("aggiornamento"));
    }

    #[test]
    fn the_evidence_is_the_specific_word_not_the_shortest_one() {
        // "inizializzazione" contains "init"; the reported evidence must be the
        // word a human would point at.
        assert_eq!(infer_role("INIZIALIZZAZIONE").evidence(), Some("inizializzazione"));
    }

    #[test]
    fn dialects_are_recognised_from_any_folders_own_name() {
        let oracle = FolderEngine::Supported(EngineKind::Oracle);
        let postgres = FolderEngine::Supported(EngineKind::Postgres);
        assert_eq!(engine_of("ORACLE"), Some(oracle));
        assert_eq!(engine_of("POSTGRES"), Some(postgres));
        assert_eq!(engine_of("PostgreSQL"), Some(postgres));
        assert_eq!(engine_of("db_pg"), Some(postgres));
        // The leaf folder of a real repository, three levels down.
        assert_eq!(engine_of("ORA"), Some(oracle));
        // …and a version stuck on the end is still the same word.
        assert_eq!(engine_of("ORACLE12"), Some(oracle));
        assert_eq!(engine_of("01_ORA"), Some(oracle));
    }

    #[test]
    fn an_engine_picus_does_not_support_is_recognised_rather_than_left_unknown() {
        // The third state. `MSSQL` is not a question — it is an answer, and the
        // folder must stop being asked about.
        assert_eq!(
            engine_of("MSSQL"),
            Some(FolderEngine::Unsupported(ForeignEngine::SqlServer))
        );
        assert_eq!(
            engine_of("SQLSERVER"),
            Some(FolderEngine::Unsupported(ForeignEngine::SqlServer))
        );
        // `db2` spans the letter/digit boundary the splitter breaks on, so the
        // whole-word rule has to match a RUN of words, not a single one.
        assert_eq!(engine_of("DB2"), Some(FolderEngine::Unsupported(ForeignEngine::Db2)));
        assert_eq!(engine_of("01_DB2"), Some(FolderEngine::Unsupported(ForeignEngine::Db2)));
        assert_eq!(engine_of("MYSQL"), Some(FolderEngine::Unsupported(ForeignEngine::MySql)));
        assert_eq!(engine_of("sqlite"), Some(FolderEngine::Unsupported(ForeignEngine::Sqlite)));
        // …and none of them yields a dialect to parse with.
        assert_eq!(engine_of("DB2").unwrap().dialect(), None);
    }

    #[test]
    fn the_unsupported_vocabulary_does_not_claim_generic_abbreviations() {
        // `DB` is not DB2 and `MSQ` is not SQL Server — not globally. Both are one
        // repository's own shorthand, and both are what `[[alias]]` is for.
        assert_eq!(engine_of("DB"), None);
        assert_eq!(engine_of("MSQ"), None);
        // `DB2000` is a version number, not a product.
        assert_eq!(engine_of("DB2000"), None);
    }

    #[test]
    fn portable_sql_is_never_inferred_from_a_name() {
        // A promise that these scripts run on both engines is not something a
        // folder name can make on the user's behalf. The tempting names are the
        // ones asserted here.
        for name in ["COMUNE", "COMMON", "GENERIC", "PORTABLE", "SHARED", "CONDIVISO", "ALL"] {
            assert_ne!(
                engine_of(name),
                Some(FolderEngine::Generic),
                "{name} must not read as portable SQL"
            );
        }
        // …and no keyword in the table produces it, whatever the name.
        assert!(
            ENGINE_KEYWORDS.iter().all(|(engine, _)| !engine.is_generic()),
            "the built-in vocabulary must never contain `generic`"
        );
    }

    #[test]
    fn an_unrecognised_folder_gets_no_engine_at_all() {
        // Never a default: guessing wrong writes Oracle syntax into a PostgreSQL
        // file, which is the failure this product exists to catch.
        assert!(engine_of("COMMON").is_none());
        assert!(engine_of("database").is_none());
        // `POS` is what one real repository calls its PostgreSQL folder and it is
        // deliberately not a keyword: three letters that generic would declare an
        // engine on somebody else's `POSIZIONI`. The user declares it per project.
        assert!(engine_of("POS").is_none());
        assert!(engine_of("MSQ").is_none());
    }

    #[test]
    fn an_engine_keyword_hiding_inside_another_word_is_not_a_match() {
        // The reason this one is whole-word while roles are substring: every
        // folder in the tree is now asked, and `ora` sits inside plenty of
        // ordinary Italian folder names.
        for name in ["LAVORAZIONE", "MEMORIA", "ORARI", "PGADMIN"] {
            assert!(engine_of(name).is_none(), "{name} must not read as an engine");
        }
    }

    #[test]
    fn the_engine_evidence_is_the_longest_word_that_matched() {
        assert_eq!(infer_engine("ORACLE_ORA").unwrap().evidence(), Some("oracle"));
    }

    // ── The project's own vocabulary ────────────────────────────────────────────

    fn vocabulary(entries: &[(&str, Option<&str>, Option<&str>)]) -> AliasVocabulary {
        let aliases: Vec<InferenceAlias> = entries
            .iter()
            .map(|(name, engine, role)| InferenceAlias {
                name: name.to_string(),
                engine: engine.map(str::to_string),
                role: role.map(str::to_string),
            })
            .collect();
        AliasVocabulary::compile(&aliases)
    }

    #[test]
    fn an_alias_adds_to_the_built_in_vocabulary_rather_than_replacing_it() {
        // The trap this avoids: declaring one alias must not cost the repository
        // every default it was already relying on.
        let v = vocabulary(&[("POS", Some("postgres"), None)]);
        assert_eq!(
            infer_engine_in("POS", &v).map(|g| g.value),
            Some(FolderEngine::Supported(EngineKind::Postgres))
        );
        // …and ORA still means Oracle, from the built-in list.
        assert_eq!(
            infer_engine_in("ORA", &v).map(|g| g.value),
            Some(FolderEngine::Supported(EngineKind::Oracle))
        );
        assert_eq!(infer_role_in("AGGIORNAMENTO", &v).value, FolderRole::Update);
    }

    #[test]
    fn an_alias_beats_the_built_in_vocabulary() {
        // A local fact outranks a global heuristic — that is the whole precedence
        // rule. A repository where `ORA` means something else gets to say so.
        let v = vocabulary(&[("ORA", Some("postgres"), None), ("DATI", None, Some("update"))]);
        assert_eq!(
            infer_engine_in("ORA", &v).map(|g| g.value),
            Some(FolderEngine::Supported(EngineKind::Postgres))
        );
        assert_eq!(infer_role_in("DATI", &v).value, FolderRole::Update);
    }

    #[test]
    fn an_alias_matches_whole_words_exactly_like_a_built_in_keyword() {
        let v = vocabulary(&[("POS", Some("postgres"), None)]);
        // The versioned folders of a real repository, and a name it is hiding in.
        for name in ["POS", "pos", "01_POS", "POS_2024"] {
            assert!(infer_engine_in(name, &v).is_some(), "{name} must read as an engine");
        }
        for name in ["POSIZIONI", "DEPOSITO", "POSTA"] {
            assert!(infer_engine_in(name, &v).is_none(), "{name} must not");
        }
    }

    #[test]
    fn an_alias_can_declare_a_name_portable_because_that_is_still_the_user_saying_it() {
        // Inference never produces `generic`, but an alias is not inference — it
        // is the user writing down a fact about their own repository, which is
        // exactly the source a promise of portability is allowed to come from.
        let v = vocabulary(&[("COMUNE", Some("generic"), Some("data"))]);
        let guess = infer_engine_in("COMUNE", &v).expect("declared");
        assert_eq!(guess.value, FolderEngine::Generic);
        assert!(EngineKind::ALL.iter().all(|d| guess.value.covers(*d)));
        assert_eq!(infer_role_in("COMUNE", &v).value, FolderRole::Data);
        // …and it still says nothing about a folder that is not called that.
        assert!(infer_engine_in("ORDINI", &v).is_none());
    }

    #[test]
    fn an_alias_can_name_an_engine_picus_does_not_support() {
        // The other half of the real repository: MSQ and DB are SQL Server and
        // DB2, which Picus will never read. Saying so is what stops the folder
        // generating a question on every scan.
        let v = vocabulary(&[("MSQ", Some("sqlserver"), None), ("DB", Some("db2"), None)]);
        let msq = infer_engine_in("MSQ", &v).expect("recognised");
        assert_eq!(msq.value, FolderEngine::Unsupported(ForeignEngine::SqlServer));
        assert_eq!(msq.value.dialect(), None, "nothing is parsed with it");
        assert_eq!(
            infer_engine_in("AGGIORNAMENTO_DB", &v).map(|g| g.value),
            Some(FolderEngine::Unsupported(ForeignEngine::Db2))
        );
    }

    #[test]
    fn an_alias_carries_its_own_name_as_the_evidence() {
        let v = vocabulary(&[("POS", Some("postgres"), None)]);
        assert_eq!(infer_engine_in("01_POS", &v).unwrap().evidence(), Some("POS"));
    }

    #[test]
    fn a_bad_alias_degrades_and_the_rest_of_the_vocabulary_still_works() {
        // `sqlserver2019` is not an engine Picus knows; the entry is dropped and
        // the good ones beside it are unaffected. `ProjectConfig::problems` is
        // where the user is told.
        let v = vocabulary(&[
            ("MSQ", Some("sqlserver2019"), None),
            ("POS", Some("postgres"), None),
            ("NOTHING", None, None),
        ]);
        assert!(infer_engine_in("MSQ", &v).is_none(), "the bad entry claims nothing");
        assert!(infer_engine_in("NOTHING", &v).is_none());
        assert!(infer_engine_in("POS", &v).is_some());
    }

    #[test]
    fn an_alias_declares_the_engine_and_the_role_independently() {
        // A repository whose update folder is called CONSEGNE has exactly the same
        // problem as one whose PostgreSQL folder is called POS.
        let v = vocabulary(&[("CONSEGNE", None, Some("update"))]);
        assert_eq!(infer_role_in("CONSEGNE", &v).value, FolderRole::Update);
        assert!(infer_engine_in("CONSEGNE", &v).is_none(), "it said nothing about the engine");
    }
}
