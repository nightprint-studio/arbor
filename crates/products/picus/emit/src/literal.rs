//! Values → SQL, and the validation that runs before they get there.
//!
//! Small, and the part most worth getting exactly right: every bug here becomes a
//! statement that either fails on a customer's database or, worse, succeeds and
//! writes the wrong thing.

use picus_types::prelude::{Column, DialectScope, EngineKind};

/// Column types whose values are emitted bare rather than quoted.
pub fn is_numeric_type(data_type: &str) -> bool {
    let t = data_type.to_ascii_uppercase();
    ["NUMBER", "INT", "NUMERIC", "DECIMAL", "FLOAT", "DOUBLE", "REAL"]
        .iter()
        .any(|k| t.contains(k))
}

/// The values a user means as **expressions**, not as string literals.
///
/// A closed list on purpose. The alternative — guessing whether something looks
/// like an expression — would eventually pass a user's literal text through
/// unquoted, and in a tool that writes SQL into someone's database that is not a
/// bug you find in review.
const EXPRESSIONS: [&str; 5] = ["SYSDATE", "CURRENT_TIMESTAMP", "CURRENT_DATE", "NOW()", "NULL"];

pub fn looks_like_expression(value: &str) -> bool {
    let v = value.trim().to_ascii_uppercase();
    EXPRESSIONS.contains(&v.as_str())
}

/// The scope's "now".
///
/// `SYSDATE` is Oracle's and `now()` is PostgreSQL's, but **`CURRENT_TIMESTAMP`
/// is standard and both engines accept it** — so the portable answer is a real
/// value rather than a refusal. That is not a lucky accident, it is the shape of
/// the whole feature: the intersection of two dialects is usually smaller than
/// either, not empty, and the point of a portable folder is to live in it.
///
/// It self-checks, too: `DIA001` over a portable folder flags `SYSDATE` and
/// `now()` and says nothing about `CURRENT_TIMESTAMP`, so what this emits is
/// exactly what the analyser will accept back.
pub fn now_function(scope: DialectScope) -> &'static str {
    match scope.dialect() {
        Some(EngineKind::Oracle) => "SYSDATE",
        Some(EngineKind::Postgres) | None => "CURRENT_TIMESTAMP",
    }
}

/// Identifier casing is a dialect difference, not a formatting preference.
///
/// `lowercase` is a per-project convention and applies to PostgreSQL only —
/// unquoted identifiers fold to lower case there and to upper case on Oracle, so
/// applying it to both would produce Oracle scripts that no longer match the
/// surrounding code. A **portable** script is in exactly that position twice
/// over: it is read by both engines, which fold in opposite directions, so it
/// leaves the identifier as written and lets each engine do what it does.
pub fn ident(name: &str, scope: DialectScope, lowercase: bool) -> String {
    match scope.dialect() {
        Some(EngineKind::Postgres) if lowercase => name.to_lowercase(),
        _ => name.to_string(),
    }
}

/// Render one value as SQL.
///
/// Empty → `NULL`; a recognised expression passes through, translated for the
/// dialect; a number stays bare when the column is numeric; everything else
/// becomes a quoted literal with its quotes doubled.
pub fn literal(value: Option<&str>, column: &Column, scope: DialectScope) -> String {
    let raw = value.unwrap_or("").trim();
    if raw.is_empty() {
        return "NULL".to_string();
    }
    if looks_like_expression(raw) {
        let upper = raw.to_ascii_uppercase();
        return match upper.as_str() {
            "NULL" => "NULL".to_string(),
            "SYSDATE" | "NOW()" | "CURRENT_TIMESTAMP" => now_function(scope).to_string(),
            other => other.to_string(),
        };
    }
    if is_numeric_type(&column.data_type) && is_plain_number(raw) {
        return raw.to_string();
    }
    format!("'{}'", raw.replace('\'', "''"))
}

/// A decimal number with no exponent, no sign games and no spaces — conservative
/// on purpose, because anything not matched simply gets quoted, which is safe.
fn is_plain_number(s: &str) -> bool {
    let body = s.strip_prefix('-').unwrap_or(s);
    if body.is_empty() {
        return false;
    }
    let mut seen_dot = false;
    for (i, c) in body.chars().enumerate() {
        match c {
            '0'..='9' => {}
            '.' if !seen_dot && i > 0 && i < body.len() - 1 => seen_dot = true,
            _ => return false,
        }
    }
    true
}

/// Why a value cannot be written to this column — `None` when it can.
///
/// Reported rather than silently corrected: the point is to tell the user before
/// the script runs, not to guess what they meant.
pub fn validate_value(value: &str, column: &Column) -> Option<String> {
    let raw = value.trim();

    if raw.is_empty() {
        // A primary key left empty is usually about to be filled by a sequence or
        // by the row's own key, so it is not flagged here.
        return (column.not_null && !column.primary_key).then(|| "required (NOT NULL)".to_string());
    }
    if looks_like_expression(raw) {
        return None;
    }
    if is_numeric_type(&column.data_type) && !is_plain_number(raw) {
        return Some(format!("not a number ({})", column.data_type));
    }
    if let Some(limit) = char_limit(&column.data_type) {
        if raw.chars().count() > limit {
            return Some(format!("longer than {limit} characters"));
        }
    }
    None
}

