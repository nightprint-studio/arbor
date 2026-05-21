//! SVG export for the full commit graph.
//!
//! Replicates the visual mathematics from `src/lib/utils/graph-renderer.ts`:
//!   ROW_HEIGHT = 28, LANE_WIDTH = 26, NODE_RADIUS = 10
//!   Same bezier edge shapes (fork = vertical-first, merge = horizontal-first)
//!   Lane colours pulled from the active theme's `--graph-lane-*` CSS vars.
//!
//! Themed: callers pass a `ThemeColors` snapshot built from the current theme's
//! CSS variables so the exported file matches what's on screen (light themes,
//! custom themes, etc.).
//!
//! Avatars: each non-merge commit is rendered with a deterministic-initials
//! circle plus a gravatar `<image>` overlay (`d=blank`), mirroring
//! `src/lib/stores/avatars.svelte.ts` so the exported file shows the same
//! gravatar that loaded in the live graph (and falls back to initials when
//! no gravatar exists).

use std::collections::HashMap;
use std::io::{BufWriter, Write as IoWrite};
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::git::graph::{EdgeType, GraphData, RefType};

// ── Layout constants (mirror graph-renderer.ts) ──────────────────────────────

const ROW_HEIGHT: f64 = 28.0;
const LANE_WIDTH: f64 = 26.0;
const NODE_RADIUS: f64 = 10.0;
const MIN_EDGE_LENGTH: f64 = 8.0;

/// Gap between the graph lane area and the first text column.
const INFO_GAP: f64 = 14.0;
/// Width of the short-SHA column (7 hex chars × ~7 px/ch at 10.5 px monospace).
const SHA_COL_W: f64 = 62.0;
/// Width reserved for ref badges (branch / tag labels).
const BADGE_COL_W: f64 = 210.0;
/// Hard cap on the author column width — sized to actual content but never
/// wider than this (long bot names, etc. get truncated with an ellipsis).
const AUTHOR_COL_MAX: f64 = 220.0;
/// Hard cap on the commit-summary column width — keeps the SVG from going
/// absurdly wide on repos with long single-line commit messages.
const SUMMARY_COL_MAX: f64 = 1400.0;
/// Approximate glyph width at the body font size (10.5 px JetBrains Mono).
const PIX_PER_CHAR: f64 = 6.4;
/// Inner left/top padding so lane glow + HEAD halo aren't clipped at the edge.
const LEFT_PAD:  f64 = 18.0;
const TOP_PAD:   f64 = 18.0;
/// Outer right/bottom padding so the last column / last commit row breathes.
const RIGHT_PAD:  f64 = 22.0;
const BOTTOM_PAD: f64 = 18.0;

// ── Default palette (used when a theme var is missing) ───────────────────────

const DEFAULT_LANES: [&str; 10] = [
    "#3d7fff", "#ff8c00", "#4db84d", "#b06fd4",
    "#ff4444", "#00d4cc", "#ffcc00", "#ff7040",
    "#4dd9d0", "#9ab0cc",
];

// ── Theme colours snapshot ───────────────────────────────────────────────────

/// All colours the SVG export needs. Built from the active theme's `vars`
/// map so light/dark/custom themes all render correctly. Missing keys fall
/// back to the original dark-theme defaults.
pub struct ThemeColors {
    pub bg:        String,
    pub alt:       String,
    pub sep:       String,
    pub sha:       String,
    pub author:    String,
    pub text:      String,
    pub head:      String,
    pub branch_bg: String,
    pub branch_fg: String,
    pub remote_bg: String,
    pub remote_fg: String,
    pub tag_bg:    String,
    pub tag_fg:    String,
    pub lanes:     [String; 10],
}

