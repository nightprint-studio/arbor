//! `agent` domain — the handlers that exist for an AI client rather than for the UI.
//!
//! The tyto verbs the frontend uses are shaped for a picker: `take_screenshot` saves a
//! file and returns its path, because the next thing the UI does is show it in the
//! library. A model cannot open a path. It needs the pixels, in its own context, small
//! enough not to cost more than the answer is worth.
//!
//! So this module is not a wrapper around the UI handlers — it is the same capture
//! engine addressed differently: **by window title instead of by opaque id**, and
//! **returning bytes instead of a filename**. Both differences are the same idea, that
//! an agent addresses the world by name and reads it by value.

use base64::Engine as _;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tyto_core::prelude::TytoState;

use crate::capture::{self, CaptureTarget};

/// The long edge a screenshot is scaled down to before it is sent.
///
/// A 4K monitor is 8.8 megapixels; as a PNG in base64 that is several megabytes and,
/// once tokenized, more context than any answer it enables. Vision models resize
/// aggressively on their own side anyway, so sending the full frame buys nothing and
/// costs everything. 1568px is the long edge below which no further downscaling
/// happens for Claude's vision path — the largest size that isn't wasted.
const DEFAULT_MAX_EDGE: u32 = 1568;

/// Hard ceiling on what a caller may ask for, so `max_edge: 100000` can't turn into a
/// multi-megabyte answer by request.
const ABSOLUTE_MAX_EDGE: u32 = 4096;

/// An image handed back inline. The shape `ToolOutput::Image` expects.
#[derive(Debug, Serialize)]
pub struct InlineImage {
    /// Always `image/png` today — lossless, and the alpha survives a freehand mask.
    pub mime_type: String,
    /// Base64, no data-URI prefix.
    pub data: String,
    /// Pixel dimensions of what was actually sent, after downscaling.
    pub width: u32,
    pub height: u32,
    /// The dimensions before downscaling, so a caller can tell what it lost.
    pub source_width: u32,
    pub source_height: u32,
}

/// Args for [`tyto_screenshot`].
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScreenshotArgs {
    /// What to capture: `monitor` for a whole display, `window` for one application
    /// window. Defaults to `monitor`.
    #[serde(default)]
    pub target_kind: Option<String>,
    /// Which one. For `monitor`, a `mon-<id>` from `tyto_list_sources` — omit for the
    /// primary display. For `window`, either a `win-<id>` or a case-insensitive
    /// fragment of the window title, e.g. "Invoice" or "Visual Studio".
    #[serde(default)]
    pub source: Option<String>,
    /// Scale the image down so its long edge is at most this many pixels. Defaults to
    /// 1568, the point past which detail stops being worth its cost in context.
    #[serde(default)]
    pub max_edge: Option<u32>,
}

/// Capture the screen (or one window) and return the image itself.
///
/// Use this to see what is actually on screen — to read a dialog, check a rendering, or
/// confirm the state of an application that exposes no other interface. For a window,
/// `source` may be part of its title, so there is no need to enumerate first; call
/// `tyto_list_sources` when the title is unknown or ambiguous.
///
/// The image is downscaled (default long edge 1568px) and returned inline as PNG. It is
/// **not** saved to the capture library — nothing appears in the user's recordings.
#[arbor_rpc::handler(mcp(
    name = "tyto_screenshot",
    title = "Capture the screen",
    safety = read,
    output = image,
))]
fn tyto_screenshot(_state: &TytoState, args: ScreenshotArgs) -> Result<InlineImage, String> {
    let kind = args.target_kind.as_deref().unwrap_or("monitor");
    if kind == "region" {
        // A region is a rectangle the user dragged; an agent has no such gesture, and
        // inventing one from coordinates it cannot see would be worse than refusing.
        return Err("region capture needs an interactive selection; capture a monitor or a window instead".into());
    }

    // Resolve a title fragment to a real source id, so the caller can say "Invoice"
    // rather than "win-132498".
    let source_id = match (kind, args.source.as_deref()) {
        ("window", Some(needle)) if !needle.starts_with("win-") => Some(resolve_window(needle)?),
        (_, s) => s.map(str::to_string),
    };

    let target = CaptureTarget::resolve(kind, source_id.as_deref(), None)?;
    let (rgba, width, height) = target.grab_rgba()?;
    let image = image::RgbaImage::from_raw(width, height, rgba)
        .ok_or_else(|| "screenshot: buffer/size mismatch".to_string())?;

    let max_edge = args.max_edge.unwrap_or(DEFAULT_MAX_EDGE).clamp(64, ABSOLUTE_MAX_EDGE);
    let scaled = downscale(image, max_edge);
    let (out_w, out_h) = (scaled.width(), scaled.height());

    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut std::io::Cursor::new(&mut png))
        .write_image(&scaled, out_w, out_h, image::ExtendedColorType::Rgba8)
        .map_err(|e| format!("screenshot: png encode failed: {e}"))?;

    Ok(InlineImage {
        mime_type: "image/png".to_string(),
        data: base64::engine::general_purpose::STANDARD.encode(&png),
        width: out_w,
        height: out_h,
        source_width: width,
        source_height: height,
    })
}

/// Find the one window whose title contains `needle`, case-insensitively.
///
/// Ambiguity is an error rather than a guess: capturing the wrong window looks like a
/// working call that returned the wrong world, which is the hardest kind of wrong for a
/// model to notice. The error names the candidates so the next call can be exact.
fn resolve_window(needle: &str) -> Result<String, String> {
    let sources = capture::source::list_capture_sources();
    let needle_lower = needle.to_lowercase();
    let matches: Vec<_> = sources
        .windows
        .iter()
        .filter(|w| w.title.to_lowercase().contains(&needle_lower))
        .collect();

    match matches.as_slice() {
        [] => Err(format!("no open window's title contains \"{needle}\"")),
        [only] => Ok(only.id.clone()),
        many => {
            let titles: Vec<&str> = many.iter().take(8).map(|w| w.title.as_str()).collect();
            Err(format!(
                "\"{needle}\" matches {} windows ({}). Pass a longer fragment or the exact id.",
                many.len(),
                titles.join(" | ")
            ))
        }
    }
}

/// Scale so the long edge is at most `max_edge`, preserving aspect ratio. An image
/// already within the budget is returned untouched — resampling it would only soften it.
fn downscale(image: image::RgbaImage, max_edge: u32) -> image::RgbaImage {
    let long = image.width().max(image.height());
    if long <= max_edge {
        return image;
    }
    let ratio = max_edge as f32 / long as f32;
    let w = ((image.width() as f32 * ratio).round() as u32).max(1);
    let h = ((image.height() as f32 * ratio).round() as u32).max(1);
    image::imageops::resize(&image, w, h, image::imageops::FilterType::Triangle)
}

// `write_image` lives on this trait.
use image::ImageEncoder as _;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downscale_only_shrinks() {
        let small = image::RgbaImage::new(100, 50);
        let out = downscale(small, 1568);
        assert_eq!((out.width(), out.height()), (100, 50), "already inside the budget");

        let big = image::RgbaImage::new(3840, 2160);
        let out = downscale(big, 1568);
        assert_eq!(out.width(), 1568);
        // Aspect ratio held: 2160 * (1568/3840) = 882.
        assert_eq!(out.height(), 882);
    }

    #[test]
    fn a_tall_image_is_bounded_by_its_long_edge() {
        let tall = image::RgbaImage::new(500, 2000);
        let out = downscale(tall, 1000);
        assert_eq!(out.height(), 1000);
        assert_eq!(out.width(), 250);
    }
}
