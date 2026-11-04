//! PNG encoding for screenshots, without an image dependency.
//!
//! The encoder writes **stored** (uncompressed) deflate blocks: always valid, no compressor to carry,
//! and the size is predictable — `width × height × 3` bytes plus a few — which is what a size cap needs.
//! The price is a larger file than a real compressor would make, so [`encode_capped`] shrinks the picture
//! (box-averaging by a whole factor) until it fits the cap rather than relying on compression.

const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
/// The largest payload of one stored deflate block.
const STORED_BLOCK: usize = 65_535;

pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

pub fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + u32::from(byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

/// zlib stream of `raw` as stored blocks.
fn zlib_stored(raw: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01];
    let mut blocks = raw.chunks(STORED_BLOCK).peekable();
    if blocks.peek().is_none() {
        out.extend_from_slice(&[1, 0, 0, 0xFF, 0xFF]);
    }
    while let Some(block) = blocks.next() {
        out.push(u8::from(blocks.peek().is_none()));
        let len = block.len() as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(block);
    }
    out.extend_from_slice(&adler32(raw).to_be_bytes());
    out
}

/// An 8-bit RGB picture as a PNG file.
pub fn encode_rgb(width: u32, height: u32, rgb: &[u8]) -> Result<Vec<u8>, String> {
    let row = width as usize * 3;
    if width == 0 || height == 0 || rgb.len() != row * height as usize {
        return Err(format!("a {width}x{height} RGB picture needs {} bytes, got {}", row * height as usize, rgb.len()));
    }
    let mut raw = Vec::with_capacity((row + 1) * height as usize);
    for scanline in rgb.chunks(row) {
        raw.push(0); // filter: none
        raw.extend_from_slice(scanline);
    }
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8-bit, truecolour, deflate, no filter, no interlace
    let mut out = SIGNATURE.to_vec();
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &zlib_stored(&raw));
    chunk(&mut out, b"IEND", &[]);
    Ok(out)
}

/// Pixel byte order of the captured frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Order {
    Rgba,
    Bgra,
}

/// Box-average `rgba` (4 bytes a pixel, alpha dropped) down by the whole `factor`, to RGB.
pub fn downscale_rgb(width: u32, height: u32, data: &[u8], order: Order, factor: u32) -> (u32, u32, Vec<u8>) {
    let factor = factor.max(1);
    let (nw, nh) = ((width / factor).max(1), (height / factor).max(1));
    let (ri, bi) = if order == Order::Rgba { (0, 2) } else { (2, 0) };
    let mut out = Vec::with_capacity(nw as usize * nh as usize * 3);
    for y in 0..nh {
        for x in 0..nw {
            let (mut r, mut g, mut b, mut n) = (0u32, 0u32, 0u32, 0u32);
            for dy in 0..factor {
                for dx in 0..factor {
                    let (sx, sy) = ((x * factor + dx).min(width - 1), (y * factor + dy).min(height - 1));
                    let at = (sy as usize * width as usize + sx as usize) * 4;
                    r += u32::from(data[at + ri]);
                    g += u32::from(data[at + 1]);
                    b += u32::from(data[at + bi]);
                    n += 1;
                }
            }
            out.extend_from_slice(&[(r / n) as u8, (g / n) as u8, (b / n) as u8]);
        }
    }
    (nw, nh, out)
}

/// A finished screenshot.
#[derive(Debug)]
pub struct Shot {
    pub width: u32,
    pub height: u32,
    pub source_width: u32,
    pub source_height: u32,
    pub png: Vec<u8>,
}

/// Encode a captured frame as a PNG at most `max_bytes` long, no wider than `max_width` (0 = no limit),
/// shrinking by whole factors until both hold.
pub fn encode_capped(width: u32, height: u32, data: &[u8], order: Order, max_width: u32, max_bytes: usize) -> Result<Shot, String> {
    if data.len() < width as usize * height as usize * 4 || width == 0 || height == 0 {
        return Err("the captured frame is empty or not 4 bytes a pixel".into());
    }
    let mut factor = if max_width == 0 { 1 } else { width.div_ceil(max_width) };
    loop {
        let (w, h, rgb) = downscale_rgb(width, height, data, order, factor);
        let predicted = (w as usize * 3 + 1) * h as usize + 128;
        if predicted <= max_bytes || (w == 1 && h == 1) {
            let png = encode_rgb(w, h, &rgb)?;
            return Ok(Shot { width: w, height: h, source_width: width, source_height: height, png });
        }
        factor += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksums_match_the_published_vectors() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn a_png_has_the_signature_header_data_and_end_chunks_with_valid_crcs() {
        let png = encode_rgb(2, 1, &[255, 0, 0, 0, 255, 0]).unwrap();
        assert_eq!(&png[..8], &SIGNATURE);
        let mut at = 8;
        let mut kinds = Vec::new();
        while at < png.len() {
            let len = u32::from_be_bytes(png[at..at + 4].try_into().unwrap()) as usize;
            let kind = &png[at + 4..at + 8];
            let crc = u32::from_be_bytes(png[at + 8 + len..at + 12 + len].try_into().unwrap());
            assert_eq!(crc32(&png[at + 4..at + 8 + len]), crc);
            kinds.push(String::from_utf8_lossy(kind).to_string());
            at += 12 + len;
        }
        assert_eq!(kinds, ["IHDR", "IDAT", "IEND"]);
    }

    #[test]
    fn a_picture_larger_than_one_stored_block_is_split_and_flagged_final_once() {
        let rgb = vec![7u8; 200 * 100 * 3];
        let z = zlib_stored(&rgb);
        // 20_000 + 100 filter bytes → one block; force several with a big payload.
        assert_eq!(z[2], 1);
        let big = zlib_stored(&vec![0u8; STORED_BLOCK * 2 + 10]);
        assert_eq!(big[2], 0);
        assert_eq!(big[2 + 5 + STORED_BLOCK], 0);
        assert_eq!(big[2 + 2 * (5 + STORED_BLOCK)], 1);
    }

    #[test]
    fn a_wrong_buffer_size_is_refused() {
        assert!(encode_rgb(2, 2, &[0; 5]).is_err());
        assert!(encode_rgb(0, 2, &[]).is_err());
    }

    #[test]
    fn downscaling_averages_a_block_and_swaps_bgra() {
        // 2x1 BGRA: blue-ish then red-ish → averaged to one pixel, R and B swapped back.
        let data = [100, 0, 0, 255, 0, 0, 200, 255];
        let (w, h, rgb) = downscale_rgb(2, 1, &data, Order::Bgra, 2);
        assert_eq!((w, h), (1, 1));
        assert_eq!(rgb, vec![100, 0, 50]);
    }

    #[test]
    fn the_cap_shrinks_the_picture_until_it_fits() {
        let data = vec![9u8; 400 * 300 * 4];
        let shot = encode_capped(400, 300, &data, Order::Rgba, 0, 100_000).unwrap();
        assert!(shot.png.len() <= 100_000 + 200, "{}", shot.png.len());
        assert!(shot.width < 400);
        assert_eq!((shot.source_width, shot.source_height), (400, 300));
        let narrow = encode_capped(400, 300, &data, Order::Rgba, 100, usize::MAX).unwrap();
        assert!(narrow.width <= 100);
    }
}
