//! Fold a recorded frame sequence into a **sprite atlas + sidecar**: one or more PNG
//! pages, plus an `atlas.ron` that says where every frame is and how long it is held.
//!
//! ## Why an atlas and not the directory of frames
//!
//! Because a game engine cannot page in seven hundred PNGs at sixty frames a second.
//! A sequence directory is the right thing for the recorder (write one file, keep the
//! deduplication, stay resumable) and the wrong thing for a renderer: every frame is a
//! separate texture upload, a separate asset handle, and a separate stall the first
//! time it is shown. An atlas is **one** upload and a UV lookup.
//!
//! ## Why pages
//!
//! Because the ceiling is hardware, not taste: `max_texture_dimension_2d` is 8192 on
//! the low end of the desktop, and a 480×270 tutorial reaches it in about forty
//! seconds. The alternative to more pages is a smaller frame, which is fixing an
//! packing problem by degrading the content. Each region carries its page index, so
//! the reader never has to work out which texture it came from.
//!
//! ## Why the sidecar is written by hand
//!
//! Because the format is `fulcrum-atlas`'s serde shape and Arbor does not depend on
//! `fulcrum` — nor on `ron`, and adding a dependency for one closed struct tree is a
//! poor trade. The shape is small, flat and fully known, so it is emitted directly.
//!
//! ⚠️ **The emitted text is the contract**, and the only thing keeping it honest is a
//! test on each side: [`tests::the_sidecar_matches_the_agreed_shape`] here asserts what
//! we write, and `fulcrum-atlas`'s `la_scheda_dell_esportatore_si_legge` asserts that
//! the same literal parses into an `AtlasSheet`. If you change one, change both — a
//! silent divergence shows up as an atlas that loads to nothing.

use std::path::{Path, PathBuf};

use image::{ImageFormat, RgbaImage};

use super::frames::{self, Manifest};

/// Sidecar file name, matching `fulcrum_atlas::prelude::ATLAS_SHEET_FILE`.
pub const SHEET_NAME: &str = "atlas.ron";
/// The group id the recording lands under inside the sheet.
pub const GROUP_ID: &str = "frames";
/// Default widest edge of a page. Not the hardware ceiling (8192) on purpose: a full
/// 8192² RGBA page is 256 MB resident while it is being assembled, and the point of
/// paging is to avoid that, not to walk up to it.
pub const DEFAULT_MAX_SIDE: u32 = 4096;
/// Pixels of guard around each frame, filled by **extruding** its border outwards.
///
/// ## Perché serve
///
/// Perché le celle sono adiacenti, e un filtro che non sia `nearest` — uno zoom, una scala
/// non intera, una mipmap — campiona **oltre** il bordo del fotogramma e pesca il pixel del
/// vicino. Si vede come una riga di colore sbagliato sui bordi, e si dà la colpa alla
/// registrazione invece che all'impacchettamento.
///
/// ## Perché estruso e non trasparente
///
/// Perché un bordo trasparente non risolve, sposta: invece del fotogramma vicino il filtro
/// pesca il *nulla*, e il bordo si scurisce o si sfrangia. Ripetere il pixel di bordo fa sì
/// che qualunque cosa il filtro peschi lì fuori sia il colore che c'era già.
///
/// Uno basta per la bilineare, che campiona mezzo texel per lato. Serve di più solo con le
/// mipmap, che qui non si generano.
pub const DEFAULT_GUTTER: u32 = 1;
/// Hard ceiling: the `wgpu` downlevel guarantee. Past this a page is a texture no
/// mainstream GPU will accept, so it is refused here rather than at upload time.
pub const HARD_MAX_SIDE: u32 = 8192;

/// What to export, and how.
#[derive(Debug, Clone)]
pub struct AtlasOptions {
    /// Widest edge of a page, in pixels. Clamped to [`HARD_MAX_SIDE`].
    pub max_side: u32,
    /// Name of the sequence inside the sheet's timeline.
    pub sequence: String,
    /// Whether the sequence loops.
    pub looping: bool,
    /// Pixels of **bleed guard** around each frame. See [`DEFAULT_GUTTER`].
    pub gutter: u32,
}

impl Default for AtlasOptions {
    fn default() -> Self {
        Self {
            max_side: DEFAULT_MAX_SIDE,
            sequence: "play".to_string(),
            looping: true,
            gutter: DEFAULT_GUTTER,
        }
    }
}

