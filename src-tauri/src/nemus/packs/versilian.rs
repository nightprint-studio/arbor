//! Versilian wav-tree indexer: turn a raw `.wav` sample tree (VSCO 2 CE **or**
//! VCSL — both by Versilian Studios, same on-disk format) into a playable nemus
//! registry at index time.
//!
//! Neither archive ships `.sfz`, only `.wav` one-shots named by pitch (and
//! velocity / round-robin). The trees differ in depth and taxonomy:
//! - **VSCO 2**: `<Family>/<Instrument>/[<Articulation>/]<…>_<Pitch>_<vN>[_rrN].wav`
//! - **VCSL**: deeper Hornbostel-Sachs nesting
//!   (`<Category>/<Sub-category>/<Instrument>/[<Articulation>/]…`), and it mixes
//!   **pitched** instruments (mallets, winds, strings) with **unpitched** one-shots
//!   (anvil, claps, woodblocks, …).
//!
//! So this indexer walks the tree to whatever depth, groups the wav folders by
//! instrument, and for each:
//! - **pitched** (filenames carry note names) → writes a `_nemus.sfz` mapping the
//!   samples across the keyboard (velocity layers + round-robin) and registers a
//!   `kind=sfz` entry — the sustain articulation under the bare
//!   `<family>.<instrument>` name, others as lazy `.<articulation>` voices.
//! - **unpitched** (no note names) → registers a `kind=sample` folder one-shot
//!   under a short `s("…")`-friendly name (the Dirt-Samples model).
//!
//! The SFZ uses exactly the opcodes the engine's loader understands (`crate`'s
//! `sfz.rs`): `sample`, `lokey/hikey/pitch_keycenter`, `lovel/hivel`,
//! `seq_length/seq_position`, `ampeg_*`. No loop points (the wavs carry none).

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

/// Generate the registry TOML body for an extracted Versilian wav tree rooted at
/// `root`, writing one `_nemus.sfz` into each pitched sample folder as a side
/// effect. Returns `(registry_toml, instrument_count)` — the same shape as
/// [`super::layout::generate`], so install / re-index treat it uniformly.
pub fn generate(root: &Path) -> (String, usize) {
    let mut out = String::from("# Auto-generated Versilian registry (nemus).\n\n");

    // Every folder that directly holds wavs, at any depth.
    let mut sample_dirs = Vec::new();
    collect_sample_dirs(root, &mut sample_dirs);
    sample_dirs.sort();

    // Group sample folders by instrument: the rel path with any trailing
    // **articulation** folder stripped. So `Strings/Violin Section/susVib` and
    // `Aerophones/Reed Aerophones/Oboe/Sus` each key on their instrument folder,
    // gathering all their articulation sets under one entry.
    let mut groups: BTreeMap<Vec<String>, Vec<(String, PathBuf)>> = BTreeMap::new();
    for dir in sample_dirs {
        let Ok(rel) = dir.strip_prefix(root) else { continue };
        let comps: Vec<String> = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect();
        if comps.len() < 2 {
            continue; // need at least <category>/<instrument>
        }
        let leaf = comps.last().unwrap();
        let (inst_path, art) = if comps.len() >= 3 && is_articulation(leaf) {
            (comps[..comps.len() - 1].to_vec(), art_name(leaf))
        } else {
            (comps.clone(), "sustain".to_string())
        };
        groups.entry(inst_path).or_default().push((art, dir));
    }

    let mut count = 0usize;
    let mut seen: HashSet<String> = HashSet::new();
    for (inst_path, mut sets) in groups {
        sets.sort_by(|a, b| a.0.cmp(&b.0));
        let family = family_token(&inst_path[0]);
        let inst = sanitize(inst_path.last().unwrap());
        if inst.is_empty() {
            continue;
        }
        // Pitched if its samples carry note names; else an unpitched one-shot.
        let pitched = sets.iter().any(|(_, d)| dir_is_pitched(d));
        if pitched {
            emit_pitched(root, &family, &inst, &sets, &mut out, &mut count, &mut seen);
        } else {
            emit_unpitched(root, &inst, &sets, &mut out, &mut count, &mut seen);
        }
    }

    (out, count)
}

