//! `file:` URI ↔ filesystem path.
//!
//! LSP identifies documents by URI, Bennu identifies them by absolute path, and the
//! conversion is not the one-liner it looks like. Hand-rolled because the crate that
//! does this properly (`url`) would be a new dependency for one screen of code whose
//! whole job is a fixed, well-specified subset: `file:` URIs with no query, no
//! fragment, and no host.
//!
//! What the cases actually are:
//!
//! * **Percent-encoding is not optional.** A path with a space or an accent produces an
//!   invalid URI unencoded, and rust-analyzer replies with an error rather than a
//!   completion list. Encoding is per RFC 3986: everything outside the unreserved set
//!   becomes `%XX` of its **UTF-8 bytes**.
//! * **Windows drives.** `C:\src\main.rs` is `file:///C:/src/main.rs` — three slashes
//!   (empty host), forward separators, drive letter kept. Some clients emit the drive
//!   colon encoded (`file:///c%3A/src`), so parsing accepts that too: a URI we cannot
//!   read is a go-to that silently does nothing.
//! * **Case.** A Windows path round-trips through a server that may lowercase the
//!   drive, so path comparison downstream must not be byte-exact on the drive letter.
//!   [`from_uri`] normalises the drive to uppercase, which is the form Bennu's own
//!   paths use.
//! * **UNC** (`\\server\share\x`) becomes `file://server/share/x` — a real host.
//!
//! Paths come back **forward-slashed** on every platform, matching the rest of the
//! Bennu wire (`UsageHit::file`, `DeclarationTarget::file`).

/// Convert an absolute filesystem path to a `file:` URI.
///
/// The path is expected to be absolute — LSP has no notion of a relative document —
/// but a relative one is not rejected: it is encoded as-is, which produces a URI the
/// server will refuse, and a server error is a better failure than a panic in a
/// completion request.
pub fn to_uri(path: &str) -> String {
    let normalised = path.replace('\\', "/");

    // UNC: `//server/share/...` → the host is a real authority, not a path segment.
    if let Some(rest) = normalised.strip_prefix("//") {
        let (host, tail) = rest.split_once('/').unwrap_or((rest, ""));
        return format!("file://{}/{}", encode_segment(host), encode_path(tail));
    }

    let body = encode_path(normalised.trim_start_matches('/'));
    // Three slashes: `file:` + empty authority + an absolute path.
    format!("file:///{body}")
}

/// Convert a `file:` URI back to a forward-slashed absolute path.
///
/// `None` for anything that isn't a `file:` URI — a server may legitimately point at
/// `jar:`, `zipfile:` or its own scheme for a dependency's source (rust-analyzer does
/// this for macro expansions), and the caller's right move is to skip the target, not
/// to fabricate a path for it.
pub fn from_uri(uri: &str) -> Option<String> {
    let rest = strip_file_scheme(uri)?;
    // Strip a query / fragment: no `file:` URI Bennu produces has them, but a server's
    // can (rust-analyzer appends none today; other servers do).
    let rest = rest.split(['?', '#']).next().unwrap_or(rest);

    // `//host/path` — the authority. An empty authority (`file:///path`) is the normal
    // case; `localhost` means the same thing and is treated as empty.
    let (host, path) = match rest.strip_prefix("//") {
        Some(after) => match after.split_once('/') {
            Some((h, p)) => (h, p),
            // `file://host` with no path at all.
            None => (after, ""),
        },
        // A `file:/path` (one slash) is legal-ish and appears in the wild.
        None => ("", rest.trim_start_matches('/')),
    };

    let decoded = decode(path)?;
    let host = decode(host)?;
    if !host.is_empty() && host != "localhost" {
        // UNC.
        return Some(format!("//{host}/{decoded}"));
    }
    Some(normalise_drive(&decoded))
}

/// Whether `uri` is a `file:` URI at all — the cheap guard before a full parse.
pub fn is_file_uri(uri: &str) -> bool {
    strip_file_scheme(uri).is_some()
}

/// The part of `uri` after a case-insensitive `file:` scheme, or `None`.
fn strip_file_scheme(uri: &str) -> Option<&str> {
    let (scheme, rest) = uri.split_once(':')?;
    scheme.eq_ignore_ascii_case("file").then_some(rest)
}

/// Re-attach a leading `/` for a POSIX path, or hand back a `C:/…` drive path as-is.
///
/// The drive letter is uppercased: a server is free to round-trip `C:` as `c:`, and
/// Bennu keys open buffers and project slots by path string, so a case flip would make
/// a go-to open a *second* tab for the file already in front of the user.
fn normalise_drive(path: &str) -> String {
    let bytes = path.as_bytes();
    let is_drive = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
    if is_drive {
        let mut out = String::with_capacity(path.len());
        out.push(bytes[0].to_ascii_uppercase() as char);
        out.push_str(&path[1..]);
        return out;
    }
    format!("/{path}")
}

/// Percent-encode a `/`-separated path, leaving the separators intact and leaving a
/// leading Windows drive (`C:`) readable.
fn encode_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len() + 8);
    for (i, segment) in path.split('/').enumerate() {
        if i > 0 {
            out.push('/');
        }
        if i == 0 && is_drive_segment(segment) {
            // `C:` — the colon is what makes it readable as a drive, so it stays.
            out.push_str(segment);
        } else {
            out.push_str(&encode_segment(segment));
        }
    }
    out
}

