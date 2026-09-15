//! The **base protocol** shared by LSP and DAP: JSON bodies inside `Content-Length`-framed
//! headers, over a plain byte stream — a child process's stdin and stdout.
//!
//! ```text
//! Content-Length: 42\r\n
//! \r\n
//! {"seq":1,"type":"request","command":"initialize"}
//! ```
//!
//! Deliberately dumb: this crate moves bytes and decides nothing. It does not know what a message
//! means, which messages exist, or who answers them. What it owns is the one thing every transport
//! bug of either protocol comes from: **the frame boundary**.
//!
//! ## Why it is its own crate
//!
//! Two protocols use it. Microsoft specified the same envelope for the Language Server Protocol and
//! the Debug Adapter Protocol, and the bodies inside are unrelated — LSP's are JSON-RPC 2.0, DAP's
//! are its own `{seq, type, …}` shape. So the *bodies* belong to their own crates and the *envelope*
//! belongs to neither. It lived in `bennu-lsp` while there was one consumer; the debugger made it two,
//! and a second copy of a frame reader is a second place for a desync bug to be fixed in.
//!
//! It has no dependencies and never will: this is `std::io` and a header parser.
//!
//! ## Three rules the spec states and implementations forget
//!
//! * the header block is ASCII and ends with an **empty** `\r\n` line;
//! * `Content-Length` counts **bytes**, not characters — a body with one `é` in it is longer than
//!   its `chars().count()`, so the body is read as bytes and decoded after;
//! * header names are **case-insensitive** (`content-length` is legal, and some peers send it).
//!
//! ## Framing errors are fatal, not per-message
//!
//! A malformed frame is a protocol desync: once the reader has lost the boundary, every subsequent
//! read is garbage. So these surface as [`io::Error`] and the caller's only correct move is to
//! declare the connection dead — which is what both clients do.

use std::io::{self, BufRead, Write};

/// The header that carries the body length. Compared case-insensitively.
const CONTENT_LENGTH: &str = "content-length";

/// A hard ceiling on one message's body, as a defence against a desynced stream turning a bogus
/// length into a multi-gigabyte allocation.
///
/// Generous on purpose: a `textDocument/semanticTokens/full` for a 20k-line file, and a DAP
/// `variables` answer on a container with thousands of elements, are legitimately megabytes. This is
/// a sanity bound, not a policy.
pub const MAX_BODY: usize = 128 * 1024 * 1024;

/// Read one framed message body from `reader`.
///
/// `Ok(None)` is a clean end of stream — the peer exited — and is the normal way a reader loop
/// terminates. `Err` means the framing itself was violated: the caller must not read again.
///
/// `peer` names the other end for the error messages only (`"language server"`, `"debug adapter"`).
/// A transport error a user might see should say which process went away.
pub fn read_message<R: BufRead>(reader: &mut R, peer: &str) -> io::Result<Option<Vec<u8>>> {
    let mut content_length: Option<usize> = None;
    let mut line = String::new();

    loop {
        line.clear();
        let read = reader.read_line(&mut line)?;
        if read == 0 {
            // EOF. Clean only *between* messages: mid-header it means the peer died with a
            // half-written frame, which the caller should hear about as an error rather than as an
            // orderly shutdown.
            return if content_length.is_none() {
                Ok(None)
            } else {
                Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    format!("{peer} closed its output mid-header"),
                ))
            };
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break; // end of the header block
        }
        let Some((name, value)) = trimmed.split_once(':') else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("malformed header line from {peer}: {trimmed:?}"),
            ));
        };
        if name.trim().eq_ignore_ascii_case(CONTENT_LENGTH) {
            content_length = Some(value.trim().parse::<usize>().map_err(|e| {
                io::Error::new(io::ErrorKind::InvalidData, format!("bad Content-Length: {e}"))
            })?);
        }
        // Every other header (`Content-Type`, and whatever a peer invents) is ignored: both specs
        // fix the charset at UTF-8 and nothing else is actionable.
    }

    let len = content_length.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("frame from {peer} had no Content-Length header"),
        )
    })?;
    if len > MAX_BODY {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("frame from {peer} claims {len} bytes — refusing (stream is probably desynced)"),
        ));
    }
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body)?;
    Ok(Some(body))
}

