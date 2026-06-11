//! A sounding **voice**: the per-event DSP chain, plus the fixed-size pool and
//! its voice-stealing policy.
//!
//! ## DSP order (frozen design)
//!
//! ```text
//! source (sampler | synth)
//!   → pitch (shift + speed, via the source's resampling rate)
//!   → hpf → lpf            (biquad filters)
//!   → shape               (waveshaper)
//!   → crush               (bitcrush)
//!   → ADSR-shaped gain    (envelope is inside the source; `gain`×`vel` here)
//!   → pan                 (equal-power L/R)
//!   → [dry → track strip] + [× room → shared reverb send]
//! ```
//!
//! The source owns its own amplitude envelope (synth ADSR / SFZ `ampeg_*`); the
//! envelope shapes dynamics and decides natural end. `vel` both selected the
//! sampled layer upstream (in the registry) **and** scales dynamics here, which
//! is why it's distinct from `gain`.

use crate::effects::{self, Biquad};
use crate::registry::{ResolvedVoice, SampleParams};
use crate::sampler::SamplePlayer;
use crate::seam::{VoiceId, VoiceParams};
use crate::synth::{midi_to_freq, Adsr, SynthVoice};

/// The sound generator backing a voice — a synth oscillator+env, or a sample
/// player. The pitch/duration coupling for `shift`/`speed` is baked into the
/// generator (the synth's frequency, the player's read ratio).
#[derive(Clone, Debug)]
enum Source {
    Synth(SynthVoice),
    Sample {
        player: SamplePlayer,
        env: Adsr,
        /// Region static gain (linear) and pan (-1..1) folded in at build time.
        region_gain: f32,
        region_pan: f32,
    },
}

impl Source {
    fn next_sample(&mut self) -> f32 {
        match self {
            Source::Synth(s) => s.next_sample(),
            Source::Sample {
                player,
                env,
                region_gain,
                ..
            } => player.next_sample() * env.next_level() * *region_gain,
        }
    }

    fn release(&mut self) {
        match self {
            Source::Synth(s) => s.release(),
            Source::Sample { player, env, .. } => {
                player.release();
                env.release();
            }
        }
    }

    fn is_done(&self) -> bool {
        match self {
            Source::Synth(s) => s.is_done(),
            Source::Sample { player, env, .. } => player.is_done() || env.is_done(),
        }
    }

    /// Current amplitude-envelope level, for voice-stealing (quietest first).
    fn env_level(&self) -> f32 {
        match self {
            Source::Synth(s) => s.env_level(),
            Source::Sample { env, .. } => env.level(),
        }
    }

    /// Whether the source has entered its release stage.
    fn is_releasing(&self) -> bool {
        match self {
            Source::Synth(s) => s.is_releasing(),
            Source::Sample { env, .. } => env.is_releasing(),
        }
    }

    /// Region pan contribution (-1..1), only sample sources carry one.
    fn region_pan(&self) -> f32 {
        match self {
            Source::Sample { region_pan, .. } => *region_pan,
            Source::Synth(_) => 0.0,
        }
    }
}

/// One active voice: a source + post-source DSP + routing.
#[derive(Clone, Debug)]
pub struct Voice {
    /// Engine id (for addressing; voices currently self-release).
    pub id: VoiceId,
    /// Mixer strip this voice routes to.
    pub track: u32,
    source: Source,
    hpf: Option<Biquad>,
    lpf: Option<Biquad>,
    shape: f32,
    crush: Option<f32>,
    /// `gain × vel`, the post-envelope amplitude scalar.
    amp: f32,
    /// Equal-power pan gains (left, right), from `pan` + region pan.
    pan_l: f32,
    pan_r: f32,
    /// Reverb send amount `0..1`.
    room: f32,
    /// Absolute frame at which this voice should release; `None` = ring out.
    release_at: Option<u64>,
    /// Set once released so we don't re-trigger release every sample.
    released: bool,
}

