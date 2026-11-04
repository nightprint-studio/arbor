//! `bennu_scheduled_jobs` — `@Scheduled` methods, from the scheduling extension's catalog rows.

use bennu_ext::prelude::ExtEntry;
use serde::Serialize;

use super::has;

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct JobRow {
    pub class: String,
    pub method: String,
    /// `cron` | `fixed` | `none` (an `@Scheduled` that says nothing about when).
    pub trigger: String,
    /// The cron expression, or `fixedRate = 5000` as written.
    pub schedule: String,
    /// A cron in plain words, when it could be read.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
}

#[derive(Debug, Default)]
pub struct JobFilter {
    pub name: Option<String>,
}

/// The extension publishes a display row: `tags` holds the cron expression (or `fixed` / `incomplete`
/// for the other two triggers) and `secondary` the reading of it.
pub fn from_entry(e: &ExtEntry) -> JobRow {
    let (owner, method) = e.id.split_once('#').map(|(o, m)| (o.to_string(), m.to_string())).unwrap_or_else(|| (String::new(), e.primary.clone()));
    let (trigger, schedule, description) = if e.tags.iter().any(|t| t == "incomplete") {
        ("none", String::new(), String::new())
    } else if e.tags.iter().any(|t| t == "fixed") {
        ("fixed", e.secondary.clone(), String::new())
    } else {
        let expression = e.tags.first().cloned().unwrap_or_default();
        // The reading is only worth a field when it is not the expression repeated.
        let words = if e.secondary == expression { String::new() } else { e.secondary.clone() };
        ("cron", expression, words)
    };
    JobRow { class: owner, method, trigger: trigger.into(), schedule, description, file: e.file.clone(), line: e.line }
}

pub fn rows(entries: &[ExtEntry], filter: &JobFilter) -> Vec<JobRow> {
    let mut rows: Vec<JobRow> = entries
        .iter()
        .map(from_entry)
        .filter(|j| has(&j.class, &filter.name) || has(&j.method, &filter.name))
        .collect();
    rows.sort_by(|a, b| a.class.cmp(&b.class).then_with(|| a.method.cmp(&b.method)));
    rows
}

pub fn collect(root: &str, filter: &JobFilter) -> Vec<JobRow> {
    rows(&crate::frameworks::catalog_of(root, "scheduling.jobs"), filter)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(secondary: &str, tags: &[&str]) -> ExtEntry {
        ExtEntry {
            id: "Nightly#run".into(),
            primary: "Nightly.run".into(),
            secondary: secondary.into(),
            kind: "@Scheduled".into(),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            ..ExtEntry::default()
        }
    }

    #[test]
    fn the_three_triggers_are_told_apart() {
        let cron = from_entry(&entry("every day at 02:00", &["0 0 2 * * *"]));
        assert_eq!((cron.trigger.as_str(), cron.schedule.as_str(), cron.description.as_str()), ("cron", "0 0 2 * * *", "every day at 02:00"));
        let fixed = from_entry(&entry("fixedRate = 5000", &["fixed"]));
        assert_eq!((fixed.trigger.as_str(), fixed.schedule.as_str()), ("fixed", "fixedRate = 5000"));
        let none = from_entry(&entry("no trigger", &["incomplete"]));
        assert_eq!(none.trigger, "none");
        assert_eq!((none.class.as_str(), none.method.as_str()), ("Nightly", "run"));
    }

    #[test]
    fn an_unreadable_cron_is_not_repeated_as_its_own_description() {
        let job = from_entry(&entry("0 0 2 * * *", &["0 0 2 * * *"]));
        assert!(job.description.is_empty());
    }

    #[test]
    fn jobs_filter_by_class_or_method() {
        let entries = vec![entry("x", &["* * * * * *"])];
        assert_eq!(rows(&entries, &JobFilter { name: Some("nightly".into()) }).len(), 1);
        assert!(rows(&entries, &JobFilter { name: Some("billing".into()) }).is_empty());
    }
}
