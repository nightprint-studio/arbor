//! Content verification for downloaded artifacts.
//!
//! ## Why this is separate from `pinned_sha`
//!
//! They answer different questions and neither substitutes for the other.
//!
//! * [`crate::github_api::verify_pinned_sha`] asks **"is this still the commit that was
//!   reviewed?"**. It is a defence against a tag being moved after the fact, and for a plugin
//!   installed from a source archive it is a complete answer — a git SHA is content-addressed,
//!   so pinning the commit pins the source.
//! * This module asks **"are these the exact bytes that were approved?"** — which is the only
//!   question available once an artifact is a build output rather than a checkout. A `.wasm`
//!   is not in the repo the commit pins, and nobody can read it if it were.
//!
//! ## Why the hash comes from the registry and not from the package
//!
//! Because a hash the author supplies verifies only that the author is consistent with
//! themselves. The registry entry is what a human reviewed in a PR, so that is where the
//! approved bytes get named: a reviewer still cannot read a `.wasm`, but they can pin which
//! `.wasm` was approved, and any later substitution then fails to install. Every package
//! manager stops at exactly this line.
//!
//! ## Why source archives are not hashed
//!
//! GitHub's `archive/{ref}.zip` is **generated on demand**, and its bytes have changed across
//! GitHub's own tooling upgrades for unchanged repositories. Pinning a hash to it would turn a
//! working install into a mysterious failure on somebody else's schedule. Source archives keep
//! `pinned_sha`, which is stable because git says so; only release assets — files an author
//! uploaded and GitHub stores verbatim — get hashed.

use sha2::{Digest, Sha256};

use crate::error::{MarketplaceError, Result};

/// Lowercase hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// Strip the optional `sha256:` prefix an index entry may spell a digest with.
///
/// Both forms are accepted on the way in because both get written by hand. Only the bare hex
/// is ever compared, so the two cannot disagree.
fn normalise(digest: &str) -> String {
    digest
        .trim()
        .strip_prefix("sha256:")
        .unwrap_or_else(|| digest.trim())
        .to_ascii_lowercase()
}

/// Check `bytes` against the digest the registry recorded for `name`.
///
/// A full 64-character digest is required. Short digests are what `pinned_sha` accepts —
/// there, a prefix of a git SHA is a human-typed convenience over an already content-addressed
/// object. Here the digest **is** the whole guarantee, and a truncated one is a weaker
/// guarantee that looks identical.
pub fn verify(name: &str, bytes: &[u8], expected: &str) -> Result<()> {
    let want = normalise(expected);
    if want.len() != 64 || !want.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(MarketplaceError::IntegrityMismatch(format!(
            "{name}: the registry entry's digest '{expected}' is not a full sha256"
        )));
    }
    let got = sha256_hex(bytes);
    if got != want {
        return Err(MarketplaceError::IntegrityMismatch(format!(
            "{name}: expected sha256 {want}, got {got} — the artifact is not the one the \
             registry approved. Refusing to install."
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// sha256("") — the one digest that can be written down from memory.
    const EMPTY: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    #[test]
    fn a_matching_digest_passes_in_both_spellings() {
        assert!(verify("x.wasm", b"", EMPTY).is_ok());
        assert!(verify("x.wasm", b"", &format!("sha256:{EMPTY}")).is_ok());
        assert!(verify("x.wasm", b"", &EMPTY.to_ascii_uppercase()).is_ok());
    }

    #[test]
    fn one_flipped_byte_fails() {
        assert!(verify("x.wasm", b"\0", EMPTY).is_err());
    }

    #[test]
    fn a_truncated_digest_is_refused_rather_than_prefix_matched() {
        // The failure this prevents: a 8-char digest looks like the `pinned_sha` next to it
        // and would silently accept 2^-32 of all possible artifacts.
        let err = verify("x.wasm", b"", &EMPTY[..8]).unwrap_err().to_string();
        assert!(err.contains("not a full sha256"), "{err}");
    }

    #[test]
    fn a_non_hex_digest_is_refused() {
        assert!(verify("x.wasm", b"", "not-a-digest").is_err());
        assert!(verify("x.wasm", b"", "").is_err());
    }

    #[test]
    fn the_error_says_what_to_conclude() {
        // These strings are what the user sees when an install fails, and "checksum
        // mismatch" is not an instruction.
        let err = verify("cloud_gcs.wasm", b"tampered", EMPTY).unwrap_err().to_string();
        assert!(err.contains("cloud_gcs.wasm"), "{err}");
        assert!(err.contains("Refusing to install"), "{err}");
    }
}
