//! Previewing and sending a request.
//!
//! [`context::build`](crate::context::build) turns a request and its surroundings into one `Prepared`
//! value; the preview shows it, "copy as cURL" prints it and this module sends it — so what the window
//! shows is what goes on the wire. A request that still has an unresolved `{{variable}}` is never sent, and
//! one aimed at a protected environment is not sent until the user has confirmed it.
//!
//! Sending is async and cancellable: `request_cancel(request_id)` stops the request in flight (the
//! connection is dropped, so the server sees it go).

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use base64::Engine;
use hirundo_core::prelude::HirundoState;
use hirundo_model::prelude::{boundary_for, content_type_header, encode_multipart, file_name, guess_content_type, Encoded, FormPart, Method, Prepared};
use serde::{Deserialize, Serialize};
use tokio::sync::Notify;

use crate::context::{build, EnvBrief, Target};

/// The largest response body kept. Past it the rest is dropped and the response says so.
const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;

pub(crate) static IN_FLIGHT: LazyLock<Mutex<HashMap<String, Arc<Notify>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Serialize)]
pub struct PreviewOut {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
    /// Variables with no value: the request cannot be sent while there are any.
    pub unresolved: Vec<String>,
    /// Secrets declared but not in the keyring.
    pub missing_secrets: Vec<String>,
    pub env: Option<EnvBrief>,
    pub needs_confirmation: bool,
}

/// What would be sent, and in which environment — without sending it.
#[arbor_rpc::handler]
fn request_preview(state: &HirundoState, target: Target) -> Result<PreviewOut, String> {
    let built = build(state, &target, true)?;
    let p = built.prepared;
    Ok(PreviewOut {
        method: p.method,
        url: p.url,
        headers: p.headers,
        body: p.body,
        unresolved: p.unresolved,
        missing_secrets: built.resolved.missing_secrets,
        env: built.env,
        needs_confirmation: built.needs_confirmation,
    })
}

#[derive(Debug, Deserialize)]
pub struct SendArgs {
    pub target: Target,
    /// Names this send so `request_cancel` can stop it.
    pub request_id: String,
    /// The user has said yes to a protected environment.
    #[serde(default)]
    pub confirmed: bool,
}

#[derive(Debug, Serialize)]
pub struct ResponseOut {
    pub status: u16,
    pub status_text: String,
    pub http_version: String,
    pub headers: Vec<(String, String)>,
    /// The body as text (lossy when it is not UTF-8).
    pub body: String,
    /// The exact bytes, base64, when the body is not valid UTF-8 (an image, a PDF).
    pub body_base64: Option<String>,
    pub size_bytes: usize,
    /// More came than was kept.
    pub truncated: bool,
    pub time_ms: u64,
    pub final_url: String,
}

#[derive(Debug, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum SendResult {
    Sent { response: ResponseOut },
    /// Not sent: these variables have no value (or these secrets are not in the keyring).
    Unresolved { names: Vec<String>, missing_secrets: Vec<String> },
    /// Not sent yet: the environment is protected and the method changes things. Ask, then send again with `confirmed`.
    NeedsConfirmation { env: Option<EnvBrief>, method: Method, url: String },
}

