//! Synchronous single-frame captures: a full-res screenshot (→ output dir) and a
//! downscaled live preview (→ temp) for the source picker.

use std::path::{Path, PathBuf};

use super::{render_template, CaptureTarget};

/// Capture `target`, encode it in the configured screenshot format and save it
/// named from `template` into `out_dir`. Returns the absolute path (the
/// `take_screenshot` handler's `String` return).
///
/// The format comes from `tyto-core`'s output config: `jpg`/`jpeg` → JPEG,
/// `webp` → WebP, anything else → PNG. Formats whose encoder isn't compiled into
/// the `image` build fall back to PNG (see [`save_in_format`]).
///
/// `mask` is an optional freehand polygon in **image-local** pixels (i.e. the
/// crop is already region-local, so the polygon is 0-based against this image):
/// when present, every pixel outside the polygon is made fully transparent and the
/// result is **forced to PNG** regardless of `screenshot_format` (JPEG/WebP-lossy
/// would drop or muddy the alpha we just introduced).
pub fn take(
    target: &CaptureTarget,
    out_dir: &Path,
    template: &str,
    mask: Option<&[[i32; 2]]>,
) -> Result<PathBuf, String> {
    let (rgba, w, h) = target.grab_rgba()?;
    let mut img = image::RgbaImage::from_raw(w, h, rgba)
        .ok_or_else(|| "screenshot: buffer/size mismatch".to_string())?;
    std::fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;

    let cfg = tyto_core::config::load();

    // Freehand mask: punch everything outside the traced polygon to transparent, then
    // save as PNG (alpha is meaningless in JPEG and lossy in WebP).
    if let Some(poly) = mask.filter(|p| p.len() >= 3) {
        apply_polygon_mask(&mut img, poly);
        let (_ext, path) = save_png(&img, out_dir, &render_template(template))?;
        maybe_copy_to_clipboard(&img, cfg.output.copy_screenshot_to_clipboard);
        return Ok(path);
    }

    let (_ext, path) = save_in_format(&img, out_dir, template, &cfg.output.screenshot_format)?;
    maybe_copy_to_clipboard(&img, cfg.output.copy_screenshot_to_clipboard);
    Ok(path)
}

/// Best-effort copy of the just-captured screenshot's RGBA to the OS clipboard (opt-in
/// via the output config). The file is already saved, so a clipboard hiccup must never
/// fail the capture — errors are swallowed. Screenshots only; recordings never copy.
fn maybe_copy_to_clipboard(img: &image::RgbaImage, enabled: bool) {
    if !enabled {
        return;
    }
    let data = arboard::ImageData {
        width: img.width() as usize,
        height: img.height() as usize,
        bytes: std::borrow::Cow::Borrowed(img.as_raw().as_slice()),
    };
    if let Ok(mut cb) = arboard::Clipboard::new() {
        let _ = cb.set_image(data);
    }
}

/// Set alpha=0 on every pixel of `img` that falls OUTSIDE `poly` (an even-odd
/// point-in-polygon test per pixel, sampling at the pixel centre). Points are
/// image-local; the polygon is treated as implicitly closed. Pixels inside keep
/// their colour and alpha.
fn apply_polygon_mask(img: &mut image::RgbaImage, poly: &[[i32; 2]]) {
    let (w, h) = (img.width(), img.height());
    for y in 0..h {
        let py = y as f32 + 0.5;
        for x in 0..w {
            let px = x as f32 + 0.5;
            if !point_in_polygon(px, py, poly) {
                img.get_pixel_mut(x, y)[3] = 0;
            }
        }
    }
}

