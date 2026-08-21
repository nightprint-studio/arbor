//! The `plugin:` URI scheme — files a plugin package ships, served to the webview.
//!
//! ## Why not the asset protocol
//!
//! `asset:` exists to show the user's own media: its scope is configured with globs like
//! `**/*.png`, and its MIME table knows html, css, js, json, svg and a list of image and
//! video types. It does **not** know `wasm`, and that is not a detail — a `.wasm` served as
//! `application/octet-stream` makes `WebAssembly.instantiateStreaming` reject, so every
//! wasm-bindgen loader falls back to `arrayBuffer()` + `instantiate()`: the whole module into
//! memory, then compiled in one go. With a Bevy bundle that is the difference between a
//! viewport appearing and the entire app going away for a minute.
//!
//! Widening `asset:` to cover plugin folders also widens it for everything else that uses it.
//! A plugin's own files are a different thing with different rules, so they get their own
//! scheme and their own check.
//!
//! ## The check
//!
//! One rule: the resolved path must be inside a plugin root. Canonicalised first, so a
//! symlinked package (how a plugin under development is usually installed) resolves to where
//! its files really are, and `..` cannot walk out. Nothing else is served, whatever the URL
//! says.
//!
//! Every installed package can read every other's files through this. That is the same reach
//! `arbor.fs` already grants a plugin over its neighbours: packages the user installed are one
//! trust boundary, not one per package.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use tauri::http::{header, Request, Response, StatusCode};

/// Scheme name. `plugin://localhost/<path>` on macOS and Linux; Tauri rewrites it to
/// `http://plugin.localhost/<path>` on Windows, and `convertFileSrc(path, "plugin")` on the
/// frontend produces whichever form this platform uses.
pub const SCHEME: &str = "plugin";

/// Serve one request. Registered on the Tauri builder; see [`crate::setup::build_builder`].
pub fn handle(request: Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> {
    // Skip exactly one leading `/`, the same convention `asset:` uses: the URL carries one
    // slash more than the filesystem path, so `/Users/x` arrives as `//Users/x` and a Windows
    // `C:/x` as `/C:/x`. Matching it keeps one URL-building helper on the frontend.
    let raw = request.uri().path();
    let path = percent_decode(&raw.as_bytes()[1..]);

    let Some(file) = resolve(Path::new(&path)) else {
        tracing::warn!(
            "plugin asset refused: {path} resolves outside every plugin root ({:?})",
            allowed_roots(),
        );
        return refuse(StatusCode::FORBIDDEN);
    };

    match std::fs::read(&file) {
        Ok(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime_for(&file))
            // The frame that asks may be sandboxed to an opaque origin, which is a
            // cross-origin fetch by definition. These are the plugin's own files being
            // handed to the plugin's own page.
            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            // Plugin files change when a package is updated or reloaded, and a stale module
            // after a rebuild is a confusing thing to debug.
            .header(header::CACHE_CONTROL, "no-cache")
            .body(Cow::Owned(bytes))
            .unwrap_or_else(|_| refuse(StatusCode::INTERNAL_SERVER_ERROR)),
        Err(e) => {
            tracing::warn!("plugin asset {}: {e}", file.display());
            refuse(StatusCode::NOT_FOUND)
        }
    }
}

/// Percent-decode a URL path into a string.
///
/// Written here rather than pulled in: `percent-encoding` is not a dependency of the shell,
/// and this is the whole of what the one call site needs. Invalid escapes and invalid UTF-8
/// are passed through as-is rather than rejected — a path that decodes to nonsense simply
/// fails the root check below, which is the same answer with fewer ways to be wrong.
fn percent_decode(bytes: &[u8]) -> String {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = |b: u8| (b as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn refuse(status: StatusCode) -> Response<Cow<'static, [u8]>> {
    Response::builder()
        .status(status)
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .body(Cow::Borrowed(&[][..]))
        .expect("static response builds")
}

/// The real file behind a requested path, or `None` when it is not a plugin's.
fn resolve(requested: &Path) -> Option<PathBuf> {
    // Canonicalise the request FIRST. It resolves `..`, symlinks and `.` in one step, and
    // fails outright when the file does not exist — so everything below compares two real
    // paths rather than two strings that might mean the same place.
    let real = std::fs::canonicalize(requested).ok()?;
    allowed_roots().into_iter().any(|root| real.starts_with(&root)).then_some(real)
}

/// Every directory a plugin's files may really live in.
///
/// The two plugin roots, and then — the part that is easy to leave out and produces a 403 on
/// a path that looks perfectly right — **each package's own real directory**. A package under
/// development is usually a symlink into the repo it is written in, so canonicalising the
/// request lands somewhere no root covers. Canonicalising the roots does not help: the roots
/// are real directories; it is the entries inside them that point elsewhere.
///
/// Both halves are needed, and neither is redundant: the roots cover ordinary installs
/// (including files added after this list was built), the package directories cover the
/// symlinked ones.
fn allowed_roots() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for root in [
        arbor_plugin_core::prelude::plugin_dir(),
        arbor_plugin_marketplace::prelude::plugins_dir(),
    ] {
        let Ok(root) = std::fs::canonicalize(&root) else { continue };
        let entries = std::fs::read_dir(&root).ok();
        out.push(root);
        let Some(entries) = entries else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            if let Ok(real) = std::fs::canonicalize(&path) {
                if !out.iter().any(|r| real.starts_with(r)) {
                    out.push(real);
                }
            }
        }
    }
    out
}

