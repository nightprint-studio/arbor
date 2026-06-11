//! General MIDI pack: convert a downloaded SoundFont (`.sf2`) into grove's
//! wav + SFZ form at **install time**.
//!
//! `rustysynth` is used here purely as a *parser* — we never run its
//! synthesizer. For every GM preset we walk its instrument regions, slice each
//! referenced sample out of the SoundFont's PCM into a `.wav`, and emit an
//! `.sfz` mapping those samples (key/velocity ranges, root key, tuning, loop,
//! amp envelope) using exactly the opcodes the engine's SFZ loader already
//! understands. The result is a normal `kind=sfz` registry — the audio engine
//! stays sample/SFZ-based, no soundfont code on the RT path.
//!
//! Output layout under the pack dir:
//! - `samples/sN.wav`        — extracted mono PCM slices (deduped by bounds)
//! - `instruments/<name>.sfz`— one per GM program (`gm_acoustic_grand_piano`, …)
//! - `registry.toml`         — `gm_*` → its `.sfz`

use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::Path;

use rustysynth::{LoopMode, SoundFont};
use tauri::AppHandle;

use super::download::{emit_phase, is_cancelled};
use super::Pack;

/// The General MIDI SoundFont downloaded for this pack. A redistributable
/// FluidR3_GM mirror; `rustysynth` needs an **uncompressed** `.sf2` (not `.sf3`).
///
/// NOTE: verify/override this URL + its licence — network resources can't be
/// checked here; swapping it is a one-line change.
pub(super) const SF2_URL: &str = "https://musical-artifacts.com/artifacts/738/FluidR3_GM.sf2";

