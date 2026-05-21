//! Security report exporters — produce a self-contained HTML report or a
//! flat CSV from a `(SecuritySummary, [SecurityFinding])` pair.
//!
//! The HTML report mirrors what the in-app `SecurityPanel` shows: severity
//! counter grid, optional risk-score gauge, optional vulnerabilities-over-
//! time chart, and a findings table. Single file, inline CSS, inline SVG,
//! no JS — opens in any browser, prints cleanly.
//!
//! The CSV is raw data only (one row per finding) — no summary header —
//! so spreadsheets can pivot without parsing a banner.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::git::stats_export::LOGO_SVG;
use crate::git_provider::types::{
    FindingState, ProviderKind, SecurityFinding, SecuritySummary, Severity,
    VulnTimeSeries,
};

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// Captured CSS-variable values from the running app — embedded into the
/// exported HTML so the report matches the user's current theme (dark
/// default, plugin overlays, custom palettes). Defaults to the shipped
/// dark theme so the export still looks right when no tokens are passed.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ThemeTokens {
    pub bg:             String,
    pub bg_elevated:    String,
    pub bg_card:        String,
    pub bg_input:       String,
    pub text_primary:   String,
    pub text_body:      String,
    pub text_muted:     String,
    pub accent:         String,
    pub border:         String,
    pub border_subtle:  String,
    pub warning:        String,
    pub warning_subtle: String,
    pub sev_critical:   String,
    pub sev_high:       String,
    pub sev_medium:     String,
    pub sev_low:        String,
    pub sev_info:       String,
    pub sev_unknown:    String,
}

impl Default for ThemeTokens {
    fn default() -> Self {
        // Mirrors the dark theme palette in `src/app.css` — kept in sync by
        // convention. Used when the frontend doesn't pass a snapshot.
        Self {
            bg:             "#1e1f22".into(),
            bg_elevated:    "#2b2d30".into(),
            bg_card:        "#23252a".into(),
            bg_input:       "#1a1b1e".into(),
            text_primary:   "#dfe1e5".into(),
            text_body:      "#9da0a8".into(),
            text_muted:     "#7a7d85".into(),
            accent:         "#3d7fff".into(),
            border:         "#404348".into(),
            border_subtle:  "#2e3035".into(),
            warning:        "#ffb52a".into(),
            warning_subtle: "rgba(226, 163, 53, 0.12)".into(),
            sev_critical:   "#d04545".into(),
            sev_high:       "#e87b3a".into(),
            sev_medium:     "#d6a93a".into(),
            sev_low:        "#d6c93a".into(),
            sev_info:       "#4a8cd6".into(),
            sev_unknown:    "#8a8a8a".into(),
        }
    }
}

pub fn export_to_file(
    summary:       &SecuritySummary,
    findings:      &[SecurityFinding],
    path:          &Path,
    format:        &str,
    repo_name:     &str,
    logo_override: Option<&str>,
    theme:         &ThemeTokens,
) -> Result<(), String> {
    let bytes = match format {
        "html" => build_html(summary, findings, repo_name, logo_override, theme).into_bytes(),
        "csv"  => build_csv(findings).into_bytes(),
        other  => return Err(format!("Unknown format '{other}'. Expected 'html' or 'csv'.")),
    };
    std::fs::write(path, bytes)
        .map_err(|e| format!("Cannot write '{}': {e}", path.display()))
}

// ---------------------------------------------------------------------------
// Severity / state presentation
// ---------------------------------------------------------------------------

/// CSS custom-property reference for a severity, e.g. `var(--sev-high)`.
/// Resolves at render time against the `:root` block emitted from the
/// captured theme — so a plugin overlay that changes severity tones is
/// honoured in the export too.
fn sev_var(s: Severity) -> &'static str {
    match s {
        Severity::Critical => "var(--sev-critical)",
        Severity::High     => "var(--sev-high)",
        Severity::Medium   => "var(--sev-medium)",
        Severity::Low      => "var(--sev-low)",
        Severity::Info     => "var(--sev-info)",
        Severity::Unknown  => "var(--sev-unknown)",
    }
}

