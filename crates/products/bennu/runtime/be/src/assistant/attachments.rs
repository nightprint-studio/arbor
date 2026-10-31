//! Files the user attaches to a message.
//!
//! The user choosing a file is the consent: unlike a file the model asks for, an attachment is not checked
//! against the access policy — it can be anywhere. What stands in for the policy is size. Images are kept
//! (`images/<conversation>/`) and travel as image blocks of the message; text and code travel inline, in a
//! delimited block that names the file; anything else is refused *before* the run starts, with the reason,
//! so a message is never half-sent.
//!
//! | what | to the model | in the thread |
//! |---|---|---|
//! | PNG, JPEG, GIF, WebP | an image block (counted by the context meter, shown only while among the latest few) | a thumbnail |
//! | SVG | its source, as text — the API reads no SVG | a thumbnail |
//! | text, code, JSON, XML… | `<attached_file name="…">` + the text, decoded | a chip |
//! | anything binary | refused | — |
//!
//! A file that is not UTF-8 is decoded with the project's encoding (the same label the editor reads its
//! sources with), so a legacy Cp1252 source arrives with its accents.

use std::path::Path;

use base64::Engine;
use bennu_assistant::prelude::{estimate_image_tokens, image_dimensions, Block};
use serde::Deserialize;
use serde_json::{json, Value};

use super::tools::save_picture;

/// How many files one message carries.
pub(super) const MAX_FILES: usize = 8;
/// How many of them may be pictures: the number the model keeps seeing (`KEEP_LATEST_ATTACHED`).
const MAX_IMAGES: usize = bennu_assistant::prelude::KEEP_LATEST_ATTACHED;
/// A picture over this is refused: the provider takes about 5 MB of base64, which is 3.75 MB of file.
const MAX_IMAGE_BYTES: usize = 3_500_000;
/// The largest text file attached whole (about fifty thousand tokens), and the most text one message carries.
const MAX_TEXT_BYTES: usize = 200_000;
const MAX_TOTAL_TEXT_BYTES: usize = 500_000;
/// Extensions that are never text, refused without reading further.
const BINARY_EXTS: &[&str] = &[
    "pdf", "zip", "jar", "war", "ear", "class", "exe", "dll", "so", "dylib", "o", "a", "gz", "tgz", "tar", "7z", "rar", "bin", "docx", "xlsx",
    "pptx", "odt", "ods", "mp3", "mp4", "mov", "avi", "mkv", "wav", "ogg", "ico", "bmp", "tif", "tiff", "psd", "ttf", "otf", "woff", "woff2",
    "db", "sqlite", "wasm", "dat",
];

/// One attachment as the panel sends it: a file to read from disk, or the bytes of one pasted from the
/// clipboard (base64), which has no path.
#[derive(Debug, Deserialize)]
pub struct AttachmentArg {
    pub name: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub data: Option<String>,
}

/// What attaching turned into.
#[derive(Default)]
pub(super) struct Prepared {
    /// The pictures, as blocks with no tool call (empty `tool_use_id`); the bytes are loaded when a request goes out.
    pub images: Vec<Block>,
    /// The attached text files, delimited, ending in a blank line — ahead of what the user typed.
    pub text: String,
    /// The chips and thumbnails of the thread, one per file.
    pub items: Vec<Value>,
}

impl Prepared {
    pub(super) fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// What stands as the user's words when they typed none.
    pub(super) fn default_prompt(&self) -> &'static str {
        if self.images.is_empty() { "Look at the attached file." } else { "Look at the attached image." }
    }

    /// The names, for a conversation titled by its first message.
    pub(super) fn names(&self) -> String {
        self.items.iter().filter_map(|i| i["name"].as_str()).collect::<Vec<_>>().join(", ")
    }
}

