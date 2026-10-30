//! What the profiler remembers about *this project*: which classes to look at, and what counts as
//! "high".
//!
//! Both live in `[profiler]` of `<repo>/.arbor/bennu/config.toml` (see [`crate::repo_config`]) —
//! per repo because they describe the project: a package list names its code, and the thresholds
//! are the team's idea of a hot method in it, so they travel with the checkout.
//!
//! The backend only stores them. Applying a filter or colouring a row is the panel's job, over the
//! rows it already has; keeping that out of the wire means flipping a filter never costs a request.

use bennu_core::prelude::BennuState;
use serde::{Deserialize, Serialize};

use super::RootArgs;

const SECTION: &str = "profiler";

/// Which classes the views keep.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProfilerFilter {
    /// Keep only the project's own classes — the ones its index knows — and drop the JDK and the
    /// libraries.
    pub project_only: bool,
    /// Package prefixes to keep. Empty means no restriction by package.
    pub include: Vec<String>,
    /// Package prefixes to drop, whatever else matched.
    pub exclude: Vec<String>,
}

/// The values at which a reading turns yellow, orange and red.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Levels {
    pub yellow: f64,
    pub orange: f64,
    pub red: f64,
}

/// One scale per kind of reading. Their units differ, so they cannot share one.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Thresholds {
    /// A method's own time as a percentage of the recording.
    pub cpu_self_pct: Levels,
    /// Garbage-collection pauses as a percentage of the recording's length.
    pub gc_pause_pct: Levels,
    /// A type's or site's share of everything allocated, in percent.
    pub alloc_share_pct: Levels,
    /// Total time blocked on one monitor, in milliseconds.
    pub lock_total_ms: Levels,
}

impl Default for Thresholds {
    fn default() -> Self {
        Thresholds {
            cpu_self_pct: Levels { yellow: 5.0, orange: 15.0, red: 30.0 },
            gc_pause_pct: Levels { yellow: 2.0, orange: 5.0, red: 10.0 },
            alloc_share_pct: Levels { yellow: 10.0, orange: 25.0, red: 50.0 },
            lock_total_ms: Levels { yellow: 50.0, orange: 250.0, red: 1000.0 },
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProfilerPrefs {
    pub filter: ProfilerFilter,
    pub thresholds: Thresholds,
}

#[arbor_rpc::handler]
fn bennu_profile_prefs_get(_ctx: &BennuState, args: RootArgs) -> Result<ProfilerPrefs, String> {
    Ok(crate::repo_config::load(&args.root, SECTION))
}

#[derive(Deserialize)]
pub struct PrefsArgs {
    pub root: String,
    pub prefs: ProfilerPrefs,
}

#[arbor_rpc::handler]
fn bennu_profile_prefs_set(_ctx: &BennuState, args: PrefsArgs) -> Result<(), String> {
    crate::repo_config::save(&args.root, SECTION, &args.prefs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_section_reads_as_the_defaults() {
        let prefs: ProfilerPrefs = toml::from_str("").unwrap();
        assert_eq!(prefs, ProfilerPrefs::default());
        assert!(prefs.thresholds.cpu_self_pct.yellow < prefs.thresholds.cpu_self_pct.red);
    }

    #[test]
    fn a_partial_section_keeps_the_defaults_for_what_it_omits() {
        let prefs: ProfilerPrefs = toml::from_str("[filter]\nproject_only = true\n").unwrap();
        assert!(prefs.filter.project_only);
        assert_eq!(prefs.thresholds, Thresholds::default());
    }

    #[test]
    fn prefs_round_trip_through_toml() {
        let mut prefs = ProfilerPrefs::default();
        prefs.filter.include = vec!["it.acme".into()];
        prefs.filter.exclude = vec!["it.acme.generated".into()];
        prefs.thresholds.gc_pause_pct.red = 12.5;
        let text = toml::to_string_pretty(&prefs).unwrap();
        assert_eq!(toml::from_str::<ProfilerPrefs>(&text).unwrap(), prefs);
    }
}