/// Convert the downloaded `.sf2` at `sf2_path` into wav + SFZ under `dir`.
/// Returns `(bytes_written, instrument_count, registry_rel)`.
pub(super) fn convert(
    app: &AppHandle,
    dir: &Path,
    sf2_path: &Path,
    pack: &Pack,
    job_id: &str,
) -> Result<(u64, usize, String), String> {
    let mut file = std::fs::File::open(sf2_path).map_err(|e| format!("open sf2: {e}"))?;
    let sf = SoundFont::new(&mut file).map_err(|e| format!("parse sf2: {e}"))?;

    let wave = sf.get_wave_data();
    let headers = sf.get_sample_headers();
    let instruments = sf.get_instruments();
    let presets = sf.get_presets();

    let samples_dir = dir.join("samples");
    let inst_dir = dir.join("instruments");
    std::fs::create_dir_all(&samples_dir).map_err(|e| format!("mkdir samples: {e}"))?;
    std::fs::create_dir_all(&inst_dir).map_err(|e| format!("mkdir instruments: {e}"))?;

    // Stable order: by (bank, program) so `sN.wav` indices are reproducible.
    let mut order: Vec<usize> = (0..presets.len()).collect();
    order.sort_by_key(|&i| (presets[i].get_bank_number(), presets[i].get_patch_number()));

    let mut wavs: HashMap<(i32, i32), String> = HashMap::new();
    let mut seen_names: HashSet<String> = HashSet::new();
    let mut registry = String::from("# Auto-generated General MIDI registry (grove).\n\n");
    let mut total_bytes: u64 = 0;
    let mut count = 0usize;
    let total = order.len().max(1) as u64;

    for (idx, &pi) in order.iter().enumerate() {
        if is_cancelled(app, job_id) {
            return Err("cancelled".to_string());
        }
        let preset = &presets[pi];
        let Some(reg_name) = registry_name(preset.get_bank_number(), preset.get_patch_number())
        else {
            continue; // a bank we don't map (variation banks, etc.)
        };
        if !seen_names.insert(reg_name.clone()) {
            continue; // first preset wins a given GM slot
        }

        let mut sfz = String::from("// Auto-generated from a General MIDI SoundFont.\n");
        let mut regions = 0usize;

        for pr in preset.get_regions() {
            let Some(inst) = instruments.get(pr.get_instrument_id()) else { continue };
            for ir in inst.get_regions() {
                // Intersect the preset-region and instrument-region ranges.
                let lokey = ir.get_key_range_start().max(pr.get_key_range_start());
                let hikey = ir.get_key_range_end().min(pr.get_key_range_end());
                let lovel = ir.get_velocity_range_start().max(pr.get_velocity_range_start());
                let hivel = ir.get_velocity_range_end().min(pr.get_velocity_range_end());
                if lokey > hikey || lovel > hivel {
                    continue;
                }

                let start = ir.get_sample_start();
                let end = ir.get_sample_end();
                if start < 0 || end <= start || end as usize > wave.len() {
                    continue;
                }
                let sid = ir.get_sample_id();
                let rate = headers.get(sid).map(|h| h.get_sample_rate()).unwrap_or(44_100).max(1) as u32;

                // Extract the slice to a wav once (dedup by resolved bounds).
                let wav = match wavs.get(&(start, end)) {
                    Some(name) => name.clone(),
                    None => {
                        let name = format!("s{}.wav", wavs.len());
                        let bytes = write_wav_i16(
                            &samples_dir.join(&name),
                            &wave[start as usize..end as usize],
                            rate,
                        )
                        .map_err(|e| format!("write {name}: {e}"))?;
                        total_bytes += bytes;
                        wavs.insert((start, end), name.clone());
                        name
                    }
                };

                let root = if ir.get_root_key() >= 0 {
                    ir.get_root_key()
                } else {
                    headers.get(sid).map(|h| h.get_original_pitch()).unwrap_or(60)
                }
                .clamp(0, 127);
                let correction = headers.get(sid).map(|h| h.get_pitch_correction()).unwrap_or(0);
                let tune = (ir.get_fine_tune() + correction).clamp(-100, 100);

                let loop_mode = match ir.get_sample_modes() {
                    LoopMode::NoLoop => "no_loop",
                    LoopMode::Continuous => "loop_continuous",
                    LoopMode::LoopUntilNoteOff => "loop_sustain",
                };
                let loop_start = (ir.get_sample_start_loop() - start).max(0);
                let loop_end = (ir.get_sample_end_loop() - start).max(0);

                // Envelope: times are seconds; sustain is dB of attenuation → level %.
                let attack = ir.get_attack_volume_envelope().max(0.0);
                let decay = ir.get_decay_volume_envelope().max(0.0);
                let release = ir.get_release_volume_envelope().max(0.0);
                let sustain_pct =
                    (10f32.powf(-ir.get_sustain_volume_envelope() / 20.0).clamp(0.0, 1.0)) * 100.0;

                sfz.push_str("<region>\n");
                sfz.push_str(&format!("sample=../samples/{wav}\n"));
                sfz.push_str(&format!("lokey={lokey} hikey={hikey} lovel={lovel} hivel={hivel}\n"));
                sfz.push_str(&format!(
                    "pitch_keycenter={root} transpose={} tune={tune}\n",
                    ir.get_coarse_tune()
                ));
                sfz.push_str(&format!(
                    "loop_mode={loop_mode} loop_start={loop_start} loop_end={loop_end}\n"
                ));
                sfz.push_str(&format!(
                    "ampeg_attack={attack:.4} ampeg_decay={decay:.4} ampeg_sustain={sustain_pct:.2} ampeg_release={release:.4}\n\n"
                ));
                regions += 1;
            }
        }

        if regions == 0 {
            continue;
        }
        let sfz_name = format!("{reg_name}.sfz");
        std::fs::write(inst_dir.join(&sfz_name), &sfz)
            .map_err(|e| format!("write {sfz_name}: {e}"))?;
        registry.push_str(&format!(
            "[\"{reg_name}\"]\nkind = \"sfz\"\nfile = \"instruments/{sfz_name}\"\n\n"
        ));
        count += 1;
        emit_phase(app, pack, job_id, "extracting", idx as u64 + 1, total);
    }

    std::fs::write(dir.join("registry.toml"), registry)
        .map_err(|e| format!("write registry: {e}"))?;
    Ok((total_bytes, count, "registry.toml".to_string()))
}

/// The registry name for a (bank, program): GM melodic programs on bank 0 map to
/// their standard `gm_<name>` (matching Strudel); the bank-128 percussion kit
/// becomes `gm_drums`. Other banks (variations) are skipped.
fn registry_name(bank: i32, program: i32) -> Option<String> {
    match bank {
        0 => GM_PROGRAM_NAMES.get(program as usize).map(|n| format!("gm_{n}")),
        128 => Some("gm_drums".to_string()),
        _ => None,
    }
}

