//! Objects the **engine** provides, which no repository installs.
//!
//! `SELECT … FROM user_tab_cols` is an Oracle script asking Oracle about itself.
//! The table is not part of the product, nobody wrote a `CREATE` for it, and no
//! PostgreSQL script will ever have a counterpart — PostgreSQL answers the same
//! question from `information_schema`. Indexing it puts a row in the inventory
//! that can only ever read as a gap, and one nobody can close.
//!
//! So these are left out of the index entirely: they take part in no rule, get no
//! row, and appear in no coverage column.
//!
//! ## Why the list is curated rather than a prefix rule
//!
//! The tempting version is "anything starting `USER_`", and it is wrong: a
//! product with a `USER_PROFILI` table would silently vanish from its own
//! inventory, which is the worst possible failure for a tool whose job is
//! noticing absences. The names below are the dictionary views scripts actually
//! query, spelled out, and the only prefixes used are ones an ordinary table
//! cannot collide with (`V$`, `GV$`, `PG_`, and a qualifying schema).
//!
//! Adding a name here is cheap and reversible. Missing one costs a single
//! unclosable finding; wrongly including one hides a real object, so the list errs
//! towards being short.

/// Schemas whose contents belong to the engine, whoever queries them.
const SYSTEM_SCHEMAS: [&str; 4] = ["SYS", "SYSTEM", "PG_CATALOG", "INFORMATION_SCHEMA"];

/// The tail of an Oracle dictionary view, after `USER_` / `ALL_` / `DBA_`.
const DICTIONARY_VIEWS: [&str; 21] = [
    "TABLES",
    "TAB_COLS",
    "TAB_COLUMNS",
    "TAB_COMMENTS",
    "TAB_PRIVS",
    "COL_COMMENTS",
    "CONSTRAINTS",
    "CONS_COLUMNS",
    "INDEXES",
    "IND_COLUMNS",
    "OBJECTS",
    "SEQUENCES",
    "TRIGGERS",
    "VIEWS",
    "SOURCE",
    "SYNONYMS",
    "PROCEDURES",
    "ARGUMENTS",
    "DEPENDENCIES",
    "PART_TABLES",
    "MVIEWS",
];

/// Oracle's one-row table, and the closest thing SQL has to a keyword that looks
/// like an object.
const ORACLE_PSEUDO_TABLES: [&str; 1] = ["DUAL"];

/// Is this an object the engine provides rather than one the repository installs?
///
/// `schema` is the qualifier as written, when the reference had one — it is the
/// strongest signal available, and the only one that catches
/// `information_schema.columns`, whose bare name is far too ordinary to match on.
pub fn is_engine_provided(schema: Option<&str>, folded_name: &str) -> bool {
    if let Some(schema) = schema {
        let schema = schema.trim_matches('"').to_uppercase();
        if SYSTEM_SCHEMAS.contains(&schema.as_str()) {
            return true;
        }
    }
    let name = folded_name;
    if ORACLE_PSEUDO_TABLES.contains(&name) {
        return true;
    }
    // `V$SESSION`, `GV$INSTANCE`: the `$` is what makes the prefix safe, because
    // no ordinary identifier carries one.
    if name.starts_with("V$") || name.starts_with("GV$") {
        return true;
    }
    // PostgreSQL reserves the `pg_` prefix for itself, in writing, in its own
    // documentation. A user table called `PG_…` is a bug in that repository.
    if name.starts_with("PG_") {
        return true;
    }
    for prefix in ["USER_", "ALL_", "DBA_"] {
        if let Some(tail) = name.strip_prefix(prefix) {
            if DICTIONARY_VIEWS.contains(&tail) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dictionary_views_scripts_actually_query_are_recognised() {
        assert!(is_engine_provided(None, "USER_TAB_COLS"));
        assert!(is_engine_provided(None, "ALL_CONSTRAINTS"));
        assert!(is_engine_provided(None, "DBA_INDEXES"));
        assert!(is_engine_provided(None, "DUAL"));
        assert!(is_engine_provided(None, "V$SESSION"));
        assert!(is_engine_provided(None, "PG_CLASS"));
    }

    #[test]
    fn a_qualifier_settles_the_names_that_are_too_ordinary_to_match_on() {
        // `information_schema.columns` folds to `COLUMNS`, which is a perfectly
        // plausible name for a real table. Only the qualifier can tell them apart.
        assert!(is_engine_provided(Some("information_schema"), "COLUMNS"));
        assert!(is_engine_provided(Some("SYS"), "OBJ$"));
        assert!(!is_engine_provided(None, "COLUMNS"));
    }

    #[test]
    fn a_products_own_table_is_never_mistaken_for_the_engines() {
        // The failure the curated list exists to prevent: a product with a
        // `USER_PROFILI` table vanishing from its own inventory, which is the
        // worst thing a tool for noticing absences can do.
        for name in [
            "USER_PROFILI",
            "USER_TAB",
            "ALL_ORDINI",
            "DBA_CLIENTI",
            "PGRAMMA",
            "DUALITA",
            "MECATALOGO",
        ] {
            assert!(!is_engine_provided(None, name), "{name} is the product's own");
        }
    }
}
