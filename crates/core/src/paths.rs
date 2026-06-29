//! On-disk locations Arbor owns.
//!
//! All Arbor state lives under the OS-conventional config / data / cache
//! roots, namespaced with the literal `"arbor"` segment. The fallback to
//! `"."` matches the long-standing behavior of the previous ad-hoc
//! `dirs::config_dir().unwrap_or_else(|| PathBuf::from("."))` callers — it
//! keeps the app usable on locked-down systems where `dirs` returns `None`,
//! at the cost of writing under the current working directory.

use std::path::{Path, PathBuf};

/// `~/.config/arbor` on Linux, `%APPDATA%\arbor` on Windows,
/// `~/Library/Application Support/arbor` on macOS.
///
/// Falls back to `./arbor` when `dirs::config_dir()` is unavailable.
pub fn arbor_config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("arbor")
}

/// Convenience: join a relative path under [`arbor_config_dir`].
pub fn arbor_config_path<P: AsRef<Path>>(sub: P) -> PathBuf {
    arbor_config_dir().join(sub)
}

/// Like [`arbor_config_path`] but propagates `None` when `dirs::config_dir()`
/// is unavailable instead of falling back to `"."`. Use for state callers
/// that prefer to silently skip persistence over writing under the current
/// working directory.
pub fn try_arbor_config_path<P: AsRef<Path>>(sub: P) -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("arbor").join(sub))
}

/// `~/.local/share/arbor` on Linux, `%APPDATA%\arbor` on Windows,
/// `~/Library/Application Support/arbor` on macOS.
///
/// On Windows and macOS this typically resolves to the same root as
/// [`arbor_config_dir`]; the helper exists so callers state intent.
pub fn arbor_data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("arbor")
}

/// `~/.cache/arbor` on Linux, `%LOCALAPPDATA%\arbor` on Windows,
/// `~/Library/Caches/arbor` on macOS.
pub fn arbor_cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("arbor")
}

// ── merula ───────────────────────────────────────────────────────────────────
//
// merula (the live-coding music workspace) is a **product bucket under the
// active profile**, exactly like corvus: `arbor/profiles/<active>/merula/`. It
// used to own a top-level sibling namespace (`%APPDATA%\merula`); that data is
// relocated into the active profile on first boot by
// `merula::config::migrate_legacy_dirs`.
//
// Config and data share one dir here (the profile root lives under the OS
// *config* root via [`arbor_config_dir`]). On Windows/macOS that already matched
// the old sibling layout; on Linux the sample banks now live under `~/.config`
// rather than `~/.local/share`, the deliberate trade-off of making everything
// profile-scoped under one root.

/// `arbor/profiles/<active>/merula` — merula's per-profile config + data dir.
pub fn merula_config_dir() -> PathBuf {
    crate::profile::product_dir(crate::profile::PRODUCT_MERULA)
}

/// Convenience: join a relative path under [`merula_config_dir`].
pub fn merula_config_path<P: AsRef<Path>>(sub: P) -> PathBuf {
    merula_config_dir().join(sub)
}

/// Home of the downloaded sample packs, the VSCO 2 bank, and the merula window
/// state. Profile-scoped — the same dir as [`merula_config_dir`] (see the module
/// note above); kept as a separate helper so call sites state intent.
pub fn merula_data_dir() -> PathBuf {
    crate::profile::product_dir(crate::profile::PRODUCT_MERULA)
}

/// The legacy top-level sibling roots merula used **before** it became a
/// profile-scoped product (`%APPDATA%\merula`, and the even older `nemus` from
/// before the rename). Used only by the one-shot boot migration to relocate that
/// data into the active profile. Returned newest-first.
pub fn merula_legacy_sibling_dirs() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for root in [dirs::config_dir(), dirs::data_dir()].into_iter().flatten() {
        for name in ["merula", "nemus"] {
            let p = root.join(name);
            if !out.contains(&p) {
                out.push(p);
            }
        }
    }
    out
}
