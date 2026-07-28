//! Text the user typed → things the schema actually has.
//!
//! Three jobs, all of them the reason this is worth more than a snippet: finding
//! a table or column and reporting the near-miss when there is none, giving every
//! table in a chain a stable alias, and reading a join condition **out of a
//! foreign key** rather than inventing one.

use crate::error::AbbrevError;
use crate::schema::{SchemaView, TableMeta, ValueKind};
use crate::statement::{ColumnRef, TableRef};

/// Find a table by the name the user typed, or refuse with the nearest one.
pub fn lookup_table<'a>(schema: &'a SchemaView, typed: &str) -> Result<&'a TableMeta, AbbrevError> {
    schema.table(typed).ok_or_else(|| AbbrevError::UnknownTable {
        name: typed.to_string(),
        suggestion: suggest(typed, schema.table_names()),
    })
}

/// One table of a statement, with the alias it will be written under.
pub struct Bound<'a> {
    pub meta: &'a TableMeta,
    /// `None` when the statement has a single table — an alias nobody needs is
    /// noise in a one-line abbreviation's whole output.
    pub alias: Option<String>,
}

/// Every table an abbreviation names, in the order it named them.
pub struct Chain<'a> {
    pub bounds: Vec<Bound<'a>>,
}

impl<'a> Chain<'a> {
    /// Resolve the typed names, then alias them.
    pub fn build(schema: &'a SchemaView, typed: &[String]) -> Result<Self, AbbrevError> {
        let metas: Vec<&TableMeta> =
            typed.iter().map(|name| lookup_table(schema, name)).collect::<Result<_, _>>()?;
        let names: Vec<&str> = metas.iter().map(|m| m.name.as_str()).collect();
        let aliases = if metas.len() > 1 { aliases(&names) } else { vec![String::new()] };
        let bounds = metas
            .into_iter()
            .zip(aliases)
            .map(|(meta, alias)| Bound { meta, alias: (!alias.is_empty()).then_some(alias) })
            .collect();
        Ok(Chain { bounds })
    }

    pub fn root(&self) -> &Bound<'a> {
        &self.bounds[0]
    }

    /// The canonical names, for an error message that has to list them.
    pub fn names(&self) -> Vec<String> {
        self.bounds.iter().map(|b| b.meta.name.clone()).collect()
    }

    pub fn table_refs(&self) -> Vec<TableRef> {
        self.bounds
            .iter()
            .map(|b| TableRef { name: b.meta.name.clone(), alias: b.alias.clone() })
            .collect()
    }

    /// Resolve a column reference, qualified (`ordini.id`) or not.
    ///
    /// Unqualified, it is looked for in **every** table of the chain, and a name
    /// that is in more than one is refused rather than bound to the first — the
    /// first is an accident of the order the user happened to type the tables in.
    pub fn column(&self, raw: &str) -> Result<ColumnRef, AbbrevError> {
        if let Some((qualifier, name)) = raw.split_once('.') {
            let bound = self
                .bounds
                .iter()
                .find(|b| matches(&b.meta.name, qualifier) || b.alias.as_deref().is_some_and(|a| matches(a, qualifier)))
                .ok_or_else(|| AbbrevError::UnknownQualifier {
                    qualifier: qualifier.to_string(),
                    tables: self.names(),
                })?;
            return self.bind(bound, name).ok_or_else(|| AbbrevError::UnknownColumn {
                name: name.to_string(),
                tables: vec![bound.meta.name.clone()],
                suggestion: suggest(name, bound.meta.column_names()),
            });
        }

        let mut found: Vec<ColumnRef> = Vec::new();
        for bound in &self.bounds {
            if let Some(column) = self.bind(bound, raw) {
                found.push(column);
            }
        }
        match found.len() {
            1 => Ok(found.remove(0)),
            0 => Err(AbbrevError::UnknownColumn {
                name: raw.to_string(),
                tables: self.names(),
                suggestion: suggest(raw, self.bounds.iter().flat_map(|b| b.meta.column_names())),
            }),
            _ => Err(AbbrevError::AmbiguousColumn {
                name: raw.to_string(),
                tables: found.into_iter().map(|c| c.table).collect(),
            }),
        }
    }

    fn bind(&self, bound: &Bound<'a>, name: &str) -> Option<ColumnRef> {
        let column = bound.meta.column(name)?;
        Some(ColumnRef {
            name: column.name.clone(),
            table: bound.meta.name.clone(),
            alias: bound.alias.clone(),
            kind: column.kind,
        })
    }

    /// A column named by a foreign key, which the schema's column list may not
    /// cover — a host is free to hand over keys without every column.
    ///
    /// The key wins: it is the schema's own statement about how the tables relate,
    /// and refusing the join because the column list was partial would turn a host's
    /// economy into a missing feature.
    pub fn key_column(&self, index: usize, name: &str) -> ColumnRef {
        let bound = &self.bounds[index];
        self.bind(bound, name).unwrap_or(ColumnRef {
            name: name.to_string(),
            table: bound.meta.name.clone(),
            alias: bound.alias.clone(),
            kind: ValueKind::Other,
        })
    }
}

