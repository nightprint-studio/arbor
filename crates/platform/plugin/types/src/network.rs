//! Whether a package may reach a host — one definition, for every guest.
//!
//! The rule lived inside `arbor.http`'s Lua binding, which was fine while Lua was the only
//! thing that could make a request. A wasm guest asks the host to perform requests too, and
//! two copies of "is this host allowed" is two places to get a wildcard wrong — in a check
//! whose entire job is to be right.
//!
//! ## The rule
//!
//! An entry in `[permissions] network` matches a host when it is:
//!
//! * `*` — anything, and the manifest is saying so out loud;
//! * the host exactly;
//! * a **parent domain** of it: `amazonaws.com` covers `s3.amazonaws.com`.
//!
//! The parent-domain form is why entries are written bare. `*.example.com` is not a pattern
//! here — it would be read as a literal label and match nothing, which fails *open-looking*
//! (the manifest appears to grant access and does not), so it is worth knowing.
//!
//! An empty list means **no network at all**, not "unrestricted". A package that did not ask
//! does not get it.

/// The host part of a URL, or `None` when there isn't one.
///
/// Hand-rolled rather than pulled from a URL crate: this runs on a permission path, and the
/// only thing that matters is that it agrees with what the request will actually connect to.
/// Splitting on the delimiters that can end an authority is exactly that, and it cannot
/// disagree with itself the way two parsers can.
pub fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    // Strip userinfo — `https://user:pass@host/…`. Without this, `evil.com#@allowed.com`
    // shapes and credential-bearing URLs read as the wrong host.
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let authority = authority.rsplit_once('@').map(|(_, h)| h).unwrap_or(authority);
    let host = authority.split(':').next().unwrap_or("");
    if host.is_empty() {
        None
    } else {
        Some(host.to_ascii_lowercase())
    }
}

/// Whether `allowlist` grants access to `host`.
pub fn host_allowed(allowlist: &[String], host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    allowlist.iter().any(|entry| {
        let e = entry.trim().to_ascii_lowercase();
        e == "*" || e == host || host.ends_with(&format!(".{e}"))
    })
}

/// Why a request was refused, with a message that names the fix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkDenial {
    /// The package declared no `network` permission at all.
    NoPermission { plugin: String },
    /// The URL has no host to check.
    Unparseable { url: String },
    /// The host is not covered by the allowlist.
    NotAllowed { host: String, allowlist: Vec<String> },
}

impl std::fmt::Display for NetworkDenial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetworkDenial::NoPermission { plugin } => write!(
                f,
                "'{plugin}' requires `network` permission. Add to plugin.toml:\n\
                 [permissions]\nnetwork = [\"<host>\"]"
            ),
            NetworkDenial::Unparseable { url } => {
                write!(f, "cannot read a host from URL '{url}'")
            }
            NetworkDenial::NotAllowed { host, allowlist } => write!(
                f,
                "host '{host}' is not in this package's network allowlist {allowlist:?}. \
                 Entries are bare domains — '{host}' or a parent of it, not a '*.' pattern."
            ),
        }
    }
}

impl std::error::Error for NetworkDenial {}

/// The whole gate: does this package get to request this URL?
pub fn check(plugin: &str, allowlist: &[String], url: &str) -> Result<String, NetworkDenial> {
    if allowlist.is_empty() {
        return Err(NetworkDenial::NoPermission { plugin: plugin.to_string() });
    }
    let host = host_of(url).ok_or_else(|| NetworkDenial::Unparseable { url: url.to_string() })?;
    if !host_allowed(allowlist, &host) {
        return Err(NetworkDenial::NotAllowed {
            host,
            allowlist: allowlist.to_vec(),
        });
    }
    Ok(host)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_host_comes_out_of_the_shapes_a_url_actually_takes() {
        assert_eq!(host_of("https://storage.googleapis.com/b/o").as_deref(), Some("storage.googleapis.com"));
        assert_eq!(host_of("http://localhost:8787/mcp").as_deref(), Some("localhost"));
        assert_eq!(host_of("example.com/path").as_deref(), Some("example.com"));
        assert_eq!(host_of("https://EXAMPLE.com").as_deref(), Some("example.com"));
        assert_eq!(host_of("https://x.com?q=1").as_deref(), Some("x.com"));
        assert_eq!(host_of(""), None);
        assert_eq!(host_of("https://"), None);
    }

    #[test]
    fn userinfo_does_not_get_mistaken_for_the_host() {
        // The shape that matters: everything before `@` is credentials, and reading it as the
        // host is how `https://allowed.com@evil.com/` passes a check it should fail.
        assert_eq!(host_of("https://user:pass@evil.com/x").as_deref(), Some("evil.com"));
        assert_eq!(host_of("https://allowed.com@evil.com/x").as_deref(), Some("evil.com"));
    }

    #[test]
    fn a_fragment_cannot_smuggle_a_host_past_the_check() {
        // `#` ends the authority, so what follows is not part of the host.
        assert_eq!(host_of("https://evil.com#@allowed.com").as_deref(), Some("evil.com"));
    }

    #[test]
    fn an_exact_entry_matches_only_that_host() {
        let l = list(&["storage.googleapis.com"]);
        assert!(host_allowed(&l, "storage.googleapis.com"));
        assert!(!host_allowed(&l, "googleapis.com"));
        assert!(!host_allowed(&l, "evil-storage.googleapis.com.attacker.net"));
    }

    #[test]
    fn a_parent_domain_covers_its_subdomains() {
        let l = list(&["amazonaws.com"]);
        assert!(host_allowed(&l, "s3.amazonaws.com"));
        assert!(host_allowed(&l, "bucket.s3.eu-west-1.amazonaws.com"));
        assert!(host_allowed(&l, "amazonaws.com"));
        // And not a host that merely ENDS with the same letters.
        assert!(!host_allowed(&l, "notamazonaws.com"));
    }

    #[test]
    fn a_star_pattern_entry_matches_nothing_and_the_error_says_so() {
        // Worth a test because it fails in the direction that looks safe: the manifest reads
        // as if it granted access, and it did not.
        let l = list(&["*.amazonaws.com"]);
        assert!(!host_allowed(&l, "s3.amazonaws.com"));
        let err = check("p", &l, "https://s3.amazonaws.com/x").unwrap_err().to_string();
        assert!(err.contains("bare domains"), "{err}");
    }

    #[test]
    fn a_bare_star_is_everything() {
        assert!(host_allowed(&list(&["*"]), "anything.at.all"));
    }

    #[test]
    fn an_empty_allowlist_is_no_network_not_all_network() {
        let err = check("p", &[], "https://x.com").unwrap_err();
        assert!(matches!(err, NetworkDenial::NoPermission { .. }));
        assert!(err.to_string().contains("network = "), "the error names the fix");
    }

    #[test]
    fn the_gate_returns_the_host_it_approved() {
        // The caller uses it for logging, and returning it means there is no second parse
        // that could disagree with the one the check ran on.
        let l = list(&["amazonaws.com"]);
        assert_eq!(check("p", &l, "https://s3.amazonaws.com/b").unwrap(), "s3.amazonaws.com");
    }
}
