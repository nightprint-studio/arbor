//! The two macOS manners of a window that acts as a **full-screen overlay**, neither
//! of which Tauri exposes: standing above the system chrome, and not keeping the app
//! the user was in pushed into the background.
//!
//! Both end up talking to AppKit directly. No new dependency, same shape as
//! [`super::screen_capture`]: the selectors used here take one scalar and return one,
//! which is the easiest possible case for the `objc_msgSend` cast — the supported way
//! to send a message without an Objective-C binding crate. The cast machinery lives
//! once, in [`imp`], so a third manner doesn't bring a third copy of it.
//!
//! ⚠️ AppKit is main-thread-only and every caller here arrives from an async command or
//! a global-shortcut handler, so everything hops through `run_on_main_thread`. That hop
//! is also what keeps the order right: the main thread runs the queue FIFO, so a
//! "remember" posted before a window is shown really does see what was in front, and an
//! "activate" posted after a hide really does happen once the overlay is gone.
//!
//! ## Standing above the chrome
//!
//! `set_always_on_top` raises a window to AppKit's *floating* level (3), and the system
//! chrome lives higher: the Dock at 20, the menu bar at 24, the menu-bar extras at 25.
//! So Tyto's Snip selector, grown to cover a whole display, was drawn *under* the strip
//! at the top of it — the real menu bar of the newly-focused app sitting on top of a
//! frozen snapshot of the previous one's, with the selector's toolbar clipped behind it.
//!
//! **The level is deliberately not the maximum.** `CGShieldingWindowLevel()` (what
//! Apple's own `screencapture` uses) sits above *everything*, system alerts included,
//! and a window there that failed to paint its own way out is a window the user cannot
//! get past. One above the menu-bar extras covers all the chrome that was in the way
//! while leaving pop-up menus (101) and the screen saver (1000) above us.
//!
//! ## Giving the app back
//!
//! A quick capture is summoned from inside whatever the user was working in, and
//! showing the overlay **activates Arbor**: tao's `set_focus` is
//! `makeKeyAndOrderFront:` *plus* `activateIgnoringOtherApps:`, and that is not
//! optional — on macOS a plain `NSWindow` can only become key if its application is
//! active, and the selector is useless without the keyboard. (The documented exception
//! is an `NSPanel` with `NSWindowStyleMaskNonactivatingPanel`, which Tauri cannot
//! create and which would be wrong for this window anyway: it is also Tyto's control
//! panel, and a panel that can never bring its app forward has no working menus.)
//!
//! So the activation stays and is **undone**: the app that was in front is remembered
//! before the overlay appears and activated again when the overlay is dismissed. The
//! app is remembered by **pid**, not by holding the `NSRunningApplication`: a number
//! needs no retain, no release and no thought about what happens if the app quits
//! meanwhile — `runningApplicationWithProcessIdentifier:` simply answers nil, and
//! nothing is activated.

use tauri::{AppHandle, WebviewWindow};

/// `NSStatusWindowLevel + 1` — one above the menu-bar extras, which is one above the
/// menu bar (24) and well above the Dock (20). See the module doc for why not higher.
#[cfg(target_os = "macos")]
const ABOVE_SYSTEM_CHROME: isize = 26;

/// `NSNormalWindowLevel` — where an ordinary window lives.
#[cfg(target_os = "macos")]
const NORMAL: isize = 0;

/// `NSFloatingWindowLevel` — what `set_always_on_top(true)` gives a window.
#[cfg(target_os = "macos")]
const FLOATING: isize = 3;

/// The pid of the application that was in front when the overlay was summoned, or `0`
/// for "nothing to give back" (we were already in front, or it has been consumed).
/// `Relaxed` is right: one number, written and read by whoever gets there first, with
/// no other state ordered against it.
#[cfg(target_os = "macos")]
static PREVIOUS_APP: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

