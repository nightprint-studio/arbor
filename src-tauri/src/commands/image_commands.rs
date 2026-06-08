//! Inline image proxy for issue/MR/PR bodies.
//!
//! Issue-tracker and code-review providers serve attached images behind their
//! own auth (private Jira/Linear instances, private GitHub/GitLab repos). The
//! WebView can't send those credentials, so it would render every private image
//! as a broken box. This command fetches the bytes through the provider's
//! authenticated HTTP path and hands them back as a `data:` URL the WebView can
//! display directly. The provider token is only ever attached to that provider's
//! own host — see the per-provider `fetch_image_bytes` implementations.

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

use crate::error::AppError;

/// 10 MB ceiling. Issue/PR screenshots are typically well under this; the cap
/// keeps a pathological attachment from ballooning the base64 payload over IPC.
const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;

/// Fetch an inline image and return it as a `data:<mime>;base64,<...>` URL.
///
/// `provider` is one of `linear` | `jira` | `github` | `gitlab`. `base_url` is
/// only used by GitLab (the instance origin, derived from the MR web URL) to
/// resolve relative `/uploads/...` paths and decide whether to attach the token.
#[tauri::command]
pub async fn fetch_remote_image(
    url:      String,
    provider: String,
    base_url: Option<String>,
) -> Result<String, AppError> {
    let (bytes, ctype) = match provider.as_str() {
        "linear" => crate::integrations::linear::fetch_image_bytes(&url).await?,
        "jira"   => crate::integrations::jira::fetch_image_bytes(&url).await?,
        "github" | "gitlab" => {
            crate::git_provider::repo_impl::fetch_image_bytes(&provider, base_url.as_deref(), &url).await?
        }
        other => return Err(AppError::Other(format!("Unknown image provider: {other}"))),
    };

    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(AppError::Other(format!(
            "Image too large ({} KB) to preview inline",
            bytes.len() / 1024
        )));
    }

    let mime = resolve_mime(ctype.as_deref(), &url, &bytes);
    if !mime.starts_with("image/") {
        return Err(AppError::Other(format!("Not an image ({mime})")));
    }
    Ok(format!("data:{mime};base64,{}", BASE64.encode(&bytes)))
}

/// Resolve the MIME type: trust the response `Content-Type` when it's an image,
/// otherwise sniff the magic bytes, otherwise fall back to the URL extension.
fn resolve_mime(content_type: Option<&str>, url: &str, bytes: &[u8]) -> String {
    if let Some(ct) = content_type {
        let ct = ct.split(';').next().unwrap_or("").trim().to_lowercase();
        if ct.starts_with("image/") {
            return ct;
        }
    }
    if let Some(m) = sniff_image(bytes) {
        return m.to_string();
    }
    mime_from_ext(url)
}

fn sniff_image(b: &[u8]) -> Option<&'static str> {
    if b.len() >= 8 && b[0..8] == [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'] {
        return Some("image/png");
    }
    if b.len() >= 3 && b[0..3] == [0xff, 0xd8, 0xff] {
        return Some("image/jpeg");
    }
    if b.len() >= 6 && (&b[0..6] == b"GIF87a" || &b[0..6] == b"GIF89a") {
        return Some("image/gif");
    }
    if b.len() >= 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    if b.len() >= 12 && &b[4..12] == b"ftypavif" {
        return Some("image/avif");
    }
    if b.len() >= 2 && &b[0..2] == b"BM" {
        return Some("image/bmp");
    }
    // SVG is text-based — look for the root element near the start.
    let head = &b[..b.len().min(256)];
    if let Ok(s) = std::str::from_utf8(head) {
        let t = s.trim_start();
        if t.starts_with("<?xml") || t.contains("<svg") {
            return Some("image/svg+xml");
        }
    }
    None
}

fn mime_from_ext(url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let ext = path.rsplit('.').next().unwrap_or("").to_lowercase();
    match ext.as_str() {
        "png"          => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif"          => "image/gif",
        "webp"         => "image/webp",
        "svg"          => "image/svg+xml",
        "bmp"          => "image/bmp",
        "avif"         => "image/avif",
        "ico"          => "image/x-icon",
        _              => "application/octet-stream",
    }
    .to_string()
}
