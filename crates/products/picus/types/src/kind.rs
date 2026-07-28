//! [`EngineKind`] — which database engine a connection, a folder or a generated
//! statement belongs to.
//!
//! This is the same vocabulary the product calls a **dialect**, and the two are
//! deliberately one type: the engine that a live connection speaks and the dialect
//! a script folder is written in must never drift apart. What must NOT follow from
//! that is an ambient value — see the note on [`EngineKind`].

use serde::{Deserialize, Serialize};

/// Serialise a small closed vocabulary as its plain wire word.
///
/// Hand-written rather than derived because the types below mix newtype and unit
/// variants: `#[serde(untagged)]` spells a unit variant `null`, and `null` is the
/// one value that already means something else here — "nobody knows". One string
/// per value keeps the project file readable (`dialect = "generic"`) and the
/// interface's job trivial.
macro_rules! wire_string_serde {
    ($type:ty, $expected:literal) => {
        impl Serialize for $type {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(self.as_str())
            }
        }

        impl<'de> Deserialize<'de> for $type {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let word = <std::borrow::Cow<'de, str>>::deserialize(d)?;
                <$type>::from_wire(&word).ok_or_else(|| {
                    serde::de::Error::invalid_value(serde::de::Unexpected::Str(&word), &$expected)
                })
            }
        }
    };
}

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

/// An engine Picus **recognises and does not support**.
///
/// The third state, and it is not a nicety. Before this existed a folder was
/// either one of the two dialects or "no engine — somebody please classify me",
/// and a repository whose `MSQ` folders are SQL Server got the second answer
/// forever. Never being able to answer a question is a different fact from not
/// knowing the answer, and it leads to different behaviour: a folder in this
/// state is named on screen, asked about **never**, and left out of every lane,
/// every comparison and — the part that matters most — every parse. Handing
/// T-SQL to a PostgreSQL grammar yields a plausible-looking parse tree, which is
/// considerably worse than no parse at all.
///
/// Deliberately a separate type from [`EngineKind`] rather than more variants of
/// it: `EngineKind` is what a driver connects with and what an emitter writes,
/// and every `match` over it would have to grow an arm claiming Picus can emit
/// T-SQL. It cannot, and the type system should keep saying so.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ForeignEngine {
    SqlServer,
    Db2,
    MySql,
    MariaDb,
    Sqlite,
}

impl ForeignEngine {
    /// The stable wire string — also the value the frontend's `ForeignEngine` uses.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SqlServer => "sqlserver",
            Self::Db2 => "db2",
            Self::MySql => "mysql",
            Self::MariaDb => "mariadb",
            Self::Sqlite => "sqlite",
        }
    }

    /// How the engine is spelled on screen.
    pub fn label(self) -> &'static str {
        match self {
            Self::SqlServer => "SQL Server",
            Self::Db2 => "DB2",
            Self::MySql => "MySQL",
            Self::MariaDb => "MariaDB",
            Self::Sqlite => "SQLite",
        }
    }

    pub fn from_wire(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|e| e.as_str() == s)
    }

    /// Every unsupported engine Picus can name, in display order.
    pub const ALL: &'static [ForeignEngine] = &[
        ForeignEngine::SqlServer,
        ForeignEngine::Db2,
        ForeignEngine::MySql,
        ForeignEngine::MariaDb,
        ForeignEngine::Sqlite,
    ];
}

impl std::fmt::Display for ForeignEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// Which dialects a piece of SQL has to be valid in.
///
/// Deliberately **not** [`FolderEngine`]: this type has no "unsupported" and no
/// "unknown". That absence is the structural half of a guarantee — nothing is
/// ever parsed, analysed or emitted for an engine Picus does not speak, because
/// there is no way to *say* it here. [`FolderEngine::scope`] is the only bridge,
/// and it returns `None` for the states that have no business reaching a parser
/// or an emitter.
///
/// The two questions it answers are duals, and keeping them apart is what makes
/// portable folders work:
///
/// | | [`covers`](Self::covers) — "does content here count for that engine?" | [`permits_syntax_of`](Self::permits_syntax_of) — "may syntax specific to it appear?" |
/// |---|---|---|
/// | `One(Oracle)` | Oracle only | Oracle only |
/// | `Portable` | **both** | **neither** |
///
/// That inversion is the whole feature. A row inserted by a portable script is
/// present on both engines, so it fills a gap on both; and a construct belonging
/// to *either* engine is a finding there, because the file promised to run on
/// both and does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DialectScope {
    /// Exactly one dialect. Everything that dialect allows is fair game.
    One(EngineKind),
    /// Every dialect Picus supports. Only the **intersection** is allowed.
    Portable,
}

