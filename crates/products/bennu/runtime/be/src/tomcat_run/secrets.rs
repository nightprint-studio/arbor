//! A data source's password, kept **outside the repository**.
//!
//! A run configuration lives in `<repo>/.arbor/bennu/config.toml`, which people commit and share;
//! the password for a development database is not theirs to share. In `secret` mode it is stored
//! here instead — one small file in the user's Bennu profile, keyed by project, configuration and
//! resource — and reaches Tomcat as a system property when it starts (see `bennu_tomcat::context`).
//!
//! This is a plain file in the user's own profile directory, not the operating system's keyring:
//! the backend has no credential store of its own and reaching the host's would be a new seam. It
//! keeps the password out of anything that is versioned, which is what the mode is for; it does not
//! protect it from somebody who can read the user's profile, and the UI says so.

use std::collections::BTreeMap;
use std::path::PathBuf;

fn path() -> PathBuf {
    bennu_core::config::config_path().with_file_name("tomcat-secrets.toml")
}

fn read() -> BTreeMap<String, String> {
    std::fs::read_to_string(path()).ok().and_then(|t| toml::from_str(&t).ok()).unwrap_or_default()
}

fn key(root: &str, config_id: &str, resource_id: &str) -> String {
    format!("{}|{config_id}|{resource_id}", root.replace('\\', "/").trim_end_matches('/').to_ascii_lowercase())
}

/// The stored password, if there is one.
pub(super) fn get(root: &str, config_id: &str, resource_id: &str) -> Option<String> {
    read().remove(&key(root, config_id, resource_id))
}

/// Store (or, with an empty value, forget) a password.
pub(super) fn set(root: &str, config_id: &str, resource_id: &str, value: &str) -> Result<(), String> {
    let mut all = read();
    let k = key(root, config_id, resource_id);
    if value.is_empty() {
        all.remove(&k);
    } else {
        all.insert(k, value.to_string());
    }
    let file = path();
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("could not create {}: {e}", parent.display()))?;
    }
    let text = toml::to_string(&all).map_err(|e| e.to_string())?;
    std::fs::write(&file, text).map_err(|e| format!("could not write {}: {e}", file.display()))
}

/// The ids of the resources of one configuration that have a stored password — which is all the UI
/// needs to know: it shows "saved", never the value.
pub(super) fn stored_ids(root: &str, config_id: &str) -> Vec<String> {
    let prefix = key(root, config_id, "");
    read().keys().filter_map(|k| k.strip_prefix(&prefix)).map(str::to_string).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_ignores_how_the_root_is_spelled() {
        assert_eq!(key("C:\\Dev\\App\\", "c1", "r1"), key("c:/dev/app", "c1", "r1"));
        assert_ne!(key("/a", "c1", "r1"), key("/a", "c1", "r2"));
    }

    #[test]
    fn the_prefix_of_a_configuration_selects_exactly_its_resources() {
        let prefix = key("/a", "c1", "");
        assert!(key("/a", "c1", "r9").starts_with(&prefix));
        assert!(!key("/a", "c10", "r9").starts_with(&prefix), "c1 must not claim c10's secrets");
    }
}
