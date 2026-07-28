//! Folder names that mean something **in this repository**.
//!
//! The built-in vocabulary in [`crate::infer`] is a global heuristic: it has to be
//! right in every repository, so it can only hold names that mean one thing
//! everywhere. `ORA` is Oracle wherever you find it. `POS` is not PostgreSQL
//! wherever you find it — it is point-of-sale, or `POSIZIONI`, or nothing — and
//! adding it to the global list would misclassify somebody else's tree.
//!
//! But it *is* PostgreSQL in one particular repository, and its owner knows that.
//! An alias is that knowledge, written down where it applies:
//!
//! ```toml
//! [[alias]]
//! name = "POS"
//! engine = "postgres"
//!
//! [[alias]]
//! name = "MSQ"
//! engine = "sqlserver"     # recognised, and not one Picus reads
//!
//! [[alias]]
//! name = "CONSEGNE"
//! role = "update"
//! ```
//!
//! ## Why a name and not a path
//!
//! Because a repository with a folder per delivered version has eleven `POS`
//! folders, and will have a twelfth next month. A per-path declaration answers for
//! one of them; the alias answers for all of them **and for every one added
//! later**, which is the difference between describing a repository once and
//! re-describing it at every release. Per-path declarations still win where they
//! exist — a specific answer beats a general rule — see [`crate::discover`].
//!
//! ## The three rules it follows
//!
//! * It **adds to** the built-in vocabulary; declaring one alias never costs the
//!   repository the defaults. Losing them by saying one thing would be a trap.
//! * It matches **exactly the way a built-in keyword does** — whole word,
//!   case-insensitively, through [`crate::infer::contains_words`]. Substring
//!   matching is how `POS` would start claiming `POSIZIONI` again, which is the
//!   precise reason `pos` is not in the global list.
//! * A bad value **degrades**. The engine and the role are stored as plain wire
//!   strings and read through typed accessors, exactly like
//!   `[generation.insertion]`: a typo drops that one entry and is reported by
//!   [`ProjectConfig::problems`](crate::config::ProjectConfig::problems), rather
//!   than failing the parse and resetting every other setting in the file.
//!
//! ## Where the name is looked for — [`AliasScope`]
//!
//! Folder names, by default. Some repositories are tidier than others, and in a
//! dirty one the engine is on the **file**: `4_12_POS.sql` and `4_12_ORA.sql`
//! side by side in one folder that can say nothing about either. `applies_to`
//! opens the same name up to file names:
//!
//! ```toml
//! [[alias]]
//! name = "POS"
//! engine = "postgres"
//! applies_to = "both"      # "folders" (default) | "files" | "both"
//! ```
//!
//! It is opt-in, and that is not timidity. A file name is a sentence — `POS`,
//! `ORA`, `DB` are three letters that appear inside ordinary Italian words, and
//! there are hundreds of file names to a handful of folder names, so a rule that
//! is merely *usually* right costs far more here. Making it a word the repository's
//! owner writes means file-name classification is always something someone
//! decided, never something Picus inferred.

use picus_types::prelude::{FolderEngine, FolderRole, ForeignEngine, EngineKind};
use serde::{Deserialize, Serialize};

use crate::infer::{contains_words, words, Guess};

/// Where an alias's name is looked for.
///
/// A separate type rather than a `bool` because the third answer is real: a
/// repository can have a `POS` **folder** for the tidy versions and a `4_12_POS.sql`
/// **file** for the years somebody was in a hurry, and one alias should answer for
/// both without a second entry that can drift from the first.
/// Serialised as the same lowercase words `applies_to` takes in the file, so the
/// RPC verb that sets an alias and the TOML it writes speak one vocabulary.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AliasScope {
    /// Folder names only — the default, and what every alias written before this
    /// existed means.
    #[default]
    Folders,
    /// File names only.
    Files,
    /// Both.
    Both,
}

