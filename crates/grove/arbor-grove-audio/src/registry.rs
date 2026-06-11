//! The sound **registry**: a TOML manifest mapping symbolic names to concrete
//! voices, plus the resident state (SFZ instruments + their [`SampleBank`]) those
//! names resolve against.
//!
//! ## Naming
//!
//! * **short names** (`bd`, `sd`, `hh`) → entries in the default drum/sound bank,
//!   each a one-shot sample (or a synth preset).
//! * **dotted names** (`strings.violin`, `synth.pad`) → `bank.instrument`: an SFZ
//!   instrument in a named bank, or a built-in `synth.*` preset.
//!
//! Synth presets and SFZ instruments coexist in one namespace. A name that
//! doesn't resolve (or whose SFZ samples aren't resident yet) **falls back to the
//! default synth** — grove always makes *a* sound.
//!
//! ## Manifest schema (TOML)
//!
//! ```toml
//! # A synth preset: built-in oscillator + ADSR.
//! [synth.bass]
//! kind = "synth"
//! waveform = "saw"          # saw | square | sine | triangle
//! attack = 0.005
//! decay = 0.12
//! sustain = 0.7
//! release = 0.2
//!
//! # A short drum name → a one-shot sample file.
//! [bd]
//! kind = "sample"
//! file = "drums/bd.wav"
//!
//! # An SFZ instrument (a whole multisampled instrument).
//! [strings.violin]
//! kind = "sfz"
//! file = "vsco2/strings/violin.sfz"
//! ```
//!
//! `kind` is one of `synth` | `sample` | `sfz`. Paths are resolved against the
//! manifest's base directory by the loader (the shell hands the audio crate
//! absolute paths). Anything not in the manifest → the default synth.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::{AudioError, Result};
use crate::sampler::{Sample, SampleBank};
use crate::sfz::{self, SfzInstrument};
use crate::synth::Waveform;

/// A synth preset: oscillator shape + ADSR (seconds / `0..1` sustain).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SynthPreset {
    pub waveform: Waveform,
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
}

impl Default for SynthPreset {
    /// The universal fallback: a moderately bright saw with a short pluck-ish
    /// envelope — grove's "electronic default" when no VSCO is installed.
    fn default() -> Self {
        SynthPreset {
            waveform: Waveform::Saw,
            attack: 0.005,
            decay: 0.15,
            sustain: 0.6,
            release: 0.2,
        }
    }
}

/// One manifest entry, post-load: a concrete thing a name resolves to.
#[derive(Clone, Debug)]
enum Entry {
    /// A built-in synth preset.
    Synth(SynthPreset),
    /// A one-shot sample, resident under `bank_key`.
    Sample { bank_key: String },
    /// An SFZ instrument (index into `instruments`).
    Sfz { instrument: usize },
}

/// What [`Registry::resolve`] produces: a concrete voice description the renderer
/// turns into runtime DSP state. RT-safe — built from resident data only.
///
/// Crate-internal: the renderer consumes it; the engine/shell never see it.
#[derive(Clone, Debug)]
pub(crate) enum ResolvedVoice {
    /// Play a synth preset at the requested note.
    Synth(SynthPreset),
    /// Play a resident sample one-shot.
    Sample {
        sample: Sample,
        /// SFZ-style playback params (offset/loop/tune carried for SFZ regions;
        /// a plain `sample` entry uses defaults).
        region: SampleParams,
    },
}

/// The subset of SFZ region params the renderer needs to drive a [`Sample`].
/// Pulled out of the SFZ [`Region`](crate::sfz::Region) so a plain (non-SFZ)
/// sample can share the same playback path with defaults. Crate-internal.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SampleParams {
    /// Key the sample plays at native pitch (drives note → pitch ratio).
    pub pitch_keycenter: u8,
    /// Fine + coarse tune folded into semitones.
    pub tune_semitones: f32,
    /// Region gain (linear, from `volume` dB).
    pub gain: f32,
    /// Region pan `-1..1`.
    pub pan: f32,
    /// Start offset in source frames.
    pub offset: u64,
    /// Loop spec, if the region loops.
    pub loop_spec: Option<crate::sampler::LoopSpec>,
    /// Amplitude envelope (seconds / `0..1`).
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
}

impl Default for SampleParams {
    fn default() -> Self {
        SampleParams {
            // Unpitched one-shot: native pitch regardless of note.
            pitch_keycenter: 60,
            tune_semitones: 0.0,
            gain: 1.0,
            pan: 0.0,
            offset: 0,
            loop_spec: None,
            attack: 0.0,
            decay: 0.0,
            sustain: 1.0,
            release: 0.02,
        }
    }
}

