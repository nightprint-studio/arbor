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
//! default synth** — nemus always makes *a* sound.
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
//! # A folder of variant samples (`s("hh:2")` selects one; round-robin otherwise).
//! [hh]
//! kind = "sample"
//! dir = "drums/hh"
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

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::error::{AudioError, Result};
use crate::sampler::{Sample, SampleBank};
use crate::sfz::{self, SfzInstrument};
use crate::synth::{NoiseColor, SynthShape, Waveform};

/// A synth preset: a sound [`SynthShape`] + ADSR (seconds / `0..1` sustain).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SynthPreset {
    pub shape: SynthShape,
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
}

impl Default for SynthPreset {
    /// The universal fallback when no name resolves. A **triangle** (alias-free,
    /// soft) with a short pluck-ish envelope — a gentle "electronic default" that
    /// stays clean across the whole register even with no VSCO installed.
    fn default() -> Self {
        SynthPreset {
            shape: SynthShape::Wave(Waveform::Triangle),
            attack: 0.005,
            decay: 0.15,
            sustain: 0.6,
            release: 0.2,
        }
    }
}

/// Build a preset with an explicit ADSR (keeps the tables below terse).
const fn preset(shape: SynthShape, attack: f32, decay: f32, sustain: f32, release: f32) -> SynthPreset {
    SynthPreset { shape, attack, decay, sustain, release }
}

/// A neutral *gate* envelope for the bare shape instruments: near-instant
/// attack, full sustain, short release — the note rings for its whole duration
/// then releases cleanly. This mirrors how a plain gated source behaves in
/// Strudel (`s("sawtooth")` holds the note rather than plucking it), so the same
/// patches sound the way someone coming from Strudel expects.
const fn gated(shape: SynthShape) -> SynthPreset {
    preset(shape, 0.005, 0.0, 1.0, 0.06)
}

/// The named `synth.*` presets — distinct, ready-to-play voices across the whole
/// palette (oscillators, supersaw, noise). All band-limited (see [`crate::synth`]).
const PRESET_SYNTHS: [(&str, SynthPreset); 8] = [
    ("synth.bass",     preset(SynthShape::Wave(Waveform::Saw),      0.005, 0.12, 0.70, 0.18)),
    ("synth.sub",      preset(SynthShape::Wave(Waveform::Sine),     0.005, 0.10, 0.90, 0.20)),
    ("synth.pad",      preset(SynthShape::Wave(Waveform::Triangle), 0.08,  0.25, 0.85, 0.50)),
    ("synth.pluck",    preset(SynthShape::Wave(Waveform::Square),   0.002, 0.14, 0.00, 0.12)),
    ("synth.lead",     preset(SynthShape::Wave(Waveform::Saw),      0.01,  0.10, 0.75, 0.20)),
    ("synth.supersaw", preset(SynthShape::Supersaw,                 0.02,  0.10, 0.85, 0.30)),
    ("synth.noise",    preset(SynthShape::Noise(NoiseColor::White), 0.001, 0.08, 0.00, 0.06)),
    ("synth.hat",      preset(SynthShape::Noise(NoiseColor::Pink),  0.001, 0.04, 0.00, 0.04)),
];

/// The bare **oscillator** instruments: the raw waveform names usable directly
/// as an instrument (`s("sawtooth")`, `.inst("sine")`), matching Strudel's
/// oscillator vocabulary — the canonical names plus the short aliases. All map
/// onto nemus's four band-limited shapes; `pulse` aliases `square` (no PWM yet).
const OSCILLATOR_SYNTHS: [(&str, SynthPreset); 9] = [
    ("sine",     gated(SynthShape::Wave(Waveform::Sine))),
    ("sin",      gated(SynthShape::Wave(Waveform::Sine))),
    ("sawtooth", gated(SynthShape::Wave(Waveform::Saw))),
    ("saw",      gated(SynthShape::Wave(Waveform::Saw))),
    ("square",   gated(SynthShape::Wave(Waveform::Square))),
    ("sqr",      gated(SynthShape::Wave(Waveform::Square))),
    ("pulse",    gated(SynthShape::Wave(Waveform::Square))),
    ("triangle", gated(SynthShape::Wave(Waveform::Triangle))),
    ("tri",      gated(SynthShape::Wave(Waveform::Triangle))),
];