impl AliasScope {
    pub const ALL: &'static [AliasScope] =
        &[AliasScope::Folders, AliasScope::Files, AliasScope::Both];

    pub fn as_str(self) -> &'static str {
        match self {
            AliasScope::Folders => "folders",
            AliasScope::Files => "files",
            AliasScope::Both => "both",
        }
    }

    pub fn from_wire(s: &str) -> Option<AliasScope> {
        AliasScope::ALL.iter().copied().find(|scope| scope.as_str() == s)
    }

    /// Does this alias answer about folder names?
    pub fn covers_folders(self) -> bool {
        matches!(self, AliasScope::Folders | AliasScope::Both)
    }

    /// Does this alias answer about file names?
    pub fn covers_files(self) -> bool {
        matches!(self, AliasScope::Files | AliasScope::Both)
    }
}

/// One name this repository uses for an engine or a role.
///
/// Every value field is a plain string on purpose — see the module note on
/// degrading. `engine` and `role` are both optional and at least one has to be
/// set; an alias that says nothing is reported and ignored.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InferenceAlias {
    /// The name, as the repository spells it. Matched whole-word and
    /// case-insensitively, so `POS` is written once and matches `POS`, `pos`,
    /// `01_POS` and `POS_2024`.
    pub name: String,
    /// The engine things with this name are written in — a dialect Picus reads
    /// (`oracle`, `postgres`), portable SQL (`generic`), or an engine it only
    /// recognises (`sqlserver`, `db2`, `mysql`, `mariadb`, `sqlite`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
    /// What folders with this name are for (`init`, `update`, `routines`,
    /// `data`, `ignored`).
    ///
    /// Folders only, whatever `applies_to` says: a role is what a *directory of
    /// scripts* is for, and the file beside it in the same directory is for the
    /// same thing. Engines are the axis that genuinely varies file by file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Whether the name is looked for in folder names, file names, or both.
    /// Absent means [`AliasScope::Folders`] — see the module note on why file
    /// names are opt-in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applies_to: Option<String>,
}

impl InferenceAlias {
    /// An alias that names a folder and says nothing about it yet.
    pub fn new(name: impl Into<String>) -> InferenceAlias {
        InferenceAlias { name: name.into(), ..InferenceAlias::default() }
    }

    /// The engine this alias declares, or `None` when it declares none — or one
    /// Picus cannot name.
    pub fn engine(&self) -> Option<FolderEngine> {
        self.engine.as_deref().and_then(FolderEngine::from_wire)
    }

    /// The role this alias declares, or `None` on the same terms.
    pub fn role(&self) -> Option<FolderRole> {
        self.role.as_deref().and_then(FolderRole::from_wire)
    }

    /// Where this alias's name is looked for.
    ///
    /// An unreadable value degrades to the **default**, not to nothing: a typo in
    /// `applies_to` must not silently un-declare an engine the user did spell
    /// correctly. [`problems`] names it.
    pub fn scope(&self) -> AliasScope {
        self.applies_to.as_deref().and_then(AliasScope::from_wire).unwrap_or_default()
    }

    /// Does it say anything at all? One that does not is noise in the file.
    ///
    /// `applies_to` alone does not count: it says *where* to look for a name that
    /// means nothing yet.
    pub fn is_empty(&self) -> bool {
        self.engine.is_none() && self.role.is_none()
    }

    /// The identity of an alias: its name, reduced to the words it matches on.
    ///
    /// `POS`, `pos` and `01 POS` are the same alias — the same rule that decides
    /// whether a folder matches decides whether two declarations collide, so the
    /// file cannot end up with two entries that fight over the same folders.
    pub fn key(&self) -> String {
        alias_key(&self.name)
    }
}

/// The identity a name reduces to. Public because callers add and remove aliases
/// by name and have to agree with the file about which one they mean.
pub fn alias_key(name: &str) -> String {
    words(name).join(" ")
}

