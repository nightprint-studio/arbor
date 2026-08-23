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
//! One rule: the requested path must be inside a plugin root, **as written**. No `..` in it,
//! and it starts with a root — with those two, it cannot name anything outside one.
//!
//! Deliberately not "canonicalise and then compare". A package under development is installed
//! as links into the checkout it is written in — sometimes the whole directory, sometimes one
//! link per shipped file — so its files really live nowhere near a plugin root, and resolving
//! before comparing turned every one of them into a 403 on a URL that reads perfectly.
//! Chasing that with a list of each package's real directory only covered the first shape.
//!
//! Where a package's bytes physically sit is a decision of whoever installed it, and a plugin
//! cannot make links (no `io`, and `arbor.fs` has no such verb) — so following them costs
//! nothing that was not already granted. Which files exist under a root is the boundary; how
//! they got there is not.
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

/// The file behind a requested path, or `None` when it is not a plugin's.
fn resolve(requested: &Path) -> Option<PathBuf> {
    within_roots(requested, &allowed_roots()).then(|| requested.to_path_buf())
}

/// Whether `requested` names something under one of `roots` — decided on the path as written.
///
/// Split out from [`resolve`] because it is the whole of the security decision and the only
/// part that can be tested: `resolve` answers against the profile this machine happens to
/// have, this answers against whatever roots you hand it.
fn within_roots(requested: &Path, roots: &[PathBuf]) -> bool {
    // `..` is the only component that can walk a path out of a prefix it starts with. Without
    // one, "starts with a root" and "is inside that root" are the same statement.
    if requested.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return false;
    }
    roots.iter().any(|root| requested.starts_with(root))
}

/// The two directories a profile keeps plugins in, each in both the form the rest of the app
/// builds paths from and its resolved form.
///
/// Both, because a root can itself be reached through a link — a workspace checked out under
/// one on macOS (`/tmp` → `/private/tmp`) is the ordinary case — and a request built from the
/// one spelling must not be refused for not matching the other.
fn allowed_roots() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for root in [
        arbor_plugin_core::prelude::plugin_dir(),
        arbor_plugin_marketplace::prelude::plugins_dir(),
    ] {
        if let Ok(real) = std::fs::canonicalize(&root) {
            if real != root {
                out.push(real);
            }
        }
        out.push(root);
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
        let roots = vec![PathBuf::from("/profiles/p/plugins")];
        assert!(!within_roots(Path::new("/etc/passwd"), &roots));
        assert!(!within_roots(Path::new("/profiles/p/plugins-elsewhere/x"), &roots));
    }

    #[test]
    fn dot_dot_is_refused_even_when_it_starts_inside() {
        // The one component that can leave a prefix it starts with. `starts_with` alone would
        // say yes to this, which is why it is checked separately rather than trusted away.
        let roots = vec![PathBuf::from("/profiles/p/plugins")];
        assert!(!within_roots(Path::new("/profiles/p/plugins/../../../etc/passwd"), &roots));
    }

    #[test]
    fn a_package_linked_whole_serves_its_files() {
        // `plugins/<pkg>` is a link into the checkout the package is written in. Nothing here
        // resolves it — the request names a path under the root, and that is the question.
        let roots = vec![PathBuf::from("/profiles/p/plugins")];
        assert!(within_roots(
            Path::new("/profiles/p/plugins/bevy-runtime/web/runtime_bg.wasm"),
            &roots,
        ));
    }

    #[test]
    fn a_package_linked_file_by_file_serves_its_files() {
        // The shape that broke the shader preview: the package directory is REAL and only its
        // entries are links, so canonicalising the request landed in the source checkout and
        // no list of package directories could have covered it.
        let tmp = std::env::temp_dir().join(format!("arbor-plugin-assets-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let root = tmp.join("plugins");
        let pkg = root.join("bevy-runtime");
        let real = tmp.join("checkout").join("web");
        std::fs::create_dir_all(&pkg).unwrap();
        std::fs::create_dir_all(&real).unwrap();
        std::fs::write(real.join("runtime_bg.wasm"), b"\0asm").unwrap();

        #[cfg(unix)]
        std::os::unix::fs::symlink(&real, pkg.join("web")).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(&real, pkg.join("web")).unwrap();

        let requested = pkg.join("web").join("runtime_bg.wasm");
        assert!(within_roots(&requested, &[root.clone()]), "{requested:?} refused");
        // And it really reads through the link — the check being right is only half of it.
        assert_eq!(std::fs::read(&requested).unwrap(), b"\0asm");
        // The old rule, for the record: resolved, it is nowhere near the root.
        assert!(!std::fs::canonicalize(&requested).unwrap().starts_with(&root));

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
