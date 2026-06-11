//! A hand-written parser for the **VSCO 2 Community Edition SFZ subset**.
//!
//! SFZ is a flat, opcode-based sampler format: `<region>` / `<group>` headers
//! introduce blocks, and `opcode=value` lines set parameters. Group opcodes are
//! inherited by the regions that follow until the next group. We support only
//! the opcodes VSCO 2 CE actually uses (listed on [`Region`]); unknown opcodes
//! are ignored so a richer `.sfz` still loads its supported parts.
//!
//! This parser is pure text → [`SfzInstrument`]; it does **no** file IO and
//! decodes nothing — sample loading is `crate::sampler`'s job. Region *selection*
//! (by key + velocity) also lives there; here we only carry the ranges.

use crate::error::{AudioError, Result};

/// One SFZ region: a sample mapped to a key range + velocity range, with the
/// VSCO 2 CE playback opcodes.
#[derive(Clone, Debug, PartialEq)]
pub struct Region {
    /// `sample=` — path relative to the `.sfz` file's directory.
    pub sample: String,
    /// `lokey` / `hikey` — inclusive MIDI key range this region answers to.
    pub lokey: u8,
    pub hikey: u8,
    /// `pitch_keycenter` — the key at which the sample plays at native pitch.
    pub pitch_keycenter: u8,
    /// `lovel` / `hivel` — inclusive velocity range (`0..127`).
    pub lovel: u8,
    pub hivel: u8,
    /// `loop_mode` — `no_loop` / `one_shot` / `loop_continuous` / `loop_sustain`.
    pub loop_mode: LoopMode,
    /// `loop_start` / `loop_end` — sample-frame loop points (when looping).
    pub loop_start: Option<u64>,
    pub loop_end: Option<u64>,
    /// `offset` — start playback this many sample frames in.
    pub offset: u64,
    /// `tune` — fine tune in cents (`-100..100`).
    pub tune: f32,
    /// `transpose` — coarse tune in semitones.
    pub transpose: i32,
    /// `volume` — region gain in dB.
    pub volume_db: f32,
    /// `pan` — region pan `-100..100` (left..right).
    pub pan: f32,
    /// `ampeg_*` — amplitude envelope (seconds, sustain is `0..1`).
    pub ampeg_attack: f32,
    pub ampeg_decay: f32,
    pub ampeg_sustain: f32,
    pub ampeg_release: f32,
}

impl Default for Region {
    fn default() -> Self {
        Region {
            sample: String::new(),
            lokey: 0,
            hikey: 127,
            pitch_keycenter: 60,
            lovel: 0,
            hivel: 127,
            loop_mode: LoopMode::NoLoop,
            loop_start: None,
            loop_end: None,
            offset: 0,
            tune: 0.0,
            transpose: 0,
            volume_db: 0.0,
            pan: 0.0,
            ampeg_attack: 0.0,
            ampeg_decay: 0.0,
            ampeg_sustain: 1.0,
            ampeg_release: 0.05,
        }
    }
}

impl Region {
    /// Whether this region answers to `(key, vel)` (both inclusive ranges).
    pub fn matches(&self, key: u8, vel: u8) -> bool {
        key >= self.lokey && key <= self.hikey && vel >= self.lovel && vel <= self.hivel
    }
}

/// SFZ `loop_mode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LoopMode {
    /// `no_loop` — play to the sample end, ignore loop points.
    #[default]
    NoLoop,
    /// `one_shot` — play the whole sample regardless of note-off.
    OneShot,
    /// `loop_continuous` — loop start..end for the note's lifetime.
    LoopContinuous,
    /// `loop_sustain` — loop while held, then play out the tail on release.
    LoopSustain,
}

impl LoopMode {
    fn parse(s: &str) -> LoopMode {
        match s {
            "one_shot" => LoopMode::OneShot,
            "loop_continuous" => LoopMode::LoopContinuous,
            "loop_sustain" => LoopMode::LoopSustain,
            _ => LoopMode::NoLoop,
        }
    }
}

/// A parsed SFZ instrument: just its regions (groups are flattened into them).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SfzInstrument {
    /// All regions, in file order. Region selection picks among these.
    pub regions: Vec<Region>,
}

