//! VSCO 2 Community Edition pack: turn the **raw wav tree** into playable SFZ
//! instruments at index time.
//!
//! The `sgossner/VSCO-2-CE` archive ships **no** `.sfz` files — only thousands of
//! `.wav` one-shots named by pitch (and velocity / round-robin), laid out as
//! `<Family>/<Instrument>/<Articulation>/<…>_<Pitch>_<vN>[_rrN].wav` (some
//! instruments keep the wavs directly under the instrument folder). So the generic
//! [`Layout::SfzTree`](super::Layout) finds nothing. This module builds the
//! missing layer: for every articulation folder it parses the wav filenames into
//! (pitch, velocity-layer, round-robin), writes a `_nemus.sfz` mapping them across
//! the keyboard, and emits a registry entry per instrument — its default
//! (sustain-preferred) articulation as the base voice, the rest as `.art(…)`
//! region alternates.
//!
//! The output uses exactly the SFZ opcodes the engine's loader understands
//! (`crate`'s `sfz.rs`): `sample`, `lokey/hikey/pitch_keycenter`, `lovel/hivel`,
//! `seq_length/seq_position`, `ampeg_*`. No loop points (VSCO CE wavs carry none),
//! so very long held notes play to the sample's natural end.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Generate SFZ instruments + the registry TOML body for an extracted VSCO-2-CE
/// tree rooted at `root`. Writes one `_nemus.sfz` into each sample-set folder as a
/// side effect; returns `(registry_toml, instrument_count)` — the same shape as
/// [`super::layout::generate`], so the install/re-index paths treat it uniformly.
pub fn generate(root: &Path) -> (String, usize) {
    let mut out = String::from("# Auto-generated VSCO 2 registry (nemus).\n\n");
    let mut count = 0usize;

    // Top-level dirs are instrument families (Strings, Brass, Woodwinds, …).
    let mut families = child_dirs(root);
    families.sort();
    for family in &families {
        let fam_token = family_token(&file_name(family));
        let mut instruments = child_dirs(family);
        instruments.sort();
        for inst in &instruments {
            let inst_token = sanitize(&file_name(inst));
            if inst_token.is_empty() {
                continue;
            }
            let sets = sample_sets(inst);
            if sets.is_empty() {
                continue;
            }
            // Build an SFZ per articulation; collect (art_name, sfz_rel_path).
            let mut arts: Vec<(String, String)> = Vec::new();
            for set in &sets {
                let Some(sfz_rel) = write_set_sfz(root, &set.dir) else {
                    continue;
                };
                arts.push((art_name(&set.label), sfz_rel));
            }
            if arts.is_empty() {
                continue;
            }
            // Default articulation: the sustain-like one if present, else the first.
            let default_idx = arts
                .iter()
                .position(|(a, _)| a == "sustain")
                .unwrap_or(0);
            // Emit one registry entry **per articulation** rather than a single
            // entry with `art.*.region` alternates. Separate entries load lazily —
            // only the referenced voice decodes its samples — whereas one entry's
            // articulations ALL eager-decode the instant the instrument is named
            // (hundreds of MB + a long stall for a few sustained notes). The
            // sustain keeps the bare `<family>.<instrument>` name; the others get a
            // `.<articulation>` suffix (`strings.violin_section.pizzicato`).
            let base = format!("{fam_token}.{inst_token}");
            out.push_str(&format!(
                "[\"{base}\"]\nkind = \"sfz\"\nfile = \"{}\"\n\n",
                arts[default_idx].1
            ));
            count += 1;
            for (i, (art, rel)) in arts.iter().enumerate() {
                if i == default_idx {
                    continue;
                }
                out.push_str(&format!("[\"{base}.{art}\"]\nkind = \"sfz\"\nfile = \"{rel}\"\n\n"));
                count += 1;
            }
        }
    }

    (out, count)
}

// ── Sample-set discovery ───────────────────────────────────────────────────────

/// One articulation's samples: the folder holding the wavs + a human label
/// (the articulation folder's name, or the instrument's when wavs sit directly
/// under it).
struct SampleSet {
    dir: PathBuf,
    label: String,
}