/// Identifiers compare case-insensitively: nobody types `LOCALSTRINGS`.
pub fn matches(schema_name: &str, typed: &str) -> bool {
    schema_name.eq_ignore_ascii_case(typed)
}

/// First letter, deduplicated: `ORDINI`, `CLIENTI`, `CLIENTI_FATT` → `O`, `C`, `C2`.
///
/// Deterministic by construction — same input, same aliases, always — because an
/// alias appears in the output the user reads and edits, and one that moved
/// between two runs of the same abbreviation would be worse than no alias.
pub fn aliases(names: &[&str]) -> Vec<String> {
    let mut used: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for name in names {
        let base = initial(name);
        let mut candidate = base.clone();
        let mut nth = 1;
        while used.iter().any(|u| u.eq_ignore_ascii_case(&candidate)) {
            nth += 1;
            candidate = format!("{base}{nth}");
        }
        used.push(candidate.clone());
        out.push(candidate);
    }
    out
}

/// The alias follows the table's own case, so a lower-case schema does not get
/// SQL with one capital letter in it.
fn initial(name: &str) -> String {
    let first = name.chars().find(|c| c.is_alphabetic()).unwrap_or('t');
    if name.chars().any(char::is_uppercase) {
        first.to_uppercase().to_string()
    } else {
        first.to_lowercase().to_string()
    }
}

/// The nearest name, when there is exactly one and it is near enough.
///
/// Silent when the best match ties with another, because a suggestion that might
/// be the wrong one costs more than no suggestion: the user reads it, tries it,
/// and is now two mistakes from where they were.
pub fn suggest<'a>(typed: &str, candidates: impl IntoIterator<Item = &'a str>) -> Option<String> {
    let budget = if typed.chars().count() <= 4 { 1 } else { 2 };
    let mut best: Option<(usize, &str)> = None;
    let mut tied = false;
    for candidate in candidates {
        let distance = edit_distance(typed, candidate);
        match best {
            Some((d, _)) if distance > d => {}
            Some((d, _)) if distance == d => tied = true,
            _ => {
                best = Some((distance, candidate));
                tied = false;
            }
        }
    }
    match best {
        Some((distance, name)) if distance > 0 && distance <= budget && !tied => Some(name.to_string()),
        _ => None,
    }
}

/// Levenshtein, case-insensitive, on characters rather than bytes.
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.to_lowercase().chars().collect();
    let b: Vec<char> = b.to_lowercase().chars().collect();
    if a.is_empty() {
        return b.len();
    }
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0usize; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        current[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let substitute = previous[j] + usize::from(ca != cb);
            current[j + 1] = substitute.min(previous[j + 1] + 1).min(current[j] + 1);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases_are_first_letters_deduplicated() {
        assert_eq!(aliases(&["ORDINI", "CLIENTI", "CLIENTI_FATT"]), vec!["O", "C", "C2"]);
        // A self-join gets the same treatment.
        assert_eq!(aliases(&["DIPENDENTI", "DIPENDENTI"]), vec!["D", "D2"]);
    }

    #[test]
    fn an_alias_follows_the_tables_own_case() {
        assert_eq!(aliases(&["ordini", "clienti"]), vec!["o", "c"]);
        assert_eq!(aliases(&["Ordini"]), vec!["O"]);
        // …and a name that starts with a digit still gets one.
        assert_eq!(aliases(&["1_TEMP"]), vec!["T"]);
    }

    #[test]
    fn the_same_chain_always_produces_the_same_aliases() {
        let once = aliases(&["A_ONE", "A_TWO", "A_THREE", "B"]);
        assert_eq!(once, aliases(&["A_ONE", "A_TWO", "A_THREE", "B"]));
        assert_eq!(once, vec!["A", "A2", "A3", "B"]);
    }

    #[test]
    fn a_near_miss_is_suggested_and_a_far_one_is_not() {
        let tables = ["LOCALSTRINGS", "ORDINI", "CLIENTI"];
        assert_eq!(suggest("localstring", tables), Some("LOCALSTRINGS".to_string()));
        assert_eq!(suggest("ordni", tables), Some("ORDINI".to_string()));
        assert_eq!(suggest("qqqqqqqq", tables), None);
    }

    #[test]
    fn a_tie_suggests_nothing() {
        // `COD` is one edit from both; naming either would be a guess.
        assert_eq!(suggest("cod", ["CODE", "CODA"]), None);
        // …and a clear winner is still named even with a runner-up behind it.
        assert_eq!(suggest("cod", ["CODE", "CODICE_X"]), Some("CODE".to_string()));
    }
}
