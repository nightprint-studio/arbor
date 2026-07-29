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

/// How a cell was written.
///
/// ## Why a prefix and not a guess
///
/// A tool that writes SQL into somebody's database cannot decide *for* the user
/// whether `SEQ_ORDINI.nextval` is a sequence or the text of a label. Guessing
/// wrong in one direction inserts a string where a number was meant; wrong in the
/// other, it passes a user's literal through unquoted — which is not a bug you
/// find in review.
///
/// So the intent is **written down**. A leading `=` means "this is an
/// expression, emit it as SQL"; everything else is a value and gets quoted. It is
/// one character, it is the convention every spreadsheet has taught everyone, and
/// there is nothing to infer.
///
/// `==` escapes it, for the value that genuinely starts with an equals sign.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Written<'a> {
    /// Nothing was typed. Not the same as `NULL`: a column nobody supplied is left
    /// out of the statement entirely, so its default still applies.
    Nothing,
    /// A value. Quoted, or emitted bare when the column is numeric.
    Value(&'a str),
    /// SQL. Passed through, with the two spellings of "now" translated per dialect.
    Expression(&'a str),
}

/// Read a cell as the user wrote it.
pub fn read(value: &str) -> Written<'_> {
    let raw = value.trim();
    if raw.is_empty() {
        return Written::Nothing;
    }
    if raw.starts_with("==") {
        // `==A` is the value `=A`: the escape eats one of the two, and what is
        // left still starts with an equals sign. Doubling to escape is the rule
        // SQL already uses for a quote inside a literal, so it is the one nobody
        // here has to be taught.
        return Written::Value(&raw[1..]);
    }
    match raw.strip_prefix('=') {
        Some(expression) => Written::Expression(expression.trim()),
        None => Written::Value(raw),
    }
}

/// The spellings of "now" that mean the same thing in both dialects.
const NOW: [&str; 4] = ["SYSDATE", "CURRENT_TIMESTAMP", "CURRENT_DATE", "NOW()"];

