//! Pictures in the conversation: what they cost, which ones the model keeps seeing, which models can see.
//!
//! A screenshot is the most expensive thing a tool can hand back — a thousand tokens or more for one
//! image, against a few hundred for a page of text — and it goes out again with *every* later request of
//! the conversation. So the rule here is blunt: the model keeps the latest few pictures and every older
//! one becomes a line saying it existed. The tool call and its result stay, so the conversation is still
//! valid; only the pixels go.
//!
//! Everything here is a pure function of the messages. Reading the file behind a picture, and putting
//! its bytes in `data`, is the host's job.

use crate::message::{Block, Message};

/// How many pictures the model is shown at once; older ones are replaced by a stub.
pub const KEEP_LATEST_IMAGES: usize = 2;

/// The most tokens one picture can cost: the provider shrinks anything larger before reading it.
const MAX_IMAGE_TOKENS: u32 = 1_600;
/// The fewest — even a thumbnail is a few dozen tokens.
const MIN_IMAGE_TOKENS: u32 = 50;

/// What a picture of `width` x `height` pixels costs the model, about: `w * h / 750`, capped where the
/// provider downscales. An estimate for the context meter, not a bill.
pub fn estimate_image_tokens(width: u32, height: u32) -> u32 {
    let raw = (u64::from(width) * u64::from(height)) / 750;
    (raw.min(u64::from(MAX_IMAGE_TOKENS)) as u32).max(MIN_IMAGE_TOKENS)
}

/// Whether the model reads images. Every Claude from the third generation on does; a model this does not
/// recognise is assumed not to, so a tool answers in words rather than sending a block that is refused.
pub fn supports_images(model: &str) -> bool {
    let Some(rest) = model.strip_prefix("claude-") else { return false };
    // `claude-2.1`, `claude-instant-1.2` and the like predate vision; the 3+ families all have it.
    ["3", "haiku", "sonnet", "opus", "fable", "mythos"].iter().any(|p| rest.starts_with(p))
}

/// The tokens every picture in `messages` costs.
pub fn image_tokens(messages: &[Message]) -> u64 {
    messages
        .iter()
        .flat_map(|m| m.blocks.iter())
        .map(|b| match b {
            Block::ToolImage { tokens, .. } => u64::from(*tokens),
            _ => 0,
        })
        .sum()
}

/// The size of a PNG, read from its header. `None` for anything that is not one.
pub fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    if bytes.len() < 24 || bytes[..8] != SIGNATURE || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let read = |at: usize| u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
    Some((read(16), read(20)))
}

/// The size of a JPEG, read from its first start-of-frame marker. `None` for anything that is not one.
pub fn jpeg_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return None;
    }
    let mut at = 2;
    while at + 9 < bytes.len() {
        if bytes[at] != 0xFF {
            return None;
        }
        let marker = bytes[at + 1];
        if marker == 0xFF {
            at += 1; // padding
            continue;
        }
        let length = usize::from(u16::from_be_bytes([bytes[at + 2], bytes[at + 3]]));
        // SOF0..SOF15, less the three markers in that range that are not frames.
        if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
            let height = u32::from(u16::from_be_bytes([bytes[at + 5], bytes[at + 6]]));
            let width = u32::from(u16::from_be_bytes([bytes[at + 7], bytes[at + 8]]));
            return Some((width, height));
        }
        at += 2 + length;
    }
    None
}

/// The size of a PNG or a JPEG, whichever `bytes` is.
pub fn image_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    png_dimensions(bytes).or_else(|| jpeg_dimensions(bytes))
}

/// The one line that stands where a picture was.
fn stub(path: &str, why: &str) -> String {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    format!("[screenshot {name} {why}]")
}

/// Keep the latest `keep` pictures and replace every older one with a line in its tool result. Each
/// result stays where it is, so every `tool_use` still has its answer.
pub fn keep_latest_images(messages: &[Message], keep: usize) -> Vec<Message> {
    let total = messages.iter().flat_map(|m| m.blocks.iter()).filter(|b| matches!(b, Block::ToolImage { .. })).count();
    let drop_first = total.saturating_sub(keep);
    replace_images(messages, |seen, _| seen < drop_first, "was cleared to save room")
}

/// Replace *every* picture with a line, for a model that cannot read them.
pub fn without_images(messages: &[Message]) -> Vec<Message> {
    replace_images(messages, |_, _| true, "is not shown: this model cannot read images")
}

