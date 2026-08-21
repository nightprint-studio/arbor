//! How a plugin's credentials are named, and why that naming *is* the enforcement.
//!
//! ## The rule
//!
//! A plugin — Lua or wasm, the rule does not distinguish — may create and read **only the
//! credential slots it declared** in its manifest, and can never reach the ones Arbor keeps
//! for itself: git-provider tokens, refresh tokens, issue-tracker keys, the MCP token.
//!
//! ## Why a namespace and not a filter
//!
//! A filter is a list of things to say no to, and a list has gaps: somebody adds a new kind of
//! Arbor credential and forgets to add it to the denylist, and a plugin can read it. A
//! namespace has no gap to find, because there is no way to *spell* a name outside it. Every
//! request a plugin can make resolves to [`PLUGIN_PREFIX`]`/<plugin>/<key>`, and Arbor's own
//! accounts are not of that shape — so they are not hidden from a plugin, they are
//! **unnameable** by one.
//!
//! That is why [`account_for`] takes the plugin name as a parameter rather than accepting a
//! pre-built account string: a function that accepted "the account to read" would be a
//! function a caller could pass anything to, and the guarantee would move from the type
//! system into a review checklist.
//!
//! ## Why the key is validated
//!
//! The account is a path-shaped string, so a key containing `/` could climb out of its own
//! namespace and a key containing `..` could be mistaken for one by anything that later
//! treats these as paths. The validation is small and total: it is the whole reason the
//! namespace holds.

use std::fmt;

/// Prefix that marks an account as belonging to a plugin rather than to Arbor.
///
/// Arbor's own accounts are hosts and service keys (`github.com/arbor`, `jira/…`), so no
/// existing account can collide with this shape. Changing it would orphan every plugin's
/// stored credentials, which is a migration and not an edit.
pub const PLUGIN_PREFIX: &str = "plugin";

/// Why a credential request was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialError {
    /// The key is not one the manifest declared.
    Undeclared { plugin: String, key: String },
    /// The key cannot be part of an account name.
    InvalidKey { key: String, why: &'static str },
}

impl fmt::Display for CredentialError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // The message names the fix, because the plugin author is the only one who can
            // apply it and they are reading this in a log.
            CredentialError::Undeclared { plugin, key } => write!(
                f,
                "plugin '{plugin}' has no credential slot '{key}'. Declare it in plugin.toml:\n\
                 [[credentials]]\nkey = \"{key}\"\nlabel = \"…\""
            ),
            CredentialError::InvalidKey { key, why } => {
                write!(f, "credential key '{key}' is not usable: {why}")
            }
        }
    }
}

impl std::error::Error for CredentialError {}

/// Whether `key` may appear in an account name.
///
/// Deliberately narrow — lowercase-ish identifiers with dashes and underscores. A permissive
/// rule here is a rule somebody has to reason about later, and there is no credential worth
/// naming that this refuses.
fn validate_key(key: &str) -> Result<(), CredentialError> {
    let bad = |why| Err(CredentialError::InvalidKey { key: key.to_string(), why });
    if key.is_empty() {
        return bad("it is empty");
    }
    if key.len() > 64 {
        return bad("it is longer than 64 characters");
    }
    if !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.') {
        return bad("only letters, digits, '-', '_' and '.' are allowed");
    }
    // `.` is allowed because a key like `oauth.refresh` reads well, but a key that is only
    // dots is the `..` that path handling elsewhere would misread.
    if key.chars().all(|c| c == '.') {
        return bad("it is only dots");
    }
    Ok(())
}

/// The account name a plugin's credential is stored under — shape only.
///
/// Used by the **storage** side, which knows the plugin and the key but not the manifest.
/// It cannot produce a name outside the namespace, which is the guarantee that matters there;
/// whether the plugin was *allowed* to ask is a different question, answered by
/// [`account_for`] at the API gate before the storage side is ever reached.
pub fn account(plugin: &str, key: &str) -> Result<String, CredentialError> {
    validate_key(key)?;
    Ok(format!("{PLUGIN_PREFIX}/{plugin}/{key}"))
}

