//! PKCE + URL helpers shared by every OAuth flow in the crate.
//!
//! Intentionally `pub` — consumers occasionally need to build their own
//! authorize URL or decode params from a custom callback handler.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

/// Random 256-bit code verifier, base64url-no-pad encoded (RFC 7636 §4.1).
pub fn code_verifier() -> String {
    let a = uuid::Uuid::new_v4();
    let b = uuid::Uuid::new_v4();
    let bytes: Vec<u8> = a.as_bytes().iter().chain(b.as_bytes().iter()).copied().collect();
    URL_SAFE_NO_PAD.encode(&bytes)
}

/// SHA-256 of the verifier, base64url-no-pad encoded (RFC 7636 §4.2).
/// Used as `code_challenge` with `code_challenge_method=S256`.
pub fn code_challenge(verifier: &str) -> String {
    let mut h = Sha256::new();
    h.update(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(h.finalize())
}

/// Opaque random state parameter (CSRF defence on the redirect).
pub fn random_state() -> String {
    URL_SAFE_NO_PAD.encode(uuid::Uuid::new_v4().as_bytes())
}

/// Strict RFC 3986 unreserved-character URL encoder. Used for query-string
/// values; we deliberately don't use `url::form_urlencoded` to avoid the
/// dep + because the relaxed encoding it does (e.g. `+` for space) breaks
/// some OAuth providers' state matching.
pub fn urlencoded(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            b => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Extract a single query parameter from a raw HTTP/1.x request line
/// (`GET /callback?code=…&state=… HTTP/1.1`).
pub fn extract_param(request: &str, key: &str) -> Option<String> {
    let line = request.lines().next()?;
    let query = line.split('?').nth(1)?.split_whitespace().next()?;
    for pair in query.split('&') {
        let mut parts = pair.splitn(2, '=');
        if parts.next() == Some(key) {
            return parts.next().map(percent_decode);
        }
    }
    None
}

/// Inverse of [`urlencoded`] — `%HH` plus `+` → space (form-encoding).
pub fn percent_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            let h1 = chars.next().unwrap_or('0');
            let h2 = chars.next().unwrap_or('0');
            if let Ok(b) = u8::from_str_radix(&format!("{h1}{h2}"), 16) {
                out.push(b as char);
                continue;
            }
            // Malformed — keep the literal '%' + the two chars so debugging
            // a bad URL is easier than silently dropping bytes.
            out.push('%'); out.push(h1); out.push(h2);
        } else if c == '+' {
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out
}
