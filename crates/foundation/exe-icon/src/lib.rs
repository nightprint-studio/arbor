//! `arbor-exe-icon` — the one line a backend's build script needs to stop being a blank
//! executable.
//!
//! ```no_run
//! // crates/products/corvus/be/build.rs
//! fn main() {
//!     arbor_exe_icon::prelude::embed();
//! }
//! ```
//!
//! ## Why a crate for five lines
//!
//! Because there are seven of them. Every product ships a headless backend — `corvus-be`,
//! `sitta-be`, `merula-be`, and the rest — and each is an ordinary `[[bin]]`, which on Windows
//! means a generic executable glyph in Explorer, in the taskbar, in Task Manager and in every
//! "an app wants to…" prompt the user is asked to recognise. Seven identical build scripts
//! carrying seven copies of a resource-compiler incantation is seven places for it to drift,
//! and the first person to add an eighth backend copies whichever one they found.
//!
//! So the mechanism lives here and the *artwork* stays with its product: each backend crate
//! holds its own `icon.ico`, beside the `Cargo.toml` that names it. A crate that moves takes
//! its icon with it, which is the point of keeping them splittable.
//!
//! ## What it writes
//!
//! The icon, and the VERSIONINFO block behind Properties → Details: the crate's own
//! description as the file description, its version, and Arbor as the product. Signed builds
//! and SmartScreen both read that block, and an executable with nothing in it is the one that
//! looks like it came from nowhere.
//!
//! ## Everywhere that is not Windows
//!
//! Nothing happens, and nothing fails. A `.ico` is a Win32 resource; macOS takes its icon from
//! the `.app` bundle around a binary and Linux from a `.desktop` file, neither of which is a
//! build script's business. `tauri-winres` reports "not building for Windows" as success, so
//! this is a no-op on those targets rather than a `cfg` the caller has to remember.
//!
//! Generate or refresh the `.ico` files with `scripts/gen-exe-icons.ps1` — they are committed
//! artefacts re-encoded from `src-tauri/icons/products/<product>.png`, never built on the fly.

use std::path::{Path, PathBuf};

pub mod prelude;

/// The icon a backend crate is expected to hold, beside its manifest.
const DEFAULT_ICON: &str = "icon.ico";

/// Embed `icon.ico` from the calling crate's root, plus its file properties.
///
/// Call it from `build.rs` and pass nothing: everything it needs — where the crate is, what it
/// is called, what it does, what version it is — Cargo has already put in the environment.
///
/// A missing icon is a warning, not a failure. A backend still works without one, and a build
/// that stops because an artwork file was not committed yet would be a worse trade than a
/// binary that looks plain for an afternoon.
pub fn embed() {
    let manifest = match std::env::var("CARGO_MANIFEST_DIR") {
        Ok(dir) => PathBuf::from(dir),
        Err(_) => {
            warn("CARGO_MANIFEST_DIR is unset — call this from a build script");
            return;
        }
    };
    embed_icon(manifest.join(DEFAULT_ICON));
}

/// Embed a specific `.ico`, for a binary whose icon is not the crate's own.
///
/// The general case of [`embed`]; a caller that needs it is a caller with two binaries in one
/// crate, and there is none today.
pub fn embed_icon(icon: impl AsRef<Path>) {
    let icon = icon.as_ref();

    // Say it on every target, not just Windows: a cross-build that silently skipped a missing
    // file would report it for the first time on the machine that packages the release.
    println!("cargo:rerun-if-changed={}", icon.display());
    if !icon.is_file() {
        warn(&format!("no icon at {} — the executable keeps the default one", icon.display()));
        return;
    }

    let mut res = tauri_winres::WindowsResource::new();
    res.set_icon(&icon.to_string_lossy());
    // `new()` already fills FileDescription / FileVersion / ProductVersion from the crate.
    // These two are what it cannot know: the product a backend belongs to, and the file it
    // will be on disk — which is what the Details tab shows for a renamed copy.
    res.set("ProductName", "Arbor");
    if let Ok(name) = std::env::var("CARGO_PKG_NAME") {
        res.set("OriginalFilename", &format!("{name}.exe"));
        res.set("InternalName", &name);
    }
    if let Ok(authors) = std::env::var("CARGO_PKG_AUTHORS") {
        // Cargo joins multiple authors with `:`, which reads as a typo in a properties dialog.
        res.set("CompanyName", &authors.replace(':', ", "));
    }

    if let Err(e) = res.compile() {
        // Not fatal on purpose: a workstation without the Windows SDK's `rc.exe` can still
        // build and run every backend, it just gets plain ones.
        warn(&format!("could not embed {}: {e}", icon.display()));
    }
}

fn warn(message: &str) {
    println!("cargo:warning=arbor-exe-icon: {message}");
}
