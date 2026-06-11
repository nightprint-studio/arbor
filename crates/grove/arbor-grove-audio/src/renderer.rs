//! The DSP core: voice pool + mixer + effects + registry + sample bank.
//!
//! `Renderer` is **transport-agnostic** — it neither opens a stream nor owns a
//! ring buffer. It is driven two ways against the exact same code path:
//! - the real-time cpal callback (`crate::stream`) drains the ring and calls
//!   [`Renderer::process`] for each device buffer;
//! - the engine's offline render driver calls [`Renderer::process`] block by
//!   block in non-real-time and writes the output to WAV.
//!
//! ## Signal flow
//!
//! ```text
//! voices ─┬─▶ per-track strip (gain, mute) ─┐
//!         │                                  ├─▶ master sum ─▶ limiter ─▶ out
//!         └─▶ × room ─▶ reverb send bus ─────┘
//! ```
//!
//! ## Real-time discipline
//!
//! `process` never allocates, locks, or does IO. Pools and the per-track scratch
//! buffer are sized in [`new`](Renderer::new) / [`configure_tracks`]; sample data
//! and SFZ instruments are loaded ahead of time on a non-RT path
//! ([`registry_mut`](Renderer::registry_mut) / [`preload_file`]) and only read in
//! the callback. A `File` voice whose sample isn't resident falls back to the
//! synth rather than blocking.

use std::collections::BinaryHeap;
use std::path::Path;

use crate::effects::{equal_power_pan, Compressor, ConvReverb, DelayLine, EqChain, Limiter};
use crate::registry::{Registry, ResolvedVoice, SampleParams};
use crate::sampler::{Sample, SampleBank};
use crate::seam::{AudioCommand, Frame, ReverbIr, TrackConfig, VoiceEvent, VoiceSource};
use crate::voice::{Voice, VoicePool};
use arbor_grove_pattern::prelude::SourceKind;

/// Default voice-pool capacity (design: 128). A `const` now; user-configurable
/// later through the renderer constructor.
pub const DEFAULT_VOICE_CAPACITY: usize = 128;

/// Default procedural reverb tail length (seconds) until an IR is installed.
/// Kept short on purpose: the convolution is naive time-domain (O(IR) per sample),
/// so a multi-second tail is a partitioned-FFT concern (Onda 3). A ~0.12 s dense
/// IR gives a convincing room while staying real-time-tractable.
const DEFAULT_REVERB_SECS: f32 = 0.12;

/// Max delay-line length per track (seconds): caps `delay` so an absurd cycle
/// fraction can't allocate without bound.
const MAX_DELAY_SECS: f32 = 4.0;

/// One mixer strip: gain, pan, mute/solo, and optional EQ + compressor inserts.
#[derive(Debug)]
struct Strip {
    gain: f32,
    /// Stereo pan `0` left … `1` right (post-mix on the strip sum).
    pan: f32,
    muted: bool,
    soloed: bool,
    /// Parametric EQ insert (empty = bypass).
    eq: EqChain,
    /// Compressor insert (`None` = bypass).
    comp: Option<Compressor>,
    /// Per-track delay bus.
    delay: DelayLine,
}

impl Strip {
    fn new(sample_rate: u32) -> Self {
        let max_delay = (MAX_DELAY_SECS * sample_rate as f32) as usize;
        Strip {
            gain: 1.0,
            pan: 0.5,
            muted: false,
            soloed: false,
            eq: EqChain::default(),
            comp: None,
            delay: DelayLine::new(max_delay),
        }
    }
}

/// A voice waiting to start at a specific absolute frame (sample-accurate
/// within-block onset). Ordered so the *earliest* start pops first.
#[derive(Debug)]
struct Pending {
    start_frame: u64,
    event: VoiceEvent,
}

impl PartialEq for Pending {
    fn eq(&self, other: &Self) -> bool {
        self.start_frame == other.start_frame
    }
}
impl Eq for Pending {}
impl PartialOrd for Pending {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Pending {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Reverse: `BinaryHeap` is a max-heap, we want the soonest start on top.
        other.start_frame.cmp(&self.start_frame)
    }
}

