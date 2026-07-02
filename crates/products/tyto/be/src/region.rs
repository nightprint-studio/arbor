//! `region` domain — resolve a CSS-pixel drag on a monitor into the physical-pixel
//! crop rectangle, using the monitor's real DPI scale (Win32). The crop rect is
//! **display-local** (0-based within the captured monitor), which is what the scap
//! region producer crops against.

use serde::{Deserialize, Serialize};
use tyto_core::prelude::TytoState;

/// A rectangle in pixels — `x`/`y` top-left, `w`/`h` size.
#[derive(Deserialize, Serialize, Clone, Copy)]
pub struct PixelRect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

/// Parameters for [`select_region`].
#[derive(Deserialize)]
pub struct SelectRegionArgs {
    pub monitor_id: String,
    pub css: PixelRect,
}

/// The resolved region selection.
#[derive(Serialize)]
pub struct RegionSelection {
    pub css: PixelRect,
    pub physical: PixelRect,
    pub scale_factor: f64,
}

/// Resolve a region selection from a CSS-pixel drag on a monitor. `physical` is
/// **display-local** (0-based within that monitor) — the scap region producer crops
/// the captured display at exactly this rect, so no virtual-desktop origin is added.
#[arbor_rpc::handler]
fn select_region(_state: &TytoState, args: SelectRegionArgs) -> Result<RegionSelection, String> {
    let (_ox, _oy, scale) = crate::capture::source::monitor_geometry(&args.monitor_id);
    let physical = PixelRect {
        x: (args.css.x as f64 * scale) as i32,
        y: (args.css.y as f64 * scale) as i32,
        w: (args.css.w as f64 * scale) as u32,
        h: (args.css.h as f64 * scale) as u32,
    };
    Ok(RegionSelection { css: args.css, physical, scale_factor: scale })
}

/// Clear the current region selection.
#[arbor_rpc::handler]
fn clear_region(_state: &TytoState) -> Result<(), String> {
    Ok(())
}

/// Enumerate the foreground window's UI element rects on `monitor_id`, in monitor-local
/// CSS pixels, for the overlay's **smart** pick (hover an element → snap to it). Empty
/// off Windows or when the app exposes no accessibility. Captured here (before the
/// overlay covers the screen) so hover hit-tests these rects rather than the overlay.
#[arbor_rpc::handler]
fn enumerate_ui_elements(_state: &TytoState, monitor_id: String) -> Result<Vec<PixelRect>, String> {
    Ok(crate::capture::uia::enumerate_elements(&monitor_id))
}