/// Emit a pitched instrument: a `_nemus.sfz` + `kind=sfz` entry per articulation.
/// Separate entries (not `art.*.region`) so each voice loads lazily — only the
/// referenced articulation decodes its samples. Sustain keeps the bare
/// `<family>.<instrument>` name; the rest get a `.<articulation>` suffix.
fn emit_pitched(
    root: &Path,
    family: &str,
    inst: &str,
    sets: &[(String, PathBuf)],
    out: &mut String,
    count: &mut usize,
    seen: &mut HashSet<String>,
) {
    let mut arts: Vec<(String, String)> = Vec::new();
    for (art, dir) in sets {
        if let Some(rel) = write_set_sfz(root, dir) {
            arts.push((art.clone(), rel));
        }
    }
    if arts.is_empty() {
        return;
    }
    let default_idx = arts.iter().position(|(a, _)| a == "sustain").unwrap_or(0);
    let base = format!("{family}.{inst}");
    if seen.insert(base.clone()) {
        out.push_str(&format!("[\"{base}\"]\nkind = \"sfz\"\nfile = \"{}\"\n\n", arts[default_idx].1));
        *count += 1;
    }
    for (i, (art, rel)) in arts.iter().enumerate() {
        if i == default_idx {
            continue;
        }
        let name = format!("{base}.{art}");
        if seen.insert(name.clone()) {
            out.push_str(&format!("[\"{name}\"]\nkind = \"sfz\"\nfile = \"{rel}\"\n\n"));
            *count += 1;
        }
    }
}

/// Emit an unpitched instrument: a `kind=sample` folder one-shot under a short,
/// `s("…")`-friendly name (Dirt-Samples model — `s("anvil")`, `s("clap")`). The
/// main set takes the instrument name; any extra subfolder gets an `_<art>` suffix.
fn emit_unpitched(
    root: &Path,
    inst: &str,
    sets: &[(String, PathBuf)],
    out: &mut String,
    count: &mut usize,
    seen: &mut HashSet<String>,
) {
    for (art, dir) in sets {
        let Ok(rel) = dir.strip_prefix(root) else { continue };
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        let name = if art == "sustain" || sets.len() == 1 {
            inst.to_string()
        } else {
            format!("{inst}_{art}")
        };
        if name.is_empty() || !seen.insert(name.clone()) {
            continue;
        }
        out.push_str(&format!("[\"{name}\"]\nkind = \"sample\"\ndir = \"{rel_str}\"\n\n"));
        *count += 1;
    }
}

// ── Pitched / unpitched detection ──────────────────────────────────────────────

/// Whether a folder's wavs are **pitch-named** (most carry a note token) — the
/// signal to map it as an SFZ instrument rather than an unpitched one-shot folder.
/// Samples up to a dozen wavs (enough to classify) so a huge folder stays cheap.
fn dir_is_pitched(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    let mut total = 0u32;
    let mut pitched = 0u32;
    for entry in entries.flatten() {
        let path = entry.path();
        if !is_wav(&path) {
            continue;
        }
        total += 1;
        if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
            if parse_wav(name).is_some() {
                pitched += 1;
            }
        }
        if total >= 12 {
            break;
        }
    }
    total > 0 && pitched * 2 >= total
}

