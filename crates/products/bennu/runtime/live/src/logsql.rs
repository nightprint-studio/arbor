//! SQL statements in logs: finding them (Hibernate's `org.hibernate.SQL`, p6spy, Spring's `JdbcTemplate`,
//! MyBatis `Preparing:`, Rust `sqlx::query`), reading the time they took when the line carries it, and
//! normalising them so a thousand `where id = 17`, `where id = 18`... become one statement with a count.

use std::sync::LazyLock;

use regex::Regex;

use crate::logrecord::Record;
use crate::logtime::to_ms;

/// A statement found in a record.
#[derive(Debug, Clone, PartialEq)]
pub struct Statement {
    pub sql: String,
    pub ms: Option<f64>,
}

const KEYWORDS: [&str; 7] = ["select", "insert", "update", "delete", "with", "merge", "call"];
/// A logger or message that makes a keyword-led line a statement and not a sentence ("Update failed").
const LOGGER_HINTS: [&str; 10] = ["sql", "jdbc", "query", "hibernate", "mybatis", "mapper", "p6spy", "diesel", "sqlx", "statement"];

static DURATION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:took|duration|elapsed|time|executed in)\s*[:=]?\s*(\d+(?:\.\d+)?)\s*(µs|us|ms|s)\b|\((\d+(?:\.\d+)?)\s*(ms)\)").expect("duration regex")
});
static STRING_LIT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"'(?:[^']|'')*'").expect("string regex"));
static NUMBER_LIT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b\d+(?:\.\d+)?\b|\$\d+").expect("number regex"));
static IN_LIST: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\bin\s*\(\s*\?(?:\s*,\s*\?)*\s*\)").expect("in regex"));
static TUPLES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\(\s*\?(?:\s*,\s*\?)*\s*\)\s*,\s*)+\(\s*\?(?:\s*,\s*\?)*\s*\)").expect("tuples regex"));
static SQLX_FIELD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"(?:db\.statement|summary)="((?:[^"\\]|\\.)*)""#).expect("sqlx regex"));
static SQLX_ELAPSED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"elapsed[=:]\s*(\d+(?:\.\d+)?)\s*(µs|us|ms|s)").expect("elapsed regex"));

/// The time a line says something took, in ms.
pub fn duration_in(text: &str) -> Option<f64> {
    let c = DURATION.captures(text)?;
    let (value, unit) = match (c.get(1), c.get(2), c.get(3), c.get(4)) {
        (Some(v), Some(u), _, _) | (_, _, Some(v), Some(u)) => (v.as_str(), u.as_str().to_ascii_lowercase()),
        _ => return None,
    };
    Some(to_ms(value.parse().ok()?, &unit))
}

fn starts_like_sql(text: &str) -> bool {
    let first = text.trim_start().split(|c: char| !c.is_ascii_alphabetic()).next().unwrap_or("").to_ascii_lowercase();
    KEYWORDS.contains(&first.as_str())
}

fn logger_hints_sql(rec: &Record) -> bool {
    let hay = format!("{} {}", rec.logger.as_deref().unwrap_or(""), rec.head.chars().take(160).collect::<String>()).to_ascii_lowercase();
    LOGGER_HINTS.iter().any(|h| hay.contains(h))
}

/// Between the first `[` after `marker` and the last `]`.
fn bracketed<'a>(text: &'a str, marker: &str) -> Option<&'a str> {
    let after = &text[text.find(marker)? + marker.len()..];
    let close = after.rfind(']')?;
    Some(&after[..close])
}

fn p6spy(head: &str) -> Option<Statement> {
    let parts: Vec<&str> = head.split('|').collect();
    if parts.len() < 5 || !matches!(parts[2].trim(), "statement" | "batch") {
        return None;
    }
    let ms: f64 = parts[1].trim().parse().ok()?;
    // Column 4 is the statement with placeholders, 5 the one with the values: the first groups better.
    let sql = parts[4].trim();
    (!sql.is_empty()).then(|| Statement { sql: sql.to_string(), ms: Some(ms) })
}

fn sqlx(head: &str) -> Option<Statement> {
    if !head.contains("sqlx::query") {
        return None;
    }
    let sql = SQLX_FIELD.captures_iter(head).filter_map(|c| c.get(1)).map(|m| m.as_str()).find(|s| !s.trim().is_empty())?;
    let ms = SQLX_ELAPSED.captures(head).and_then(|c| Some(to_ms(c[1].parse().ok()?, &c[2])));
    Some(Statement { sql: sql.replace("\\\"", "\""), ms })
}

/// The statement a record logs, if it logs one.
pub fn extract(rec: &Record) -> Option<Statement> {
    if let Some(s) = p6spy(&rec.head).or_else(|| sqlx(&rec.head)) {
        return Some(s);
    }
    let msg = if rec.message.is_empty() && rec.logger.is_none() { rec.head.as_str() } else { rec.message.as_str() };
    let sql = [
        "Executing SQL query [",
        "Executing prepared SQL statement [",
        "Executing SQL update [",
        "Executing SQL batch update [",
    ]
    .iter()
    .find_map(|m| bracketed(msg, m).map(str::to_string))
    .or_else(|| msg.split_once("Preparing:").map(|(_, s)| s.trim().to_string()))
    .or_else(|| {
        let hibernate = rec.logger.as_deref().is_some_and(|l| l.ends_with("hibernate.SQL") || l.ends_with("SQL"));
        match (hibernate, msg.trim().is_empty()) {
            // `format_sql: true` puts the statement on the lines under an empty head.
            (true, true) if !rec.rest.is_empty() => Some(rec.rest.iter().map(|l| l.trim()).collect::<Vec<_>>().join(" ")),
            (true, false) => Some(msg.trim().to_string()),
            _ => None,
        }
    })
    .or_else(|| (starts_like_sql(msg) && logger_hints_sql(rec)).then(|| msg.trim().to_string()))?;
    if !starts_like_sql(&sql) {
        return None;
    }
    Some(Statement { sql, ms: duration_in(&rec.head) })
}