/// Would an alias called `alias_name` apply to a folder called `folder_name`?
///
/// The matching rule on its own, for the one caller that needs to answer "and how
/// many other folders would this affect" before anything is written. It exists so
/// that question is answered by *this* rule rather than by a second
/// implementation of it somewhere else — `POS` matching `01_POS` but not
/// `POSIZIONI` is load-bearing, and a copy of a load-bearing rule is a copy that
/// drifts.
pub fn name_matches(alias_name: &str, folder_name: &str) -> bool {
    let needle = words(alias_name);
    !needle.is_empty() && contains_words(&words(folder_name), &needle)
}

/// The project's vocabulary, compiled once and asked many times.
///
/// Compiling is what turns "a list of strings a human typed" into "something that
/// can answer a question about a folder": names split into words, values resolved
/// to types, and entries that resolved to nothing dropped. The dropping is not
/// silent — [`problems`] reports the same list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AliasVocabulary {
    entries: Vec<CompiledAlias>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompiledAlias {
    /// The words a name has to contain, in order.
    needle: Vec<String>,
    /// The name as the project spells it — the evidence a proposal shows.
    display: String,
    engine: Option<FolderEngine>,
    role: Option<FolderRole>,
    /// Which kind of name this entry answers about.
    scope: AliasScope,
}

impl AliasVocabulary {
    /// A project that declares nothing. Every call site that has no configuration
    /// — a repository being discovered for the first time — passes this, so the
    /// built-in path and the alias path are one code path.
    pub const EMPTY: AliasVocabulary = AliasVocabulary { entries: Vec::new() };

    /// Compile a project's declarations, dropping the ones that say nothing
    /// usable.
    pub fn compile(aliases: &[InferenceAlias]) -> AliasVocabulary {
        let entries = aliases
            .iter()
            .filter_map(|alias| {
                let needle = words(&alias.name);
                let engine = alias.engine();
                let role = alias.role();
                // An unnamed alias would match every folder in the repository;
                // one that resolved to nothing has nothing to say.
                if needle.is_empty() || (engine.is_none() && role.is_none()) {
                    return None;
                }
                Some(CompiledAlias {
                    needle,
                    display: alias.name.clone(),
                    engine,
                    role,
                    scope: alias.scope(),
                })
            })
            .collect();
        AliasVocabulary { entries }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The engine this project declares for a **folder** of this name.
    pub fn engine(&self, folder_name: &str) -> Option<Guess<FolderEngine>> {
        self.best(folder_name, AliasScope::covers_folders, |entry| entry.engine)
    }

    /// The engine this project declares for a **file** of this name.
    ///
    /// Answers only from the entries that opted into file names — see
    /// [`AliasScope`]. A repository that declared nothing gets `None` here for
    /// every file, which is precisely today's behaviour and why this addition
    /// changes nothing until someone asks for it.
    ///
    /// The name is the file's, extension and all; [`crate::infer::file_stem`]
    /// takes the extension off before this is reached, so `.sql` never matches an
    /// alias called `SQL`.
    pub fn file_engine(&self, file_name: &str) -> Option<Guess<FolderEngine>> {
        self.best(file_name, AliasScope::covers_files, |entry| entry.engine)
    }

    /// The role this project declares for a folder of this name.
    ///
    /// Folders only, by design — see [`InferenceAlias::role`].
    pub fn role(&self, folder_name: &str) -> Option<Guess<FolderRole>> {
        self.best(folder_name, AliasScope::covers_folders, |entry| entry.role)
    }

    /// The longest alias that applies here, matches, and answers the question
    /// being asked.
    ///
    /// Longest first for the same reason the built-in tables are: a repository
    /// declaring both `POS` and `POS_LEGACY` means the more specific one where
    /// both apply, and the evidence should name the word a human would point at.
    fn best<T: Copy>(
        &self,
        name: &str,
        applies: impl Fn(AliasScope) -> bool,
        pick: impl Fn(&CompiledAlias) -> Option<T>,
    ) -> Option<Guess<T>> {
        let haystack = words(name);
        let mut best: Option<(&CompiledAlias, T)> = None;
        for entry in &self.entries {
            if !applies(entry.scope) {
                continue;
            }
            let Some(value) = pick(entry) else { continue };
            if !contains_words(&haystack, &entry.needle) {
                continue;
            }
            let better = best
                .map(|(current, _)| entry.display.len() > current.display.len())
                .unwrap_or(true);
            if better {
                best = Some((entry, value));
            }
        }
        best.map(|(entry, value)| Guess::matched(value, entry.display.clone()))
    }
}

/// What is wrong with a project's declared vocabulary, in the user's words.
///
/// Reported rather than fatal: a mistyped engine costs the repository one rule,
/// and refusing to open would leave the user nowhere to fix it from.
pub fn problems(aliases: &[InferenceAlias]) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen: Vec<String> = Vec::new();