// ── SFZ emission for one (pitched) sample folder ───────────────────────────────

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
/// Returns the SFZ path **relative to `root`** (forward slashes). `None` when the
/// folder yields no pitched sample.
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
    // (over the union of pitches, so coverage has no gaps).
    let key_ranges = key_ranges(&parsed);

    let sustained = !is_short_articulation(dir);
    let mut sfz = String::from("// Auto-generated by nemus from Versilian wavs.\n");
    sfz.push_str(&group_header(sustained));

    // Nest variants by pitch → velocity-layer → round-robin. Velocity banding is
    // **per pitch** so every pitch covers the whole 0..127 range even when it is
    // missing a layer some other pitch has — otherwise that note has a vel hole.
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
                sfz.push_str(&format!("lokey={lokey} hikey={hikey} pitch_keycenter={}\n", p.midi));
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
    let rel = sfz_path.strip_prefix(root).ok()?.to_string_lossy().replace('\\', "/");
    Some(rel)
}

/// The `<group>` header carrying the shared amp envelope. Sustained voices get a
/// soft attack + long release; short articulations a near-instant attack + short
/// release.
fn group_header(sustained: bool) -> String {
    if sustained {
        "<group>\nloop_mode=no_loop ampeg_attack=0.02 ampeg_release=0.3\n\n".to_string()
    } else {
        "<group>\nloop_mode=no_loop ampeg_attack=0.001 ampeg_release=0.15\n\n".to_string()
    }
}

// ── Filename parsing ───────────────────────────────────────────────────────────

/// Parse a wav filename into (pitch, velocity-layer, round-robin). The pitch is
/// the single `_`/space/`-`-separated token that reads as a note name (`A2`,
/// `Db4`); velocity a `v<n>` / dynamic token; round-robin an `rr<n>` token or a
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
    // A bare trailing number after the pitch (the `susvib_A2_v1_1.wav` case) is a
    // round-robin counter.
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
/// `c4 = 60` convention. Requires a trailing octave digit, so non-pitch tokens
/// (`v1`, `Vib`, `Ens`) never match.
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
    let oct: i32 = tok.get(idx..)?.parse().ok()?;
    let midi = (oct + 1) * 12 + semi;
    u8::try_from(midi).ok()
}

