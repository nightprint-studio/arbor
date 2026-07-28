//! `>` — where the join condition comes from.
//!
//! Nowhere else. A join is read out of a foreign key or it is refused; there is
//! no name-matching heuristic, no `id`-means-`id` rule, no "the columns are
//! called the same thing so they are probably related". That restraint is the
//! feature: a tool that guessed would be right most of the time, and the times it
//! was wrong it would produce a query that runs, returns rows, and means
//! something else.
//!
//! Three answers, and all three are useful:
//!
//! | keys between the two tables | answer |
//! |---|---|
//! | exactly one, **either direction** | use it — a join is symmetric |
//! | more than one | refuse, naming the candidates so the user can pick |
//! | none | refuse, saying so |

use crate::error::AbbrevError;
use crate::resolve::matches;
use crate::schema::TableMeta;

/// One foreign key between two tables, flattened into the four things a join
/// needs to know.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JoinKey {
    /// The table that holds the referencing columns.
    pub child: String,
    pub child_columns: Vec<String>,
    pub parent: String,
    pub parent_columns: Vec<String>,
}

impl JoinKey {
    /// `ORDINI.ID_CLIENTE → CLIENTI.ID_CLIENTE` — what an ambiguity has to print
    /// for the user to be able to resolve it.
    pub fn describe(&self) -> String {
        format!("{} → {}", side(&self.child, &self.child_columns), side(&self.parent, &self.parent_columns))
    }

    /// Does this key involve a column of that name, on either side?
    ///
    /// Either side, because the user disambiguating with `>clienti:x` is naming
    /// the column that tells the keys apart, and which end it sits on is a detail
    /// of how the schema was drawn.
    pub fn mentions(&self, column: &str) -> bool {
        self.child_columns.iter().chain(&self.parent_columns).any(|c| matches(c, column))
    }

    /// The column pairs, oriented so the left of each pair is on `left`.
    pub fn oriented(&self, left: &str) -> Vec<(String, String)> {
        let (near, far) = if matches(&self.child, left) {
            (&self.child_columns, &self.parent_columns)
        } else {
            (&self.parent_columns, &self.child_columns)
        };
        near.iter().cloned().zip(far.iter().cloned()).collect()
    }

    /// The column to name in a `>table:column` that would pick this key.
    fn distinguishing_column(&self) -> &str {
        self.child_columns.first().map(String::as_str).unwrap_or_default()
    }
}

fn side(table: &str, columns: &[String]) -> String {
    match columns {
        [one] => format!("{table}.{one}"),
        many => format!("{table}.({})", many.join(", ")),
    }
}