/// The resolved sound registry: name → entry, plus resident SFZ + samples.
#[derive(Debug)]
pub struct Registry {
    entries: HashMap<String, Entry>,
    instruments: Vec<SfzInstrument>,
    bank: SampleBank,
    fallback: SynthPreset,
}

impl Default for Registry {
    fn default() -> Self {
        Registry {
            entries: HashMap::new(),
            instruments: Vec::new(),
            bank: SampleBank::new(),
            fallback: SynthPreset::default(),
        }
    }
}

impl Registry {
    /// An empty registry whose every lookup falls back to the default synth.
    /// Enough to make sound with no manifest installed.
    pub fn new() -> Self {
        Registry::default()
    }

    /// Load a registry from a TOML manifest file. `base_dir` resolves relative
    /// `file =` paths; samples + SFZ instruments are decoded eagerly here
    /// (non-RT) so the callback only reads resident data.
    pub fn load_manifest(path: &Path) -> Result<Registry> {
        let text = std::fs::read_to_string(path).map_err(|e| AudioError::Io(e.to_string()))?;
        let base = path.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
        Registry::from_toml(&text, &base)
    }

    /// Register a synth preset under `name` (non-RT). Lets the engine/shell wire
    /// built-in voices without a manifest file.
    pub fn insert_synth(&mut self, name: impl Into<String>, preset: SynthPreset) {
        self.entries.insert(name.into(), Entry::Synth(preset));
    }

    /// Parse + load a manifest from already-read TOML text rooted at `base_dir`.
    ///
    /// Note: this hand-parses the small manifest schema (no `toml` crate
    /// dependency — the audio crate's dep set is frozen). The schema is flat
    /// `[name]` tables with `kind` + a couple of keys; see the module docs.
    pub fn from_toml(text: &str, base_dir: &Path) -> Result<Registry> {
        let tables = parse_toml_tables(text)?;
        let mut reg = Registry::new();
        for (name, kv) in tables {
            reg.add_entry(&name, &kv, base_dir)?;
        }
        Ok(reg)
    }

    /// Register one manifest entry, loading any referenced files (non-RT).
    fn add_entry(&mut self, name: &str, kv: &HashMap<String, String>, base: &Path) -> Result<()> {
        let kind = kv.get("kind").map(String::as_str).unwrap_or("synth");
        match kind {
            "synth" => {
                let preset = SynthPreset {
                    waveform: parse_waveform(kv.get("waveform").map(String::as_str)),
                    attack: parse_f32(kv, "attack", 0.005),
                    decay: parse_f32(kv, "decay", 0.15),
                    sustain: parse_f32(kv, "sustain", 0.6),
                    release: parse_f32(kv, "release", 0.2),
                };
                self.entries.insert(name.to_string(), Entry::Synth(preset));
            }
            "sample" => {
                let file = kv.get("file").ok_or_else(|| AudioError::Io(format!(
                    "registry entry `{name}` is kind=sample but has no `file`"
                )))?;
                let full = resolve_path(base, file);
                let sample = self.bank.load(&full)?;
                let key = full.to_string_lossy().into_owned();
                self.bank.insert(key.clone(), sample);
                self.entries
                    .insert(name.to_string(), Entry::Sample { bank_key: key });
            }
            "sfz" => {
                let file = kv.get("file").ok_or_else(|| AudioError::Io(format!(
                    "registry entry `{name}` is kind=sfz but has no `file`"
                )))?;
                let full = resolve_path(base, file);
                let idx = self.load_sfz(&full)?;
                self.entries
                    .insert(name.to_string(), Entry::Sfz { instrument: idx });
            }
            other => {
                return Err(AudioError::Io(format!(
                    "registry entry `{name}` has unknown kind `{other}`"
                )));
            }
        }
        Ok(())
    }

    /// Parse an SFZ file and decode every sample it references (non-RT). Returns
    /// the instrument index.
    fn load_sfz(&mut self, path: &Path) -> Result<usize> {
        let text = std::fs::read_to_string(path).map_err(|e| AudioError::Io(e.to_string()))?;
        let instrument = sfz::parse(&path.to_string_lossy(), &text)?;
        let sfz_dir = path.parent().unwrap_or_else(|| Path::new("."));
        for region in &instrument.regions {
            let sample_path = resolve_path(sfz_dir, &region.sample);
            // Resident under its absolute path key; a missing sample is left
            // unresident so the renderer falls back to the synth for it.
            if let Ok(sample) = self.bank.load(&sample_path) {
                let key = sample_path.to_string_lossy().into_owned();
                self.bank.insert(key, sample);
            }
        }
        self.instruments.push(instrument);
        Ok(self.instruments.len() - 1)
    }