impl SfzInstrument {
    /// Select the best region for `(key, vel)`: the first region (file order)
    /// whose key + velocity ranges both contain the request. Returns `None` if
    /// nothing matches (caller falls back to the synth).
    pub fn select(&self, key: u8, vel: u8) -> Option<&Region> {
        self.regions.iter().find(|r| r.matches(key, vel))
    }
}

/// Which block a pending opcode applies to while parsing.
#[derive(Clone, Copy, PartialEq)]
enum Scope {
    /// Before any header, or under `<global>` — applies to everything.
    Global,
    /// Under `<group>` — applies to following regions until the next group.
    Group,
    /// Under `<region>` — applies to the current region only.
    Region,
}

/// Parse SFZ source text into an [`SfzInstrument`].
///
/// `path` is used only for error messages. Group opcodes set on a `<group>` (or
/// `<global>`) header are inherited by every following `<region>` until the next
/// group/global header; a region opcode overrides the inherited value.
pub fn parse(path: &str, text: &str) -> Result<SfzInstrument> {
    let mut instrument = SfzInstrument::default();
    // The accumulated group/global defaults applied to each new region.
    let mut group_defaults = Region::default();
    // The region currently being built (`Some` only inside a `<region>` block).
    let mut current: Option<Region> = None;
    let mut scope = Scope::Global;

    for (lineno, raw) in text.lines().enumerate() {
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }

        // A line can hold a header and/or several `opcode=value` tokens. SFZ
        // tokens are whitespace-separated, but values may contain spaces (file
        // paths) — so we split lazily on `key=` boundaries below.
        let mut rest = line;
        while !rest.is_empty() {
            rest = rest.trim_start();
            if rest.is_empty() {
                break;
            }
            if let Some(after) = rest.strip_prefix('<') {
                // A header token `<name>`.
                let end = after.find('>').ok_or_else(|| AudioError::Sfz {
                    path: path.to_string(),
                    reason: format!("line {}: unterminated header", lineno + 1),
                })?;
                let header = &after[..end];
                flush_region(&mut instrument, &mut current);
                match header {
                    "global" => {
                        scope = Scope::Global;
                        group_defaults = Region::default();
                    }
                    "group" => {
                        scope = Scope::Group;
                        group_defaults = Region::default();
                    }
                    "region" => {
                        scope = Scope::Region;
                        current = Some(group_defaults.clone());
                    }
                    // `<control>` / `<master>` / `<curve>` etc.: not in the VSCO
                    // subset — skip the header but keep parsing its opcodes into
                    // the global defaults so nothing crashes.
                    _ => scope = Scope::Global,
                }
                rest = &after[end + 1..];
                continue;
            }

            // An `opcode=value` token. The value runs until the next ` opcode=`
            // boundary (to allow spaces in sample paths).
            let eq = match rest.find('=') {
                Some(e) => e,
                // A stray token with no `=`: ignore the rest of the line.
                None => break,
            };
            let key = rest[..eq].trim();
            let value_region = &rest[eq + 1..];
            let (value, consumed) = take_value(value_region);
            apply_opcode(
                key,
                value,
                scope,
                &mut group_defaults,
                current.as_mut(),
            );
            rest = &value_region[consumed..];
        }
    }

    flush_region(&mut instrument, &mut current);
    if instrument.regions.is_empty() {
        return Err(AudioError::Sfz {
            path: path.to_string(),
            reason: "no <region> with a sample".to_string(),
        });
    }
    Ok(instrument)
}

/// Push the in-progress region (if any) into the instrument, requiring a sample.
fn flush_region(instrument: &mut SfzInstrument, current: &mut Option<Region>) {
    if let Some(region) = current.take() {
        if !region.sample.is_empty() {
            instrument.regions.push(region);
        }
    }
}

/// Drop a `//` line comment.
fn strip_comment(line: &str) -> &str {
    match line.find("//") {
        Some(i) => &line[..i],
        None => line,
    }
}

