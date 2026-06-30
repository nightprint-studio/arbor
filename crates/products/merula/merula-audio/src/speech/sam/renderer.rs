//! Renderer — faithful 1:1 port of `discordier/sam-js` (`src/renderer/*.es6`).
//! Turns parsed phoneme tuples into an 8-bit unsigned PCM buffer at 22050 Hz.
//!
//! Integer semantics matter: the original is C-on-6502 arithmetic emulated in JS
//! doubles with `& 0xFF` / `>> n` / `| 0` (truncate toward zero). The Rust port
//! mirrors those exactly with `i32`, masks, and `as` casts (which also truncate
//! toward zero). Out-of-range table reads return 0 rather than panicking — the
//! JS `undefined` only ever surfaces at the terminal frame where the result is
//! discarded.

use super::parse_tables::{PHONEME_PERIOD, PHONEME_QUESTION};
use super::parser::PhonemeTuple;
use super::render_tables::*;

const RISING_INFLECTION: i32 = 255;
const FALLING_INFLECTION: i32 = 1;

// ── SetMouthThroat ───────────────────────────────────────────────────────────

/// Recompute the F1 (mouth) and F2 (throat) formant frequency tables for the
/// vowel/sonorant phonemes (5..30 and 48..54). Returns the three base frequency
/// rows split out of [`FREQUENCY_DATA`].
fn set_mouth_throat(mouth: i32, throat: i32) -> [Vec<i32>; 3] {
    let trans = |factor: i32, initial: i32| -> i32 { (((factor * initial) >> 8) & 0xFF) << 1 };

    let mut freqdata = [vec![0i32; 80], vec![0i32; 80], vec![0i32; 80]];
    for i in 0..80 {
        let v = FREQUENCY_DATA[i];
        freqdata[0][i] = (v & 0xFF) as i32;
        freqdata[1][i] = ((v >> 8) & 0xFF) as i32;
        freqdata[2][i] = ((v >> 16) & 0xFF) as i32;
    }
    for pos in 5..30 {
        freqdata[0][pos] = trans(mouth, freqdata[0][pos]);
        freqdata[1][pos] = trans(throat, freqdata[1][pos]);
    }
    for pos in 48..54 {
        freqdata[0][pos] = trans(mouth, freqdata[0][pos]);
        freqdata[1][pos] = trans(throat, freqdata[1][pos]);
    }
    freqdata
}

// ── CreateFrames ─────────────────────────────────────────────────────────────

struct Frames {
    pitches: Vec<i32>,
    frequency: [Vec<i32>; 3],
    amplitude: [Vec<i32>; 3],
    sampled: Vec<i32>,
}

/// Add a rising (question) or falling (statement) inflection in the 30 frames
/// preceding `end`.
fn add_inflection(inflection: i32, end: i32, pitches: &mut [i32]) {
    let mut pos = if end < 30 { 0 } else { end - 30 };

    // Skip leading 127 markers; `a` holds the first non-127 pitch.
    let mut a;
    loop {
        match pitches.get(pos as usize).copied() {
            Some(127) => pos += 1,
            other => {
                a = other.unwrap_or(0);
                break;
            }
        }
    }

    while pos != end {
        a += inflection;
        if (pos as usize) < pitches.len() {
            pitches[pos as usize] = a & 0xFF;
        }
        loop {
            pos += 1;
            if pos == end || pitches.get(pos as usize).copied() != Some(255) {
                break;
            }
        }
    }
}

/// Expand each phoneme to `length` frames, copying its formant data verbatim.
fn create_frames(pitch: i32, tuples: &[PhonemeTuple], freqdata: &[Vec<i32>; 3]) -> Frames {
    let mut f = Frames {
        pitches: Vec::new(),
        frequency: [Vec::new(), Vec::new(), Vec::new()],
        amplitude: [Vec::new(), Vec::new(), Vec::new()],
        sampled: Vec::new(),
    };

    for &(phoneme, frames, stress) in tuples {
        let phoneme = phoneme as usize;
        let here = f.pitches.len() as i32;
        if phoneme == PHONEME_PERIOD as usize {
            add_inflection(FALLING_INFLECTION, here, &mut f.pitches);
        } else if phoneme == PHONEME_QUESTION as usize {
            add_inflection(RISING_INFLECTION, here, &mut f.pitches);
        }

        let phase1 = STRESS_PITCH[stress as usize];
        let mut n = frames;
        while n > 0 {
            f.frequency[0].push(freqdata[0][phoneme]);
            f.frequency[1].push(freqdata[1][phoneme]);
            f.frequency[2].push(freqdata[2][phoneme]);
            f.amplitude[0].push((AMPL_DATA[phoneme] & 0xFF) as i32);
            f.amplitude[1].push(((AMPL_DATA[phoneme] >> 8) & 0xFF) as i32);
            f.amplitude[2].push(((AMPL_DATA[phoneme] >> 16) & 0xFF) as i32);
            f.sampled.push(SAMPLED_CONSONANT_FLAGS[phoneme]);
            f.pitches.push((pitch + phase1) & 0xFF);
            n -= 1;
        }
    }
    f
}