    for alias in aliases {
        let key = alias.key();
        if key.is_empty() {
            out.push(
                "an `[[alias]]` has no `name`, so there is no folder it could apply to".to_string(),
            );
            continue;
        }
        if seen.contains(&key) {
            out.push(format!(
                "`{}` is declared as an alias more than once — only the first is used",
                alias.name
            ));
        } else {
            seen.push(key);
        }

        if let Some(engine) = &alias.engine {
            if FolderEngine::from_wire(engine).is_none() {
                out.push(format!(
                    "the alias `{}` names `{engine}`, which is not an engine Picus knows — \
                     folders called `{}` are classified as if the alias were not there ({})",
                    alias.name,
                    alias.name,
                    known_engines()
                ));
            }
        }
        if let Some(role) = &alias.role {
            if FolderRole::from_wire(role).is_none() {
                out.push(format!(
                    "the alias `{}` names the role `{role}`, which is not a folder role — \
                     folders called `{}` keep the role they would have had ({})",
                    alias.name,
                    alias.name,
                    known_roles()
                ));
            }
        }
        if let Some(applies) = &alias.applies_to {
            if AliasScope::from_wire(applies).is_none() {
                out.push(format!(
                    "the alias `{}` says `applies_to = \"{applies}\"`, which is not one of {} — \
                     it is treated as `{}`",
                    alias.name,
                    known_scopes(),
                    AliasScope::default().as_str()
                ));
            }
        }
        // A role is a fact about a directory, so an alias that only declares one
        // and is pointed at file names has nothing left to say. Worth naming: the
        // user wrote two correct-looking lines that together do nothing.
        if alias.scope() == AliasScope::Files && alias.engine.is_none() && alias.role.is_some() {
            out.push(format!(
                "the alias `{}` applies to file names and declares only a role, which is a fact \
                 about a folder — it will classify nothing",
                alias.name
            ));
        }
        if alias.is_empty() {
            out.push(format!(
                "the alias `{}` declares neither an engine nor a role, so it does nothing",
                alias.name
            ));
        }
    }
    out
}

fn known_engines() -> String {
    let mut names: Vec<&str> = EngineKind::ALL.iter().map(|e| e.as_str()).collect();
    names.extend(ForeignEngine::ALL.iter().map(|e| e.as_str()));
    names.join(", ")
}

fn known_roles() -> String {
    FolderRole::ALL.iter().map(|r| r.as_str()).collect::<Vec<_>>().join(", ")
}