impl Voice {
    /// Build a voice from a resolved source + the event's params at `sample_rate`.
    ///
    /// `note` drives synth frequency / sample pitch; `release_at` is
    /// `start_frame + dur_frames` (absolute), or `None` to ring to natural end.
    pub fn build(
        id: VoiceId,
        track: u32,
        resolved: ResolvedVoice,
        note: Option<f32>,
        params: &VoiceParams,
        release_at: Option<u64>,
        sample_rate: f32,
    ) -> Self {
        // Total pitch offset in semitones: note offset from middle reference +
        // `shift`. `speed` is a separate rate multiply handled by the player.
        let shift = params.shift;
        let source = build_source(resolved, note, shift, params.speed, sample_rate);

        let hpf = params
            .hpf
            .map(|hz| Biquad::highpass(hz, sample_rate));
        let lpf = params
            .lpf
            .map(|hz| Biquad::lowpass(hz, sample_rate));

        // `vel` scales dynamics in addition to selecting the sampled layer
        // upstream; `gain` is the explicit output amplitude.
        let amp = params.gain * params.vel;

        let region_pan = source.region_pan();
        let pan = (params.pan + region_pan * 0.5).clamp(0.0, 1.0);
        let (pan_l, pan_r) = equal_power_pan(pan);

        Voice {
            id,
            track,
            source,
            hpf,
            lpf,
            shape: params.shape,
            crush: params.crush,
            amp,
            pan_l,
            pan_r,
            room: params.room.clamp(0.0, 1.0),
            release_at,
            released: false,
        }
    }

    /// Whether the voice has finished and can be reclaimed.
    pub fn is_done(&self) -> bool {
        self.source.is_done()
    }

    /// Force the voice into its release stage now (transport stop / steal).
    pub fn force_release(&mut self) {
        if !self.released {
            self.source.release();
            self.released = true;
        }
    }

    /// Process one sample at absolute frame `frame`, accumulating its dry signal
    /// into `dry` (the track strip's L/R) and its reverb send into `send`.
    pub fn process_into(&mut self, frame: u64, dry: &mut [f32; 2], send: &mut [f32; 2]) {
        // Self-release at the deadline.
        if let Some(at) = self.release_at {
            if !self.released && frame >= at {
                self.source.release();
                self.released = true;
            }
        }

        let mut s = self.source.next_sample();
        if let Some(hpf) = self.hpf.as_mut() {
            s = hpf.process(s);
        }
        if let Some(lpf) = self.lpf.as_mut() {
            s = lpf.process(s);
        }
        s = effects::shape(s, self.shape);
        if let Some(bits) = self.crush {
            s = effects::crush(s, bits);
        }
        s *= self.amp;

        let l = s * self.pan_l;
        let r = s * self.pan_r;
        dry[0] += l;
        dry[1] += r;
        if self.room > 0.0 {
            send[0] += l * self.room;
            send[1] += r * self.room;
        }
    }

    /// A stealing-priority key: lower = better steal candidate. Releasing voices
    /// rank below sustaining ones, and among them the quietest wins.
    fn steal_score(&self) -> f32 {
        if self.source.is_releasing() {
            // Releasing voices: quietest first (small score). Keep < 1 so any
            // releasing voice beats any non-releasing one.
            self.source.env_level().min(0.999)
        } else {
            // Non-releasing: never preferred over a releasing voice.
            1.0 + self.source.env_level()
        }
    }
}

/// Build the [`Source`] for a resolved voice, applying note → pitch.
fn build_source(
    resolved: ResolvedVoice,
    note: Option<f32>,
    shift: f32,
    speed: f32,
    sample_rate: f32,
) -> Source {
    match resolved {
        ResolvedVoice::Synth(preset) => {
            // Unpitched synth triggers default to middle C.
            let midi = note.unwrap_or(60.0) + shift;
            let freq = midi_to_freq(midi);
            let env = Adsr::new(
                preset.attack,
                preset.decay,
                preset.sustain,
                preset.release,
                sample_rate,
            );
            // `speed` on a synth maps to a frequency multiply (it has no buffer
            // to resample) — keeps pitch+speed coupled like a sampler.
            let freq = freq * speed.max(0.001);
            Source::Synth(SynthVoice::new(preset.waveform, freq, env, sample_rate))
        }
        ResolvedVoice::Sample { sample, region } => {
            build_sample_source(&sample, region, note, shift, speed, sample_rate)
        }
    }
}