fn sev_label(s: Severity) -> &'static str {
    match s {
        Severity::Critical => "Critical",
        Severity::High     => "High",
        Severity::Medium   => "Medium",
        Severity::Low      => "Low",
        Severity::Info     => "Info",
        Severity::Unknown  => "Unknown",
    }
}

fn sev_csv(s: Severity) -> &'static str {
    // Lowercase form, matching the JSON wire vocabulary.
    match s {
        Severity::Critical => "critical",
        Severity::High     => "high",
        Severity::Medium   => "medium",
        Severity::Low      => "low",
        Severity::Info     => "info",
        Severity::Unknown  => "unknown",
    }
}

fn sev_rank(s: Severity) -> u8 {
    // Inverted — higher rank = sorted first.
    match s {
        Severity::Critical => 6,
        Severity::High     => 5,
        Severity::Medium   => 4,
        Severity::Low      => 3,
        Severity::Info     => 2,
        Severity::Unknown  => 1,
    }
}

fn state_label(s: FindingState) -> &'static str {
    match s {
        FindingState::Detected  => "Detected",
        FindingState::Confirmed => "Confirmed",
        FindingState::Resolved  => "Resolved",
        FindingState::Dismissed => "Dismissed",
    }
}

fn state_csv(s: FindingState) -> &'static str {
    match s {
        FindingState::Detected  => "detected",
        FindingState::Confirmed => "confirmed",
        FindingState::Resolved  => "resolved",
        FindingState::Dismissed => "dismissed",
    }
}

fn provider_label(p: ProviderKind) -> &'static str {
    match p {
        ProviderKind::GitHub => "GitHub",
        ProviderKind::GitLab => "GitLab",
        _                    => "Provider",
    }
}

// ---------------------------------------------------------------------------
// Date helper — same Hinnant civil_from_days trick as stats_export, kept
// local so the modules stay independent.
// ---------------------------------------------------------------------------

fn unix_to_iso(ts: i64) -> String {
    let z = ts.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y   = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp  = (5 * doy + 2) / 153;
    let d   = doy - (153 * mp + 2) / 5 + 1;
    let m   = if mp < 10 { mp + 3 } else { mp - 9 };
    let y   = if m <= 2 { y + 1 } else { y };
    format!("{:04}-{:02}-{:02}", y as i32, m, d)
}

fn current_datetime_str() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let date = unix_to_iso(secs);
    let s_in_day = secs.rem_euclid(86_400);
    let h = (s_in_day / 3600)        as u32;
    let m = ((s_in_day % 3600) / 60) as u32;
    format!("{date} {h:02}:{m:02} UTC")
}

// ---------------------------------------------------------------------------
// HTML escaping
// ---------------------------------------------------------------------------

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
     .replace('<', "&lt;")
     .replace('>', "&gt;")
     .replace('"', "&quot;")
}

// ---------------------------------------------------------------------------
// HTML report
// ---------------------------------------------------------------------------

