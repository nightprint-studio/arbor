//! Reading a profile file that may or may not be gzip-compressed.
//!
//! `samply` writes `profile.json.gz` by default; a hand-exported one is often plain JSON. The kind
//! is decided by the two magic bytes, never by the extension (a `.json` that is really gzip is
//! common after a rename). Decompression is capped on the **decoded** size, because a small
//! archive can inflate to gigabytes.

use std::io::{BufRead, BufReader, Read};

use flate2::bufread::MultiGzDecoder;

/// Decoded bytes accepted from one profile file.
pub const MAX_DECODED_BYTES: u64 = 512 * 1024 * 1024;

const GZIP_MAGIC: [u8; 2] = [0x1f, 0x8b];

/// Read all of `source`, inflating it first when it starts with the gzip magic. Fails once more
/// than `max_decoded` bytes have come out.
pub fn read_maybe_gzip<R: Read>(source: R, max_decoded: u64) -> Result<Vec<u8>, String> {
    let mut reader = BufReader::new(source);
    let gzipped = reader.fill_buf().map_err(|e| e.to_string())?.starts_with(&GZIP_MAGIC);
    let mut out = Vec::new();
    // `take(max + 1)`: one byte past the cap is how an over-size stream is noticed.
    let read = if gzipped {
        MultiGzDecoder::new(reader).take(max_decoded + 1).read_to_end(&mut out)
    } else {
        reader.take(max_decoded + 1).read_to_end(&mut out)
    };
    read.map_err(|e| if gzipped { format!("the gzip stream is damaged: {e}") } else { e.to_string() })?;
    if out.len() as u64 > max_decoded {
        return Err(format!("the profile is larger than {} MB once decompressed; it is not loaded", max_decoded >> 20));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{write::GzEncoder, Compression};
    use std::io::Write;

    fn gz(data: &[u8]) -> Vec<u8> {
        let mut e = GzEncoder::new(Vec::new(), Compression::fast());
        e.write_all(data).unwrap();
        e.finish().unwrap()
    }

    #[test]
    fn plain_bytes_pass_through() {
        assert_eq!(read_maybe_gzip(&b"{\"a\":1}"[..], 1024).unwrap(), b"{\"a\":1}");
    }

    #[test]
    fn gzip_is_detected_by_magic_and_inflated() {
        let packed = gz(b"{\"threads\":[]}");
        assert_eq!(&packed[..2], &GZIP_MAGIC);
        assert_eq!(read_maybe_gzip(&packed[..], 1024).unwrap(), b"{\"threads\":[]}");
    }

    #[test]
    fn a_bomb_is_stopped_at_the_decoded_cap() {
        let packed = gz(&vec![b' '; 100_000]);
        assert!(packed.len() < 1_000);
        let err = read_maybe_gzip(&packed[..], 10_000).unwrap_err();
        assert!(err.contains("once decompressed"), "{err}");
    }

    #[test]
    fn exactly_at_the_cap_is_accepted() {
        assert_eq!(read_maybe_gzip(&gz(&[b'x'; 64])[..], 64).unwrap().len(), 64);
    }

    #[test]
    fn a_truncated_stream_is_an_error() {
        let mut packed = gz(&vec![b'z'; 50_000]);
        packed.truncate(packed.len() / 2);
        assert!(read_maybe_gzip(&packed[..], 1 << 20).is_err());
    }

    #[test]
    fn empty_input_is_empty_not_an_error() {
        assert!(read_maybe_gzip(&b""[..], 10).unwrap().is_empty());
    }
}