// ── CreateTransitions ────────────────────────────────────────────────────────

/// Linearly interpolate `change` across `width` frames of `tbl`, starting at
/// `frame`. Reproduces SAM's error-accumulating integer interpolation.
fn interpolate(width: i32, frame: i32, change: i32, tbl: &mut [i32]) {
    if width < 2 {
        return; // matches JS: `while(--pos > 0)` never runs, no writes
    }
    let sign = change < 0;
    let remainder = change.abs() % width;
    let div = change / width;

    let mut error = 0;
    let mut pos = width;
    let mut frame = frame;
    loop {
        pos -= 1;
        if pos <= 0 {
            break;
        }
        let mut val = tbl.get(frame as usize).copied().unwrap_or(0) + div;
        error += remainder;
        if error >= width {
            error -= width;
            if sign {
                val -= 1;
            } else if val != 0 {
                val += 1;
            }
        }
        frame += 1;
        if (frame as usize) < tbl.len() {
            tbl[frame as usize] = val;
        }
    }
}

/// Select the frequency/amplitude row for transition tables 1..=6.
fn table_mut<'a>(
    table: i32,
    frequency: &'a mut [Vec<i32>; 3],
    amplitude: &'a mut [Vec<i32>; 3],
) -> &'a mut Vec<i32> {
    match table {
        1 => &mut frequency[0],
        2 => &mut frequency[1],
        3 => &mut frequency[2],
        4 => &mut amplitude[0],
        5 => &mut amplitude[1],
        6 => &mut amplitude[2],
        _ => unreachable!(),
    }
}

/// Create transitions between phonemes and return the total frame count.
fn create_transitions(f: &mut Frames, tuples: &[PhonemeTuple]) -> i32 {
    if tuples.is_empty() {
        return 0;
    }
    let mut boundary = 0i32;
    for pos in 0..tuples.len().saturating_sub(1) {
        let phoneme = tuples[pos].0 as usize;
        let next_phoneme = tuples[pos + 1].0 as usize;

        let next_rank = BLEND_RANK[next_phoneme];
        let rank = BLEND_RANK[phoneme];

        let (out_blend_frames, in_blend_frames) = if rank == next_rank {
            (OUT_BLEND_LENGTH[phoneme], OUT_BLEND_LENGTH[next_phoneme])
        } else if rank < next_rank {
            (IN_BLEND_LENGTH[next_phoneme], OUT_BLEND_LENGTH[next_phoneme])
        } else {
            (OUT_BLEND_LENGTH[phoneme], IN_BLEND_LENGTH[phoneme])
        };

        boundary += tuples[pos].1;
        let trans_end = boundary + in_blend_frames;
        let trans_start = boundary - out_blend_frames;
        let trans_length = out_blend_frames + in_blend_frames;

        if ((trans_length - 2) & 128) == 0 {
            // Pitch interpolates from the centre of this phoneme to the centre
            // of the next; the rest interpolate over the blend window.
            let cur_width = tuples[pos].1 >> 1;
            let next_width = tuples[pos + 1].1 >> 1;
            let pitch = f.pitches.get((boundary + next_width) as usize).copied().unwrap_or(0)
                - f.pitches.get((boundary - cur_width) as usize).copied().unwrap_or(0);
            interpolate(cur_width + next_width, trans_start, pitch, &mut f.pitches);

            for table in 1..7 {
                let arr = table_mut(table, &mut f.frequency, &mut f.amplitude);
                let end_v = arr.get(trans_end as usize).copied().unwrap_or(0);
                let start_v = arr.get(trans_start as usize).copied().unwrap_or(0);
                interpolate(trans_length, trans_start, end_v - start_v, arr);
            }
        }
    }
    boundary + tuples[tuples.len() - 1].1
}

// ── PrepareFrames ────────────────────────────────────────────────────────────