#[arbor_rpc::handler]
async fn request_send(state: &HirundoState, args: SendArgs) -> Result<SendResult, String> {
    let built = build(state, &args.target, true)?;
    if !built.prepared.unresolved.is_empty() || !built.resolved.missing_secrets.is_empty() {
        return Ok(SendResult::Unresolved { names: built.prepared.unresolved, missing_secrets: built.resolved.missing_secrets });
    }
    if built.needs_confirmation && !args.confirmed {
        return Ok(SendResult::NeedsConfirmation { env: built.env, method: built.prepared.method, url: built.prepared.url });
    }

    // The environment's cookie jar: its cookies go out with the request, the ones the answer sets are kept.
    let project = args.target.project.clone();
    let env_key = built.env.as_ref().map(|e| e.id.clone()).unwrap_or_default();
    let mut prepared = built.prepared.clone();
    if prepared.cookie_jar {
        crate::cookie_store::attach(&project, &env_key, &mut prepared);
    }

    let cancel = Arc::new(Notify::new());
    IN_FLIGHT.lock().unwrap_or_else(|p| p.into_inner()).insert(args.request_id.clone(), Arc::clone(&cancel));
    let outcome = tokio::select! {
        r = execute(&prepared) => r,
        _ = cancel.notified() => Err("cancelled".to_string()),
    };
    IN_FLIGHT.lock().unwrap_or_else(|p| p.into_inner()).remove(&args.request_id);
    if let (true, Ok(response)) = (prepared.cookie_jar, &outcome) {
        crate::cookie_store::keep(&project, &env_key, &response.final_url, &response.headers);
    }
    outcome.map(|response| SendResult::Sent { response })
}

/// Stop a request in flight. False when it already finished.
#[arbor_rpc::handler]
fn request_cancel(_state: &HirundoState, request_id: String) -> Result<bool, String> {
    let found = IN_FLIGHT.lock().unwrap_or_else(|p| p.into_inner()).get(&request_id).cloned();
    Ok(found.map(|n| n.notify_one()).is_some())
}

fn to_reqwest_method(m: Method) -> reqwest::Method {
    match m {
        Method::Get => reqwest::Method::GET,
        Method::Post => reqwest::Method::POST,
        Method::Put => reqwest::Method::PUT,
        Method::Patch => reqwest::Method::PATCH,
        Method::Delete => reqwest::Method::DELETE,
        Method::Head => reqwest::Method::HEAD,
        Method::Options => reqwest::Method::OPTIONS,
    }
}

pub(crate) async fn execute(p: &Prepared) -> Result<ResponseOut, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(p.timeout_ms.max(1)))
        .redirect(if p.follow_redirects { reqwest::redirect::Policy::limited(10) } else { reqwest::redirect::Policy::none() })
        .danger_accept_invalid_certs(!p.verify_tls)
        .user_agent(concat!("Hirundo/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| format!("could not build the HTTP client: {e}"))?;

    let mut request = client.request(to_reqwest_method(p.method), &p.url);
    for (name, value) in &p.headers {
        request = request.header(name.as_str(), value.as_str());
    }
    if let Some(parts) = &p.multipart {
        let (content_type, bytes) = build_multipart(parts)?;
        if !p.headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("content-type")) {
            request = request.header("Content-Type", content_type);
        }
        request = request.body(bytes);
    } else if let Some(body) = &p.body {
        request = request.body(body.clone());
    }

    let started = Instant::now();
    let mut response = request.send().await.map_err(|e| describe_error(&e, &p.url))?;
    let status = response.status();
    let http_version = format!("{:?}", response.version());
    let final_url = response.url().to_string();
    let headers: Vec<(String, String)> = response.headers().iter().map(|(k, v)| (k.to_string(), String::from_utf8_lossy(v.as_bytes()).into_owned())).collect();

    let mut bytes: Vec<u8> = Vec::new();
    let mut total = 0usize;
    while let Some(chunk) = response.chunk().await.map_err(|e| describe_error(&e, &p.url))? {
        total += chunk.len();
        if bytes.len() < MAX_BODY_BYTES {
            let room = MAX_BODY_BYTES - bytes.len();
            bytes.extend_from_slice(&chunk[..chunk.len().min(room)]);
        }
    }
    let time_ms = started.elapsed().as_millis() as u64;

    let (body, body_base64) = decode_body(&bytes);
    Ok(ResponseOut {
        status: status.as_u16(),
        status_text: status.canonical_reason().unwrap_or("").to_string(),
        http_version,
        headers,
        body,
        body_base64,
        size_bytes: total,
        truncated: total > bytes.len(),
        time_ms,
        final_url,
    })
}

