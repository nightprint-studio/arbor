//! Build-wide constants: API contract version, app version, host OS string.

/// API version supported by this build of Arbor.
/// Plugins with a higher arbor_api are rejected at load time.
pub const ARBOR_API_VERSION: u32 = 1;

/// App version pulled from `Cargo.toml` at compile time.
pub const ARBOR_APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Current OS as a manifest-friendly string. Matches values accepted in the
/// `os` field of plugin.toml.
pub fn current_os() -> &'static str {
    if cfg!(target_os = "windows") { "windows" }
    else if cfg!(target_os = "macos") { "macos" }
    else if cfg!(target_os = "linux") { "linux" }
    else { "other" }
}
