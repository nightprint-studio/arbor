//! macOS display + window geometry, straight from CoreGraphics.
//!
//! Everything here answers questions scap can't: **where** a display is on the
//! desktop, **how dense** it is, which display the pointer is on, and what windows
//! are actually on screen and who owns them. scap 0.0.8 exposes a display's id and
//! localized name and nothing else, and its window side is worse than thin — see the
//! note on [`windows`] below.
//!
//! No new dependency, deliberately: this is the same shape as
//! `src-tauri/src/window/screen_capture.rs` — CoreGraphics and CoreFoundation are
//! system frameworks every macOS process already links, and the entry points used
//! here take and return scalars, C strings and CF containers, with no Objective-C
//! message sends anywhere.
//!
//! **The two coordinate spaces, because mixing them is the whole class of bug this
//! module exists to close.** `CGDisplayBounds` works in *points*: one global,
//! top-left-origin plane across every display, which is also the space
//! `NSEvent.mouseLocation` and Tauri's `LogicalPosition` live in. The capture engine
//! and every crop rectangle work in *physical pixels*, which is points × that
//! display's backing scale — and the scale is **per display**, so a rectangle in
//! physical pixels means nothing until you know which display it belongs to. The
//! functions here return physical pixels (the capture side's unit) and carry the
//! `scale` along, so a caller can always get back to points.

use std::ffi::{c_char, c_void, CStr};

// ── CoreFoundation / CoreGraphics types ─────────────────────────────────────

type CGDirectDisplayID = u32;
type CGWindowID = u32;
/// Any `CFTypeRef`-shaped pointer (array, dictionary, number, string, display mode,
/// event). Opaque on purpose: nothing here inspects one except through the C API.
type CFRef = *const c_void;

#[repr(C)]
#[derive(Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CGSize {
    width: f64,
    height: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}

/// `kCFNumberSInt64Type` — the widest integer `CFNumberGetValue` will convert into,
/// so one accessor covers every integer field in a window dictionary.
const CF_NUMBER_SINT64: i32 = 4;
/// `kCFNumberFloat64Type`, for the one fractional field we read (alpha).
const CF_NUMBER_FLOAT64: i32 = 6;
/// `kCFStringEncodingUTF8`.
const CF_STRING_UTF8: u32 = 0x0800_0100;

/// `kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements` — the
/// windows a person can actually see, minus the desktop picture and the icon layer.
const WINDOW_LIST_ON_SCREEN: u32 = (1 << 0) | (1 << 4);
/// `kCGWindowListOptionIncludingWindow`, used with a window id to ask about one.
const WINDOW_LIST_ONE: u32 = 1 << 3;
/// `kCGNullWindowID`.
const NULL_WINDOW: u32 = 0;

#[link(name = "CoreGraphics", kind = "framework")]
// C names, kept verbatim so they're greppable against the SDK — including the
// `kCG…` globals, which are lowercase-k by Apple's convention and not ours.
#[allow(non_snake_case, non_upper_case_globals)]
extern "C" {
    fn CGMainDisplayID() -> CGDirectDisplayID;
    fn CGGetActiveDisplayList(max: u32, displays: *mut CGDirectDisplayID, count: *mut u32) -> i32;
    /// The display's rectangle in the global **points** plane.
    fn CGDisplayBounds(display: CGDirectDisplayID) -> CGRect;
    fn CGDisplayCopyDisplayMode(display: CGDirectDisplayID) -> CFRef;
    fn CGDisplayModeGetWidth(mode: CFRef) -> usize;
    fn CGDisplayModeGetPixelWidth(mode: CFRef) -> usize;
    fn CGDisplayModeRelease(mode: CFRef);
    /// A fresh event carrying the current pointer location. `source` may be null.
    fn CGEventCreate(source: CFRef) -> CFRef;
    /// Pointer position in the global **points** plane (top-left origin).
    fn CGEventGetLocation(event: CFRef) -> CGPoint;
    fn CGWindowListCopyWindowInfo(option: u32, relative_to: CGWindowID) -> CFRef;
    /// Decodes a `kCGWindowBounds` dictionary into a rect (in points).
    fn CGRectMakeWithDictionaryRepresentation(dict: CFRef, rect: *mut CGRect) -> bool;

    static kCGWindowNumber: CFRef;
    static kCGWindowOwnerName: CFRef;
    static kCGWindowName: CFRef;
    static kCGWindowLayer: CFRef;
    static kCGWindowBounds: CFRef;
    static kCGWindowAlpha: CFRef;
}