/// Render the phoneme list into the frame parameters used by [`process_frames`].
/// Returns `(frame_count, frequency, pitches, amplitude, sampled)`.
fn prepare_frames(
    tuples: &[PhonemeTuple],
    pitch: i32,
    mouth: i32,
    throat: i32,
    singmode: bool,
) -> (i32, [Vec<i32>; 3], Vec<i32>, [Vec<i32>; 3], Vec<i32>) {
    let freqdata = set_mouth_throat(mouth, throat);
    let mut f = create_frames(pitch, tuples, &freqdata);
    let t = create_transitions(&mut f, tuples);

    if !singmode {
        // Subtract half of F1 from the pitch to create a pitch contour.
        for i in 0..f.pitches.len() {
            f.pitches[i] -= f.frequency[0][i] >> 1;
        }
    }

    // Rescale amplitude from decibels to a linear scale.
    for i in (0..f.amplitude[0].len()).rev() {
        for row in 0..3 {
            let idx = f.amplitude[row][i];
            f.amplitude[row][i] = *AMPLITUDE_RESCALE.get(idx as usize).unwrap_or(&0);
        }
    }

    (t, f.frequency, f.pitches, f.amplitude, f.sampled)
}

// ── Output buffer ────────────────────────────────────────────────────────────

/// C64-timed output writer (mirrors `output-buffer.es6`).
struct OutputBuffer {
    buffer: Vec<u8>,
    bufferpos: usize,
    old_index: usize,
}

/// Per-source timing table (c64 simulation).
const TIMETABLE: [[i32; 5]; 5] = [
    [162, 167, 167, 127, 128], // formants synth
    [226, 60, 60, 0, 0],       // unvoiced sample 0
    [225, 60, 59, 0, 0],       // unvoiced sample 1
    [200, 0, 0, 54, 55],       // voiced sample 0
    [199, 0, 0, 54, 54],       // voiced sample 1
];

impl OutputBuffer {
    fn new(size: usize) -> Self {
        OutputBuffer { buffer: vec![0u8; size], bufferpos: 0, old_index: 0 }
    }

    /// Write five samples (`ary`) under timing row `index`.
    fn write_ary(&mut self, index: usize, ary: [i32; 5]) {
        self.bufferpos += TIMETABLE[self.old_index][index] as usize;
        self.old_index = index;
        let base = self.bufferpos / 50;
        for (k, &v) in ary.iter().enumerate() {
            if base + k < self.buffer.len() {
                self.buffer[base + k] = v as u8;
            }
        }
    }

    /// Scale a single 4-bit value by 16 and write it five times.
    fn write(&mut self, index: usize, a: i32) {
        let scaled = (a & 15) * 16;
        self.write_ary(index, [scaled; 5]);
    }

    fn into_buffer(self) -> Vec<u8> {
        let end = (self.bufferpos / 50).min(self.buffer.len());
        self.buffer[..end].to_vec()
    }
}

// ── ProcessFrames ────────────────────────────────────────────────────────────

#[inline]
fn sinus(x: i32) -> i32 {
    ((2.0 * std::f64::consts::PI * (x as f64 / 256.0)).sin() * 127.0) as i32
}

/// Emit the eight bits of one sampled-consonant byte.
fn render_sample_bits(
    out: &mut OutputBuffer,
    sample_page: usize,
    off: usize,
    index1: usize,
    value1: i32,
    index0: usize,
    value0: i32,
) {
    let mut sample = *SAMPLE_TABLE.get(sample_page + off).unwrap_or(&0) as i32;
    let mut bit = 8;
    loop {
        if (sample & 128) != 0 {
            out.write(index1, value1);
        } else {
            out.write(index0, value0);
        }
        sample <<= 1;
        bit -= 1;
        if bit == 0 {
            break;
        }
    }
}

/// Render a sampled consonant (voiced interleaved, or unvoiced run).
fn render_sample(out: &mut OutputBuffer, last_sample_offset: i32, consonant_flag: i32, pitch: i32) -> i32 {
    let kind = (consonant_flag & 7) - 1;
    let sample_page = ((kind * 256) & 0xFFFF) as usize;
    let mut off = consonant_flag & 248;

    if off == 0 {
        // Voiced phoneme: Z*, ZH, V*, DH.
        let mut phase1 = (pitch >> 4) ^ 255;
        off = last_sample_offset & 0xFF;
        loop {
            render_sample_bits(out, sample_page, off as usize, 3, 26, 4, 6);
            off += 1;
            off &= 0xFF;
            phase1 += 1;
            if (phase1 & 0xFF) == 0 {
                break;
            }
        }
        return off;
    }

    // Unvoiced.
    off ^= 255;
    let value0 = (SAMPLED_CONSONANT_VALUES0[kind as usize] as i32) & 0xFF;
    loop {
        render_sample_bits(out, sample_page, off as usize, 2, 5, 1, value0);
        off += 1;
        if (off & 0xFF) == 0 {
            break;
        }
    }
    last_sample_offset
}

