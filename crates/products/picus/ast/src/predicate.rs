//! [`Predicate`] — the `WHERE` of an update or a delete, as a tree.
//!
//! ## Why a tree and not a string
//!
//! A free-text `WHERE` would be one field and no work. It would also be the point
//! at which Picus stops knowing what a script does: nothing to validate, nothing
//! to compare between the two dialects, nothing to reconcile against what is
//! already installed. Every rule this product has rests on the model being
//! *structured*, and a hole in it is not a hole in one feature — it is a hole in
//! the guarantee.
//!
//! So the shape is closed: a comparison is a column, an operator and the operands
//! that operator takes, and a group is `AND` or `OR` over more of them. It is not
//! all of SQL, and it is not meant to be. What it does not cover is the extreme
//! minority in an installation script, and where a condition's *value* needs SQL,
//! the operand carries it — `=SYSDATE`, `=(SELECT …)` — through the same `=`
//! prefix every DML value uses.
//!
//! ## Emission is not here
//!
//! Nothing in this module mentions a dialect, for the same reason nothing else in
//! `picus-ast` does. `picus-emit` turns one of these into Oracle and into
//! PostgreSQL, and the two are the same tree read twice.

use serde::{Deserialize, Serialize};

/// How the conditions of a group are joined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Join {
    And,
    Or,
}

impl Join {
    pub fn keyword(self) -> &'static str {
        match self {
            Join::And => "AND",
            Join::Or => "OR",
        }
    }
}

/// What a condition tests.
///
/// A closed list, and the arity of each is fixed — see [`Operator::operands`].
/// Adding one means adding it here, in the emitter and in the picker, which is the
/// point: an operator nobody can emit must not be expressible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Operator {
    Equals,
    NotEquals,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
    Like,
    NotLike,
    In,
    NotIn,
    IsNull,
    IsNotNull,
    Between,
}

/// How many operands an operator takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arity {
    /// `IS NULL`, `IS NOT NULL`.
    None,
    /// Everything that compares against one thing.
    One,
    /// `BETWEEN a AND b`.
    Two,
    /// `IN (…)` — at least one.
    Many,
}

impl Operator {
    pub fn operands(self) -> Arity {
        match self {
            Operator::IsNull | Operator::IsNotNull => Arity::None,
            Operator::Between => Arity::Two,
            Operator::In | Operator::NotIn => Arity::Many,
            _ => Arity::One,
        }
    }

    /// The SQL, identical in both dialects — which is why the emitter needs no
    /// per-engine table for this and a portable script can carry any of them.
    pub fn keyword(self) -> &'static str {
        match self {
            Operator::Equals => "=",
            Operator::NotEquals => "<>",
            Operator::Less => "<",
            Operator::LessOrEqual => "<=",
            Operator::Greater => ">",
            Operator::GreaterOrEqual => ">=",
            Operator::Like => "LIKE",
            Operator::NotLike => "NOT LIKE",
            Operator::In => "IN",
            Operator::NotIn => "NOT IN",
            Operator::IsNull => "IS NULL",
            Operator::IsNotNull => "IS NOT NULL",
            Operator::Between => "BETWEEN",
        }
    }
}

/// One condition, or a group of them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Predicate {
    #[serde(rename_all = "camelCase")]
    Condition {
        column: String,
        operator: Operator,
        /// As many as the operator takes, each written in the same notation as a
        /// DML value: quoted unless it starts with `=`.
        #[serde(default)]
        operands: Vec<String>,
    },
    #[serde(rename_all = "camelCase")]
    Group {
        join: Join,
        #[serde(default)]
        of: Vec<Predicate>,
    },
}

impl Predicate {
    /// An empty `AND` group — what a new `WHERE` starts as.
    pub fn empty() -> Predicate {
        Predicate::Group { join: Join::And, of: Vec::new() }
    }

    /// Does this describe anything at all?
    ///
    /// A group of nothing is nothing, however deeply it is nested — which matters
    /// because an *empty* `WHERE` on a `DELETE` means "every row in the table",
    /// and that is the one statement this product must never emit by accident.
    pub fn is_empty(&self) -> bool {
        match self {
            Predicate::Condition { column, .. } => column.trim().is_empty(),
            Predicate::Group { of, .. } => of.iter().all(Predicate::is_empty),
        }
    }

    /// Every column this predicate names, in order, without duplicates.
    pub fn columns(&self) -> Vec<&str> {
        let mut out = Vec::new();
        self.collect_columns(&mut out);
        out
    }