impl DialectScope {
    /// Does content written here count as present for `dialect`?
    ///
    /// This is the lane question, and it is the reason a portable folder is the
    /// first thing in the model to belong to more than one lane.
    pub fn covers(self, dialect: EngineKind) -> bool {
        match self {
            DialectScope::One(kind) => kind == dialect,
            DialectScope::Portable => true,
        }
    }

    /// May syntax that only `dialect` understands appear here?
    ///
    /// `false` for **every** dialect under `Portable`: a script that promises to
    /// run on both engines may use what both engines understand and nothing else.
    pub fn permits_syntax_of(self, dialect: EngineKind) -> bool {
        matches!(self, DialectScope::One(kind) if kind == dialect)
    }

    /// The single dialect, when there is one. `None` for `Portable` — and that
    /// `None` is why every dialect-dependent decision in the emitter had to grow
    /// a portable answer rather than quietly defaulting to one engine.
    pub fn dialect(self) -> Option<EngineKind> {
        match self {
            DialectScope::One(kind) => Some(kind),
            DialectScope::Portable => None,
        }
    }

    /// Every dialect this scope answers for.
    pub fn dialects(self) -> &'static [EngineKind] {
        match self {
            DialectScope::One(EngineKind::Oracle) => &[EngineKind::Oracle],
            DialectScope::One(EngineKind::Postgres) => &[EngineKind::Postgres],
            DialectScope::Portable => EngineKind::ALL,
        }
    }

    pub fn is_portable(self) -> bool {
        matches!(self, DialectScope::Portable)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            DialectScope::One(kind) => kind.as_str(),
            DialectScope::Portable => GENERIC_WIRE,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            DialectScope::One(kind) => dialect_label(kind),
            DialectScope::Portable => GENERIC_LABEL,
        }
    }

    pub fn from_wire(s: &str) -> Option<Self> {
        if s == GENERIC_WIRE {
            return Some(DialectScope::Portable);
        }
        EngineKind::from_wire(s).map(DialectScope::One)
    }
}

impl std::fmt::Display for DialectScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

wire_string_serde!(DialectScope, "a dialect or `generic`");

/// The wire word for portable SQL, in both `FolderEngine` and `DialectScope`.
const GENERIC_WIRE: &str = "generic";
const GENERIC_LABEL: &str = "Portable SQL";

fn dialect_label(kind: EngineKind) -> &'static str {
    match kind {
        EngineKind::Postgres => "PostgreSQL",
        EngineKind::Oracle => "Oracle",
    }
}

/// The engine a folder of scripts is written in — all four answers.
///
/// One slot, because a folder has one engine, and it serialises as one plain wire
/// word: `dialect = "oracle"`, `"generic"`, `"sqlserver"` are the same key in the
/// same file.
///
/// | | Means | What happens |
/// |---|---|---|
/// | `Supported(_)` | Oracle / PostgreSQL | parsed, analysed, generated into |
/// | `Generic` | portable SQL, valid on **both** | parsed against both, counts for both, generated into with the intersection |
/// | `Unsupported(_)` | recognised, unsupported | named, never asked about, never parsed |
/// | *(absent)* | nobody knows | the interface asks |
///
/// The two that must never collapse into each other are the last two: not being
/// able to answer a question is a different fact from not knowing the answer, and
/// they produce different words on screen and different behaviour.
///
/// `Generic` is **never inferred**. No keyword produces it and no heuristic
/// reaches it; it only ever arrives because someone declared it, which is the
/// only honest source for "I promise these scripts run on both engines".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FolderEngine {
    /// Picus reads, analyses and generates this one.
    Supported(EngineKind),
    /// Portable SQL — valid on every dialect Picus supports.
    Generic,
    /// Picus can name it, and does nothing else with it.
    Unsupported(ForeignEngine),
}

