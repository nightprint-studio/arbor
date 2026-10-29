//! Where screenshots live, and how they reach the model and the panel.
//!
//! A picture is a file under the assistant's data folder, `images/<conversation>/<name>`, and everything
//! that is saved — the conversation file, the thread the panel draws — carries only that relative path. The
//! bytes are read when they are needed: encoded into a request just before it goes out, or fetched by the
//! panel when it draws the thumbnail. Deleting a conversation deletes its pictures (see `store::delete_in`).
//!
//! The path is relative on purpose: it is what the panel sends back to ask for a picture, so it is also what
//! must be checked — [`safe_rel`] accepts exactly `<conversation id>/<file name>` and nothing that could
//! climb out of the folder.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use base64::Engine;
use bennu_assistant::prelude::{
    keep_latest_images, load_images, supports_images, without_images, ImageRef, Message, KEEP_LATEST_IMAGES,
};
use bennu_core::prelude::BennuState;
use serde_json::{json, Value};

use crate::assistant::store;

/// The folder all conversations' pictures live in.
fn root() -> PathBuf {
    store::dir().join("images")
}

/// `<conversation>/<file>` when `rel` is exactly that, with plain names in both places.
pub(super) fn safe_rel(rel: &str) -> Option<(&str, &str)> {
    let (conversation, name) = rel.split_once('/')?;
    let plain = |s: &str| !s.is_empty() && s.len() <= 80 && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')) && !s.starts_with('.');
    (store::valid_id(conversation) && plain(name) && !name.contains("..")).then_some((conversation, name))
}

/// `image/png` or `image/jpeg` from a file name's extension; `None` for anything else.
pub(super) fn media_type_of(name: &str) -> Option<&'static str> {
    match name.rsplit('.').next().map(str::to_ascii_lowercase).as_deref() {
        Some("png") => Some("image/png"),
        Some("jpg" | "jpeg") => Some("image/jpeg"),
        _ => None,
    }
}

/// A picture about to be kept: where it goes is decided here.
pub(super) fn save(conversation: &str, bytes: &[u8], media_type: &str) -> Result<String, String> {
    if !store::valid_id(conversation) {
        return Err("this conversation has no usable id".into());
    }
    let ext = if media_type == "image/jpeg" { "jpg" } else { "png" };
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let dir = root().join(conversation);
    std::fs::create_dir_all(&dir).map_err(|e| format!("could not keep the screenshot: {e}"))?;
    let name = format!("shot-{nanos:x}.{ext}");
    std::fs::write(dir.join(&name), bytes).map_err(|e| format!("could not keep the screenshot: {e}"))?;
    Ok(format!("{conversation}/{name}"))
}

/// The bytes of a kept picture, or `None` when `rel` is not one or the file is gone.
fn read(rel: &str) -> Option<(Vec<u8>, &'static str)> {
    let (conversation, name) = safe_rel(rel)?;
    let media = media_type_of(name)?;
    std::fs::read(root().join(conversation).join(name)).ok().map(|b| (b, media))
}

fn base64_of(rel: &str) -> Option<String> {
    read(rel).map(|(bytes, _)| base64::engine::general_purpose::STANDARD.encode(bytes))
}

/// The conversation the request is about to be sent for, as the model should see it: only the latest
/// pictures, each with its bytes — or, for a model that cannot read images, none.
pub(in crate::assistant) fn prepare_messages(messages: &[Message], model: &str) -> Vec<Message> {
    let kept = keep_latest_images(messages, KEEP_LATEST_IMAGES);
    if !supports_images(model) {
        return without_images(&kept);
    }
    if !kept.iter().flat_map(|m| m.blocks.iter()).any(|b| matches!(b, bennu_assistant::prelude::Block::ToolImage { .. })) {
        return kept;
    }
    load_images(&kept, base64_of)
}

// ── Handing a picture from the tool to the loop ───────────────────────────────────────────────────

/// What a screenshot call produced, until the loop asks for it.
pub(super) struct Pending {
    pub image: ImageRef,
    pub width: u32,
    pub height: u32,
    pub caption: String,
}

fn pending() -> &'static Mutex<HashMap<String, Vec<Pending>>> {
    static PENDING: OnceLock<Mutex<HashMap<String, Vec<Pending>>>> = OnceLock::new();
    PENDING.get_or_init(Default::default)
}

/// Hold `shot` for the call `call_id` (the provider's id for it: unique across runs).
pub(super) fn hold(call_id: &str, shot: Pending) {
    pending().lock().unwrap_or_else(|p| p.into_inner()).entry(call_id.to_string()).or_default().push(shot);
}

