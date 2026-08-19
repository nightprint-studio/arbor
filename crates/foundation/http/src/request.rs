//! An inbound request, and the parser that reads one off a socket.
//!
//! Deliberately small: request line, headers, and a `Content-Length` body. What is
//! **not** here is as much of the design as what is.
//!
//! - **No `Transfer-Encoding: chunked`.** Every client this server exists for (MCP over
//!   loopback, an OAuth redirect from a browser) sends a length-delimited body or none at
//!   all. Chunked is answered with `411 Length Required` rather than half-implemented —
//!   a decoder nobody exercises is a decoder nobody has tested.
//! - **No header multi-map.** A repeated header keeps its first value. The headers this
//!   server reads (`Authorization`, `Origin`, `Content-Type`, `Accept`) are single-valued
//!   by definition, and pretending otherwise would only add a lookup shape to every
//!   call site.
//! - **Header names are lowercased on the way in**, so [`Request::header`] can be
//!   case-insensitive without allocating per lookup.

use std::collections::HashMap;

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, BufReader};

use crate::error::{HttpError, Result};

/// One parsed HTTP request.
#[derive(Debug, Clone)]
pub struct Request {
    /// Uppercased method (`GET`, `POST`, …).
    pub method: String,
    /// Path with the query string removed, percent-decoded.
    pub path: String,
    /// Raw query string (everything after the first `?`), still encoded.
    pub query: String,
    /// Header names lowercased; values trimmed.
    pub headers: HashMap<String, String>,
    /// Body bytes, exactly `Content-Length` long (empty when absent).
    pub body: Vec<u8>,
}

impl Request {
    /// A header value by (case-insensitive) name.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(&name.to_ascii_lowercase()).map(String::as_str)
    }

    /// The body as text, lossily decoded. Callers that need strict UTF-8 should
    /// go through [`Request::body`] themselves.
    pub fn body_str(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(&self.body)
    }

    /// One decoded query parameter, first occurrence wins.
    pub fn query_param(&self, name: &str) -> Option<String> {
        self.query.split('&').find_map(|pair| {
            let (k, v) = pair.split_once('=')?;
            (percent_decode(k) == name).then(|| percent_decode(v))
        })
    }

    /// Whether the client accepts `text/event-stream` (the SSE negotiation MCP's
    /// Streamable HTTP transport performs on every POST).
    pub fn accepts_event_stream(&self) -> bool {
        self.header("accept")
            .is_some_and(|a| a.contains("text/event-stream"))
    }
}

/// Read one request off `reader`, or `Ok(None)` at a clean EOF between requests
/// (the peer closed a keep-alive connection it wasn't using).
pub(crate) async fn read_request<R>(
    reader: &mut BufReader<R>,
    max_head: usize,
    max_body: usize,
) -> Result<Option<Request>>
where
    R: AsyncRead + Unpin,
{
    // ── Request line ────────────────────────────────────────────────────────
    let mut budget = max_head;
    let line = match read_line(reader, &mut budget).await? {
        Some(l) if l.trim().is_empty() => {
            // A stray blank line before the request line is legal slack in HTTP/1.1.
            match read_line(reader, &mut budget).await? {
                Some(l) => l,
                None => return Ok(None),
            }
        }
        Some(l) => l,
        None => return Ok(None),
    };

    let mut parts = line.trim_end().split(' ');
    let method = parts.next().ok_or(HttpError::MalformedRequestLine)?.to_ascii_uppercase();
    let target = parts.next().ok_or(HttpError::MalformedRequestLine)?;
    // The version is present and ignored: this server answers HTTP/1.1 regardless,
    // and a 1.0 client simply gets a connection close (see `Connection` handling).
    if parts.next().is_none() || method.is_empty() || target.is_empty() {
        return Err(HttpError::MalformedRequestLine);
    }

    let (raw_path, query) = match target.split_once('?') {
        Some((p, q)) => (p, q.to_string()),
        None => (target, String::new()),
    };

    // ── Headers ─────────────────────────────────────────────────────────────
    let mut headers = HashMap::new();
    loop {
        let line = read_line(reader, &mut budget).await?.ok_or(HttpError::MalformedHeader)?;
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        let (name, value) = line.split_once(':').ok_or(HttpError::MalformedHeader)?;
        if name.is_empty() {
            return Err(HttpError::MalformedHeader);
        }
        headers
            .entry(name.trim().to_ascii_lowercase())
            .or_insert_with(|| value.trim().to_string());
    }

    // ── Body ────────────────────────────────────────────────────────────────
    if headers
        .get("transfer-encoding")
        .is_some_and(|te| te.to_ascii_lowercase().contains("chunked"))
    {
        return Err(HttpError::ChunkedUnsupported);
    }

    let len = match headers.get("content-length") {
        Some(v) => v.trim().parse::<usize>().map_err(|_| HttpError::InvalidContentLength)?,
        None => 0,
    };
    if len > max_body {
        return Err(HttpError::BodyTooLarge { len, limit: max_body });
    }
    let mut body = vec![0u8; len];
    if len > 0 {
        reader.read_exact(&mut body).await?;
    }

    Ok(Some(Request {
        method,
        path: percent_decode(raw_path),
        query,
        headers,
        body,
    }))
}