impl FolderEngine {
    /// The **single** dialect to emit and parse with — `None` for portable SQL as
    /// well as for an engine Picus only recognises.
    ///
    /// Callers that mean "which dialects does this answer for" want
    /// [`scope`](Self::scope) or [`covers`](Self::covers) instead; this one is
    /// specifically "is there exactly one, and which".
    pub fn dialect(self) -> Option<EngineKind> {
        self.scope().and_then(DialectScope::dialect)
    }

    /// What this folder's SQL has to be valid in — `None` when the question does
    /// not apply, which is exactly the unsupported case.
    ///
    /// The single bridge from "what a folder is" to "what may be parsed and
    /// emitted", and the reason an unsupported folder cannot reach either.
    pub fn scope(self) -> Option<DialectScope> {
        match self {
            FolderEngine::Supported(kind) => Some(DialectScope::One(kind)),
            FolderEngine::Generic => Some(DialectScope::Portable),
            FolderEngine::Unsupported(_) => None,
        }
    }

    /// Does content in this folder count as present for `dialect`?
    pub fn covers(self, dialect: EngineKind) -> bool {
        self.scope().map(|s| s.covers(dialect)).unwrap_or(false)
    }

    /// Every dialect this folder answers for — two for portable, one for a
    /// dialect, none for an unsupported engine.
    pub fn dialects(self) -> &'static [EngineKind] {
        self.scope().map(DialectScope::dialects).unwrap_or(&[])
    }

    /// The engine when Picus does not support it, `None` otherwise.
    pub fn foreign(self) -> Option<ForeignEngine> {
        match self {
            FolderEngine::Unsupported(engine) => Some(engine),
            _ => None,
        }
    }

    pub fn is_generic(self) -> bool {
        matches!(self, FolderEngine::Generic)
    }

    /// Does Picus read, analyse and generate this folder at all?
    pub fn is_readable(self) -> bool {
        self.scope().is_some()
    }

    pub fn as_str(self) -> &'static str {
        match self {
            FolderEngine::Supported(kind) => kind.as_str(),
            FolderEngine::Generic => GENERIC_WIRE,
            FolderEngine::Unsupported(engine) => engine.as_str(),
        }
    }

    /// How it is spelled on screen.
    pub fn label(self) -> &'static str {
        match self {
            FolderEngine::Supported(kind) => dialect_label(kind),
            FolderEngine::Generic => GENERIC_LABEL,
            FolderEngine::Unsupported(engine) => engine.label(),
        }
    }

    /// Parse a wire word.
    pub fn from_wire(s: &str) -> Option<Self> {
        if s == GENERIC_WIRE {
            return Some(FolderEngine::Generic);
        }
        EngineKind::from_wire(s)
            .map(FolderEngine::Supported)
            .or_else(|| ForeignEngine::from_wire(s).map(FolderEngine::Unsupported))
    }
}

impl std::fmt::Display for FolderEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

