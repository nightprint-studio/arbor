//! One statement, no wrapper.
//!
//! Where the dialects visibly diverge: an upsert is `MERGE … FROM DUAL` on Oracle
//! and `INSERT … ON CONFLICT` on PostgreSQL. Same intent, described once in the
//! model, spelled differently here — which is the whole reason the model carries no
//! dialect.

use picus_ast::prelude::{
    Arity, Column, DialectScope, DmlModel, DmlOperation, DmlRow, EngineKind, Predicate,
    Target,
};

use crate::literal::{ident, literal};

/// Why one statement cannot be written for a scope.
///
/// A `Result` rather than a best effort, and that is the structural half of the
/// promise that nothing engine-specific reaches a portable folder: an upsert has
/// no portable spelling, `scope` carries no `EngineKind` to fall back on, and so
/// the caller is made to handle the refusal by the type system rather than by
/// remembering to ask first.
pub type EmitResult = Result<String, &'static str>;

/// Emit one row as a single statement valid in `scope`.
///
/// Takes the operation as an argument rather than reading `model.operation`,
/// because what a destination emits is not always what the model says: an upsert
/// into a seeding script is a plain insert. `Target::operation_for` owns that
/// rule; this signature is what makes it impossible to bypass.
pub fn statement_for(
    model: &DmlModel,
    row: &DmlRow,
    scope: DialectScope,
    operation: DmlOperation,
) -> EmitResult {
    let lc = model.lowercase_postgres;
    let table = ident(&model.table, scope, lc);
    let cols = model.supplied_columns(row);
    let non_key = model.non_key_columns(row);
    let keys = &model.key_columns;

    let id = |name: &str| ident(name, scope, lc);
    let val = |name: &str| {
        let column = model
            .columns
            .iter()
            .find(|c| c.name == name)
            .or_else(|| keys.iter().find(|c| c.name == name));
        match column {
            Some(c) => literal(row.get(name).map(String::as_str), c, scope),
            None => "NULL".to_string(),
        }
    };

    let col_list = |list: &[&picus_types::prelude::Column]| {
        list.iter().map(|c| id(&c.name)).collect::<Vec<_>>().join(", ")
    };
    let val_list =
        |list: &[&picus_types::prelude::Column]| list.iter().map(|c| val(&c.name)).collect::<Vec<_>>().join(", ");
    let key_predicate = |sep: &str| {
        keys.iter().map(|c| format!("{} = {}", id(&c.name), val(&c.name))).collect::<Vec<_>>().join(sep)
    };

    // The WHERE of an update or a delete: the model's own predicate where it has
    // one, the comparison key otherwise.
    //
    // Never both. They answer different questions — the key says *which row*, a
    // predicate says *which rows* — and AND-ing them together would silently
    // narrow a filter somebody wrote deliberately.
    let filter = || -> Result<String, &'static str> {
        match model.where_clause.as_ref().filter(|p| !p.is_empty()) {
            Some(predicate) => predicate_sql(predicate, model, scope),
            None if keys.is_empty() => Err(NO_FILTER),
            None => Ok(key_predicate(" AND ")),
        }
    };

    Ok(match operation {
        DmlOperation::Insert => format!(
            "INSERT INTO {table} ({})\nVALUES ({});",
            col_list(&cols),
            val_list(&cols)
        ),

        DmlOperation::Update => format!(
            "UPDATE {table} SET {}\n WHERE {};",
            non_key
                .iter()
                .map(|c| format!("{} = {}", id(&c.name), val(&c.name)))
                .collect::<Vec<_>>()
                .join(", "),
            filter()?
        ),

        DmlOperation::Delete => {
            format!("DELETE FROM {table}\n WHERE {};", filter()?)
        }

        // Two statements, one intention. The `DELETE` matches on the **comparison
        // key alone** and not on the whole row: the point is to make the row be
        // this, so a row somebody has since edited by hand must still be replaced
        // — matching on every column would leave it in place and then insert a
        // second copy, which is the one outcome nobody wants.
        DmlOperation::Replace => format!(
            "DELETE FROM {table}\n WHERE {};\nINSERT INTO {table} ({})\nVALUES ({});",
            key_predicate(" AND "),
            col_list(&cols),
            val_list(&cols)
        ),

        DmlOperation::Upsert => match scope.dialect() {
            // No portable arm, because there is no portable upsert: the two
            // engines spell it with constructs the other cannot parse. The
            // refusal names both spellings so the user can choose one.
            None => return Err(PORTABLE_UPSERT),
            Some(EngineKind::Postgres) => format!(
                "INSERT INTO {table} ({})\nVALUES ({})\nON CONFLICT ({}) DO UPDATE\n   SET {};",
                col_list(&cols),
                val_list(&cols),
                keys.iter().map(|c| id(&c.name)).collect::<Vec<_>>().join(", "),
                non_key
                    .iter()
                    .map(|c| format!("{} = EXCLUDED.{}", id(&c.name), id(&c.name)))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Some(EngineKind::Oracle) => format!(
                "MERGE INTO {table} d\nUSING (SELECT {} FROM DUAL) s\n   ON ({})\nWHEN MATCHED THEN UPDATE SET {}\nWHEN NOT MATCHED THEN INSERT ({}) VALUES ({});",
                keys.iter()
                    .map(|c| format!("{} AS {}", val(&c.name), id(&c.name)))
                    .collect::<Vec<_>>()
                    .join(", "),
                keys.iter()
                    .map(|c| format!("d.{} = s.{}", id(&c.name), id(&c.name)))
                    .collect::<Vec<_>>()
                    .join(" AND "),
                non_key
                    .iter()
                    .map(|c| format!("d.{} = {}", id(&c.name), val(&c.name)))
                    .collect::<Vec<_>>()
                    .join(", "),
                col_list(&cols),
                val_list(&cols)
            ),
        },
    })
}