/// Generate the formant waveforms and sampled consonants into `out`.
#[allow(clippy::too_many_arguments)]
fn process_frames(
    out: &mut OutputBuffer,
    mut frame_count: i32,
    speed: i32,
    frequency: &[Vec<i32>; 3],
    pitches: &[i32],
    amplitude: &[Vec<i32>; 3],
    sampled: &[i32],
) {
    let fr = |c: usize, i: usize| frequency[c].get(i).copied().unwrap_or(0);
    let am = |c: usize, i: usize| amplitude[c].get(i).copied().unwrap_or(0);
    let pit = |i: usize| pitches.get(i).copied().unwrap_or(0);
    let flg = |i: usize| sampled.get(i).copied().unwrap_or(0);

    let mut speedcounter = speed;
    let mut phase1 = 0i32;
    let mut phase2 = 0i32;
    let mut phase3 = 0i32;
    let mut last_sample_offset = 0i32;
    let mut pos = 0usize;
    let mut glottal_pulse = pit(0);
    let mut mem38 = (glottal_pulse as f64 * 0.75) as i32;

    while frame_count != 0 {
        let flags = flg(pos);

        if (flags & 248) != 0 {
            // Unvoiced sampled phoneme: render it and skip ahead two frames.
            last_sample_offset = render_sample(out, last_sample_offset, flags, pit(pos & 0xFF));
            pos += 2;
            frame_count -= 2;
            speedcounter = speed;
        } else {
            // Rectangle + two sines reset on each glottal pulse.
            let mut ary = [0i32; 5];
            let mut p1 = phase1 * 256;
            let mut p2 = phase2 * 256;
            let mut p3 = phase3 * 256;
            for slot in ary.iter_mut() {
                let sp1 = sinus((p1 >> 8) & 0xff);
                let sp2 = sinus((p2 >> 8) & 0xff);
                let rp3 = if ((p3 >> 8) & 0xff) < 129 { -0x70 } else { 0x70 };
                let sin1 = sp1 * (am(0, pos) & 0x0F);
                let sin2 = sp2 * (am(1, pos) & 0x0F);
                let rect = rp3 * (am(2, pos) & 0x0F);
                let mut mux = (sin1 + sin2 + rect) as f64;
                mux /= 32.0;
                mux += 128.0;
                *slot = mux as i32;
                p1 += fr(0, pos) * 256 / 4;
                p2 += fr(1, pos) * 256 / 4;
                p3 += fr(2, pos) * 256 / 4;
            }
            out.write_ary(0, ary);

            speedcounter -= 1;
            if speedcounter == 0 {
                pos += 1;
                frame_count -= 1;
                if frame_count == 0 {
                    return;
                }
                speedcounter = speed;
            }

            glottal_pulse -= 1;
            if glottal_pulse != 0 {
                mem38 -= 1;
                if mem38 != 0 || flags == 0 {
                    phase1 += fr(0, pos);
                    phase2 += fr(1, pos);
                    phase3 += fr(2, pos);
                    continue;
                }
                // Voiced sampled phoneme interleaved with the glottal pulse.
                last_sample_offset = render_sample(out, last_sample_offset, flags, pit(pos & 0xFF));
            }
        }

        glottal_pulse = pit(pos);
        mem38 = (glottal_pulse as f64 * 0.75) as i32;
        phase1 = 0;
        phase2 = 0;
        phase3 = 0;
    }
}

// ── Public entry ─────────────────────────────────────────────────────────────

/// Render parsed phoneme tuples to an 8-bit unsigned PCM buffer (22050 Hz mono).
/// `pitch`/`mouth`/`throat`/`speed` are the SAM knobs; `speed == 0` means the
/// default 72 (matches JS `speed || 72`).
pub fn render(
    tuples: &[PhonemeTuple],
    pitch: i32,
    mouth: i32,
    throat: i32,
    speed: i32,
    singmode: bool,
) -> Vec<u8> {
    let pitch = pitch & 0xFF;
    let mouth = mouth & 0xFF;
    let throat = throat & 0xFF;
    let speed = if speed == 0 { 72 } else { speed } & 0xFF;

    let (t, frequency, pitches, amplitude, sampled) =
        prepare_frames(tuples, pitch, mouth, throat, singmode);

    // Reserve 176.4 (= 22050/125) samples per frame, scaled by speed.
    let total_len: i32 = tuples.iter().map(|t| t.1).sum();
    let size = (176.4 * total_len as f64 * speed as f64) as usize;

    let mut out = OutputBuffer::new(size);
    process_frames(&mut out, t, speed, &frequency, &pitches, &amplitude, &sampled);
    out.into_buffer()
}