#[link(name = "CoreFoundation", kind = "framework")]
#[allow(non_snake_case)]
extern "C" {
    fn CFRelease(cf: CFRef);
    fn CFArrayGetCount(array: CFRef) -> isize;
    fn CFArrayGetValueAtIndex(array: CFRef, index: isize) -> CFRef;
    fn CFDictionaryGetValue(dict: CFRef, key: CFRef) -> CFRef;
    fn CFNumberGetValue(number: CFRef, the_type: i32, value: *mut c_void) -> bool;
    fn CFStringGetCString(string: CFRef, buffer: *mut c_char, size: isize, encoding: u32) -> bool;
}

// ── Displays ────────────────────────────────────────────────────────────────

/// One display's geometry, in **physical pixels** (points × [`Self::scale`]), plus
/// its scale so a caller can convert back to points.
#[derive(Clone, Copy, Debug)]
pub struct DisplayGeometry {
    /// `CGDirectDisplayID` — the same number scap uses, so `mon-<id>` matches.
    pub id: u32,
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub scale: f64,
    pub primary: bool,
}

/// Every active display, in the order CoreGraphics reports them (the main display
/// first). Empty only if the window server has nothing to say, which in practice
/// means no display at all.
pub fn displays() -> Vec<DisplayGeometry> {
    const MAX: u32 = 16;
    let mut ids = [0u32; MAX as usize];
    let mut count = 0u32;
    // SAFETY: a fixed-size buffer whose capacity is passed alongside it; CoreGraphics
    // writes at most `MAX` ids and reports how many through `count`.
    let err = unsafe { CGGetActiveDisplayList(MAX, ids.as_mut_ptr(), &mut count) };
    if err != 0 {
        return Vec::new();
    }
    // SAFETY: nullary C accessor.
    let main = unsafe { CGMainDisplayID() };
    ids[..count.min(MAX) as usize]
        .iter()
        .map(|&id| geometry(id, main))
        .collect()
}

/// Geometry of one display, by `CGDirectDisplayID`.
pub fn display(id: u32) -> Option<DisplayGeometry> {
    let all = displays();
    all.into_iter().find(|d| d.id == id)
}

/// The display the pointer is currently on, or `None` when the pointer sits in the
/// gap no display covers (possible with mismatched arrangements).
pub fn display_at_cursor() -> Option<DisplayGeometry> {
    let p = cursor_point()?;
    displays()
        .into_iter()
        .find(|d| contains(d, p.0, p.1))
}

/// Pointer position in **points**. Goes through a throwaway `CGEvent` because that
/// is the only way to ask CoreGraphics (rather than AppKit, which would mean
/// Objective-C) where the pointer is.
fn cursor_point() -> Option<(f64, f64)> {
    // SAFETY: `CGEventCreate(null)` is the documented way to build an event bound to
    // the current state; it returns null only under memory pressure, which is checked.
    unsafe {
        let ev = CGEventCreate(std::ptr::null());
        if ev.is_null() {
            return None;
        }
        let p = CGEventGetLocation(ev);
        CFRelease(ev);
        Some((p.x, p.y))
    }
}

/// Is the point (in points) inside this display? The geometry is stored in physical
/// pixels, so it is converted back for the test.
fn contains(d: &DisplayGeometry, px: f64, py: f64) -> bool {
    let (x, y) = (d.x as f64 / d.scale, d.y as f64 / d.scale);
    let (w, h) = (d.w as f64 / d.scale, d.h as f64 / d.scale);
    px >= x && px < x + w && py >= y && py < y + h
}