/// The articulation sample-sets of an instrument folder: every subfolder that
/// holds wavs is one articulation, **and** wavs sitting directly under the
/// instrument folder are themselves a (default `sustain`) set. Handling both —
/// not just one — matters for the mixed instruments that keep a default set of
/// loose wavs alongside articulation subfolders; using only the loose wavs (or
/// only the subfolders) silently drops the other half.
fn sample_sets(inst: &Path) -> Vec<SampleSet> {
    let mut sets = Vec::new();
    let mut subs = child_dirs(inst);
    subs.sort();
    for sub in subs {
        if dir_has_wavs(&sub) {
            let label = file_name(&sub);
            sets.push(SampleSet { dir: sub, label });
        }
    }
    if dir_has_wavs(inst) {
        sets.push(SampleSet { dir: inst.to_path_buf(), label: "sustain".to_string() });
    }
    sets
}

// ── SFZ emission for one articulation folder ───────────────────────────────────

/// A wav parsed into the bits the mapping needs.
struct ParsedWav {
    file: String,
    midi: u8,
    /// Velocity-layer key (`v1`, `mf`, …); `None` when the name carries none.
    vel: Option<String>,
    /// Round-robin variant index (1-based) when the name carries one.
    rr: Option<u32>,
}

/// Parse + map every wav in `dir` into an SFZ, written as `dir/_nemus.sfz`.
/// Returns the SFZ path **relative to `root`** (forward slashes) for the registry
/// `file =` / `art.*.region =`. `None` when the folder yields no pitched sample.
fn write_set_sfz(root: &Path, dir: &Path) -> Option<String> {
    let mut parsed: Vec<ParsedWav> = Vec::new();
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| is_wav(p))
        .collect();
    entries.sort();
    for path in &entries {
        let Some(name) = path.file_name().map(|s| s.to_string_lossy().to_string()) else {
            continue;
        };
        if let Some(p) = parse_wav(&name) {
            parsed.push(p);
        }
    }
    if parsed.is_empty() {
        return None;
    }

    // Key ranges: split the keyboard at the midpoints between sampled pitches
    // (computed over the union of pitches, so coverage has no gaps).
    let key_ranges = key_ranges(&parsed);

    let sustained = !is_short_articulation(dir);
    let mut sfz = String::from("// Auto-generated by nemus from VSCO 2 CE wavs.\n");
    sfz.push_str(&group_header(sustained));

    // Nest variants by pitch → velocity-layer → round-robin variants. Velocity
    // banding is done **per pitch** (not globally) so every pitch always covers
    // the whole 0..127 velocity range even when a pitch is missing a layer some
    // other pitch has — otherwise that note would have a velocity hole.
    let mut by_pitch: BTreeMap<u8, BTreeMap<String, Vec<&ParsedWav>>> = BTreeMap::new();
    for p in &parsed {
        let vel = p.vel.clone().unwrap_or_default();
        by_pitch.entry(p.midi).or_default().entry(vel).or_default().push(p);
    }
    for (midi, by_vel) in &by_pitch {
        let (lokey, hikey) = key_ranges.get(midi).copied().unwrap_or((*midi, *midi));
        let layers: Vec<String> = by_vel.keys().cloned().collect();
        let bands = vel_bands_for(&layers);
        for (vel, variants) in by_vel {
            let (lovel, hivel) = bands.get(vel).copied().unwrap_or((0, 127));
            let seq_len = variants.len() as u32;
            for (i, p) in variants.iter().enumerate() {
                sfz.push_str("<region>\n");
                sfz.push_str(&format!("sample={}\n", p.file));
                sfz.push_str(&format!(
                    "lokey={lokey} hikey={hikey} pitch_keycenter={}\n",
                    p.midi
                ));
                sfz.push_str(&format!("lovel={lovel} hivel={hivel}\n"));
                if seq_len > 1 {
                    sfz.push_str(&format!("seq_length={seq_len} seq_position={}\n", i as u32 + 1));
                }
                sfz.push('\n');
            }
        }
    }

    let sfz_path = dir.join("_nemus.sfz");
    std::fs::write(&sfz_path, sfz).ok()?;
    let rel = sfz_path
        .strip_prefix(root)
        .ok()?
        .to_string_lossy()
        .replace('\\', "/");
    Some(rel)
}