/// The declared length of a character type, e.g. `varchar(30)` → 30.
///
/// Counts **characters**, not bytes, which is what the length in the type means on
/// both engines for their character types — and is why an accented value that fits
/// must not be reported as too long.
fn char_limit(data_type: &str) -> Option<usize> {
    let upper = data_type.to_ascii_uppercase();
    if !upper.contains("CHAR") {
        return None;
    }
    let open = data_type.find('(')?;
    let close = data_type[open..].find(')')? + open;
    let inside = &data_type[open + 1..close];
    // `numeric(5,2)`-style precision is not a character limit; for a char type the
    // first number is the length.
    inside.split(',').next()?.trim().parse::<usize>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn col(name: &str, ty: &str) -> Column {
        Column {
            name: name.to_string(),
            data_type: ty.to_string(),
            primary_key: false,
            not_null: false,
            default_value: None,
        }
    }

    #[test]
    fn a_quote_in_a_value_cannot_end_the_literal() {
        let c = col("NOTE", "varchar(50)");
        assert_eq!(literal(Some("d'Annunzio"), &c, DialectScope::One(EngineKind::Oracle)), "'d''Annunzio'");
        // The hostile shape: it must stay one literal.
        assert_eq!(
            literal(Some("x'; DROP TABLE T; --"), &c, DialectScope::One(EngineKind::Oracle)),
            "'x''; DROP TABLE T; --'"
        );
    }

    #[test]
    fn empty_becomes_null_and_null_stays_null() {
        let c = col("NOTE", "varchar(50)");
        assert_eq!(literal(Some(""), &c, DialectScope::One(EngineKind::Oracle)), "NULL");
        assert_eq!(literal(None, &c, DialectScope::One(EngineKind::Oracle)), "NULL");
        assert_eq!(literal(Some("null"), &c, DialectScope::One(EngineKind::Oracle)), "NULL");
    }

    #[test]
    fn now_is_translated_per_dialect() {
        let c = col("D", "date");
        assert_eq!(literal(Some("SYSDATE"), &c, DialectScope::One(EngineKind::Oracle)), "SYSDATE");
        assert_eq!(literal(Some("SYSDATE"), &c, DialectScope::One(EngineKind::Postgres)), "CURRENT_TIMESTAMP");
        assert_eq!(literal(Some("now()"), &c, DialectScope::One(EngineKind::Oracle)), "SYSDATE");
    }

    #[test]
    fn a_portable_script_gets_the_now_both_engines_accept() {
        // Neither `SYSDATE` nor `now()` — the standard spelling, which is the
        // intersection and is what `DIA001` will accept back from the file.
        let c = col("D", "date");
        for written in ["SYSDATE", "now()", "CURRENT_TIMESTAMP"] {
            assert_eq!(literal(Some(written), &c, DialectScope::Portable), "CURRENT_TIMESTAMP");
        }
        assert_eq!(now_function(DialectScope::Portable), "CURRENT_TIMESTAMP");
    }

    #[test]
    fn a_portable_script_never_folds_an_identifier() {
        // The two engines fold unquoted identifiers in opposite directions, so
        // one file cannot be pre-folded for both: it is left as written and each
        // engine does what it does.
        assert_eq!(ident("PARAMETRI", DialectScope::Portable, true), "PARAMETRI");
        assert_eq!(ident("PARAMETRI", DialectScope::Portable, false), "PARAMETRI");
    }

    #[test]
    fn numbers_stay_bare_only_in_numeric_columns() {
        let n = col("V", "numeric(5,2)");
        let t = col("CODE", "varchar(10)");
        assert_eq!(literal(Some("15"), &n, DialectScope::One(EngineKind::Oracle)), "15");
        assert_eq!(literal(Some("-1.5"), &n, DialectScope::One(EngineKind::Oracle)), "-1.5");
        // The one that matters: an account code must keep its leading zeros.
        assert_eq!(literal(Some("007"), &t, DialectScope::One(EngineKind::Oracle)), "'007'");
    }

    #[test]
    fn a_value_that_only_looks_numeric_is_quoted() {
        let n = col("V", "numeric");
        for odd in ["1e5", "1.2.3", "1 2", "+3", "0x1F", ".5", "5."] {
            assert!(
                literal(Some(odd), &n, DialectScope::One(EngineKind::Oracle)).starts_with('\''),
                "{odd} must be quoted rather than emitted bare"
            );
        }
    }

    #[test]
    fn identifier_casing_never_touches_oracle() {
        assert_eq!(ident("PARAMETRI", DialectScope::One(EngineKind::Postgres), true), "parametri");
        assert_eq!(ident("PARAMETRI", DialectScope::One(EngineKind::Postgres), false), "PARAMETRI");
        assert_eq!(ident("PARAMETRI", DialectScope::One(EngineKind::Oracle), true), "PARAMETRI");
    }

    #[test]
    fn validation_reports_the_reason() {
        let mut c = col("CODE", "varchar(3)");
        assert_eq!(validate_value("AB", &c), None);
        assert_eq!(validate_value("ABCD", &c).as_deref(), Some("longer than 3 characters"));

        let n = col("V", "numeric");
        assert_eq!(validate_value("nope", &n).as_deref(), Some("not a number (numeric)"));
        assert_eq!(validate_value("SYSDATE", &n), None, "an expression is not type-checked");

        c.not_null = true;
        assert_eq!(validate_value("", &c).as_deref(), Some("required (NOT NULL)"));
        c.primary_key = true;
        assert_eq!(validate_value("", &c), None, "a key is often filled by a sequence");
    }

    #[test]
    fn a_length_limit_counts_characters_not_bytes() {
        let c = col("NOTE", "varchar(5)");
        // Five accented characters are five characters, though they are ten bytes.
        assert_eq!(validate_value("àèìòù", &c), None);
        assert_eq!(validate_value("àèìòùx", &c).as_deref(), Some("longer than 5 characters"));
    }

    #[test]
    fn numeric_precision_is_not_mistaken_for_a_length_limit() {
        let c = col("V", "numeric(5,2)");
        assert_eq!(validate_value("123.45", &c), None);
    }
}