fn geometry(id: CGDirectDisplayID, main: CGDirectDisplayID) -> DisplayGeometry {
    // SAFETY: a C accessor returning a POD rect for a display id CoreGraphics gave us.
    let b = unsafe { CGDisplayBounds(id) };
    let scale = backing_scale(id);
    DisplayGeometry {
        id,
        x: (b.origin.x * scale).round() as i32,
        y: (b.origin.y * scale).round() as i32,
        w: (b.size.width * scale).round() as u32,
        h: (b.size.height * scale).round() as u32,
        scale,
        primary: id == main,
    }
}

/// The display's backing scale: framebuffer pixels per point, which is what AppKit
/// calls `backingScaleFactor` and what the capture engine's frames are sized in.
///
/// A HiDPI mode always reports a framebuffer twice its point size (macOS renders at
/// 2× and downsamples to the panel, including for the "More Space" modes), so this
/// ratio is 2.0 there and 1.0 on a plain display.
fn backing_scale(id: CGDirectDisplayID) -> f64 {
    // SAFETY: `CGDisplayCopyDisplayMode` follows the Copy rule — the mode is ours to
    // release, which happens on every path below.
    unsafe {
        let mode = CGDisplayCopyDisplayMode(id);
        if mode.is_null() {
            return 1.0;
        }
        let points = CGDisplayModeGetWidth(mode);
        let pixels = CGDisplayModeGetPixelWidth(mode);
        CGDisplayModeRelease(mode);
        if points == 0 || pixels == 0 {
            return 1.0;
        }
        pixels as f64 / points as f64
    }
}

// ── Windows ─────────────────────────────────────────────────────────────────

