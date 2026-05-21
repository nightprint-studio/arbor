//! In-memory branding overrides — currently the app logo.
//!
//! A plugin can replace the default Arbor logo for the lifetime of the
//! session via `arbor.ui.set_branding{svg = ...}`. The override is stored
//! in `AppState.branding` so that:
//!   - the frontend reads it for the title bar, welcome screen, About modal;
//!   - backend code that emits self-contained artefacts (the HTML stats
//!     export) embeds the same SVG, so exports stay branded too.
//!
//! Branding is *RAM only*: nothing is persisted. A reload restores the
//! default Arbor identity unless the same plugin re-applies the override
//! during its `on_plugin_load` handler.

use std::sync::Mutex;

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct Branding {
    /// Inline SVG markup for the in-app mark (title bar, welcome screen,
    /// About modal, HTML stats export). `None` means "use default".
    pub logo_svg: Option<String>,
    /// Absolute path to a raster image (PNG / ICO) used as the OS-level
    /// window icon — the one shown in the taskbar / Alt-Tab list / window
    /// chrome on Windows and Linux. SVG is not accepted here because the
    /// platforms need a rasterised buffer; conversion happens at the
    /// plugin's discretion (Arbor doesn't bundle an SVG renderer).
    pub window_icon_path: Option<String>,
    /// Plugin that owns the current override (for diagnostics / unload cleanup).
    pub owner: Option<String>,
}

#[derive(Debug, Default)]
pub struct BrandingState {
    inner: Mutex<Branding>,
}

impl BrandingState {
    pub fn snapshot(&self) -> Branding {
        self.inner.lock().map(|g| g.clone()).unwrap_or_default()
    }

    /// Apply a branding update.  Each field is independent: passing `None`
    /// for a field leaves the previously-stored value alone, so a plugin
    /// can issue follow-up calls that only swap one aspect (e.g. switch
    /// the window icon mid-session without re-uploading the SVG).
    /// Use `clear` to drop everything in one shot.
    pub fn apply(
        &self,
        logo_svg:         Option<String>,
        window_icon_path: Option<String>,
        owner:            String,
    ) {
        if let Ok(mut g) = self.inner.lock() {
            if logo_svg.is_some()         { g.logo_svg         = logo_svg; }
            if window_icon_path.is_some() { g.window_icon_path = window_icon_path; }
            g.owner = Some(owner);
        }
    }

    /// Clear the override unconditionally, or only when `owner` matches the
    /// recorded owner. The latter prevents a plugin from accidentally
    /// dropping another plugin's branding when both apply on_plugin_load.
    /// Returns the previous state when something was actually cleared so
    /// the caller can decide whether to repaint.
    pub fn clear(&self, owner: Option<&str>) -> Option<Branding> {
        let mut g = self.inner.lock().ok()?;
        match (owner, &g.owner) {
            (Some(req), Some(cur)) if req != cur => None,
            _ => {
                if g.logo_svg.is_none() && g.window_icon_path.is_none() {
                    return None;
                }
                let prev = g.clone();
                g.logo_svg         = None;
                g.window_icon_path = None;
                g.owner            = None;
                Some(prev)
            }
        }
    }
}