/// Read one `\n`-terminated line, charging it against the shared head budget so a
/// client cannot stream headers forever. `Ok(None)` at EOF **before** any bytes.
async fn read_line<R>(reader: &mut BufReader<R>, budget: &mut usize) -> Result<Option<String>>
where
    R: AsyncRead + Unpin,
{
    let mut buf = Vec::new();
    let n = reader.read_until(b'\n', &mut buf).await?;
    if n == 0 {
        return Ok(None);
    }
    if n > *budget {
        return Err(HttpError::HeadTooLarge { limit: *budget });
    }
    *budget -= n;
    String::from_utf8(buf)
        .map(Some)
        .map_err(|_| HttpError::MalformedHeader)
}

/// Percent-decode, with `+` left alone.
///
/// `+` means space only inside `application/x-www-form-urlencoded`, and treating it that
/// way in a path is how a filename with a plus in it silently loses it. Callers that hold
/// a form-encoded body decode the `+` themselves, where they know that is what they have.
pub fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                Some(b) => {
                    out.push(b);
                    i += 3;
                    continue;
                }
                // A stray `%` is data, not a broken escape — keep it verbatim.
                None => {}
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn parse(raw: &str) -> Result<Option<Request>> {
        let mut reader = BufReader::new(raw.as_bytes());
        read_request(&mut reader, 16 * 1024, 1024 * 1024).await
    }

    #[tokio::test]
    async fn parses_a_post_with_a_body() {
        let req = parse("POST /mcp?x=1 HTTP/1.1\r\nHost: a\r\nContent-Length: 5\r\n\r\nhello")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(req.method, "POST");
        assert_eq!(req.path, "/mcp");
        assert_eq!(req.query, "x=1");
        assert_eq!(req.body_str(), "hello");
        assert_eq!(req.query_param("x").as_deref(), Some("1"));
    }

    #[tokio::test]
    async fn header_lookup_is_case_insensitive() {
        let req = parse("GET / HTTP/1.1\r\nAuthorization: Bearer t\r\n\r\n")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(req.header("authorization"), Some("Bearer t"));
        assert_eq!(req.header("AUTHORIZATION"), Some("Bearer t"));
    }

    #[tokio::test]
    async fn clean_eof_is_not_an_error() {
        assert!(parse("").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn chunked_is_refused_rather_than_half_read() {
        let err = parse("POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n")
            .await
            .unwrap_err();
        assert_eq!(err.status(), Some(411));
    }

    #[tokio::test]
    async fn oversized_body_is_refused_before_it_is_read() {
        let mut reader = BufReader::new("POST / HTTP/1.1\r\nContent-Length: 99\r\n\r\n".as_bytes());
        let err = read_request(&mut reader, 16 * 1024, 8).await.unwrap_err();
        assert_eq!(err.status(), Some(413));
    }

    #[test]
    fn percent_decoding_keeps_a_stray_percent() {
        assert_eq!(percent_decode("a%20b"), "a b");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("a+b"), "a+b");
    }
}
