//! The one-call entry points wiring L1 → L2 → emit.

use midly::Smf;

use crate::emit;
use crate::error::Result;
use crate::model::{ImportOptions, Song};
use crate::quantize;
use crate::transcode;

/// Convert raw `.mid` bytes (on-disk or transient) to idiomatic `.merula` source.
pub fn midi_to_merula(bytes: &[u8], opts: &ImportOptions) -> Result<String> {
    Ok(emit::song_to_merula(&midi_to_song(bytes, opts)?, opts))
}

/// Convert an already-parsed [`Smf`] to idiomatic `.merula` source.
pub fn smf_to_merula(smf: &Smf, opts: &ImportOptions) -> Result<String> {
    let mut song = transcode::from_smf(smf, opts)?;
    quantize::quantize_song(&mut song, opts.grid);
    Ok(emit::song_to_merula(&song, opts))
}

/// Transcode + quantise to the neutral [`Song`] model, without emitting text —
/// the seam for callers that want to inspect or post-process before printing.
pub fn midi_to_song(bytes: &[u8], opts: &ImportOptions) -> Result<Song> {
    let mut song = transcode::from_bytes(bytes, opts)?;
    quantize::quantize_song(&mut song, opts.grid);
    Ok(song)
}
