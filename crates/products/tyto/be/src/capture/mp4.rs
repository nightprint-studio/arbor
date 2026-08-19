//! Just enough MP4 to answer "how long is this file", by reading the `moov/mvhd`
//! box header.
//!
//! The library shows a duration for every capture, and a frame sequence gets its
//! own from the manifest it writes. A video had nothing: the alternative to this is
//! spawning `ffprobe` once per file on **every** library refresh — a process per
//! recording to read twelve bytes — or writing a sidecar, which litters the user's
//! output folder with files they didn't ask for and doesn't help for a video Tyto
//! didn't produce. Parsing the header costs one open and a handful of seeks, needs
//! no dependency, and works on any mp4 that happens to be in the folder.
//!
//! Deliberately partial: it walks top-level boxes for `moov`, then its children for
//! `mvhd`, and reads the timescale and duration. Everything else in the format is
//! skipped by size. Anything unexpected returns `None` — a missing duration is a
//! library row without a badge, never a failed scan.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// The movie's duration in milliseconds, or `None` when the file isn't an MP4 the
/// header can be read from (truncated, fragmented with an unknown duration, or not
/// an MP4 at all).
pub fn duration_ms(path: &Path) -> Option<u64> {
    let mut f = File::open(path).ok()?;
    let end = f.seek(SeekFrom::End(0)).ok()?;
    let (moov_start, moov_end) = find_box(&mut f, 0, end, b"moov")?;
    let (mvhd_start, mvhd_end) = find_box(&mut f, moov_start, moov_end, b"mvhd")?;
    read_mvhd(&mut f, mvhd_start, mvhd_end)
}

/// Scan the sibling boxes in `[pos, end)` for one of type `kind`, returning its
/// **payload** range. Boxes are `[u32 size][4-byte type][payload]`, with `size == 1`
/// meaning a 64-bit size follows the type and `size == 0` meaning "to the end".
///
/// A box whose size doesn't fit inside its parent, or is smaller than its own
/// header, ends the walk: a malformed file must not turn into an infinite loop over
/// a zero-length box.
fn find_box<R: Read + Seek>(r: &mut R, mut pos: u64, end: u64, kind: &[u8; 4]) -> Option<(u64, u64)> {
    while pos.checked_add(8)? <= end {
        r.seek(SeekFrom::Start(pos)).ok()?;
        let mut header = [0u8; 8];
        r.read_exact(&mut header).ok()?;
        let declared = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as u64;
        let (size, header_len) = match declared {
            1 => {
                let mut large = [0u8; 8];
                r.read_exact(&mut large).ok()?;
                (u64::from_be_bytes(large), 16u64)
            }
            0 => (end - pos, 8),
            n => (n, 8),
        };
        if size < header_len || pos.checked_add(size)? > end {
            return None;
        }
        if &header[4..8] == kind {
            return Some((pos + header_len, pos + size));
        }
        pos += size;
    }
    None
}