/// One on-screen window: who owns it, what it's called, and where it is in
/// **points** (the space `CGWindowBounds` reports, left as-is because the caller
/// decides which display's scale applies).
#[derive(Clone, Debug)]
pub struct WindowInfo {
    /// `CGWindowID` — the same number scap uses, so `win-<id>` matches.
    pub id: u32,
    pub title: String,
    /// The owning application's name ("Safari", "Code") — what scap doesn't have.
    pub app: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// The windows worth offering as a capture target.
///
/// scap's own macOS enumeration is every entry of `SCShareableContent.windows` that
/// carries a title, which is not a list of windows a person recognises: it keeps the
/// off-screen ones, the menu-bar extras, the one-pixel helpers and the sheets, and it
/// has no idea which application owns any of them. The window server knows all of it,
/// so the list is built from `CGWindowListCopyWindowInfo` instead and filtered on the
/// three things that make an entry meaningless:
///
/// * **layer ≠ 0** — the normal-window layer. Everything above it is chrome (the Dock
///   is 20, menu-bar extras 25, screen-saver windows higher still) and nothing below
///   it is a window the user thinks of as one.
/// * **fully transparent** — the invisible overlays apps keep around for hit-testing.
/// * **smaller than 64 points on a side** — tooltips, badges and the status-item
///   scratch windows.
///
/// A window with no name is dropped too: with the screen-recording permission in hand
/// `kCGWindowName` is populated, so an empty one is a window with nothing to show for
/// itself in a picker.
pub fn windows() -> Vec<WindowInfo> {
    // SAFETY: `CopyWindowInfo` follows the Copy rule; released on both paths.
    unsafe {
        let list = CGWindowListCopyWindowInfo(WINDOW_LIST_ON_SCREEN, NULL_WINDOW);
        if list.is_null() {
            return Vec::new();
        }
        let count = CFArrayGetCount(list);
        let mut out = Vec::with_capacity(count.max(0) as usize);
        for i in 0..count {
            let dict = CFArrayGetValueAtIndex(list, i);
            if let Some(w) = read_window(dict) {
                out.push(w);
            }
        }
        CFRelease(list);
        out
    }
}

/// Where one window is right now, by `CGWindowID`. `None` once it has closed.
pub fn window(id: u32) -> Option<WindowInfo> {
    // SAFETY: as [`windows`]; the single-window option needs the id as the relative
    // window, and the result is a one-element array (or empty if it is gone).
    unsafe {
        let list = CGWindowListCopyWindowInfo(WINDOW_LIST_ONE, id);
        if list.is_null() {
            return None;
        }
        let found = (CFArrayGetCount(list) > 0)
            .then(|| read_window_raw(CFArrayGetValueAtIndex(list, 0)))
            .flatten();
        CFRelease(list);
        found
    }
}

/// Windows narrower or shorter than this (in points) are never a capture target.
const MIN_WINDOW_SIDE: f64 = 64.0;

/// A window dictionary, filtered for the picker. See [`windows`] for the three rules.
unsafe fn read_window(dict: CFRef) -> Option<WindowInfo> {
    // A missing layer is read as the normal one: better to offer a window we can't
    // classify than to hide it.
    if dict_int(dict, kCGWindowLayer).unwrap_or(0) != 0 {
        return None;
    }
    // Absent alpha means opaque — the key is only present when the window server has
    // a value to report.
    if dict_float(dict, kCGWindowAlpha).unwrap_or(1.0) <= 0.01 {
        return None;
    }
    let w = read_window_raw(dict)?;
    if w.title.trim().is_empty() || w.w < MIN_WINDOW_SIDE || w.h < MIN_WINDOW_SIDE {
        return None;
    }
    Some(w)
}

/// The fields of a window dictionary, with no judgement about whether it's useful —
/// [`window`] wants the geometry of whatever it was asked about, filters included.
unsafe fn read_window_raw(dict: CFRef) -> Option<WindowInfo> {
    if dict.is_null() {
        return None;
    }
    let id = dict_int(dict, kCGWindowNumber)? as u32;
    let bounds = dict_rect(dict, kCGWindowBounds)?;
    Some(WindowInfo {
        id,
        title: dict_string(dict, kCGWindowName).unwrap_or_default(),
        app: dict_string(dict, kCGWindowOwnerName).unwrap_or_default(),
        x: bounds.origin.x,
        y: bounds.origin.y,
        w: bounds.size.width,
        h: bounds.size.height,
    })
}

unsafe fn dict_int(dict: CFRef, key: CFRef) -> Option<i64> {
    let v = CFDictionaryGetValue(dict, key);
    if v.is_null() {
        return None;
    }
    let mut n: i64 = 0;
    CFNumberGetValue(v, CF_NUMBER_SINT64, (&mut n as *mut i64).cast()).then_some(n)
}

unsafe fn dict_float(dict: CFRef, key: CFRef) -> Option<f64> {
    let v = CFDictionaryGetValue(dict, key);
    if v.is_null() {
        return None;
    }
    let mut n: f64 = 0.0;
    CFNumberGetValue(v, CF_NUMBER_FLOAT64, (&mut n as *mut f64).cast()).then_some(n)
}

unsafe fn dict_rect(dict: CFRef, key: CFRef) -> Option<CGRect> {
    let v = CFDictionaryGetValue(dict, key);
    if v.is_null() {
        return None;
    }
    let mut r = CGRect {
        origin: CGPoint { x: 0.0, y: 0.0 },
        size: CGSize { width: 0.0, height: 0.0 },
    };
    CGRectMakeWithDictionaryRepresentation(v, &mut r).then_some(r)
}

/// A dictionary's string value as a Rust `String`. The buffer is generous rather than
/// exact: `CFStringGetCString` fails outright when it doesn't fit, and a window title
/// that long is already past anything a picker will show.
unsafe fn dict_string(dict: CFRef, key: CFRef) -> Option<String> {
    let v = CFDictionaryGetValue(dict, key);
    if v.is_null() {
        return None;
    }
    let mut buf = [0 as c_char; 512];
    if !CFStringGetCString(v, buf.as_mut_ptr(), buf.len() as isize, CF_STRING_UTF8) {
        return None;
    }
    Some(CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned())
}
