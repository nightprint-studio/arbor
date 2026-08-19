//! The screen-recording permission, asked by the process that owns the app identity.
//!
//! On macOS screen capture is gated by TCC, and a grant is recorded against the
//! **responsible process** — for a child like `tyto-be` that is normally the Arbor
//! bundle that spawned it. "Normally" is the problem: a helper that ends up
//! responsible for itself asks under its own identity, and the grant lands somewhere
//! the user can't find. Asking from here removes the word: the bundle asks for
//! itself, at the moment the user opened the recorder — which is when a permission
//! dialog is answerable rather than a surprise from a background process.
//!
//! No new dependency. CoreGraphics is a system framework every macOS app already
//! links, and both entry points are plain C returning `_Bool`.
//!
//! **The limit that shapes everything above this file: macOS shows the dialog once
//! per app, ever.** After a refusal `CGRequestScreenCaptureAccess` returns `false`
//! without showing anything, so there is no retry worth writing — only
//! [`open_privacy_settings`], and the relaunch macOS requires before a fresh grant
//! reaches a process that was already refused.

#[cfg(target_os = "macos")]
mod imp {
    #[link(name = "CoreGraphics", kind = "framework")]
    #[allow(non_snake_case)] // C names, kept verbatim so they're greppable against the SDK.
    extern "C" {
        /// Has this app been granted screen recording? Never prompts.
        fn CGPreflightScreenCaptureAccess() -> bool;
        /// Prompt for it — **once per app, ever**; `false` without a dialog after that.
        fn CGRequestScreenCaptureAccess() -> bool;
    }

    pub fn granted() -> bool {
        // SAFETY: a nullary C predicate from a system framework, no arguments to get
        // wrong and no pointers crossing the boundary.
        unsafe { CGPreflightScreenCaptureAccess() }
    }

    pub fn request() -> bool {
        // SAFETY: as above. Blocks while the system dialog is up — see the caller.
        unsafe { CGRequestScreenCaptureAccess() }
    }

    pub fn open_privacy_settings() -> bool {
        // The pane is "Screen Recording" through Sonoma and "Screen & System Audio
        // Recording" on newer macOS; the anchor is the same either way.
        // `status` rather than `spawn`: `open` hands off to LaunchServices and exits
        // immediately, so waiting costs nothing and leaves no zombie behind — and its
        // exit code is the only way to know whether the pane actually opened.
        std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    /// Windows (WGC) and Linux (PipeWire portal, which asks per session at capture
    /// time) have nothing for the app to hold up front.
    pub fn granted() -> bool {
        true
    }
    pub fn request() -> bool {
        true
    }
    /// No single pane to send anyone to.
    pub fn open_privacy_settings() -> bool {
        false
    }
}

/// What the shell can say about the screen-recording permission — enough to tell
/// three different failures apart, which the bare "denied" cannot.
#[derive(Debug, serde::Serialize)]
pub struct ScreenRecordingStatus {
    /// Whether **this** process — the app, the one that owns the bundle identity —
    /// has the permission. Distinct from whether `tyto-be` can capture: they are
    /// separate processes, and that difference is exactly one of the failures.
    pub granted: bool,
    /// Whether the running executable lives inside a `.app` bundle.
    pub bundled: bool,
    /// The running executable's path — the thing TCC actually recorded a decision
    /// about, which is not always the thing the user thinks they granted.
    pub executable: String,
    /// The likeliest cause when capture is refused anyway, or `None` when the plain
    /// "grant it in System Settings" message already covers it.
    pub hint: Option<String>,
}

/// Report the permission as the shell sees it, plus how Arbor was started.
///
/// This exists because "I granted it and it still says no" has three very different
/// causes and no way to tell them apart from inside the recorder: the grant may not
/// have reached this process yet, it may belong to a *different* binary than the one
/// running, or it may have been attributed to whatever launched Arbor rather than to
/// Arbor. The first is a relaunch, the second is a rebuild, and the third is a
/// different app entirely — advice for one is useless for the others.
pub fn status() -> ScreenRecordingStatus {
    let executable = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let bundled = executable.contains("/Contents/MacOS/");
    let granted = imp::granted();

    let hint = if !bundled && cfg!(target_os = "macos") {
        // The case the permission list makes look solved when it isn't: macOS records
        // a screen-recording decision against the *responsible* process, and for a
        // binary started from a terminal that is the terminal — which is why a
        // terminal emulator shows up in that list at all.
        Some(format!(
            "Arbor is running from {executable}, not from an installed app. macOS credits the              permission to whatever launched it, so if you started Arbor from a terminal it is              that terminal — Ghostty, iTerm, Terminal — that needs to be switched on in the same              list, and restarted afterwards."
        ))
    } else if granted {
        Some(
            "Arbor itself has the permission — it is the recorder's own process that is being              refused. Quit and reopen Arbor: a grant only reaches processes started after it."
                .to_string(),
        )
    } else {
        None
    };

    ScreenRecordingStatus { granted, bundled, executable, hint }
}

/// Take the user to the OS screen where the permission is granted. `false` when this
/// platform has nowhere to go, so the caller can say something useful instead of
/// leaving a button that quietly does nothing.
pub fn open_privacy_settings() -> bool {
    imp::open_privacy_settings()
}

/// Ask for the screen-recording permission unless it has already been answered.
///
/// **Blocks while the dialog is up** — for as long as the user takes to read it. It
/// must therefore never run on the main thread (the UI would freeze behind its own
/// dialog) nor on a runtime worker (landmine #1: a blocked worker is a worker the
/// reverse channel can't use). Its one caller is the detached thread Tyto already
/// spawns to bring up its backend.
///
/// Returns whether capture is allowed afterwards. Callers use it for logging, not for
/// control flow: `tyto-be` re-checks at the point it actually captures, which is the
/// only check that can't be stale.
pub fn request_if_needed() -> bool {
    imp::granted() || imp::request()
}

/// Open the OS privacy settings for screen recording. Returns `false` when there is
/// no such screen on this platform.
///
/// A command rather than a reverse-channel call: the frontend asking is the frontend
/// the shell already serves, and routing it through `tyto-be` would put OS-integration
/// glue in a headless process that has none.
#[tauri::command]
#[allow(clippy::unused_async)] // async keeps it off the main thread, like its neighbours.
pub async fn open_screen_recording_settings() -> bool {
    open_privacy_settings()
}

/// The shell's view of the screen-recording permission, for the recorder to show
/// beside its own refusal. See [`status`].
#[tauri::command]
#[allow(clippy::unused_async)]
pub async fn screen_recording_status() -> ScreenRecordingStatus {
    status()
}