/// What the export produced.
#[derive(Debug, Clone)]
pub struct AtlasReport {
    /// The directory everything was written into.
    pub dir: PathBuf,
    /// The sidecar.
    pub sheet: PathBuf,
    /// The page PNGs, in index order.
    pub pages: Vec<PathBuf>,
    pub frame_count: usize,
    /// Frame size in pixels.
    pub frame_width: u32,
    pub frame_height: u32,
    /// How many frames fit on a full page.
    pub per_page: usize,
    pub duration_ms: u64,
    /// Bytes written across pages and sidecar.
    pub size_bytes: u64,
}

/// How a page is laid out: a regular grid of `columns × rows` cells of `cell`, each sitting
/// inside a slot that is `gutter` pixels wider and taller on **every** side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageGrid {
    pub columns: u32,
    pub rows: u32,
    pub cell_w: u32,
    pub cell_h: u32,
    /// Guard around each frame — see [`DEFAULT_GUTTER`].
    pub gutter: u32,
}

impl PageGrid {
    pub fn capacity(&self) -> usize {
        self.columns as usize * self.rows as usize
    }

    /// Quanto spazio occupa uno slot: la cella più il guard sui due lati.
    pub fn slot_w(&self) -> u32 {
        self.cell_w + self.gutter * 2
    }

    pub fn slot_h(&self) -> u32 {
        self.cell_h + self.gutter * 2
    }

    /// The pixel size of a page holding `count` frames in this arrangement — tight,
    /// not padded to the ceiling: the last page of a six-page export is usually a
    /// sliver, and rounding it up to 4096² would cost more than every other page.
    pub fn page_size(&self, count: usize) -> (u32, u32) {
        let count = count.max(1) as u32;
        let cols = count.min(self.columns);
        let rows = count.div_ceil(self.columns);
        (cols * self.slot_w(), rows * self.slot_h())
    }

    /// Dove comincia lo **slot** di `slot`, guard incluso.
    pub fn slot_origin(&self, slot: usize) -> (u32, u32) {
        let slot = slot as u32;
        ((slot % self.columns) * self.slot_w(), (slot / self.columns) * self.slot_h())
    }

    /// Dove comincia il **fotogramma** dentro il suo slot: il guard più in là.
    pub fn frame_origin(&self, slot: usize) -> (u32, u32) {
        let (x, y) = self.slot_origin(slot);
        (x + self.gutter, y + self.gutter)
    }
}

/// How many cells of `cell_w × cell_h` fit on a page of at most `max_side` per edge.
///
/// `None` when a single frame is already wider or taller than a page: there is no
/// arrangement that helps, and the honest answer is that this recording cannot be
/// atlased at this resolution.
pub fn plan_grid(cell_w: u32, cell_h: u32, max_side: u32, gutter: u32) -> Option<PageGrid> {
    if cell_w == 0 || cell_h == 0 {
        return None;
    }
    // Il guard va contato nel «ci sta»: un fotogramma largo esattamente quanto la pagina non
    // ci sta più una volta che gli si mette una cornice attorno.
    let slot_w = cell_w.checked_add(gutter * 2)?;
    let slot_h = cell_h.checked_add(gutter * 2)?;
    if slot_w > max_side || slot_h > max_side {
        return None;
    }
    Some(PageGrid { columns: max_side / slot_w, rows: max_side / slot_h, cell_w, cell_h, gutter })
}

// ── Export ───────────────────────────────────────────────────────────────────