/// The master strip: gain + optional EQ/compressor inserts, before the limiter.
#[derive(Debug)]
struct Master {
    gain: f32,
    eq: EqChain,
    comp: Option<Compressor>,
}

impl Default for Master {
    fn default() -> Self {
        Master {
            gain: 1.0,
            eq: EqChain::default(),
            comp: None,
        }
    }
}

/// Owns all sounding state and turns commands + time into samples.
pub struct Renderer {
    sample_rate: u32,
    /// Absolute output frame of the *next* frame to render.
    clock: u64,
    pool: VoicePool,
    strips: Vec<Strip>,
    /// Whether any strip is currently soloed (drives solo-mutes-the-rest).
    any_soloed: bool,
    /// Reused per-track dry accumulator (one L/R per strip), cleared each frame.
    track_dry: Vec<Frame>,
    /// Reused per-track delay-bus send accumulator, cleared each frame.
    track_delay_send: Vec<Frame>,
    /// Per-track post-fader peak `|L|/|R|` over the current `process` block, for
    /// the out-of-band meter tap. Block-local: cleared at the top of `process`,
    /// max'd each frame; the callback applies its own decay against the shared
    /// atomic (so ballistics live in one place, next to the master peak).
    track_peak: Vec<Frame>,
    master: Master,
    reverb: ConvReverb,
    limiter: Limiter,
    registry: Registry,
    /// Resident audio for `File` (`sample`/`audio`) voices, keyed by path.
    files: SampleBank,
    /// Voices scheduled to start later within the current/next block.
    pending: BinaryHeap<Pending>,
}

impl std::fmt::Debug for Renderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Renderer")
            .field("sample_rate", &self.sample_rate)
            .field("clock", &self.clock)
            .field("active_voices", &self.pool.active())
            .field("tracks", &self.strips.len())
            .finish_non_exhaustive()
    }
}

impl Renderer {
    /// Build a renderer for `sample_rate`, laying out a mixer strip per track.
    pub fn new(sample_rate: u32, tracks: &[TrackConfig]) -> Self {
        Renderer::with_capacity(sample_rate, tracks, DEFAULT_VOICE_CAPACITY)
    }

    /// Build a renderer with an explicit voice-pool capacity.
    pub fn with_capacity(sample_rate: u32, tracks: &[TrackConfig], capacity: usize) -> Self {
        let strip_count = tracks.len().max(1);
        Renderer {
            sample_rate,
            clock: 0,
            pool: VoicePool::new(capacity),
            strips: (0..strip_count).map(|_| Strip::new(sample_rate)).collect(),
            any_soloed: false,
            track_dry: vec![[0.0; 2]; strip_count],
            track_delay_send: vec![[0.0; 2]; strip_count],
            track_peak: vec![[0.0; 2]; strip_count],
            master: Master::default(),
            reverb: ConvReverb::procedural(DEFAULT_REVERB_SECS, sample_rate as f32),
            limiter: Limiter::new(0.95, 0.05, sample_rate as f32),
            registry: Registry::new(),
            files: SampleBank::new(),
            // Pre-reserve so per-block `push`es don't grow the heap in the RT
            // callback (one look-ahead window is a few hundred events at most).
            pending: BinaryHeap::with_capacity(capacity * 4),
        }
    }

    /// Frames per second this renderer produces.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// The current absolute frame clock (next frame to be rendered).
    pub fn now_frame(&self) -> u64 {
        self.clock
    }

    /// Mutable access to the sound registry for **non-RT** setup (load a manifest,
    /// install presets). Never call from the audio callback.
    pub fn registry_mut(&mut self) -> &mut Registry {
        &mut self.registry
    }

    /// Replace the registry wholesale (non-RT).
    pub fn set_registry(&mut self, registry: Registry) {
        self.registry = registry;
    }

    /// Decode a `sample`/`audio` file and keep it resident so `File` voices for
    /// `path` play instead of falling back to the synth. **Non-RT** (IO + alloc).
    pub fn preload_file(&mut self, path: &Path) -> crate::error::Result<()> {
        self.files.load(path)?;
        Ok(())
    }