/// Read an opcode value starting at `s`: everything up to the next token that
/// looks like `opcode=` (i.e. a word followed by `=`). Returns the trimmed value
/// and how many bytes of `s` were consumed (including any trailing space).
fn take_value(s: &str) -> (&str, usize) {
    // Walk byte indices of whitespace; the value ends just before a whitespace
    // that is followed by `word=`.
    let bytes = s.as_bytes();
    let mut i = 0;
    // Skip a single leading space already handled by caller; here value starts
    // at 0. Find the end.
    let mut end = s.len();
    while i < bytes.len() {
        if bytes[i] == b' ' || bytes[i] == b'\t' {
            // Does a `word=` start after this whitespace run?
            let mut j = i;
            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                j += 1;
            }
            if looks_like_opcode_start(&s[j..]) {
                end = i;
                break;
            }
        }
        i += 1;
    }
    let value = s[..end].trim();
    // Consume through the trailing whitespace so the caller resumes at the next token.
    let mut consumed = end;
    while consumed < bytes.len() && (bytes[consumed] == b' ' || bytes[consumed] == b'\t') {
        consumed += 1;
    }
    (value, consumed)
}

/// Whether `s` begins with `<` (a header) or `ident=` (an opcode assignment).
fn looks_like_opcode_start(s: &str) -> bool {
    if s.starts_with('<') {
        return true;
    }
    let mut chars = s.char_indices();
    let mut saw_ident = false;
    for (i, c) in &mut chars {
        if c == '=' {
            return saw_ident && i > 0;
        }
        if c.is_alphanumeric() || c == '_' {
            saw_ident = true;
        } else {
            return false;
        }
    }
    false
}

/// Apply one opcode to the active scope's region (group defaults, or the current
/// region). Unknown opcodes are silently ignored.
fn apply_opcode(
    key: &str,
    value: &str,
    scope: Scope,
    group_defaults: &mut Region,
    current: Option<&mut Region>,
) {
    // The target region: the current `<region>` when inside one, else the
    // shared group/global defaults.
    let target: &mut Region = match (scope, current) {
        (Scope::Region, Some(r)) => r,
        _ => group_defaults,
    };
    set_field(target, key, value);
}

/// Set a single field on a [`Region`] from a raw opcode value. Parse failures
/// leave the field unchanged (lenient, like real SFZ players).
fn set_field(r: &mut Region, key: &str, value: &str) {
    match key {
        "sample" => r.sample = value.replace('\\', "/"),
        "lokey" => {
            if let Some(k) = parse_key(value) {
                r.lokey = k;
            }
        }
        "hikey" => {
            if let Some(k) = parse_key(value) {
                r.hikey = k;
            }
        }
        "key" => {
            // `key=` is shorthand for lokey=hikey=pitch_keycenter.
            if let Some(k) = parse_key(value) {
                r.lokey = k;
                r.hikey = k;
                r.pitch_keycenter = k;
            }
        }
        "pitch_keycenter" => {
            if let Some(k) = parse_key(value) {
                r.pitch_keycenter = k;
            }
        }
        "lovel" => {
            if let Ok(v) = value.parse() {
                r.lovel = v;
            }
        }
        "hivel" => {
            if let Ok(v) = value.parse() {
                r.hivel = v;
            }
        }
        "loop_mode" => r.loop_mode = LoopMode::parse(value),
        "loop_start" => {
            if let Ok(v) = value.parse() {
                r.loop_start = Some(v);
            }
        }
        "loop_end" => {
            if let Ok(v) = value.parse() {
                r.loop_end = Some(v);
            }
        }
        "offset" => {
            if let Ok(v) = value.parse() {
                r.offset = v;
            }
        }
        "tune" => {
            if let Ok(v) = value.parse() {
                r.tune = v;
            }
        }
        "transpose" => {
            if let Ok(v) = value.parse() {
                r.transpose = v;
            }
        }
        "volume" => {
            if let Ok(v) = value.parse() {
                r.volume_db = v;
            }
        }
        "pan" => {
            if let Ok(v) = value.parse() {
                r.pan = v;
            }
        }
        "ampeg_attack" => {
            if let Ok(v) = value.parse() {
                r.ampeg_attack = v;
            }
        }
        "ampeg_decay" => {
            if let Ok(v) = value.parse() {
                r.ampeg_decay = v;
            }
        }
        "ampeg_sustain" => {
            // SFZ sustain is a percentage `0..100`.
            if let Ok(v) = value.parse::<f32>() {
                r.ampeg_sustain = (v / 100.0).clamp(0.0, 1.0);
            }
        }
        "ampeg_release" => {
            if let Ok(v) = value.parse() {
                r.ampeg_release = v;
            }
        }
        _ => {}
    }
}