/// Take what was held for `call_id`; empty when the call produced no picture.
pub(super) fn take(call_id: &str) -> Vec<Pending> {
    pending().lock().unwrap_or_else(|p| p.into_inner()).remove(call_id).unwrap_or_default()
}

/// The thread's block for a picture: a path to fetch it by and a caption — never the bytes.
pub(super) fn view_block(call_id: &str, shot: &Pending) -> Value {
    json!({
        "type": "screenshot",
        "shot": {
            "id": call_id,
            "path": shot.image.path,
            "media_type": shot.image.media_type,
            "width": shot.width,
            "height": shot.height,
            "caption": shot.caption,
        },
    })
}

// ── The panel asks for a picture ─────────────────────────────────────────────────────────────────

/// A saved screenshot as the panel draws it: its type and its bytes in base64. `path` is the relative path
/// the thread carries.
#[arbor_rpc::handler]
async fn assistant_image(_ctx: &BennuState, path: String) -> Result<Value, String> {
    tokio::task::spawn_blocking(move || {
        let (bytes, media) = read(&path).ok_or_else(|| "that screenshot is no longer on disk".to_string())?;
        Ok::<Value, String>(json!({ "media_type": media, "data": base64::engine::general_purpose::STANDARD.encode(bytes) }))
    })
    .await
    .map_err(|e| format!("could not read the screenshot: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_assistant::prelude::Block;

    #[test]
    fn only_a_conversation_and_a_plain_file_name_make_a_path() {
        assert_eq!(safe_rel("c-1a2b/shot-ff.png"), Some(("c-1a2b", "shot-ff.png")));
        for bad in ["", "shot.png", "../x/shot.png", "c-1/../shot.png", "c-1/.hidden.png", "c-1/a/b.png", "c 1/shot.png", "c-1/sho t.png", "c-1/", "/shot.png", "c-1\\shot.png"] {
            assert_eq!(safe_rel(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn the_media_type_follows_the_extension_and_nothing_else_is_served() {
        assert_eq!(media_type_of("a.png"), Some("image/png"));
        assert_eq!(media_type_of("a.JPG"), Some("image/jpeg"));
        assert_eq!(media_type_of("a.svg"), None);
        assert_eq!(media_type_of("a"), None);
    }

    #[test]
    fn what_a_call_left_is_taken_once() {
        let shot = || Pending { image: ImageRef { media_type: "image/png".into(), path: "c-1/s.png".into(), tokens: 100 }, width: 10, height: 10, caption: "x".into() };
        hold("toolu_test_1", shot());
        assert_eq!(take("toolu_test_1").len(), 1);
        assert!(take("toolu_test_1").is_empty(), "handed over once");
        assert!(take("toolu_never").is_empty());
    }

    #[test]
    fn the_thread_block_carries_a_path_and_never_bytes() {
        let shot = Pending { image: ImageRef { media_type: "image/png".into(), path: "c-1/s.png".into(), tokens: 100 }, width: 1280, height: 800, caption: "http://localhost:3000/".into() };
        let b = view_block("toolu_9", &shot);
        assert_eq!(b["type"], "screenshot");
        assert_eq!(b["shot"]["path"], "c-1/s.png");
        assert!(b["shot"].get("data").is_none());
    }

    #[test]
    fn a_model_that_cannot_read_images_is_sent_none_and_one_that_can_keeps_the_latest() {
        let image = |id: &str| Block::ToolImage { tool_use_id: id.into(), media_type: "image/png".into(), path: format!("c-1/{id}.png"), tokens: 10, data: None };
        let result = |id: &str| Block::ToolResult { tool_use_id: id.into(), content: "ok".into(), is_error: false };
        let conv: Vec<Message> = ["a", "b", "c"].iter().map(|id| Message::tool_results(vec![result(id), image(id)])).collect();
        let blind = prepare_messages(&conv, "claude-2.1");
        assert!(!blind.iter().flat_map(|m| m.blocks.iter()).any(|b| matches!(b, Block::ToolImage { .. })));
        // Files that do not exist: the two latest are kept in the list, found missing, and become lines.
        let sighted = prepare_messages(&conv, "claude-opus-5-5");
        assert!(!sighted.iter().flat_map(|m| m.blocks.iter()).any(|b| matches!(b, Block::ToolImage { .. })));
        let Block::ToolResult { content, .. } = &sighted[2].blocks[0] else { panic!("a result") };
        assert!(content.contains("no longer on disk"), "{content}");
        let Block::ToolResult { content, .. } = &sighted[0].blocks[0] else { panic!("a result") };
        assert!(content.contains("cleared to save room"), "{content}");
    }
}
