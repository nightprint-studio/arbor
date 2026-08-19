//! The one door to scap, and the only place that knows two awkward things about it.
//!
//! **It needs an OS permission that can be refused.** On macOS screen capture is
//! gated by TCC; the user is asked once, and a refusal is permanent as far as the
//! process is concerned — `CGRequestScreenCaptureAccess` returns `false` from then
//! on without showing anything. So there is nothing to retry: the only way out is
//! System Settings plus a relaunch, and the error has to say so. Windows and Linux
//! report permission as always granted, which is why the gate is unconditional here
//! rather than `#[cfg]`'d — one code path, one place to look.
//!
//! **It panics instead of returning that refusal.** `scap::get_all_targets` reaches
//! `SCShareableContent::current()`, which `unwrap()`s the very `Err` that carries
//! "the user refused TCC permission", so enumerating sources on a machine that said
//! no takes down the calling thread. Nothing above it can be written correctly
//! against a function that unwinds instead of erroring, so every call into scap goes
//! through [`guard`] and comes back as a `Result` like everything else.
//!
//! The consequence to keep in mind: **no module outside this one calls `scap::`
//! directly.** A new call site added elsewhere is a new way for the app to die on a
//! Mac where the user clicked "Don't Allow".

use scap::Target;

/// Whether this OS can capture at all. scap's own check `.expect()`s on the version
/// string it parses, so even this is guarded.
pub fn supported() -> bool {
    guard("checking screen-capture support", scap::is_supported).unwrap_or(false)
}

/// Confirm we may capture, asking the OS once if it hasn't been asked.
///
/// Errors with instructions rather than a bare "denied": at the moment this fails
/// there is nothing the user can do inside Arbor, and an error that doesn't say
/// where to go leaves them with a recorder that simply refuses.
pub fn ensure_permission() -> Result<(), String> {
    if !supported() {
        return Err("screen capture isn't supported on this system".to_string());
    }
    if guard("checking screen-capture permission", scap::has_permission).unwrap_or(false) {
        return Ok(());
    }
    // Exactly one attempt. After a refusal the OS answers `false` without prompting,
    // so asking again in a loop would spin in silence.
    if guard("requesting screen-capture permission", scap::request_permission).unwrap_or(false) {
        return Ok(());
    }
    Err(denied_message())
}

/// Every capturable display and window, permission-checked and panic-safe.
pub fn targets() -> Result<Vec<Target>, String> {
    ensure_permission()?;
    // Guarded even though permission just checked out: `preflight` can report a grant
    // that the capture service then refuses (a stale TCC entry after the bundle is
    // rebuilt or moved does exactly this), and scap turns that refusal into a panic.
    guard("listing capture sources", scap::get_all_targets).map_err(with_permission_hint)
}

/// Run something that calls into scap, turning a panic into an error.
///
/// `AssertUnwindSafe` is honest here rather than a shortcut: on the unwinding path we
/// discard every value the closure touched and return an error, so no caller ever
/// observes half-updated state. The alternative — letting it unwind — kills the
/// capture thread and leaves the engine waiting on a channel nobody will ever send on.
pub fn guard<T>(what: &str, f: impl FnOnce() -> T) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f))
        .map_err(|payload| format!("{what} failed: {}", panic_text(payload.as_ref())))
}

/// Best-effort text out of a panic payload (`&str` and `String` cover everything the
/// standard macros produce; anything else is opaque by construction).
fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        return (*s).to_string();
    }
    if let Some(s) = payload.downcast_ref::<String>() {
        return s.clone();
    }
    "the capture backend panicked".to_string()
}

/// Append the permission instructions to a scap failure.
///
/// scap panics for essentially one reason — the OS refusing the capture service — and
/// the payload it carries is the OS's own message, which is localized and therefore
/// not something to pattern-match on. So the hint is offered rather than asserted:
/// the original text is kept in front of it, and the reader decides.
fn with_permission_hint(err: String) -> String {
    format!("{err}. This is almost always the screen-recording permission — {}", help())
}

/// "Permission refused" with the way out.
fn denied_message() -> String {
    format!("Arbor doesn't have permission to record the screen. {}", help())
}

/// Where to grant it, per platform. Written as a sentence because it is shown to the
/// user verbatim, in a toast and in the source picker.
#[cfg(target_os = "macos")]
fn help() -> &'static str {
    // The relaunch is not optional advice: macOS applies a newly granted capture
    // permission to the next launch of the app, not to the process that was refused.
    "open System Settings → Privacy & Security → Screen Recording (\"Screen & System \
     Audio Recording\" on newer macOS), switch Arbor on, then quit and reopen Arbor — \
     macOS only applies the change to a fresh launch."
}

#[cfg(target_os = "linux")]
fn help() -> &'static str {
    "your desktop asks for screen sharing through a portal dialog — allow it when it \
     appears, and check that a screen-sharing portal (xdg-desktop-portal) is installed."
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn help() -> &'static str {
    "check that screen capture isn't blocked by a system or group policy."
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panic_becomes_an_error_carrying_its_message() {
        let e = guard("doing the thing", || panic!("the user refused TCC permission")).unwrap_err();
        assert!(e.contains("doing the thing failed"), "the error names what failed: {e}");
        assert!(e.contains("refused TCC permission"), "the OS's own words survive: {e}");
    }

    #[test]
    fn a_value_passes_straight_through() {
        assert_eq!(guard("counting", || 41 + 1), Ok(42));
    }

    #[test]
    fn the_hint_keeps_the_original_message_in_front() {
        let out = with_permission_hint("listing capture sources failed: nope".to_string());
        assert!(out.starts_with("listing capture sources failed: nope"));
        assert!(out.contains("screen-recording permission"));
    }

    #[test]
    fn the_denial_message_says_where_to_go() {
        let m = denied_message();
        assert!(m.contains("permission to record the screen"));
        assert!(!help().is_empty(), "every platform has a way out to point at");
    }
}
