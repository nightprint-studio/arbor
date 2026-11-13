//! Showing the user a picture.
//!
//! A diagram says what a flow is; a picture says what a thing looks like — a logo the assistant just wrote as
//! SVG, an icon in the project, a mock-up it was pointed at. The tool puts the picture in the thread as a
//! thumbnail that opens larger. It is for the **user's** eyes: nothing is added to what the model reads (a page
//! it needs to look at is `screenshot_page`, and a picture the user wants it to see is an attachment).
//!
//! The thread carries a path into the pictures folder and never bytes, like a screenshot does. An SVG is drawn
//! by the panel as an `<img>`, so whatever it contains cannot run.

use bennu_assistant::prelude::ToolSpec;
use serde_json::{json, Value};

use super::screenshot::{picture_media_type, save_picture};
use super::{spec, unique_id, ToolHost};

pub(super) const NAME: &str = "show_image";
/// The other direction: a picture on disk handed to the model, as a screenshot is.
pub(super) const VIEW: &str = "view_image";

/// A picture file over this is not shown (the panel decodes it whole); an SVG source over this is not either.
const MAX_FILE_BYTES: u64 = 5_000_000;
const MAX_SVG_CHARS: usize = 200_000;

pub(super) fn specs() -> Vec<ToolSpec> {
    vec![spec(
        NAME,
        "Show the user a picture in the conversation, as a small preview they can open larger: either an image file (PNG, JPEG, GIF, WebP, SVG) with `path`, or an SVG you wrote with `svg`. Use it for a logo, an icon, a chart or a mock-up you made, or to point at an image in the project. The user sees it; you do not get the pixels back (to look at a running page use screenshot_page). Send SVG as the source only, complete, with a `viewBox`; do not repeat it in your text.",
        json!({"type":"object","properties":{
            "path":{"type":"string","description":"An image file, absolute or relative to the project. A chat has no files: use `svg`."},
            "svg":{"type":"string","description":"The SVG source, starting with `<svg`."},
            "caption":{"type":"string","description":"A short line shown under the preview."}
        }}),
    ), spec(
        VIEW,
        "Look at an image file yourself: a PNG, JPEG, GIF or WebP that a program, a script, a test or a build wrote (a render, a chart, a generated texture, a golden image), or one in the project. You receive the picture, as with a screenshot; the user sees it in the thread too. Pictures are expensive (up to a few thousand tokens each) and only the latest two stay in what you read — the older ones become a line — so look at what you need, once. At most 3.5 MB.",
        json!({"type":"object","properties":{
            "path":{"type":"string","description":"The image file, absolute or relative to the project (or `project:path`)."},
            "caption":{"type":"string","description":"A short line saying what it is, shown under it."}
        },"required":["path"]}),
    )]
}

/// The SVG as an `<img>` can draw it: one without the namespace declaration renders as nothing at all.
fn drawable(svg: &str) -> Result<String, String> {
    let source = svg.trim();
    let Some(start) = source.find("<svg") else {
        return Err("`svg` must be an SVG document, starting with `<svg`".into());
    };
    if source.chars().count() > MAX_SVG_CHARS {
        return Err(format!("The SVG is too large (over {MAX_SVG_CHARS} characters). Simplify it."));
    }
    if source[start..].split('>').next().is_some_and(|tag| tag.contains("xmlns=")) {
        return Ok(source.to_string());
    }
    Ok(format!("{}<svg xmlns=\"http://www.w3.org/2000/svg\"{}", &source[..start], &source[start + 4..]))
}

impl ToolHost {
    pub(super) fn run_show_image(&self, input: &Value) -> Result<String, String> {
        let text = |k: &str| input.get(k).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty());
        let (bytes, media, default_caption) = match (text("path"), text("svg")) {
            (Some(_), _) if self.chat => return Err("a chat has no files: send the picture as `svg`".into()),
            (Some(raw), _) => {
                let path = self.resolve(raw);
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let media = picture_media_type(&name).ok_or_else(|| format!("`{name}` is not a picture I can show (PNG, JPEG, GIF, WebP or SVG)"))?;
                let size = std::fs::metadata(&path).map_err(|e| format!("could not read {}: {e}", path.display()))?.len();
                if size > MAX_FILE_BYTES {
                    return Err(format!("{name} is {} MB, too large to show", size / 1_000_000));
                }
                (std::fs::read(&path).map_err(|e| format!("could not read {}: {e}", path.display()))?, media, name)
            }
            (None, Some(svg)) => (drawable(svg)?.into_bytes(), "image/svg+xml", String::new()),
            (None, None) => return Err("give either `path` (an image file) or `svg` (its source)".into()),
        };
        let stored = save_picture(&self.session.conversation_id, &bytes, media)?;
        let id = unique_id("im");
        let caption = text("caption").map(str::to_string).unwrap_or(default_caption);
        self.session.view().push(json!({ "type": "image", "image": { "id": id, "path": stored, "media_type": media, "caption": caption } }));
        self.session.push_message();
        Ok("Picture shown to the user. Do not describe it again or repeat its source in text.".into())
    }
}

impl ToolHost {
    /// Hand an image file to the model (see [`VIEW`]).
    pub(super) fn run_view_image(&self, call_id: &str, input: &Value) -> Result<String, String> {
        let raw = input.get("path").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).ok_or("`path` is required")?;
        let path = self.resolve(raw);
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let media = picture_media_type(&name)
            .filter(|m| *m != "image/svg+xml")
            .ok_or_else(|| format!("`{name}` is not a picture the model reads (PNG, JPEG, GIF or WebP); an SVG is text: read it as a file"))?;
        let bytes = std::fs::read(&path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
        let caption = input.get("caption").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).unwrap_or(name);
        let described = self.attach_picture(call_id, &bytes, media, &caption)?;
        Ok(format!("{} — {described}. The picture follows.", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_svg_without_its_namespace_gets_one_and_one_with_it_is_left_alone() {
        let bare = drawable("<svg viewBox=\"0 0 1 1\"><rect/></svg>").unwrap();
        assert!(bare.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox"), "{bare}");
        let ready = "<?xml version=\"1.0\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\"><rect/></svg>";
        assert_eq!(drawable(ready).unwrap(), ready);
        assert!(drawable("<div/>").is_err());
        assert!(drawable(&format!("<svg>{}</svg>", "x".repeat(MAX_SVG_CHARS))).is_err());
    }

    #[test]
    fn the_tool_takes_a_file_or_a_source_and_says_the_user_sees_it() {
        let s = specs().remove(0);
        assert_eq!(s.name, NAME);
        assert!(s.input_schema["properties"].get("path").is_some() && s.input_schema["properties"].get("svg").is_some());
        assert!(s.description.contains("you do not get the pixels"));
    }
}
