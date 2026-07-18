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
use crate::seam::{EnvOverride, VoiceId, VoiceParams};
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

    /// Re-pitch the source to `note` for a **monophonic-legato glide**, carrying
    /// the existing amplitude envelope forward (no re-attack). A synth retunes its
    /// oscillator in place; a sampler rebuilds its player at the new pitch but
    /// keeps its current envelope so there's no amplitude dip between notes. If the
    /// new resolution is a *different* source kind (synth↔sampler — only on an
    /// instrument swap), we rebuild from scratch, which re-attacks.
    fn reglide(
        &mut self,
        resolved: ResolvedVoice,
        note: Option<f32>,
        shift: f32,
        speed: f32,
        sample_rate: f32,
        seed: u64,
        ov: EnvOverride,
    ) {
        match resolved {
            ResolvedVoice::Synth(preset) => {
                if let Source::Synth(s) = self {
                    let midi = note.unwrap_or(60.0) + shift;
                    let freq = midi_to_freq(midi) * speed.max(0.001);
                    s.set_pitch(freq, sample_rate);
                    return;
                }
                *self = build_source(
                    ResolvedVoice::Synth(preset),
                    note,
                    shift,
                    speed,
                    sample_rate,
                    seed,
                    ov,
                );
            }
            ResolvedVoice::Sample { sample, region } => {
                if let Source::Sample {
                    player,
                    region_gain,
                    region_pan,
                    ..
                } = self
                {
                    let note_offset = match note {
                        Some(n) => n - region.pitch_keycenter as f32,
                        None => 0.0,
                    };
                    let semitones = note_offset + shift + region.tune_semitones;
                    *player = SamplePlayer::new(
                        &sample,
                        sample_rate,
                        semitones,
                        speed,
                        region.offset,
                        region.loop_spec,
                    );
                    *region_gain = region.gain;
                    *region_pan = region.pan;
                    return;
                }
                *self = build_sample_source(&sample, region, note, shift, speed, sample_rate, ov);
            }
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
    /// Per-track delay-bus send amount `0..1` (post-pan dry × this → the line).
    delay_mix: f32,
    /// Absolute frame at which this voice should release; `None` = ring out.
    release_at: Option<u64>,
    /// Set once released so we don't re-trigger release every sample.
    released: bool,
    /// Short linear **crossfade** gain `0..1` multiplied over the output, used by
    /// sampler legato: the outgoing note fades to `0`, the incoming note fades from
    /// `0`, so the recorded sample onset is masked and there's no boundary gap.
    /// `1.0` with a zero increment = no fade (the common case).
    fade: f32,
    /// Per-sample change applied to `fade` (`0.0` = steady).
    fade_inc: f32,
    /// When a fade-out reaches `0`, free the voice (the outgoing half of a
    /// crossfade is done once silent, regardless of its envelope).
    fade_kill: bool,
    /// Latched once a `fade_kill` fade has hit zero, so the pool reclaims the voice.
    faded_out: bool,
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
        // The voice id seeds noise generators so repeated hits decorrelate.
        let source = build_source(
            resolved,
            note,
            shift,
            params.speed,
            sample_rate,
            id.0,
            params.env(),
        );

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
        let (pan_l, pan_r) = effects::equal_power_pan(pan);

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
            delay_mix: params.delay_mix.unwrap_or(0.0).clamp(0.0, 1.0),
            release_at,
            released: false,
            fade: 1.0,
            fade_inc: 0.0,
            fade_kill: false,
            faded_out: false,
        }
    }

    /// Whether the voice has finished and can be reclaimed.
    pub fn is_done(&self) -> bool {
        self.source.is_done() || self.faded_out
    }

    /// Whether this voice is a synth source. The renderer uses it to choose the
    /// legato strategy: synths re-pitch in place (truly continuous), samplers
    /// crossfade (re-pitching from the sample start would replay the bow onset).
    pub fn is_synth(&self) -> bool {
        matches!(self.source, Source::Synth(_))
    }

    /// Begin a linear **fade-in** over `frames` (the incoming half of a legato
    /// crossfade): start silent and ramp to unity.
    pub fn fade_in(&mut self, frames: u64) {
        self.fade = 0.0;
        self.fade_inc = 1.0 / frames.max(1) as f32;
        self.fade_kill = false;
    }

    /// Begin a linear **fade-out** over `frames` from the current level, freeing
    /// the voice once silent (the outgoing half of a legato crossfade).
    pub fn fade_out(&mut self, frames: u64) {
        self.fade_inc = -self.fade / frames.max(1) as f32;
        self.fade_kill = true;
    }

    /// Glide this sounding voice to a new note + params for **monophonic legato**:
    /// re-pitch the source without re-attacking the envelope, refresh the
    /// post-source mix params, and re-arm the release deadline so the (re-used)
    /// voice now lives until the new note's boundary. The caller guarantees the
    /// voice is still sounding (so the envelope is mid-sustain and the glide is
    /// seamless).
    ///
    /// This is the renderer's **synth** legato path — a synth tone re-pitches
    /// continuously. Samplers can't reuse a voice this way (restarting the sample
    /// replays its recorded onset), so the renderer crossfades them instead
    /// ([`fade_in`](Self::fade_in) / [`fade_out`](Self::fade_out)).
    pub fn reglide(
        &mut self,
        resolved: ResolvedVoice,
        note: Option<f32>,
        params: &VoiceParams,
        release_at: Option<u64>,
        sample_rate: f32,
    ) {
        self.source.reglide(
            resolved,
            note,
            params.shift,
            params.speed,
            sample_rate,
            self.id.0,
            params.env(),
        );

        self.amp = params.gain * params.vel;
        let region_pan = self.source.region_pan();
        let pan = (params.pan + region_pan * 0.5).clamp(0.0, 1.0);
        let (pan_l, pan_r) = effects::equal_power_pan(pan);
        self.pan_l = pan_l;
        self.pan_r = pan_r;
        self.hpf = params.hpf.map(|hz| Biquad::highpass(hz, sample_rate));
        self.lpf = params.lpf.map(|hz| Biquad::lowpass(hz, sample_rate));
        self.shape = params.shape;
        self.crush = params.crush;
        self.room = params.room.clamp(0.0, 1.0);
        self.delay_mix = params.delay_mix.unwrap_or(0.0).clamp(0.0, 1.0);
        self.release_at = release_at;
        self.released = false;
    }

    /// Process one sample at absolute frame `frame`, accumulating its dry signal
    /// into `dry` (the track strip's L/R), its reverb send into `send`, and its
    /// per-track delay-bus send into `delay_send`.
    pub fn process_into(
        &mut self,
        frame: u64,
        dry: &mut [f32; 2],
        send: &mut [f32; 2],
        delay_send: &mut [f32; 2],
    ) {
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
        s *= self.amp * self.fade;

        // Advance the crossfade ramp (steady once it reaches its endpoint).
        if self.fade_inc != 0.0 {
            self.fade += self.fade_inc;
            if self.fade <= 0.0 {
                self.fade = 0.0;
                self.fade_inc = 0.0;
                if self.fade_kill {
                    self.faded_out = true;
                }
            } else if self.fade >= 1.0 {
                self.fade = 1.0;
                self.fade_inc = 0.0;
            }
        }

        let l = s * self.pan_l;
        let r = s * self.pan_r;
        dry[0] += l;
        dry[1] += r;
        if self.room > 0.0 {
            send[0] += l * self.room;
            send[1] += r * self.room;
        }
        if self.delay_mix > 0.0 {
            delay_send[0] += l * self.delay_mix;
            delay_send[1] += r * self.delay_mix;
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

/// Build the [`Source`] for a resolved voice, applying note → pitch. `seed` (the
/// voice id) decorrelates noise synths across repeated triggers.
fn build_source(
    resolved: ResolvedVoice,
    note: Option<f32>,
    shift: f32,
    speed: f32,
    sample_rate: f32,
    seed: u64,
    ov: EnvOverride,
) -> Source {
    match resolved {
        ResolvedVoice::Synth(preset) => {
            // Unpitched synth triggers default to middle C.
            let midi = note.unwrap_or(60.0) + shift;
            let freq = midi_to_freq(midi);
            // Per-stage override: an unset stage keeps the preset's own value.
            let env = Adsr::new(
                ov.attack.unwrap_or(preset.attack),
                ov.decay.unwrap_or(preset.decay),
                ov.sustain.unwrap_or(preset.sustain),
                ov.release.unwrap_or(preset.release),
                sample_rate,
            );
            // `speed` on a synth maps to a frequency multiply (it has no buffer
            // to resample) — keeps pitch+speed coupled like a sampler.
            let freq = freq * speed.max(0.001);
            Source::Synth(SynthVoice::new(preset.shape, freq, env, sample_rate, seed))
        }
        ResolvedVoice::Sample { sample, region } => {
            build_sample_source(&sample, region, note, shift, speed, sample_rate, ov)
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
    ov: EnvOverride,
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
    // Per-stage override: an unset stage keeps the sampled region's own `ampeg_*`.
    let env = Adsr::new(
        ov.attack.unwrap_or(region.attack),
        ov.decay.unwrap_or(region.decay),
        ov.sustain.unwrap_or(region.sustain),
        ov.release.unwrap_or(region.release),
        sample_rate,
    );
    Source::Sample {
        player,
        env,
        region_gain: region.gain,
        region_pan: region.pan,
    }
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

    /// The sounding voice with this id, if still resident (for monophonic-legato
    /// re-use). Ids are unique per onset, so at most one slot matches; a stolen /
    /// freed voice simply no longer matches and the caller spawns a fresh one.
    pub fn get_mut_by_id(&mut self, id: VoiceId) -> Option<&mut Voice> {
        self.slots.iter_mut().flatten().find(|v| v.id == id)
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

    /// Drop every sounding voice immediately (transport stop / `StopAll`): the pool
    /// goes empty in one frame, so the voice count reads zero and no residual voice
    /// keeps the renderer busy. Paired with an effects-tail flush on stop, this
    /// leaves the DSP truly at rest.
    pub fn clear(&mut self) {
        for slot in self.slots.iter_mut() {
            *slot = None;
        }
    }

    /// Render one sample of every voice into the per-track dry accumulators, the
    /// shared reverb send, and the per-track delay-bus sends, reclaiming finished
    /// voices.
    ///
    /// `track_dry[i]` / `track_delay[i]` are the L/R accumulators for strip `i`; a
    /// voice whose `track` is out of range routes to strip `0` (defensive — the
    /// engine sends valid indices). `send` is the shared reverb send accumulator.
    pub fn process_sample(
        &mut self,
        frame: u64,
        track_dry: &mut [[f32; 2]],
        send: &mut [f32; 2],
        track_delay: &mut [[f32; 2]],
    ) {
        if track_dry.is_empty() {
            return;
        }
        for slot in self.slots.iter_mut() {
            if let Some(voice) = slot {
                let idx = (voice.track as usize).min(track_dry.len() - 1);
                voice.process_into(frame, &mut track_dry[idx], send, &mut track_delay[idx]);
                if voice.is_done() {
                    *slot = None;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::{ResolvedVoice, SynthPreset};
    use crate::seam::VoiceParams;

    const SR: f32 = 48_000.0;

    /// A held synth voice (no release deadline → rings until told otherwise).
    fn synth_voice() -> Voice {
        Voice::build(
            VoiceId(1),
            0,
            ResolvedVoice::Synth(SynthPreset::default()),
            Some(69.0),
            &VoiceParams::default(),
            None,
            SR,
        )
    }

    /// Advance one sample, returning the (left) dry output.
    fn tick(v: &mut Voice) -> f32 {
        let (mut dry, mut send, mut delay) = ([0.0; 2], [0.0; 2], [0.0; 2]);
        v.process_into(0, &mut dry, &mut send, &mut delay);
        dry[0]
    }

    #[test]
    fn fade_out_frees_the_voice_once_silent() {
        let mut v = synth_voice();
        for _ in 0..32 {
            tick(&mut v);
        }
        assert!(!v.is_done(), "a held voice keeps sounding");
        v.fade_out(64);
        for _ in 0..128 {
            tick(&mut v);
        }
        assert!(v.is_done(), "the outgoing crossfade half is reclaimed once it hits zero");
    }

    #[test]
    fn fade_in_opens_from_near_silence() {
        let mut v = synth_voice();
        v.fade_in(256);
        let mut early = 0.0f32;
        for _ in 0..40 {
            early = early.max(tick(&mut v).abs());
        }
        for _ in 0..180 {
            tick(&mut v);
        }
        let mut later = 0.0f32;
        for _ in 0..40 {
            later = later.max(tick(&mut v).abs());
        }
        assert!(later > early, "the incoming crossfade half ramps up (early {early}, later {later})");
    }
}