impl ThemeColors {
    /// Build from a theme's CSS-vars map. Empty map → all defaults.
    pub fn from_vars(vars: &HashMap<String, String>) -> Self {
        let g = |k: &str, d: &str| {
            vars.get(k)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| d.to_string())
        };
        let lanes: [String; 10] = std::array::from_fn(|i| {
            g(&format!("--graph-lane-{i}"), DEFAULT_LANES[i])
        });
        Self {
            bg:        g("--bg-base",       "#1e1f22"),
            alt:       g("--bg-elevated",   "#2b2d30"),
            sep:       g("--border-subtle", "#2e3035"),
            // The SHA column originally used a custom muted blue; map it to the
            // theme's secondary text so light themes get readable contrast.
            sha:       g("--text-secondary","#9da0a8"),
            author:    g("--text-muted",    "#7a7d85"),
            text:      g("--text-primary",  "#dfe1e5"),
            // HEAD row's commit summary uses the brightest text token.
            head:      g("--text-primary",  "#dce4f0"),
            branch_bg: g("--success-subtle","rgba(95, 173, 86, 0.18)"),
            branch_fg: g("--success",       "#4dc94d"),
            remote_bg: g("--info-subtle",   "rgba(77, 120, 204, 0.18)"),
            remote_fg: g("--info",          "#4d78cc"),
            tag_bg:    g("--warning-subtle","rgba(226, 163, 53, 0.18)"),
            tag_fg:    g("--warning",       "#ffb52a"),
            lanes,
        }
    }

    fn lane(&self, idx: usize) -> &str {
        &self.lanes[idx % self.lanes.len()]
    }
}

// ── Geometry helpers ─────────────────────────────────────────────────────────

fn node_x(lane: usize) -> f64 {
    LANE_WIDTH * lane as f64 + LANE_WIDTH / 2.0
}

fn node_y(row: usize) -> f64 {
    ROW_HEIGHT * row as f64 + ROW_HEIGHT / 2.0
}

/// Produces the SVG `d` attribute for an edge, replicating the TypeScript
/// `edgePath()` function in `graph-renderer.ts` exactly.
fn edge_path(edge: &crate::git::graph::GraphEdge) -> String {
    let x1 = node_x(edge.from_lane);
    let y1 = node_y(edge.from_row);
    let x2 = node_x(edge.to_lane);
    let y2 = node_y(edge.to_row);

    // Same lane → clipped straight line.
    if edge.from_lane == edge.to_lane {
        let clip = NODE_RADIUS + 1.0;
        let dir  = if y2 > y1 { 1.0 } else { -1.0 };
        let raw  = (y2 - y1).abs() - 2.0 * clip;
        let len  = raw.max(MIN_EDGE_LENGTH);
        let mid  = (y1 + y2) / 2.0;
        return format!(
            "M {:.2} {:.2} L {:.2} {:.2}",
            x1, mid - dir * len / 2.0,
            x1, mid + dir * len / 2.0,
        );
    }

    let dx = (x2 - x1).abs();
    let dy = y2 - y1;
    let r  = 6.0_f64.min(dx / 2.0).min(dy / 2.0).max(0.0);
    let sx: f64 = if x2 > x1 { 1.0 } else { -1.0 };

    let is_fork = matches!(edge.edge_type, EdgeType::ForkLeft | EdgeType::ForkRight);

    if is_fork {
        // Vertical-first (⌟ / ⌞ shape)
        if r <= 0.0 {
            return format!(
                "M {:.2} {:.2} L {:.2} {:.2} L {:.2} {:.2}",
                x1, y1, x1, y2, x2, y2
            );
        }
        format!(
            "M {:.2} {:.2} L {:.2} {:.2} Q {:.2} {:.2} {:.2} {:.2} L {:.2} {:.2}",
            x1, y1,
            x1, y2 - r,
            x1, y2, x1 + sx * r, y2,
            x2, y2
        )
    } else {
        // Horizontal-first (┌ / ┐ shape)
        if r <= 0.0 {
            return format!(
                "M {:.2} {:.2} L {:.2} {:.2} L {:.2} {:.2}",
                x1, y1, x2, y1, x2, y2
            );
        }
        format!(
            "M {:.2} {:.2} L {:.2} {:.2} Q {:.2} {:.2} {:.2} {:.2} L {:.2} {:.2}",
            x1, y1,
            x2 - sx * r, y1,
            x2, y1, x2, y1 + r,
            x2, y2
        )
    }
}

