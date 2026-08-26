//! What the project declared — the `[naming]` section of `<repo>/.arbor/bennu/config.toml`.
//!
//! ```toml
//! [naming]
//! enabled = false
//! ignore  = ["**/generated/**", "**/*Stub.java"]
//!
//! [naming.rules.java]
//! type     = "PascalCase"
//! method   = "camelCase"
//! constant = "UPPER_SNAKE_CASE"
//!
//! # A subtree that plays by different rules.
//! [[naming.overrides]]
//! name  = "tests"
//! paths = ["**/src/test/**"]
//!
//! [naming.overrides.rules.java]
//! method = "any"
//! ```
//!
//! ## Overrides, because a project does not have one convention
//!
//! Test sources are the standing example: `test00_invalid_ragioneSociale` mixes camelCase and
//! snake_case deliberately, and judging it by the main rule is not a finding. `ignore` would
//! silence the whole tree — including the test names that really are wrong — so an override
//! replaces only the targets it names and leaves the rest of the rules in force.
//!
//! ## Everything defaults to off, and that is not timidity
//!
//! `enabled` is `false` and every unset target is [`Convention::Any`]. Switching a naming rule on
//! by default would greet a legacy project with some thousands of weak warnings on the first open
//! — which is not a code-quality signal, it is a reason to turn the whole feature off and never
//! look at it again. The project opts in, per target, and [`LanguageRules::is_off`] means the scan
//! does not even parse.
//!
//! ## Rules are keyed by pack id, not by a field per language
//!
//! `[naming.rules.<pack>]` is a map because a pack is meant to be addable without touching this
//! struct — the same reason [`crate::pack`] is a registry rather than a match. A pack that is not
//! installed leaves its section sitting harmlessly in the file.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::convention::Convention;
use crate::target::Target;

/// The `[naming]` section.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NamingConfig {
    /// Master switch. Off by default — see the module doc.
    pub enabled: bool,
    /// Path globs (`*`, `?`, `**`) matched against the project-relative path, forward-slashed.
    /// A file that matches is skipped entirely.
    pub ignore: Vec<String>,
    /// Per-pack rules, keyed by [`crate::pack::Pack::id`].
    pub rules: BTreeMap<String, LanguageRules>,
    /// Rule sets that replace some of the above for the paths they match, in declaration order —
    /// a later match wins. See [`NamingOverride`].
    pub overrides: Vec<NamingOverride>,
}

impl NamingConfig {
    /// The rules for one pack — empty (so: everything off) when the project never configured it.
    pub fn rules_for(&self, pack_id: &str) -> LanguageRules {
        self.rules.get(pack_id).cloned().unwrap_or_default()
    }

    /// The rules that actually apply to `rel` (project-relative, forward-slashed): the pack's
    /// rules with every matching override layered on top, in declaration order.
    ///
    /// This is what the scan asks, and `rules_for` is only its base case. A project does not have
    /// one convention — test sources are the standing example: `test00_invalid_ragioneSociale`
    /// mixes camelCase and snake_case **on purpose**, and judging it by the main rule produced
    /// hundreds of violations that were not violations of anything. `ignore` could silence them,
    /// but silence is not the same answer: it also stops reporting the test names that ARE wrong.
    pub fn rules_for_path(&self, pack_id: &str, rel: &str) -> LanguageRules {
        let mut rules = self.rules_for(pack_id);
        for over in &self.overrides {
            if !over.matches(rel) {
                continue;
            }
            if let Some(extra) = over.rules.get(pack_id) {
                rules.overlay(extra);
            }
        }
        rules
    }

    /// Whether `rel` (a project-relative, forward-slashed path) is excluded by `ignore`.
    pub fn ignores(&self, rel: &str) -> bool {
        self.ignore.iter().any(|pattern| glob_matches(pattern, rel))
    }
}

/// A rule set that applies only to the paths it names.
///
/// ```toml
/// [[naming.overrides]]
/// name  = "tests"
/// paths = ["**/src/test/**"]
///
/// [naming.overrides.rules.java]
/// method = "any"
/// ```
///
/// Only the targets it names are replaced; everything else still comes from `[naming.rules.*]`.
/// That is the whole point of an override rather than a second config: a test tree can free up
/// method names without also giving up the type and constant rules.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NamingOverride {
    /// A label for the settings UI. Free text; it never affects matching.
    pub name: String,
    /// Path globs (`*`, `?`, `**`) matched against the project-relative, forward-slashed path.
    pub paths: Vec<String>,
    /// The per-pack conventions this override replaces, keyed like [`NamingConfig::rules`].
    pub rules: BTreeMap<String, LanguageRules>,
}