    /// Resolve a named source to a concrete voice.
    ///
    /// `inst` takes priority over `sound` (a melodic instrument over a drum
    /// leaf); `variant` selects a sample bank variant (currently unused beyond
    /// the lookup key); `note`/`vel` choose the SFZ region. Anything unresolved
    /// → the fallback synth. **RT-safe**: reads resident state only.
    ///
    /// Crate-internal: called by the renderer per voice trigger.
    pub(crate) fn resolve(
        &self,
        sound: Option<&str>,
        inst: Option<&str>,
        note: Option<f32>,
        vel: f32,
    ) -> ResolvedVoice {
        // Prefer an explicit instrument, then a sound leaf.
        let name = inst.or(sound);
        let entry = name.and_then(|n| self.entries.get(n));

        match entry {
            Some(Entry::Synth(preset)) => ResolvedVoice::Synth(*preset),
            Some(Entry::Sample { bank_key }) => match self.bank.get(bank_key) {
                Some(sample) => ResolvedVoice::Sample {
                    sample,
                    region: SampleParams::default(),
                },
                None => ResolvedVoice::Synth(self.fallback),
            },
            Some(Entry::Sfz { instrument }) => {
                self.resolve_sfz(*instrument, note, vel).unwrap_or(ResolvedVoice::Synth(self.fallback))
            }
            None => ResolvedVoice::Synth(self.fallback),
        }
    }

    /// Pick the SFZ region for `(note, vel)` and bind its resident sample.
    fn resolve_sfz(&self, idx: usize, note: Option<f32>, vel: f32) -> Option<ResolvedVoice> {
        let instrument = self.instruments.get(idx)?;
        let key = note.unwrap_or(60.0).round().clamp(0.0, 127.0) as u8;
        let velo = (vel.clamp(0.0, 1.0) * 127.0).round() as u8;
        let region = instrument.select(key, velo)?;

        // Resolve the region's resident sample (re-derive its key the same way
        // `load_sfz` stored it: absolute path string). We can't know the sfz dir
        // here, so the sample was stored under its absolute path; the region only
        // holds the relative path. We therefore look it up by suffix match.
        let sample = self.find_region_sample(&region.sample)?;

        Some(ResolvedVoice::Sample {
            sample,
            region: SampleParams {
                pitch_keycenter: region.pitch_keycenter,
                tune_semitones: region.transpose as f32 + region.tune / 100.0,
                gain: db_to_linear(region.volume_db),
                pan: (region.pan / 100.0).clamp(-1.0, 1.0),
                offset: region.offset,
                loop_spec: region_loop_spec(region),
                attack: region.ampeg_attack,
                decay: region.ampeg_decay,
                sustain: region.ampeg_sustain,
                release: region.ampeg_release,
            },
        })
    }

    /// Find a resident sample for an SFZ region's (relative) sample path by
    /// matching the resident absolute-path keys on suffix.
    fn find_region_sample(&self, relative: &str) -> Option<Sample> {
        let needle = relative.replace('\\', "/");
        // Fast path: exact key.
        if let Some(s) = self.bank.get(&needle) {
            return Some(s);
        }
        // Suffix match against resident keys.
        let matched: Option<String> = self
            .bank
            .keys()
            .find(|k| k.replace('\\', "/").ends_with(&needle))
            .map(|k| k.to_string());
        matched.and_then(|k| self.bank.get(&k))
    }

    /// Direct access to the bank for tests / the renderer's offline path.
    pub(crate) fn bank(&self) -> &SampleBank {
        &self.bank
    }

    /// The fallback synth preset (used by the renderer for unresolved File
    /// sources too).
    pub(crate) fn fallback(&self) -> SynthPreset {
        self.fallback
    }
}