/// Every foreign key relating the two tables, in both directions.
pub fn keys_between(left: &TableMeta, right: &TableMeta) -> Vec<JoinKey> {
    let mut keys: Vec<JoinKey> = from(left, &right.name);
    // A self-join would otherwise find every one of its keys twice.
    if !matches(&left.name, &right.name) {
        for key in from(right, &left.name) {
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
    }
    keys
}

fn from(child: &TableMeta, parent: &str) -> Vec<JoinKey> {
    child
        .foreign_keys
        .iter()
        .filter(|fk| fk.is_well_formed() && matches(&fk.referenced_table, parent))
        .map(|fk| JoinKey {
            child: child.name.clone(),
            child_columns: fk.columns.clone(),
            parent: fk.referenced_table.clone(),
            parent_columns: fk.referenced_columns.clone(),
        })
        .collect()
}

/// The one key to join on — or a refusal that says which of the three cases it is.
///
/// `pick` is the `:column` the user wrote, if any.
pub fn resolve(left: &TableMeta, right: &TableMeta, pick: Option<&str>) -> Result<JoinKey, AbbrevError> {
    let keys = keys_between(left, right);
    if keys.is_empty() {
        return Err(AbbrevError::NoForeignKey { from: left.name.clone(), to: right.name.clone() });
    }

    let named = |keys: &[JoinKey]| keys.iter().map(JoinKey::describe).collect::<Vec<_>>();

    let Some(pick) = pick else {
        return match keys.len() {
            1 => Ok(keys.into_iter().next().expect("just checked")),
            _ => Err(AbbrevError::AmbiguousJoin {
                from: left.name.clone(),
                to: right.name.clone(),
                hint: hint(&right.name, &keys),
                candidates: named(&keys),
                }),
        };
    };

    let mut matching: Vec<JoinKey> = keys.iter().filter(|k| k.mentions(pick)).cloned().collect();
    match matching.len() {
        1 => Ok(matching.remove(0)),
        0 => Err(AbbrevError::UnknownJoinColumn {
            from: left.name.clone(),
            to: right.name.clone(),
            column: pick.to_string(),
            candidates: named(&keys),
        }),
        _ => Err(AbbrevError::AmbiguousJoin {
            from: left.name.clone(),
            to: right.name.clone(),
            hint: hint(&right.name, &matching),
            candidates: named(&matching),
        }),
    }
}

/// A concrete `>table:column` the user can copy, built from the **last**
/// candidate: the first one is what they would have got by luck anyway, so
/// showing the other one is what actually teaches the syntax.
fn hint(right: &str, keys: &[JoinKey]) -> String {
    let column = keys.last().map(JoinKey::distinguishing_column).unwrap_or_default();
    format!(">{}:{}", right.to_lowercase(), column.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{ColumnMeta, ForeignKeyMeta, ValueKind};

    fn table(name: &str, keys: Vec<ForeignKeyMeta>) -> TableMeta {
        TableMeta::new(name, vec![ColumnMeta::new("ID", ValueKind::Number)]).with_foreign_keys(keys)
    }

    #[test]
    fn one_key_is_found_from_either_end() {
        let ordini = table("ORDINI", vec![ForeignKeyMeta::new("ID_CLIENTE", "CLIENTI", "ID")]);
        let clienti = table("CLIENTI", vec![]);
        // The chain can be written in either order; a join is symmetric.
        let a = resolve(&ordini, &clienti, None).expect("joined");
        let b = resolve(&clienti, &ordini, None).expect("joined");
        assert_eq!(a, b);
        assert_eq!(a.oriented("ORDINI"), vec![("ID_CLIENTE".to_string(), "ID".to_string())]);
        assert_eq!(a.oriented("CLIENTI"), vec![("ID".to_string(), "ID_CLIENTE".to_string())]);
    }

    #[test]
    fn two_keys_are_refused_with_the_columns_that_tell_them_apart() {
        let ordini = table(
            "ORDINI",
            vec![
                ForeignKeyMeta::new("ID_CLIENTE", "CLIENTI", "ID"),
                ForeignKeyMeta::new("ID_CLIENTE_FATTURAZIONE", "CLIENTI", "ID"),
            ],
        );
        let clienti = table("CLIENTI", vec![]);
        let error = resolve(&ordini, &clienti, None).expect_err("ambiguous");
        let message = error.to_string();
        assert!(message.contains("ORDINI.ID_CLIENTE →"), "{message}");
        assert!(message.contains("ORDINI.ID_CLIENTE_FATTURAZIONE →"), "{message}");
        assert!(message.contains(">clienti:id_cliente_fatturazione"), "{message}");

        // …and naming one of them resolves it.
        let picked = resolve(&ordini, &clienti, Some("id_cliente_fatturazione")).expect("picked");
        assert_eq!(picked.child_columns, vec!["ID_CLIENTE_FATTURAZIONE"]);
    }

    #[test]
    fn no_key_is_refused_by_name() {
        let a = table("ORDINI", vec![]);
        let b = table("PRODOTTI", vec![]);
        let message = resolve(&a, &b, None).expect_err("no key").to_string();
        assert!(message.contains("no foreign key between ORDINI and PRODOTTI"), "{message}");
    }

    #[test]
    fn a_self_join_finds_its_key_once() {
        let dip = table("DIPENDENTI", vec![ForeignKeyMeta::new("ID_CAPO", "DIPENDENTI", "ID")]);
        // Scanning both ends would find the same key twice and call it ambiguous.
        let key = resolve(&dip, &dip, None).expect("joined");
        assert_eq!(key.oriented("DIPENDENTI"), vec![("ID_CAPO".to_string(), "ID".to_string())]);
    }

    #[test]
    fn a_composite_key_pairs_up_positionally() {
        let child = table(
            "RIGHE",
            vec![ForeignKeyMeta {
                columns: vec!["ANNO".into(), "NUMERO".into()],
                referenced_table: "TESTATE".into(),
                referenced_columns: vec!["ANNO".into(), "NUM".into()],
            }],
        );
        let parent = table("TESTATE", vec![]);
        let key = resolve(&child, &parent, None).expect("joined");
        assert_eq!(
            key.oriented("RIGHE"),
            vec![("ANNO".to_string(), "ANNO".to_string()), ("NUMERO".to_string(), "NUM".to_string())]
        );
        assert!(key.describe().contains("RIGHE.(ANNO, NUMERO)"), "{}", key.describe());
    }

    #[test]
    fn a_ragged_key_is_ignored_rather_than_half_used() {
        let child = table(
            "RIGHE",
            vec![ForeignKeyMeta {
                columns: vec!["ANNO".into(), "NUMERO".into()],
                referenced_table: "TESTATE".into(),
                referenced_columns: vec!["ANNO".into()],
            }],
        );
        let parent = table("TESTATE", vec![]);
        assert!(matches!(resolve(&child, &parent, None), Err(AbbrevError::NoForeignKey { .. })));
    }

    #[test]
    fn naming_a_column_no_key_uses_says_which_keys_there_are() {
        let ordini = table("ORDINI", vec![ForeignKeyMeta::new("ID_CLIENTE", "CLIENTI", "ID")]);
        let clienti = table("CLIENTI", vec![]);
        let message = resolve(&ordini, &clienti, Some("id_fornitore")).expect_err("no such").to_string();
        assert!(message.contains("`id_fornitore` is not part of any foreign key"), "{message}");
        assert!(message.contains("ORDINI.ID_CLIENTE"), "{message}");
    }
}
