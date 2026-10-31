//! What the user told Bennu to stop using of what it learned.
//!
//! What a project taught Bennu is counted from its files and is gone and re-counted at the next scan, so "forgetting" one
//! habit by deleting its count would last until the next time a file changed. A rule says it the other way round — *do not
//! offer this* — and is consulted wherever a learned thing is about to be used. Stored beside the profile, a short list of
//! `(kind, key, text)`, never sent anywhere; restoring one is deleting its line.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};

use serde::{Deserialize, Serialize};

/// One thing not to offer: `kind` says which table (`habit`, `name`, `sequence`, `member_args`), `key` where in it
/// (`filter#0`), `text` what.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub kind: String,
    pub key: String,
    pub text: String,
}

fn path() -> PathBuf {
    arbor_core::prelude::bennu_config_path("learned-hidden.json")
}

fn held() -> MutexGuard<'static, Vec<Rule>> {
    static HELD: OnceLock<Mutex<Vec<Rule>>> = OnceLock::new();
    HELD.get_or_init(|| Mutex::new(std::fs::read_to_string(path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()))
        .lock()
        .unwrap_or_else(|p| p.into_inner())
}

/// Whether the user asked for `text` at `key` of table `kind` not to be offered — by naming it, or by hiding the whole
/// place (a rule whose `text` is empty).
pub(crate) fn hidden(kind: &str, key: &str, text: &str) -> bool {
    held().iter().any(|r| r.kind == kind && r.key == key && (r.text.is_empty() || r.text == text))
}

/// Whether a whole place is hidden, as opposed to some of its rows.
pub(crate) fn place_hidden(kind: &str, key: &str) -> bool {
    held().iter().any(|r| r.kind == kind && r.key == key && r.text.is_empty())
}

/// What the rules take, for the process monitor.
pub(crate) fn memory_item() -> arbor_be::prelude::MemoryItem {
    use crate::flc_memory::text;
    let rules = held();
    let bytes = rules.iter().map(|r| text(&r.kind) + text(&r.key) + text(&r.text)).sum();
    arbor_be::prelude::MemoryItem::estimate("process", "Learned · rows you stopped offering (rules)", rules.len(), bytes)
}

pub(crate) fn all() -> Vec<Rule> {
    held().clone()
}

fn save(rules: &[Rule]) {
    if let Some(dir) = path().parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // a list that cannot be written is a rule that lasts until the next start, not a failure of the editor
    if let Ok(text) = serde_json::to_string_pretty(rules) {
        let _ = std::fs::write(path(), text);
    }
}

pub(crate) fn hide(rule: Rule) {
    let mut rules = held();
    if !rules.contains(&rule) {
        rules.push(rule);
        save(&rules);
    }
}

pub(crate) fn restore(rule: &Rule) {
    let mut rules = held();
    let before = rules.len();
    rules.retain(|r| r != rule);
    if rules.len() != before {
        save(&rules);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rule_is_matched_by_all_three_parts() {
        let rule = Rule { kind: "habit".into(), key: "zz_test#0".into(), text: "Foo.BAR".into() };
        hide(rule.clone());
        assert!(hidden("habit", "zz_test#0", "Foo.BAR"));
        assert!(!hidden("habit", "zz_test#1", "Foo.BAR"));
        assert!(!hidden("name", "zz_test#0", "Foo.BAR"));
        restore(&rule);
        assert!(!hidden("habit", "zz_test#0", "Foo.BAR"));
    }
}