/// Fold the sequence directory `seq_dir` into `out_dir`, which is created.
pub fn export(seq_dir: &Path, out_dir: &Path, opts: &AtlasOptions) -> Result<AtlasReport, String> {
    let manifest = frames::read_manifest(seq_dir)?;
    if manifest.frame_count == 0 {
        return Err("the sequence has no frames".to_string());
    }
    let max_side = opts.max_side.clamp(1, HARD_MAX_SIDE);
    let grid = plan_grid(manifest.width, manifest.height, max_side, opts.gutter).ok_or_else(|| {
        format!(
            "a {}×{} frame does not fit a {max_side}px page — re-record with a lower \
             max width, or raise the page limit",
            manifest.width, manifest.height
        )
    })?;

    std::fs::create_dir_all(out_dir).map_err(|e| format!("atlas output dir: {e}"))?;

    let per_page = grid.capacity();
    let page_count = manifest.frame_count.div_ceil(per_page);
    let mut pages = Vec::with_capacity(page_count);
    let mut regions = Vec::with_capacity(manifest.frame_count);
    let mut size_bytes = 0u64;

    for page_index in 0..page_count {
        let first = page_index * per_page;
        let count = per_page.min(manifest.frame_count - first);
        let (page_w, page_h) = grid.page_size(count);
        let mut canvas = RgbaImage::new(page_w, page_h);

        for slot in 0..count {
            let frame = read_frame(seq_dir, first + slot, &manifest)?;
            let (ox, oy) = grid.frame_origin(slot);
            blit(&mut canvas, &frame, ox, oy);
            extrude(&mut canvas, ox, oy, grid.cell_w, grid.cell_h, grid.gutter);
            regions.push(Region {
                page: page_index as u32,
                min: (ox as f32 / page_w as f32, oy as f32 / page_h as f32),
                max: (
                    (ox + grid.cell_w) as f32 / page_w as f32,
                    (oy + grid.cell_h) as f32 / page_h as f32,
                ),
            });
        }

        let name = page_name(page_index);
        let path = out_dir.join(&name);
        canvas.save_with_format(&path, ImageFormat::Png).map_err(|e| format!("atlas page: {e}"))?;
        size_bytes += std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        pages.push(PageInfo { index: page_index as u32, texture: name, width: page_w, height: page_h });
    }

    let durations = durations_ms(&manifest);
    let sheet_text = sidecar(&pages, &regions, &durations, &manifest, opts);
    let sheet = out_dir.join(SHEET_NAME);
    std::fs::write(&sheet, &sheet_text).map_err(|e| format!("atlas sidecar: {e}"))?;
    size_bytes += sheet_text.len() as u64;

    Ok(AtlasReport {
        dir: out_dir.to_path_buf(),
        sheet,
        pages: pages.iter().map(|p| out_dir.join(&p.texture)).collect(),
        frame_count: manifest.frame_count,
        frame_width: manifest.width,
        frame_height: manifest.height,
        per_page,
        duration_ms: manifest.duration_ms,
        size_bytes,
    })
}

fn page_name(index: usize) -> String {
    format!("atlas_{index:03}.png")
}

fn read_frame(seq_dir: &Path, index: usize, m: &Manifest) -> Result<RgbaImage, String> {
    let path = frames::frame_path(seq_dir, index, &m.format);
    let bytes = std::fs::read(&path).map_err(|e| format!("frame {index}: {e}"))?;
    let format = match m.format.as_str() {
        "png" => ImageFormat::Png,
        "jpg" | "jpeg" => ImageFormat::Jpeg,
        "webp" => ImageFormat::WebP,
        other => return Err(format!("unknown frame format \"{other}\"")),
    };
    // The decoder for jpg/webp lives behind the same feature as its encoder, so a
    // PNG-only build says which flag is missing instead of "unsupported format".
    let img = image::load_from_memory_with_format(&bytes, format)
        .map_err(|e| format!("frame {index} ({}): {e}", m.format))?;
    Ok(img.to_rgba8())
}

/// Ripete il bordo del fotogramma dentro il guard che lo circonda.
///
/// ⚠️ **Non basta lasciare il guard trasparente.** Un filtro che campiona oltre il bordo —
/// uno zoom, una scala non intera — pescherebbe il nulla invece del vicino, e il fotogramma
/// si sfrangia sui lati invece di sporcarsi: un difetto diverso, non uno di meno. Ripetendo
/// il pixel di bordo, qualunque cosa il filtro peschi lì fuori è il colore che c'era già.
///
/// Gli angoli prendono il pixel d'angolo, che è l'unico che confina con entrambi i lati.
fn extrude(dst: &mut RgbaImage, ox: u32, oy: u32, w: u32, h: u32, gutter: u32) {
    if gutter == 0 || w == 0 || h == 0 {
        return;
    }
    let (right, bottom) = (ox + w - 1, oy + h - 1);
    let clamp_x = |x: i64| x.clamp(ox as i64, right as i64) as u32;
    let clamp_y = |y: i64| y.clamp(oy as i64, bottom as i64) as u32;

    let x0 = ox as i64 - gutter as i64;
    let y0 = oy as i64 - gutter as i64;
    let x1 = (right + gutter) as i64;
    let y1 = (bottom + gutter) as i64;

    for y in y0..=y1 {
        for x in x0..=x1 {
            // Dentro il fotogramma non si tocca niente.
            if x >= ox as i64 && x <= right as i64 && y >= oy as i64 && y <= bottom as i64 {
                continue;
            }
            if x < 0 || y < 0 || x >= dst.width() as i64 || y >= dst.height() as i64 {
                continue;
            }
            let src = *dst.get_pixel(clamp_x(x), clamp_y(y));
            dst.put_pixel(x as u32, y as u32, src);
        }
    }
}

