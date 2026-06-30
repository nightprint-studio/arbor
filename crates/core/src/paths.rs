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

/// `arbor/data` — a **global** data root sibling to [`arbor_config_dir`], on the
/// same volume as the per-profile `profiles/` tree (on Windows that is
/// `%APPDATA%\arbor\data`). Holds heavy, profile-independent assets that must not
/// be duplicated per profile. Deliberately rooted under the OS *config* root (not
/// [`arbor_data_dir`]) so it always lives next to `profiles/`.
pub fn arbor_global_data_dir() -> PathBuf {
    arbor_config_dir().join("data")
}

// ── merula ───────────────────────────────────────────────────────────────────
//
// merula (the live-coding music workspace) splits its state in two:
//
//   * **config = per-profile** ([`merula_config_dir`] →
//     `arbor/profiles/<active>/merula/`): only small per-profile data —
//     `config.toml`, `state.json`, `aliases.json`, `scratch.json`,
//     `active_packs.toml`, `speech-cache/`.
//   * **heavy data = global, shared across profiles** ([`merula_data_dir`] →
//     `arbor/data/merula/`): the multi-GB assets (VSCO 2 bank, downloaded sample
//     packs, models, libraries). Rooted under [`arbor_global_data_dir`] so it is
//     stored **once** and reused by every profile.
//
// merula used to own a top-level sibling namespace (`%APPDATA%\merula`); that
// data is relocated on first boot by `merula::config::migrate_legacy_dirs`.
//
// Linux nuance: [`arbor_global_data_dir`] hangs off the OS *config* root, so the
// heavy assets land under `~/.config/arbor/data/merula` rather than
// `~/.local/share`. That is a deliberate trade-off — it keeps the global data on
// the same volume as `profiles/` and avoids splitting merula across two OS roots.

/// `arbor/profiles/<active>/merula` — merula's **per-profile** config dir. Holds
/// only small per-profile state (config, window state, aliases, scratch, active
/// packs, speech cache); the heavy shared assets live in [`merula_data_dir`].
pub fn merula_config_dir() -> PathBuf {
    crate::profile::product_dir(crate::profile::PRODUCT_MERULA)
}

/// Convenience: join a relative path under [`merula_config_dir`].
pub fn merula_config_path<P: AsRef<Path>>(sub: P) -> PathBuf {
    merula_config_dir().join(sub)
}

/// `arbor/data/merula` — the **global, shared** heavy-asset root for merula:
/// the downloaded sample packs, the VSCO 2 bank, models, and libraries. Rooted
/// under [`arbor_global_data_dir`] so these multi-GB assets are stored once and
/// shared across every profile — **not** duplicated per profile (unlike
/// [`merula_config_dir`]).
pub fn merula_data_dir() -> PathBuf {
    arbor_global_data_dir().join("merula")
}

/// `arbor/profiles/<active>/sitta` — sitta's **per-profile** config dir. Holds the
/// file-explorer's own settings (`config.toml`) and any small per-profile state.
/// Resolved by `sitta-be` itself after `init_active_profile()` — not pushed by the
/// shell — mirroring [`merula_config_dir`].
pub fn sitta_config_dir() -> PathBuf {
    crate::profile::product_dir(crate::profile::PRODUCT_SITTA)
}

/// Convenience: join a relative path under [`sitta_config_dir`].
pub fn sitta_config_path<P: AsRef<Path>>(sub: P) -> PathBuf {
    sitta_config_dir().join(sub)
}

/// `arbor/data/sitta` — the **global, shared** heavy-asset root for sitta (e.g. a
/// thumbnail / icon cache). Rooted under [`arbor_global_data_dir`] so caches are
/// shared across profiles, not duplicated. Sibling of [`merula_data_dir`].
pub fn sitta_data_dir() -> PathBuf {
    arbor_global_data_dir().join("sitta")
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