/// Write mono 16-bit PCM as a canonical RIFF/WAVE file (a tiny self-contained
/// writer, so this module needs no extra WAV dependency). Returns bytes written.
fn write_wav_i16(path: &Path, samples: &[i16], sample_rate: u32) -> std::io::Result<u64> {
    let data_len = (samples.len() * 2) as u32;
    let mut buf = Vec::with_capacity(44 + data_len as usize);
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&(36 + data_len).to_le_bytes());
    buf.extend_from_slice(b"WAVE");
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes()); // PCM fmt chunk size
    buf.extend_from_slice(&1u16.to_le_bytes()); // format = PCM
    buf.extend_from_slice(&1u16.to_le_bytes()); // channels = mono
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&(sample_rate * 2).to_le_bytes()); // byte rate
    buf.extend_from_slice(&2u16.to_le_bytes()); // block align
    buf.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_len.to_le_bytes());
    for &s in samples {
        buf.extend_from_slice(&s.to_le_bytes());
    }
    let mut f = std::fs::File::create(path)?;
    f.write_all(&buf)?;
    Ok(buf.len() as u64)
}

/// The 128 General MIDI Level 1 program names (program 0–127), snake_cased to
/// match Strudel's `gm_*` instrument vocabulary.
const GM_PROGRAM_NAMES: [&str; 128] = [
    "acoustic_grand_piano", "bright_acoustic_piano", "electric_grand_piano", "honky_tonk_piano",
    "electric_piano_1", "electric_piano_2", "harpsichord", "clavinet",
    "celesta", "glockenspiel", "music_box", "vibraphone",
    "marimba", "xylophone", "tubular_bells", "dulcimer",
    "drawbar_organ", "percussive_organ", "rock_organ", "church_organ",
    "reed_organ", "accordion", "harmonica", "tango_accordion",
    "acoustic_guitar_nylon", "acoustic_guitar_steel", "electric_guitar_jazz", "electric_guitar_clean",
    "electric_guitar_muted", "overdriven_guitar", "distortion_guitar", "guitar_harmonics",
    "acoustic_bass", "electric_bass_finger", "electric_bass_pick", "fretless_bass",
    "slap_bass_1", "slap_bass_2", "synth_bass_1", "synth_bass_2",
    "violin", "viola", "cello", "contrabass",
    "tremolo_strings", "pizzicato_strings", "orchestral_harp", "timpani",
    "string_ensemble_1", "string_ensemble_2", "synth_strings_1", "synth_strings_2",
    "choir_aahs", "voice_oohs", "synth_voice", "orchestra_hit",
    "trumpet", "trombone", "tuba", "muted_trumpet",
    "french_horn", "brass_section", "synth_brass_1", "synth_brass_2",
    "soprano_sax", "alto_sax", "tenor_sax", "baritone_sax",
    "oboe", "english_horn", "bassoon", "clarinet",
    "piccolo", "flute", "recorder", "pan_flute",
    "blown_bottle", "shakuhachi", "whistle", "ocarina",
    "lead_1_square", "lead_2_sawtooth", "lead_3_calliope", "lead_4_chiff",
    "lead_5_charang", "lead_6_voice", "lead_7_fifths", "lead_8_bass_and_lead",
    "pad_1_new_age", "pad_2_warm", "pad_3_polysynth", "pad_4_choir",
    "pad_5_bowed", "pad_6_metallic", "pad_7_halo", "pad_8_sweep",
    "fx_1_rain", "fx_2_soundtrack", "fx_3_crystal", "fx_4_atmosphere",
    "fx_5_brightness", "fx_6_goblins", "fx_7_echoes", "fx_8_scifi",
    "sitar", "banjo", "shamisen", "koto",
    "kalimba", "bagpipe", "fiddle", "shanai",
    "tinkle_bell", "agogo", "steel_drums", "woodblock",
    "taiko_drum", "melodic_tom", "synth_drum", "reverse_cymbal",
    "guitar_fret_noise", "breath_noise", "seashore", "bird_tweet",
    "telephone_ring", "helicopter", "applause", "gunshot",
];
