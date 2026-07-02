//! A minimal 16-bit-PCM WAV writer, shared by the mic ([`super::audio`]) and
//! system-audio ([`super::sysaudio`]) capturers. Avoids a `hound` dependency: the
//! 44-byte header is written as a placeholder and patched on [`WavWriter::finalize`]
//! once the total sample count is known.

use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// Clamp a normalized `f32` sample to the 16-bit PCM range.
pub fn f32_to_i16(f: f32) -> i16 {
    (f.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
}

/// Parse a WAV header written by [`WavWriter`] → `(sample_rate, channels, data_bytes)`.
/// Used for A/V-sync diagnostics (a track whose duration ≠ the video's points at a
/// rate/channel mismatch). `None` if the file is too short / unreadable.
pub fn read_wav_info(path: &Path) -> Option<(u32, u16, u32)> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let channels = u16::from_le_bytes([bytes[22], bytes[23]]);
    let sample_rate = u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]);
    let data_bytes = u32::from_le_bytes([bytes[40], bytes[41], bytes[42], bytes[43]]);
    Some((sample_rate, channels, data_bytes))
}

/// Streaming 16-bit-PCM WAV writer. Interleaved samples are appended with
/// [`write_sample`](Self::write_sample); the channel count / sample rate given at
/// [`create`](Self::create) are baked into the header on [`finalize`](Self::finalize).
///
/// **Buffered** (`BufWriter`): a raw `File::write_all` per 2-byte sample is one syscall
/// each — millions over a long recording, slow enough that the system-audio drain loop
/// falls behind and WASAPI drops frames (a shorter, sped-up track). Batching the writes
/// keeps the drain fast.
pub struct WavWriter {
    file: BufWriter<File>,
    data_bytes: u32,
    channels: u16,
    sample_rate: u32,
}

impl WavWriter {
    pub fn create(path: PathBuf, channels: u16, sample_rate: u32) -> std::io::Result<Self> {
        let mut file = BufWriter::with_capacity(64 * 1024, File::create(path)?);
        // 44-byte placeholder header, patched in `finalize`.
        file.write_all(&[0u8; 44])?;
        Ok(WavWriter { file, data_bytes: 0, channels, sample_rate })
    }

    pub fn write_sample(&mut self, s: i16) -> std::io::Result<()> {
        self.file.write_all(&s.to_le_bytes())?;
        self.data_bytes += 2;
        Ok(())
    }

    pub fn finalize(mut self) -> std::io::Result<()> {
        let bits = 16u16;
        let block_align = self.channels * (bits / 8);
        let byte_rate = self.sample_rate * block_align as u32;
        let mut h = Vec::with_capacity(44);
        h.extend_from_slice(b"RIFF");
        h.extend_from_slice(&(36 + self.data_bytes).to_le_bytes());
        h.extend_from_slice(b"WAVE");
        h.extend_from_slice(b"fmt ");
        h.extend_from_slice(&16u32.to_le_bytes());
        h.extend_from_slice(&1u16.to_le_bytes()); // PCM
        h.extend_from_slice(&self.channels.to_le_bytes());
        h.extend_from_slice(&self.sample_rate.to_le_bytes());
        h.extend_from_slice(&byte_rate.to_le_bytes());
        h.extend_from_slice(&block_align.to_le_bytes());
        h.extend_from_slice(&bits.to_le_bytes());
        h.extend_from_slice(b"data");
        h.extend_from_slice(&self.data_bytes.to_le_bytes());
        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(&h)?;
        self.file.flush()
    }
}
