//! Turning hits into an answer: the table `group` asks for.
//!
//! ## Why the unresolved are a row and not a footnote
//!
//! A type constraint the classpath could not decide leaves a hit marked
//! [`Hit::unresolved`](crate::engine::Hit::unresolved). Those must appear in the total and in
//! their own row, because the alternative — dropping them — produces a table that *looks*
//! complete and is short by however much the project failed to resolve. On a legacy tree with
//! half its dependencies missing, that is the difference between "this API is used 12 times" and
//! "12 that I could confirm, and 380 I could not read".
//!
//! ## Rows are sorted by count, then by name
//!
//! Descending count, because the question behind every `group` is "which is the big one".
//! Ties break by name so a report is reproducible — a table whose rows shuffle between runs is a
//! table nobody trusts.

use serde::Serialize;

use crate::engine::Hit;
use crate::query::GroupBy;

/// One row of a grouped report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    /// What this row is about: a captured text, a file, a module, an enclosing declaration.
    pub key: String,
    pub count: usize,
    /// How many of `count` carried an undecided type constraint.
    pub unresolved: usize,
    /// Distinct files this row's hits are in — the "where" half of "what, where, how often".
    pub files: usize,
}

/// The answer to a query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// `null` for an ungrouped query, which is a plain hit list.
    pub grouped_by: Option<String>,
    pub rows: Vec<Row>,
    pub total: usize,
    /// The whole query's undecided count, so the panel can say it once at the top.
    pub unresolved: usize,
    pub files: usize,
}

/// The key a hit falls under, or `None` when the query does not group.
///
/// A hit that cannot answer the grouping question — an `enclosing` outside any declaration, a
/// capture a `...` left empty — gets a named bucket rather than being dropped or given an empty
/// key that sorts to the top and reads as a blank row.
fn key_of(hit: &Hit, group: &GroupBy) -> String {
    match group {
        GroupBy::Capture(name) => hit
            .capture(name)
            .map(|c| c.text.clone())
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| "(nothing)".to_string()),
        GroupBy::File => hit.file.clone(),
        GroupBy::Module => module_of(&hit.file),
        GroupBy::Enclosing => hit.enclosing.clone().unwrap_or_else(|| "(top level)".to_string()),
    }
}

/// The Maven module a project-relative path belongs to: everything before `src/`.
///
/// Path-shaped rather than pom-driven on purpose — this crate has no project model, and every
/// Maven layout puts a module's sources under `<module>/src/`. A file outside that shape is
/// reported as the root module rather than guessed at.
pub fn module_of(path: &str) -> String {
    match path.find("src/") {
        Some(0) | None => "(root)".to_string(),
        Some(at) => path[..at].trim_end_matches('/').to_string(),
    }
}

/// Build the report for `hits`.
pub fn build(hits: &[Hit], group: Option<&GroupBy>) -> Report {
    let total = hits.len();
    let unresolved = hits.iter().filter(|h| h.unresolved).count();
    let files = distinct(hits.iter().map(|h| h.file.as_str()));

    let Some(group) = group else {
        return Report { grouped_by: None, rows: Vec::new(), total, unresolved, files };
    };

    // Insertion-ordered accumulation, then one sort — so the tie-break is by name and not by
    // whichever file the walk happened to reach first.
    let mut keys: Vec<String> = Vec::new();
    let mut buckets: Vec<Vec<&Hit>> = Vec::new();
    for hit in hits {
        let key = key_of(hit, group);
        match keys.iter().position(|k| *k == key) {
            Some(at) => buckets[at].push(hit),
            None => {
                keys.push(key);
                buckets.push(vec![hit]);
            }
        }
    }

    let mut rows: Vec<Row> = keys
        .into_iter()
        .zip(buckets)
        .map(|(key, bucket)| Row {
            key,
            count: bucket.len(),
            unresolved: bucket.iter().filter(|h| h.unresolved).count(),
            files: distinct(bucket.iter().map(|h| h.file.as_str())),
        })
        .collect();
    rows.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.key.cmp(&b.key)));

    Report { grouped_by: Some(group.to_string()), rows, total, unresolved, files }
}