    /// Drain the **due** commands, render exactly `out.len()` frames into `out`,
    /// and advance the internal frame clock by that many frames.
    ///
    /// `commands` yields the commands scheduled up to the end of this block;
    /// voice `start_frame`s inside the block are honoured sample-accurately
    /// (a voice starting mid-block stays silent until its frame).
    pub fn process(&mut self, commands: &mut dyn Iterator<Item = AudioCommand>, out: &mut [Frame]) {
        let block_end = self.clock + out.len() as u64;

        // Reset the per-track block peak; `render_frame` maxes into it and the
        // callback reads it after this call (with its own decay).
        for p in &mut self.track_peak {
            *p = [0.0; 2];
        }

        // 1. Apply commands. Voice triggers are queued (sample-accurate); mixer
        //    / transport commands take effect immediately.
        for cmd in commands {
            self.apply_command(cmd);
        }

        // 2. Render frame by frame, starting any pending voices at their frame.
        //    `start_due_voices` pops every voice with start_frame ≤ the current
        //    frame, so by block end nothing due this block is left pending; a
        //    voice whose start lands in a later block simply waits.
        for (i, frame_out) in out.iter_mut().enumerate() {
            let frame = self.clock + i as u64;
            self.start_due_voices(frame);
            *frame_out = self.render_frame(frame);
        }

        self.clock = block_end;
    }

    /// Number of currently sounding voices (for meters / tests).
    pub fn active_voices(&self) -> usize {
        self.pool.active()
    }

    /// Per-track post-fader peak `[L, R]` over the most recent `process` block,
    /// for the out-of-band meter tap. Indexed by mixer strip; not decayed (the
    /// caller applies meter ballistics, as with the master peak).
    pub fn track_peaks(&self) -> &[Frame] {
        &self.track_peak
    }

    // ── internals ────────────────────────────────────────────────────────────

    fn apply_command(&mut self, cmd: AudioCommand) {
        match cmd {
            AudioCommand::Voice(ev) => {
                // A voice already overdue (start ≤ clock) starts at the block top.
                let start = ev.start_frame.max(self.clock);
                self.pending.push(Pending {
                    start_frame: start,
                    event: ev,
                });
            }
            AudioCommand::ConfigureTracks(tracks) => self.configure_tracks(&tracks),
            AudioCommand::SetTrackGain(i, g) => {
                if let Some(s) = self.strips.get_mut(i as usize) {
                    s.gain = g;
                }
            }
            AudioCommand::SetTrackPan(i, p) => {
                if let Some(s) = self.strips.get_mut(i as usize) {
                    s.pan = p.clamp(0.0, 1.0);
                }
            }
            AudioCommand::SetTrackMute(i, m) => {
                if let Some(s) = self.strips.get_mut(i as usize) {
                    s.muted = m;
                }
            }
            AudioCommand::SetTrackSolo(i, solo) => {
                if let Some(s) = self.strips.get_mut(i as usize) {
                    s.soloed = solo;
                }
                self.any_soloed = self.strips.iter().any(|s| s.soloed);
            }
            AudioCommand::SetMasterGain(g) => self.master.gain = g,
            AudioCommand::SetTrackEq(i, bands) => {
                let sr = self.sample_rate as f32;
                if let Some(s) = self.strips.get_mut(i as usize) {
                    s.eq = EqChain::new(&bands, sr);
                }
            }
            AudioCommand::SetMasterEq(bands) => {
                self.master.eq = EqChain::new(&bands, self.sample_rate as f32);
            }
            AudioCommand::SetTrackComp(i, settings) => {
                let sr = self.sample_rate as f32;
                if let Some(s) = self.strips.get_mut(i as usize) {
                    s.comp = settings.map(|c| Compressor::new(&c, sr));
                }
            }
            AudioCommand::SetMasterComp(settings) => {
                let sr = self.sample_rate as f32;
                self.master.comp = settings.map(|c| Compressor::new(&c, sr));
            }
            AudioCommand::SetTrackDelay(i, cfg) => {
                if let Some(s) = self.strips.get_mut(i as usize) {
                    s.delay.configure(cfg.time_frames, cfg.feedback);
                }
            }
            AudioCommand::SetReverbIr(ir) => {
                self.reverb = match ir {
                    ReverbIr::Procedural { seconds } => {
                        ConvReverb::procedural(seconds, self.sample_rate as f32)
                    }
                    ReverbIr::Buffer(buf) => ConvReverb::from_buffer(buf),
                };
            }
            AudioCommand::StopAll => {
                self.pool.release_all();
                self.pending.clear();
            }
        }
    }

