//! System text-to-speech engine.
//!
//! Windows: WinRT `Windows.Media.SpeechSynthesis.SpeechSynthesizer` renders the
//! text to a WAV stream, which we decode to mono `f32`. Other platforms return
//! empty for now (the caller falls back to SAM). mac (`AVSpeechSynthesizer`) and
//! linux (speech-dispatcher) are future work behind the same seam.
//!
//! Every failure degrades to empty rather than panicking — speech is best-effort
//! and the caller has a SAM fallback.

use super::SpeechParams;

/// Render `text` with the OS engine. Returns `(mono f32 samples, sample_rate)`;
/// an empty vec means "unavailable / failed" (the caller falls back to SAM).
#[cfg(windows)]
pub fn synthesize(text: &str, params: &SpeechParams) -> (Vec<f32>, u32) {
    windows_impl::synthesize(text, params).unwrap_or_default()
}

#[cfg(not(windows))]
pub fn synthesize(_text: &str, _params: &SpeechParams) -> (Vec<f32>, u32) {
    (Vec::new(), 0)
}

#[cfg(windows)]
mod windows_impl {
    use super::SpeechParams;
    use windows::core::HSTRING;
    use windows::Media::SpeechSynthesis::SpeechSynthesizer;
    use windows::Storage::Streams::DataReader;
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

    pub fn synthesize(text: &str, params: &SpeechParams) -> Option<(Vec<f32>, u32)> {
        // The blocking worker thread has no COM apartment; init MTA (idempotent —
        // returns S_FALSE if already initialised, which we ignore).
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        run(text, params).ok().flatten()
    }

    fn run(text: &str, params: &SpeechParams) -> windows::core::Result<Option<(Vec<f32>, u32)>> {
        let synth = SpeechSynthesizer::new()?;
        select_voice(&synth, params);

        // Synthesize to a WAV/PCM stream, blocking on the async op.
        let stream = synth
            .SynthesizeTextToStreamAsync(&HSTRING::from(text))?
            .get()?;

        let size = stream.Size()?;
        let input = stream.GetInputStreamAt(0)?;
        let reader = DataReader::CreateDataReader(&input)?;
        reader.LoadAsync(size as u32)?.get()?;
        let mut bytes = vec![0u8; size as usize];
        reader.ReadBytes(&mut bytes)?;

        Ok(wav_bytes_to_mono(&bytes))
    }

    /// Pick a voice by `lang` (prefix match on the voice language) then by `voice`
    /// (case-insensitive substring of the display name). Leaves the default voice
    /// when neither is set or nothing matches.
    fn select_voice(synth: &SpeechSynthesizer, params: &SpeechParams) {
        let want_lang = params.lang.as_deref().map(str::to_ascii_lowercase);
        let want_voice = params.voice.as_deref().map(str::to_ascii_lowercase);
        if want_lang.is_none() && want_voice.is_none() {
            return;
        }
        // `AllVoices()` is an inherent static (returns an `IVectorView`, so it needs
        // the `Foundation_Collections` feature to be enabled).
        let Ok(voices) = SpeechSynthesizer::AllVoices() else {
            return;
        };
        let Ok(count) = voices.Size() else { return };
        for i in 0..count {
            let Ok(v) = voices.GetAt(i) else { continue };
            let lang_ok = match &want_lang {
                Some(l) => v
                    .Language()
                    .map(|s| s.to_string().to_ascii_lowercase().starts_with(l))
                    .unwrap_or(false),
                None => true,
            };
            let name_ok = match &want_voice {
                Some(n) => v
                    .DisplayName()
                    .map(|s| s.to_string().to_ascii_lowercase().contains(n))
                    .unwrap_or(false),
                None => true,
            };
            if lang_ok && name_ok {
                let _ = synth.SetVoice(&v);
                return;
            }
        }
    }

    /// Parse a RIFF/WAV byte buffer (PCM — what the synthesizer emits) into mono
    /// `f32` + its sample rate via `hound`.
    fn wav_bytes_to_mono(bytes: &[u8]) -> Option<(Vec<f32>, u32)> {
        let reader = hound::WavReader::new(std::io::Cursor::new(bytes)).ok()?;
        let spec = reader.spec();
        let channels = spec.channels.max(1) as usize;
        let rate = spec.sample_rate;
        let mono = match spec.sample_format {
            hound::SampleFormat::Float => {
                let s: Vec<f32> = reader.into_samples::<f32>().filter_map(Result::ok).collect();
                downmix(&s, channels)
            }
            hound::SampleFormat::Int => {
                let scale = 1.0 / (1i64 << (spec.bits_per_sample - 1)) as f32;
                let s: Vec<f32> = reader
                    .into_samples::<i32>()
                    .filter_map(Result::ok)
                    .map(|v| v as f32 * scale)
                    .collect();
                downmix(&s, channels)
            }
        };
        Some((mono, rate))
    }

    fn downmix(interleaved: &[f32], channels: usize) -> Vec<f32> {
        if channels <= 1 {
            return interleaved.to_vec();
        }
        interleaved
            .chunks(channels)
            .map(|f| f.iter().sum::<f32>() / channels as f32)
            .collect()
    }
}
