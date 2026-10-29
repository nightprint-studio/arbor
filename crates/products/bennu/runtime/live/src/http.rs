//! A loopback-only HTTP/1.1 client over `TcpStream`.
//!
//! Loopback only is the whole design: the assistant may talk to the services *it started on this
//! machine*, and nothing else. With no TLS to speak, no redirects to follow off-host and no proxy,
//! there is no way for this client to reach a remote host even by a crafted URL — [`parse_local_url`]
//! refuses everything but `http://localhost`, `127.0.0.1` and `[::1]`, and userinfo tricks
//! (`http://localhost@evil/`) are refused too.
//!
//! Requests carry an `X-Correlation-Id` (added when the caller did not give one) so the services' own
//! logs can be tied back to the call; see `timeline::trace`.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::clock::now_ms;

pub const CORRELATION_HEADER: &str = "X-Correlation-Id";

/// Headers whose values are secrets: shown to the model as `<redacted>` unless the user set them.
const SENSITIVE: [&str; 6] = ["authorization", "proxy-authorization", "cookie", "set-cookie", "x-api-key", "x-auth-token"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalUrl {
    /// `localhost`, `127.0.0.1` or `::1` (no brackets).
    pub host: String,
    pub port: u16,
    /// Path and query, starting with `/`.
    pub path: String,
}

impl LocalUrl {
    /// The `Host` header value.
    fn authority(&self) -> String {
        let host = if self.host.contains(':') { format!("[{}]", self.host) } else { self.host.clone() };
        if self.port == 80 { host } else { format!("{host}:{}", self.port) }
    }
}

/// Parse `url`, accepting only plain-HTTP loopback targets.
pub fn parse_local_url(url: &str) -> Result<LocalUrl, String> {
    let url = url.trim();
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("https://") {
        return Err("only plain http:// to localhost is allowed (a service on this machine does not need TLS)".into());
    }
    let rest = lower.strip_prefix("http://").map(|_| &url["http://".len()..]).ok_or("the URL must start with http://")?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(end);
    if authority.contains('@') {
        return Err("a URL with credentials is not allowed".into());
    }
    let (host, port_text) = if let Some(inner) = authority.strip_prefix('[') {
        let close = inner.find(']').ok_or("unclosed `[` in the host")?;
        let after = &inner[close + 1..];
        (inner[..close].to_string(), after.strip_prefix(':').map(str::to_string))
    } else {
        match authority.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), Some(p.to_string())),
            None => (authority.to_string(), None),
        }
    };
    let host = host.to_ascii_lowercase();
    if !matches!(host.as_str(), "localhost" | "127.0.0.1" | "::1") {
        return Err(format!("`{host}` is not loopback: only localhost, 127.0.0.1 and [::1] are allowed"));
    }
    let port = match port_text.as_deref() {
        None | Some("") => 80,
        Some(p) => p.parse::<u16>().map_err(|_| format!("`{p}` is not a port"))?,
    };
    let tail = tail.split('#').next().unwrap_or("");
    let path = if tail.is_empty() { "/".to_string() } else if tail.starts_with('/') { tail.to_string() } else { format!("/{tail}") };
    Ok(LocalUrl { host, port, path })
}

#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
    pub timeout_ms: u64,
}

impl HttpRequest {
    pub fn get(url: &str) -> Self {
        Self { method: "GET".into(), url: url.into(), headers: Vec::new(), body: None, timeout_ms: 10_000 }
    }

