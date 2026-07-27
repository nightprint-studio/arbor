//! Pure SQL helpers — no driver, no connection, fully unit-testable.
//!
//! Everything here is the kind of logic that is easy to get subtly wrong and
//! expensive to debug against a live server, so it lives apart from the session
//! and is tested directly.

use picus_db_api::prelude::DbError;

/// Quote an identifier for PostgreSQL: wrap in double quotes, doubling any inside.
///
/// **Load-bearing for safety.** Object names reach us from the schema and from the
/// user, and they are the one part of a generated statement that cannot be a bound
/// parameter — a table called `x"; DROP TABLE y; --` has to come out as a single
/// harmless identifier.
pub fn quote_ident(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 2);
    out.push('"');
    for ch in name.chars() {
        if ch == '"' {
            out.push('"');
        }
        out.push(ch);
    }
    out.push('"');
    out
}

/// Quote a possibly schema-qualified name (`public.orders` → `"public"."orders"`).
///
/// Splits on the FIRST dot only: a schema name containing a dot must be quoted by
/// the caller, and an unqualified name stays unqualified rather than being pinned
/// to a guessed schema.
pub fn quote_qualified(name: &str) -> String {
    match name.split_once('.') {
        Some((schema, rel)) => format!("{}.{}", quote_ident(schema), quote_ident(rel)),
        None => quote_ident(name),
    }
}

/// What a statement does, as far as a lexical look can tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatementKind {
    /// `SELECT`, `SHOW`, `EXPLAIN`, a read-only `WITH`.
    Read,
    /// Anything that can change data or schema.
    Write,
    /// Session setup (`SET`, `BEGIN`, `COMMIT`) — neither, and allowed read-only.
    Session,
}

impl StatementKind {
    /// The verb to put in a refusal message.
    pub fn label(self) -> &'static str {
        match self {
            Self::Read => "a read",
            Self::Write => "a write",
            Self::Session => "a session command",
        }
    }
}

/// Classify a statement by its leading keyword.
///
/// This is a **courtesy check**, not the enforcement: the session is opened in a
/// read-only transaction mode, so the server is the authority that refuses a write.
/// The value of doing it here anyway is the message — "this connection is
/// read-only" is a better thing to read than PostgreSQL's
/// `cannot execute UPDATE in a read-only transaction`, and it arrives without a
/// round-trip. Being lexical, it errs toward [`StatementKind::Write`]: an unknown
/// verb is treated as a write, so a gap in this list can only ever be too strict.
pub fn statement_kind(sql: &str) -> StatementKind {
    let s = strip_leading_noise(sql);
    let head = s
        .split(|c: char| c.is_whitespace() || c == '(' || c == ';')
        .find(|w| !w.is_empty())
        .unwrap_or("")
        .to_ascii_uppercase();

    match head.as_str() {
        "SELECT" | "SHOW" | "EXPLAIN" | "TABLE" | "VALUES" | "FETCH" => StatementKind::Read,
        "SET" | "BEGIN" | "START" | "COMMIT" | "ROLLBACK" | "SAVEPOINT" | "RELEASE"
        | "DISCARD" | "RESET" | "DEALLOCATE" | "CLOSE" | "DECLARE" | "LISTEN" | "UNLISTEN" => {
            StatementKind::Session
        }
        // A CTE is read-only until it isn't: `WITH x AS (…) INSERT …` is a write,
        // and so is a data-modifying CTE. Look for the verbs anywhere in the rest.
        "WITH" => {
            if contains_write_verb(&s) {
                StatementKind::Write
            } else {
                StatementKind::Read
            }
        }
        _ => StatementKind::Write,
    }
}