/// Parse a key as either a MIDI number (`60`) or a note name (`c4`, `f#3`).
fn parse_key(value: &str) -> Option<u8> {
    if let Ok(n) = value.parse::<i32>() {
        return u8::try_from(n).ok();
    }
    note_name_to_midi(value)
}

/// Parse an SFZ note name (`c4`, `c#4`, `db4`) to a MIDI number. SFZ uses
/// `c4 = 60` (the same convention as the seam).
fn note_name_to_midi(s: &str) -> Option<u8> {
    let s = s.trim().to_ascii_lowercase();
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let base = match bytes[0] {
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
    let mut semitone = base;
    if idx < bytes.len() {
        match bytes[idx] {
            b'#' => {
                semitone += 1;
                idx += 1;
            }
            b'b' => {
                semitone -= 1;
                idx += 1;
            }
            _ => {}
        }
    }
    let octave: i32 = s[idx..].parse().ok()?;
    // c4 = 60 → midi = (octave + 1) * 12 + semitone.
    let midi = (octave + 1) * 12 + semitone;
    u8::try_from(midi).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_region() {
        let src = "<region> sample=kick.wav key=36";
        let inst = parse("k.sfz", src).unwrap();
        assert_eq!(inst.regions.len(), 1);
        let r = &inst.regions[0];
        assert_eq!(r.sample, "kick.wav");
        assert_eq!(r.lokey, 36);
        assert_eq!(r.hikey, 36);
        assert_eq!(r.pitch_keycenter, 36);
    }

    #[test]
    fn group_defaults_inherited_by_regions() {
        let src = "\
<group> volume=-6 ampeg_release=0.3
<region> sample=a.wav lokey=60 hikey=63 pitch_keycenter=60
<region> sample=b.wav lokey=64 hikey=67 pitch_keycenter=64 volume=-3";
        let inst = parse("i.sfz", src).unwrap();
        assert_eq!(inst.regions.len(), 2);
        // First region inherits the group volume + release.
        assert_eq!(inst.regions[0].volume_db, -6.0);
        assert_eq!(inst.regions[0].ampeg_release, 0.3);
        // Second region overrides volume, keeps inherited release.
        assert_eq!(inst.regions[1].volume_db, -3.0);
        assert_eq!(inst.regions[1].ampeg_release, 0.3);
    }

    #[test]
    fn velocity_layers_select_correctly() {
        let src = "\
<region> sample=soft.wav lokey=60 hikey=60 pitch_keycenter=60 lovel=0 hivel=63
<region> sample=loud.wav lokey=60 hikey=60 pitch_keycenter=60 lovel=64 hivel=127";
        let inst = parse("v.sfz", src).unwrap();
        assert_eq!(inst.select(60, 30).unwrap().sample, "soft.wav");
        assert_eq!(inst.select(60, 100).unwrap().sample, "loud.wav");
        assert!(inst.select(72, 100).is_none());
    }

    #[test]
    fn note_names_and_accidentals() {
        assert_eq!(note_name_to_midi("c4"), Some(60));
        assert_eq!(note_name_to_midi("a4"), Some(69));
        assert_eq!(note_name_to_midi("c#4"), Some(61));
        assert_eq!(note_name_to_midi("db4"), Some(61));
        assert_eq!(note_name_to_midi("c-1"), Some(0));
    }

    #[test]
    fn sample_path_with_spaces_and_comment() {
        let src = "<region> sample=Sub Folder/my kick.wav key=36 // a comment";
        let inst = parse("s.sfz", src).unwrap();
        assert_eq!(inst.regions[0].sample, "Sub Folder/my kick.wav");
    }

    #[test]
    fn sustain_percent_to_unit() {
        let src = "<region> sample=a.wav key=60 ampeg_sustain=50";
        let inst = parse("p.sfz", src).unwrap();
        assert_eq!(inst.regions[0].ampeg_sustain, 0.5);
    }

    #[test]
    fn error_when_no_regions() {
        assert!(parse("empty.sfz", "// nothing here").is_err());
    }
}
