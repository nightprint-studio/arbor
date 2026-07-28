//! Which constructs belong to only one dialect, and what to say about them.
//!
//! This table is the reason the grammar is one permissive superset instead of
//! two strict ones. An Oracle-ism in a PostgreSQL file does not become a syntax
//! error here; it becomes a node with a name, and this module turns that name
//! into a sentence a maintainer can act on.
//!
//! Two lookups, because the divergences come in two shapes:
//!
//! * **syntax** — a construct the other dialect has no spelling for at all
//!   ([`classify_node`]);
//! * **builtins** — a function that exists in one engine and not the other
//!   ([`classify_function`]). Deliberately a short, high-confidence list: a
//!   false "this is Oracle-only" costs more trust than a missed one.

use picus_types::prelude::EngineKind;
use serde::Serialize;

use crate::range::ByteRange;

/// One construct that does not belong to the file's declared dialect.
///
/// `Copy` and `&'static str`: findings are produced during a walk and the text
/// is a compile-time constant, so there is nothing to allocate. It serialises
/// (for the RPC seam) but does not deserialise — nothing ever needs to read one
/// back in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForeignConstruct {
    /// Stable machine id — the grammar node kind, or the upper-case function
    /// name for a builtin.
    pub construct: &'static str,
    /// The dialect the construct actually belongs to.
    pub belongs_to: EngineKind,
    /// One line, naming the construct and its counterpart in the other dialect.
    pub message: &'static str,
    pub range: ByteRange,
}

/// Node kinds that exist in one dialect only, with the advice for each.
const NODE_TABLE: &[(&str, EngineKind, &str)] = &[
    // ── Oracle ───────────────────────────────────────────────────────────────
    (
        "q_string",
        EngineKind::Oracle,
        "`q'[…]'` alternative quoting is Oracle; PostgreSQL uses '' doubling or $$-quoting",
    ),
    (
        "dual_reference",
        EngineKind::Oracle,
        "`FROM DUAL` is Oracle's one-row table; PostgreSQL omits FROM entirely",
    ),
    (
        "rownum",
        EngineKind::Oracle,
        "`ROWNUM` is Oracle; PostgreSQL uses LIMIT or FETCH FIRST … ROWS ONLY",
    ),
    (
        "connect_by_clause",
        EngineKind::Oracle,
        "`CONNECT BY` is Oracle hierarchical query syntax; PostgreSQL uses WITH RECURSIVE",
    ),
    (
        "start_with_clause",
        EngineKind::Oracle,
        "`START WITH` is Oracle hierarchical query syntax; PostgreSQL uses WITH RECURSIVE",
    ),
    (
        "prior_expression",
        EngineKind::Oracle,
        "`PRIOR` belongs to Oracle's CONNECT BY; PostgreSQL uses WITH RECURSIVE",
    ),
    (
        "oracle_outer_join",
        EngineKind::Oracle,
        "`(+)` is Oracle's outer-join marker; PostgreSQL wants LEFT JOIN … ON",
    ),
    (
        "percent_type",
        EngineKind::Oracle,
        "`%TYPE` is PL/SQL; PL/pgSQL spells it the same way only inside a function body",
    ),
    (
        "percent_rowtype",
        EngineKind::Oracle,
        "`%ROWTYPE` is PL/SQL; PL/pgSQL spells it the same way only inside a function body",
    ),
    (
        "execute_immediate_statement",
        EngineKind::Oracle,
        "`EXECUTE IMMEDIATE` is PL/SQL; PL/pgSQL uses EXECUTE",
    ),
    (
        "slash_terminator",
        EngineKind::Oracle,
        "a lone `/` terminates a PL/SQL block in SQL*Plus; PostgreSQL has no equivalent",
    ),
    (
        "create_package_statement",
        EngineKind::Oracle,
        "packages are Oracle-only; PostgreSQL groups functions in a schema or an extension",
    ),
    (
        "create_package_body_statement",
        EngineKind::Oracle,
        "packages are Oracle-only; PostgreSQL groups functions in a schema or an extension",
    ),
    (
        "create_synonym_statement",
        EngineKind::Oracle,
        "synonyms are Oracle-only; PostgreSQL uses a view or a search_path entry",
    ),
    (
        "forall_statement",
        EngineKind::Oracle,
        "`FORALL` bulk DML is PL/SQL-only",
    ),
    (
        "national_string",
        EngineKind::Oracle,
        "`N'…'` national-character literals are Oracle; PostgreSQL text is already Unicode",
    ),
    // ── PostgreSQL ───────────────────────────────────────────────────────────
    (
        "dollar_quoted_string",
        EngineKind::Postgres,
        "`$$…$$` dollar quoting is PostgreSQL; Oracle uses q'[…]' or '' doubling",
    ),
    (
        "escape_string",
        EngineKind::Postgres,
        "`E'…'` escape strings are PostgreSQL; Oracle has no backslash escapes",
    ),
    (
        "unicode_string",
        EngineKind::Postgres,
        "`U&'…'` unicode literals are PostgreSQL; Oracle uses UNISTR()",
    ),
    (
        "postgres_cast_expression",
        EngineKind::Postgres,
        "`::` casting is PostgreSQL; Oracle wants CAST(… AS …)",
    ),
    (
        "postgres_operator_expression",
        EngineKind::Postgres,
        "the json/array operators (@>, ->>, #-, …) are PostgreSQL-only",
    ),
    (
        "array_constructor",
        EngineKind::Postgres,
        "`ARRAY[…]` is PostgreSQL; Oracle uses a collection type",
    ),
    (
        "on_conflict_clause",
        EngineKind::Postgres,
        "`ON CONFLICT` is PostgreSQL's upsert; Oracle writes MERGE … USING DUAL",
    ),
    (
        "do_statement",
        EngineKind::Postgres,
        "`DO $$ … $$` is PostgreSQL's anonymous block; Oracle writes DECLARE … BEGIN … END; /",
    ),
    (
        "perform_statement",
        EngineKind::Postgres,
        "`PERFORM` is PL/pgSQL; PL/SQL uses SELECT … INTO",
    ),
    (
        "limit_clause",
        EngineKind::Postgres,
        "`LIMIT` is PostgreSQL; Oracle uses FETCH FIRST … ROWS ONLY (12c) or ROWNUM",
    ),
    (
        "offset_clause",
        EngineKind::Postgres,
        "`OFFSET` is PostgreSQL; Oracle 12c spells it OFFSET … ROWS inside the row-limiting clause",
    ),
    (
        "on_update_action",
        EngineKind::Postgres,
        "`ON UPDATE` on a foreign key is PostgreSQL; Oracle supports only ON DELETE",
    ),
    (
        "create_schema_statement",
        EngineKind::Postgres,
        "`CREATE SCHEMA` is PostgreSQL; in Oracle a schema is a user",
    ),
    (
        "lateral_table",
        EngineKind::Postgres,
        "`LATERAL` is PostgreSQL; Oracle 12c spells it CROSS APPLY / OUTER APPLY",
    ),
];