/// The account name, refusing a key the manifest did not declare.
///
/// Used by the **API gate**, which has the slot list. The check and the name come out of the
/// same call on purpose: there is then no order in which a caller can perform the write
/// without having performed the check.
pub fn account_for(
    plugin:   &str,
    key:      &str,
    declared: &[String],
) -> Result<String, CredentialError> {
    validate_key(key)?;
    if !declared.iter().any(|d| d == key) {
        return Err(CredentialError::Undeclared {
            plugin: plugin.to_string(),
            key:    key.to_string(),
        });
    }
    account(plugin, key)
}

/// Whether an account belongs to **this** plugin. Used when uninstalling one.
pub fn belongs_to(account: &str, plugin: &str) -> bool {
    account.starts_with(&format!("{PLUGIN_PREFIX}/{plugin}/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declared() -> Vec<String> {
        vec!["oauth".to_string(), "hmac".to_string()]
    }

    #[test]
    fn a_declared_key_resolves_inside_the_plugins_namespace() {
        assert_eq!(
            account_for("cloud-gcs", "oauth", &declared()).unwrap(),
            "plugin/cloud-gcs/oauth"
        );
    }

    #[test]
    fn an_undeclared_key_is_refused_and_the_error_says_how_to_declare_it() {
        let err = account_for("cloud-gcs", "sneaky", &declared()).unwrap_err();
        assert!(matches!(err, CredentialError::Undeclared { .. }));
        let msg = err.to_string();
        assert!(msg.contains("[[credentials]]"), "{msg}");
        assert!(msg.contains("key = \"sneaky\""), "{msg}");
    }

    #[test]
    fn a_plugin_that_declared_nothing_reaches_nothing() {
        assert!(account_for("x", "oauth", &[]).is_err());
    }

    #[test]
    fn a_key_cannot_climb_out_of_the_namespace() {
        // The attack this closes: `../../github.com/arbor` would resolve to one of Arbor's
        // own accounts if the key were pasted into the name unchecked.
        let sneaky = vec!["../../github.com/arbor".to_string()];
        assert!(account_for("x", "../../github.com/arbor", &sneaky).is_err());
        // Declaring it does not help — the key is refused on its shape, before the
        // declaration is even consulted.
        let dots = vec!["..".to_string()];
        assert!(account_for("x", "..", &dots).is_err());
        let slash = vec!["a/b".to_string()];
        assert!(account_for("x", "a/b", &slash).is_err());
    }

    #[test]
    fn arbors_own_accounts_are_not_plugin_accounts() {
        // Not a filter — the point is that these shapes simply are not what `account_for`
        // can produce.
        assert!(!belongs_to("github.com/arbor", "cloud-gcs"));
        assert!(!belongs_to("github.com/arbor-refresh", "cloud-gcs"));
        assert!(!belongs_to("credentials", "cloud-gcs"));
        assert!(belongs_to("plugin/cloud-gcs/oauth", "cloud-gcs"));
    }

    #[test]
    fn ownership_is_per_plugin_not_just_per_prefix() {
        // Uninstalling one plugin must not take another's secrets with it.
        assert!(belongs_to("plugin/cloud-gcs/oauth", "cloud-gcs"));
        assert!(!belongs_to("plugin/cloud-s3/hmac", "cloud-gcs"));
        // And a prefix that merely starts the same is a different plugin.
        assert!(!belongs_to("plugin/cloud-gcs-extra/oauth", "cloud-gcs"));
    }

    #[test]
    fn the_storage_side_still_cannot_leave_the_namespace() {
        // It does not consult the manifest — that check already ran — but the shape rule is
        // what keeps `..` from resolving to one of Arbor's accounts, so it runs here too.
        assert_eq!(account("x", "oauth").unwrap(), "plugin/x/oauth");
        assert!(account("x", "../../github.com/arbor").is_err());
        assert!(account("x", "").is_err());
    }

    #[test]
    fn a_key_that_reads_well_is_allowed() {
        let d = vec!["oauth.refresh".to_string(), "api_key-2".to_string()];
        assert!(account_for("x", "oauth.refresh", &d).is_ok());
        assert!(account_for("x", "api_key-2", &d).is_ok());
    }

    #[test]
    fn an_empty_or_overlong_key_is_refused() {
        assert!(account_for("x", "", &["".to_string()]).is_err());
        let long = "a".repeat(65);
        assert!(account_for("x", &long, &[long.clone()]).is_err());
    }
}
