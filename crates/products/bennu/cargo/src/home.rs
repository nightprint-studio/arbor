//! Where cargo keeps things on this machine.
//!
//! One resolver, because two consumers need the same answer about the same directory: completion
//! reads the registry **cache** for the crate names and versions it offers, and the dependency
//! report reads the registry **src** to say whether a locked crate is actually unpacked here. Both
//! start from `$CARGO_HOME`, and a second copy of that lookup is a second place to get the
//! environment-variable fallback wrong.

use std::path::PathBuf;

/// `$CARGO_HOME`, or `~/.cargo` when it is unset.
///
/// `None` only when there is no home directory either — a state a service account can genuinely be
/// in, and one where the honest answer is "no registry" rather than a guess at `/root/.cargo`.
pub fn cargo_home() -> Option<PathBuf> {
    match std::env::var_os("CARGO_HOME") {
        Some(h) if !h.is_empty() => Some(PathBuf::from(h)),
        // A windowed app inherits very little environment, so `CARGO_HOME` being absent is the
        // ordinary case rather than the exception.
        _ => arbor_core::prelude::user_home().map(|h| h.join(".cargo")),
    }
}

/// The immediate subdirectories of `$CARGO_HOME/registry/<kind>` — one per configured registry.
///
/// Empty when there is no cargo home, when the directory does not exist, or when it cannot be read.
/// Every caller treats an empty list as "nothing known", which is the correct reading of all three.
pub fn registry_dirs(kind: &str) -> Vec<PathBuf> {
    let Some(home) = cargo_home() else { return Vec::new() };
    let base = home.join("registry").join(kind);
    std::fs::read_dir(&base)
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_registry_directory_is_nothing_known_rather_than_an_error() {
        // Whatever this machine has, the answer for a kind that does not exist is an empty list.
        assert!(registry_dirs("not-a-registry-kind").is_empty());
    }

    /// The fallback is the point of the function: a windowed app inherits almost no environment, so
    /// `CARGO_HOME` being unset is the ordinary case.
    #[test]
    fn the_home_falls_back_to_the_conventional_path() {
        if std::env::var_os("CARGO_HOME").is_none() {
            let home = cargo_home();
            assert_eq!(home.is_some(), arbor_core::prelude::user_home().is_some());
            if let Some(h) = home {
                assert!(h.ends_with(".cargo"), "{h:?}");
            }
        }
    }
}