fn is_now(value: &str) -> bool {
    let v = value.trim().to_ascii_uppercase();
    NOW.contains(&v.as_str())
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
/// Empty → `NULL`; an `=` expression passes through, with "now" translated for
/// the dialect; a number stays bare when the column is numeric; everything else
/// becomes a quoted literal with its quotes doubled.
pub fn literal(value: Option<&str>, column: &Column, scope: DialectScope) -> String {
    match read(value.unwrap_or("")) {
        Written::Nothing => "NULL".to_string(),
        Written::Expression(sql) => expression(sql, scope),
        Written::Value(raw) => {
            if is_numeric_type(&column.data_type) && is_plain_number(raw) {
                return raw.to_string();
            }
            format!("'{}'", raw.replace('\'', "''"))
        }
    }
}

/// An `=` cell, as SQL.
///
/// Passed through as written, with one exception that earns itself: the two
/// dialects spell "now" differently, and a portable script needs the spelling they
/// share. Everything else — a sequence, a subquery, another column, a function
/// call — is the user's SQL and Picus does not touch it. It cannot: a rewrite of
/// somebody's expression is a rewrite of something only they understand.
fn expression(sql: &str, scope: DialectScope) -> String {
    if is_now(sql) {
        return now_function(scope).to_string();
    }
    sql.to_string()
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
    let raw = match read(value) {
        // An expression is the user's SQL. Picus cannot type-check it — the value
        // is decided by the database at install time — and pretending otherwise
        // would mean refusing a correct `SEQ.nextval` because it is not a number.
        // The one thing it can say is that the `=` was left on its own.
        Written::Expression(sql) => {
            return sql
                .is_empty()
                .then(|| "an = on its own says nothing — write the SQL after it".to_string());
        }
        Written::Value(raw) => raw,
        Written::Nothing => "",
    };

    if raw.is_empty() {
        // An empty cell means *not supplied*, and a column that is not supplied is
        // left out of the statement entirely (`DmlModel::supplied_columns`) — so
        // what decides this is not whether the column accepts NULL, but whether
        // the database has something to put there when nobody says.
        //
        //  • a **default** does. `customized NUMBER DEFAULT 0 NOT NULL` is the
        //    ordinary shape of an audit column, and reporting it as required was
        //    demanding a value for the one case the default exists to cover;
        //  • a **primary key** usually does too — a sequence, a trigger, or the
        //    row's own key — so it has never been flagged here.
        let supplied_by_the_database = column.primary_key || column.default_value.is_some();
        return (column.not_null && !supplied_by_the_database)
            .then(|| "required (NOT NULL, and the column has no default)".to_string());
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
    fn empty_becomes_null_and_an_explicit_null_stays_null() {
        let c = col("NOTE", "varchar(50)");
        let oracle = DialectScope::One(EngineKind::Oracle);
        assert_eq!(literal(Some(""), &c, oracle), "NULL");
        assert_eq!(literal(None, &c, oracle), "NULL");
        // `=NULL`, not `null`: since the prefix, the bare word is the word. See
        // `a_value_that_reads_like_sql_is_still_a_value_without_the_prefix`.
        assert_eq!(literal(Some("=null"), &c, oracle), "null");
        assert_eq!(literal(Some("=NULL"), &c, oracle), "NULL");
    }

    #[test]
    fn now_is_translated_per_dialect() {
        let c = col("D", "date");
        assert_eq!(literal(Some("=SYSDATE"), &c, DialectScope::One(EngineKind::Oracle)), "SYSDATE");
        assert_eq!(
            literal(Some("=SYSDATE"), &c, DialectScope::One(EngineKind::Postgres)),
            "CURRENT_TIMESTAMP"
        );
        assert_eq!(literal(Some("=now()"), &c, DialectScope::One(EngineKind::Oracle)), "SYSDATE");
    }

    #[test]
    fn a_portable_script_gets_the_now_both_engines_accept() {
        // Neither `SYSDATE` nor `now()` — the standard spelling, which is the
        // intersection and is what `DIA001` will accept back from the file.
        let c = col("D", "date");
        for written in ["=SYSDATE", "=now()", "=CURRENT_TIMESTAMP"] {
            assert_eq!(literal(Some(written), &c, DialectScope::Portable), "CURRENT_TIMESTAMP");
        }
        assert_eq!(now_function(DialectScope::Portable), "CURRENT_TIMESTAMP");
    }

    // ── The `=` prefix ────────────────────────────────────────────────────────

    #[test]
    fn an_expression_is_whatever_the_user_wrote_after_the_equals() {
        // The whole point: not a closed list. A sequence, a subquery, another
        // column, a function nobody here has heard of — all of them are SQL and
        // none of them is Picus's to interpret.
        let c = col("ID", "numeric");
        let oracle = DialectScope::One(EngineKind::Oracle);
        for (written, expected) in [
            ("=SEQ_CATALOGO.nextval", "SEQ_CATALOGO.nextval"),
            ("=(SELECT MAX(ID) + 1 FROM CATALOGO_WIDGET)", "(SELECT MAX(ID) + 1 FROM CATALOGO_WIDGET)"),
            ("=CHIAVE", "CHIAVE"),
            ("=NULL", "NULL"),
            ("=UPPER(TRIM(' x '))", "UPPER(TRIM(' x '))"),
        ] {
            assert_eq!(literal(Some(written), &c, oracle), expected, "{written}");
        }
    }

    #[test]
    fn a_value_that_reads_like_sql_is_still_a_value_without_the_prefix() {
        // The behaviour the prefix exists to make unambiguous, and the reason it
        // is a **breaking** change worth making: `SYSDATE` typed into a
        // description column is a description.
        let c = col("NOTE", "varchar(50)");
        let oracle = DialectScope::One(EngineKind::Oracle);
        assert_eq!(literal(Some("SYSDATE"), &c, oracle), "'SYSDATE'");
        assert_eq!(literal(Some("SEQ.nextval"), &c, oracle), "'SEQ.nextval'");
        // …including NULL. An empty cell is how "nothing" is said; the word is a
        // word.
        assert_eq!(literal(Some("NULL"), &c, oracle), "'NULL'");
    }

    #[test]
    fn a_doubled_equals_is_a_value_that_starts_with_one() {
        let c = col("NOTE", "varchar(50)");
        let oracle = DialectScope::One(EngineKind::Oracle);
        assert_eq!(literal(Some("==A"), &c, oracle), "'=A'");
        assert_eq!(literal(Some("=="), &c, oracle), "'='");
        assert_eq!(read("==A"), Written::Value("=A"));
    }

    #[test]
    fn an_expression_is_not_type_checked_but_an_empty_one_is_refused() {
        let n = col("V", "numeric");
        assert_eq!(validate_value("=SEQ.nextval", &n), None, "a sequence is not a number yet");
        assert_eq!(validate_value("=(SELECT MAX(V) FROM T)", &n), None);
        let short = col("CODE", "varchar(3)");
        assert_eq!(validate_value("=UPPER(QUALCOSA_DI_LUNGO)", &short), None, "no length limit");

        let refused = validate_value("=", &n).expect("an = alone says nothing");
        assert!(refused.contains("write the SQL after it"), "{refused}");
    }

    #[test]
    fn reading_a_cell_keeps_nothing_apart_from_null() {
        // Three different intentions, three different answers — and the first two
        // are the pair the generator rests on: an unsupplied column is left OUT of
        // the statement, so its default applies.
        assert_eq!(read(""), Written::Nothing);
        assert_eq!(read("   "), Written::Nothing);
        assert_eq!(read("=NULL"), Written::Expression("NULL"));
        assert_eq!(read("ciao"), Written::Value("ciao"));
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
        assert_eq!(validate_value("=SYSDATE", &n), None, "an expression is not type-checked");
        // …and without the prefix it is a value, so it is: the point of the prefix.
        assert_eq!(validate_value("SYSDATE", &n).as_deref(), Some("not a number (numeric)"));

        c.not_null = true;
        assert_eq!(
            validate_value("", &c).as_deref(),
            Some("required (NOT NULL, and the column has no default)")
        );
        c.primary_key = true;
        assert_eq!(validate_value("", &c), None, "a key is often filled by a sequence");
    }

    #[test]
    fn a_not_null_column_with_a_default_is_not_required() {
        // The ordinary shape of an audit column — `CUSTOMIZED NUMBER DEFAULT 0 NOT
        // NULL` — and reporting it as required demanded a value for exactly the
        // case the default exists to cover. An empty cell means *not supplied*, and
        // an unsupplied column is left out of the statement, so the default applies.
        let mut c = col("CUSTOMIZED", "numeric");
        c.not_null = true;
        c.default_value = Some("0".to_string());
        assert_eq!(validate_value("", &c), None);

        // Take the default away and it is required again.
        c.default_value = None;
        assert!(validate_value("", &c).is_some());
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
