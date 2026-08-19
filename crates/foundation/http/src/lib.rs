//! `arbor-http` — the minimal HTTP/1.1 server behind Arbor's loopback endpoints.
//!
//! Arbor needs to *listen* on HTTP in exactly two places: the MCP endpoint the AI client
//! POSTs to, and the OAuth redirect a browser lands on. Both are localhost, both are
//! low-traffic, and both were (or would have been) hand-rolled independently — the OAuth
//! flow in `arbor-auth` already parses a request out of a byte buffer by hand. This crate
//! is that code, done once, with the parts those two cases actually need:
//!
//! - [`Request`] — method, path, query, headers, `Content-Length` body.
//! - [`Response`] — status, headers, and a body that is either bytes or a live
//!   [`SseEvent`] stream.
//! - [`Server`] — bind, then serve a `Fn(Request) -> Future<Response>` with keep-alive
//!   and per-connection tasks.
//!
//! What it is **not**: a framework. No routing, no extractors, no middleware, no TLS, no
//! chunked encoding, no HTTP/2. Anything reachable from outside the machine belongs
//! behind a real server, not behind this.
//!
//! ## Public API: use the [`prelude`]

pub mod error;
pub mod prelude;
pub mod request;
pub mod response;
pub mod server;

pub use error::{HttpError, Result};
pub use request::{percent_decode, Request};
pub use response::{Body, Response, SseEvent};
pub use server::{error_response, Server, ServerConfig};

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    /// Bind, serve an echo handler, and speak HTTP at it by hand.
    #[tokio::test]
    async fn round_trips_a_post_over_a_real_socket() {
        let server = Server::bind("127.0.0.1:0".parse().unwrap()).await.unwrap();
        let addr = server.local_addr().unwrap();
        let handle = tokio::spawn(server.serve(|req: Request| async move {
            Response::json(format!(r#"{{"path":"{}","body":"{}"}}"#, req.path, req.body_str()))
        }));

        let mut client = TcpStream::connect(addr).await.unwrap();
        client
            .write_all(b"POST /mcp HTTP/1.1\r\nHost: x\r\nContent-Length: 2\r\nConnection: close\r\n\r\nhi")
            .await
            .unwrap();
        let mut out = String::new();
        client.read_to_string(&mut out).await.unwrap();

        assert!(out.starts_with("HTTP/1.1 200 OK\r\n"), "{out}");
        assert!(out.contains(r#""path":"/mcp""#), "{out}");
        assert!(out.contains(r#""body":"hi""#), "{out}");
        handle.abort();
    }

    /// Two requests on one connection — the keep-alive path, which is what an MCP
    /// client actually does between `tools/list` and `tools/call`.
    #[tokio::test]
    async fn keeps_the_connection_alive_between_requests() {
        let server = Server::bind("127.0.0.1:0".parse().unwrap()).await.unwrap();
        let addr = server.local_addr().unwrap();
        let handle = tokio::spawn(server.serve(|_req: Request| async move { Response::text("ok") }));

        let mut client = TcpStream::connect(addr).await.unwrap();
        let mut seen = 0;
        for _ in 0..2 {
            client.write_all(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n").await.unwrap();
            let mut buf = [0u8; 512];
            let n = client.read(&mut buf).await.unwrap();
            let text = String::from_utf8_lossy(&buf[..n]);
            assert!(text.contains("200 OK"), "{text}");
            seen += 1;
        }
        assert_eq!(seen, 2);
        handle.abort();
    }

    /// The stream arm: headers first, then events as the sender produces them.
    #[tokio::test]
    async fn streams_server_sent_events() {
        let server = Server::bind("127.0.0.1:0".parse().unwrap()).await.unwrap();
        let addr = server.local_addr().unwrap();
        let handle = tokio::spawn(server.serve(|_req: Request| async move {
            let (tx, rx) = tokio::sync::mpsc::channel(4);
            tokio::spawn(async move {
                let _ = tx.send(SseEvent::data("one")).await;
                let _ = tx.send(SseEvent::data("two").named("tick")).await;
            });
            Response::sse(rx)
        }));

        let mut client = TcpStream::connect(addr).await.unwrap();
        client.write_all(b"GET /events HTTP/1.1\r\nHost: x\r\n\r\n").await.unwrap();
        let mut out = String::new();
        client.read_to_string(&mut out).await.unwrap();

        assert!(out.contains("content-type: text/event-stream"), "{out}");
        assert!(out.contains("data: one\n\n"), "{out}");
        assert!(out.contains("event: tick\ndata: two\n\n"), "{out}");
        handle.abort();
    }

    /// A malformed request still gets an answer, not a silent drop.
    #[tokio::test]
    async fn malformed_request_gets_a_status() {
        let server = Server::bind("127.0.0.1:0".parse().unwrap()).await.unwrap();
        let addr = server.local_addr().unwrap();
        let handle = tokio::spawn(server.serve(|_req: Request| async move { Response::text("unreachable") }));

        let mut client = TcpStream::connect(addr).await.unwrap();
        client.write_all(b"GARBAGE\r\n\r\n").await.unwrap();
        let mut out = String::new();
        client.read_to_string(&mut out).await.unwrap();
        assert!(out.starts_with("HTTP/1.1 400 Bad Request"), "{out}");
        handle.abort();
    }
}