impl NamingOverride {
    /// Whether this override claims `rel`. An override with no path claims nothing — an empty
    /// list is an unfinished entry, not a wildcard.
    pub fn matches(&self, rel: &str) -> bool {
        self.paths.iter().any(|pattern| glob_matches(pattern, rel))
    }
}

/// One pack's rules: a convention per target, absent meaning [`Convention::Any`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LanguageRules(pub BTreeMap<Target, Convention>);

impl LanguageRules {
    /// Build from pairs — how a pack states its defaults.
    pub fn from_pairs(pairs: impl IntoIterator<Item = (Target, Convention)>) -> Self {
        LanguageRules(pairs.into_iter().collect())
    }

    /// Replace the targets `other` names, leaving the rest alone — how an override layers onto the
    /// base rules. Setting a target to `any` in an override is meaningful: it turns that one rule
    /// off for the matched paths.
    pub fn overlay(&mut self, other: &LanguageRules) {
        for (target, convention) in &other.0 {
            self.0.insert(*target, *convention);
        }
    }

    /// The convention configured for `target`; [`Convention::Any`] when unset.
    pub fn convention_for(&self, target: Target) -> Convention {
        self.0.get(&target).copied().unwrap_or_default()
    }

    /// Whether every target is off — the fast path that lets the scan skip parsing the file.
    pub fn is_off(&self) -> bool {
        self.0.values().all(|c| c.is_off())
    }
}

/// Match `pattern` against `path`, both forward-slashed.
///
/// A glob, not a regex, and for the same reason the conventions are enumerated: this is a value a
/// user types into a settings field, and the failure mode of a regex there is a pattern that
/// silently matches nothing (or everything) with no way to tell which. `*` is any run within a
/// segment, `**` crosses segments, `?` is one character; everything else is literal, and the match
/// is anchored at both ends.
fn glob_matches(pattern: &str, path: &str) -> bool {
    // `**/x` must also match a bare `x` at the root, which the literal walk below cannot express:
    // it would require the separator that is not there.
    if let Some(rest) = pattern.strip_prefix("**/") {
        if glob_matches(rest, path) {
            return true;
        }
    }
    let p: Vec<char> = pattern.chars().collect();
    let s: Vec<char> = path.chars().collect();
    matches_from(&p, 0, &s, 0)
}