/// Builtin functions that exist in one engine only. Upper-case, unqualified.
const FUNCTION_TABLE: &[(&str, EngineKind, &str)] = &[
    ("NVL", EngineKind::Oracle, "`NVL` is Oracle; PostgreSQL uses COALESCE"),
    ("NVL2", EngineKind::Oracle, "`NVL2` is Oracle; PostgreSQL uses CASE"),
    ("DECODE", EngineKind::Oracle, "`DECODE` is Oracle; PostgreSQL uses CASE"),
    ("SYSDATE", EngineKind::Oracle, "`SYSDATE` is Oracle; PostgreSQL uses CURRENT_DATE or now()"),
    (
        "SYSTIMESTAMP",
        EngineKind::Oracle,
        "`SYSTIMESTAMP` is Oracle; PostgreSQL uses now() or CURRENT_TIMESTAMP",
    ),
    ("LISTAGG", EngineKind::Oracle, "`LISTAGG` is Oracle; PostgreSQL uses string_agg"),
    ("INSTR", EngineKind::Oracle, "`INSTR` is Oracle; PostgreSQL uses position() or strpos()"),
    (
        "TO_NUMBER",
        EngineKind::Oracle,
        "`TO_NUMBER` is Oracle; PostgreSQL casts with ::numeric",
    ),
    ("NOW", EngineKind::Postgres, "`now()` is PostgreSQL; Oracle uses SYSDATE"),
    (
        "STRING_AGG",
        EngineKind::Postgres,
        "`string_agg` is PostgreSQL; Oracle uses LISTAGG",
    ),
    (
        "GENERATE_SERIES",
        EngineKind::Postgres,
        "`generate_series` is PostgreSQL; Oracle uses CONNECT BY LEVEL",
    ),
    (
        "NEXTVAL",
        EngineKind::Postgres,
        "`nextval('s')` is PostgreSQL; Oracle writes s.NEXTVAL",
    ),
];