pub fn build_html(
    summary:       &SecuritySummary,
    findings:      &[SecurityFinding],
    repo_name:     &str,
    logo_override: Option<&str>,
    theme:         &ThemeTokens,
) -> String {
    let logo = logo_override.unwrap_or(LOGO_SVG);
    let provider = provider_label(summary.provider_kind);
    let generated = current_datetime_str();
    let theme_root = render_theme_root(theme);

    // Sort: severity desc → age desc. Returns a Vec of indices to avoid
    // cloning the whole findings list. (Stable sort over a Vec<&_>.)
    let mut sorted: Vec<&SecurityFinding> = findings.iter().collect();
    sorted.sort_by(|a, b| {
        sev_rank(b.severity).cmp(&sev_rank(a.severity))
            .then_with(|| b.age_days.cmp(&a.age_days))
    });

    let counters_html = render_counters(summary);
    let risk_html  = summary.risk_score.as_ref().map(render_gauge).unwrap_or_default();
    let chart_html = summary.time_series.as_ref().map(render_time_series).unwrap_or_default();
    let dashboard_html = if summary.risk_score.is_some() || summary.time_series.is_some() {
        format!(
            r#"<section class="dashboard">
              <div class="dash-grid">
                {risk_html}
                {chart_html}
              </div>
            </section>"#
        )
    } else {
        String::new()
    };

    let truncated_html = if summary.truncated {
        format!(
            r#"<div class="trunc">Showing the first {} findings — refine filters in-app to narrow.</div>"#,
            summary.findings_seen
        )
    } else {
        String::new()
    };

    let table_html = render_findings_table(&sorted);

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Security Report — {title}</title>
<style>{theme_root}{css}</style>
</head>
<body>
<header class="hdr">
  <div class="logo">{logo}</div>
  <div class="hdr-meta">
    <h1>Security Report</h1>
    <div class="hdr-sub">
      <span class="repo">{repo}</span>
      <span class="dot">·</span>
      <span class="prov">{provider}</span>
      <span class="dot">·</span>
      <span class="when">{generated}</span>
    </div>
  </div>
  <div class="hdr-totals">
    <div class="total-num">{total}</div>
    <div class="total-lbl">findings</div>
  </div>
</header>

<section class="counters">
{counters_html}
</section>

{dashboard_html}

<section class="findings-section">
  <div class="findings-head">
    <h2>Findings</h2>
    <span class="findings-count">{count} shown</span>
  </div>
  {truncated_html}
  {table_html}
</section>

<footer class="ftr">
  Generated by Arbor · {generated}
</footer>
</body>
</html>"#,
        title         = esc(repo_name),
        theme_root    = theme_root,
        css           = inline_css(),
        logo          = logo,
        repo          = esc(repo_name),
        provider      = provider,
        generated     = generated,
        total         = summary.counts.total(),
        counters_html = counters_html,
        dashboard_html= dashboard_html,
        count         = sorted.len(),
        truncated_html= truncated_html,
        table_html    = table_html,
    )
}