/// Read `timescale` and `duration` out of an `mvhd` payload and turn them into
/// milliseconds.
///
/// Layout after the 4-byte version+flags word: version 0 has two 32-bit times then
/// `timescale` (u32) and `duration` (u32); version 1 has two 64-bit times then
/// `timescale` (u32) and `duration` (u64).
fn read_mvhd<R: Read + Seek>(r: &mut R, start: u64, end: u64) -> Option<u64> {
    r.seek(SeekFrom::Start(start)).ok()?;
    let mut version_flags = [0u8; 4];
    r.read_exact(&mut version_flags).ok()?;
    let wide = version_flags[0] == 1;
    let times_len = if wide { 16u64 } else { 8 };
    let duration_len = if wide { 8u64 } else { 4 };
    if start + 4 + times_len + 4 + duration_len > end {
        return None; // truncated box — better no duration than a number off the end
    }

    r.seek(SeekFrom::Current(times_len as i64)).ok()?;
    let mut scale = [0u8; 4];
    r.read_exact(&mut scale).ok()?;
    let timescale = u32::from_be_bytes(scale) as u64;
    if timescale == 0 {
        return None;
    }

    let duration = if wide {
        let mut d = [0u8; 8];
        r.read_exact(&mut d).ok()?;
        u64::from_be_bytes(d)
    } else {
        let mut d = [0u8; 4];
        r.read_exact(&mut d).ok()?;
        u32::from_be_bytes(d) as u64
    };
    // All-ones is the format's "unknown", which a fragmented file writes.
    let unknown = if wide { u64::MAX } else { u32::MAX as u64 };
    if duration == unknown {
        return None;
    }
    Some(duration.saturating_mul(1000) / timescale)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// `[u32 size][type][payload]`.
    fn boxed(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let size = (8 + payload.len()) as u32;
        let mut out = size.to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(payload);
        out
    }

    /// A version-0 `mvhd` payload with the given timescale and duration.
    fn mvhd_v0(timescale: u32, duration: u32) -> Vec<u8> {
        let mut p = vec![0u8, 0, 0, 0]; // version 0 + flags
        p.extend_from_slice(&0u32.to_be_bytes()); // creation
        p.extend_from_slice(&0u32.to_be_bytes()); // modification
        p.extend_from_slice(&timescale.to_be_bytes());
        p.extend_from_slice(&duration.to_be_bytes());
        p
    }

    /// A version-1 `mvhd` payload (64-bit times and duration).
    fn mvhd_v1(timescale: u32, duration: u64) -> Vec<u8> {
        let mut p = vec![1u8, 0, 0, 0]; // version 1 + flags
        p.extend_from_slice(&0u64.to_be_bytes());
        p.extend_from_slice(&0u64.to_be_bytes());
        p.extend_from_slice(&timescale.to_be_bytes());
        p.extend_from_slice(&duration.to_be_bytes());
        p
    }

    /// Read a duration out of an in-memory file, the way [`duration_ms`] does.
    fn read(bytes: &[u8]) -> Option<u64> {
        let mut c = Cursor::new(bytes.to_vec());
        let end = bytes.len() as u64;
        let (ms, me) = find_box(&mut c, 0, end, b"moov")?;
        let (vs, ve) = find_box(&mut c, ms, me, b"mvhd")?;
        read_mvhd(&mut c, vs, ve)
    }

    #[test]
    fn reads_a_version_0_movie_header() {
        // 1000 ticks/s, 12_500 ticks → 12.5 s.
        let file = [boxed(b"ftyp", b"isom"), boxed(b"moov", &boxed(b"mvhd", &mvhd_v0(1000, 12_500)))].concat();
        assert_eq!(read(&file), Some(12_500));
    }

    #[test]
    fn reads_a_version_1_movie_header() {
        let file = boxed(b"moov", &boxed(b"mvhd", &mvhd_v1(90_000, 90_000 * 7)));
        assert_eq!(read(&file), Some(7_000));
    }

    #[test]
    fn skips_boxes_before_the_movie_header() {
        // ffmpeg writes `moov` LAST (no faststart), after a large `mdat` — the walk
        // has to step over it by size rather than expect it up front.
        let mdat = boxed(b"mdat", &vec![0u8; 4096]);
        let file = [boxed(b"ftyp", b"isom"), mdat, boxed(b"moov", &boxed(b"mvhd", &mvhd_v0(600, 1200)))].concat();
        assert_eq!(read(&file), Some(2_000));
    }

    #[test]
    fn finds_the_header_past_a_sibling_inside_moov() {
        let moov = boxed(b"moov", &[boxed(b"udta", b"whatever"), boxed(b"mvhd", &mvhd_v0(1000, 500))].concat());
        assert_eq!(read(&moov), Some(500));
    }

    #[test]
    fn an_unknown_duration_is_not_a_number() {
        let file = boxed(b"moov", &boxed(b"mvhd", &mvhd_v0(1000, u32::MAX)));
        assert_eq!(read(&file), None, "all-ones means the file doesn't know");
    }

    #[test]
    fn a_zero_timescale_never_divides_by_zero() {
        let file = boxed(b"moov", &boxed(b"mvhd", &mvhd_v0(0, 1000)));
        assert_eq!(read(&file), None);
    }

    #[test]
    fn a_truncated_header_yields_nothing() {
        let file = boxed(b"moov", &boxed(b"mvhd", &[0u8, 0, 0, 0, 1, 2]));
        assert_eq!(read(&file), None);
    }

    #[test]
    fn a_zero_length_box_ends_the_walk_instead_of_looping() {
        // size 4 is smaller than the 8-byte header — a malformed file must not spin.
        let mut file = 4u32.to_be_bytes().to_vec();
        file.extend_from_slice(b"junk");
        file.extend_from_slice(&boxed(b"moov", &boxed(b"mvhd", &mvhd_v0(1000, 1000))));
        assert_eq!(read(&file), None, "the walk stops at the malformed box");
    }

    #[test]
    fn a_file_that_isnt_an_mp4_yields_nothing() {
        assert_eq!(read(b"this is a png, honest"), None);
        assert_eq!(read(b""), None);
    }
}