/// Several rows as `INSERT`s — one statement on PostgreSQL, one per row on Oracle.
///
/// The asymmetry is not a style choice: **Oracle has no multi-row `VALUES`**. It
/// is the reason a row count is not enough information to write an insert, and the
/// reason this lives here rather than being assembled by whoever needed it — the
/// caller should not have to know which engines can bundle.
///
/// Every row must supply the same columns; the first one decides the column list.
/// Callers that build rows from one source (an abbreviation's template, a grid's
/// selection) satisfy that by construction, and a row that quietly omitted a
/// column would otherwise shift its neighbours' values one place along.
pub fn insert_rows(model: &DmlModel, rows: &[DmlRow], scope: DialectScope) -> EmitResult {
    let Some(first) = rows.first() else { return Ok(String::new()) };
    if scope.dialect() != Some(EngineKind::Postgres) || rows.len() == 1 {
        let each: Result<Vec<String>, &'static str> = rows
            .iter()
            .map(|row| statement_for(model, row, scope, DmlOperation::Insert))
            .collect();
        return Ok(each?.join("\n"));
    }

    let lc = model.lowercase_postgres;
    let columns = model.supplied_columns(first);
    let names = columns.iter().map(|c| ident(&c.name, scope, lc)).collect::<Vec<_>>().join(", ");
    let tuples = rows
        .iter()
        .map(|row| {
            let values = columns
                .iter()
                .map(|c| literal(row.get(&c.name).map(String::as_str), c, scope))
                .collect::<Vec<_>>()
                .join(", ");
            format!("       ({values})")
        })
        .collect::<Vec<_>>()
        .join(",\n");
    Ok(format!(
        "INSERT INTO {} ({names})\nVALUES {};",
        ident(&model.table, scope, lc),
        tuples.trim_start()
    ))
}

/// One `UPDATE`, with the columns to set and the columns to match given
/// **separately**.
///
/// [`statement_for`] reads both out of one row and takes the key columns from the
/// model, which makes one thing inexpressible: setting a column that is also part
/// of the key. That is exactly what editing a primary key in a grid is, and it is
/// legitimate — the `WHERE` has to carry the value the row had *before* the edit
/// while the `SET` carries the new one, and a single map cannot hold both.
///
/// So the two arrive apart. `keys` is matched on, `set` is written, and a column may
/// appear in both.
///
/// Refuses an empty `keys`, for the reason [`NO_FILTER`] exists: an `UPDATE` with
/// nothing to match on rewrites every row of the table.
pub fn update_row(
    model: &DmlModel,
    set: &DmlRow,
    keys: &DmlRow,
    scope: DialectScope,
) -> EmitResult {
    if keys.is_empty() {
        return Err(NO_FILTER);
    }
    if set.is_empty() {
        return Err(NOTHING_TO_SET);
    }
    let lc = model.lowercase_postgres;
    let described = |name: &str| model.columns.iter().find(|c| c.name == name);

    // A column the model does not describe cannot be quoted correctly, and quoting
    // it as text would be a guess written into somebody's database.
    let clause = |name: &String, value: &Option<&String>| -> Result<String, &'static str> {
        let column = described(name).ok_or(UNKNOWN_COLUMN)?;
        Ok(format!(
            "{} = {}",
            ident(name, scope, lc),
            literal(value.map(|v| v.as_str()), column, scope)
        ))
    };

    let assignments: Vec<String> = set
        .iter()
        .map(|(name, value)| clause(name, &Some(value)))
        .collect::<Result<_, _>>()?;
    let filter: Vec<String> = keys
        .iter()
        .map(|(name, value)| clause(name, &Some(value)))
        .collect::<Result<_, _>>()?;

    Ok(format!(
        "UPDATE {} SET {}\n WHERE {};",
        ident(&model.table, scope, lc),
        assignments.join(", "),
        filter.join(" AND ")
    ))
}