/// The `<group>` header carrying the shared amp envelope. Sustained families get
/// a soft attack + long release; short articulations (pizz/spiccato/staccato/
/// percussion) a near-instant attack + short release.
fn group_header(sustained: bool) -> String {
    if sustained {
        "<group>\nloop_mode=no_loop ampeg_attack=0.02 ampeg_release=0.3\n\n".to_string()
    } else {
        "<group>\nloop_mode=no_loop ampeg_attack=0.001 ampeg_release=0.15\n\n".to_string()
    }
}

// ── Filename parsing ───────────────────────────────────────────────────────────

/// Parse a VSCO wav filename into (pitch, velocity-layer, round-robin). The pitch
/// is the single `_`/space-separated token that reads as a note name (e.g. `A2`,
/// `Db4`); velocity is a `v<n>` / dynamic token; round-robin an `rr<n>` token or a
/// trailing bare number. `None` when no pitch token is present.
fn parse_wav(file: &str) -> Option<ParsedWav> {
    let stem = file.rsplit_once('.').map(|(s, _)| s).unwrap_or(file);
    let tokens: Vec<&str> = stem.split(['_', ' ', '-']).filter(|t| !t.is_empty()).collect();

    let mut midi = None;
    let mut vel = None;
    let mut rr = None;
    let mut pitch_idx = None;
    for (i, tok) in tokens.iter().enumerate() {
        if midi.is_none() {
            if let Some(m) = note_to_midi(tok) {
                midi = Some(m);
                pitch_idx = Some(i);
                continue;
            }
        }
        if vel.is_none() {
            if let Some(v) = vel_key(tok) {
                vel = Some(v);
                continue;
            }
        }
        if rr.is_none() {
            if let Some(n) = rr_index(tok) {
                rr = Some(n);
            }
        }
    }
    let midi = midi?;
    // A bare trailing number after the pitch but with no `rr`/`v` reading (the
    // `susvib_A2_v1_1.wav` case) is a round-robin counter.
    if rr.is_none() {
        if let (Some(pi), Some(last)) = (pitch_idx, tokens.last()) {
            if tokens.len() > pi + 1 {
                if let Ok(n) = last.parse::<u32>() {
                    rr = Some(n);
                }
            }
        }
    }
    Some(ParsedWav { file: file.to_string(), midi, vel, rr })
}

/// Parse a note-name token (`A2`, `c4`, `Db3`, `F#5`) to a MIDI number using the
/// `c4 = 60` convention (scientific pitch, what VSCO labels follow). Requires a
/// trailing octave digit, so non-pitch tokens (`v1`, `Vib`, `Ens`) never match.
fn note_to_midi(tok: &str) -> Option<u8> {
    let b = tok.as_bytes();
    if b.is_empty() {
        return None;
    }
    let base = match b[0].to_ascii_lowercase() {
        b'c' => 0,
        b'd' => 2,
        b'e' => 4,
        b'f' => 5,
        b'g' => 7,
        b'a' => 9,
        b'b' => 11,
        _ => return None,
    };
    let mut idx = 1;
    let mut semi: i32 = base;
    if idx < b.len() {
        match b[idx] {
            b'#' | b's' | b'S' => { semi += 1; idx += 1; }
            b'b' | b'f' | b'F' => { semi -= 1; idx += 1; }
            _ => {}
        }
    }
    // The remainder must be a (possibly negative) octave number, nothing else.
    let oct: i32 = tok.get(idx..)?.parse().ok()?;
    let midi = (oct + 1) * 12 + semi;
    u8::try_from(midi).ok()
}

/// A velocity-layer key from a token: `v1`/`v2`… or a dynamic marking (`pp`..`ff`).
fn vel_key(tok: &str) -> Option<String> {
    let lower = tok.to_ascii_lowercase();
    if lower.len() >= 2 && lower.starts_with('v') && lower[1..].chars().all(|c| c.is_ascii_digit())
    {
        return Some(lower);
    }
    matches!(lower.as_str(), "pp" | "p" | "mp" | "mf" | "f" | "ff" | "fff").then_some(lower)
}