/// One hover-target window in the picker overlay, in **virtual-desktop CSS pixels**.
/// `id` = `win-<hwnd>` (same formula as the source picker, so the two ids match).
#[derive(Serialize)]
pub struct WindowPickRect {
    pub id: String,
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

/// One hover-target monitor in the picker overlay, in **virtual-desktop CSS pixels**.
/// `id` = `mon-<hmonitor>` (same formula as the source picker).
#[derive(Serialize)]
pub struct MonitorPickRect {
    pub id: String,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

/// All hover targets for the on-screen window/display picker overlay, in
/// virtual-desktop CSS pixels.
#[derive(Serialize)]
pub struct PickTargets {
    pub windows: Vec<WindowPickRect>,
    pub monitors: Vec<MonitorPickRect>,
}

/// Enumerate whole-window + whole-monitor hover targets for the picker overlay, in
/// **virtual-desktop CSS pixels** (origin-subtracted, primary-monitor scaled). Empty
/// off Windows.
///
/// Assumes **uniform DPI** across monitors: every rect is converted with the primary
/// monitor's scale. Mixed-DPI multi-monitor is a known limitation.
#[arbor_rpc::handler]
fn enumerate_pick_targets(_state: &TytoState) -> Result<PickTargets, String> {
    Ok(crate::capture::winpick::enumerate_pick_targets())
}

/// A frozen desktop snapshot for the region selector: the PNG path + the target
/// monitor's **logical** bounds (for sizing the opaque selection window) + scale.
#[derive(Serialize)]
pub struct FrozenFrame {
    pub path: String,
    pub monitor_id: String,
    /// Logical top-left + size (physical ÷ scale) — what Tauri positions/sizes with.
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale: f64,
}

/// Grab a frozen screenshot of a monitor (default: primary) to a temp PNG for the
/// region-selection overlay. Multi-monitor union is a follow-up — this is one
/// monitor.
#[arbor_rpc::handler]
fn freeze_screen(_state: &TytoState, monitor_id: Option<String>) -> Result<FrozenFrame, String> {
    // Default to the primary monitor (else the first) when none is given.
    let monitor_id = match monitor_id {
        Some(id) => id,
        None => {
            let mons = crate::capture::source::list_monitors();
            mons.iter()
                .find(|m| m.primary)
                .or_else(|| mons.first())
                .map(|m| m.id.clone())
                .ok_or_else(|| "no monitor available".to_string())?
        }
    };
    let (px, py, scale) = crate::capture::source::monitor_geometry(&monitor_id);
    let scale = scale.max(0.1);

    // Full-monitor still for the selection backdrop. On Windows use GDI (a plain
    // BitBlt) so no WGC "being captured" yellow border flashes; elsewhere fall back to
    // the scap one-shot. The exact pixel resolution only affects the backdrop's
    // sharpness — the selection math resolves against the live scap capture, not this.
    #[cfg(target_os = "windows")]
    let (rgba, pw, ph) = crate::capture::gdi::capture_monitor_rgba(&monitor_id)?;
    #[cfg(not(target_os = "windows"))]
    let (rgba, pw, ph) = {
        let target = crate::capture::CaptureTarget::resolve("monitor", Some(&monitor_id), None)?;
        target.grab_rgba()?
    };
    let img = image::RgbaImage::from_raw(pw, ph, rgba)
        .ok_or_else(|| "freeze: buffer/size mismatch".to_string())?;
    let path = std::env::temp_dir().join(format!("tyto-region-{}.png", uuid::Uuid::new_v4().simple()));
    img.save(&path).map_err(|e| e.to_string())?;

    Ok(FrozenFrame {
        path: path.to_string_lossy().to_string(),
        monitor_id,
        x: (px as f64 / scale).round() as i32,
        y: (py as f64 / scale).round() as i32,
        width: (pw as f64 / scale).round() as u32,
        height: (ph as f64 / scale).round() as u32,
        scale,
    })
}

/// Grab a frozen screenshot of the WHOLE virtual desktop (all monitors, union rect)
/// to a temp PNG — the backdrop for the on-screen window/display picker overlay.
///
/// `monitor_id` is the sentinel `"virtual"`; `x`/`y`/`width`/`height` are the overlay's
/// **logical** (CSS) bounds (physical ÷ primary scale) — what Tauri positions/sizes the
/// opaque picker window with. `scale` is the primary monitor's DPI scale.
///
/// Assumes **uniform DPI** across monitors (primary scale for the whole conversion);
/// mixed-DPI multi-monitor is a known limitation. Windows-only: elsewhere returns an
/// error (the on-screen picker targets Windows first).
#[arbor_rpc::handler]
fn freeze_virtual_desktop(_state: &TytoState) -> Result<FrozenFrame, String> {
    #[cfg(target_os = "windows")]
    {
        // Same virtual-desktop bounds + primary scale the pick-target enumeration uses,
        // so the freeze and the hover rects share one coordinate space.
        let geom = crate::capture::winpick::virtual_desktop_geometry();
        let scale = geom.scale.max(0.1);

        let (rgba, pw, ph) = crate::capture::gdi::capture_virtual_desktop_rgba()?;
        let img = image::RgbaImage::from_raw(pw, ph, rgba)
            .ok_or_else(|| "freeze: buffer/size mismatch".to_string())?;
        let path = std::env::temp_dir().join(format!("tyto-region-{}.png", uuid::Uuid::new_v4().simple()));
        img.save(&path).map_err(|e| e.to_string())?;

        Ok(FrozenFrame {
            path: path.to_string_lossy().to_string(),
            monitor_id: "virtual".to_string(),
            x: (geom.left as f64 / scale).round() as i32,
            y: (geom.top as f64 / scale).round() as i32,
            width: (pw as f64 / scale).round() as u32,
            height: (ph as f64 / scale).round() as u32,
            scale,
        })
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("freeze_virtual_desktop: Windows-only".to_string())
    }
}
