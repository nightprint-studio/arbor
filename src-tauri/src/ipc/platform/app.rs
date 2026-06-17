//! `app` domain — app-level metadata (version / OS / arch) for the About modal.
//!
//! The pilot handler that proves the `platform` backend seam end-to-end (macro
//! `program = "platform"` tag → `registry_for("platform")` → platform dispatch
//! → router → FE `platform("get_app_info")`). Wave 3 fills in the rest of the
//! platform domains (config, theme, session, workspace, jobs, fs, terminal)
//! against this template.

use serde::Serialize;

use crate::error::AppError;
use crate::ipc::platform;
use crate::AppState;

#[derive(Serialize)]
pub struct AppInfo {
    /// Semantic version. Compile-time `CARGO_PKG_VERSION` — the same string
    /// `tauri.conf.json` mirrors from `Cargo.toml`, so the About modal shows
    /// the identical value it did when this was a Tauri command reading
    /// `app.package_info().version`.
    pub version: String,
    /// Friendly OS family: "Windows", "macOS", "Linux", or the raw
    /// `std::env::consts::OS` value as a fallback.
    pub os: String,
    /// CPU architecture as reported by `std::env::consts::ARCH`
    /// (e.g. "x86_64", "aarch64").
    pub arch: String,
}

#[platform::handler(program = "platform")]
fn get_app_info(_state: &AppState) -> Result<AppInfo, AppError> {
    let os = match std::env::consts::OS {
        "windows" => "Windows",
        "macos"   => "macOS",
        "linux"   => "Linux",
        other     => other,
    }
    .to_string();

    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        os,
        arch: std::env::consts::ARCH.to_string(),
    })
}
