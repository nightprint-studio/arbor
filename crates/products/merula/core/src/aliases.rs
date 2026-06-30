//! Global sound **aliases** — the read helper the audio registry builder needs.
//!
//! User-defined `alias → target` name map (e.g. `kick = "RolandTR808_bd"`),
//! resolved by the audio registry so `s("kick")` plays the target. Global (NOT
//! per-project / per-file) but per-profile (`<merula-config>/aliases.json`).
//!
//! Only the **read** half lives here: the audio substrate (`audio_thread`'s
//! `build_registry`) reads aliases when building a session registry, so the read
//! helper must move with it into merula-core. The `get/set_merula_aliases`
//! handlers (+ the other window/project state) stay in merula-be's `fstate`, which
//! re-imports [`load_aliases`] from here.

use std::collections::HashMap;
use std::path::PathBuf;

/// Per-profile path of the alias map.
fn aliases_path() -> PathBuf {
    arbor_core::prelude::merula_config_dir().join("aliases.json")
}

/// Read the global sound-alias map (`alias → target`), defaulting to empty on a
/// missing / unparseable file (a clean start, never an error). Read by the registry
/// builder when building a session registry, and by merula-be's alias handlers.
pub fn load_aliases() -> HashMap<String, String> {
    std::fs::read_to_string(aliases_path())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}