/// Copy `src` onto `dst` at `(ox, oy)`. Rows only — the frames are all the same size
/// and land on cell boundaries, so there is no clipping to do.
fn blit(dst: &mut RgbaImage, src: &RgbaImage, ox: u32, oy: u32) {
    let (w, h) = (src.width().min(dst.width().saturating_sub(ox)), src.height().min(dst.height().saturating_sub(oy)));
    for y in 0..h {
        for x in 0..w {
            dst.put_pixel(ox + x, oy + y, *src.get_pixel(x, y));
        }
    }
}

/// Presentation times → per-frame durations.
///
/// The last frame is the one that cannot be derived from a difference: without the
/// recording's total length it would come out zero, i.e. invisible. This mirrors
/// `AtlasSequence::from_timestamps` on the reading side, deliberately — the two must
/// agree, and a test pins the numbers.
pub fn durations_ms(m: &Manifest) -> Vec<u32> {
    m.times
        .iter()
        .enumerate()
        .map(|(i, start)| {
            let end = m.times.get(i + 1).copied().unwrap_or(m.duration_ms.max(*start as u64) as u32);
            end.saturating_sub(*start).max(1)
        })
        .collect()
}

// ── The sidecar ──────────────────────────────────────────────────────────────

struct PageInfo {
    index: u32,
    texture: String,
    width: u32,
    height: u32,
}

struct Region {
    page: u32,
    min: (f32, f32),
    max: (f32, f32),
}

/// Emit the `AtlasSheet` RON. See the module header: this text is the contract with
/// `fulcrum-atlas`, and both sides pin it with a test.
fn sidecar(
    pages: &[PageInfo],
    regions: &[Region],
    durations: &[u32],
    m: &Manifest,
    opts: &AtlasOptions,
) -> String {
    let mut s = String::with_capacity(regions.len() * 128);
    s.push_str("(\n    pages: [\n");
    for p in pages {
        s.push_str(&format!(
            "        (index: {}, texture: \"{}\", size: ({}, {})),\n",
            p.index, p.texture, p.width, p.height
        ));
    }
    s.push_str("    ],\n    images: {\n");
    s.push_str(&format!("        \"{GROUP_ID}\": Image((\n            layout: List([\n"));
    for r in regions {
        s.push_str(&format!(
            "                (uv_region: (min: (x: {:?}, y: {:?}), max: (x: {:?}, y: {:?})), page: {}),\n",
            r.min.0, r.min.1, r.max.0, r.max.1, r.page
        ));
    }
    s.push_str("            ]),\n            channels: [Diffuse],\n");
    s.push_str(&format!(
        "            source_size: Some(({}, {})),\n        )),\n    }},\n",
        m.width, m.height
    ));
    s.push_str("    timeline: (\n        sequences: [\n            (\n");
    s.push_str(&format!("                name: \"{}\",\n", escape(&opts.sequence)));
    s.push_str(&format!("                image: \"{GROUP_ID}\",\n                frames: [\n"));
    for (i, d) in durations.iter().enumerate() {
        s.push_str(&format!("                    (index: {i}, duration_ms: {d}),\n"));
    }
    s.push_str("                ],\n");
    s.push_str(&format!("                looping: {},\n            ),\n        ],\n    ),\n)\n", opts.looping));
    s
}