/// Fill in the bytes of every picture that has none, with `load(path)` (base64, or `None` when the file
/// is gone — that picture then becomes a line, so the model is told rather than left guessing).
pub fn load_images(messages: &[Message], load: impl Fn(&str) -> Option<String>) -> Vec<Message> {
    let mut missing: Vec<String> = Vec::new();
    let mut out: Vec<Message> = messages.to_vec();
    for b in out.iter_mut().flat_map(|m| m.blocks.iter_mut()) {
        if let Block::ToolImage { path, data, .. } = b {
            if data.is_none() {
                match load(path) {
                    Some(bytes) => *data = Some(bytes),
                    None => missing.push(path.clone()),
                }
            }
        }
    }
    if missing.is_empty() {
        return out;
    }
    replace_images(&out, |_, p| missing.iter().any(|m| m == p), "is no longer on disk")
}

/// Replace the pictures `replace(index, path)` selects (index counted over all of them, oldest first),
/// moving what each one was into a line of the result it belongs to.
fn replace_images(messages: &[Message], replace: impl Fn(usize, &str) -> bool, why: &str) -> Vec<Message> {
    let mut seen = 0usize;
    let mut out: Vec<Message> = Vec::with_capacity(messages.len());
    for m in messages {
        if !m.blocks.iter().any(|b| matches!(b, Block::ToolImage { .. })) {
            out.push(m.clone());
            continue;
        }
        let mut blocks: Vec<Block> = Vec::with_capacity(m.blocks.len());
        let mut stubs: Vec<(String, String)> = Vec::new();
        for b in &m.blocks {
            match b {
                Block::ToolImage { tool_use_id, path, .. } if replace(seen, path) => {
                    stubs.push((tool_use_id.clone(), stub(path, why)));
                    seen += 1;
                }
                Block::ToolImage { .. } => {
                    seen += 1;
                    blocks.push(b.clone());
                }
                other => blocks.push(other.clone()),
            }
        }
        for (id, line) in stubs {
            note_in_result(&mut blocks, &id, &line);
        }
        out.push(Message { role: m.role, blocks });
    }
    out
}