    fn collect_columns<'a>(&'a self, out: &mut Vec<&'a str>) {
        match self {
            Predicate::Condition { column, .. } => {
                let name = column.trim();
                if !name.is_empty() && !out.contains(&name) {
                    out.push(name);
                }
            }
            Predicate::Group { of, .. } => {
                for child in of {
                    child.collect_columns(out);
                }
            }
        }
    }

    /// Why this predicate cannot be emitted — `None` when it can.
    ///
    /// Reported rather than repaired: a condition missing its second operand could
    /// be completed a dozen ways, and picking one would be inventing a filter for
    /// a statement that deletes rows.
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        self.collect_problems(&mut out);
        out
    }

    fn collect_problems(&self, out: &mut Vec<String>) {
        match self {
            Predicate::Condition { column, operator, operands } => {
                let name = column.trim();
                if name.is_empty() {
                    out.push("a condition with no column".to_string());
                    return;
                }
                let supplied: Vec<&String> =
                    operands.iter().filter(|o| !o.trim().is_empty()).collect();
                match operator.operands() {
                    Arity::None => {}
                    Arity::One if supplied.len() != 1 => {
                        out.push(format!("{name} {} needs a value", operator.keyword()));
                    }
                    Arity::Two if supplied.len() != 2 => {
                        out.push(format!("{name} BETWEEN needs both bounds"));
                    }
                    Arity::Many if supplied.is_empty() => {
                        out.push(format!("{name} {} needs at least one value", operator.keyword()));
                    }
                    _ => {}
                }
            }
            Predicate::Group { of, .. } => {
                for child in of {
                    child.collect_problems(out);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn condition(column: &str, operator: Operator, operands: &[&str]) -> Predicate {
        Predicate::Condition {
            column: column.to_string(),
            operator,
            operands: operands.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn a_group_of_empty_groups_is_still_empty() {
        // Load-bearing: an empty WHERE on a DELETE means every row in the table,
        // and "empty" has to mean it however deeply it is buried.
        let nested = Predicate::Group {
            join: Join::Or,
            of: vec![Predicate::empty(), Predicate::Group { join: Join::And, of: vec![] }],
        };
        assert!(nested.is_empty());
        assert!(Predicate::empty().is_empty());

        let real = Predicate::Group {
            join: Join::Or,
            of: vec![Predicate::empty(), condition("CHIAVE", Operator::Equals, &["A"])],
        };
        assert!(!real.is_empty());
    }

    #[test]
    fn each_operator_says_how_many_operands_it_wants() {
        let cases = [
            (Operator::IsNull, Arity::None),
            (Operator::Equals, Arity::One),
            (Operator::Like, Arity::One),
            (Operator::Between, Arity::Two),
            (Operator::In, Arity::Many),
            (Operator::NotIn, Arity::Many),
        ];
        for (operator, expected) in cases {
            assert_eq!(operator.operands(), expected, "{}", operator.keyword());
        }
    }

    #[test]
    fn a_condition_missing_an_operand_is_reported_and_never_repaired() {
        let missing = condition("ETICHETTA", Operator::Equals, &[]);
        assert_eq!(missing.problems(), vec!["ETICHETTA = needs a value"]);

        let half = condition("PESO", Operator::Between, &["1"]);
        assert_eq!(half.problems(), vec!["PESO BETWEEN needs both bounds"]);

        let empty_list = condition("CHIAVE", Operator::In, &["", "  "]);
        assert_eq!(empty_list.problems(), vec!["CHIAVE IN needs at least one value"]);

        // …and the ones that are complete say nothing.
        assert!(condition("CHIAVE", Operator::IsNull, &[]).problems().is_empty());
        assert!(condition("PESO", Operator::Between, &["1", "9"]).problems().is_empty());
        assert!(condition("CHIAVE", Operator::In, &["A", "B"]).problems().is_empty());
    }

    #[test]
    fn the_columns_come_back_once_each_in_the_order_they_appear() {
        let tree = Predicate::Group {
            join: Join::And,
            of: vec![
                condition("CHIAVE", Operator::Equals, &["A"]),
                Predicate::Group {
                    join: Join::Or,
                    of: vec![
                        condition("LINGUA", Operator::Equals, &["it"]),
                        condition("CHIAVE", Operator::NotEquals, &["B"]),
                    ],
                },
            ],
        };
        assert_eq!(tree.columns(), vec!["CHIAVE", "LINGUA"]);
    }

    #[test]
    fn it_round_trips_through_json() {
        // It crosses the IPC seam, so its shape is a contract.
        let tree = Predicate::Group {
            join: Join::Or,
            of: vec![
                condition("CHIAVE", Operator::In, &["A", "B"]),
                condition("DATA", Operator::Less, &["=SYSDATE"]),
                condition("NOTE", Operator::IsNull, &[]),
            ],
        };
        let json = serde_json::to_string(&tree).expect("serialises");
        assert_eq!(serde_json::from_str::<Predicate>(&json).expect("parses"), tree);
    }
}