/// A round-robin index from an `rr<n>` token.
fn rr_index(tok: &str) -> Option<u32> {
    let lower = tok.to_ascii_lowercase();
    lower.strip_prefix("rr").and_then(|n| n.parse().ok())
}

// ── Mapping helpers ────────────────────────────────────────────────────────────

/// Map a pitch's velocity-layer keys to `[lovel, hivel]` bands, splitting `0..=127`
/// evenly across the layers in intensity order. A single (or absent) layer covers
/// the whole range, so every pitch is always fully velocity-covered.
fn vel_bands_for(layer_keys: &[String]) -> BTreeMap<String, (u8, u8)> {
    let mut keys: Vec<String> = layer_keys.to_vec();
    keys.sort_by_key(|k| vel_rank(k));
    keys.dedup();
    let mut bands = BTreeMap::new();
    if keys.is_empty() {
        bands.insert(String::new(), (0u8, 127u8));
        return bands;
    }
    let n = keys.len() as u32;
    for (i, k) in keys.iter().enumerate() {
        let lo = (i as u32 * 128 / n) as u8;
        let hi = if i as u32 + 1 == n { 127 } else { ((i as u32 + 1) * 128 / n - 1) as u8 };
        bands.insert(k.clone(), (lo, hi));
    }
    bands
}

/// Intensity rank for ordering velocity layers (`v1 < v2 …`, `pp < p < … < fff`).
fn vel_rank(k: &str) -> i32 {
    if let Some(n) = k.strip_prefix('v') {
        if let Ok(v) = n.parse::<i32>() {
            return v;
        }
    }
    match k {
        "pp" => 0,
        "p" => 1,
        "mp" => 2,
        "mf" => 3,
        "f" => 4,
        "ff" => 5,
        "fff" => 6,
        _ => 99,
    }
}

/// Assign each sampled pitch a `[lokey, hikey]` range by splitting at the
/// midpoints between adjacent sampled pitches; the lowest/highest extend to the
/// keyboard ends so there are no gaps.
fn key_ranges(parsed: &[ParsedWav]) -> BTreeMap<u8, (u8, u8)> {
    let mut pitches: Vec<u8> = parsed.iter().map(|p| p.midi).collect();
    pitches.sort_unstable();
    pitches.dedup();
    let mut ranges = BTreeMap::new();
    for (i, &p) in pitches.iter().enumerate() {
        let lo = if i == 0 {
            0
        } else {
            let prev = pitches[i - 1];
            (prev + p) / 2 + 1
        };
        let hi = if i + 1 == pitches.len() {
            127
        } else {
            let next = pitches[i + 1];
            (p + next) / 2
        };
        ranges.insert(p, (lo, hi));
    }
    ranges
}

// ── Naming ─────────────────────────────────────────────────────────────────────

/// The dotted-name family token for a top-level VSCO folder. Common families get
/// the short names the sound catalogue describes (`strings`, `brass`, `ww`,
/// `keys`, `perc`); anything else is sanitised verbatim.
fn family_token(name: &str) -> String {
    let s = sanitize(name);
    match s.as_str() {
        "strings" => "strings",
        "brass" => "brass",
        "woodwinds" | "winds" => "ww",
        "keys" | "keyboards" | "keyboard" => "keys",
        "percussion" => "perc",
        _ => return s,
    }
    .to_string()
}

/// Map an articulation folder name to a clean `.art(…)` name. Sustains (incl.
/// `arco`) become the default `"sustain"`; the common short articulations get
/// canonical names; anything else is sanitised verbatim.
fn art_name(label: &str) -> String {
    let s = sanitize(label);
    if s.contains("sus") || s.contains("arco") || s.contains("long") {
        return "sustain".to_string();
    }
    if s.contains("pizz") {
        return "pizzicato".to_string();
    }
    if s.contains("spic") {
        return "spiccato".to_string();
    }
    if s.contains("trem") {
        return "tremolo".to_string();
    }
    if s.contains("stac") {
        return "staccato".to_string();
    }
    if s.contains("legato") || s.contains("leg") {
        return "legato".to_string();
    }
    s
}