/// The bare names for the non-oscillator shapes — the detuned `supersaw` and the
/// noise colours — again matching Strudel (`s("supersaw")`, `s("white")`, …).
const SHAPE_SYNTHS: [(&str, SynthPreset); 5] = [
    ("supersaw", gated(SynthShape::Supersaw)),
    ("white",    gated(SynthShape::Noise(NoiseColor::White))),
    ("pink",     gated(SynthShape::Noise(NoiseColor::Pink))),
    ("brown",    gated(SynthShape::Noise(NoiseColor::Brown))),
    ("crackle",  gated(SynthShape::Noise(NoiseColor::Crackle))),
];

/// How a named articulation re-targets sample selection on an SFZ instrument.
///
/// * `Keyswitch` — the articulation activates a keyswitch (an SFZ `sw_last` key),
///   filtering the same instrument's regions to that articulation's set.
/// * `Region` — the articulation is a *separate* SFZ instrument (an alternate
///   sample set in its own subdir), selected wholesale.
#[derive(Clone, Debug)]
enum Articulation {
    /// Filter regions by this keyswitch key (MIDI).
    Keyswitch(u8),
    /// Use an alternate SFZ instrument (index into `instruments`).
    Region(usize),
}

/// One manifest entry, post-load: a concrete thing a name resolves to.
#[derive(Clone, Debug)]
enum Entry {
    /// A built-in synth preset.
    Synth(SynthPreset),
    /// A one-shot sample with one or more **variants** (resident bank keys). A
    /// single-file entry has one; a `dir =` entry collects every decodable file
    /// in a folder (Strudel's Dirt-Samples model: `s("bd:3")` selects a variant).
    Sample { variants: Vec<String> },
    /// An SFZ instrument (index into `instruments`) plus its named articulations.
    Sfz {
        instrument: usize,
        /// Articulation name → how it re-targets selection. Empty = no articulations.
        articulations: HashMap<String, Articulation>,
    },
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

/// The broad category of a resolvable instrument, for registry introspection
/// (the sound-bank UI). Mirrors the manifest's `kind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstrumentKind {
    Synth,
    Sample,
    Sfz,
}

