//! Tauri-agnostic handle into the host process.
//!
//! Domain crates that need to emit events, locate the Arbor data root, or
//! ask whether the user is currently focused on the window take a
//! `&dyn AppCtx` (or `Arc<dyn AppCtx>`) instead of a `tauri::AppHandle`.
//! The Tauri shell crate implements this trait once on top of `AppHandle`;
//! tests implement a lightweight mock.
//!
//! The trait is intentionally minimal — every method is a "the host has
//! this and the domain needs it" capability. New methods are added only
//! when a domain crate actually needs one, never speculatively.
//!
//! No consumer in `arbor-core` itself uses this trait; it lives here so
//! every domain crate can depend on a single common definition without
//! pulling in the Tauri shell.

use std::any::Any;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

pub trait AppCtx: Any + Send + Sync {
    /// Downcast hook so host-specific call sites (e.g. the Tauri shell's
    /// per-namespace installers that still need a real `tauri::AppHandle`)
    /// can recover the concrete impl from a `&dyn AppCtx`. Domain crates
    /// should never call this — the existence of a downcast is a smell that
    /// a capability is missing from the trait surface.
    fn as_any(&self) -> &dyn Any;

    /// Emit a frontend event with a JSON payload. Equivalent to
    /// `tauri::AppHandle::emit(event, payload)` on the Tauri impl.
    fn emit(&self, event: &str, payload: serde_json::Value);