    /// (Re)lay the mixer strips, preserving existing gain/pan/mute/solo/inserts
    /// where indices line up. Resizes the per-track scratch buffers to match.
    fn configure_tracks(&mut self, tracks: &[TrackConfig]) {
        let count = tracks.len().max(1);
        let sr = self.sample_rate;
        // Drain existing strips so their non-`Copy` inserts (EQ/comp/delay state)
        // are moved across rather than dropped where indices line up.
        let mut old: Vec<Option<Strip>> = self.strips.drain(..).map(Some).collect();
        let mut next = Vec::with_capacity(count);
        for i in 0..count {
            match old.get_mut(i).and_then(Option::take) {
                Some(s) => next.push(s),
                None => next.push(Strip::new(sr)),
            }
        }
        self.strips = next;
        self.any_soloed = self.strips.iter().any(|s| s.soloed);
        self.track_dry = vec![[0.0; 2]; count];
        self.track_delay_send = vec![[0.0; 2]; count];
        self.track_peak = vec![[0.0; 2]; count];
    }

    /// Start every pending voice whose `start_frame` equals `frame`.
    fn start_due_voices(&mut self, frame: u64) {
        while let Some(p) = self.pending.peek() {
            if p.start_frame <= frame {
                let Pending { event, .. } = self.pending.pop().expect("peeked");
                self.spawn_voice(event, frame);
            } else {
                break;
            }
        }
    }

    /// Resolve an event's source + build a [`Voice`] and add it to the pool.
    fn spawn_voice(&mut self, ev: VoiceEvent, frame: u64) {
        let resolved = self.resolve_source(&ev);
        let release_at = ev.dur_frames.map(|d| frame.saturating_add(d));
        let voice = Voice::build(
            ev.id,
            ev.track,
            resolved,
            ev.note,
            &ev.params,
            release_at,
            self.sample_rate as f32,
        );
        self.pool.insert(voice);
    }

    /// Turn a [`VoiceSource`] into a concrete [`ResolvedVoice`]; falls back to the
    /// synth for unresolved names and unresident files. RT-safe (reads resident
    /// state only).
    fn resolve_source(&self, ev: &VoiceEvent) -> ResolvedVoice {
        match &ev.source {
            VoiceSource::Named {
                sound,
                variant,
                inst,
                art,
            } => {
                // Deterministic per-onset seed for round-robin: derived from the
                // voice id (the engine assigns ids stably per onset, so a given
                // onset picks the same variant every loop).
                let seed = onset_seed(ev.id.0);
                self.registry.resolve(
                    sound.as_deref(),
                    inst.as_deref(),
                    *variant,
                    ev.note,
                    ev.params.vel,
                    art.as_deref(),
                    seed,
                )
            }
            VoiceSource::File { path, kind } => self.resolve_file(path, *kind),
        }
    }

    /// Resolve a `File` source against the resident file bank; fall back to the
    /// synth if not yet decoded. A `Sustained` stem plays whole from its start;
    /// a `OneShot` plays the hit and decays (both via the sample player — the
    /// distinction is just the release behaviour the engine drives via
    /// `dur_frames`).
    fn resolve_file(&self, path: &str, kind: SourceKind) -> ResolvedVoice {
        match self.files.get(path).or_else(|| self.files_get_by_suffix(path)) {
            Some(sample) => ResolvedVoice::Sample {
                sample,
                region: file_region(kind),
            },
            None => ResolvedVoice::Synth(self.registry.fallback()),
        }
    }

    /// Files are keyed by the absolute path used at preload; the engine may send
    /// the source-relative path. Resolve by suffix match as a fallback.
    fn files_get_by_suffix(&self, path: &str) -> Option<Sample> {
        let needle = path.replace('\\', "/");
        let key = self
            .files
            .keys()
            .find(|k| k.replace('\\', "/").ends_with(&needle))
            .map(|k| k.to_string())?;
        self.files.get(&key)
    }