/// One resolvable instrument: its registry name + how it's voiced. Introspection
/// only — carries no resident data, so it's cheap to clone across the IPC seam.
#[derive(Clone, Debug, PartialEq)]
pub struct InstrumentInfo {
    pub name: String,
    pub kind: InstrumentKind,
    /// Named articulations the instrument exposes (`.art("legato")`), sorted.
    /// Empty for synth / sample voices and SFZ instruments with no `art.*` decls.
    pub articulations: Vec<String>,
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
        let mut reg = Registry::new();
        reg.load_manifest_into(path)?;
        Ok(reg)
    }

    /// Merge a manifest file's entries into this registry (additive; a later
    /// entry overrides an earlier same-named one). Lets the shell stack several
    /// installed sample packs onto the built-in synths in a single registry —
    /// each pack's `registry.toml` is merged in turn. Decodes eagerly (non-RT).
    pub fn load_manifest_into(&mut self, path: &Path) -> Result<()> {
        let text = std::fs::read_to_string(path).map_err(|e| AudioError::Io(e.to_string()))?;
        let base = path.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
        self.add_manifest_text(&text, &base)
    }

    /// Like [`load_manifest_into`], but decode **only** the entries whose name is
    /// in `needed`; every other table is parsed then skipped (no sample decode).
    ///
    /// This is the lazy-loading path: an arrangement references a handful of
    /// instruments, so the live session loads just those instead of a whole pack
    /// — decoding all of VSCO/Dirt to play one drum would read gigabytes into RAM.
    /// Decodes eagerly (non-RT) for the selected entries only.
    pub fn load_manifest_subset_into(
        &mut self,
        path: &Path,
        needed: &HashSet<String>,
    ) -> Result<()> {
        let text = std::fs::read_to_string(path).map_err(|e| AudioError::Io(e.to_string()))?;
        let base = path.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
        for (name, kv) in parse_toml_tables(&text)? {
            if needed.contains(&name) {
                self.add_entry(&name, &kv, &base)?;
            }
        }
        Ok(())
    }

    /// Register a synth preset under `name` (non-RT). Lets the engine/shell wire
    /// built-in voices without a manifest file.
    pub fn insert_synth(&mut self, name: impl Into<String>, preset: SynthPreset) {
        self.entries.insert(name.into(), Entry::Synth(preset));
    }

    /// Install nemus's always-available built-in voices, with **no manifest and
    /// no VSCO**, so a patch that asks for one sounds as intended instead of
    /// falling back to the default voice:
    ///
    /// * the named `synth.*` presets — `synth.bass` / `synth.sub` / `synth.pad`
    ///   / `synth.pluck` / `synth.lead` / `synth.supersaw` / `synth.noise` /
    ///   `synth.hat` (the names the language and docs/examples reference);
    /// * the bare **shape** names matching Strudel's vocabulary — the oscillators
    ///   `sine` / `sawtooth` / `square` / `triangle` / `pulse` (+ aliases `saw` /
    ///   `tri` / `sqr` / `sin`), the detuned `supersaw`, and the noise colours
    ///   `white` / `pink` / `brown` / `crackle`.
    ///
    /// Existing same-named entries are overwritten (these namespaces are ours;
    /// VSCO ships orchestral instruments, not these).
    pub fn install_builtin_synths(&mut self) {
        let all = PRESET_SYNTHS
            .into_iter()
            .chain(OSCILLATOR_SYNTHS)
            .chain(SHAPE_SYNTHS);
        for (name, preset) in all {
            self.insert_synth(name, preset);
        }
    }

    /// Parse + load a manifest from already-read TOML text rooted at `base_dir`.
    ///
    /// Note: this hand-parses the small manifest schema (no `toml` crate
    /// dependency — the audio crate's dep set is frozen). The schema is flat
    /// `[name]` tables with `kind` + a couple of keys; see the module docs.
    pub fn from_toml(text: &str, base_dir: &Path) -> Result<Registry> {
        let mut reg = Registry::new();
        reg.add_manifest_text(text, base_dir)?;
        Ok(reg)
    }

    /// Parse manifest TOML text and merge its entries into this registry.
    fn add_manifest_text(&mut self, text: &str, base_dir: &Path) -> Result<()> {
        let tables = parse_toml_tables(text)?;
        for (name, kv) in tables {
            self.add_entry(&name, &kv, base_dir)?;
        }
        Ok(())
    }

    /// Register one manifest entry, loading any referenced files (non-RT).
    fn add_entry(&mut self, name: &str, kv: &HashMap<String, String>, base: &Path) -> Result<()> {
        let kind = kv.get("kind").map(String::as_str).unwrap_or("synth");
        match kind {
            "synth" => {
                let preset = SynthPreset {
                    shape: SynthShape::Wave(parse_waveform(kv.get("waveform").map(String::as_str))),
                    attack: parse_f32(kv, "attack", 0.005),
                    decay: parse_f32(kv, "decay", 0.15),
                    sustain: parse_f32(kv, "sustain", 0.6),
                    release: parse_f32(kv, "release", 0.2),
                };
                self.entries.insert(name.to_string(), Entry::Synth(preset));
            }
            "sample" => {
                // `dir =` collects a folder of variant samples; `file =` is the
                // single-sample case (one variant). Exactly one must be present.
                let variants = if let Some(dir) = kv.get("dir") {
                    self.load_sample_dir(&resolve_path(base, dir))?
                } else if let Some(file) = kv.get("file") {
                    vec![self.load_sample_file(&resolve_path(base, file))?]
                } else {
                    return Err(AudioError::Io(format!(
                        "registry entry `{name}` is kind=sample but has neither `file` nor `dir`"
                    )));
                };
                if variants.is_empty() {
                    return Err(AudioError::Io(format!(
                        "registry entry `{name}` (kind=sample) resolved to zero samples"
                    )));
                }
                self.entries
                    .insert(name.to_string(), Entry::Sample { variants });
            }
            "sfz" => {
                let file = kv.get("file").ok_or_else(|| AudioError::Io(format!(
                    "registry entry `{name}` is kind=sfz but has no `file`"
                )))?;
                let full = resolve_path(base, file);
                let idx = self.load_sfz(&full)?;
                let articulations = self.parse_articulations(kv, base)?;
                self.entries.insert(
                    name.to_string(),
                    Entry::Sfz {
                        instrument: idx,
                        articulations,
                    },
                );
            }
            other => {
                return Err(AudioError::Io(format!(
                    "registry entry `{name}` has unknown kind `{other}`"
                )));
            }
        }
        Ok(())
    }

    /// Decode one sample file into the bank, returning its (absolute-path) key.
    fn load_sample_file(&mut self, full: &Path) -> Result<String> {
        let sample = self.bank.load(full)?;
        let key = full.to_string_lossy().into_owned();
        self.bank.insert(key.clone(), sample);
        Ok(key)
    }

    /// Decode every decodable file directly under `dir` (sorted by name) as sample
    /// variants, returning their bank keys. A file that fails to decode is skipped
    /// rather than failing the whole pack — sample folders sometimes carry stray
    /// non-audio files. Sorting makes `:n` indices stable across installs.
    fn load_sample_dir(&mut self, dir: &Path) -> Result<Vec<String>> {
        let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
            .map_err(|e| AudioError::Io(format!("reading sample dir {}: {e}", dir.display())))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_file() && is_decodable(p))
            .collect();
        files.sort();
        let mut keys = Vec::with_capacity(files.len());
        for f in &files {
            if let Ok(key) = self.load_sample_file(f) {
                keys.push(key);
            }
        }
        Ok(keys)
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

    /// Parse the per-instrument articulation declarations out of an sfz entry's
    /// key/value map. Each articulation is keyed `art.<name>.keyswitch = <midi>`
    /// **or** `art.<name>.region = "<file.sfz>"` (an alternate sample set loaded as
    /// its own SFZ instrument). A name may declare only one of the two.
    fn parse_articulations(
        &mut self,
        kv: &HashMap<String, String>,
        base: &Path,
    ) -> Result<HashMap<String, Articulation>> {
        let mut arts: HashMap<String, Articulation> = HashMap::new();
        for (key, value) in kv {
            let Some(rest) = key.strip_prefix("art.") else {
                continue;
            };
            // `rest` is `<name>.<field>`.
            let Some((art_name, field)) = rest.rsplit_once('.') else {
                continue;
            };
            match field {
                "keyswitch" => {
                    if let Ok(midi) = value.parse::<u8>() {
                        arts.insert(art_name.to_string(), Articulation::Keyswitch(midi));
                    }
                }
                "region" => {
                    let full = resolve_path(base, value);
                    let idx = self.load_sfz(&full)?;
                    arts.insert(art_name.to_string(), Articulation::Region(idx));
                }
                _ => {}
            }
        }
        Ok(arts)
    }

    /// Resolve a named source to a concrete voice.
    ///
    /// `inst` takes priority over `sound` (a melodic instrument over a drum
    /// leaf); `variant` is the `:n` sample-variant index (`None` → round-robin by
    /// `seed`); `note`/`vel` choose the SFZ region; `art` selects an articulation
    /// (keyswitch or alternate region set); `seed` is the deterministic onset seed
    /// driving round-robin variant choice. Anything unresolved → the fallback
    /// synth. **RT-safe**: reads resident state only.
    ///
    /// Crate-internal: called by the renderer per voice trigger.
    pub(crate) fn resolve(
        &self,
        sound: Option<&str>,
        inst: Option<&str>,
        variant: Option<u32>,
        note: Option<f32>,
        vel: f32,
        art: Option<&str>,
        seed: u64,
    ) -> ResolvedVoice {
        // Prefer an explicit instrument, then a sound leaf.
        let name = inst.or(sound);
        let entry = name.and_then(|n| self.entries.get(n));

        match entry {
            Some(Entry::Synth(preset)) => ResolvedVoice::Synth(*preset),
            Some(Entry::Sample { variants }) => self
                .resolve_sample(variants, variant, seed)
                .unwrap_or(ResolvedVoice::Synth(self.fallback)),
            Some(Entry::Sfz {
                instrument,
                articulations,
            }) => self
                .resolve_sfz(*instrument, articulations, note, vel, art, seed)
                .unwrap_or(ResolvedVoice::Synth(self.fallback)),
            None => ResolvedVoice::Synth(self.fallback),
        }
    }

    /// Pick a sample variant and bind its resident sample. An explicit `:n`
    /// (`variant`) indexes the list (wrapped); without one, the onset `seed`
    /// drives a deterministic round-robin — the same onset picks the same variant
    /// every loop, mirroring the SFZ round-robin policy.
    fn resolve_sample(
        &self,
        variants: &[String],
        variant: Option<u32>,
        seed: u64,
    ) -> Option<ResolvedVoice> {
        if variants.is_empty() {
            return None;
        }
        let idx = match variant {
            Some(n) => n as usize % variants.len(),
            None => (seed as usize) % variants.len(),
        };
        let sample = self.bank.get(&variants[idx])?;
        Some(ResolvedVoice::Sample {
            sample,
            region: SampleParams::default(),
        })
    }

    /// Pick the SFZ region for `(note, vel)`, honouring the requested articulation
    /// + round-robin, and bind its resident sample.
    fn resolve_sfz(
        &self,
        idx: usize,
        articulations: &HashMap<String, Articulation>,
        note: Option<f32>,
        vel: f32,
        art: Option<&str>,
        seed: u64,
    ) -> Option<ResolvedVoice> {
        // An articulation may redirect to a separate SFZ instrument (a `region`
        // alternate sample set) or activate a keyswitch on this instrument.
        let (inst_idx, keyswitch) = match art.and_then(|a| articulations.get(a)) {
            Some(Articulation::Region(other)) => (*other, None),
            Some(Articulation::Keyswitch(sw)) => (idx, Some(*sw)),
            None => (idx, None),
        };
        let instrument = self.instruments.get(inst_idx)?;
        let key = note.unwrap_or(60.0).round().clamp(0.0, 127.0) as u8;
        let velo = (vel.clamp(0.0, 1.0) * 127.0).round() as u8;
        let region = instrument.select_rr(key, velo, seed, keyswitch)?;

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

    /// Enumerate every resolvable instrument (sorted by name). Introspection for
    /// the sound-bank UI; reflects exactly what this registry resolves.
    pub fn instruments_list(&self) -> Vec<InstrumentInfo> {
        let mut list: Vec<InstrumentInfo> = self
            .entries
            .iter()
            .map(|(name, entry)| {
                let kind = match entry {
                    Entry::Synth(_) => InstrumentKind::Synth,
                    Entry::Sample { .. } => InstrumentKind::Sample,
                    Entry::Sfz { .. } => InstrumentKind::Sfz,
                };
                let mut articulations: Vec<String> = match entry {
                    Entry::Sfz { articulations, .. } => articulations.keys().cloned().collect(),
                    _ => Vec::new(),
                };
                articulations.sort();
                InstrumentInfo { name: name.clone(), kind, articulations }
            })
            .collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }
}

