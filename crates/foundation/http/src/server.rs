//! The accept → read → dispatch → write loop.
//!
//! One task per connection, keep-alive within a connection, and a hard cap on both the
//! head and the body so a peer cannot make the process hold memory it will never use.
//! Nothing here knows what a route is: the handler gets a [`Request`] and returns a
//! [`Response`], and everything above this crate decides what that means.

use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;

use tokio::io::{AsyncWriteExt, BufReader, BufWriter};
use tokio::net::{TcpListener, TcpStream};

use crate::error::{HttpError, Result};
use crate::request::{read_request, Request};
use crate::response::{reason, Body, Response};

/// Per-server limits. The defaults are sized for a loopback control channel, not for
/// the open internet: generous enough for a large `tools/call` payload, small enough
/// that a stuck client is bounded.
#[derive(Debug, Clone, Copy)]
pub struct ServerConfig {
    /// Cap on the request line + headers, together.
    pub max_head_bytes: usize,
    /// Cap on `Content-Length`.
    pub max_body_bytes: usize,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self { max_head_bytes: 16 * 1024, max_body_bytes: 8 * 1024 * 1024 }
    }
}

/// A bound listener, not yet serving.
///
/// Binding is separate from serving so the caller can learn the real port (relevant
/// when it asked for `:0`) and publish it before the first request can arrive.
pub struct Server {
    listener: TcpListener,
    config: ServerConfig,
}

impl Server {
    /// Bind `addr`. Fails fast on a port conflict rather than retrying — a control
    /// channel on an unexpected port is worse than one that is plainly not up.
    pub async fn bind(addr: SocketAddr) -> Result<Self> {
        Self::bind_with(addr, ServerConfig::default()).await
    }

    /// [`Server::bind`] with non-default limits.
    pub async fn bind_with(addr: SocketAddr, config: ServerConfig) -> Result<Self> {
        let listener = TcpListener::bind(addr).await?;
        Ok(Self { listener, config })
    }

    /// The address actually bound (the resolved port when `:0` was requested).
    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.listener.local_addr()?)
    }

    /// Serve until `shutdown` resolves. In-flight connections are dropped at that
    /// point: this is a local control channel, and a clean drain would mean waiting on
    /// an SSE stream that by design never ends.
    pub async fn serve_with_shutdown<H, F, S>(self, handler: H, shutdown: S)
    where
        H: Fn(Request) -> F + Send + Sync + 'static,
        F: Future<Output = Response> + Send + 'static,
        S: Future<Output = ()> + Send,
    {
        let handler = Arc::new(handler);
        let config = self.config;
        let listener = self.listener;

        tokio::pin!(shutdown);
        loop {
            tokio::select! {
                _ = &mut shutdown => return,
                accepted = listener.accept() => {
                    let (stream, peer) = match accepted {
                        Ok(pair) => pair,
                        // A failed accept is per-connection (fd exhaustion, a peer that
                        // vanished mid-handshake); the listener is still good.
                        Err(e) => {
                            eprintln!("arbor-http: accept failed: {e}");
                            continue;
                        }
                    };
                    let handler = Arc::clone(&handler);
                    tokio::spawn(async move {
                        if let Err(e) = serve_connection(stream, handler, config).await {
                            // Peer-side sloppiness is routine on a control channel; log
                            // and move on. stderr, never stdout — a backend's stdout is
                            // the framed-IPC protocol channel.
                            eprintln!("arbor-http: connection from {peer} ended: {e}");
                        }
                    });
                }
            }
        }
    }

    /// Serve forever (until the task is dropped or aborted).
    pub async fn serve<H, F>(self, handler: H)
    where
        H: Fn(Request) -> F + Send + Sync + 'static,
        F: Future<Output = Response> + Send + 'static,
    {
        self.serve_with_shutdown(handler, std::future::pending()).await
    }
}

/// One connection: read requests until the peer stops, or until a response says stop.
async fn serve_connection<H, F>(stream: TcpStream, handler: Arc<H>, config: ServerConfig) -> Result<()>
where
    H: Fn(Request) -> F + Send + Sync + 'static,
    F: Future<Output = Response> + Send + 'static,
{
    // Nagle off: every message here is a complete small frame that wants to leave now.
    let _ = stream.set_nodelay(true);
    let (rx, tx) = stream.into_split();
    let mut reader = BufReader::new(rx);
    let mut writer = BufWriter::new(tx);

    loop {
        let request = match read_request(&mut reader, config.max_head_bytes, config.max_body_bytes).await {
            Ok(Some(r)) => r,
            Ok(None) => return Ok(()), // clean EOF between requests
            Err(e) => {
                // Malformed input still deserves an answer when the socket survives.
                if let Some(status) = e.status() {
                    let mut resp = Response::status(status);
                    resp = resp.with_header("connection", "close");
                    let _ = write_response(&mut writer, resp, false).await;
                }
                return Err(e);
            }
        };

        let client_wants_close = request
            .header("connection")
            .is_some_and(|c| c.to_ascii_lowercase().contains("close"));

        let response = handler(request).await;
        let keep_alive = !client_wants_close && !response.forces_close();

        write_response(&mut writer, response, keep_alive).await?;

        if !keep_alive {
            let _ = writer.shutdown().await;
            return Ok(());
        }
    }
}

/// Write status line, headers and body. Bytes bodies get a `Content-Length`; SSE
/// bodies get streamed until their sender drops.
async fn write_response<W>(writer: &mut BufWriter<W>, response: Response, keep_alive: bool) -> Result<()>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    let Response { status, headers, body } = response;

    let mut head = format!("HTTP/1.1 {status} {}\r\n", reason(status));
    for (name, value) in &headers {
        // A header value with a newline in it would let a caller inject a whole
        // response. There is no legitimate reason for one, so it is dropped.
        if value.contains('\r') || value.contains('\n') {
            continue;
        }
        head.push_str(name);
        head.push_str(": ");
        head.push_str(value);
        head.push_str("\r\n");
    }

    match body {
        Body::Bytes(bytes) => {
            head.push_str(&format!("content-length: {}\r\n", bytes.len()));
            head.push_str(if keep_alive { "connection: keep-alive\r\n" } else { "connection: close\r\n" });
            head.push_str("\r\n");
            writer.write_all(head.as_bytes()).await?;
            writer.write_all(&bytes).await?;
            writer.flush().await?;
        }
        Body::Sse(mut rx) => {
            // No content-length: the stream ends with the connection.
            head.push_str("connection: close\r\n\r\n");
            writer.write_all(head.as_bytes()).await?;
            writer.flush().await?;
            while let Some(event) = rx.recv().await {
                writer.write_all(event.encode().as_bytes()).await?;
                // Flush per event: an SSE consumer that gets its events in batches
                // when the buffer happens to fill is an SSE consumer with no reason
                // to exist.
                writer.flush().await?;
            }
            let _ = writer.shutdown().await;
        }
    }
    Ok(())
}

/// Convenience for the common "the peer sent something we won't serve" answer.
pub fn error_response(status: u16, message: &str) -> Response {
    Response::with_body(status, "text/plain; charset=utf-8", message.as_bytes().to_vec())
}

impl From<HttpError> for Response {
    fn from(e: HttpError) -> Self {
        error_response(e.status().unwrap_or(500), &e.to_string())
    }
}
