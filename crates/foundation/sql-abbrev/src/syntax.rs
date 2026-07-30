//! What the parser produces: the abbreviation as typed, with a span on every
//! position the grammar allows.
//!
//! Nothing here is resolved. A table is the text the user typed, a column may not
//! exist, an operator may be nonsense — the schema has not been consulted yet.
//! That separation is what lets one parse answer "what is under the caret" for an
//! abbreviation that could never expand.

use serde::{Deserialize, Serialize};

use crate::span::{Slot, Span};

/// The verbs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verb {
    Select,
    Insert,
    Update,
    Delete,
    /// `m#` — one row, inserted or updated depending on whether the key is there.
    Merge,
    /// `a#` — a column added to or retyped on an existing table.
    Alter,
    /// `fc#` — a `SELECT` with a procedural loop wrapped around it.
    ForCursor,
}

impl Verb {
    /// Every verb, in the order a completion list should offer them.
    ///
    /// The four that write rows first, because they are what the language is for;
    /// the three that shape or iterate follow.
    pub const ALL: &'static [Verb] = &[
        Verb::Select,
        Verb::Insert,
        Verb::Update,
        Verb::Delete,
        Verb::Merge,
        Verb::Alter,
        Verb::ForCursor,
    ];

    /// The short form, which is the point of the whole language.
    pub fn marker(self) -> &'static str {
        match self {
            Verb::Select => "s",
            Verb::Insert => "i",
            Verb::Update => "u",
            Verb::Delete => "d",
            Verb::Merge => "m",
            Verb::Alter => "a",
            // Two letters, because `f` alone reads as nothing in particular and
            // this is the only construct in the language that is a *block*.
            Verb::ForCursor => "fc",
        }
    }

    /// The SQL word, upper case.
    pub fn keyword(self) -> &'static str {
        match self {
            Verb::Select => "SELECT",
            Verb::Insert => "INSERT",
            Verb::Update => "UPDATE",
            Verb::Delete => "DELETE",
            Verb::Merge => "MERGE",
            Verb::Alter => "ALTER",
            Verb::ForCursor => "FOR",
        }
    }

    /// What the verb is called when a sentence needs to name it — the keyword for
    /// most, something readable for the one whose keyword is a preposition.
    pub fn describe(self) -> &'static str {
        match self {
            Verb::ForCursor => "FOR loop",
            other => other.keyword(),
        }
    }

    /// The marker, the whole word, or a familiar synonym.
    ///
    /// The word is accepted as well as the letter because a user who has just met
    /// the feature types `select#…` before they trust `s#…`, and refusing that
    /// teaches them nothing. It is never *produced* — the marker is the language.
    pub fn from_word(word: &str) -> Option<Verb> {
        let lower = word.trim().to_ascii_lowercase();
        if let Some((_, verb)) = SYNONYMS.iter().find(|(w, _)| *w == lower) {
            return Some(*verb);
        }
        Verb::ALL
            .iter()
            .copied()
            .find(|v| lower == v.marker() || lower == v.keyword().to_ascii_lowercase())
    }
}

/// Words that mean a verb without being its marker or its keyword.
///
/// `upsert` is here because it is what most people call the thing, and `for` /
/// `loop` because `FOR` alone is not a statement anybody would search for.
const SYNONYMS: &[(&str, Verb)] = &[
    ("upsert", Verb::Merge),
    ("alter", Verb::Alter),
    ("for", Verb::ForCursor),
    ("loop", Verb::ForCursor),
    ("forcursor", Verb::ForCursor),
];

/// A comma-separated list between brackets, however far the user has got with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block<T> {
    /// From the opening bracket to wherever the list stopped.
    pub span: Span,
    pub items: Vec<T>,
    /// Was the closing bracket actually typed?
    pub closed: bool,
}

/// `>table` or `>table:column` — one hop along the chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainLink {
    /// Offset of the `>`.
    pub arrow: usize,
    pub table: Slot,
    /// The disambiguating column, when the user typed `:`.
    pub column: Option<Slot>,
}

/// A value as written: quoted or bare.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawValue {
    /// The source text **including** the quotes, when there are any.
    pub slot: Slot,
    pub quoted: bool,
    /// False for a string the user has not closed yet — fine for a completion,
    /// fatal for an expansion.
    pub terminated: bool,
}

impl RawValue {
    pub fn is_blank(&self) -> bool {
        self.slot.is_blank()
    }

    /// The value without its quotes, with `''` folded back to `'`.
    pub fn inner(&self) -> String {
        if !self.quoted {
            return self.slot.text.clone();
        }
        let body = self.slot.text.strip_prefix('\'').unwrap_or(&self.slot.text);
        let body = if self.terminated { body.strip_suffix('\'').unwrap_or(body) } else { body };
        body.replace("''", "'")
    }
}

/// One entry of a `(...)` list: a column name, and for `u#` an assigned value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColItem {
    pub name: Slot,
    /// Offset of the `=`, when the user typed one.
    pub eq: Option<usize>,
    /// Present exactly when `eq` is — possibly blank, because `u#t(a=` is a
    /// perfectly good thing to ask for a completion in.
    pub value: Option<RawValue>,
}

/// One entry of a `[...]` list: `name op value`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PredItem {
    pub name: Slot,
    /// Blank when the user has not typed one yet.
    pub op: Slot,
    pub value: RawValue,
}

/// Adding a column, or retyping one that is already there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChangeKind {
    /// `+name:type`
    Add,
    /// `~name:type`
    Modify,
}

impl ChangeKind {
    pub fn symbol(self) -> char {
        match self {
            ChangeKind::Add => '+',
            ChangeKind::Modify => '~',
        }
    }
}

/// One `+name:type` or `~name:type` of an `a#`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeItem {
    /// Offset of the `+` or `~`.
    pub at: usize,
    pub kind: ChangeKind,
    pub column: Slot,
    /// Everything after the `:`, as typed. `None` while the user has not typed the
    /// `:` yet — which is most of the time they spend on this line.
    pub data_type: Option<Slot>,
}

/// Where the input stopped making sense, and what was there.
///
/// One per parse. The parser stops describing structure at the first thing it
/// cannot place, because a second error is nearly always a consequence of the
/// first and reporting both makes the real one harder to find.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxError {
    pub at: usize,
    pub message: String,
}

/// The abbreviation as typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed {
    /// The text before the `#` — not yet known to be a verb.
    pub verb: Slot,
    /// Offset of the `#`. `None` while the user is still typing the verb.
    pub hash: Option<usize>,
    pub table: Slot,
    pub chain: Vec<ChainLink>,
    /// The `+col:type` / `~col:type` list of an `a#`.
    pub changes: Vec<ChangeItem>,
    pub cols: Option<Block<ColItem>>,
    pub preds: Option<Block<PredItem>>,
    /// The digits after `*`.
    pub mult: Option<Slot>,
    /// The `{…}` row template: one value per column, with `$` standing for the
    /// row number. What turns `*3` from three identical rows into three rows.
    pub template: Option<Block<RawValue>>,
    pub error: Option<SyntaxError>,
}

impl Parsed {
    /// Every table named in the chain, as typed, skipping the ones not typed yet.
    pub fn table_names(&self) -> Vec<String> {
        std::iter::once(&self.table)
            .chain(self.chain.iter().map(|l| &l.table))
            .filter(|s| !s.is_blank())
            .map(|s| s.text.clone())
            .collect()
    }

    /// The table a link hangs off — the previous one in the chain.
    pub fn link_source(&self, index: usize) -> String {
        if index == 0 {
            self.table.text.clone()
        } else {
            self.chain[index - 1].table.text.clone()
        }
    }
}