// ── Avatar helpers (mirror src/lib/stores/avatars.svelte.ts) ─────────────────

fn gravatar_hash(email: &str) -> String {
    let mut h = Sha256::new();
    h.update(email.trim().to_lowercase().as_bytes());
    let out = h.finalize();
    let mut s = String::with_capacity(64);
    for b in out { s.push_str(&format!("{b:02x}")); }
    s
}

fn initials_of(name: &str) -> String {
    let s: String = name
        .split_whitespace()
        .filter_map(|w| w.chars().next())
        .take(2)
        .collect::<String>()
        .to_uppercase();
    if s.is_empty() { "?".to_string() } else { s }
}

/// Deterministic HSL fallback colour, matching `colorFor` in avatars.svelte.ts
/// (Math.imul-style 32-bit hash, mod 360).
fn initials_color(email: &str) -> String {
    let mut h: i32 = 0;
    for c in email.chars() {
        // Wrapping multiplication to mirror JS Math.imul.
        h = h.wrapping_mul(31).wrapping_add(c as i32);
    }
    let hue = (h.unsigned_abs() % 360) as u32;
    format!("hsl({hue},46%,36%)")
}

// ── XML helpers ──────────────────────────────────────────────────────────────

fn esc(s: &str) -> String {
    s.replace('&',  "&amp;")
     .replace('<',  "&lt;")
     .replace('>',  "&gt;")
     .replace('"',  "&quot;")
     .replace('\'', "&apos;")
}

fn trunc(s: &str, max_chars: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let t: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{}…", t)
    }
}

// ── Main entry point ─────────────────────────────────────────────────────────