/// The statement with its literals and parameters replaced by `?`, whitespace collapsed, cut at `max`.
pub fn normalize(sql: &str, max: usize) -> String {
    let flat = sql.split_whitespace().collect::<Vec<_>>().join(" ");
    let flat = flat.trim_end_matches(';');
    let s = STRING_LIT.replace_all(flat, "?");
    let s = NUMBER_LIT.replace_all(&s, "?");
    let s = IN_LIST.replace_all(&s, "in (?)");
    let s = TUPLES.replace_all(&s, "(?)...");
    if s.chars().count() <= max {
        return s.into_owned();
    }
    format!("{}…", s.chars().take(max).collect::<String>())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logbuf::Stream;
    use crate::logrecord::{build_records, RawLine};
    use arbor_logscan::prelude::RuleSet;

    fn records(texts: &[&str]) -> Vec<Record> {
        let lines: Vec<RawLine> = texts.iter().enumerate().map(|(i, t)| RawLine { seq: i as u64 + 1, ts_ms: None, stream: Stream::Stdout, text: t.to_string() }).collect();
        build_records(&RuleSet::java(), &lines)
    }

    #[test]
    fn hibernate_statements_are_found_and_literals_dropped() {
        let r = records(&[
            "2026-08-05 12:33:01.123 DEBUG 77 --- [nio-8080-exec-1] org.hibernate.SQL : select o1_0.id,o1_0.total from orders o1_0 where o1_0.customer_id=17 and o1_0.status='NEW'",
            "2026-08-05 12:33:01.130 TRACE 77 --- [nio-8080-exec-1] org.hibernate.orm.jdbc.bind : binding parameter [1] as [BIGINT] - [17]",
        ]);
        let s = extract(&r[0]).unwrap();
        assert_eq!(normalize(&s.sql, 200), "select o1_0.id,o1_0.total from orders o1_0 where o1_0.customer_id=? and o1_0.status=?");
        assert!(extract(&r[1]).is_none());
    }

    #[test]
    fn a_multi_line_hibernate_statement_is_joined() {
        let r = records(&["2026-08-05 12:33:01.123 DEBUG 77 --- [main] org.hibernate.SQL :", "    select id", "    from orders"]);
        assert_eq!(extract(&r[0]).unwrap().sql, "select id from orders");
    }

    #[test]
    fn p6spy_carries_its_time() {
        let r = records(&["1785933181000|42|statement|connection 3|select * from t where id=5|select * from t where id=5"]);
        let s = extract(&r[0]).unwrap();
        assert_eq!(s.ms, Some(42.0));
        assert_eq!(normalize(&s.sql, 100), "select * from t where id=?");
    }

    #[test]
    fn sqlx_and_jdbc_template_and_mybatis_are_understood() {
        let r = records(&[
            r#"2026-08-05T12:33:01.123456Z  INFO sqlx::query: summary="SELECT id FROM users WHERE id = $1" db.statement="" rows_affected=0 rows_returned=1 elapsed=1.234ms"#,
            "2026-08-05 12:33:01.123 DEBUG 7 --- [main] o.s.jdbc.core.JdbcTemplate : Executing SQL query [select count(*) from orders where day > '2026-01-01']",
            "2026-08-05 12:33:01.123 DEBUG 7 --- [main] c.a.OrderMapper.find : ==>  Preparing: select * from orders where id = ?",
        ]);
        let a = extract(&r[0]).unwrap();
        assert_eq!(normalize(&a.sql, 100), "SELECT id FROM users WHERE id = ?");
        assert!((a.ms.unwrap() - 1.234).abs() < 1e-9);
        assert_eq!(normalize(&extract(&r[1]).unwrap().sql, 100), "select count(*) from orders where day > ?");
        assert!(extract(&r[2]).unwrap().sql.starts_with("select * from orders"));
    }

    #[test]
    fn a_sentence_that_starts_with_a_keyword_is_not_a_statement() {
        let r = records(&["2026-08-05 12:33:01.123 WARN 7 --- [main] c.a.Service : Update failed for user 3"]);
        assert!(extract(&r[0]).is_none());
    }

    #[test]
    fn in_lists_and_value_tuples_collapse() {
        assert_eq!(normalize("select * from t where id in (1, 2, 3, 4)", 100), "select * from t where id in (?)");
        assert_eq!(normalize("insert into t (a,b) values (1,'x'), (2,'y'), (3,'z');", 100), "insert into t (a,b) values (?)...");
    }

    #[test]
    fn durations_are_read_in_several_spellings() {
        assert_eq!(duration_in("query took 12 ms"), Some(12.0));
        assert_eq!(duration_in("Duration: 0.5s"), Some(500.0));
        assert_eq!(duration_in("done (340 ms)"), Some(340.0));
        assert_eq!(duration_in("nothing here"), None);
    }

    #[test]
    fn long_statements_are_cut_with_a_mark() {
        let n = normalize(&format!("select {} from t", "col, ".repeat(100)), 50);
        assert!(n.ends_with('…') && n.chars().count() == 51);
    }
}