/// Emit one row the way `target` would — the ordinary entry point.
///
/// Goes through `Target::operation_for`, so a caller cannot accidentally emit a
/// `MERGE` into an initialisation that meant a plain insert.
pub fn plain_statement(model: &DmlModel, row: &DmlRow, target: &Target) -> EmitResult {
    statement_for(model, row, target.dialect, target.operation_for(model.operation))
}

/// Kept byte-identical to `Target::refuses`' wording: the user may meet this
/// refusal from the preview or from the write, and two spellings of one rule
/// read as two rules. Public for the same reason — the abbreviation expander
/// refuses `m#` into a portable folder and must say it in these exact words.
pub const PORTABLE_UPSERT: &str = "an upsert has no portable spelling: Oracle writes \
    `MERGE … USING DUAL` and PostgreSQL `INSERT … ON CONFLICT`. Write it into the dialect \
    folders, or use a plain INSERT here";

/// The refusal that matters most in this file.
///
/// An update or a delete with neither a comparison key nor a `WHERE` is
/// `DELETE FROM T` — every row in the table. It is one keystroke away from being
/// generated by accident and it is not recoverable, so the emitter refuses it
/// rather than trusting every caller to have checked. A user who genuinely means
/// to empty a table can write that statement themselves; the tool that writes into
/// four hundred scripts at once does not get to.
const NO_FILTER: &str = "an update or a delete needs something to match on — pick a comparison \
    key, or write a WHERE. Without one it would touch every row in the table, which Picus \
    will not generate";

/// One predicate, as SQL.
///
/// Operators are spelled identically in both dialects, so there is no per-engine
/// table here; what the scope decides is the *values*, through `literal` — which
/// is also what makes `=SYSDATE` in a condition translate the same way it does in
/// a value.
pub fn predicate_sql(
    predicate: &Predicate,
    model: &DmlModel,
    scope: DialectScope,
) -> Result<String, &'static str> {
    let lc = model.lowercase_postgres;

    match predicate {
        Predicate::Condition { column, operator, operands } => {
            let name = column.trim();
            if name.is_empty() {
                return Err(INCOMPLETE_CONDITION);
            }
            // The column's declared type decides how a value is written — a
            // numeric column takes a bare number — so a condition on a column the
            // model does not know is quoted, which is the answer that fails safely.
            let unknown;
            let described = match model.columns.iter().find(|c| c.name == name) {
                Some(found) => found,
                None => {
                    unknown = Column {
                        name: name.to_string(),
                        data_type: "text".to_string(),
                        primary_key: false,
                        not_null: false,
                        default_value: None,
                    };
                    &unknown
                }
            };
            let value = |raw: &str| literal(Some(raw), described, scope);
            let supplied: Vec<&String> =
                operands.iter().filter(|o| !o.trim().is_empty()).collect();
            let id = ident(name, scope, lc);

            Ok(match (operator.operands(), supplied.as_slice()) {
                (Arity::None, _) => format!("{id} {}", operator.keyword()),
                (Arity::One, [one]) => format!("{id} {} {}", operator.keyword(), value(one)),
                (Arity::Two, [low, high]) => {
                    format!("{id} BETWEEN {} AND {}", value(low), value(high))
                }
                (Arity::Many, values) if !values.is_empty() => format!(
                    "{id} {} ({})",
                    operator.keyword(),
                    values.iter().map(|v| value(v)).collect::<Vec<_>>().join(", ")
                ),
                _ => return Err(INCOMPLETE_CONDITION),
            })
        }

        Predicate::Group { join, of } => {
            let parts: Vec<String> = of
                .iter()
                .filter(|child| !child.is_empty())
                .map(|child| predicate_sql(child, model, scope))
                .collect::<Result<_, _>>()?;
            match parts.len() {
                0 => Err(INCOMPLETE_CONDITION),
                1 => Ok(parts.into_iter().next().unwrap_or_default()),
                // Always parenthesised, even where precedence would not require
                // it. `A AND (B OR C)` and `A AND B OR C` are different statements
                // and the tree said which one; relying on the reader to know SQL's
                // precedence table is how the wrong rows get deleted.
                _ => Ok(format!("({})", parts.join(&format!(" {} ", join.keyword())))),
            }
        }
    }
}

/// An `UPDATE` whose `SET` list is empty. Not harmful, but not a statement either
/// — and a caller asking for it has lost track of what it collected.
const NOTHING_TO_SET: &str = "this update has nothing to set";

/// A column the model does not describe. Refused rather than quoted as text: the
/// declared type is what decides quoting, and guessing it writes a wrong value
/// into a real table.
const UNKNOWN_COLUMN: &str =
    "one of the columns is not part of this table as the connection reported it — reload the \
     schema and try again";

const INCOMPLETE_CONDITION: &str =
    "the WHERE has a condition that is not finished — every condition needs a column, and as \
     many values as its operator takes";