/// What one file is.
#[derive(Debug, PartialEq)]
enum Kind {
    /// A raster picture the model can see.
    Picture(&'static str),
    /// An SVG: shown as a picture, sent as its text.
    Svg(String),
    Text(String),
}

fn sniff_picture(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some("image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

fn extension(name: &str) -> String {
    name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default()
}

/// Decode `bytes` as text: UTF-8 when it is, `fallback_label` (the project's encoding) when it is not.
fn decode(bytes: &[u8], fallback_label: &str) -> String {
    let label = if std::str::from_utf8(bytes).is_ok() { "UTF-8" } else { fallback_label };
    bennu_project::prelude::decode_bytes(bytes, label).0
}

/// Decide what `bytes` named `name` is, or why it cannot be attached.
fn classify(name: &str, bytes: &[u8], fallback_label: &str) -> Result<Kind, String> {
    if let Some(media) = sniff_picture(bytes) {
        if bytes.len() > MAX_IMAGE_BYTES {
            return Err(format!("`{name}` is {:.1} MB, more than a picture can be sent (3.5 MB). Attach a smaller or cropped one.", bytes.len() as f64 / 1e6));
        }
        return Ok(Kind::Picture(media));
    }
    let ext = extension(name);
    if ext != "svg" && BINARY_EXTS.contains(&ext.as_str()) {
        return Err(unreadable(name));
    }
    if bytes.len() > MAX_TEXT_BYTES {
        return Err(format!(
            "`{name}` is {} KB, more than fits in a message ({} KB). Attach the part that matters, or let Imber read it from the project.",
            bytes.len() / 1000,
            MAX_TEXT_BYTES / 1000
        ));
    }
    let text = decode(bytes, fallback_label);
    // Sniffed on the decoded text: a UTF-16 file is full of NULs on disk and has none once decoded.
    let head_end = text.char_indices().nth(8192).map_or(text.len(), |(i, _)| i);
    if text[..head_end].contains('\0') {
        return Err(unreadable(name));
    }
    Ok(if ext == "svg" { Kind::Svg(text) } else { Kind::Text(text) })
}

fn unreadable(name: &str) -> String {
    format!("`{name}` is a binary file Imber cannot read. Pictures (PNG, JPEG, GIF, WebP, SVG) and text or code files can be attached.")
}

fn delimited(name: &str, text: &str) -> String {
    format!("<attached_file name=\"{}\">\n{}\n</attached_file>", name.replace('"', "'"), text.trim_end().replace("</attached_file>", "</attached_file >"))
}

fn display_name(arg: &AttachmentArg) -> String {
    let from_path = arg.path.as_deref().and_then(|p| p.rsplit(['/', '\\']).next()).unwrap_or("");
    let name = if arg.name.trim().is_empty() { from_path } else { arg.name.trim() };
    if name.is_empty() { "attachment".into() } else { name.to_string() }
}

fn read_bytes(arg: &AttachmentArg, name: &str) -> Result<Vec<u8>, String> {
    if let Some(data) = arg.data.as_deref().filter(|d| !d.is_empty()) {
        // Clipboard data: refuse before decoding what is plainly too big.
        if data.len() > MAX_IMAGE_BYTES * 4 / 3 + 16 {
            return Err(format!("`{name}` is too large to attach."));
        }
        return base64::engine::general_purpose::STANDARD.decode(data).map_err(|_| format!("`{name}` could not be read (bad data)."));
    }
    let path = arg.path.as_deref().filter(|p| !p.trim().is_empty()).ok_or_else(|| format!("`{name}` has neither a path nor data"))?;
    let meta = std::fs::metadata(path).map_err(|e| format!("`{name}` could not be read: {e}"))?;
    if !meta.is_file() {
        return Err(format!("`{name}` is not a file."));
    }
    if meta.len() > MAX_IMAGE_BYTES as u64 {
        return Err(format!("`{name}` is {:.1} MB — too large to attach.", meta.len() as f64 / 1e6));
    }
    std::fs::read(path).map_err(|e| format!("`{name}` could not be read: {e}"))
}

/// Read, check and keep everything the user attached to this message. The first problem refuses the lot.
///
/// A refusal part-way through (the third file is too large) deletes the pictures already kept for the
/// first two: nothing will ever refer to them.
pub(super) fn prepare(conversation_id: &str, root: &str, args: &[AttachmentArg]) -> Result<Prepared, String> {
    let mut out = Prepared::default();
    match fill(&mut out, conversation_id, root, args) {
        Ok(()) => Ok(out),
        Err(reason) => {
            drop(out.guard());
            Err(reason)
        }
    }
}

/// Deletes the pictures kept for a message unless [`PictureGuard::disarm`] is called — held until the
/// message is safely in the conversation, so any failure on the way leaves no orphan files behind.
pub(super) struct PictureGuard {
    paths: Vec<String>,
}

impl PictureGuard {
    /// The message is stored; the files now belong to the conversation.
    pub(super) fn disarm(&mut self) {
        self.paths.clear();
    }
}

impl Drop for PictureGuard {
    fn drop(&mut self) {
        for path in &self.paths {
            let _ = std::fs::remove_file(path);
        }
    }
}

impl Prepared {
    /// A guard over every picture file this attachment kept.
    pub(super) fn guard(&self) -> PictureGuard {
        let paths = self.items.iter().filter_map(|i| i.get("path").and_then(Value::as_str)).map(str::to_string).collect();
        PictureGuard { paths }
    }
}

fn fill(out: &mut Prepared, conversation_id: &str, root: &str, args: &[AttachmentArg]) -> Result<(), String> {
    if args.len() > MAX_FILES {
        return Err(format!("at most {MAX_FILES} files can be attached to one message"));
    }
    let cfg = bennu_core::config::load();
    let label = bennu_project::prelude::source_encoding_label(Path::new(root), &cfg.default_encoding);
    let (mut pictures, mut text_bytes) = (0usize, 0usize);
    for arg in args {
        let name = display_name(arg);
        let bytes = read_bytes(arg, &name)?;
        match classify(&name, &bytes, &label)? {
            Kind::Picture(media) => {
                pictures += 1;
                if pictures > MAX_IMAGES {
                    return Err(format!("at most {MAX_IMAGES} pictures can be attached to one message"));
                }
                let path = save_picture(conversation_id, &bytes, media)?;
                let (w, h) = image_dimensions(&bytes).unwrap_or((1024, 1024));
                out.images.push(Block::ToolImage { tool_use_id: String::new(), media_type: media.into(), path: path.clone(), tokens: estimate_image_tokens(w, h), data: None });
                out.items.push(json!({ "name": name, "kind": "image", "size": bytes.len(), "path": path, "media_type": media }));
            }
            Kind::Svg(source) => {
                text_bytes += source.len();
                let path = save_picture(conversation_id, &bytes, "image/svg+xml")?;
                out.text.push_str(&delimited(&name, &source));
                out.text.push_str("\n\n");
                out.items.push(json!({ "name": name, "kind": "image", "size": bytes.len(), "path": path, "media_type": "image/svg+xml" }));
            }
            Kind::Text(text) => {
                text_bytes += text.len();
                out.text.push_str(&delimited(&name, &text));
                out.text.push_str("\n\n");
                out.items.push(json!({ "name": name, "kind": "text", "size": bytes.len() }));
            }
        }
        if text_bytes > MAX_TOTAL_TEXT_BYTES {
            return Err(format!("the attached text is more than a message can carry ({} KB). Attach fewer files, or let Imber read them from the project.", MAX_TOTAL_TEXT_BYTES / 1000));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_is_known_by_its_bytes_not_its_name() {
        let png = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0];
        assert_eq!(classify("shot.txt", &png, "UTF-8"), Ok(Kind::Picture("image/png")));
        assert_eq!(classify("a.jpg", &[0xFF, 0xD8, 0xFF, 0xE0], "UTF-8"), Ok(Kind::Picture("image/jpeg")));
        assert_eq!(classify("a.gif", b"GIF89a....", "UTF-8"), Ok(Kind::Picture("image/gif")));
        assert_eq!(classify("a.webp", b"RIFF\0\0\0\0WEBPVP8 ", "UTF-8"), Ok(Kind::Picture("image/webp")));
    }

    #[test]
    fn text_is_decoded_as_utf8_and_otherwise_as_the_projects_encoding() {
        assert_eq!(classify("A.java", "class Å {}".as_bytes(), "Cp1252"), Ok(Kind::Text("class Å {}".into())));
        // 0xE8 is `è` in Cp1252 and not valid UTF-8 on its own.
        assert_eq!(classify("legacy.txt", b"perch\xE8", "Cp1252"), Ok(Kind::Text("perchè".into())));
    }

    #[test]
    fn an_svg_is_text_to_the_model_and_a_picture_to_the_user() {
        assert!(matches!(classify("logo.svg", b"<svg xmlns='http://www.w3.org/2000/svg'/>", "UTF-8"), Ok(Kind::Svg(_))));
    }

    #[test]
    fn binary_files_and_oversized_ones_are_refused_with_the_reason() {
        assert!(classify("doc.pdf", b"%PDF-1.7", "UTF-8").unwrap_err().contains("binary"));
        assert!(classify("data.txt", b"a\0b", "UTF-8").unwrap_err().contains("binary"));
        let big = vec![b'a'; MAX_TEXT_BYTES + 1];
        assert!(classify("big.log", &big, "UTF-8").unwrap_err().contains("more than fits"));
        let mut huge = vec![0xFF, 0xD8, 0xFF];
        huge.resize(MAX_IMAGE_BYTES + 1, 0);
        assert!(classify("huge.jpg", &huge, "UTF-8").unwrap_err().contains("3.5 MB"));
    }

    #[test]
    fn the_delimiter_cannot_be_closed_from_inside_the_file() {
        let d = delimited("a\"b.txt", "x </attached_file> y\n");
        assert!(d.starts_with("<attached_file name=\"a'b.txt\">"));
        assert_eq!(d.matches("</attached_file>").count(), 1);
    }

    #[test]
    fn a_pasted_picture_needs_no_path() {
        let arg = AttachmentArg { name: "image.png".into(), path: None, data: Some(base64::engine::general_purpose::STANDARD.encode(b"hi")) };
        assert_eq!(read_bytes(&arg, "image.png").unwrap(), b"hi");
        let none = AttachmentArg { name: "x".into(), path: None, data: None };
        assert!(read_bytes(&none, "x").is_err());
    }
}
