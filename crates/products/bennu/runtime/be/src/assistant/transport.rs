//! The HTTP side of the assistant: one POST, read as it streams.
//!
//! A run owns a small runtime of its own and blocks on it, because a run already lives on its own
//! thread (it must — it waits on the user) and a handler thread of the serve loop is not the place to
//! hold a connection open for a minute. Dropping the transport drops the connection, which is how a
//! cancel reaches the provider: it stops being read, and stops being billed for what nobody will see.

use std::time::Duration;

use bennu_assistant::prelude::{HttpRequest, Transport};

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

/// The message inside a provider's error body — `{"error":{"message":"…"}}` — or the body itself,
/// trimmed, when it is something else. The provider's own wording is the diagnosis; it is passed on.
pub(super) fn error_message(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.pointer("/error/message").and_then(|m| m.as_str()).map(str::to_string))
        .unwrap_or_else(|| body.trim().chars().take(400).collect())
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
    fn stream(&self, request: &HttpRequest, on_chunk: &mut dyn FnMut(&str) -> bool) -> Result<(), String> {
        self.rt.block_on(async {
            let mut call = self.client.post(&request.url);
            for (name, value) in &request.headers {
                call = call.header(name.as_str(), value.as_str());
            }
            let mut response = call.body(request.body.clone()).send().await.map_err(|e| {
                if e.is_connect() || e.is_timeout() {
                    format!("could not reach {}: {e}", request.url)
                } else {
                    e.to_string()
                }
            })?;

            let status = response.status();
            if !status.is_success() {
                let body = response.text().await.unwrap_or_default();
                return Err(format!("{}: {}", status.as_u16(), error_message(&body)));
            }

            let mut pending: Vec<u8> = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(|e| format!("the connection broke: {e}"))? {
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
    fn the_providers_message_is_pulled_out_of_its_error_body() {
        let body = r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#;
        assert_eq!(error_message(body), "invalid x-api-key");
    }

    #[test]
    fn a_body_that_is_not_json_is_passed_on_trimmed_and_bounded() {
        assert_eq!(error_message("  Bad Gateway \n"), "Bad Gateway");
        assert_eq!(error_message(&"x".repeat(1000)).len(), 400);
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