/// Enumerate the instruments a manifest declares — name, kind, and (for SFZ)
/// articulation names — **without decoding any samples**.
///
/// The sound-bank UI only needs the listing, not the audio. Building a real
/// [`Registry`] via [`Registry::load_manifest_into`] decodes every referenced
/// sample eagerly, so listing a pack like VSCO or Dirt-Samples that way reads
/// gigabytes of WAV into RAM just to show some names. This parses the manifest's
/// `[name]` tables only (the same flat schema the loader reads) — pure text, no
/// filesystem walk into the samples. A missing / unparseable file yields an
/// empty list.
pub fn list_manifest_instruments(path: &Path) -> Vec<InstrumentInfo> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(tables) = parse_toml_tables(&text) else {
        return Vec::new();
    };
    tables
        .into_iter()
        .map(|(name, kv)| {
            let kind = match kv.get("kind").map(String::as_str).unwrap_or("synth") {
                "sample" => InstrumentKind::Sample,
                "sfz" => InstrumentKind::Sfz,
                _ => InstrumentKind::Synth,
            };
            // Articulation declarations are keyed `art.<name>.<field>`; collect the
            // distinct `<name>`s (the same set `parse_articulations` would build).
            let mut articulations: Vec<String> = kv
                .keys()
                .filter_map(|k| k.strip_prefix("art."))
                .filter_map(|rest| rest.rsplit_once('.').map(|(art, _)| art.to_string()))
                .collect();
            articulations.sort();
            articulations.dedup();
            InstrumentInfo { name, kind, articulations }
        })
        .collect()
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