/// Content type by extension.
///
/// `wasm` is the reason this function exists rather than a call into Tauri's table, which
/// does not list it. The rest is the set a shipped page actually uses; anything unknown falls
/// back to a type the webview will not try to execute or render.
fn mime_for(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("wasm") => "application/wasm",
        Some("html" | "htm") => "text/html",
        Some("js" | "mjs") => "text/javascript",
        Some("css") => "text/css",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("ico") => "image/vnd.microsoft.icon",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("ttf") => "font/ttf",
        Some("txt" | "md") => "text/plain",
        Some("ron" | "toml" | "lua") => "text/plain",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wasm_is_served_as_wasm() {
        // The whole point. `application/octet-stream` here sends every wasm-bindgen loader
        // down the non-streaming path: the module buffered whole, then compiled in one go.
        assert_eq!(mime_for(Path::new("/p/web/runtime_bg.wasm")), "application/wasm");
    }

    #[test]
    fn a_page_and_its_module_get_executable_types() {
        assert_eq!(mime_for(Path::new("/p/web/index.html")), "text/html");
        assert_eq!(mime_for(Path::new("/p/web/runtime.js")), "text/javascript");
        assert_eq!(mime_for(Path::new("/p/web/runtime.mjs")), "text/javascript");
    }

    #[test]
    fn an_unknown_extension_is_not_executable() {
        // Falling back to something the webview would run or render would make an unknown
        // file type a way to get code executed.
        assert_eq!(mime_for(Path::new("/p/data.sqlite")), "application/octet-stream");
        assert_eq!(mime_for(Path::new("/p/noext")), "application/octet-stream");
    }

    #[test]
    fn the_extension_match_is_case_insensitive() {
        assert_eq!(mime_for(Path::new("/p/LOGO.PNG")), "image/png");
    }

    #[test]
    fn percent_escapes_are_decoded() {
        assert_eq!(percent_decode(b"/Users/a%20b/web/index.html"), "/Users/a b/web/index.html");
        // A Windows drive letter arrives encoded, because `:` is not path-safe in a URL.
        assert_eq!(percent_decode(b"C%3A/Sviluppo/pkg"), "C:/Sviluppo/pkg");
    }

    #[test]
    fn a_broken_escape_is_left_alone() {
        // Not an error: it will fail the root check, which is the same answer.
        assert_eq!(percent_decode(b"/a%zz/b"), "/a%zz/b");
        assert_eq!(percent_decode(b"/trailing%"), "/trailing%");
    }

    #[test]
    fn a_path_outside_every_root_is_refused() {
        // `resolve` canonicalises, so this also covers `..` walking out of a plugin folder:
        // the resolved path simply is not under a root any more.
        assert!(resolve(Path::new("/etc/passwd")).is_none());
    }

    #[test]
    fn a_path_that_does_not_exist_is_refused() {
        assert!(resolve(Path::new("/definitely/not/here/at/all.wasm")).is_none());
    }

    #[test]
    fn a_symlinked_package_serves_from_where_its_files_really_are() {
        // The shape that broke twice: `plugins/<pkg>` is a symlink into the repo the package
        // is developed in, so the canonical path of its files is nowhere near the plugin
        // root. If only the roots were listed, every file in a linked package would 403 on a
        // path that reads correctly.
        let tmp = std::env::temp_dir().join("arbor-plugin-assets-symlink-test");
        let _ = std::fs::remove_dir_all(&tmp);
        let root = tmp.join("plugins");
        let real = tmp.join("elsewhere").join("my-pkg").join("web");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&real).unwrap();
        std::fs::write(real.join("runtime_bg.wasm"), b"\0asm").unwrap();

        #[cfg(unix)]
        std::os::unix::fs::symlink(real.parent().unwrap(), root.join("my-pkg")).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(real.parent().unwrap(), root.join("my-pkg")).unwrap();

        // What `allowed_roots` does, against this fixture rather than the real profile.
        let mut roots = vec![std::fs::canonicalize(&root).unwrap()];
        for entry in std::fs::read_dir(&root).unwrap().flatten() {
            if entry.path().is_dir() {
                roots.push(std::fs::canonicalize(entry.path()).unwrap());
            }
        }

        let requested = root.join("my-pkg").join("web").join("runtime_bg.wasm");
        let resolved = std::fs::canonicalize(&requested).unwrap();
        assert!(
            roots.iter().any(|r| resolved.starts_with(r)),
            "a linked package's real path must be covered: {resolved:?} vs {roots:?}",
        );
        // And the root alone is NOT enough — which is the whole point.
        assert!(!resolved.starts_with(std::fs::canonicalize(&root).unwrap()));

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
