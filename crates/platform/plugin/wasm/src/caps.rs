//! What a guest is allowed to reach — the gates every host call goes through.
//!
//! A component instantiated by this crate has **no ambient capability**. It cannot open a
//! socket, cannot touch the filesystem, and has no WASI worlds beyond what is imported here.
//! Everything it can do arrives as a function the host implements, and every one of those
//! functions starts by asking this type.
//!
//! ## Why the gate is a value and not a check scattered at each call site
//!
//! Because the two rules it holds are the ones the user consented to at install time, and a
//! rule enforced in three places is a rule with three chances to be forgotten. A
//! [`GuestCaps`] is built once from the manifest when the guest is instantiated, and the
//! guest's identity is in it — so there is no host function that *could* be written to
//! resolve a credential for a different package: the name is not a parameter.
//!
//! ## Both rules delegate
//!
//! Neither the network rule nor the credential namespace is defined here. They live in
//! `arbor_plugin_types` and are shared with the Lua host, because "may this package reach
//! this host" must not mean two different things depending on which language the package
//! was written in.

use arbor_plugin_types::prelude::{
    credential_account_for, network_check, CredentialError, Manifest, NetworkDenial,
};

/// The capability envelope of one instantiated guest.
///
/// A snapshot taken when the guest is created, exactly like the Lua host's permission
/// snapshot: editing `plugin.toml` under a running guest cannot widen what it reaches.
#[derive(Debug, Clone)]
pub struct GuestCaps {
    /// The package's name. Its identity for credentials and for logging — never a parameter
    /// of a host call, so a guest cannot ask about another package by naming it.
    plugin: String,
    /// `[permissions] network`.
    network: Vec<String>,
    /// Keys from `[[credentials]]`.
    slots: Vec<String>,
}

impl GuestCaps {
    pub fn from_manifest(m: &Manifest) -> Self {
        Self {
            plugin:  m.name.clone(),
            network: m.permissions.network.clone(),
            slots:   m.credentials.iter().map(|c| c.key.clone()).collect(),
        }
    }

    /// Build one directly. For tests and for hosts that assemble the envelope themselves.
    pub fn new(plugin: impl Into<String>, network: Vec<String>, slots: Vec<String>) -> Self {
        Self { plugin: plugin.into(), network, slots }
    }

    pub fn plugin(&self) -> &str {
        &self.plugin
    }

    /// May this guest request this URL? Returns the host it approved.
    ///
    /// Returning the host rather than `()` so the caller logs the same string the check ran
    /// on — a second parse to produce a log line is a second parse that can disagree.
    pub fn allow_url(&self, url: &str) -> Result<String, NetworkDenial> {
        network_check(&self.plugin, &self.network, url)
    }

    /// May this guest touch this credential slot? Returns the account name to store under.
    ///
    /// The check and the name come out of one call, so there is no order in which a host
    /// function could perform the store operation without having performed the check.
    pub fn credential_account(&self, key: &str) -> Result<String, CredentialError> {
        credential_account_for(&self.plugin, key, &self.slots)
    }

    /// The slots this package declared.
    ///
    /// For an embedder deciding whether the package has been set up at all — it declares what
    /// it needs, so the host never has to know that a GCS provider calls its token `oauth`.
    pub fn slots(&self) -> &[String] {
        &self.slots
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps() -> GuestCaps {
        GuestCaps::new(
            "cloud-gcs",
            vec!["storage.googleapis.com".to_string()],
            vec!["oauth".to_string()],
        )
    }

    #[test]
    fn an_allowed_host_passes_and_comes_back_named() {
        assert_eq!(
            caps().allow_url("https://storage.googleapis.com/b/o").unwrap(),
            "storage.googleapis.com"
        );
    }

    #[test]
    fn a_host_outside_the_allowlist_is_refused() {
        assert!(caps().allow_url("https://evil.com/x").is_err());
        // Including one that only looks like it belongs.
        assert!(caps().allow_url("https://storage.googleapis.com.evil.com/x").is_err());
    }

    #[test]
    fn a_declared_slot_resolves_inside_the_packages_namespace() {
        assert_eq!(caps().credential_account("oauth").unwrap(), "plugin/cloud-gcs/oauth");
    }

    #[test]
    fn an_undeclared_slot_is_refused() {
        assert!(caps().credential_account("sneaky").is_err());
    }

    #[test]
    fn the_guests_identity_is_not_something_it_can_pass_in() {
        // The property that makes the namespace hold at this layer: every account this type
        // can produce carries the plugin it was built for, and a host function has no
        // parameter with which to name a different one.
        let c = caps();
        assert!(c.credential_account("oauth").unwrap().starts_with("plugin/cloud-gcs/"));
        // Even a key shaped like a path cannot climb out — the shape rule runs first.
        assert!(c.credential_account("../other/oauth").is_err());
    }

    #[test]
    fn a_package_that_declared_no_network_reaches_nothing() {
        let c = GuestCaps::new("x", vec![], vec![]);
        assert!(c.allow_url("https://anything.com").is_err());
    }
}