/// Convert dB to a linear gain multiplier.
fn db_to_linear(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

/// Build a [`LoopSpec`](crate::sampler::LoopSpec) from a region, if it loops.
fn region_loop_spec(region: &crate::sfz::Region) -> Option<crate::sampler::LoopSpec> {
    use crate::sfz::LoopMode;
    match region.loop_mode {
        LoopMode::LoopContinuous | LoopMode::LoopSustain => Some(crate::sampler::LoopSpec {
            mode: region.loop_mode,
            start: region.loop_start.unwrap_or(0),
            end: region.loop_end.unwrap_or(0),
        }),
        LoopMode::NoLoop | LoopMode::OneShot => None,
    }
}

/// Resolve a possibly-relative `file` against the manifest base directory.
fn resolve_path(base: &Path, file: &str) -> PathBuf {
    let p = Path::new(file);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        base.join(p)
    }
}

fn parse_f32(kv: &HashMap<String, String>, key: &str, default: f32) -> f32 {
    kv.get(key).and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn parse_waveform(s: Option<&str>) -> Waveform {
    match s {
        Some("square") => Waveform::Square,
        Some("sine") => Waveform::Sine,
        Some("triangle") => Waveform::Triangle,
        _ => Waveform::Saw,
    }
}

/// Parse the flat manifest TOML into `name → { key → value }` string tables.
///
/// Deliberately minimal: handles `[name]` / `[a.b]` table headers, `key = value`
/// pairs with string / number / bare values, `"quoted"` strings, `#` comments,
/// and blank lines. The manifest schema is small and flat by design, so this
/// avoids adding the `toml` crate to the frozen dep set.
fn parse_toml_tables(text: &str) -> Result<Vec<(String, HashMap<String, String>)>> {
    let mut tables: Vec<(String, HashMap<String, String>)> = Vec::new();
    let mut current: Option<(String, HashMap<String, String>)> = None;

    for (lineno, raw) in text.lines().enumerate() {
        let line = strip_toml_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if let Some(inner) = line.strip_prefix('[') {
            let name = inner.strip_suffix(']').ok_or_else(|| {
                AudioError::Io(format!("manifest line {}: unterminated table header", lineno + 1))
            })?;
            if let Some(t) = current.take() {
                tables.push(t);
            }
            current = Some((name.trim().to_string(), HashMap::new()));
            continue;
        }
        let eq = line.find('=').ok_or_else(|| {
            AudioError::Io(format!("manifest line {}: expected `key = value`", lineno + 1))
        })?;
        let key = line[..eq].trim().to_string();
        let value = unquote(line[eq + 1..].trim());
        match current.as_mut() {
            Some((_, kv)) => {
                kv.insert(key, value);
            }
            None => {
                return Err(AudioError::Io(format!(
                    "manifest line {}: key outside any [table]",
                    lineno + 1
                )));
            }
        }
    }
    if let Some(t) = current.take() {
        tables.push(t);
    }
    Ok(tables)
}

fn strip_toml_comment(line: &str) -> &str {
    // A naive `#` comment strip; fine for this flat manifest (no `#` in values
    // expected — paths are quoted or bare without `#`).
    match line.find('#') {
        Some(i) => &line[..i],
        None => line,
    }
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"') && s.len() >= 2)
        || (s.starts_with('\'') && s.ends_with('\'') && s.len() >= 2)
    {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_flat_manifest() {
        let src = "\
[synth.bass]
kind = \"synth\"
waveform = \"square\"
attack = 0.01

[bd]
kind = \"sample\"
file = \"drums/bd.wav\"
";
        let tables = parse_toml_tables(src).unwrap();
        assert_eq!(tables.len(), 2);
        assert_eq!(tables[0].0, "synth.bass");
        assert_eq!(tables[0].1.get("waveform").unwrap(), "square");
        assert_eq!(tables[1].0, "bd");
        assert_eq!(tables[1].1.get("file").unwrap(), "drums/bd.wav");
    }

    #[test]
    fn unresolved_name_falls_back_to_synth() {
        let reg = Registry::new();
        match reg.resolve(Some("nope"), None, Some(60.0), 0.8) {
            ResolvedVoice::Synth(_) => {}
            _ => panic!("expected synth fallback"),
        }
    }

    #[test]
    fn synth_preset_resolves() {
        let mut reg = Registry::new();
        reg.insert_synth(
            "synth.pad",
            SynthPreset {
                waveform: Waveform::Triangle,
                ..SynthPreset::default()
            },
        );
        match reg.resolve(None, Some("synth.pad"), Some(60.0), 0.8) {
            ResolvedVoice::Synth(p) => assert_eq!(p.waveform, Waveform::Triangle),
            _ => panic!("expected synth"),
        }
    }
}