/// A velocity-layer key from a token: `v1`/`v2`… or a dynamic marking (`pp`..`ff`).
fn vel_key(tok: &str) -> Option<String> {
    let lower = tok.to_ascii_lowercase();
    if lower.len() >= 2 && lower.starts_with('v') && lower[1..].chars().all(|c| c.is_ascii_digit()) {
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

/// Assign each sampled pitch a `[lokey, hikey]` range by splitting at the midpoints
/// between adjacent sampled pitches; the lowest/highest extend to the keyboard ends
/// so there are no gaps.
fn key_ranges(parsed: &[ParsedWav]) -> BTreeMap<u8, (u8, u8)> {
    let mut pitches: Vec<u8> = parsed.iter().map(|p| p.midi).collect();
    pitches.sort_unstable();
    pitches.dedup();
    let mut ranges = BTreeMap::new();
    for (i, &p) in pitches.iter().enumerate() {
        let lo = if i == 0 { 0 } else { (pitches[i - 1] + p) / 2 + 1 };
        let hi = if i + 1 == pitches.len() { 127 } else { (p + pitches[i + 1]) / 2 };
        ranges.insert(p, (lo, hi));
    }
    ranges
}

// ── Naming ─────────────────────────────────────────────────────────────────────

/// The dotted-name family token for a top-level folder. VSCO 2 families and VCSL
/// Hornbostel-Sachs categories both map to friendly short families; anything else
/// is sanitised verbatim.
fn family_token(name: &str) -> String {
    let s = sanitize(name);
    match s.as_str() {
        // VSCO 2 families
        "strings" => "strings",
        "brass" => "brass",
        "woodwinds" | "winds" => "ww",
        "keys" | "keyboards" | "keyboard" => "keys",
        "percussion" => "perc",
        // VCSL Hornbostel-Sachs categories
        "aerophones" => "winds",
        "chordophones" => "strings",
        "idiophones" => "mallets",
        "membranophones" => "drums",
        "electrophones" => "electro",
        _ => return s,
    }
    .to_string()
}

/// Whether a folder name is an **articulation** (a technique under an instrument)
/// rather than an instrument itself. Matched on the exact sanitised token so an
/// instrument that merely *contains* such a substring (`vibraphone` ⊃ `vib`,
/// `vibraslap`) is never mistaken for one.
fn is_articulation(name: &str) -> bool {
    matches!(
        sanitize(name).as_str(),
        "sus" | "sustain" | "susvib" | "susnv" | "sus_vib" | "sus_nv" | "suslong"
            | "vib" | "vibrato" | "novib" | "nonvib" | "nv" | "expvib"
            | "arco" | "arcovib" | "arco_vib"
            | "stac" | "stacc" | "staccato" | "spic" | "spiccato"
            | "pizz" | "pizzt" | "trem" | "tremolo"
            | "legato" | "leg" | "long" | "short"
            | "mute" | "muted" | "open" | "buzz" | "fall"
            | "harmonm_sus" | "straightm_sus"
    )
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

/// Whether a sample folder holds a *short* articulation (pizz/spiccato/staccato) —
/// those get a snappy envelope instead of a sustained one.
fn is_short_articulation(dir: &Path) -> bool {
    let s = sanitize(&file_name(dir));
    s.contains("pizz") || s.contains("spic") || s.contains("stac")
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

/// Recursively collect every directory that directly contains ≥1 `.wav`.
fn collect_sample_dirs(dir: &Path, out: &mut Vec<PathBuf>) {
    if dir_has_wavs(dir) {
        out.push(dir.to_path_buf());
    }
    for sub in child_dirs(dir) {
        collect_sample_dirs(&sub, out);
    }
}

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
        assert_eq!(note_to_midi("v1"), None);
        assert_eq!(note_to_midi("Vib"), None);
        assert_eq!(note_to_midi("rr2"), None);
    }

    #[test]
    fn filename_shapes_parse() {
        let a = parse_wav("VlnEns_susVib_A2_v1.wav").unwrap();
        assert_eq!(a.midi, 45);
        assert_eq!(a.vel.as_deref(), Some("v1"));
        let b = parse_wav("VlnEns_Pizz_A2_v1_rr2.wav").unwrap();
        assert_eq!(b.rr, Some(2));
        let c = parse_wav("susvib_A2_v3_1.wav").unwrap();
        assert_eq!(c.rr, Some(1));
        // Unpitched one-shot (no note token) → not parseable.
        assert!(parse_wav("Anvil_Hit_01.wav").is_none());
    }

    #[test]
    fn articulation_vs_instrument() {
        // Real articulation folders.
        assert!(is_articulation("susVib"));
        assert!(is_articulation("Pizz"));
        assert!(is_articulation("Trem"));
        assert!(is_articulation("Sus"));
        // Instruments that merely contain an articulation substring must NOT match.
        assert!(!is_articulation("Vibraphone"));
        assert!(!is_articulation("Vibraslap"));
        assert!(!is_articulation("Anvil"));
        assert!(!is_articulation("Marimba"));
    }

    #[test]
    fn family_and_art_names() {
        // VSCO families + VCSL categories both map to friendly names.
        assert_eq!(family_token("Strings"), "strings");
        assert_eq!(family_token("Woodwinds"), "ww");
        assert_eq!(family_token("Aerophones"), "winds");
        assert_eq!(family_token("Idiophones"), "mallets");
        assert_eq!(family_token("Membranophones"), "drums");
        assert_eq!(sanitize("Violin Section"), "violin_section");
        assert_eq!(art_name("susVib"), "sustain");
        assert_eq!(art_name("Pizz"), "pizzicato");
    }

    #[test]
    fn velocity_bands_split_evenly() {
        let bands = vel_bands_for(&["v2".to_string(), "v1".to_string()]);
        assert_eq!(bands.get("v1"), Some(&(0, 63)));
        assert_eq!(bands.get("v2"), Some(&(64, 127)));
    }
}