fn matches_from(p: &[char], mut pi: usize, s: &[char], mut si: usize) -> bool {
    while pi < p.len() {
        match p[pi] {
            '*' => {
                let crosses_segments = p.get(pi + 1) == Some(&'*');
                let next = pi + if crosses_segments { 2 } else { 1 };
                // Try every length this wildcard could consume, shortest first.
                let mut end = si;
                loop {
                    if matches_from(p, next, s, end) {
                        return true;
                    }
                    if end >= s.len() || (!crosses_segments && s[end] == '/') {
                        return false;
                    }
                    end += 1;
                }
            }
            '?' => {
                if si >= s.len() || s[si] == '/' {
                    return false;
                }
                pi += 1;
                si += 1;
            }
            c => {
                if si >= s.len() || s[si] != c {
                    return false;
                }
                pi += 1;
                si += 1;
            }
        }
    }
    si == s.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_default_config_checks_nothing() {
        let cfg = NamingConfig::default();
        assert!(!cfg.enabled);
        assert!(cfg.rules_for("java").is_off());
        assert!(!cfg.ignores("src/main/java/Foo.java"));
    }

    #[test]
    fn an_unset_target_is_any() {
        let rules = LanguageRules::from_pairs([(Target::Method, Convention::Camel)]);
        assert_eq!(rules.convention_for(Target::Method), Convention::Camel);
        assert_eq!(rules.convention_for(Target::Field), Convention::Any);
        assert!(!rules.is_off());
    }

    #[test]
    fn globs_anchor_and_respect_segments() {
        assert!(glob_matches("**/generated/**", "target/generated/Foo.java"));
        assert!(glob_matches("**/*Stub.java", "src/a/b/OrderStub.java"));
        assert!(glob_matches("*.java", "Foo.java"));
        // `*` does not cross a separator, so a single-segment pattern cannot match a nested path.
        assert!(!glob_matches("*.java", "src/Foo.java"));
        // Anchored at both ends: a prefix match is not a match.
        assert!(!glob_matches("src/Foo", "src/Foobar"));
        // `**/x` matches `x` at the root too.
        assert!(glob_matches("**/Foo.java", "Foo.java"));
    }

    #[test]
    fn ignore_is_any_of_the_patterns() {
        let cfg = NamingConfig {
            ignore: vec!["**/generated/**".into(), "**/*Test.java".into()],
            ..Default::default()
        };
        assert!(cfg.ignores("build/generated/A.java"));
        assert!(cfg.ignores("src/OrderTest.java"));
        assert!(!cfg.ignores("src/Order.java"));
    }

    #[test]
    fn round_trips_through_toml_shaped_json() {
        // The config is decoded from TOML in the be layer; serde shape is what matters here.
        let cfg = NamingConfig {
            enabled: true,
            ignore: vec!["**/gen/**".into()],
            rules: BTreeMap::from([(
                "java".to_string(),
                LanguageRules::from_pairs([
                    (Target::Method, Convention::Camel),
                    (Target::Constant, Convention::UpperSnake),
                ]),
            )]),
            overrides: vec![NamingOverride {
                name: "tests".into(),
                paths: vec!["**/src/test/**".into()],
                rules: java_rules(&[(Target::Method, Convention::Any)]),
            }],
        };
        let json = serde_json::to_string(&cfg).expect("serialises");
        assert!(json.contains("\"overrides\""), "{json}");
        assert!(json.contains("\"method\":\"camelCase\""), "{json}");
        assert!(json.contains("\"constant\":\"UPPER_SNAKE_CASE\""), "{json}");
        let back: NamingConfig = serde_json::from_str(&json).expect("round-trips");
        assert_eq!(back, cfg);
    }
    fn java_rules(pairs: &[(Target, Convention)]) -> BTreeMap<String, LanguageRules> {
        let mut m = BTreeMap::new();
        m.insert("java".to_string(), LanguageRules::from_pairs(pairs.iter().copied()));
        m
    }

    #[test]
    fn an_override_replaces_only_the_targets_it_names() {
        // Test sources spell method names their own way on purpose; everything else still applies.
        let cfg = NamingConfig {
            enabled: true,
            rules: java_rules(&[(Target::Method, Convention::Camel), (Target::Type, Convention::Pascal)]),
            overrides: vec![NamingOverride {
                name: "tests".into(),
                paths: vec!["**/src/test/**".into()],
                rules: java_rules(&[(Target::Method, Convention::Any)]),
            }],
            ..Default::default()
        };
        let main = cfg.rules_for_path("java", "src/main/java/p/Order.java");
        assert_eq!(main.convention_for(Target::Method), Convention::Camel);

        let test = cfg.rules_for_path("java", "src/test/java/p/OrderTest.java");
        assert_eq!(test.convention_for(Target::Method), Convention::Any, "freed by the override");
        assert_eq!(test.convention_for(Target::Type), Convention::Pascal, "untouched targets stay");
    }

    #[test]
    fn a_later_override_wins_over_an_earlier_one() {
        let cfg = NamingConfig {
            enabled: true,
            rules: java_rules(&[(Target::Method, Convention::Camel)]),
            overrides: vec![
                NamingOverride { name: "a".into(), paths: vec!["src/test/**".into()], rules: java_rules(&[(Target::Method, Convention::Any)]) },
                NamingOverride { name: "b".into(), paths: vec!["src/test/it/**".into()], rules: java_rules(&[(Target::Method, Convention::LowerSnake)]) },
            ],
            ..Default::default()
        };
        assert_eq!(cfg.rules_for_path("java", "src/test/p/A.java").convention_for(Target::Method), Convention::Any);
        assert_eq!(cfg.rules_for_path("java", "src/test/it/B.java").convention_for(Target::Method), Convention::LowerSnake);
    }

    #[test]
    fn an_override_with_no_paths_claims_nothing() {
        let cfg = NamingConfig {
            enabled: true,
            rules: java_rules(&[(Target::Method, Convention::Camel)]),
            overrides: vec![NamingOverride { name: "wip".into(), paths: Vec::new(), rules: java_rules(&[(Target::Method, Convention::Any)]) }],
            ..Default::default()
        };
        assert_eq!(cfg.rules_for_path("java", "anything.java").convention_for(Target::Method), Convention::Camel);
    }
}