/// Raise `w` above the menu bar and the Dock (`true`), or hand it back to the ordinary
/// window levels (`false`, staying floating when `stays_on_top`).
///
/// Call it **after** `set_always_on_top`, which sets the level itself and would
/// otherwise undo this. A no-op off macOS, where an always-on-top window is already
/// above the taskbar.
pub fn set_above_system_chrome(w: &WebviewWindow, above: bool, stays_on_top: bool) {
    #[cfg(target_os = "macos")]
    {
        let level = if above {
            ABOVE_SYSTEM_CHROME
        } else if stays_on_top {
            FLOATING
        } else {
            NORMAL
        };
        let win = w.clone();
        // The pointer can't cross a thread boundary, so the window does and the
        // pointer is taken on the other side.
        if let Err(e) = w.run_on_main_thread(move || {
            if let Ok(ns_window) = win.ns_window() {
                // SAFETY: a live `NSWindow` from Tauri, on the main thread, sent a
                // selector that takes an `NSInteger` and returns nothing.
                unsafe { imp::set_window_level(ns_window, level) };
            }
        }) {
            tracing::warn!("couldn't set the window level: {e}");
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (w, above, stays_on_top);
    }
}

/// Remember which application is in front, to be activated again by
/// [`restore_frontmost_app`].
///
/// Call it at the entry point of the overlay, **before** anything shows or focuses a
/// window — by the time the window is up, Arbor is already the answer. Remembering
/// Arbor itself is pointedly not done: there would be nothing to give back, and
/// activating ourselves on dismiss would be a flicker for no reason.
pub fn remember_frontmost_app(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    {
        let ours = std::process::id() as i32;
        let _ = app.run_on_main_thread(move || {
            // SAFETY: AppKit accessors on the main thread; nil is checked inside.
            let pid = unsafe { imp::frontmost_app_pid() };
            let keep = if pid == ours { 0 } else { pid };
            PREVIOUS_APP.store(keep, std::sync::atomic::Ordering::Relaxed);
        });
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
    }
}

/// Drop whatever [`remember_frontmost_app`] remembered, without activating it.
///
/// For the paths that bring Arbor's own window forward on purpose — expanding the
/// selector into the full control panel, finishing a recording. The user ended up *in*
/// Arbor, so there is nothing to hand back, and a later dismiss must not resurrect an
/// app they left minutes ago.
pub fn forget_frontmost_app() {
    #[cfg(target_os = "macos")]
    {
        PREVIOUS_APP.store(0, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Activate the application remembered by [`remember_frontmost_app`], and forget it.
///
/// Take-and-clear, so it can only be spent once: a dismiss hands the app back, and a
/// second dismiss with nothing remembered does nothing rather than reaching for a stale
/// pid. Call it **after** the overlay has been hidden — activating another app from
/// under a window that is still up just puts Arbor straight back in front.
pub fn restore_frontmost_app(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    {
        let pid = PREVIOUS_APP.swap(0, std::sync::atomic::Ordering::Relaxed);
        if pid == 0 {
            return;
        }
        let _ = app.run_on_main_thread(move || {
            // SAFETY: AppKit accessors on the main thread; a pid that no longer belongs
            // to a running application resolves to nil and is dropped there.
            unsafe { imp::activate_app(pid) };
        });
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use std::ffi::{c_char, c_void};

    type Id = *mut c_void;
    type Class = *const c_void;
    type Sel = *const c_void;

    #[link(name = "objc", kind = "dylib")]
    #[allow(non_snake_case)] // C names, verbatim so they're greppable against the runtime.
    extern "C" {
        fn sel_registerName(name: *const c_char) -> Sel;
        fn objc_getClass(name: *const c_char) -> Class;
        /// Declared nullary **only to take its address**. `objc_msgSend` has no single
        /// signature — the caller is required to cast it to the one of the message it
        /// is sending, which is what the `send_*` helpers below do. Calling it through
        /// this declaration would be wrong, and nothing here does.
        fn objc_msgSend();
    }

    /// `NSWorkspace` and `NSRunningApplication` are looked up by name rather than
    /// linked: AppKit is already loaded (Tauri's own window layer needs it), so the
    /// classes are registered, and asking the runtime keeps this file free of a
    /// framework dependency it would be the only user of.
    fn class(name: &[u8]) -> Class {
        // SAFETY: a NUL-terminated literal; an unknown name returns null, which every
        // caller checks.
        unsafe { objc_getClass(name.as_ptr().cast()) }
    }

    fn sel(name: &[u8]) -> Sel {
        // SAFETY: as above. `sel_registerName` interns the name and never fails for a
        // well-formed one.
        unsafe { sel_registerName(name.as_ptr().cast()) }
    }

    // ── `objc_msgSend`, once per signature we actually send ──────────────────
    //
    // One helper per shape, because that is the contract: the cast has to match the
    // method's own types or the arguments land in the wrong registers.

    /// `[receiver sel]` returning an object.
    unsafe fn send_id(receiver: *const c_void, sel: Sel) -> Id {
        let f: unsafe extern "C" fn(*const c_void, Sel) -> Id =
            std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        f(receiver, sel)
    }

    /// `[receiver sel]` returning a 32-bit integer (`pid_t`).
    unsafe fn send_i32(receiver: *const c_void, sel: Sel) -> i32 {
        let f: unsafe extern "C" fn(*const c_void, Sel) -> i32 =
            std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        f(receiver, sel)
    }

    /// `[receiver sel: arg]` with a 32-bit integer, returning an object.
    unsafe fn send_id_i32(receiver: *const c_void, sel: Sel, arg: i32) -> Id {
        let f: unsafe extern "C" fn(*const c_void, Sel, i32) -> Id =
            std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        f(receiver, sel, arg)
    }

    /// `[receiver sel: arg]` with a word-sized argument, discarding the result.
    ///
    /// Covers both a `void` return and a `BOOL` we don't read — the caller ignores the
    /// return register either way. One signedness serves both messages sent through it:
    /// `setLevel:` takes a signed `NSInteger`, and the unsigned option mask of
    /// `activateWithOptions:` is only ever `0` here, where the two agree bit for bit.
    unsafe fn send_void_word(receiver: *const c_void, sel: Sel, arg: isize) {
        let f: unsafe extern "C" fn(*const c_void, Sel, isize) =
            std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        f(receiver, sel, arg)
    }

    // ── The three messages ──────────────────────────────────────────────────

    /// `[ns_window setLevel: level]`.
    ///
    /// # Safety
    /// `ns_window` must be a live `NSWindow`, and this must run on the main thread.
    pub unsafe fn set_window_level(ns_window: *mut c_void, level: isize) {
        let s = sel(b"setLevel:\0");
        if s.is_null() {
            return;
        }
        send_void_word(ns_window, s, level);
    }

    /// `[[[NSWorkspace sharedWorkspace] frontmostApplication] processIdentifier]`, or
    /// `0` when there is no frontmost application (nothing is active, or AppKit has
    /// nothing to report yet).
    ///
    /// # Safety
    /// Must run on the main thread.
    pub unsafe fn frontmost_app_pid() -> i32 {
        let workspace_class = class(b"NSWorkspace\0");
        if workspace_class.is_null() {
            return 0;
        }
        let workspace = send_id(workspace_class, sel(b"sharedWorkspace\0"));
        if workspace.is_null() {
            return 0;
        }
        let front = send_id(workspace, sel(b"frontmostApplication\0"));
        if front.is_null() {
            return 0;
        }
        send_i32(front, sel(b"processIdentifier\0"))
    }

    /// `[[NSRunningApplication runningApplicationWithProcessIdentifier: pid]
    /// activateWithOptions: 0]` — hand the front back to that application.
    ///
    /// Options `0`, not `NSApplicationActivateIgnoringOtherApps`: this is the *yielding*
    /// case, one active app letting another take over, which is the one macOS still
    /// honours — "ignoring other apps" is the stealing case, which recent systems are
    /// free to refuse.
    ///
    /// # Safety
    /// Must run on the main thread.
    pub unsafe fn activate_app(pid: i32) {
        let app_class = class(b"NSRunningApplication\0");
        if app_class.is_null() {
            return;
        }
        let app = send_id_i32(
            app_class,
            sel(b"runningApplicationWithProcessIdentifier:\0"),
            pid,
        );
        if app.is_null() {
            // It quit while the overlay was up. Nothing to give back.
            return;
        }
        send_void_word(app, sel(b"activateWithOptions:\0"), 0);
    }
}
