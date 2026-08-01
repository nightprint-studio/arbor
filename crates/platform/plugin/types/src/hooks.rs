//! `[hooks]` section of `plugin.toml`: the per-hook opt-in the host consults
//! before routing a broadcast to a plugin.
//!
//! ```toml
//! [hooks]
//! "arbor:plugin_load"  = true
//! "corvus:commit"      = true
//! "garrulus:*"         = true   # every vault hook
//! ```
//!
//! ## Why this is a map and not a struct of `bool` fields
//!
//! It used to be 58 named fields plus a `match` with 58 arms, and it had
//! already drifted: eight catalog hooks had no field at all and reached every
//! plugin through the match's `_ => true` fallback, whether or not the plugin
//! asked for them. Adding a hook meant editing three lists in lockstep and
//! nothing failed when you edited two.
//!
//! Keying on the name instead makes [`crate::hook_catalog`] the single
//! authority: a name the catalog knows requires an explicit opt-in, and a name
//! it does not know is routed unconditionally, because that is precisely the
//! set of names the manifest cannot enumerate ahead of time — plugin-defined
//! events, command / timer / job callbacks, scheduler-fired actions.
//!
//! Note this is only the *broadcast* gate. Targeted delivery (`fire_on`: view
//! hooks, job results, per-plugin pipeline requests) never passes through here
//! — the payload is already addressed to one plugin.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::hook_catalog;

/// The declared hook subscriptions of one plugin.
///
/// Keys are fully-qualified hook names (`"corvus:commit"`), or a namespace
/// wildcard (`"corvus:*"`). Values are `bool` so a wildcard can be narrowed by
/// an explicit `false` on a single hook.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Hooks {
    declared: BTreeMap<String, bool>,
}

impl Hooks {
    /// Build from an explicit list — for tests and for the manifest scaffolder.
    pub fn from_declared<I, S>(entries: I) -> Self
    where
        I: IntoIterator<Item = (S, bool)>,
        S: Into<String>,
    {
        Self {
            declared: entries.into_iter().map(|(name, on)| (name.into(), on)).collect(),
        }
    }

    /// True when this plugin should receive a broadcast of `hook`.
    ///
    /// Resolution order, most specific first:
    ///
    /// 1. an exact key — an explicit `false` here beats any wildcard;
    /// 2. the longest matching trailing-`*` key;
    /// 3. otherwise: routed only if the name is *not* a built-in, i.e. it is a
    ///    plugin event or a callback the manifest could never have listed.
    ///
    /// The default in (3) is deliberately the opposite for the two cases. A
    /// built-in the plugin never asked for is noise it has to filter itself; a
    /// callback it did ask for by registering it at runtime must not be
    /// silently dropped for want of a manifest line.
    pub fn subscribes_to(&self, hook: &str) -> bool {
        if let Some(&declared) = self.declared.get(hook) {
            return declared;
        }
        if let Some(declared) = self.wildcard_match(hook) {
            return declared;
        }
        hook_catalog::find(hook).is_none()
    }

    /// The value of the most specific trailing-`*` key covering `hook`.
    ///
    /// Longest prefix wins so `"corvus:workspace_*" = false` can carve a hole
    /// out of `"corvus:*" = true`. Only a trailing `*` is honoured — the
    /// manifest is a subscription list, not a pattern language, and the full
    /// glob matcher belongs to the runtime's dispatch path.
    fn wildcard_match(&self, hook: &str) -> Option<bool> {
        self.declared
            .iter()
            .filter_map(|(key, &on)| {
                let prefix = key.strip_suffix('*')?;
                hook.starts_with(prefix).then_some((prefix.len(), on))
            })
            .max_by_key(|(len, _)| *len)
            .map(|(_, on)| on)
    }

    /// Every declared entry, in name order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, bool)> + '_ {
        self.declared.iter().map(|(name, &on)| (name.as_str(), on))
    }

    /// The names this plugin opted into, in name order. What the Plugin Manager
    /// shows as the plugin's hook badges.
    pub fn enabled_names(&self) -> impl Iterator<Item = &str> + '_ {
        self.iter().filter(|(_, on)| *on).map(|(name, _)| name)
    }

    /// True when the manifest declared no `[hooks]` section at all.
    pub fn is_empty(&self) -> bool {
        self.declared.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hook_names::{arbor, corvus, garrulus};

    fn hooks(entries: &[(&str, bool)]) -> Hooks {
        Hooks::from_declared(entries.iter().map(|(n, on)| (*n, *on)))
    }

    #[test]
    fn an_undeclared_builtin_is_not_routed() {
        let h = hooks(&[(corvus::COMMIT, true)]);
        assert!(h.subscribes_to(corvus::COMMIT));
        assert!(!h.subscribes_to(corvus::PUSH));
    }

    /// The half of the old `_ => true` fallback that was load-bearing: names the
    /// manifest cannot know about must still arrive.
    #[test]
    fn a_name_outside_the_catalog_is_always_routed() {
        let h = Hooks::default();
        assert!(h.subscribes_to("my-plugin:build_done"));
        assert!(h.subscribes_to("__timer_7"));
    }

    #[test]
    fn an_empty_section_routes_no_builtin() {
        let h = Hooks::default();
        assert!(h.is_empty());
        assert!(!h.subscribes_to(arbor::PLUGIN_LOAD));
    }

    #[test]
    fn a_namespace_wildcard_covers_the_whole_namespace() {
        let h = hooks(&[("garrulus:*", true)]);
        assert!(h.subscribes_to(garrulus::NOTE_SAVED));
        assert!(h.subscribes_to(garrulus::SYNC_DONE));
        assert!(!h.subscribes_to(corvus::COMMIT));
    }

    #[test]
    fn an_exact_false_beats_a_wildcard_true() {
        let h = hooks(&[("garrulus:*", true), (garrulus::SYNC_CONFLICT, false)]);
        assert!(h.subscribes_to(garrulus::NOTE_SAVED));
        assert!(!h.subscribes_to(garrulus::SYNC_CONFLICT));
    }

    #[test]
    fn the_longest_wildcard_wins() {
        let h = hooks(&[("corvus:*", true), ("corvus:workspace_*", false)]);
        assert!(h.subscribes_to(corvus::COMMIT));
        assert!(!h.subscribes_to(corvus::WORKSPACE_CREATED));
    }

    #[test]
    fn a_bare_star_covers_everything_declared_true() {
        let h = hooks(&[("*", true)]);
        assert!(h.subscribes_to(corvus::COMMIT));
        assert!(h.subscribes_to(garrulus::NOTE_SAVED));
    }

    #[test]
    fn enabled_names_skips_the_disabled_entries() {
        let h = hooks(&[(corvus::COMMIT, true), (corvus::PUSH, false)]);
        let names: Vec<&str> = h.enabled_names().collect();
        assert_eq!(names, vec![corvus::COMMIT]);
    }

    #[test]
    fn round_trips_through_toml() {
        let parsed: Hooks = toml::from_str(
            r#"
            "arbor:plugin_load" = true
            "corvus:commit"     = true
            "corvus:*"          = false
            "#,
        )
        .expect("hooks section parses");
        assert!(parsed.subscribes_to(arbor::PLUGIN_LOAD));
        assert!(parsed.subscribes_to(corvus::COMMIT));
        assert!(!parsed.subscribes_to(corvus::PUSH));
    }
}