wire_string_serde!(FolderEngine, "an engine Picus can name");

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

    #[test]
    fn a_foreign_engine_round_trips_and_is_spelled_properly() {
        for engine in ForeignEngine::ALL {
            assert_eq!(ForeignEngine::from_wire(engine.as_str()), Some(*engine));
            assert_eq!(serde_json::to_string(engine).unwrap(), format!("\"{}\"", engine.as_str()));
        }
        assert_eq!(ForeignEngine::SqlServer.label(), "SQL Server");
        assert_eq!(ForeignEngine::from_wire("oracle"), None);
    }

    #[test]
    fn a_folder_engine_is_one_wire_word_whatever_it_is() {
        // One slot, one key: `dialect = "oracle"`, `"generic"` and `"sqlserver"`
        // are the same field in the same file.
        for (value, word) in [
            (FolderEngine::Supported(EngineKind::Oracle), "\"oracle\""),
            (FolderEngine::Generic, "\"generic\""),
            (FolderEngine::Unsupported(ForeignEngine::Db2), "\"db2\""),
        ] {
            assert_eq!(serde_json::to_string(&value).unwrap(), word);
            assert_eq!(serde_json::from_str::<FolderEngine>(word).unwrap(), value);
        }
        assert!(serde_json::from_str::<FolderEngine>("\"mssql\"").is_err());
        // `null` still means "nobody knows" — which is why a unit variant must
        // not serialise as null, and why this serde is hand-written.
        assert_eq!(serde_json::from_str::<Option<FolderEngine>>("null").unwrap(), None);
    }

    #[test]
    fn only_a_readable_engine_yields_a_scope_to_parse_and_emit_with() {
        // This is what keeps T-SQL out of the grammar: everything downstream asks
        // for a scope, and an unsupported engine has none to give.
        assert_eq!(
            FolderEngine::Supported(EngineKind::Postgres).scope(),
            Some(DialectScope::One(EngineKind::Postgres))
        );
        assert_eq!(FolderEngine::Generic.scope(), Some(DialectScope::Portable));
        assert_eq!(FolderEngine::Unsupported(ForeignEngine::SqlServer).scope(), None);

        assert!(FolderEngine::Generic.is_readable());
        assert!(!FolderEngine::Unsupported(ForeignEngine::SqlServer).is_readable());
        assert_eq!(
            FolderEngine::Unsupported(ForeignEngine::SqlServer).foreign(),
            Some(ForeignEngine::SqlServer)
        );
    }

    #[test]
    fn portable_sql_has_no_single_dialect_but_answers_for_both() {
        // The two questions are duals, and the whole feature lives in the gap
        // between them.
        let generic = FolderEngine::Generic;
        assert_eq!(generic.dialect(), None, "there is no single one to emit with");
        for dialect in EngineKind::ALL {
            assert!(generic.covers(*dialect), "{dialect} — a row here is present there");
        }
        assert_eq!(generic.dialects(), EngineKind::ALL);
        assert!(generic.is_generic());
    }

    #[test]
    fn an_unsupported_engine_covers_nothing_and_a_dialect_covers_only_itself() {
        let oracle = FolderEngine::Supported(EngineKind::Oracle);
        assert!(oracle.covers(EngineKind::Oracle));
        assert!(!oracle.covers(EngineKind::Postgres));
        assert_eq!(oracle.dialects(), &[EngineKind::Oracle]);

        let msq = FolderEngine::Unsupported(ForeignEngine::SqlServer);
        assert!(EngineKind::ALL.iter().all(|d| !msq.covers(*d)));
        assert!(msq.dialects().is_empty());
    }

    #[test]
    fn a_portable_scope_covers_both_dialects_and_permits_the_syntax_of_neither() {
        // The inversion `DIA001` rests on: under `Portable`, a construct
        // belonging to *either* engine is foreign, because the file promised both.
        let portable = DialectScope::Portable;
        for dialect in EngineKind::ALL {
            assert!(portable.covers(*dialect), "{dialect}");
            assert!(!portable.permits_syntax_of(*dialect), "{dialect}");
        }
        assert!(portable.is_portable());
        assert_eq!(portable.dialect(), None);

        let oracle = DialectScope::One(EngineKind::Oracle);
        assert!(oracle.permits_syntax_of(EngineKind::Oracle));
        assert!(!oracle.permits_syntax_of(EngineKind::Postgres));
        assert_eq!(oracle.dialect(), Some(EngineKind::Oracle));
    }

    #[test]
    fn a_scope_cannot_name_an_engine_picus_does_not_speak() {
        // The structural half of the guarantee: there is no variant for it, so a
        // parse target or a generation target simply cannot be one.
        assert_eq!(DialectScope::from_wire("sqlserver"), None);
        assert_eq!(DialectScope::from_wire("generic"), Some(DialectScope::Portable));
        assert_eq!(serde_json::to_string(&DialectScope::Portable).unwrap(), "\"generic\"");
        assert!(serde_json::from_str::<DialectScope>("\"db2\"").is_err());
    }
}