/// A sequence name reaches us from a tool argument, so it can carry a quote. RON
/// strings escape the same two characters JSON does.
fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(width: u32, height: u32, times: Vec<u32>, duration_ms: u64) -> Manifest {
        Manifest {
            version: 1,
            kind: "tyto-frames".to_string(),
            width,
            height,
            format: "png".to_string(),
            sample_fps: 12,
            duration_ms,
            created_at: 0,
            target: "Monitor 1".to_string(),
            size_bytes: 0,
            frame_count: times.len(),
            times,
        }
    }

    #[test]
    fn a_page_holds_as_many_whole_cells_as_it_fits() {
        let g = plan_grid(480, 270, 4096, 0).unwrap();
        assert_eq!((g.columns, g.rows), (8, 15));
        assert_eq!(g.capacity(), 120);
    }

    /// Il guard occupa spazio, quindi entrano meno fotogrammi: 482×272 per slot invece di
    /// 480×270. Contarlo dopo, sul solo `cell`, darebbe una pagina che sfora.
    #[test]
    fn il_guard_entra_nel_conto_di_quanti_ci_stanno() {
        let g = plan_grid(480, 270, 4096, 1).unwrap();
        assert_eq!((g.slot_w(), g.slot_h()), (482, 272));
        assert_eq!((g.columns, g.rows), (8, 15));
        assert!(g.page_size(g.capacity()).0 <= 4096);
        assert!(g.page_size(g.capacity()).1 <= 4096);
    }

    /// ⚠️ Un fotogramma largo **esattamente** quanto la pagina ci stava senza guard e non
    /// ci sta più con: se il guard si contasse dopo, la pagina uscirebbe più grande del
    /// tetto e nessuna GPU la accetterebbe.
    #[test]
    fn un_fotogramma_al_limite_non_ci_sta_piu_col_guard() {
        assert!(plan_grid(4096, 100, 4096, 0).is_some());
        assert!(plan_grid(4096, 100, 4096, 1).is_none());
    }

    /// ⚠️ A frame bigger than a page has no arrangement that helps. Returning a 1×1
    /// grid would "work" and silently crop every frame to a page-sized window.
    #[test]
    fn a_frame_that_does_not_fit_a_page_is_refused() {
        assert!(plan_grid(9000, 270, 8192, 0).is_none());
        assert!(plan_grid(480, 9000, 8192, 0).is_none());
        assert!(plan_grid(0, 270, 4096, 0).is_none());
    }

    /// The last page of an export is usually a sliver; padding it to the ceiling would
    /// cost more than every other page put together.
    #[test]
    fn the_last_page_is_only_as_big_as_it_needs() {
        let g = plan_grid(100, 100, 400, 0).unwrap(); // 4×4 = 16 per page
        assert_eq!(g.page_size(16), (400, 400));
        assert_eq!(g.page_size(3), (300, 100), "una riga sola, tre colonne");
        assert_eq!(g.page_size(5), (400, 200), "due righe, la seconda incompleta");
        assert_eq!(g.page_size(0), (100, 100), "mai una pagina di lato zero");
    }

    #[test]
    fn slots_fill_rows_left_to_right() {
        let g = plan_grid(100, 100, 400, 0).unwrap();
        assert_eq!(g.slot_origin(0), (0, 0));
        assert_eq!(g.slot_origin(3), (300, 0));
        assert_eq!(g.slot_origin(4), (0, 100));
        assert_eq!(g.slot_origin(15), (300, 300));
    }

    /// ⚠️ The last frame is the only one not derived from a difference. Left to the
    /// obvious `times[i+1] - times[i]` it comes out zero, i.e. never shown.
    #[test]
    fn the_last_frame_lasts_until_the_end_of_the_recording() {
        let m = manifest(4, 4, vec![0, 80, 90, 990], 1200);
        assert_eq!(durations_ms(&m), vec![80, 10, 900, 210]);
    }

    #[test]
    fn no_frame_lasts_zero_milliseconds() {
        // Two frames stamped at the same ms, and a total shorter than the last stamp.
        let m = manifest(4, 4, vec![0, 500, 500], 400);
        assert!(durations_ms(&m).iter().all(|d| *d >= 1));
    }

    /// ⚠️ **This literal is the contract with `fulcrum-atlas`.** The same text appears
    /// there as the fixture of `la_scheda_dell_esportatore_si_legge`; if you change the
    /// emitter, change both, or the atlas loads to nothing without an error.
    #[test]
    fn the_sidecar_matches_the_agreed_shape() {
        let pages = vec![PageInfo { index: 0, texture: "atlas_000.png".into(), width: 4, height: 2 }];
        let regions = vec![
            Region { page: 0, min: (0.0, 0.0), max: (0.5, 1.0) },
            Region { page: 0, min: (0.5, 0.0), max: (1.0, 1.0) },
        ];
        let m = manifest(2, 2, vec![0, 100], 250);
        let opts = AtlasOptions {
            max_side: 4096,
            sequence: "play".into(),
            looping: true,
            gutter: 0,
        };

        let text = sidecar(&pages, &regions, &durations_ms(&m), &m, &opts);
        assert_eq!(text, EXPECTED_SHEET);
    }

    const EXPECTED_SHEET: &str = r#"(
    pages: [
        (index: 0, texture: "atlas_000.png", size: (4, 2)),
    ],
    images: {
        "frames": Image((
            layout: List([
                (uv_region: (min: (x: 0.0, y: 0.0), max: (x: 0.5, y: 1.0)), page: 0),
                (uv_region: (min: (x: 0.5, y: 0.0), max: (x: 1.0, y: 1.0)), page: 0),
            ]),
            channels: [Diffuse],
            source_size: Some((2, 2)),
        )),
    },
    timeline: (
        sequences: [
            (
                name: "play",
                image: "frames",
                frames: [
                    (index: 0, duration_ms: 100),
                    (index: 1, duration_ms: 150),
                ],
                looping: true,
            ),
        ],
    ),
)
"#;

    /// A name from a tool argument can carry a quote; unescaped it would end the RON
    /// string early and make the whole sheet unparseable.
    #[test]
    fn a_quote_in_the_sequence_name_does_not_break_the_file() {
        let opts = AtlasOptions { sequence: "he said \"go\"".into(), ..Default::default() };
        let m = manifest(2, 2, vec![0], 100);
        let text = sidecar(&[], &[], &durations_ms(&m), &m, &opts);
        assert!(text.contains(r#"name: "he said \"go\"","#), "{text}");
    }

    /// ⚠️ **Il guard ripete il bordo, non lo lascia vuoto.** Trasparente non risolve il
    /// bleeding, lo scambia con un bordo sfrangiato — e a quel punto si dà la colpa alla
    /// registrazione invece che all'impacchettamento.
    #[test]
    fn il_guard_ripete_il_bordo_del_fotogramma() {
        let mut canvas = RgbaImage::new(4, 4);
        // Un fotogramma 2×2 al centro, con un angolo distinguibile.
        for (x, y, c) in [(1, 1, [10, 0, 0, 255]), (2, 1, [20, 0, 0, 255]),
                          (1, 2, [30, 0, 0, 255]), (2, 2, [40, 0, 0, 255])] {
            canvas.put_pixel(x, y, image::Rgba(c));
        }
        extrude(&mut canvas, 1, 1, 2, 2, 1);

        assert_eq!(canvas.get_pixel(0, 0).0, [10, 0, 0, 255], "l'angolo prende l'angolo");
        assert_eq!(canvas.get_pixel(1, 0).0, [10, 0, 0, 255], "sopra prende il pixel sotto");
        assert_eq!(canvas.get_pixel(3, 3).0, [40, 0, 0, 255], "l'angolo opposto");
        assert_eq!(canvas.get_pixel(0, 2).0, [30, 0, 0, 255], "a sinistra prende il vicino");
        assert_eq!(canvas.get_pixel(1, 1).0, [10, 0, 0, 255], "dentro non si tocca niente");
    }

    /// Un guard a zero non deve toccare niente: è il modo di spegnerlo.
    #[test]
    fn senza_guard_l_estrusione_non_scrive() {
        let mut canvas = RgbaImage::new(3, 3);
        extrude(&mut canvas, 1, 1, 1, 1, 0);
        assert!(canvas.pixels().all(|p| p.0 == [0, 0, 0, 0]));
    }

    /// Il fotogramma sta al suo posto **dentro** lo slot: il guard lo sposta di un pixel.
    #[test]
    fn il_fotogramma_sta_dentro_lo_slot_non_sul_bordo() {
        let g = plan_grid(10, 10, 100, 1).unwrap();
        assert_eq!(g.slot_origin(0), (0, 0));
        assert_eq!(g.frame_origin(0), (1, 1));
        assert_eq!(g.slot_origin(1), (12, 0));
        assert_eq!(g.frame_origin(1), (13, 1));
    }

    #[test]
    fn blit_lands_the_frame_where_the_slot_says() {
        let mut canvas = RgbaImage::new(4, 2);
        let mut frame = RgbaImage::new(2, 2);
        for p in frame.pixels_mut() {
            *p = image::Rgba([1, 2, 3, 255]);
        }
        blit(&mut canvas, &frame, 2, 0);
        assert_eq!(canvas.get_pixel(2, 0).0, [1, 2, 3, 255]);
        assert_eq!(canvas.get_pixel(3, 1).0, [1, 2, 3, 255]);
        assert_eq!(canvas.get_pixel(0, 0).0, [0, 0, 0, 0], "fuori dallo slot resta vuoto");
    }
}