/// Write one framed message body to `writer` and flush it.
///
/// The flush is not optional: both peers are request/response, so a body sitting in our buffer is a
/// request the other end never sees and a caller that blocks until it times out.
pub fn write_message<W: Write>(writer: &mut W, body: &[u8]) -> io::Result<()> {
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(body)?;
    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufReader, Cursor};

    fn read_all(bytes: &[u8]) -> io::Result<Vec<String>> {
        let mut reader = BufReader::new(Cursor::new(bytes.to_vec()));
        let mut out = Vec::new();
        while let Some(body) = read_message(&mut reader, "test peer")? {
            out.push(String::from_utf8_lossy(&body).into_owned());
        }
        Ok(out)
    }

    #[test]
    fn a_frame_round_trips() {
        let mut buf = Vec::new();
        write_message(&mut buf, br#"{"seq":1}"#).unwrap();
        assert_eq!(String::from_utf8_lossy(&buf), "Content-Length: 9\r\n\r\n{\"seq\":1}");
        assert_eq!(read_all(&buf).unwrap(), vec![r#"{"seq":1}"#]);
    }

    #[test]
    fn several_frames_read_back_in_order() {
        let mut buf = Vec::new();
        for body in [b"one".as_slice(), b"two", b"three"] {
            write_message(&mut buf, body).unwrap();
        }
        assert_eq!(read_all(&buf).unwrap(), vec!["one", "two", "three"]);
    }

    /// The length is in BYTES. A body counted in characters is short by one per non-ASCII
    /// character, and the reader then starts the next frame inside this one's tail — which is a
    /// desync that looks like a peer sending garbage.
    #[test]
    fn the_length_counts_bytes_and_not_characters() {
        let body = "città 日本".as_bytes();
        assert!(body.len() > "città 日本".chars().count());
        let mut buf = Vec::new();
        write_message(&mut buf, body).unwrap();
        write_message(&mut buf, b"after").unwrap();
        assert_eq!(read_all(&buf).unwrap(), vec!["città 日本", "after"]);
    }

    #[test]
    fn the_header_name_is_case_insensitive() {
        let raw = b"content-length: 2\r\n\r\nhi";
        assert_eq!(read_all(raw).unwrap(), vec!["hi"]);
    }

    /// Anything else in the header block is ignored rather than rejected: peers send `Content-Type`,
    /// and refusing a header we have no use for would break a conforming one.
    #[test]
    fn other_headers_are_ignored() {
        let raw = b"Content-Type: application/vscode-jsonrpc; charset=utf-8\r\nContent-Length: 2\r\n\r\nhi";
        assert_eq!(read_all(raw).unwrap(), vec!["hi"]);
    }

    #[test]
    fn end_of_stream_between_frames_is_clean() {
        let mut reader = BufReader::new(Cursor::new(Vec::new()));
        assert!(read_message(&mut reader, "test peer").unwrap().is_none());
    }

    /// …and end of stream MID-header is not: the peer died with a half-written frame, and reporting
    /// that as an orderly shutdown would make a crashed adapter look like one that exited normally.
    #[test]
    fn end_of_stream_mid_header_is_an_error() {
        let err = read_all(b"Content-Length: 5\r\n").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
        assert!(err.to_string().contains("test peer"), "{err}");
    }

    #[test]
    fn a_body_shorter_than_its_length_is_an_error() {
        let err = read_all(b"Content-Length: 99\r\n\r\nshort").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
    }

    #[test]
    fn a_header_without_a_colon_is_a_desync() {
        let err = read_all(b"garbage\r\n\r\n").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn a_frame_with_no_length_is_refused() {
        let err = read_all(b"Content-Type: x\r\n\r\nbody").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("no Content-Length"), "{err}");
    }

    /// A bogus length must not become a bogus allocation: a desynced stream will happily claim
    /// gigabytes, and reserving them is how a transport bug becomes an out-of-memory kill.
    #[test]
    fn an_absurd_length_is_refused_rather_than_allocated() {
        let raw = format!("Content-Length: {}\r\n\r\n", MAX_BODY + 1);
        let err = read_all(raw.as_bytes()).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("desynced"), "{err}");
    }

    #[test]
    fn an_empty_body_is_a_legal_frame() {
        let mut buf = Vec::new();
        write_message(&mut buf, b"").unwrap();
        assert_eq!(read_all(&buf).unwrap(), vec![""]);
    }
}