    /// Spawn a detached future on the host's async runtime.
    ///
    /// Domain crates use this for background work that must NOT assume an
    /// ambient Tokio reactor on the calling thread — the plugin-boot OS
    /// thread, in particular, runs lifecycle hooks (`on_plugin_load`) with no
    /// runtime in scope, so a bare `tokio::spawn` there panics. The Tauri impl
    /// delegates to `tauri::async_runtime::spawn`, which carries a
    /// process-global runtime handle and therefore works from any thread (and
    /// drives `tokio::time` timers inside the future).
    fn spawn(&self, fut: Pin<Box<dyn Future<Output = ()> + Send + 'static>>);

    /// Root of Arbor's on-disk state (typically the value of
    /// [`crate::paths::arbor_config_dir`]). Exposed through the trait so
    /// hosts that rebase Arbor under a portable directory can override it
    /// without monkey-patching the global helper.
    fn arbor_dir(&self) -> &Path;

    /// Whether the Arbor window currently has user focus. Used by
    /// throughput-sensitive background loops (auto-refresh, polling) to
    /// back off while the user is in another app.
    fn is_focused(&self) -> bool;

    /// Append a line to the Plugin Logs panel (the in-memory ring buffer
    /// that streams to the frontend via `arbor://plugin-log` events).
    ///
    /// `level` is one of `"debug" | "info" | "warn" | "error"`. `plugin`
    /// is the offending plugin's name. `message` is the human-readable
    /// payload. Default is a no-op so headless hosts (CLI, tests) don't
    /// need to wire up a buffer.
    fn record_plugin_log(&self, _level: &str, _plugin: &str, _message: &str) {}

    /// Path of the repository currently visible in the active tab, if any.
    /// Used by host-pure namespaces (`arbor.settings.read_project`, …) that
    /// need to scope per-repo state without depending on a shell-side
    /// `AppState`. Default is `None` so headless / test hosts trivially
    /// satisfy the contract.
    fn active_repo_path(&self) -> Option<PathBuf> { None }

    /// Reveal a file/folder path in the user's chosen file manager. Backs
    /// `arbor.ui.open_path`. A FILE is revealed inside its containing folder
    /// (selected); a FOLDER is opened as the listing. The host applies the
    /// user's OS-vs-built-in explorer preference — either the OS file manager
    /// (Explorer / Finder / xdg-open) or Arbor's built-in explorer window.
    /// Default errors out so headless hosts surface a clear "unsupported"
    /// rather than silently succeeding.
    fn open_path(&self, _path: &str) -> Result<(), String> {
        Err("open_path: not supported by this host".to_string())
    }

    // ── Plugin-owned credentials ─────────────────────────────────────────
    //
    // Three methods rather than one because the storage is the host's and the *policy* is
    // not: whether a plugin may touch a key at all was already decided at the API gate,
    // against the slots its manifest declared. What reaches here is a plugin name and a key
    // that belong together by construction, and the implementation's only job is to resolve
    // them through `arbor_plugin_types::credentials::account` and hit the store.
    //
    // The signatures take `(plugin, key)` and never an account string, so there is no way to
    // ask a host for a credential outside a plugin's own namespace — Arbor's own entries are
    // not filtered out of these calls, they are unreachable through them.
    //
    // Defaults refuse, so a headless host that has no keychain says so instead of silently
    // losing a secret.

    /// Read one of a plugin's own credentials. `Ok(None)` when the slot is empty.
    fn credential_get(&self, _plugin: &str, _key: &str) -> Result<Option<String>, String> {
        Err("credentials: not supported by this host".to_string())
    }

    /// Create or replace one of a plugin's own credentials.
    fn credential_set(&self, _plugin: &str, _key: &str, _value: &str) -> Result<(), String> {
        Err("credentials: not supported by this host".to_string())
    }

    /// Remove one of a plugin's own credentials. Removing an empty slot succeeds — the
    /// caller asked for it to be gone, and it is.
    fn credential_delete(&self, _plugin: &str, _key: &str) -> Result<(), String> {
        Err("credentials: not supported by this host".to_string())
    }

    // ── Extensions ───────────────────────────────────────────────────────
    //
    // JSON in, JSON out, and deliberately opaque. Everything past this seam belongs to the
    // extension's own interface, and a typed signature here would mean this crate learning
    // what a mesh or a shader is — which is the whole thing the extension seam exists to
    // avoid. The capability gate already ran against the calling plugin's manifest.
    //
    // Defaults refuse, so a host with no wasm runtime says so instead of silently doing
    // nothing.

    /// Everything installed and what it exports, as a JSON array.
    fn ext_surface(&self, _plugin: &str) -> Result<String, String> {
        Err("extensions: not supported by this host".to_string())
    }

    /// Call one function on one extension. `spec_json` carries the address, the method and
    /// the positional arguments; the answer is that function's return value as JSON.
    fn ext_call(&self, _plugin: &str, _spec_json: &str) -> Result<String, String> {
        Err("extensions: not supported by this host".to_string())
    }

    /// Call one function and write the bytes it returns into a local file, answering with how
    /// many were written.
    ///
    /// Separate from [`ext_call`](Self::ext_call) because of what a blob costs as JSON: a
    /// megabyte of payload becomes six megabytes of number-array, serialised, parsed and held
    /// once in each process it crosses. Anything moving bytes — a download written chunk by
    /// chunk — uses this and they never become a document.
    ///
    /// `file_json` is the destination: the absolute path, and whether to append. The path is
    /// checked against the calling plugin's `fs` permission BEFORE this is reached, in the
    /// namespace that has the plugin's context; a host implementing this writes where it is
    /// told.
    fn ext_call_to_file(
        &self,
        _plugin: &str,
        _spec_json: &str,
        _file_json: &str,
    ) -> Result<u64, String> {
        Err("extensions: not supported by this host".to_string())
    }

    /// Call one function passing the contents of a local file as one of its arguments — the
    /// upload direction of [`ext_call_to_file`](Self::ext_call_to_file), with the same
    /// reasoning and the same permission rule. Answers with the call's return value as JSON.
    fn ext_call_from_file(
        &self,
        _plugin: &str,
        _spec_json: &str,
        _file_json: &str,
    ) -> Result<String, String> {
        Err("extensions: not supported by this host".to_string())
    }

    // ── OAuth ────────────────────────────────────────────────────────────
    //
    // The engine, never the provider. Which endpoints, which scopes, which client: all of
    // that arrives as data in `spec_json`, from the plugin that knows. What the host
    // contributes is the two halves a plugin cannot have — the loopback listener the browser
    // redirects to, and the credential store the tokens land in.
    //
    // The slot named in the spec was already checked against the plugin's declared
    // `[[credentials]]` by `arbor.oauth`, in the host that holds the manifest.

    /// Begin an installed-app flow; answers with the URL to open in a browser. The outcome
    /// arrives later as the plugin hook the spec named.
    fn oauth_start(&self, _plugin: &str, _spec_json: &str) -> Result<String, String> {
        Err("oauth: not supported by this host".to_string())
    }

    /// Renew the access token in a plugin's slot from the refresh token beside it. Answers
    /// `{ refreshed, expires_in }` as JSON — `refreshed: false` when the stored one still had
    /// enough life left to be worth keeping.
    fn oauth_refresh(&self, _plugin: &str, _spec_json: &str) -> Result<String, String> {
        Err("oauth: not supported by this host".to_string())
    }

    /// Dispatch a gated host built-in command (`arbor:area.verb`) a plugin
    /// invoked through the command-invocation protocol. Resolution + both
    /// capability gates already ran in the plugin host; this only runs the
    /// handler (which needs host state the plugin crate can't reach).
    ///
    /// **Must be non-blocking.** The caller holds the plugin-host lock, so the
    /// implementation has to defer the actual work (spawn on the host runtime)
    /// and return immediately — otherwise a handler that fires a plugin hook
    /// would deadlock on the same lock. `ctx_json` is the node payload (form
    /// values + declared `args`). Default is a no-op + warn so headless / test
    /// hosts that expose no built-ins satisfy the contract trivially.
    fn invoke_host_command(&self, id: &str, _ctx_json: &str) {
        tracing::warn!("invoke_host_command('{id}'): no host built-ins on this host");
    }
}
