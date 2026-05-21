//! App-level metadata exposed to the frontend (About modal, etc).
//!
//! Single source of truth for the version string — read from
//! `tauri.conf.json` (which mirrors `Cargo.toml`) at compile time and
//! surfaced over IPC so the UI never hardcodes "0.1.0" alongside it.

use serde::Serialize;
use tauri::AppHandle;

use crate::error::AppError;

#[derive(Serialize)]
pub struct AppInfo {
    /// Semantic version, from `tauri.conf.json` / `Cargo.toml`.
    pub version: String,
    /// Friendly OS family: "Windows", "macOS", "Linux", or the raw
    /// `std::env::consts::OS` value as a fallback.
    pub os:      String,
    /// CPU architecture as reported by `std::env::consts::ARCH`
    /// (e.g. "x86_64", "aarch64").
    pub arch:    String,
}

#[tauri::command]
pub fn get_app_info(app: AppHandle) -> Result<AppInfo, AppError> {
    let os = match std::env::consts::OS {
        "windows" => "Windows",
        "macos"   => "macOS",
        "linux"   => "Linux",
        other     => other,
    }
    .to_string();

    Ok(AppInfo {
        version: app.package_info().version.to_string(),
        os,
        arch: std::env::consts::ARCH.to_string(),
    })
}
