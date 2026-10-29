//! The HTTP side of the assistant: one POST, read as it streams.
//!
//! A run owns a small runtime of its own and blocks on it, because a run already lives on its own
//! thread (it must — it waits on the user) and a handler thread of the serve loop is not the place to
//! hold a connection open for a minute. Dropping the transport drops the connection, which is how a
//! cancel reaches the provider: it stops being read, and stops being billed for what nobody will see.

use std::time::Duration;

use bennu_assistant::prelude::{HttpRequest, ProviderError, Transport};

/// A failure to simulate, for trying the retry and the error messages by hand without a broken network. The
/// environment variable `ARBOR_BENNU_FAKE_API_FAILURE` names it and how many requests it hits: `offline:3`,
/// `timeout:2`, `429:2`, `529:1`, `500:5`, `401:1`; the requests after those go to the real API.
const FAULT_VAR: &str = "ARBOR_BENNU_FAKE_API_FAILURE";

/// Parse `kind:count` (`count` defaults to 1). `None` for anything else, which simulates nothing.
fn parse_fault(spec: &str) -> Option<(ProviderError, u32)> {
    let (kind, count) = spec.trim().split_once(':').map_or((spec.trim(), 1), |(k, c)| (k.trim(), c.trim().parse().unwrap_or(1)));
    let error = match kind {
        "offline" => ProviderError::offline("simulated: could not reach the API"),
        "timeout" => ProviderError::timeout("simulated: the connection went silent"),
        "broken" => ProviderError::stream_broke("simulated: the connection broke"),
        status => {
            let status: u16 = status.parse().ok()?;
            let body = format!(r#"{{"type":"error","error":{{"type":"simulated","message":"simulated {status}"}}}}"#);
            ProviderError::from_response(status, (status == 429).then_some("3"), &body)
        }
    };
    Some((error, count))
}

/// The simulated failure for this request, while the budget of them lasts.
fn simulated_failure() -> Option<ProviderError> {
    static LEFT: std::sync::OnceLock<std::sync::atomic::AtomicU32> = std::sync::OnceLock::new();
    static FAULT: std::sync::OnceLock<Option<ProviderError>> = std::sync::OnceLock::new();
    let spec = std::env::var(FAULT_VAR).ok()?;
    let fault = FAULT.get_or_init(|| parse_fault(&spec).map(|(e, n)| {
        let _ = LEFT.set(std::sync::atomic::AtomicU32::new(n));
        e
    }));
    let error = fault.as_ref()?;
    let left = LEFT.get()?;
    left.fetch_update(std::sync::atomic::Ordering::SeqCst, std::sync::atomic::Ordering::SeqCst, |n| n.checked_sub(1)).ok().map(|_| error.clone())
}

pub(super) struct HttpTransport {
    rt: tokio::runtime::Runtime,
    client: reqwest::Client,
}

impl HttpTransport {
    pub(super) fn new() -> Result<Self, String> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| format!("could not start the network runtime: {e}"))?;
        // No overall timeout: a long answer is legitimately minutes. What is bounded is silence — a
        // connection that says nothing for two minutes is dead, not thinking.
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(20))
            .read_timeout(Duration::from_secs(120))
            .user_agent(concat!("Arbor-Bennu/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| format!("could not build the HTTP client: {e}"))?;
        Ok(Self { rt, client })
    }
}

/// What a failure to send (connecting, or waiting for the first byte) was: the class of network error decides
/// whether it is worth another try, the error's own text is kept for the user.
fn send_error(url: &str, e: &reqwest::Error) -> ProviderError {
    if e.is_connect() {
        ProviderError::offline(format!("could not reach {url}: {e}"))
    } else if e.is_timeout() {
        ProviderError::timeout(format!("no answer from {url}: {e}"))
    } else if e.is_builder() {
        ProviderError::local(e.to_string())
    } else {
        // A reset or a refused TLS handshake midway: the connection broke before the answer began.
        ProviderError::stream_broke(e.to_string())
    }
}

/// A break while the answer streams: silence past the read timeout is a timeout, anything else a dropped connection.
fn read_error(e: &reqwest::Error) -> ProviderError {
    if e.is_timeout() {
        ProviderError::timeout(format!("the connection went silent: {e}"))
    } else {
        ProviderError::stream_broke(format!("the connection broke: {e}"))
    }
}

/// The longest prefix of `pending` that is whole UTF-8, removed from it and returned. A network chunk
/// can end in the middle of a character; the rest stays for the next one.
pub(super) fn take_valid_utf8(pending: &mut Vec<u8>) -> String {
    let valid = match std::str::from_utf8(pending) {
        Ok(s) => s.len(),
        Err(e) => e.valid_up_to(),
    };
    String::from_utf8_lossy(&pending.drain(..valid).collect::<Vec<u8>>()).into_owned()
}

impl Transport for HttpTransport {
    fn stream(&self, request: &HttpRequest, on_chunk: &mut dyn FnMut(&str) -> bool) -> Result<(), ProviderError> {
        if let Some(error) = simulated_failure() {
            return Err(error);
        }
        self.rt.block_on(async {
            let mut call = self.client.post(&request.url);
            for (name, value) in &request.headers {
                call = call.header(name.as_str(), value.as_str());
            }
            let mut response = call.body(request.body.clone()).send().await.map_err(|e| send_error(&request.url, &e))?;

            let status = response.status();
            if !status.is_success() {
                let retry_after = response.headers().get("retry-after").and_then(|v| v.to_str().ok()).map(str::to_string);
                let body = response.text().await.unwrap_or_default();
                return Err(ProviderError::from_response(status.as_u16(), retry_after.as_deref(), &body));
            }

            let mut pending: Vec<u8> = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(|e| read_error(&e))? {
                pending.extend_from_slice(&chunk);
                let text = take_valid_utf8(&mut pending);
                if !text.is_empty() && !on_chunk(&text) {
                    return Ok(());
                }
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_providers_message_is_kept_in_the_error() {
        let body = r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#;
        let e = ProviderError::from_response(401, None, body);
        assert!(e.detail.contains("invalid x-api-key"), "{}", e.detail);
    }

    #[test]
    fn a_body_that_is_not_json_is_passed_on_trimmed_and_bounded() {
        assert!(ProviderError::from_response(502, None, "  Bad Gateway \n").detail.ends_with("Bad Gateway"));
        assert!(ProviderError::from_response(502, None, &"x".repeat(1000)).detail.len() < 410);
    }

    #[test]
    fn the_simulated_failure_is_read_from_kind_and_count() {
        let (e, n) = parse_fault("offline:3").unwrap();
        assert_eq!((e.kind, n), (bennu_assistant::prelude::ErrorKind::Offline, 3));
        let (e, n) = parse_fault("429:2").unwrap();
        assert_eq!((e.kind, n), (bennu_assistant::prelude::ErrorKind::RateLimited, 2));
        assert!(e.retry_after.is_some());
        assert_eq!(parse_fault("401").unwrap().1, 1);
        assert_eq!(parse_fault("401").unwrap().0.kind, bennu_assistant::prelude::ErrorKind::Auth);
        assert!(parse_fault("bogus:1").is_none());
    }

    #[test]
    fn a_chunk_cut_inside_a_character_waits_for_the_rest() {
        let bytes = "è".as_bytes(); // two bytes
        let mut pending = vec![b'a', bytes[0]];
        assert_eq!(take_valid_utf8(&mut pending), "a");
        assert_eq!(pending, vec![bytes[0]], "the half character is kept");
        pending.push(bytes[1]);
        assert_eq!(take_valid_utf8(&mut pending), "è");
        assert!(pending.is_empty());
    }
}