/// Stream the full commit graph as an SVG file to `output_path`.
///
/// `theme` is the active theme colour snapshot — pass
/// `ThemeColors::from_vars(&HashMap::new())` for the dark-theme defaults.
///
/// `emit` is called periodically with human-readable progress lines that are
/// forwarded to the job-output ring-buffer visible in the Jobs panel.
pub fn generate_svg_to_file(
    graph: &GraphData,
    output_path: &Path,
    theme: &ThemeColors,
    emit: &dyn Fn(&str),
) -> Result<(), String> {
    let n_rows = graph.nodes.len();
    if n_rows == 0 {
        return Err("Repository has no commits to export.".into());
    }

    let lane_count = graph.lane_count.max(1);

    // ── Column widths (sized to actual content) ──────────────────────────────
    // Author + summary widen to fit the longest entries so wide screens don't
    // get an artificially-truncated message column. Both have hard caps so
    // pathological one-liners don't blow up the canvas width.
    let author_max_chars = graph
        .nodes
        .iter()
        .map(|n| n.author.name.chars().count())
        .max()
        .unwrap_or(0);
    let author_col_w = ((author_max_chars as f64 * PIX_PER_CHAR) + 14.0)
        .clamp(135.0, AUTHOR_COL_MAX);
    let author_max_chars_render = ((author_col_w - 14.0) / PIX_PER_CHAR).floor() as usize;

    let summary_max_chars = graph
        .nodes
        .iter()
        .map(|n| n.summary.chars().count())
        .max()
        .unwrap_or(0);
    let summary_col_w = ((summary_max_chars as f64 * PIX_PER_CHAR) + 16.0)
        .clamp(430.0, SUMMARY_COL_MAX);
    let summary_max_chars_render = ((summary_col_w - 16.0) / PIX_PER_CHAR).floor() as usize;

    // ── Canvas dimensions (in inner / content coordinates) ──────────────────
    let graph_w   = lane_count as f64 * LANE_WIDTH + LANE_WIDTH;
    let text_base = graph_w + INFO_GAP;
    let sha_x     = text_base;
    let badge_x   = sha_x   + SHA_COL_W;
    let author_x  = badge_x + BADGE_COL_W;
    let summary_x = author_x + author_col_w;
    let inner_w   = summary_x + summary_col_w;
    let inner_h   = n_rows as f64 * ROW_HEIGHT;
    // Outer canvas adds breathing room around the content.
    let total_w   = inner_w + LEFT_PAD + RIGHT_PAD;
    let total_h   = inner_h + TOP_PAD  + BOTTOM_PAD;

    // ── Open output file ─────────────────────────────────────────────────────
    let file = std::fs::File::create(output_path)
        .map_err(|e| format!("Cannot create '{}': {e}", output_path.display()))?;
    let mut w = BufWriter::new(file);

    macro_rules! wr {
        ($($arg:tt)*) => {
            write!(w, $($arg)*).map_err(|e: std::io::Error| e.to_string())?
        };
    }

    let bg = &theme.bg;

    // ── SVG header ───────────────────────────────────────────────────────────
    // Use explicit width/height (no `width=100%`) so the browser shows the SVG
    // at native size and provides horizontal + vertical scrollbars for tall
    // or wide graphs. This was previously scaled to viewport width via
    // aspect-ratio, which squished wide graphs and clipped the right columns.
    let tw = total_w as u32;
    let th = total_h as u32;
    wr!("<svg xmlns=\"http://www.w3.org/2000/svg\" \
         width=\"{tw}\" height=\"{th}\" \
         viewBox=\"0 0 {tw} {th}\" \
         style=\"display:block;background:{bg};\">");

    // ── <defs> ───────────────────────────────────────────────────────────────
    wr!("<defs>");
    wr!("<style>text{{font-family:'JetBrains Mono','Courier New',Courier,monospace;dominant-baseline:middle;}}</style>");

    // Single circular clip shared by every gravatar overlay (objectBoundingBox
    // → automatically clips any `<image>` to the inscribed circle of its bbox).
    wr!("<clipPath id=\"avatar-clip\" clipPathUnits=\"objectBoundingBox\">\
         <circle cx=\"0.5\" cy=\"0.5\" r=\"0.5\"/></clipPath>");

    // Row-background gradients — one per lane color.
    // objectBoundingBox (default) stretches each gradient across its own rect.
    for (ci, c) in theme.lanes.iter().enumerate() {
        wr!("<linearGradient id=\"row-grad-{ci}\" x1=\"0\" y1=\"0\" x2=\"1\" y2=\"0\">");
        wr!("<stop offset=\"0%\" stop-color=\"{c}\" stop-opacity=\"0.22\"/>");
        wr!("<stop offset=\"100%\" stop-color=\"{c}\" stop-opacity=\"0\"/>");
        wr!("</linearGradient>");
    }

    // Gaussian blur filter for the lane glow pass (mirrors CommitGraph.svelte).
    wr!("<filter id=\"lane-glow\" color-interpolation-filters=\"sRGB\" \
         x=\"-80%\" y=\"-10%\" width=\"260%\" height=\"120%\">");
    wr!("<feGaussianBlur in=\"SourceGraphic\" stdDeviation=\"2.8\"/>");
    wr!("</filter>");

    wr!("</defs>");

    // Background fill — full outer canvas
    wr!("<rect width=\"{tw}\" height=\"{th}\" fill=\"{bg}\"/>");

    // Wrap the rest of the document in a translate group so the inner content
    // (graph, edges, nodes, text) sits inside an even padding box.
    let lp = format!("{LEFT_PAD:.1}");
    let tp = format!("{TOP_PAD:.1}");
    wr!("<g transform=\"translate({lp},{tp})\">");

    // Subtle alternating row bands — extended past the inner origin on both
    // sides so they cover the full canvas width despite the translate.
    emit("Rendering row backgrounds…");
    let alt    = &theme.alt;
    let band_x = format!("{:.1}", -LEFT_PAD);
    let band_w = format!("{:.1}", inner_w + LEFT_PAD + RIGHT_PAD);
    for i in (0..n_rows).step_by(2) {
        let y  = i as f64 * ROW_HEIGHT;
        let yo = format!("{y:.1}");
        let rh = ROW_HEIGHT as u32;
        wr!("<rect x=\"{band_x}\" y=\"{yo}\" width=\"{band_w}\" height=\"{rh}\" fill=\"{alt}\" opacity=\"0.5\"/>");
    }

    // ── Row background glow (PASS 0) ─────────────────────────────────────────
    // For each commit: gradient rect from node's lane-x to the right edge of
    // the graph column, plus a thin vertical accent line ("bordatura") on the right.
    emit("Rendering row background glows…");
    for node in &graph.nodes {
        let rx  = node_x(node.lane);
        let ry  = node_y(node.row) - ROW_HEIGHT / 2.0;
        let rw  = graph_w - rx;
        if rw <= 0.0 { continue; }
        let ci  = node.color_index % theme.lanes.len();
        let rh  = ROW_HEIGHT as u32;
        let rx1 = format!("{rx:.1}");
        let ry1 = format!("{ry:.1}");
        let rw1 = format!("{rw:.1}");
        wr!("<rect x=\"{rx1}\" y=\"{ry1}\" width=\"{rw1}\" height=\"{rh}\" \
             fill=\"url(#row-grad-{ci})\" opacity=\"0.7\"/>");
        // Bordatura — thin accent line capping the gradient on the right
        let lx  = format!("{:.1}", graph_w - 1.0);
        let ly1 = format!("{:.1}", ry + 3.0);
        let ly2 = format!("{:.1}", ry + ROW_HEIGHT - 3.0);
        let lc  = theme.lane(node.color_index);
        wr!("<line x1=\"{lx}\" y1=\"{ly1}\" x2=\"{lx}\" y2=\"{ly2}\" \
             stroke=\"{lc}\" stroke-width=\"1.5\" stroke-linecap=\"round\"/>");
    }

    // Thin separator line between graph area and text area
    let sep_x = format!("{:.1}", text_base - 3.0);
    let sep_y = format!("{inner_h:.1}");
    let sep   = &theme.sep;
    wr!("<line x1=\"{sep_x}\" y1=\"0\" x2=\"{sep_x}\" y2=\"{sep_y}\" stroke=\"{sep}\" stroke-width=\"1\"/>");

    // ── Edges ────────────────────────────────────────────────────────────────
    emit("Rendering edges…");

    // Squash-merge ghost edges (below everything else)
    for edge in &graph.edges {
        if matches!(edge.edge_type, EdgeType::SquashMerge) {
            let d = esc(&edge_path(edge));
            let c = theme.lane(edge.color_index);
            wr!("<path d=\"{d}\" stroke=\"{c}\" stroke-width=\"1.5\" fill=\"none\" \
                 stroke-dasharray=\"4 3\" opacity=\"0.4\"/>");
        }
    }

    // PASS 1 — glow layer: all non-squash edges blurred as a group.
    // The single filter application means adjacent lanes accumulate glow,
    // matching the neon-trail effect in the live graph.
    wr!("<g filter=\"url(#lane-glow)\">");
    for edge in &graph.edges {
        if !matches!(edge.edge_type, EdgeType::SquashMerge) {
            let d = esc(&edge_path(edge));
            let c = theme.lane(edge.color_index);
            wr!("<path d=\"{d}\" stroke=\"{c}\" stroke-width=\"3.5\" fill=\"none\" \
                 stroke-linecap=\"round\" opacity=\"0.7\"/>");
        }
    }
    wr!("</g>");

    // PASS 2 — crisp edges on top
    for edge in &graph.edges {
        if !matches!(edge.edge_type, EdgeType::SquashMerge) {
            let d = esc(&edge_path(edge));
            let c = theme.lane(edge.color_index);
            wr!("<path d=\"{d}\" stroke=\"{c}\" stroke-width=\"1.8\" fill=\"none\" \
                 stroke-linecap=\"round\" opacity=\"0.88\"/>");
        }
    }

    // ── Nodes + text columns ─────────────────────────────────────────────────
    emit("Rendering nodes and labels…");

    let nr_str = format!("{:.1}", NODE_RADIUS);

    for (i, node) in graph.nodes.iter().enumerate() {
        let cx    = node_x(node.lane);
        let cy    = node_y(node.row);
        let color = theme.lane(node.color_index);
        let cx1   = format!("{cx:.1}");
        let cy1   = format!("{cy:.1}");

        // ── Node ambient halo (behind the node circle) ────────────────────────
        if node.is_head {
            // Strong bloom blob for HEAD
            let hr = format!("{:.1}", NODE_RADIUS + 10.0);
            wr!("<circle cx=\"{cx1}\" cy=\"{cy1}\" r=\"{hr}\" fill=\"{color}\" \
                 opacity=\"0.38\" style=\"filter:blur(10px)\"/>");
            let ar = format!("{:.1}", NODE_RADIUS + 4.0);
            wr!("<circle cx=\"{cx1}\" cy=\"{cy1}\" r=\"{ar}\" fill=\"none\" \
                 stroke=\"{color}\" stroke-width=\"2\" opacity=\"0.75\"/>");
        } else {
            // Subtle ambient halo for all other nodes
            let hr = format!("{:.1}", NODE_RADIUS + 3.0);
            wr!("<circle cx=\"{cx1}\" cy=\"{cy1}\" r=\"{hr}\" fill=\"{color}\" opacity=\"0.11\"/>");
        }

        // ── Node circle ──────────────────────────────────────────────────────
        if node.is_merge {
            // Merge commits stay as a coloured dot — no avatar (matches
            // GraphNode.svelte where merge nodes are pure topology markers).
            // Render at ~65% scale to keep them visually subordinate to
            // avatar nodes, mirroring the live graph's `MR` calc.
            let mr  = NODE_RADIUS * 0.65;
            let mr1 = format!("{mr:.1}");
            let mro = format!("{:.1}", mr + 2.5);
            wr!("<circle cx=\"{cx1}\" cy=\"{cy1}\" r=\"{mr1}\" fill=\"{color}\" opacity=\"0.95\"/>");
            wr!("<circle cx=\"{cx1}\" cy=\"{cy1}\" r=\"{mro}\" fill=\"none\" stroke=\"{color}\" \
                 stroke-width=\"1.5\" opacity=\"0.55\"/>");
        } else {
            // ── Avatar node: ring + initials + gravatar overlay ──────────────
            // Ring colour follows the lane (matches GraphNode.svelte exactly).
            // HEAD gets a slightly thicker stroke for emphasis.
            let ring_w = if node.is_head { 2.5 } else { 1.8 };
            let ring_r = format!("{:.1}", NODE_RADIUS + 1.0);
            wr!("<circle cx=\"{cx1}\" cy=\"{cy1}\" r=\"{ring_r}\" fill=\"none\" \
                 stroke=\"{color}\" stroke-width=\"{ring_w}\"/>");

            // Initials backdrop (deterministic colour from email).
            let initials = esc(&initials_of(&node.author.name));
            let bgcol    = initials_color(&node.author.email);
            wr!("<circle cx=\"{cx1}\" cy=\"{cy1}\" r=\"{nr_str}\" fill=\"{bgcol}\"/>");
            // Centred initials text — y-offset matches the live initials avatar.
            let ty = format!("{:.1}", cy + 0.5);
            wr!("<text x=\"{cx1}\" y=\"{ty}\" text-anchor=\"middle\" \
                 font-family=\"system-ui,-apple-system,Segoe UI,sans-serif\" \
                 font-size=\"9\" font-weight=\"600\" fill=\"#ffffff\">{initials}</text>");

            // Gravatar overlay — `d=blank` so missing gravatars stay
            // transparent and the initials show through.
            if !node.author.email.is_empty() {
                let hash = gravatar_hash(&node.author.email);
                let ix   = format!("{:.1}", cx - NODE_RADIUS);
                let iy   = format!("{:.1}", cy - NODE_RADIUS);
                let sz   = format!("{:.1}", NODE_RADIUS * 2.0);
                wr!("<image href=\"https://www.gravatar.com/avatar/{hash}?s=24&amp;d=blank\" \
                     x=\"{ix}\" y=\"{iy}\" width=\"{sz}\" height=\"{sz}\" \
                     clip-path=\"url(#avatar-clip)\" preserveAspectRatio=\"xMidYMid slice\"/>");
            }
        }

        // ── Short SHA ────────────────────────────────────────────────────────
        let sha_x1 = format!("{sha_x:.1}");
        let sha    = esc(&node.short_oid);
        let sha_c  = &theme.sha;
        wr!("<text x=\"{sha_x1}\" y=\"{cy1}\" font-size=\"10.5\" fill=\"{sha_c}\" letter-spacing=\"0.5\">{sha}</text>");

        // ── Ref badges ───────────────────────────────────────────────────────
        let mut bx = badge_x;
        for rl in &node.refs {
            let (bg_c, fg_c, prefix): (&str, &str, &str) = match rl.ref_type {
                RefType::LocalBranch  => (theme.branch_bg.as_str(), theme.branch_fg.as_str(), if rl.is_current { "● " } else { "" }),
                RefType::RemoteBranch => (theme.remote_bg.as_str(), theme.remote_fg.as_str(), ""),
                RefType::Tag          => (theme.tag_bg.as_str(),    theme.tag_fg.as_str(),    "◆ "),
            };
            let label     = format!("{}{}", prefix, rl.name);
            let label_esc = esc(&label);
            let badge_w   = (label.chars().count() as f64 * 6.1 + 12.0)
                                .min(BADGE_COL_W - 4.0)
                                .max(20.0);
            let badge_h   = 14.0_f64;
            let badge_y   = cy - 8.5;

            let bx1 = format!("{bx:.1}");
            let by1 = format!("{badge_y:.1}");
            let bw1 = format!("{badge_w:.1}");
            let bh1 = badge_h as u32;
            let tx1 = format!("{:.1}", bx + 6.0);
            let ty1 = format!("{:.1}", badge_y + badge_h / 2.0);

            wr!("<rect x=\"{bx1}\" y=\"{by1}\" width=\"{bw1}\" height=\"{bh1}\" rx=\"3\" fill=\"{bg_c}\"/>");
            wr!("<text x=\"{tx1}\" y=\"{ty1}\" font-size=\"9\" fill=\"{fg_c}\">{label_esc}</text>");

            bx += badge_w + 4.0;
            if bx >= badge_x + BADGE_COL_W { break; } // overflow guard
        }

        // ── Author name ──────────────────────────────────────────────────────
        let author    = trunc(&node.author.name, author_max_chars_render.max(3));
        let author    = esc(&author);
        let auth_x1   = format!("{author_x:.1}");
        let author_c  = &theme.author;
        wr!("<text x=\"{auth_x1}\" y=\"{cy1}\" font-size=\"10.5\" fill=\"{author_c}\">{author}</text>");

        // ── Commit summary ───────────────────────────────────────────────────
        let summary  = trunc(&node.summary, summary_max_chars_render.max(3));
        let summary  = esc(&summary);
        let sum_x1   = format!("{summary_x:.1}");
        let msg_fill = if node.is_head { theme.head.as_str() } else { theme.text.as_str() };
        wr!("<text x=\"{sum_x1}\" y=\"{cy1}\" font-size=\"10.5\" fill=\"{msg_fill}\">{summary}</text>");

        // Progress heartbeat every 2 000 commits
        if i > 0 && i % 2_000 == 0 {
            emit(&format!("  {i} / {n_rows} commits rendered…"));
        }
    }

    // ── Close ────────────────────────────────────────────────────────────────
    wr!("</g>"); // close the inner-padding translate group
    wr!("</svg>");
    w.flush().map_err(|e| e.to_string())?;

    Ok(())
}