/// The multipart body: the files read, the parts framed. Returns the `Content-Type` (with its boundary) and the bytes.
fn build_multipart(parts: &[FormPart]) -> Result<(String, Vec<u8>), String> {
    /// Files larger than this in total are refused rather than held in memory.
    const MAX_TOTAL: u64 = 64 * 1024 * 1024;
    let mut total = 0u64;
    let mut encoded = Vec::new();
    for part in parts {
        let (data, filename, content_type) = match &part.file {
            Some(path) => {
                let size = std::fs::metadata(path).map_err(|e| format!("could not read the file {path}: {e}"))?.len();
                total += size;
                if total > MAX_TOTAL {
                    return Err("the files of the form are larger than 64 MB in all".into());
                }
                let data = std::fs::read(path).map_err(|e| format!("could not read the file {path}: {e}"))?;
                (data, Some(file_name(path)), Some(part.content_type.clone().unwrap_or_else(|| guess_content_type(path).to_string())))
            }
            None => (part.value.clone().into_bytes(), None, part.content_type.clone()),
        };
        encoded.push(Encoded { name: part.name.clone(), filename, content_type, data });
    }
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0);
    let boundary = boundary_for(nanos);
    Ok((content_type_header(&boundary), encode_multipart(&encoded, &boundary)))
}

/// The body as text, and — only when it is not UTF-8 — the exact bytes, so an image or a PDF survives.
fn decode_body(bytes: &[u8]) -> (String, Option<String>) {
    match std::str::from_utf8(bytes) {
        Ok(text) => (text.to_string(), None),
        Err(_) => (String::from_utf8_lossy(bytes).into_owned(), Some(base64::engine::general_purpose::STANDARD.encode(bytes))),
    }
}

/// A failure in words a person can act on.
fn describe_error(e: &reqwest::Error, url: &str) -> String {
    if e.is_timeout() {
        format!("The request timed out ({url}). Raise the timeout in the request settings if the server is slow.")
    } else if e.is_connect() {
        format!("Could not connect to {url}: {e}. Is the server running, and is the address right?")
    } else if e.is_redirect() {
        format!("Too many redirects from {url}.")
    } else if e.is_builder() {
        format!("The request is not valid: {e}")
    } else {
        format!("{e}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_text_body_is_text_and_a_binary_one_keeps_its_bytes() {
        assert_eq!(decode_body("ciao è".as_bytes()), ("ciao è".to_string(), None));
        let png = [0x89, b'P', b'N', b'G', 0xFF, 0xFE];
        let (text, b64) = decode_body(&png);
        assert!(text.contains('\u{FFFD}'));
        assert_eq!(base64::engine::general_purpose::STANDARD.decode(b64.unwrap()).unwrap(), png);
    }

    #[test]
    fn every_method_maps_to_the_same_word() {
        for m in Method::ALL {
            assert_eq!(to_reqwest_method(m).as_str(), m.as_str());
        }
    }

    #[test]
    fn cancelling_a_request_that_is_not_in_flight_says_so() {
        let notify = Arc::new(Notify::new());
        IN_FLIGHT.lock().unwrap().insert("r1".into(), Arc::clone(&notify));
        let found = IN_FLIGHT.lock().unwrap().get("r1").cloned();
        assert!(found.is_some());
        IN_FLIGHT.lock().unwrap().remove("r1");
        assert!(IN_FLIGHT.lock().unwrap().get("r1").is_none());
    }

    #[tokio::test]
    async fn a_request_that_cannot_connect_fails_in_words() {
        let p = Prepared {
            method: Method::Get,
            url: "http://127.0.0.1:1/never".into(),
            headers: vec![],
            body: None,
            multipart: None,
            cookie_jar: false,
            timeout_ms: 2000,
            follow_redirects: true,
            verify_tls: true,
            unresolved: vec![],
        };
        let err = execute(&p).await.unwrap_err();
        assert!(err.contains("connect") || err.contains("timed out") || !err.is_empty(), "{err}");
    }
}