/// Even-odd (ray-cast) point-in-polygon test. `poly` is implicitly closed (the last
/// vertex connects back to the first).
fn point_in_polygon(px: f32, py: f32, poly: &[[i32; 2]]) -> bool {
    let mut inside = false;
    let n = poly.len();
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = (poly[i][0] as f32, poly[i][1] as f32);
        let (xj, yj) = (poly[j][0] as f32, poly[j][1] as f32);
        // Does a horizontal ray at py cross the edge (i, j)?
        if (yi > py) != (yj > py) {
            let x_cross = xi + (py - yi) / (yj - yi) * (xj - xi);
            if px < x_cross {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

/// JPEG encode quality (0-100). ~90 keeps screenshots crisp at a modest size.
#[cfg(feature = "jpeg-screenshots")]
const JPEG_QUALITY: u8 = 90;

/// The extension a requested format **actually** resolves to, accounting for which
/// encoders are compiled in: `jpg`/`jpeg` → `jpg`, `webp` → `webp`, anything else
/// → `png`; a format whose `image` feature is off resolves to `png` instead of
/// failing.
///
/// Callers name the file from this, never from the raw request — otherwise a
/// `.webp` on disk could hold PNG bytes on a lean build. The frame-sequence writer
/// records the resolved extension in its manifest for the same reason.
pub fn resolve_format(fmt: &str) -> &'static str {
    match fmt.to_ascii_lowercase().as_str() {
        #[cfg(feature = "jpeg-screenshots")]
        "jpg" | "jpeg" => "jpg",
        #[cfg(feature = "webp-screenshots")]
        "webp" => "webp",
        _ => "png",
    }
}

/// Encode `img` into `path` using the encoder for `ext` (a value already run
/// through [`resolve_format`], so it is always one this build can write).
///
/// The single encode seam in the capture engine: screenshots and every frame of a
/// frame sequence go through it, so "which formats exist" is answered in one place.
pub fn encode_to(img: &image::RgbaImage, path: &std::path::Path, ext: &str) -> Result<(), String> {
    match ext {
        #[cfg(feature = "jpeg-screenshots")]
        "jpg" => {
            let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
            let mut w = std::io::BufWriter::new(file);
            let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut w, JPEG_QUALITY);
            // JPEG has no alpha — drop it rather than letting the encoder guess.
            let rgb = image::DynamicImage::ImageRgba8(img.clone()).to_rgb8();
            enc.encode_image(&rgb).map_err(|e| e.to_string())
        }
        #[cfg(feature = "webp-screenshots")]
        "webp" => img.save_with_format(path, image::ImageFormat::WebP).map_err(|e| e.to_string()),
        _ => img.save_with_format(path, image::ImageFormat::Png).map_err(|e| e.to_string()),
    }
}

/// Encode `img` in the requested format into `out_dir` and return `(extension,
/// path)`. The extension is the RESOLVED one (see [`resolve_format`]), so the file
/// name never promises a format the bytes aren't in.
fn save_in_format(
    img: &image::RgbaImage,
    out_dir: &Path,
    template: &str,
    fmt: &str,
) -> Result<(&'static str, PathBuf), String> {
    let ext = resolve_format(fmt);
    let path = out_dir.join(format!("{}.{ext}", render_template(template)));
    encode_to(img, &path, ext)?;
    Ok((ext, path))
}

/// Save `img` as PNG (the masked-screenshot path, which must keep its alpha).
fn save_png(img: &image::RgbaImage, out_dir: &Path, base: &str) -> Result<(&'static str, PathBuf), String> {
    let path = out_dir.join(format!("{base}.png"));
    encode_to(img, &path, "png")?;
    Ok(("png", path))
}

/// Widest edge of a source preview thumbnail, in pixels.
const PREVIEW_MAX_W: u32 = 480;

/// Grab one frame of `target` and save a **downscaled** PNG thumbnail to a temp
/// file (for the picker's live preview of the selected source). Returns the path.
pub fn preview(target: &CaptureTarget) -> Result<PathBuf, String> {
    let (rgba, w, h) = target.grab_rgba()?;
    if w == 0 || h == 0 {
        return Err("preview: zero-size frame".to_string());
    }
    let img = image::RgbaImage::from_raw(w, h, rgba)
        .ok_or_else(|| "preview: buffer/size mismatch".to_string())?;
    let (tw, th) = if w > PREVIEW_MAX_W {
        (PREVIEW_MAX_W, ((h as u64 * PREVIEW_MAX_W as u64) / w as u64).max(1) as u32)
    } else {
        (w, h)
    };
    let thumb = image::imageops::thumbnail(&img, tw, th);
    let path = std::env::temp_dir().join(format!("tyto-preview-{}.png", uuid::Uuid::new_v4().simple()));
    thumb.save(&path).map_err(|e| e.to_string())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 10×10 CCW square from (2,2) to (8,8).
    fn square() -> Vec<[i32; 2]> {
        vec![[2, 2], [8, 2], [8, 8], [2, 8]]
    }

    #[test]
    fn inside_and_outside_a_square() {
        let sq = square();
        assert!(point_in_polygon(5.0, 5.0, &sq), "centre is inside");
        assert!(!point_in_polygon(0.0, 0.0, &sq), "corner of image is outside");
        assert!(!point_in_polygon(9.0, 5.0, &sq), "to the right is outside");
        assert!(!point_in_polygon(5.0, 9.0, &sq), "below is outside");
    }

    #[test]
    fn mask_clears_alpha_outside_only() {
        // A 10×10 opaque image, masked to the inner square: inside keeps alpha, the
        // outer ring goes transparent.
        let mut img = image::RgbaImage::from_pixel(10, 10, image::Rgba([255, 0, 0, 255]));
        apply_polygon_mask(&mut img, &square());
        assert_eq!(img.get_pixel(5, 5)[3], 255, "inside stays opaque");
        assert_eq!(img.get_pixel(0, 0)[3], 0, "outside is transparent");
        assert_eq!(img.get_pixel(5, 5)[0], 255, "inside colour is untouched");
    }

    #[test]
    fn concave_polygon_excludes_the_notch() {
        // An arrow-ish concave shape: the notch between the prongs must read outside.
        let poly = vec![[0, 0], [10, 0], [10, 10], [5, 4], [0, 10]];
        assert!(point_in_polygon(5.0, 1.0, &poly), "top band is inside");
        assert!(!point_in_polygon(5.0, 8.0, &poly), "the notch is outside");
    }
}
