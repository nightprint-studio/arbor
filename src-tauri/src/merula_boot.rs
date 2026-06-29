//! merula launcher-boot: the one-shot legacy-storage migration.
//!
//! merula is fully out-of-process now (its state + audio substrate live in
//! `merula-core`, driven by the `merula-be` child process), so the shell holds no
//! merula facade and no merula state. The single launcher-boot concern that
//! remains is relocating merula's *legacy* on-disk storage into the split
//! profile/global layout — and it is **facade-free** (only `arbor_core` path
//! helpers + std), so it lives here rather than in the (deleted)
//! `src-tauri/src/merula/` subtree.
//!
//! It must run once at launcher startup, **before** any profile read — not in
//! merula-be, which spawns lazily per-window (too late, and per-profile wrong).

use std::fs;

/// Heavy, profile-independent assets — relocated to the GLOBAL merula data dir
/// ([`merula_data_dir`](arbor_core::prelude::merula_data_dir) →
/// `arbor/data/merula`). These are the multi-GB sample banks / VSCO bank /
/// download caches: sharing them across profiles avoids duplicating gigabytes
/// per profile. Used by both migration directions below.
const HEAVY_SUBDIRS: &[&str] = &["vsco", "packs", "models", "libraries"];

/// Lightweight config / state entries — relocated to the PER-PROFILE merula
/// config dir ([`merula_config_dir`](arbor_core::prelude::merula_config_dir) →
/// `arbor/profiles/<active>/merula`). These are small and profile-specific.
const CONFIG_ENTRIES: &[&str] = &[
    "config.toml",
    "state.json",
    "aliases.json",
    "scratch.json",
    "speech-cache",
];

/// Relocate merula's legacy storage into the split profile/global layout, once.
///
/// merula used to live in its own top-level sibling namespace next to `arbor`
/// (`%APPDATA%\merula`, and the even older `%APPDATA%\nemus` from before the
/// rename), with settings and the multi-GB sample banks all under one roof.
/// Storage is now SPLIT in two:
///
///   * **config/state → per-profile** ([`merula_config_dir`] →
///     `arbor/profiles/<active>/merula`) — see [`CONFIG_ENTRIES`].
///   * **heavy assets → global, shared across profiles** ([`merula_data_dir`] →
///     `arbor/data/merula`) — see [`HEAVY_SUBDIRS`].
///
/// Dumping the heavy banks per-profile would waste gigabytes, so the migration
/// fans the legacy sibling out into the two destinations. It is non-destructive,
/// idempotent, and crash-safe: every move is guarded on source-existence +
/// dest-absence, so partial runs converge on the next boot, and errors are
/// reported (`eprintln!`) but never panic — the leftovers stay for a retry.
/// Within `%APPDATA%` every move is a same-volume rename — atomic and instant
/// even for the multi-GB banks.
pub fn migrate_legacy_dirs() {
    migrate_legacy_sibling();
    migrate_profile_data_to_global();
}

/// Fan the legacy top-level sibling (`%APPDATA%\merula`, or pre-rename `…\nemus`)
/// out into the split layout: heavy subdirs → global data dir, config/state →
/// per-profile config dir. If the legacy dir is left empty afterwards, remove it
/// (non-recursively — any unknown user files keep it alive and untouched).
fn migrate_legacy_sibling() {
    use arbor_core::prelude::{merula_config_dir, merula_data_dir, merula_legacy_sibling_dirs};

    let Some(legacy) = merula_legacy_sibling_dirs().into_iter().find(|p| p.is_dir()) else {
        return; // fresh install — nothing to migrate
    };

    // Heavy assets → global data dir.
    for sub in HEAVY_SUBDIRS {
        let src = legacy.join(sub);
        let dest = merula_data_dir().join(sub);
        if src.is_dir() && !dest.exists() {
            if let Err(e) = fs::create_dir_all(merula_data_dir()) {
                eprintln!("merula: legacy migration mkdir data dir failed: {e}");
                continue;
            }
            if let Err(e) = fs::rename(&src, &dest) {
                eprintln!("merula: legacy heavy migration {src:?} -> {dest:?} failed: {e}");
            }
        }
    }

    // Config / state → per-profile config dir.
    for entry in CONFIG_ENTRIES {
        let src = legacy.join(entry);
        let dest = merula_config_dir().join(entry);
        if src.exists() && !dest.exists() {
            if let Err(e) = fs::create_dir_all(merula_config_dir()) {
                eprintln!("merula: legacy migration mkdir config dir failed: {e}");
                continue;
            }
            if let Err(e) = fs::rename(&src, &dest) {
                eprintln!("merula: legacy config migration {src:?} -> {dest:?} failed: {e}");
            }
        }
    }

    // Drop the legacy dir only if it is now empty — a non-recursive remove never
    // touches files we did not move (e.g. user drops we don't recognise).
    if let Ok(mut entries) = fs::read_dir(&legacy) {
        if entries.next().is_none() {
            if let Err(e) = fs::remove_dir(&legacy) {
                eprintln!("merula: legacy dir cleanup {legacy:?} failed: {e}");
            }
        }
    }
}

/// Defensive second pass for installs already migrated by the PRIOR version of
/// this function, which renamed the whole legacy sibling into the per-profile
/// config dir — leaving the heavy banks sitting under
/// [`merula_config_dir`](arbor_core::prelude::merula_config_dir). Lift any heavy
/// subdir found there into the global data dir, guarded the same way so it is a
/// no-op once converged.
fn migrate_profile_data_to_global() {
    use arbor_core::prelude::{merula_config_dir, merula_data_dir};

    for sub in HEAVY_SUBDIRS {
        let src = merula_config_dir().join(sub);
        let dest = merula_data_dir().join(sub);
        if src.is_dir() && !dest.exists() {
            if let Err(e) = fs::create_dir_all(merula_data_dir()) {
                eprintln!("merula: profile->global mkdir data dir failed: {e}");
                continue;
            }
            if let Err(e) = fs::rename(&src, &dest) {
                eprintln!("merula: profile->global heavy migration {src:?} -> {dest:?} failed: {e}");
            }
        }
    }
}