/// Whether a path looks like an audio file nemus's decoder can read (WAV via
/// `hound`, the rest via `symphonia`). Used to filter a sample `dir` so stray
/// non-audio files (READMEs, `.DS_Store`) don't become phantom variants.
fn is_decodable(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("wav" | "wave" | "flac" | "ogg" | "mp3")
    )
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
        Some("square" | "sqr" | "pulse") => Waveform::Square,
        Some("sine" | "sin") => Waveform::Sine,
        Some("triangle" | "tri") => Waveform::Triangle,
        // `saw` | `sawtooth` | anything unrecognised.
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
            // Headers may be quoted (`["808"]`, `["strings.violin"]`) or bare
            // (`[bd]`, `[synth.bass]`). Unquote so the entry's resolvable name is
            // `808`, not `"808"` — otherwise `s("808")` never matches it (and the
            // sound bank shows the literal quotes).
            current = Some((unquote(name.trim()), HashMap::new()));
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
    fn quoted_table_headers_are_unquoted() {
        // Dirt-Samples / VSCO write quoted headers (`["808"]`); the resolvable
        // name must be the bare `808`, not `"808"` (else `s("808")` misses it
        // and the sound bank shows the literal quotes).
        let src = "\
[\"808\"]
kind = \"sample\"
dir = \"808\"

[\"strings.violin\"]
kind = \"sfz\"
file = \"strings/violin.sfz\"
";
        let tables = parse_toml_tables(src).unwrap();
        assert_eq!(tables[0].0, "808");
        assert_eq!(tables[1].0, "strings.violin");
    }

    #[test]
    fn unresolved_name_falls_back_to_synth() {
        let reg = Registry::new();
        match reg.resolve(Some("nope"), None, None, Some(60.0), 0.8, None, 0) {
            ResolvedVoice::Synth(_) => {}
            _ => panic!("expected synth fallback"),
        }
    }

    #[test]
    fn builtin_synths_resolve_after_install() {
        let mut reg = Registry::new();
        reg.install_builtin_synths();
        for name in ["synth.bass", "synth.pad", "synth.pluck", "synth.lead"] {
            match reg.resolve(None, Some(name), None, Some(60.0), 0.8, None, 0) {
                ResolvedVoice::Synth(_) => {}
                other => panic!("`{name}` should resolve to a synth preset, got {other:?}"),
            }
        }
        // The pad is the soft triangle; the bass is the saw.
        match reg.resolve(None, Some("synth.pad"), None, Some(60.0), 0.8, None, 0) {
            ResolvedVoice::Synth(p) => assert_eq!(p.shape, SynthShape::Wave(Waveform::Triangle)),
            _ => unreachable!(),
        }
        match reg.resolve(None, Some("synth.bass"), None, Some(60.0), 0.8, None, 0) {
            ResolvedVoice::Synth(p) => assert_eq!(p.shape, SynthShape::Wave(Waveform::Saw)),
            _ => unreachable!(),
        }
    }

    #[test]
    fn oscillator_names_resolve_to_their_waveform() {
        let mut reg = Registry::new();
        reg.install_builtin_synths();
        // Bare names + aliases all resolve to a synth on the expected shape.
        for (name, want) in [
            ("sine", Waveform::Sine),
            ("sin", Waveform::Sine),
            ("sawtooth", Waveform::Saw),
            ("saw", Waveform::Saw),
            ("square", Waveform::Square),
            ("sqr", Waveform::Square),
            ("pulse", Waveform::Square),
            ("triangle", Waveform::Triangle),
            ("tri", Waveform::Triangle),
        ] {
            match reg.resolve(Some(name), None, None, Some(60.0), 0.8, None, 0) {
                ResolvedVoice::Synth(p) => {
                    assert_eq!(p.shape, SynthShape::Wave(want), "`{name}` should be {want:?}");
                    // Gate envelope: full sustain so the note holds for its duration.
                    assert_eq!(p.sustain, 1.0, "`{name}` should use the gate envelope");
                }
                other => panic!("`{name}` should resolve to a synth, got {other:?}"),
            }
        }
    }

    #[test]
    fn noise_and_supersaw_names_resolve() {
        let mut reg = Registry::new();
        reg.install_builtin_synths();
        for (name, want) in [
            ("supersaw", SynthShape::Supersaw),
            ("white", SynthShape::Noise(NoiseColor::White)),
            ("pink", SynthShape::Noise(NoiseColor::Pink)),
            ("brown", SynthShape::Noise(NoiseColor::Brown)),
            ("crackle", SynthShape::Noise(NoiseColor::Crackle)),
        ] {
            match reg.resolve(Some(name), None, None, Some(60.0), 0.8, None, 0) {
                ResolvedVoice::Synth(p) => assert_eq!(p.shape, want, "`{name}`"),
                other => panic!("`{name}` should resolve to a synth, got {other:?}"),
            }
        }
    }

    #[test]
    fn synth_preset_resolves() {
        let mut reg = Registry::new();
        reg.insert_synth(
            "synth.pad",
            SynthPreset {
                shape: SynthShape::Wave(Waveform::Triangle),
                ..SynthPreset::default()
            },
        );
        match reg.resolve(None, Some("synth.pad"), None, Some(60.0), 0.8, None, 0) {
            ResolvedVoice::Synth(p) => assert_eq!(p.shape, SynthShape::Wave(Waveform::Triangle)),
            _ => panic!("expected synth"),
        }
    }

    #[test]
    fn sample_variants_select_by_index_and_round_robin() {
        use crate::decode::DecodedAudio;
        let mut reg = Registry::new();
        // Three resident samples, each tagged by a distinct (dummy) sample rate
        // so the test can tell which variant was picked.
        let tag = |rate: u32| Sample::from_decoded(DecodedAudio { samples: vec![0.0; 4], sample_rate: rate });
        reg.bank.insert("a", tag(1));
        reg.bank.insert("b", tag(2));
        reg.bank.insert("c", tag(3));
        reg.entries.insert(
            "hh".to_string(),
            Entry::Sample { variants: vec!["a".into(), "b".into(), "c".into()] },
        );
        let pick = |variant, seed| match reg.resolve(Some("hh"), None, variant, None, 0.8, None, seed) {
            ResolvedVoice::Sample { sample, .. } => sample.sample_rate,
            other => panic!("expected a sample, got {other:?}"),
        };
        // Explicit `:n` indexes, wrapping past the end.
        assert_eq!(pick(Some(0), 0), 1);
        assert_eq!(pick(Some(1), 0), 2);
        assert_eq!(pick(Some(2), 0), 3);
        assert_eq!(pick(Some(3), 0), 1); // 3 % 3
        // No `:n` → deterministic round-robin by onset seed.
        assert_eq!(pick(None, 0), 1); // 0 % 3
        assert_eq!(pick(None, 1), 2);
        assert_eq!(pick(None, 5), 3); // 5 % 3 == 2
    }

    #[test]
    fn sample_entry_requires_file_or_dir() {
        // kind=sample with neither `file` nor `dir` is a manifest error.
        let mut reg = Registry::new();
        let mut kv = HashMap::new();
        kv.insert("kind".to_string(), "sample".to_string());
        assert!(reg.add_entry("bd", &kv, Path::new(".")).is_err());
    }

    #[test]
    fn parses_articulation_keyswitch_declarations() {
        // The flat manifest parser flattens `art.<name>.keyswitch` keys; parsing
        // them into `Articulation::Keyswitch` is what we check here (no files).
        let mut kv = HashMap::new();
        kv.insert("art.legato.keyswitch".to_string(), "24".to_string());
        kv.insert("art.pizzicato.keyswitch".to_string(), "26".to_string());
        let mut reg = Registry::new();
        let arts = reg.parse_articulations(&kv, Path::new(".")).unwrap();
        assert_eq!(arts.len(), 2);
        match arts.get("legato") {
            Some(Articulation::Keyswitch(24)) => {}
            other => panic!("expected legato keyswitch 24, got {other:?}"),
        }
        match arts.get("pizzicato") {
            Some(Articulation::Keyswitch(26)) => {}
            other => panic!("expected pizzicato keyswitch 26, got {other:?}"),
        }
    }
}