/// Drop leading whitespace and comments so the first real keyword is reachable.
/// Handles `--` line comments and `/* */` blocks (including nested ones, which
/// PostgreSQL allows).
fn strip_leading_noise(sql: &str) -> &str {
    let mut s = sql.trim_start();
    loop {
        if let Some(rest) = s.strip_prefix("--") {
            s = match rest.find('\n') {
                Some(i) => rest[i + 1..].trim_start(),
                None => return "",
            };
            continue;
        }
        if s.starts_with("/*") {
            let bytes = s.as_bytes();
            let mut depth = 0usize;
            let mut i = 0usize;
            while i + 1 < bytes.len() {
                if bytes[i] == b'/' && bytes[i + 1] == b'*' {
                    depth += 1;
                    i += 2;
                } else if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    i += 1;
                }
            }
            if depth != 0 {
                return ""; // unterminated comment: nothing classifiable
            }
            s = s[i..].trim_start();
            continue;
        }
        return s;
    }
}

/// Does the statement contain a data-modifying verb as a standalone word?
fn contains_write_verb(s: &str) -> bool {
    const VERBS: [&str; 6] = ["INSERT", "UPDATE", "DELETE", "MERGE", "TRUNCATE", "CREATE"];
    let upper = s.to_ascii_uppercase();
    VERBS.iter().any(|v| {
        upper.match_indices(v).any(|(i, _)| {
            let before_ok = i == 0
                || !upper.as_bytes()[i - 1].is_ascii_alphanumeric() && upper.as_bytes()[i - 1] != b'_';
            let after = i + v.len();
            let after_ok = after >= upper.len()
                || !upper.as_bytes()[after].is_ascii_alphanumeric() && upper.as_bytes()[after] != b'_';
            before_ok && after_ok
        })
    })
}

/// Refuse a statement that would write on a read-only connection.
pub fn guard_read_only(sql: &str, read_only: bool) -> Result<(), DbError> {
    if !read_only {
        return Ok(());
    }
    match statement_kind(sql) {
        StatementKind::Write => {
            Err(DbError::ReadOnly { statement: StatementKind::Write.label().to_string() })
        }
        _ => Ok(()),
    }
}

// ── pg_trigger.tgtype bit decoding ─────────────────────────────────────────────
//
// The bitmask is documented in the PostgreSQL catalogs and is the only way to get
// a trigger's timing and events out of pg_catalog. Decoded here, tested here.

const TG_ROW: i16 = 1 << 0;
const TG_BEFORE: i16 = 1 << 1;
const TG_INSERT: i16 = 1 << 2;
const TG_DELETE: i16 = 1 << 3;
const TG_UPDATE: i16 = 1 << 4;
const TG_TRUNCATE: i16 = 1 << 5;
const TG_INSTEAD: i16 = 1 << 6;

/// `BEFORE` / `AFTER` / `INSTEAD OF` from `pg_trigger.tgtype`.
pub fn trigger_timing(tgtype: i16) -> &'static str {
    if tgtype & TG_INSTEAD != 0 {
        "INSTEAD OF"
    } else if tgtype & TG_BEFORE != 0 {
        "BEFORE"
    } else {
        "AFTER"
    }
}

/// The events a trigger answers to, in statement order.
pub fn trigger_events(tgtype: i16) -> Vec<String> {
    let mut out = Vec::new();
    for (bit, name) in [
        (TG_INSERT, "INSERT"),
        (TG_UPDATE, "UPDATE"),
        (TG_DELETE, "DELETE"),
        (TG_TRUNCATE, "TRUNCATE"),
    ] {
        if tgtype & bit != 0 {
            out.push(name.to_string());
        }
    }
    out
}

/// Row-level (`FOR EACH ROW`) vs statement-level.
pub fn trigger_for_each_row(tgtype: i16) -> bool {
    tgtype & TG_ROW != 0
}