fn distinct<'a>(items: impl Iterator<Item = &'a str>) -> usize {
    let mut seen: Vec<&str> = Vec::new();
    for item in items {
        if !seen.contains(&item) {
            seen.push(item);
        }
    }
    seen.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use arbor_syntax::prelude::ByteRange;

    use crate::engine::HitCapture;

    fn hit(file: &str, member: &str, unresolved: bool) -> Hit {
        Hit {
            file: file.to_string(),
            range: ByteRange::new(0, 1),
            line: 1,
            preview: String::new(),
            captures: vec![HitCapture {
                name: "m".to_string(),
                range: ByteRange::new(0, 1),
                text: member.to_string(),
            }],
            enclosing: None,
            unresolved,
        }
    }

    #[test]
    fn grouping_by_a_capture_counts_and_orders_by_size() {
        let hits = [
            hit("a/src/A.java", "place", false),
            hit("a/src/B.java", "place", false),
            hit("a/src/A.java", "cancel", false),
        ];
        let report = build(&hits, Some(&GroupBy::Capture("m".into())));
        assert_eq!(report.total, 3);
        assert_eq!(report.rows[0].key, "place");
        assert_eq!(report.rows[0].count, 2);
        assert_eq!(report.rows[0].files, 2, "the WHERE half of the answer");
        assert_eq!(report.rows[1].key, "cancel");
    }

    /// Reproducibility: two rows with the same count must not shuffle between runs.
    #[test]
    fn ties_break_by_name() {
        let hits = [hit("a/src/A.java", "zeta", false), hit("a/src/A.java", "alpha", false)];
        let report = build(&hits, Some(&GroupBy::Capture("m".into())));
        assert_eq!(report.rows.iter().map(|r| r.key.as_str()).collect::<Vec<_>>(), ["alpha", "zeta"]);
    }

    /// The one that keeps a legacy count honest.
    #[test]
    fn the_undecided_are_counted_in_their_row_and_in_the_total() {
        let hits = [
            hit("a/src/A.java", "place", false),
            hit("a/src/A.java", "place", true),
        ];
        let report = build(&hits, Some(&GroupBy::Capture("m".into())));
        assert_eq!(report.total, 2);
        assert_eq!(report.unresolved, 1);
        assert_eq!(report.rows[0].count, 2, "counted, not dropped");
        assert_eq!(report.rows[0].unresolved, 1, "and visibly undecided");
    }

    #[test]
    fn an_ungrouped_query_is_a_total_and_no_rows() {
        let report = build(&[hit("a/src/A.java", "place", false)], None);
        assert!(report.grouped_by.is_none());
        assert!(report.rows.is_empty());
        assert_eq!(report.total, 1);
    }

    #[test]
    fn a_capture_that_matched_nothing_gets_a_named_bucket() {
        let mut h = hit("a/src/A.java", "", false);
        h.captures[0].text = String::new();
        let report = build(&[h], Some(&GroupBy::Capture("m".into())));
        assert_eq!(report.rows[0].key, "(nothing)", "never a blank row");
    }

    #[test]
    fn modules_come_from_the_path_and_the_root_is_named() {
        assert_eq!(module_of("modules/core/src/main/java/A.java"), "modules/core");
        assert_eq!(module_of("src/main/java/A.java"), "(root)");
        assert_eq!(module_of("README.md"), "(root)");
    }

    #[test]
    fn grouping_by_enclosing_buckets_what_is_outside_one() {
        let mut inside = hit("a/src/A.java", "x", false);
        inside.enclosing = Some("OrderDao.findAll".to_string());
        let outside = hit("a/src/A.java", "x", false);
        let report = build(&[inside, outside], Some(&GroupBy::Enclosing));
        let keys: Vec<&str> = report.rows.iter().map(|r| r.key.as_str()).collect();
        assert!(keys.contains(&"OrderDao.findAll"));
        assert!(keys.contains(&"(top level)"));
    }
}
