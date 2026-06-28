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
// merula (the live-coding music workspace) owns its own top-level namespace,
// sibling to `arbor` rather than nested under it: `%APPDATA%\merula` instead of
// `%APPDATA%\arbor\merula`. It's effectively a separate app sharing the same
// binary, so its config + the (potentially multi-GB) sample banks live apart.
// A future "shared" segment can still be carved out under `arbor` for anything
// the two genuinely co-own.

/// `~/.config/merula` on Linux, `%APPDATA%\merula` on Windows,
/// `~/Library/Application Support/merula` on macOS.
///
/// Falls back to `./merula` when `dirs::config_dir()` is unavailable.
pub fn merula_config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("merula")
}

/// Convenience: join a relative path under [`merula_config_dir`].
pub fn merula_config_path<P: AsRef<Path>>(sub: P) -> PathBuf {
    merula_config_dir().join(sub)
}

/// `~/.local/share/merula` on Linux, `%APPDATA%\merula` on Windows,
/// `~/Library/Application Support/merula` on macOS. Home of the downloaded
/// sample packs, the VSCO 2 bank, and the merula window state.
pub fn merula_data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("merula")
}