/// Append `line` to the result of `tool_use_id`; a picture whose result is missing keeps its line as text,
/// so nothing is lost silently.
fn note_in_result(blocks: &mut Vec<Block>, tool_use_id: &str, line: &str) {
    for b in blocks.iter_mut() {
        if let Block::ToolResult { tool_use_id: id, content, .. } = b {
            if id == tool_use_id {
                content.push('\n');
                content.push_str(line);
                return;
            }
        }
    }
    blocks.push(Block::Text { text: line.to_string() });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(id: &str, text: &str) -> Block {
        Block::ToolResult { tool_use_id: id.into(), content: text.into(), is_error: false }
    }

    fn image(id: &str, path: &str) -> Block {
        Block::ToolImage { tool_use_id: id.into(), media_type: "image/png".into(), path: path.into(), tokens: 1000, data: Some("AAAA".into()) }
    }

    /// assistant(tool_use a) · user(result a + image a) · … for each id.
    fn conversation(ids: &[&str]) -> Vec<Message> {
        let mut t = vec![Message::user("look at it")];
        for id in ids {
            t.push(Message::assistant(vec![Block::ToolUse { id: (*id).into(), name: "screenshot_page".into(), input: serde_json::json!({}) }]));
            t.push(Message::tool_results(vec![result(id, "shot taken"), image(id, &format!("C:\\data\\{id}.png"))]));
        }
        t
    }

    fn images_in(messages: &[Message]) -> Vec<String> {
        messages
            .iter()
            .flat_map(|m| m.blocks.iter())
            .filter_map(|b| if let Block::ToolImage { tool_use_id, .. } = b { Some(tool_use_id.clone()) } else { None })
            .collect()
    }

    #[test]
    fn only_the_latest_pictures_stay_and_older_ones_become_a_line() {
        let out = keep_latest_images(&conversation(&["a", "b", "c", "d"]), 2);
        assert_eq!(images_in(&out), ["c", "d"]);
        let Block::ToolResult { content, .. } = &out[2].blocks[0] else { panic!("a result") };
        assert!(content.starts_with("shot taken") && content.contains("[screenshot a.png was cleared"), "{content}");
        assert_eq!(out[2].blocks.len(), 1, "the picture left, the result stayed");
    }

    #[test]
    fn nothing_is_cleared_while_the_pictures_fit() {
        let conv = conversation(&["a", "b"]);
        assert_eq!(keep_latest_images(&conv, 2), conv);
        assert_eq!(keep_latest_images(&conv, 5), conv);
    }

    #[test]
    fn every_tool_call_still_has_its_result_after_clearing() {
        let out = keep_latest_images(&conversation(&["a", "b", "c"]), 1);
        let uses: Vec<_> = out.iter().flat_map(|m| m.tool_uses().map(|(id, _, _)| id.to_string())).collect();
        let answered: Vec<_> = out
            .iter()
            .flat_map(|m| m.blocks.iter())
            .filter_map(|b| if let Block::ToolResult { tool_use_id, .. } = b { Some(tool_use_id.clone()) } else { None })
            .collect();
        assert_eq!(uses, answered);
    }

    #[test]
    fn a_model_that_cannot_see_gets_words_only() {
        let out = without_images(&conversation(&["a", "b"]));
        assert!(images_in(&out).is_empty());
        let Block::ToolResult { content, .. } = &out[2].blocks[0] else { panic!("a result") };
        assert!(content.contains("cannot read images"));
    }

    #[test]
    fn loading_fills_the_bytes_and_a_missing_file_becomes_a_line() {
        let mut conv = conversation(&["a", "b"]);
        for b in conv.iter_mut().flat_map(|m| m.blocks.iter_mut()) {
            if let Block::ToolImage { data, .. } = b {
                *data = None;
            }
        }
        let out = load_images(&conv, |p| if p.ends_with("a.png") { None } else { Some("QUJD".to_string()) });
        assert_eq!(images_in(&out), ["b"]);
        let Block::ToolImage { data, .. } = out[4].blocks.iter().find(|b| matches!(b, Block::ToolImage { .. })).unwrap() else { panic!() };
        assert_eq!(data.as_deref(), Some("QUJD"));
        let Block::ToolResult { content, .. } = &out[2].blocks[0] else { panic!("a result") };
        assert!(content.contains("no longer on disk"));
    }

    #[test]
    fn a_picture_without_its_result_keeps_its_line_as_text() {
        let conv = vec![Message::tool_results(vec![image("x", "x.png")])];
        let out = without_images(&conv);
        assert!(matches!(&out[0].blocks[0], Block::Text { text } if text.contains("x.png")));
    }

    #[test]
    fn pictures_are_counted_by_the_context_meter() {
        assert_eq!(image_tokens(&conversation(&["a", "b", "c"])), 3000);
        assert_eq!(image_tokens(&keep_latest_images(&conversation(&["a", "b", "c"]), 2)), 2000);
    }

    #[test]
    fn a_picture_costs_about_its_area_and_no_more_than_the_provider_reads() {
        assert_eq!(estimate_image_tokens(1280, 800), 1365);
        assert_eq!(estimate_image_tokens(4000, 4000), 1600);
        assert_eq!(estimate_image_tokens(10, 10), 50);
    }

    #[test]
    fn the_claude_models_see_and_the_old_ones_do_not() {
        for m in ["claude-opus-5-5", "claude-sonnet-5-1", "claude-haiku-4-5", "claude-fable-5-1", "claude-3-5-sonnet-20241022"] {
            assert!(supports_images(m), "{m}");
        }
        for m in ["claude-2.1", "claude-instant-1.2", "gpt-5", ""] {
            assert!(!supports_images(m), "{m}");
        }
    }

    #[test]
    fn a_png_header_gives_its_size() {
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, b'I', b'H', b'D', b'R'];
        png.extend_from_slice(&1280u32.to_be_bytes());
        png.extend_from_slice(&720u32.to_be_bytes());
        assert_eq!(png_dimensions(&png), Some((1280, 720)));
        assert_eq!(png_dimensions(b"not a png at all, just text"), None);
        assert_eq!(png_dimensions(&png[..10]), None);
    }

    #[test]
    fn a_jpeg_header_gives_its_size_past_the_segments_before_the_frame() {
        // SOI, an APP0 segment (length 4: two bytes of payload), then SOF0 for 640 x 480.
        let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x4A, 0x46, 0xFF, 0xC0, 0x00, 0x11, 0x08];
        jpeg.extend_from_slice(&480u16.to_be_bytes());
        jpeg.extend_from_slice(&640u16.to_be_bytes());
        jpeg.extend_from_slice(&[3, 1, 0x22, 0, 2, 0x11, 1]);
        assert_eq!(jpeg_dimensions(&jpeg), Some((640, 480)));
        assert_eq!(image_dimensions(&jpeg), Some((640, 480)));
        assert_eq!(jpeg_dimensions(b"GIF89a-not-a-jpeg"), None);
    }
}