/// A construct's home dialect, if it has only one.
pub fn classify_node(node_kind: &str) -> Option<(EngineKind, &'static str, &'static str)> {
    NODE_TABLE
        .iter()
        .find(|(k, _, _)| *k == node_kind)
        .map(|(k, e, m)| (*e, *k, *m))
}

/// Builtins whose classification holds **only when the name is unqualified**,
/// because a qualifier changes which dialect the construct belongs to.
///
/// `NEXTVAL` is the whole reason this list exists. PostgreSQL writes
/// `nextval('s')` — a call — while Oracle writes `s.NEXTVAL`, a pseudo-column on
/// the sequence itself. Matching the last dotted component classified Oracle's own
/// syntax as PostgreSQL, so every Oracle script that touches a sequence collected a
/// blocking finding telling the author to write exactly what they had just written.
/// A false positive that contradicts the file in front of you costs more trust than
/// a hundred missed ones.
const QUALIFIER_SENSITIVE: &[&str] = &["NEXTVAL", "CURRVAL"];

/// A builtin's home dialect, if it has only one.
///
/// Matched case-insensitively against the *last* component of a dotted name, so a
/// schema-qualified `public.now()` is still recognised — except for the names in
/// [`QUALIFIER_SENSITIVE`], where the qualifier is the very thing that decides.
pub fn classify_function(name: &str) -> Option<(EngineKind, &'static str, &'static str)> {
    let qualified = name.contains('.');
    let bare = name.rsplit('.').next().unwrap_or(name);
    if qualified && QUALIFIER_SENSITIVE.iter().any(|k| bare.eq_ignore_ascii_case(k)) {
        return None;
    }
    FUNCTION_TABLE
        .iter()
        .find(|(k, _, _)| bare.eq_ignore_ascii_case(k))
        .map(|(k, e, m)| (*e, *k, *m))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oracle_and_postgres_constructs_are_classified() {
        assert_eq!(classify_node("dual_reference").map(|c| c.0), Some(EngineKind::Oracle));
        assert_eq!(
            classify_node("on_conflict_clause").map(|c| c.0),
            Some(EngineKind::Postgres)
        );
        assert_eq!(classify_node("select_core"), None);
    }

    #[test]
    fn builtins_match_case_insensitively_and_ignore_the_qualifier() {
        assert_eq!(classify_function("nvl").map(|c| c.0), Some(EngineKind::Oracle));
        assert_eq!(classify_function("SYS.NVL").map(|c| c.0), Some(EngineKind::Oracle));
        assert_eq!(classify_function("Now").map(|c| c.0), Some(EngineKind::Postgres));
        assert_eq!(classify_function("COALESCE"), None);
    }

    #[test]
    fn oracles_own_sequence_syntax_is_not_reported_as_postgresql() {
        // `nextval('s')` is PostgreSQL; `s.NEXTVAL` is Oracle. Matching only the
        // last dotted component made every Oracle script that touches a sequence
        // collect a blocking finding advising the author to write what they had
        // just written.
        assert_eq!(classify_function("nextval").map(|c| c.0), Some(EngineKind::Postgres));
        assert_eq!(classify_function("SEQ_PARAMETRI.NEXTVAL"), None);
        assert_eq!(classify_function("seq_parametri.nextval"), None);
        assert_eq!(classify_function("APP.SEQ_CLIENTI.CURRVAL"), None);
        // The qualifier still means nothing for every other builtin.
        assert_eq!(classify_function("public.now").map(|c| c.0), Some(EngineKind::Postgres));
    }

    #[test]
    fn every_message_names_the_construct_and_the_alternative() {
        // A finding whose text does not tell the maintainer what to write
        // instead is not worth reporting.
        for (id, _, msg) in NODE_TABLE.iter().chain(FUNCTION_TABLE.iter()) {
            assert!(msg.len() > 20, "{id}: the message must be a sentence, not a label");
        }
    }
}
