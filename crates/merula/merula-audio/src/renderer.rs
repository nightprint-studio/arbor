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

use crate::effects::{equal_power_pan, Compressor, DelayLine, EqChain, Limiter, Reverb};
use crate::registry::{Registry, ResolvedVoice, SampleParams};
use crate::sampler::{Sample, SampleBank};
use crate::seam::{AudioCommand, Frame, ReverbIr, TrackConfig, VoiceEvent, VoiceId, VoiceSource};
use crate::voice::{Voice, VoicePool};
use merula_pattern::prelude::SourceKind;

/// Default voice-pool capacity (design: 128). A `const` now; user-configurable
/// later through the renderer constructor.
pub const DEFAULT_VOICE_CAPACITY: usize = 128;

/// Voice capacity of the dedicated **audition** bus (instrument preview + one-shot
/// snippet test). A single preview note needs only a few voices, but a multi-track
/// snippet played over several cycles can stack dozens of overlapping voices — so
/// the bus is sized to host a small arrangement without audible voice-stealing.
const AUDITION_VOICE_CAPACITY: usize = 64;

/// Default `room` reverb size (`0..1`) until an IR is installed. The default
/// reverb is the **O(1) algorithmic [`Reverb::Algo`]** (Freeverb), whose cost is
/// constant per sample regardless of tail length — so unlike the old naive
/// convolution (O(IR taps) per sample, which overran the callback on `room`-heavy
/// material and was heard as distortion) the size is purely a tonal choice. `0.5`
/// is a medium room; higher = longer/more diffuse.
const DEFAULT_REVERB_SECS: f32 = 0.5;

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
    /// Dedicated **audition** voices (instrument preview). Separate from the song
    /// `pool`: they bypass the mixer strips and fold straight into the master, so a
    /// preview is unaffected by — and doesn't disturb — the song mix.
    audition: VoicePool,
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
    reverb: Reverb,
    limiter: Limiter,
    /// Deepest limiter gain-reduction multiplier `0..1` over the current `process`
    /// block (1 = none). Block-local like `track_peak`: reset to 1.0 at the top of
    /// `process`, min'd each frame; the callback reads it via [`limiter_reduction`]
    /// (Self::limiter_reduction) for the master GR meter, applying its own decay.
    limiter_gr_min: f32,
    registry: Registry,
    /// Resident audio for `File` (`sample`/`audio`) voices, keyed by path.
    files: SampleBank,
    /// Voices scheduled to start later within the current/next block.
    pending: BinaryHeap<Pending>,
    /// Per-track currently-sounding legato voice (the `art("legato")` mono line).
    /// When the next legato onset on a track arrives and this voice is still
    /// resident, it is re-pitched instead of spawning a fresh voice — connecting
    /// the notes with no envelope re-attack. Indexed by mixer strip; `None` =
    /// nothing to glide from (start of line, after a rest, or after a steal).
    legato: Vec<Option<VoiceId>>,
    /// Idle gate: set on [`AudioCommand::StopAll`] (transport stop), cleared on the
    /// next real activity (a queued voice). While idle *and* nothing is sounding,
    /// [`process`](Self::process) takes a silence fast-path that skips the entire
    /// per-frame DSP graph (strips, delay buses, reverb, limiter) — so
    /// a stopped session is genuinely idle (DSP load → ~0) instead of grinding the
    /// effect graph on silence for every device buffer until window close.
    idle: bool,
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
            audition: VoicePool::new(AUDITION_VOICE_CAPACITY),
            strips: (0..strip_count).map(|_| Strip::new(sample_rate)).collect(),
            any_soloed: false,
            track_dry: vec![[0.0; 2]; strip_count],
            track_delay_send: vec![[0.0; 2]; strip_count],
            track_peak: vec![[0.0; 2]; strip_count],
            master: Master::default(),
            reverb: Reverb::procedural(DEFAULT_REVERB_SECS, sample_rate as f32),
            limiter: Limiter::new(0.95, 0.05, sample_rate as f32),
            limiter_gr_min: 1.0,
            registry: Registry::new(),
            files: SampleBank::new(),
            // Pre-reserve so per-block `push`es don't grow the heap in the RT
            // callback (one look-ahead window is a few hundred events at most).
            pending: BinaryHeap::with_capacity(capacity * 4),
            legato: vec![None; strip_count],
            // A fresh renderer has never played: start idle so an open-but-unplayed
            // stream costs ~nothing until the first voice arrives.
            idle: true,
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
        // Reset the block limiter gain-reduction floor; `render_frame` mins into it.
        self.limiter_gr_min = 1.0;

        // 1. Apply commands. Voice triggers are queued (sample-accurate); mixer
        //    / transport commands take effect immediately. A queued voice clears
        //    the idle gate (see `apply_command`), so a Play after Stop wakes the
        //    DSP path here, the same buffer the first voice arrives.
        for cmd in commands {
            self.apply_command(cmd);
        }

        // Idle fast-path: stopped (StopAll seen) and nothing left sounding or
        // pending. Tails are already flushed by StopAll, so the graph would only
        // grind out silence — skip it. Write zeros, hold the meters at zero, and
        // still advance the clock so the transport's free-running "now" stays
        // truthful (seek/play re-anchor against it).
        if self.idle && self.pool.active() == 0 && self.pending.is_empty()
            && self.audition.active() == 0
        {
            for frame_out in out.iter_mut() {
                *frame_out = [0.0; 2];
            }
            self.clock = block_end;
            return;
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

    /// Number of currently sounding voices (for meters / tests) — song voices plus
    /// any live preview/audition voices.
    pub fn active_voices(&self) -> usize {
        self.pool.active() + self.audition.active()
    }

    /// Whether the renderer is on the idle silence fast-path: stopped, with nothing
    /// sounding or pending. The real-time callback reads this to zero the DSP-load
    /// meter directly (the fast-path does no measurable work, so an EMA would only
    /// crawl toward zero — this snaps it).
    pub fn is_idle(&self) -> bool {
        self.idle
            && self.pool.active() == 0
            && self.pending.is_empty()
            && self.audition.active() == 0
    }

    /// Per-track post-fader peak `[L, R]` over the most recent `process` block,
    /// for the out-of-band meter tap. Indexed by mixer strip; not decayed (the
    /// caller applies meter ballistics, as with the master peak).
    pub fn track_peaks(&self) -> &[Frame] {
        &self.track_peak
    }

    /// Deepest master limiter **gain reduction** `0..1` over the most recent
    /// `process` block (`0` = none, `0.3` ≈ −3 dB-ish of ducking). For the
    /// out-of-band GR meter; not decayed (the caller applies meter ballistics).
    pub fn limiter_reduction(&self) -> f32 {
        1.0 - self.limiter_gr_min
    }

    // ── internals ────────────────────────────────────────────────────────────

    fn apply_command(&mut self, cmd: AudioCommand) {
        match cmd {
            AudioCommand::Voice(ev) => {
                // A real trigger means playback resumed: leave the idle fast-path
                // so the DSP graph runs again from this buffer.
                self.idle = false;
                // A voice already overdue (start ≤ clock) starts at the block top.
                let start = ev.start_frame.max(self.clock);
                self.pending.push(Pending {
                    start_frame: start,
                    event: ev,
                });
            }
            AudioCommand::Audition(ev) => {
                // Preview note: spawn immediately into the dedicated audition pool
                // — no sample-accurate scheduling, no legato, no strip routing. We
                // deliberately don't touch `idle`: the idle gate already accounts
                // for live audition voices, so a stopped session wakes the graph
                // for the preview and re-idles once it finishes.
                let resolved = self.resolve_source(&ev);
                let release_at = ev.dur_frames.map(|d| self.clock.saturating_add(d));
                let voice = Voice::build(
                    ev.id,
                    ev.track,
                    resolved,
                    ev.note,
                    &ev.params,
                    release_at,
                    self.sample_rate as f32,
                );
                self.audition.insert(voice);
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
                        Reverb::procedural(seconds, self.sample_rate as f32)
                    }
                    ReverbIr::Buffer(buf) => Reverb::from_buffer(buf),
                };
            }
            AudioCommand::StopAll => {
                // Transport stop / panic: drop every voice and pending trigger,
                // then flush the effect tails. A feedback delay line and the
                // convolution reverb don't decay to *exact* zero on their own, and
                // the renderer advances them every frame regardless of play state —
                // so without this flush a `room`/delay tail rings on (audibly, and
                // as perpetual DSP load) long after playback has stopped.
                self.pool.clear();
                self.audition.clear();
                self.pending.clear();
                for l in &mut self.legato {
                    *l = None;
                }
                self.reset_dsp_tails();
                // Arm the idle fast-path: from the next buffer (with nothing left
                // sounding) `process` skips the whole DSP graph until a voice wakes
                // it, so a stopped session reports ~0 DSP load instead of grinding
                // the effect chain on silence.
                self.idle = true;
            }
            AudioCommand::StopAudition => {
                // Stop an in-flight snippet preview early: drop only the audition
                // voices. The song's pool / pending / effect tails are untouched, so
                // a playing song keeps sounding. Don't arm `idle` — the main pool may
                // still be live.
                self.audition.clear();
            }
        }
    }

    /// Clear the buffered tails / running state of every strip + master effect and
    /// the reverb bus, so the renderer returns to exact silence and idle DSP. Keeps
    /// all *configuration* (gains, EQ bands, delay times, IR) intact — only the
    /// time-varying state is flushed.
    fn reset_dsp_tails(&mut self) {
        for strip in &mut self.strips {
            strip.eq.reset();
            if let Some(comp) = strip.comp.as_mut() {
                comp.reset();
            }
            strip.delay.reset();
        }
        self.master.eq.reset();
        if let Some(comp) = self.master.comp.as_mut() {
            comp.reset();
        }
        self.reverb.reset();
        self.limiter.reset();
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
        // A track-set swap invalidates any in-flight legato line.
        self.legato = vec![None; count];
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
    ///
    /// A `legato` event (`art("legato")`) on a track whose previous voice is still
    /// sounding connects to it instead of starting detached — by strategy:
    /// - **synth** sources re-pitch the existing voice in place (truly continuous,
    ///   no re-attack);
    /// - **sampler** sources **crossfade**: the old note fades out while a fresh,
    ///   correctly-pitched voice fades in, so the recorded sample onset is masked
    ///   and there's no gap (an in-place restart would replay the bow attack and
    ///   still sound detached).
    ///
    /// Otherwise a fresh voice is built. The per-track legato slot is updated to
    /// the voice now carrying the line (legato events) or cleared (non-legato).
    fn spawn_voice(&mut self, ev: VoiceEvent, frame: u64) {
        let resolved = self.resolve_source(&ev);
        let release_at = ev.dur_frames.map(|d| frame.saturating_add(d));
        let track = ev.track as usize;
        let legato = is_legato(&ev);
        let sr = self.sample_rate as f32;
        let xfade = legato_xfade_frames(self.sample_rate);

        // Whether to fade the new voice in (set when we just faded out a sampler
        // voice it's crossfading from).
        let mut crossfade_in = false;
        if legato {
            if let Some(prev) = self.legato.get(track).copied().flatten() {
                if let Some(voice) = self.pool.get_mut_by_id(prev) {
                    if !voice.is_done() {
                        if voice.is_synth() {
                            voice.reglide(resolved, ev.note, &ev.params, release_at, sr);
                            return;
                        }
                        // Sampler: keep the old note ringing and crossfade into the
                        // new one (built below).
                        voice.fade_out(xfade);
                        crossfade_in = true;
                    }
                }
            }
        }

        let mut voice = Voice::build(ev.id, ev.track, resolved, ev.note, &ev.params, release_at, sr);
        if crossfade_in {
            voice.fade_in(xfade);
        }
        let id = voice.id;
        self.pool.insert(voice);
        if let Some(cell) = self.legato.get_mut(track) {
            *cell = if legato { Some(id) } else { None };
        }
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
    /// → gain, the reverb send folds in, then the limiter caps the output.
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

        // Preview / audition voices: a dedicated bus that bypasses the song strips
        // (no per-strip gain / mute / solo / insert) and folds straight into the
        // master sum, so a preview is heard cleanly whatever the song mix is doing.
        // The shared reverb send is passed through (audition voices default to no
        // `room`, so they don't feed it unless asked); the per-track delay send is a
        // throwaway (the audition bus has no delay line).
        if self.audition.active() > 0 {
            let mut aud = [[0.0f32; 2]];
            let mut aud_delay = [[0.0f32; 2]];
            self.audition
                .process_sample(frame, &mut aud, &mut reverb_send, &mut aud_delay);
            master[0] += aud[0][0];
            master[1] += aud[0][1];
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
        let out = self.limiter.process(master);
        // Tap the deepest reduction this block for the out-of-band GR meter.
        self.limiter_gr_min = self.limiter_gr_min.min(self.limiter.current_gain());
        out
    }
}

/// Crossfade length for sampler legato, in seconds. Long enough to mask a bowed
/// sample's recorded onset and bridge the note boundary, short enough not to
/// audibly double fast notes.
const LEGATO_XFADE_SECS: f32 = 0.04;

/// [`LEGATO_XFADE_SECS`] in frames at `sample_rate` (at least one frame).
fn legato_xfade_frames(sample_rate: u32) -> u64 {
    ((LEGATO_XFADE_SECS * sample_rate as f32).round() as u64).max(1)
}

/// Whether an event requests monophonic **connected** voicing — `art("legato")`
/// or any `.hold(...)` (drone / pad). The engine computes this from the full
/// `ControlMap`; the renderer just reads the flag. Connected events re-pitch the
/// track's voice (synth) or crossfade (sampler); detached articulations
/// (`staccato`, …) and plain notes always spawn a fresh voice.
fn is_legato(ev: &VoiceEvent) -> bool {
    ev.legato
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seam::VoiceParams;

    const SR: u32 = 48_000;

    /// A pitched named-instrument onset; unknown `inst` resolves to the fallback
    /// synth, which is all these voice-management tests need.
    fn note_event(id: u64, start: u64, dur: u64, art: Option<&str>) -> VoiceEvent {
        VoiceEvent {
            id: VoiceId(id),
            start_frame: start,
            dur_frames: Some(dur),
            legato: art.is_some_and(|a| a.eq_ignore_ascii_case("legato")),
            source: VoiceSource::Named {
                sound: None,
                variant: None,
                inst: Some("test.tone".into()),
                art: art.map(str::to_string),
            },
            note: Some(60.0 + id as f32), // distinct pitch per onset
            params: VoiceParams::default(),
            track: 0,
            span: None,
        }
    }

    fn render(r: &mut Renderer, cmds: Vec<AudioCommand>, frames: usize) {
        let mut out = vec![[0.0f32; 2]; frames];
        r.process(&mut cmds.into_iter(), &mut out);
    }

    #[test]
    fn legato_onset_reuses_the_sounding_voice() {
        let mut r = Renderer::new(SR, &[TrackConfig { name: "lead".into() }]);
        // First legato note starts at 0, lasting 4000 frames.
        render(&mut r, vec![AudioCommand::Voice(note_event(1, 0, 4000, Some("legato")))], 1000);
        assert_eq!(r.active_voices(), 1, "first note sounds");
        // A second legato note arrives mid-way through the first: it must glide the
        // existing voice, not stack a new one.
        render(&mut r, vec![AudioCommand::Voice(note_event(2, 1000, 4000, Some("legato")))], 1000);
        assert_eq!(r.active_voices(), 1, "legato glides one voice, no stacking");
    }

    #[test]
    fn non_legato_onset_stacks_a_new_voice() {
        let mut r = Renderer::new(SR, &[TrackConfig { name: "lead".into() }]);
        render(&mut r, vec![AudioCommand::Voice(note_event(1, 0, 4000, None))], 1000);
        render(&mut r, vec![AudioCommand::Voice(note_event(2, 1000, 4000, None))], 1000);
        assert_eq!(r.active_voices(), 2, "detached notes overlap as two voices");
    }

    #[test]
    fn legato_reattacks_after_the_line_goes_silent() {
        let mut r = Renderer::new(SR, &[TrackConfig { name: "lead".into() }]);
        // Short note that fully rings out (release) before the next onset. The
        // fallback preset's release is ~0.2 s, so a generous window guarantees the
        // exponential tail has dropped below the silence floor and freed the voice.
        render(&mut r, vec![AudioCommand::Voice(note_event(1, 0, 100, Some("legato")))], 20_000);
        assert_eq!(r.active_voices(), 0, "first note released into silence");
        // With nothing to glide from, the next legato onset spawns a fresh voice.
        render(&mut r, vec![AudioCommand::Voice(note_event(2, 20_000, 4000, Some("legato")))], 1000);
        assert_eq!(r.active_voices(), 1, "fresh attack after the rest");
    }
}