/// Whether a sample-set folder holds a *short* articulation (pizz/spiccato/
/// staccato/percussion) — those get a snappy envelope instead of a sustained one.
fn is_short_articulation(dir: &Path) -> bool {
    let s = sanitize(&file_name(dir));
    s.contains("pizz") || s.contains("spic") || s.contains("stac") || s.contains("perc")
}

/// Lowercase + collapse any run of non-alphanumerics to a single `_` (and trim
/// leading/trailing `_`). `"Violin Section"` → `"violin_section"`.
fn sanitize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut pending_us = false;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            if pending_us && !out.is_empty() {
                out.push('_');
            }
            pending_us = false;
            out.push(c.to_ascii_lowercase());
        } else {
            pending_us = true;
        }
    }
    out
}

// ── Filesystem helpers ─────────────────────────────────────────────────────────

/// The immediate child directories of `dir` (empty on any read error).
fn child_dirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect()
}

/// Whether `dir` directly contains at least one `.wav`.
fn dir_has_wavs(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|e| is_wav(&e.path()))
}

/// Whether `path` is a `.wav` file.
fn is_wav(path: &Path) -> bool {
    path.is_file()
        && path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("wav"))
}

/// `path`'s file name as an owned `String` (empty if none).
fn file_name(path: &Path) -> String {
    path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_parsing_scientific_c4_60() {
        assert_eq!(note_to_midi("C4"), Some(60));
        assert_eq!(note_to_midi("A4"), Some(69));
        assert_eq!(note_to_midi("a2"), Some(45));
        assert_eq!(note_to_midi("Db4"), Some(61));
        assert_eq!(note_to_midi("F#5"), Some(78));
        // Non-pitch tokens never match (no trailing octave).
        assert_eq!(note_to_midi("v1"), None);
        assert_eq!(note_to_midi("Vib"), None);
        assert_eq!(note_to_midi("Ens"), None);
        assert_eq!(note_to_midi("rr2"), None);
    }

    #[test]
    fn filename_shapes_parse() {
        // inst_artic_pitch_vel
        let a = parse_wav("VlnEns_susVib_A2_v1.wav").unwrap();
        assert_eq!(a.midi, 45);
        assert_eq!(a.vel.as_deref(), Some("v1"));
        assert_eq!(a.rr, None);
        // inst_artic_pitch_vel_rr
        let b = parse_wav("VlnEns_Pizz_A2_v1_rr2.wav").unwrap();
        assert_eq!(b.rr, Some(2));
        // artic_pitch_vel_<bare rr>
        let c = parse_wav("susvib_A2_v3_1.wav").unwrap();
        assert_eq!(c.midi, 45);
        assert_eq!(c.vel.as_deref(), Some("v3"));
        assert_eq!(c.rr, Some(1));
        // inst_pitch_dyn
        let d = parse_wav("KSHarp_A2_mf.wav").unwrap();
        assert_eq!(d.vel.as_deref(), Some("mf"));
    }

    #[test]
    fn velocity_bands_split_evenly() {
        let bands = vel_bands_for(&["v2".to_string(), "v1".to_string()]);
        assert_eq!(bands.get("v1"), Some(&(0, 63)));
        assert_eq!(bands.get("v2"), Some(&(64, 127)));
        // A single layer covers the whole range (no holes).
        let one = vel_bands_for(&["v1".to_string()]);
        assert_eq!(one.get("v1"), Some(&(0, 127)));
    }

    #[test]
    fn key_ranges_partition_without_gaps() {
        let p = |midi: u8| ParsedWav { file: String::new(), midi, vel: None, rr: None };
        let r = key_ranges(&[p(60), p(64), p(67)]);
        assert_eq!(r.get(&60), Some(&(0, 62)));
        assert_eq!(r.get(&64), Some(&(63, 65)));
        assert_eq!(r.get(&67), Some(&(66, 127)));
    }

    #[test]
    fn family_and_art_names() {
        assert_eq!(family_token("Strings"), "strings");
        assert_eq!(family_token("Woodwinds"), "ww");
        assert_eq!(sanitize("Violin Section"), "violin_section");
        assert_eq!(art_name("susVib"), "sustain");
        assert_eq!(art_name("Arco Vib"), "sustain");
        assert_eq!(art_name("Pizz"), "pizzicato");
        assert_eq!(art_name("Trem"), "tremolo");
    }
}