fn render_counters(summary: &SecuritySummary) -> String {
    let mut s = String::new();
    s.push_str(r#"<div class="counter-grid">"#);
    for sev in Severity::ALL {
        let count = summary.counts.get(sev);
        let median = match sev {
            Severity::Critical => summary.median_age_days.critical,
            Severity::High     => summary.median_age_days.high,
            Severity::Medium   => summary.median_age_days.medium,
            Severity::Low      => summary.median_age_days.low,
            Severity::Info     => summary.median_age_days.info,
            Severity::Unknown  => summary.median_age_days.unknown,
        };
        let median_str = match median {
            Some(d) => format!("{d} days median"),
            None    => "—".to_string(),
        };
        let dim = if count == 0 { " dim" } else { "" };
        s.push_str(&format!(
            r#"<div class="counter{dim}" style="--sev:{color}">
              <div class="counter-label">{label}</div>
              <div class="counter-value">{count}</div>
              <div class="counter-median">{median}</div>
            </div>"#,
            color  = sev_var(sev),
            label  = sev_label(sev),
            count  = count,
            median = median_str,
        ));
    }
    s.push_str("</div>");
    s
}

fn render_gauge(rs: &crate::git_provider::types::RiskScore) -> String {
    // Semicircle 180°, four colored arcs at 0/25/50/75/100, needle points
    // to interpolated value. Identical look to the in-app `<GaugeChart>`.
    const CX: f32 = 180.0;
    const CY: f32 = 180.0;
    const R:  f32 = 140.0;

    let v = rs.value.clamp(0.0, 100.0);
    let needle_angle = std::f32::consts::PI * (1.0 - v / 100.0); // 0% → π, 100% → 0
    let nx = CX + R * 0.85 * needle_angle.cos();
    let ny = CY - R * 0.85 * needle_angle.sin();

    let segments = [
        ( 0.0,  25.0, "var(--sev-info)"),
        (25.0,  50.0, "var(--sev-medium)"),
        (50.0,  75.0, "var(--sev-high)"),
        (75.0, 100.0, "var(--sev-critical)"),
    ];

    let mut arcs = String::new();
    for (a, b, color) in segments {
        arcs.push_str(&arc_path(CX, CY, R, a, b, color));
    }

    format!(
        r##"<div class="dash-cell gauge-cell">
          <h3>Risk score</h3>
          <svg viewBox="0 0 360 220" class="gauge-svg" xmlns="http://www.w3.org/2000/svg">
            {arcs}
            <line x1="{cx}" y1="{cy}" x2="{nx:.1}" y2="{ny:.1}" stroke="var(--text-primary)" stroke-width="3" stroke-linecap="round" />
            <circle cx="{cx}" cy="{cy}" r="6" fill="var(--text-primary)" />
            <text x="{cx}" y="{ty}" text-anchor="middle" font-size="36" font-weight="700" fill="var(--text-primary)">{val:.1}</text>
            <text x="{cx}" y="{lly}" text-anchor="middle" font-size="14" fill="var(--text-muted)">{label} risk</text>
          </svg>
        </div>"##,
        arcs  = arcs,
        cx    = CX,
        cy    = CY,
        nx    = nx,
        ny    = ny,
        val   = v,
        ty    = CY - 20.0,
        lly   = CY + 10.0,
        label = esc(&rs.label),
    )
}

/// SVG arc helper: arc from `start_pct` to `end_pct` (0..100, inclusive)
/// of a 180° semicircle centred at (cx, cy) with radius r.
fn arc_path(cx: f32, cy: f32, r: f32, start_pct: f32, end_pct: f32, color: &str) -> String {
    let a1 = std::f32::consts::PI * (1.0 - start_pct / 100.0);
    let a2 = std::f32::consts::PI * (1.0 - end_pct   / 100.0);
    let x1 = cx + r * a1.cos();
    let y1 = cy - r * a1.sin();
    let x2 = cx + r * a2.cos();
    let y2 = cy - r * a2.sin();
    let large = if (a1 - a2).abs() > std::f32::consts::PI { 1 } else { 0 };
    format!(
        "<path d=\"M {x1:.1} {y1:.1} A {r:.1} {r:.1} 0 {large} 1 {x2:.1} {y2:.1}\" \
         fill=\"none\" stroke=\"{color}\" stroke-width=\"22\" stroke-linecap=\"butt\" />",
    )
}

fn render_time_series(ts: &VulnTimeSeries) -> String {
    if ts.points.is_empty() {
        return String::new();
    }

    const W: f32 = 540.0;
    const H: f32 = 220.0;
    const PAD_L: f32 = 36.0;
    const PAD_R: f32 = 14.0;
    const PAD_T: f32 = 12.0;
    const PAD_B: f32 = 30.0;
    let plot_w = W - PAD_L - PAD_R;
    let plot_h = H - PAD_T - PAD_B;

    // Build series — one polyline per non-empty severity.
    let series: [(Severity, Vec<u32>); 6] = [
        (Severity::Critical, ts.points.iter().map(|p| p.critical).collect()),
        (Severity::High,     ts.points.iter().map(|p| p.high    ).collect()),
        (Severity::Medium,   ts.points.iter().map(|p| p.medium  ).collect()),
        (Severity::Low,      ts.points.iter().map(|p| p.low     ).collect()),
        (Severity::Info,     ts.points.iter().map(|p| p.info    ).collect()),
        (Severity::Unknown,  ts.points.iter().map(|p| p.unknown ).collect()),
    ];
    let visible: Vec<&(Severity, Vec<u32>)> = series.iter()
        .filter(|(_, vals)| vals.iter().any(|&v| v > 0))
        .collect();

    if visible.is_empty() {
        return format!(
            r#"<div class="dash-cell chart-cell">
              <h3>Vulnerabilities over time</h3>
              <div class="chart-empty">No findings recorded in the selected window.</div>
            </div>"#
        );
    }

    let n = ts.points.len();
    let max_y = visible.iter()
        .flat_map(|(_, vals)| vals.iter().copied())
        .max()
        .unwrap_or(1)
        .max(1);
    let max_y_nice = nice_ceil(max_y);

    // Y-axis ticks: 5 evenly-spaced ticks 0..max_y_nice
    let mut grid = String::new();
    for i in 0..=4 {
        let frac = i as f32 / 4.0;
        let y = PAD_T + plot_h * (1.0 - frac);
        let val = (max_y_nice as f32 * frac).round() as u32;
        grid.push_str(&format!(
            "<line x1=\"{l:.1}\" y1=\"{y:.1}\" x2=\"{r:.1}\" y2=\"{y:.1}\" stroke=\"var(--border-subtle)\" stroke-width=\"1\" />\
             <text x=\"{tx:.1}\" y=\"{ty:.1}\" font-size=\"10\" fill=\"var(--text-muted)\" text-anchor=\"end\">{val}</text>",
            l  = PAD_L,
            r  = W - PAD_R,
            y  = y,
            tx = PAD_L - 4.0,
            ty = y + 3.0,
            val = val,
        ));
    }

    // X-axis: up to 5 evenly-spaced date labels
    let label_count = 5.min(n);
    let mut x_labels = String::new();
    for i in 0..label_count {
        let idx = if label_count == 1 { 0 } else { i * (n - 1) / (label_count - 1) };
        let x = PAD_L + plot_w * (idx as f32 / (n.max(2) - 1) as f32);
        let date_short = ts.points[idx].date.get(5..10).unwrap_or(&ts.points[idx].date);
        x_labels.push_str(&format!(
            "<text x=\"{x:.1}\" y=\"{y:.1}\" font-size=\"10\" fill=\"var(--text-muted)\" text-anchor=\"middle\">{d}</text>",
            x = x,
            y = H - 10.0,
            d = esc(date_short),
        ));
    }

    let mut paths = String::new();
    for (sev, vals) in &visible {
        let mut d = String::new();
        for (i, &v) in vals.iter().enumerate() {
            let x = PAD_L + plot_w * (i as f32 / (n.max(2) - 1) as f32);
            let y = PAD_T + plot_h * (1.0 - v as f32 / max_y_nice as f32);
            if i == 0 { d.push_str(&format!("M {x:.1} {y:.1}")); }
            else      { d.push_str(&format!(" L {x:.1} {y:.1}")); }
        }
        paths.push_str(&format!(
            "<path d=\"{d}\" fill=\"none\" stroke=\"{c}\" stroke-width=\"2\" stroke-linejoin=\"round\" />",
            d = d,
            c = sev_var(*sev),
        ));
    }

    let mut legend = String::new();
    legend.push_str(r#"<div class="chart-legend">"#);
    for (sev, _) in &visible {
        legend.push_str(&format!(
            r#"<span class="legend-item"><span class="legend-dot" style="background:{c}"></span>{l}</span>"#,
            c = sev_var(*sev),
            l = sev_label(*sev),
        ));
    }
    legend.push_str("</div>");

    format!(
        r#"<div class="dash-cell chart-cell">
          <div class="chart-head">
            <h3>Vulnerabilities over time</h3>
            <span class="chart-range">{range} days</span>
          </div>
          <svg viewBox="0 0 {w} {h}" class="chart-svg" xmlns="http://www.w3.org/2000/svg" preserveAspectRatio="xMidYMid meet">
            {grid}
            {paths}
            {x_labels}
          </svg>
          {legend}
        </div>"#,
        range = ts.range_days,
        w = W as i32,
        h = H as i32,
    )
}

/// Round `n` up to a "nice" axis ceiling (1/2/5 × 10^k).
fn nice_ceil(n: u32) -> u32 {
    if n == 0 { return 1; }
    let exp = (n as f64).log10().floor() as i32;
    let pow = 10f64.powi(exp);
    let m   = n as f64 / pow;
    let nice = if      m <= 1.0 { 1.0 }
               else if m <= 2.0 { 2.0 }
               else if m <= 5.0 { 5.0 }
               else             { 10.0 };
    (nice * pow).round() as u32
}

fn render_findings_table(sorted: &[&SecurityFinding]) -> String {
    if sorted.is_empty() {
        return r#"<div class="empty-table">No findings to display.</div>"#.to_string();
    }

    let mut rows = String::new();
    for f in sorted {
        let title_cell = match &f.web_url {
            Some(u) => format!(r#"<a href="{u}" target="_blank" rel="noopener">{t}</a>"#,
                u = esc(u),
                t = esc(&f.title),
            ),
            None => esc(&f.title),
        };
        let file_cell = match (&f.file_path, f.start_line) {
            (Some(p), Some(l)) => format!("{p}:{l}", p = esc(p)),
            (Some(p), None)    => esc(p),
            _                  => "—".to_string(),
        };
        let scanner   = f.scanner.as_deref().map(esc).unwrap_or_else(|| "—".into());
        let category  = f.report_type.as_deref().map(format_report_type).unwrap_or_else(|| "—".into());

        rows.push_str(&format!(
            r#"<tr>
              <td><span class="sev-pill" style="--sev:{c};color:{c}">{lbl}</span></td>
              <td class="t-title">{title}</td>
              <td class="t-file"><code>{file}</code></td>
              <td>{scanner}</td>
              <td>{category}</td>
              <td class="t-state">{state}</td>
              <td class="t-age">{age}d</td>
            </tr>"#,
            c = sev_var(f.severity),
            lbl = sev_label(f.severity),
            title = title_cell,
            file = file_cell,
            scanner = scanner,
            category = category,
            state = state_label(f.state),
            age = f.age_days,
        ));
    }

    format!(
        r#"<table class="findings-table">
          <thead>
            <tr>
              <th class="th-sev">Severity</th>
              <th>Title</th>
              <th>File</th>
              <th>Scanner</th>
              <th>Type</th>
              <th>State</th>
              <th class="th-age">Age</th>
            </tr>
          </thead>
          <tbody>{rows}</tbody>
        </table>"#
    )
}

fn format_report_type(rt: &str) -> String {
    // snake_case → Title Case
    rt.split('_')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut c = p.chars();
            match c.next() {
                Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
                None        => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------
// CSS
// ---------------------------------------------------------------------------

/// Emit a `:root { --bg: …; --sev-critical: …; … }` block using the captured
/// theme tokens. Kept ahead of the static stylesheet so callers see all the
/// custom-property values up-front.
fn render_theme_root(t: &ThemeTokens) -> String {
    format!(
        ":root{{\
         --bg:{bg};--bg-elevated:{be};--bg-card:{bc};--bg-input:{bi};\
         --text-primary:{tp};--text-body:{tb};--text-muted:{tm};\
         --accent:{a};--border:{br};--border-subtle:{bs};\
         --warning:{w};--warning-subtle:{ws};\
         --sev-critical:{sc};--sev-high:{sh};--sev-medium:{sm};\
         --sev-low:{sl};--sev-info:{si};--sev-unknown:{su};\
         }}",
        bg = t.bg, be = t.bg_elevated, bc = t.bg_card, bi = t.bg_input,
        tp = t.text_primary, tb = t.text_body, tm = t.text_muted,
        a  = t.accent, br = t.border, bs = t.border_subtle,
        w  = t.warning, ws = t.warning_subtle,
        sc = t.sev_critical, sh = t.sev_high, sm = t.sev_medium,
        sl = t.sev_low, si = t.sev_info, su = t.sev_unknown,
    )
}

fn inline_css() -> &'static str {
    // All colours flow from the `:root` block emitted by `render_theme_root`,
    // so this stylesheet adapts to whatever theme the user had active when
    // they triggered the export (default dark, plugin overlay, custom palette).
    r#"
    *,*::before,*::after { box-sizing: border-box; }
    html, body { margin: 0; padding: 0; }
    body {
      font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif;
      background: var(--bg);
      color: var(--text-body);
      font-size: 13px;
      line-height: 1.5;
    }

    .hdr {
      display: flex;
      align-items: center;
      gap: 18px;
      padding: 20px 32px;
      background: var(--bg-elevated);
      color: var(--text-primary);
      border-bottom: 1px solid var(--border-subtle);
    }
    .hdr .logo {
      flex: 0 0 auto;
      width: 48px;
      height: 48px;
      display: flex;
      align-items: center;
      justify-content: center;
      overflow: hidden;
    }
    /* Force any logo SVG (Arbor default or branding override) to fit the
       fixed-size container — branding SVGs typically have no intrinsic
       width/height attribute and would otherwise blow up the header. */
    .hdr .logo > svg { width: 100%; height: 100%; display: block; }
    .hdr-meta { flex: 1 1 auto; min-width: 0; }
    .hdr h1 { margin: 0 0 4px; font-size: 18px; font-weight: 600; letter-spacing: -0.01em; color: var(--text-primary); }
    .hdr-sub { font-size: 12px; color: var(--text-muted); }
    .hdr-sub .dot { margin: 0 6px; opacity: 0.5; }
    .hdr-totals { text-align: right; flex: 0 0 auto; }
    .hdr-totals .total-num { font-size: 28px; font-weight: 700; line-height: 1; color: var(--text-primary); }
    .hdr-totals .total-lbl { font-size: 10px; color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.05em; margin-top: 4px; }

    section { padding: 20px 32px; }

    .counters { padding-bottom: 4px; }
    .counter-grid {
      display: grid;
      grid-template-columns: repeat(6, 1fr);
      gap: 12px;
    }
    .counter {
      background: var(--bg-elevated);
      border: 1px solid var(--border-subtle);
      border-left: 3px solid var(--sev);
      border-radius: 6px;
      padding: 12px 14px;
    }
    .counter.dim { opacity: 0.5; }
    .counter-label {
      font-size: 10px;
      font-weight: 700;
      color: var(--sev);
      text-transform: uppercase;
      letter-spacing: 0.06em;
    }
    .counter-value {
      font-size: 26px;
      font-weight: 700;
      color: var(--text-primary);
      line-height: 1.1;
      margin-top: 4px;
    }
    .counter-median {
      font-size: 11px;
      color: var(--text-muted);
      margin-top: 2px;
    }

    .dashboard { padding-top: 4px; }
    .dash-grid {
      display: grid;
      grid-template-columns: minmax(220px, 320px) 1fr;
      gap: 16px;
      align-items: start;
    }
    @media (max-width: 720px) {
      .counter-grid { grid-template-columns: repeat(3, 1fr); }
      .dash-grid    { grid-template-columns: 1fr; }
    }
    .dash-cell {
      background: var(--bg-elevated);
      border: 1px solid var(--border-subtle);
      border-radius: 6px;
      padding: 14px 16px;
    }
    .dash-cell h3 {
      margin: 0 0 8px;
      font-size: 10px;
      font-weight: 700;
      color: var(--text-muted);
      text-transform: uppercase;
      letter-spacing: 0.06em;
    }
    .gauge-cell { text-align: center; }
    .gauge-svg  { width: 100%; max-width: 320px; height: auto; }
    .chart-svg  { width: 100%; height: auto; }
    .chart-head { display: flex; justify-content: space-between; align-items: baseline; }
    .chart-range { font-size: 11px; color: var(--text-muted); }
    .chart-empty {
      padding: 32px 12px;
      text-align: center;
      color: var(--text-muted);
      font-size: 12px;
    }
    .chart-legend {
      display: flex;
      flex-wrap: wrap;
      gap: 12px;
      margin-top: 8px;
      font-size: 11px;
      color: var(--text-body);
    }
    .legend-item { display: inline-flex; align-items: center; gap: 6px; }
    .legend-dot {
      width: 9px; height: 9px;
      border-radius: 50%;
      display: inline-block;
    }

    .findings-section { padding-top: 4px; padding-bottom: 28px; }
    .findings-head {
      display: flex;
      align-items: baseline;
      justify-content: space-between;
      margin-bottom: 10px;
    }
    .findings-head h2 { margin: 0; font-size: 14px; font-weight: 600; color: var(--text-primary); }
    .findings-count { font-size: 11px; color: var(--text-muted); }
    .trunc {
      background: var(--warning-subtle);
      border: 1px solid var(--warning);
      color: var(--warning);
      font-size: 11px;
      padding: 8px 12px;
      border-radius: 4px;
      margin-bottom: 10px;
    }
    .findings-table {
      width: 100%;
      border-collapse: collapse;
      background: var(--bg-elevated);
      border: 1px solid var(--border-subtle);
      border-radius: 6px;
      overflow: hidden;
      font-size: 12px;
    }
    .findings-table th, .findings-table td {
      padding: 8px 12px;
      text-align: left;
      border-bottom: 1px solid var(--border-subtle);
    }
    .findings-table thead th {
      background: var(--bg-card);
      font-size: 10px;
      font-weight: 700;
      color: var(--text-muted);
      text-transform: uppercase;
      letter-spacing: 0.06em;
      border-bottom: 1px solid var(--border);
    }
    .findings-table tbody tr:last-child td { border-bottom: none; }
    .findings-table tbody tr:hover { background: var(--bg-card); }
    .th-sev { width: 90px; }
    .th-age { width: 60px; text-align: right; }
    .t-age  { text-align: right; color: var(--text-muted); }
    .t-state { color: var(--text-muted); }
    .t-title { color: var(--text-primary); }
    .t-title a { color: var(--accent); text-decoration: none; }
    .t-title a:hover { text-decoration: underline; }
    .t-file code {
      font-family: ui-monospace, 'SFMono-Regular', Menlo, Consolas, monospace;
      font-size: 11px;
      color: var(--text-body);
      background: var(--bg-input);
      padding: 1px 6px;
      border-radius: 3px;
    }
    .empty-table {
      background: var(--bg-elevated);
      border: 1px dashed var(--border);
      border-radius: 6px;
      padding: 28px;
      text-align: center;
      color: var(--text-muted);
    }
    .sev-pill {
      display: inline-block;
      padding: 2px 8px;
      font-size: 10px;
      font-weight: 700;
      border-radius: 10px;
      text-transform: uppercase;
      letter-spacing: 0.06em;
      background: color-mix(in srgb, var(--sev) 16%, transparent);
      border: 1px solid color-mix(in srgb, var(--sev) 35%, transparent);
    }

    .ftr {
      padding: 14px 32px;
      font-size: 11px;
      color: var(--text-muted);
      text-align: center;
      border-top: 1px solid var(--border-subtle);
      background: var(--bg-elevated);
    }

    @media print {
      body { background: white; color: #1f2937; }
      .hdr, .dash-cell, .findings-table, .counter, .ftr {
        -webkit-print-color-adjust: exact;
        print-color-adjust: exact;
      }
      .findings-table tbody tr:hover { background: var(--bg-elevated); }
      section, .ftr { padding-left: 12px; padding-right: 12px; }
    }
    "#
}

// ---------------------------------------------------------------------------
// CSV
// ---------------------------------------------------------------------------

pub fn build_csv(findings: &[SecurityFinding]) -> String {
    let mut s = String::new();
    s.push_str("severity,state,title,scanner,report_type,file_path,start_line,age_days,created_at,web_url,identifiers\n");
    for f in findings {
        let identifiers = f.identifiers.iter()
            .map(|i| format!("{}:{}", i.kind, i.value))
            .collect::<Vec<_>>()
            .join(";");
        s.push_str(&format!(
            "{sev},{state},{title},{scanner},{rtype},{file},{line},{age},{created},{url},{ids}\n",
            sev     = sev_csv(f.severity),
            state   = state_csv(f.state),
            title   = csv_field(&f.title),
            scanner = csv_field(f.scanner.as_deref().unwrap_or("")),
            rtype   = csv_field(f.report_type.as_deref().unwrap_or("")),
            file    = csv_field(f.file_path.as_deref().unwrap_or("")),
            line    = f.start_line.map(|l| l.to_string()).unwrap_or_default(),
            age     = f.age_days,
            created = csv_field(&f.created_at),
            url     = csv_field(f.web_url.as_deref().unwrap_or("")),
            ids     = csv_field(&identifiers),
        ));
    }
    s
}

/// CSV escape per RFC 4180: wrap in double quotes when the value contains a
/// comma, quote, newline, or carriage return; double internal quotes.
fn csv_field(s: &str) -> String {
    let needs_quote = s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r');
    if !needs_quote {
        return s.to_string();
    }
    let escaped = s.replace('"', "\"\"");
    format!("\"{escaped}\"")
}
