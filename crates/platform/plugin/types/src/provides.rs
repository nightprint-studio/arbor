//! What a package **provides** beyond Lua — the `[lua]`, `[[provides]]` and `[wasm]`
//! sections of `plugin.toml`.
//!
//! ## Why `entry` was not enough
//!
//! The old shape said `entry = "main.lua"`, which describes the package as *being* a Lua
//! plugin. That is one thing a package can be, and it stopped being the only one the moment
//! a package could also carry an implementation of an interface the host defines — a Studio
//! format backend, a cloud provider. A connector is both at once: a wasm module that reads a
//! bucket, and the Lua that draws the panel configuring it.
//!
//! So the manifest describes **what a package contains** rather than what it is. A package
//! with no [`Provides`] is exactly what it always was, and installs down exactly the path it
//! always did.
//!
//! ## Why this is static, when Lua contributions are not
//!
//! Everything a Lua plugin contributes — views, panels, activity-bar entries — is registered
//! **at runtime**, from `main.lua`. That works because the plugin is already running when it
//! registers. A wasm interface cannot be declared that way: the host has to know what a
//! package provides *before instantiating it*, to decide whether to load it at all and to
//! show it in the marketplace before a single byte of it has run.
//!
//! That asymmetry is the reason this section exists and the boundary on how far it should
//! grow: it holds what must be known before the guest runs, and nothing a plugin could just
//! as well say once it is running.

use serde::{Deserialize, Serialize};

/// The `[lua]` section — the Lua half of a package, when it has one.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LuaSection {
    /// Entry point, relative to the package directory.
    #[serde(default = "default_entry")]
    pub entry: String,
}

impl Default for LuaSection {
    fn default() -> Self {
        Self { entry: default_entry() }
    }
}

fn default_entry() -> String {
    "main.lua".to_string()
}

/// The wasm target a package's modules were built for.
///
/// Declared rather than assumed. `wasm32-unknown-unknown`, `wasip1` and `wasip2` are
/// different worlds with different ABIs, and a host that guesses is a host that eventually
/// supports two of them because a package built for the other one shipped and worked by
/// accident.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum WasmTarget {
    /// The component model on WASI 0.2 — what Arbor's interfaces are defined against.
    #[default]
    Wasm32Wasip2,
    /// Core wasm, no WASI. Accepted so a pure-compute guest that needs nothing from the
    /// system can be built without one.
    Wasm32UnknownUnknown,
}

/// The `[wasm]` section — settings shared by every module in a package.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WasmSection {
    #[serde(default)]
    pub target: WasmTarget,
}

/// One implementation of one host interface.
///
/// The three identifying fields are separate on purpose:
///
/// * `interface` says **which contract** — `studio-format`, `cloud-provider`.
/// * `version` versions **that contract**, not the package and not the Lua API. A package's
///   own `version` moves when its author releases; `arbor_api` moves when the Lua surface
///   changes; an interface moves when *it* changes. Collapsing any two of the three means one
///   of them cannot move without invalidating things that did not change.
/// * `id` says **which member** of that interface this is — the `json` of `studio-format`,
///   the `gcs` of `cloud-provider`. It is what the host dispatches on and what the user sees.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provides {
    pub interface: String,
    pub version:   u32,
    pub id:        String,
    /// The module file, relative to the package directory. Present in an installed package;
    /// in the repo it names what the release will carry.
    pub module:    String,
}

/// A credential slot a package owns.
///
/// ## The rule this type exists to make expressible
///
/// A plugin — Lua or wasm, the rule does not distinguish — may create and read **only the
/// credentials it declared here**, and can never reach the ones Arbor keeps for itself: the
/// git provider tokens, the MCP token, anything the shell brokers on the user's behalf.
///
/// It is enforced as a **namespace, not a filter**. The credential API takes a `key` and
/// resolves it inside the declaring package's own space, so Arbor's own entries are not
/// hidden from a plugin — they are *unnameable* by one. A filter is a list of things to say
/// no to, and lists have gaps; a namespace has no way to spell "outside".
///
/// Declaring the slots rather than a boolean is what lets the consent dialog say *what* will
/// be stored instead of "this plugin uses credentials", which is the difference between a
/// question and a formality.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialSlot {
    /// Stable key, unique within the package. Becomes the plugin-scoped name in the keyring.
    pub key:   String,
    /// What it is, shown in the consent dialog and the Plugin Manager.
    pub label: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lua_section_defaults_to_main_lua() {
        let s: LuaSection = toml::from_str("").unwrap();
        assert_eq!(s.entry, "main.lua");
        assert_eq!(LuaSection::default().entry, "main.lua");
    }

    #[test]
    fn a_provides_entry_round_trips() {
        let p: Provides = toml::from_str(
            r#"
            interface = "studio-format"
            version   = 1
            id        = "json"
            module    = "studio_json.wasm"
        "#,
        )
        .unwrap();
        assert_eq!(p.interface, "studio-format");
        assert_eq!(p.version, 1);
        assert_eq!(p.id, "json");
    }

    #[test]
    fn the_wasm_target_is_kebab_case_and_defaults_to_the_component_model() {
        let w: WasmSection = toml::from_str("target = \"wasm32-wasip2\"").unwrap();
        assert_eq!(w.target, WasmTarget::Wasm32Wasip2);
        let w: WasmSection = toml::from_str("target = \"wasm32-unknown-unknown\"").unwrap();
        assert_eq!(w.target, WasmTarget::Wasm32UnknownUnknown);
        // Omitted entirely: the interfaces are defined against the component model, so that
        // is what a package that did not say gets checked as.
        assert_eq!(WasmSection::default().target, WasmTarget::Wasm32Wasip2);
    }

    #[test]
    fn an_unknown_wasm_target_is_rejected_rather_than_defaulted() {
        // Silently treating `wasm32-wasip1` as wasip2 would load a module built against a
        // different ABI and fail somewhere unrelated.
        assert!(toml::from_str::<WasmSection>("target = \"wasm32-wasip1\"").is_err());
    }

    #[test]
    fn a_credential_slot_declares_what_it_is_for() {
        let c: CredentialSlot =
            toml::from_str("key = \"oauth\"\nlabel = \"Google account\"").unwrap();
        assert_eq!(c.key, "oauth");
        assert_eq!(c.label, "Google account");
    }
}