fn known_scopes() -> String {
    AliasScope::ALL.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alias(name: &str, engine: Option<&str>, role: Option<&str>) -> InferenceAlias {
        InferenceAlias {
            name: name.to_string(),
            engine: engine.map(str::to_string),
            role: role.map(str::to_string),
            applies_to: None,
        }
    }

    /// The same, pointed at a kind of name.
    fn scoped(name: &str, engine: &str, scope: AliasScope) -> InferenceAlias {
        InferenceAlias { applies_to: Some(scope.as_str().to_string()), ..alias(name, Some(engine), None) }
    }

    /// The real repository: eleven folders of each name, per delivered version.
    fn real() -> Vec<InferenceAlias> {
        vec![
            alias("POS", Some("postgres"), None),
            alias("MSQ", Some("sqlserver"), None),
            alias("DB", Some("db2"), None),
        ]
    }

    #[test]
    fn one_alias_answers_for_every_folder_of_that_name() {
        // The argument the whole shape rests on: a per-path declaration answers
        // for one folder, an alias for all of them and for the next one too.
        let v = AliasVocabulary::compile(&real());
        for version in ["2023", "2024", "2025", "2026"] {
            let name = "POS";
            assert!(v.engine(name).is_some(), "version {version} folder");
        }
        assert_eq!(
            v.engine("POS").unwrap().value,
            FolderEngine::Supported(EngineKind::Postgres)
        );
    }

    #[test]
    fn an_alias_can_say_this_is_not_an_engine_picus_reads() {
        let v = AliasVocabulary::compile(&real());
        assert_eq!(
            v.engine("MSQ").unwrap().value,
            FolderEngine::Unsupported(ForeignEngine::SqlServer)
        );
        assert_eq!(v.engine("DB").unwrap().value, FolderEngine::Unsupported(ForeignEngine::Db2));
        // …and neither yields a dialect, so neither is ever parsed.
        assert!(v.engine("MSQ").unwrap().value.dialect().is_none());
    }

    #[test]
    fn matching_is_whole_word_and_case_insensitive() {
        let v = AliasVocabulary::compile(&real());
        for name in ["POS", "pos", "Pos", "01_POS", "POS-2024", "4.13 POS"] {
            assert!(v.engine(name).is_some(), "{name}");
        }
        // The exact failure that keeps `pos` out of the global vocabulary.
        for name in ["POSIZIONI", "DEPOSITO", "COMPOSITE"] {
            assert!(v.engine(name).is_none(), "{name}");
        }
    }

    #[test]
    fn an_alias_that_names_nothing_matches_nothing() {
        // Otherwise an empty `name` would claim every folder in the repository —
        // the one degradation that could not be recovered from by editing a row.
        let v = AliasVocabulary::compile(&[alias("", Some("postgres"), None)]);
        assert!(v.is_empty());
        assert!(v.engine("ANYTHING").is_none());
        assert!(v.engine("").is_none());
    }

    #[test]
    fn a_bad_value_drops_its_own_entry_and_nothing_else() {
        let aliases = vec![
            alias("MSQ", Some("sqlserver2019"), None),
            alias("CONSEGNE", None, Some("aggiornamento")),
            alias("POS", Some("postgres"), Some("update")),
        ];
        let v = AliasVocabulary::compile(&aliases);
        assert!(v.engine("MSQ").is_none());
        assert!(v.role("CONSEGNE").is_none());
        assert_eq!(
            v.engine("POS").unwrap().value,
            FolderEngine::Supported(EngineKind::Postgres)
        );
        assert_eq!(v.role("POS").unwrap().value, FolderRole::Update);
    }

    #[test]
    fn a_half_bad_alias_keeps_the_half_that_parsed() {
        // The role is fine and the engine is a typo: dropping both would punish
        // the part the user got right.
        let v = AliasVocabulary::compile(&[alias("POS", Some("postgre"), Some("update"))]);
        assert!(v.engine("POS").is_none());
        assert_eq!(v.role("POS").unwrap().value, FolderRole::Update);
    }

    #[test]
    fn every_degradation_is_reported_in_the_users_words() {
        let aliases = vec![
            alias("MSQ", Some("sqlserver2019"), None),
            alias("CONSEGNE", None, Some("aggiornamento")),
            alias("NOTHING", None, None),
            alias("", Some("postgres"), None),
            alias("POS", Some("postgres"), None),
            alias("pos", Some("oracle"), None),
        ];
        let problems = problems(&aliases);

        assert!(problems.iter().any(|p| p.contains("sqlserver2019")), "{problems:?}");
        assert!(problems.iter().any(|p| p.contains("aggiornamento")), "{problems:?}");
        assert!(problems.iter().any(|p| p.contains("neither an engine nor a role")), "{problems:?}");
        assert!(problems.iter().any(|p| p.contains("has no `name`")), "{problems:?}");
        // `POS` and `pos` are the same alias, and two entries fighting over the
        // same folders is a mistake worth naming.
        assert!(problems.iter().any(|p| p.contains("more than once")), "{problems:?}");
        // A healthy vocabulary reports nothing.
        assert!(super::problems(&real()).is_empty());
    }

    #[test]
    fn the_problem_message_lists_what_would_have_worked() {
        let problems = problems(&[alias("MSQ", Some("mssql"), None)]);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("sqlserver"), "{problems:?}");
        assert!(problems[0].contains("oracle"), "{problems:?}");
    }

    #[test]
    fn the_more_specific_alias_wins_where_both_apply() {
        let v = AliasVocabulary::compile(&[
            alias("POS", Some("postgres"), None),
            alias("POS LEGACY", Some("oracle"), None),
        ]);
        assert_eq!(
            v.engine("2019_POS_LEGACY").unwrap().value,
            FolderEngine::Supported(EngineKind::Oracle)
        );
        assert_eq!(
            v.engine("2024_POS").unwrap().value,
            FolderEngine::Supported(EngineKind::Postgres)
        );
    }

    #[test]
    fn the_evidence_is_the_name_the_project_spelled() {
        let v = AliasVocabulary::compile(&real());
        assert_eq!(v.engine("01_pos").unwrap().evidence(), Some("POS"));
    }

    #[test]
    fn an_alias_is_identified_by_the_words_of_its_name() {
        assert_eq!(alias_key("POS"), alias_key("pos"));
        assert_eq!(alias_key("01_POS"), "01 pos");
        assert_eq!(alias_key("  "), "");
        assert_eq!(InferenceAlias::new("MSQ").key(), "msq");
    }

    #[test]
    fn name_matches_answers_the_same_question_the_vocabulary_does() {
        // The count shown in the offer has to agree with what the alias will
        // actually do, so both go through this.
        let v = AliasVocabulary::compile(&real());
        for folder in ["POS", "pos", "01_POS", "POSIZIONI", "DEPOSITO", "ORA"] {
            assert_eq!(
                name_matches("POS", folder),
                v.engine(folder).map(|g| g.value)
                    == Some(FolderEngine::Supported(EngineKind::Postgres)),
                "{folder}"
            );
        }
        // An unnamed alias applies to nothing, here as everywhere.
        assert!(!name_matches("", "POS"));
        assert!(!name_matches("  ", "POS"));
    }

    // ── Where the name is looked for ──────────────────────────────────────────

    #[test]
    fn an_alias_says_nothing_about_a_file_name_until_it_is_asked_to() {
        // The whole safety of the feature: every alias written before `applies_to`
        // existed, and every one written without it, classifies folders and only
        // folders. Turning it on is a word somebody types.
        let v = AliasVocabulary::compile(&real());
        assert!(v.engine("POS").is_some(), "folders, as always");
        for name in ["POS", "4_12_POS", "01_pos"] {
            assert!(v.file_engine(name).is_none(), "{name} must not be classified by name yet");
        }
    }

    #[test]
    fn an_alias_pointed_at_files_classifies_the_scattered_ones() {
        // The dirty repository: one folder, two engines, and the only thing that
        // knows which is which is the file name.
        let v = AliasVocabulary::compile(&[
            scoped("POS", "postgres", AliasScope::Both),
            scoped("ORA", "oracle", AliasScope::Both),
        ]);
        assert_eq!(
            v.file_engine("4_12_POS").map(|g| g.value),
            Some(FolderEngine::Supported(EngineKind::Postgres))
        );
        assert_eq!(
            v.file_engine("4_12_ORA").map(|g| g.value),
            Some(FolderEngine::Supported(EngineKind::Oracle))
        );
        // …and the same whole-word rule as everywhere else, so an ordinary
        // Italian file name is left alone.
        for name in ["AGGIORNA_ORARI", "POSIZIONI", "DEPOSITO"] {
            assert!(v.file_engine(name).is_none(), "{name}");
        }
    }

    #[test]
    fn files_only_means_files_only_and_folders_only_means_folders_only() {
        let v = AliasVocabulary::compile(&[
            scoped("POS", "postgres", AliasScope::Files),
            scoped("ORA", "oracle", AliasScope::Folders),
        ]);
        assert!(v.file_engine("4_12_POS").is_some());
        assert!(v.engine("POS").is_none(), "this one was pointed at files");
        assert!(v.engine("ORA").is_some());
        assert!(v.file_engine("4_12_ORA").is_none(), "and this one at folders");
    }

    #[test]
    fn a_role_is_a_fact_about_a_folder_whatever_the_scope_says() {
        // Roles do not vary file by file — the file beside another in one folder
        // is for the same thing — so `applies_to` moves the engine and nothing
        // else. Asserted because the opposite is the tempting shortcut.
        let v = AliasVocabulary::compile(&[InferenceAlias {
            applies_to: Some("both".to_string()),
            ..alias("CONSEGNE", Some("postgres"), Some("update"))
        }]);
        assert!(v.role("CONSEGNE").is_some(), "folders still get the role");
        assert!(v.file_engine("4_12_CONSEGNE").is_some(), "and files get the engine");
    }

    #[test]
    fn a_scope_that_is_a_typo_degrades_to_the_default_rather_than_to_nothing() {
        // The rule that keeps a typo cheap: `applies_to` is about *where* to look,
        // so an unreadable one must not un-declare an engine that is spelled
        // correctly. The alias keeps working on folders, and the user is told.
        let typo = InferenceAlias {
            applies_to: Some("file".to_string()), // singular — not a word Picus knows
            ..alias("POS", Some("postgres"), None)
        };
        let v = AliasVocabulary::compile(std::slice::from_ref(&typo));
        assert!(v.engine("POS").is_some(), "the engine survived the typo");
        assert!(v.file_engine("4_12_POS").is_none(), "…but the unreadable scope claims nothing");

        let problems = problems(std::slice::from_ref(&typo));
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("file"), "{problems:?}");
        assert!(problems[0].contains("folders"), "{problems:?}");
    }

    #[test]
    fn an_alias_that_only_declares_a_role_and_points_at_files_is_reported() {
        // Two lines that each look right and together do nothing.
        let alias = InferenceAlias {
            applies_to: Some("files".to_string()),
            ..alias("CONSEGNE", None, Some("update"))
        };
        let problems = problems(&[alias]);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("classify nothing"), "{problems:?}");
    }

    #[test]
    fn the_scope_round_trips_and_defaults_to_folders() {
        for scope in AliasScope::ALL {
            assert_eq!(AliasScope::from_wire(scope.as_str()), Some(*scope));
        }
        assert_eq!(AliasScope::from_wire("everything"), None);
        assert_eq!(AliasScope::default(), AliasScope::Folders);
        // An alias that never mentions it means folders — the behaviour every
        // existing project file already relies on.
        assert_eq!(alias("POS", Some("postgres"), None).scope(), AliasScope::Folders);
    }

    #[test]
    fn the_empty_vocabulary_answers_nothing_and_costs_nothing() {
        assert!(AliasVocabulary::EMPTY.is_empty());
        assert!(AliasVocabulary::EMPTY.engine("POS").is_none());
        assert!(AliasVocabulary::EMPTY.role("POS").is_none());
    }
}