/// Whether `s` is exactly a Windows drive designator (`C:`).
fn is_drive_segment(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

/// Percent-encode one path segment: keep RFC 3986's unreserved set, encode every other
/// byte. Conservative by design — over-encoding is always accepted, under-encoding is
/// what breaks.
fn encode_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(hex_digit(b >> 4));
            out.push(hex_digit(b & 0x0f));
        }
    }
    out
}

/// Uppercase hex, which is the form RFC 3986 prefers for percent-encoding.
fn hex_digit(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        _ => (b'A' + (nibble - 10)) as char,
    }
}

/// Percent-decode to bytes, then to UTF-8. `None` when a `%XX` escape is truncated or
/// not hex, or the decoded bytes are not UTF-8 — all of which mean "this is not a URI
/// we produced and cannot be trusted as a path".
fn decode(s: &str) -> Option<String> {
    if !s.contains('%') {
        return Some(s.to_string());
    }
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hi = hex_value(*bytes.get(i + 1)?)?;
            let lo = hex_value(*bytes.get(i + 2)?)?;
            out.push(hi << 4 | lo);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// One hex digit's value, or `None` when it isn't one.
fn hex_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_posix_path_round_trips() {
        let uri = to_uri("/home/me/src/main.rs");
        assert_eq!(uri, "file:///home/me/src/main.rs");
        assert_eq!(from_uri(&uri).as_deref(), Some("/home/me/src/main.rs"));
    }

    #[test]
    fn a_windows_path_round_trips_with_a_readable_drive() {
        let uri = to_uri(r"C:\Sviluppo\src\main.rs");
        assert_eq!(uri, "file:///C:/Sviluppo/src/main.rs");
        assert_eq!(from_uri(&uri).as_deref(), Some("C:/Sviluppo/src/main.rs"));
    }

    #[test]
    fn a_drive_encoded_by_the_server_is_still_read() {
        // VS Code's own `Uri.file` emits this form, so a server that echoes a client URI
        // can hand it back to us. Failing to parse it is a go-to that does nothing.
        assert_eq!(from_uri("file:///c%3A/src/main.rs").as_deref(), Some("C:/src/main.rs"));
    }

    #[test]
    fn the_drive_letter_is_normalised_to_uppercase() {
        // Bennu keys tabs and project slots by path string: a case flip would open a
        // second tab for the file already on screen.
        assert_eq!(from_uri("file:///d:/x/y.rs").as_deref(), Some("D:/x/y.rs"));
    }

    #[test]
    fn spaces_and_accents_are_encoded_and_decoded() {
        let path = "/home/me/Mio Progetto/città/main.rs";
        let uri = to_uri(path);
        assert!(!uri.contains(' '), "a raw space makes the URI invalid: {uri}");
        assert!(uri.contains("Mio%20Progetto"), "{uri}");
        assert!(uri.contains("citt%C3%A0"), "UTF-8 bytes, not code points: {uri}");
        assert_eq!(from_uri(&uri).as_deref(), Some(path));
    }

    #[test]
    fn separators_survive_encoding() {
        // The one thing `encode_segment` must never touch.
        assert_eq!(to_uri("/a/b/c"), "file:///a/b/c");
    }

    #[test]
    fn a_unc_path_round_trips_through_a_real_host() {
        let uri = to_uri(r"\\build01\share\proj\main.rs");
        assert_eq!(uri, "file://build01/share/proj/main.rs");
        assert_eq!(from_uri(&uri).as_deref(), Some("//build01/share/proj/main.rs"));
    }

    #[test]
    fn localhost_means_the_local_filesystem() {
        assert_eq!(from_uri("file://localhost/etc/hosts").as_deref(), Some("/etc/hosts"));
    }

    #[test]
    fn a_single_slash_uri_is_tolerated() {
        assert_eq!(from_uri("file:/tmp/x.rs").as_deref(), Some("/tmp/x.rs"));
    }

    #[test]
    fn a_non_file_scheme_is_refused_rather_than_guessed() {
        // rust-analyzer uses its own scheme for macro expansions; the caller must skip
        // those targets, not open a made-up path.
        assert_eq!(from_uri("rust-macro-expansion:///x"), None);
        assert_eq!(from_uri("jar:file:///a.jar!/B.class"), None);
        assert!(!is_file_uri("untitled:Untitled-1"));
        assert!(is_file_uri("FILE:///x"), "the scheme is case-insensitive");
    }

    #[test]
    fn a_broken_escape_is_refused() {
        assert_eq!(from_uri("file:///a%zz/b"), None, "not hex");
        assert_eq!(from_uri("file:///a%4"), None, "truncated");
    }

    #[test]
    fn a_query_or_fragment_is_dropped() {
        assert_eq!(from_uri("file:///a/b.rs?v=2").as_deref(), Some("/a/b.rs"));
        assert_eq!(from_uri("file:///a/b.rs#L10").as_deref(), Some("/a/b.rs"));
    }
}