/// Build a sample-player source, computing the semitone offset from the region's
/// `pitch_keycenter` and the requested note.
fn build_sample_source(
    sample: &crate::sampler::Sample,
    region: SampleParams,
    note: Option<f32>,
    shift: f32,
    speed: f32,
    sample_rate: f32,
) -> Source {
    // Pitched playback only when a note is given; an unpitched one-shot plays at
    // native pitch (just `shift` + region tune).
    let note_offset = match note {
        Some(n) => n - region.pitch_keycenter as f32,
        None => 0.0,
    };
    let semitones = note_offset + shift + region.tune_semitones;

    let player = SamplePlayer::new(
        sample,
        sample_rate,
        semitones,
        speed,
        region.offset,
        region.loop_spec,
    );
    let env = Adsr::new(
        region.attack,
        region.decay,
        region.sustain,
        region.release,
        sample_rate,
    );
    Source::Sample {
        player,
        env,
        region_gain: region.gain,
        region_pan: region.pan,
    }
}

/// Equal-power pan law: `pan ∈ [0,1]` → (left, right) gains summing in power.
fn equal_power_pan(pan: f32) -> (f32, f32) {
    let p = pan.clamp(0.0, 1.0);
    let angle = p * std::f32::consts::FRAC_PI_2;
    (angle.cos(), angle.sin())
}

/// Fixed-capacity voice pool with the design's voice-stealing policy.
///
/// Slots are `Option<Voice>`; an empty slot is taken first, otherwise the
/// quietest releasing voice is stolen, otherwise the oldest voice. "Oldest" is
/// tracked by an insertion serial so stealing is O(n) over a small fixed pool.
#[derive(Debug)]
pub struct VoicePool {
    slots: Vec<Option<Voice>>,
    /// Per-slot insertion serial (monotonic), to find the oldest voice.
    serials: Vec<u64>,
    next_serial: u64,
}

impl VoicePool {
    /// A pool of `capacity` voices.
    pub fn new(capacity: usize) -> Self {
        VoicePool {
            slots: (0..capacity).map(|_| None).collect(),
            serials: vec![0; capacity],
            next_serial: 1,
        }
    }

    /// Number of currently sounding voices.
    pub fn active(&self) -> usize {
        self.slots.iter().filter(|s| s.is_some()).count()
    }

    /// Insert a voice, stealing per policy if the pool is full.
    pub fn insert(&mut self, voice: Voice) {
        let slot = self.pick_slot();
        self.slots[slot] = Some(voice);
        self.serials[slot] = self.next_serial;
        self.next_serial += 1;
    }

    /// Choose the slot a new voice goes into: first free, else steal.
    fn pick_slot(&self) -> usize {
        // 1. A free slot.
        if let Some(i) = self.slots.iter().position(|s| s.is_none()) {
            return i;
        }
        // 2. The quietest releasing voice; among non-releasing, the oldest.
        // `steal_score` already ranks releasing < non-releasing; break ties on
        // age (smaller serial = older).
        let mut best = 0;
        let mut best_score = f32::INFINITY;
        let mut best_serial = u64::MAX;
        for (i, slot) in self.slots.iter().enumerate() {
            if let Some(v) = slot {
                let score = v.steal_score();
                let serial = self.serials[i];
                if score < best_score || (score == best_score && serial < best_serial) {
                    best = i;
                    best_score = score;
                    best_serial = serial;
                }
            }
        }
        best
    }

    /// Release every sounding voice (transport stop / `StopAll`).
    pub fn release_all(&mut self) {
        for slot in self.slots.iter_mut().flatten() {
            slot.force_release();
        }
    }

    /// Render one sample of every voice into the per-track dry accumulators and
    /// the shared reverb send, reclaiming finished voices.
    ///
    /// `track_dry[i]` is the L/R accumulator for strip `i`; a voice whose `track`
    /// is out of range routes to strip `0` (defensive — the engine sends valid
    /// indices). `send` is the shared reverb send accumulator.
    pub fn process_sample(&mut self, frame: u64, track_dry: &mut [[f32; 2]], send: &mut [f32; 2]) {
        if track_dry.is_empty() {
            return;
        }
        for slot in self.slots.iter_mut() {
            if let Some(voice) = slot {
                let idx = (voice.track as usize).min(track_dry.len() - 1);
                voice.process_into(frame, &mut track_dry[idx], send);
                if voice.is_done() {
                    *slot = None;
                }
            }
        }
    }
}