    /// Render and mix one frame at absolute `frame`.
    ///
    /// Per strip: voices sum into `track_dry` (+ reverb send + per-track delay
    /// send); the strip applies EQ → compressor → delay-bus mix-back → pan → gain
    /// → mute/solo, then sums into the master. The master applies EQ → compressor
    /// → gain, the convolution reverb folds in, then the limiter caps the output.
    fn render_frame(&mut self, frame: u64) -> Frame {
        // Clear per-track + send accumulators.
        for d in &mut self.track_dry {
            *d = [0.0; 2];
        }
        for d in &mut self.track_delay_send {
            *d = [0.0; 2];
        }
        let mut reverb_send = [0.0f32; 2];

        // Sum all voices into their track strips + the reverb / delay sends.
        self.pool.process_sample(
            frame,
            &mut self.track_dry,
            &mut reverb_send,
            &mut self.track_delay_send,
        );

        let any_soloed = self.any_soloed;
        let mut master = [0.0f32; 2];
        // Split the borrows: `strips` is mutated, the per-track accumulators are
        // read by index — distinct fields, so borrow them separately up front.
        let strips = &mut self.strips;
        let track_dry = &self.track_dry;
        let track_delay_send = &self.track_delay_send;
        let track_peak = &mut self.track_peak;
        for (i, strip) in strips.iter_mut().enumerate() {
            // Solo wins over an un-soloed strip; an explicit mute always silences.
            let audible = !strip.muted && (!any_soloed || strip.soloed);

            // Feed the per-track delay bus (always advances so a held tail keeps
            // ringing even when the source has stopped) and read its echo.
            strip.delay.send(track_delay_send[i]);
            let echo = strip.delay.process();

            if !audible {
                continue;
            }

            // Dry track signal + the delay echo, through the strip inserts.
            let mut s = [
                track_dry[i][0] + echo[0],
                track_dry[i][1] + echo[1],
            ];
            if strip.eq.is_active() {
                s = strip.eq.process(s);
            }
            if let Some(comp) = strip.comp.as_mut() {
                s = comp.process(s);
            }
            // Strip pan is a stereo **balance** (attenuates the opposite side),
            // preserving the per-voice stereo image rather than collapsing it.
            let (bl, br) = equal_power_pan(strip.pan);
            // Equal-power balance: at centre both gains are ~0.707, so scale by
            // √2 to keep a centred strip unity.
            const CENTER_COMP: f32 = std::f32::consts::SQRT_2;
            let contrib = [
                s[0] * bl * CENTER_COMP * strip.gain,
                s[1] * br * CENTER_COMP * strip.gain,
            ];
            master[0] += contrib[0];
            master[1] += contrib[1];

            // Post-fader meter: this strip's contribution to the master sum.
            // Block-max; the callback decays it for the shared tap.
            let peak = &mut track_peak[i];
            peak[0] = peak[0].max(contrib[0].abs());
            peak[1] = peak[1].max(contrib[1].abs());
        }

        // Master EQ → compressor → gain.
        if self.master.eq.is_active() {
            master = self.master.eq.process(master);
        }
        if let Some(comp) = self.master.comp.as_mut() {
            master = comp.process(master);
        }
        master[0] *= self.master.gain;
        master[1] *= self.master.gain;

        // Convolution reverb send → wet → fold back into master.
        let wet = self.reverb.process(reverb_send);
        master[0] += wet[0];
        master[1] += wet[1];

        // Master limiter.
        self.limiter.process(master)
    }
}

/// Hash a voice id into a deterministic round-robin seed. A cheap integer mix so
/// consecutive ids don't trivially map to consecutive variants (which would defeat
/// the point of randomised round-robin) while staying reproducible loop-to-loop.
fn onset_seed(id: u64) -> u64 {
    // SplitMix64 finaliser.
    let mut z = id.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// SFZ-style playback params for a plain `File` source: full-range, native pitch.
/// `Sustained` stems ring out; the engine's `dur_frames` (or natural end) drives
/// release. We give a tiny release so stopping doesn't click.
fn file_region(_kind: SourceKind) -> SampleParams {
    SampleParams {
        // `pitch_keycenter` is irrelevant when the engine sends no note; an
        // unpitched File plays at native pitch (note=None → no offset).
        release: 0.01,
        ..SampleParams::default()
    }
}