/// `pg_constraint.confdeltype` → the `ON DELETE` action, or `None` for the default
/// (`NO ACTION`), which is not worth showing.
pub fn fk_on_delete(code: i8) -> Option<String> {
    match code as u8 as char {
        'c' => Some("CASCADE".to_string()),
        'n' => Some("SET NULL".to_string()),
        'r' => Some("RESTRICT".to_string()),
        'd' => Some("SET DEFAULT".to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting_neutralises_a_hostile_identifier() {
        assert_eq!(quote_ident("orders"), r#""orders""#);
        assert_eq!(quote_ident("Mixed Case"), r#""Mixed Case""#);
        // The whole point: this must come out as ONE identifier, not a statement.
        assert_eq!(quote_ident(r#"x"; DROP TABLE y; --"#), r#""x""; DROP TABLE y; --""#);
    }

    #[test]
    fn qualified_names_split_on_the_first_dot_only() {
        assert_eq!(quote_qualified("public.orders"), r#""public"."orders""#);
        assert_eq!(quote_qualified("orders"), r#""orders""#);
        assert_eq!(quote_qualified("a.b.c"), r#""a"."b.c""#);
    }

    #[test]
    fn reads_writes_and_session_commands_are_told_apart() {
        assert_eq!(statement_kind("select 1"), StatementKind::Read);
        assert_eq!(statement_kind("  SELECT * FROM t"), StatementKind::Read);
        assert_eq!(statement_kind("EXPLAIN ANALYZE SELECT 1"), StatementKind::Read);
        assert_eq!(statement_kind("update t set a=1"), StatementKind::Write);
        assert_eq!(statement_kind("DROP TABLE t"), StatementKind::Write);
        assert_eq!(statement_kind("SET search_path TO x"), StatementKind::Session);
        assert_eq!(statement_kind("BEGIN"), StatementKind::Session);
    }

    #[test]
    fn leading_comments_do_not_hide_the_verb() {
        assert_eq!(statement_kind("-- a note\nDELETE FROM t"), StatementKind::Write);
        assert_eq!(statement_kind("/* header */ SELECT 1"), StatementKind::Read);
        assert_eq!(statement_kind("/* a /* nested */ block */ SELECT 1"), StatementKind::Read);
        assert_eq!(statement_kind("--only a comment"), StatementKind::Write, "unclassifiable is a write");
    }

    #[test]
    fn a_data_modifying_cte_is_a_write() {
        assert_eq!(
            statement_kind("WITH x AS (SELECT 1) SELECT * FROM x"),
            StatementKind::Read
        );
        assert_eq!(
            statement_kind("WITH d AS (DELETE FROM t RETURNING *) SELECT * FROM d"),
            StatementKind::Write
        );
    }

    #[test]
    fn a_word_containing_a_verb_is_not_a_verb() {
        // `UPDATED_AT` must not make a read-only CTE look like a write.
        assert_eq!(
            statement_kind("WITH x AS (SELECT updated_at FROM t) SELECT * FROM x"),
            StatementKind::Read
        );
    }

    #[test]
    fn the_read_only_guard_refuses_writes_only() {
        assert!(guard_read_only("SELECT 1", true).is_ok());
        assert!(guard_read_only("SET search_path TO x", true).is_ok());
        assert!(guard_read_only("DELETE FROM t", true).is_err());
        assert!(guard_read_only("DELETE FROM t", false).is_ok(), "not read-only: allowed");
    }

    #[test]
    fn trigger_bits_decode() {
        // BEFORE INSERT OR UPDATE, FOR EACH ROW = ROW|BEFORE|INSERT|UPDATE
        let tg = TG_ROW | TG_BEFORE | TG_INSERT | TG_UPDATE;
        assert_eq!(trigger_timing(tg), "BEFORE");
        assert_eq!(trigger_events(tg), vec!["INSERT", "UPDATE"]);
        assert!(trigger_for_each_row(tg));

        // AFTER DELETE, statement level
        let tg = TG_DELETE;
        assert_eq!(trigger_timing(tg), "AFTER");
        assert_eq!(trigger_events(tg), vec!["DELETE"]);
        assert!(!trigger_for_each_row(tg));

        // INSTEAD OF wins over the BEFORE bit
        let tg = TG_ROW | TG_INSTEAD | TG_BEFORE | TG_INSERT;
        assert_eq!(trigger_timing(tg), "INSTEAD OF");
    }

    #[test]
    fn fk_delete_actions_map_and_the_default_stays_quiet() {
        assert_eq!(fk_on_delete(b'c' as i8).as_deref(), Some("CASCADE"));
        assert_eq!(fk_on_delete(b'n' as i8).as_deref(), Some("SET NULL"));
        assert_eq!(fk_on_delete(b'a' as i8), None, "NO ACTION is not worth showing");
    }
}