    /// The value of a header, case-insensitively.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    /// The correlation id this request carries, adding a fresh one when it has none.
    pub fn ensure_correlation(&mut self) -> String {
        if let Some(id) = self.header(CORRELATION_HEADER) {
            return id.to_string();
        }
        let id = new_correlation_id();
        self.headers.push((CORRELATION_HEADER.to_string(), id.clone()));
        id
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct HttpResponse {
    pub status: u16,
    pub reason: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
    /// The body was longer than the cap and was cut.
    pub truncated: bool,
    pub elapsed_ms: u64,
}

impl HttpResponse {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

static CORRELATION_SEQ: AtomicU64 = AtomicU64::new(1);

/// A short unique id: time plus a process-wide counter. Unique enough to grep for, short enough to log.
pub fn new_correlation_id() -> String {
    format!("bl-{:x}-{:x}", now_ms(), CORRELATION_SEQ.fetch_add(1, Ordering::SeqCst))
}

/// `headers` with the secret values replaced, except those whose names are in `reveal` (the ones the
/// user themselves set in a recipe or scenario).
pub fn redact_headers(headers: &[(String, String)], reveal: &[String]) -> Vec<(String, String)> {
    headers
        .iter()
        .map(|(k, v)| {
            let secret = SENSITIVE.contains(&k.to_ascii_lowercase().as_str()) && !reveal.iter().any(|r| r.eq_ignore_ascii_case(k));
            (k.clone(), if secret { "<redacted>".to_string() } else { v.clone() })
        })
        .collect()
}

/// Send `req` and read the answer, keeping at most `max_body` bytes of the body.
pub fn send(req: &HttpRequest, max_body: usize) -> Result<HttpResponse, String> {
    let target = parse_local_url(&req.url)?;
    let method = req.method.trim().to_ascii_uppercase();
    if method.is_empty() || !method.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err(format!("`{}` is not an HTTP method", req.method));
    }
    for (k, v) in &req.headers {
        if k.contains(['\r', '\n', ':']) || v.contains(['\r', '\n']) {
            return Err(format!("header `{k}` contains a line break"));
        }
    }
    let started = Instant::now();
    let budget = Duration::from_millis(req.timeout_ms.max(100));
    let mut stream = connect(&target, budget)?;

    let mut head = format!("{method} {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nAccept: */*\r\nUser-Agent: bennu-live\r\n", target.path, target.authority());
    for (k, v) in &req.headers {
        if matches!(k.to_ascii_lowercase().as_str(), "host" | "connection" | "content-length" | "transfer-encoding") {
            continue;
        }
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    let body = req.body.as_deref().unwrap_or("");
    if !body.is_empty() || matches!(method.as_str(), "POST" | "PUT" | "PATCH") {
        head.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    head.push_str("\r\n");
    stream.set_write_timeout(Some(budget)).map_err(|e| e.to_string())?;
    stream.write_all(head.as_bytes()).and_then(|_| stream.write_all(body.as_bytes())).map_err(|e| format!("could not send the request: {e}"))?;

    // Raw bytes kept: the cap plus room for the head and chunk framing.
    let raw_cap = max_body.saturating_add(64 * 1024);
    let mut raw: Vec<u8> = Vec::new();
    let mut buf = [0u8; 16 * 1024];
    let mut over_cap = false;
    loop {
        let left = budget.checked_sub(started.elapsed()).ok_or_else(|| format!("timed out after {} ms", req.timeout_ms))?;
        stream.set_read_timeout(Some(left.max(Duration::from_millis(10)))).map_err(|e| e.to_string())?;
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                raw.extend_from_slice(&buf[..n]);
                if raw.len() > raw_cap {
                    over_cap = true;
                    break;
                }
            }
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {
                return Err(format!("timed out after {} ms", req.timeout_ms));
            }
            // A reset after a full answer is how some servers close; keep what arrived.
            Err(_) if !raw.is_empty() => break,
            Err(e) => return Err(format!("could not read the response: {e}")),
        }
    }
    let mut response = parse_response(&raw, max_body, over_cap)?;
    response.elapsed_ms = started.elapsed().as_millis() as u64;
    Ok(response)
}

fn connect(target: &LocalUrl, budget: Duration) -> Result<TcpStream, String> {
    let addrs = (target.host.as_str(), target.port).to_socket_addrs().map_err(|e| format!("could not resolve {}: {e}", target.host))?;
    let wait = budget.min(Duration::from_secs(5));
    let mut last = String::from("no address");
    for addr in addrs {
        // Only loopback addresses, even if `localhost` resolves to something else in a hosts file.
        if !addr.ip().is_loopback() {
            continue;
        }
        match TcpStream::connect_timeout(&addr, wait) {
            Ok(s) => {
                let _ = s.set_nodelay(true);
                return Ok(s);
            }
            Err(e) => last = e.to_string(),
        }
    }
    Err(format!("nothing is listening on {}:{} ({last})", target.host, target.port))
}

/// Parse a raw HTTP/1.x response. `over_cap` says the read stopped because of the byte cap.
fn parse_response(raw: &[u8], max_body: usize, over_cap: bool) -> Result<HttpResponse, String> {
    let split = find(raw, b"\r\n\r\n").ok_or("the response had no header block")?;
    let head = String::from_utf8_lossy(&raw[..split]).to_string();
    let mut lines = head.lines();
    let status_line = lines.next().ok_or("empty response")?;
    let mut parts = status_line.splitn(3, ' ');
    let version = parts.next().unwrap_or("");
    if !version.starts_with("HTTP/") {
        return Err(format!("not an HTTP response: `{}`", status_line.chars().take(60).collect::<String>()));
    }
    let status: u16 = parts.next().and_then(|s| s.parse().ok()).ok_or("the status line has no status code")?;
    let reason = parts.next().unwrap_or("").to_string();
    let headers: Vec<(String, String)> = lines.filter_map(|l| l.split_once(':')).map(|(k, v)| (k.trim().to_string(), v.trim().to_string())).collect();
    let find_header = |name: &str| headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str());

    let body_raw = &raw[split + 4..];
    let (body_bytes, complete) = if find_header("transfer-encoding").is_some_and(|v| v.to_ascii_lowercase().contains("chunked")) {
        decode_chunked(body_raw)
    } else if let Some(len) = find_header("content-length").and_then(|v| v.parse::<usize>().ok()) {
        (body_raw[..len.min(body_raw.len())].to_vec(), body_raw.len() >= len)
    } else {
        (body_raw.to_vec(), true)
    };
    let truncated = body_bytes.len() > max_body || over_cap || !complete;
    let kept = &body_bytes[..body_bytes.len().min(max_body)];
    Ok(HttpResponse { status, reason, headers, body: String::from_utf8_lossy(kept).to_string(), truncated, elapsed_ms: 0 })
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Decode a chunked body; the flag says the terminating chunk was seen.
fn decode_chunked(mut data: &[u8]) -> (Vec<u8>, bool) {
    let mut out = Vec::new();
    loop {
        let Some(eol) = find(data, b"\r\n") else { return (out, false) };
        let size_text = String::from_utf8_lossy(&data[..eol]);
        let size_text = size_text.split(';').next().unwrap_or("").trim();
        let Ok(size) = usize::from_str_radix(size_text, 16) else { return (out, false) };
        data = &data[eol + 2..];
        if size == 0 {
            return (out, true);
        }
        if data.len() < size {
            out.extend_from_slice(data);
            return (out, false);
        }
        out.extend_from_slice(&data[..size]);
        data = &data[size..];
        data = data.strip_prefix(b"\r\n").unwrap_or(data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    /// A server that answers one request with `reply` and reports what it received.
    fn serve_once(reply: Vec<u8>) -> (u16, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut got = Vec::new();
            let mut buf = [0u8; 4096];
            // The request ends at the blank line (tests send small bodies in the same packet).
            while let Ok(n) = s.read(&mut buf) {
                got.extend_from_slice(&buf[..n]);
                if n == 0 || find(&got, b"\r\n\r\n").is_some() {
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(20));
            let mut rest = [0u8; 4096];
            s.set_read_timeout(Some(Duration::from_millis(30))).ok();
            if let Ok(n) = s.read(&mut rest) {
                got.extend_from_slice(&rest[..n]);
            }
            s.write_all(&reply).unwrap();
            String::from_utf8_lossy(&got).to_string()
        });
        (port, handle)
    }

    #[test]
    fn only_plain_loopback_urls_are_accepted() {
        assert_eq!(parse_local_url("http://localhost:8080/a?b=1").unwrap(), LocalUrl { host: "localhost".into(), port: 8080, path: "/a?b=1".into() });
        assert_eq!(parse_local_url("http://127.0.0.1/").unwrap().port, 80);
        assert_eq!(parse_local_url("http://[::1]:9000").unwrap(), LocalUrl { host: "::1".into(), port: 9000, path: "/".into() });
        assert_eq!(parse_local_url("HTTP://LocalHost:81/x#frag").unwrap().path, "/x");
        for bad in [
            "https://localhost/", "http://example.com/", "http://localhost.evil.com/", "http://localhost@evil.com/", "http://evil.com:80@localhost/",
            "ftp://localhost/", "localhost:80", "http://192.168.1.2/", "http://0.0.0.0:80/", "http://localhost:99999/",
        ] {
            assert!(parse_local_url(bad).is_err(), "{bad} must be refused");
        }
    }

    #[test]
    fn secrets_are_redacted_unless_the_user_set_them() {
        let h = vec![
            ("Authorization".to_string(), "Bearer abc".to_string()),
            ("cookie".to_string(), "sid=1".to_string()),
            ("Accept".to_string(), "json".to_string()),
        ];
        let red = redact_headers(&h, &[]);
        assert_eq!(red[0].1, "<redacted>");
        assert_eq!(red[1].1, "<redacted>");
        assert_eq!(red[2].1, "json");
        let shown = redact_headers(&h, &["authorization".to_string()]);
        assert_eq!(shown[0].1, "Bearer abc");
        assert_eq!(shown[1].1, "<redacted>");
    }

    #[test]
    fn a_correlation_id_is_added_once_and_kept_when_given() {
        let mut r = HttpRequest::get("http://localhost/");
        let id = r.ensure_correlation();
        assert_eq!(r.header("x-correlation-id"), Some(id.as_str()));
        assert_eq!(r.ensure_correlation(), id);
        let mut own = HttpRequest::get("http://localhost/");
        own.headers.push(("x-correlation-id".into(), "mine".into()));
        assert_eq!(own.ensure_correlation(), "mine");
        assert_ne!(new_correlation_id(), new_correlation_id());
    }

    #[test]
    fn a_content_length_response_round_trips() {
        let (port, server) = serve_once(b"HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: 11\r\n\r\n{\"ok\":true}".to_vec());
        let mut req = HttpRequest::get(&format!("http://127.0.0.1:{port}/things"));
        req.method = "post".into();
        req.body = Some("{\"a\":1}".into());
        req.headers.push(("Content-Type".into(), "application/json".into()));
        req.ensure_correlation();
        let resp = send(&req, 1000).unwrap();
        assert_eq!((resp.status, resp.reason.as_str(), resp.body.as_str(), resp.truncated), (201, "Created", "{\"ok\":true}", false));
        assert_eq!(resp.header("content-type"), Some("application/json"));
        let seen = server.join().unwrap();
        assert!(seen.starts_with("POST /things HTTP/1.1\r\n"), "{seen}");
        assert!(seen.contains("X-Correlation-Id: bl-") && seen.contains("Content-Length: 7"), "{seen}");
        assert!(seen.ends_with("{\"a\":1}"), "{seen}");
    }

    #[test]
    fn a_chunked_response_is_decoded() {
        let (port, _s) = serve_once(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n".to_vec());
        let resp = send(&HttpRequest::get(&format!("http://localhost:{port}/")), 1000).unwrap();
        assert_eq!(resp.body, "hello world");
        assert!(!resp.truncated);
    }

    #[test]
    fn a_long_body_is_cut_and_marked() {
        let body = "z".repeat(5000);
        let reply = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}", body.len());
        let (port, _s) = serve_once(reply.into_bytes());
        let resp = send(&HttpRequest::get(&format!("http://127.0.0.1:{port}/")), 100).unwrap();
        assert_eq!(resp.body.len(), 100);
        assert!(resp.truncated);
    }

    #[test]
    fn a_closed_port_and_a_remote_host_are_errors() {
        let port = {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        let err = send(&HttpRequest::get(&format!("http://127.0.0.1:{port}/")), 100).unwrap_err();
        assert!(err.contains("nothing is listening"), "{err}");
        assert!(send(&HttpRequest::get("http://example.com/"), 100).unwrap_err().contains("not loopback"));
    }

    #[test]
    fn header_injection_is_refused() {
        let mut r = HttpRequest::get("http://localhost:1/");
        r.headers.push(("X-A".into(), "b\r\nInjected: 1".into()));
        assert!(send(&r, 10).unwrap_err().contains("line break"));
    }
}
