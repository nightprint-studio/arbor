//! The wizard's only network access: GETs to three fixed hosts.
//!
//! The allow-list is enforced twice — on every URL a generator builds ([`check_url`]) and on every
//! redirect the client is asked to follow — because a generator's URL is built from user input and
//! a service's redirect is built by someone else. Anything else is refused before a socket opens.

use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

use reqwest::Url;
use serde_json::Value;

pub const SPRING_HOST: &str = "start.spring.io";
pub const QUARKUS_HOST: &str = "code.quarkus.io";
pub const MICRONAUT_HOST: &str = "launch.micronaut.io";
const HOSTS: &[&str] = &[SPRING_HOST, QUARKUS_HOST, MICRONAUT_HOST];

/// The largest archive a generator may hand back. A skeleton is kilobytes; this is a ceiling for a
/// misbehaving server, not a budget.
pub const MAX_DOWNLOAD_BYTES: u64 = 64 * 1024 * 1024;

fn host_allowed(url: &Url) -> bool {
    url.scheme() == "https" && url.host_str().is_some_and(|h| HOSTS.contains(&h))
}

/// Parse `url` and refuse anything that isn't `https` on one of the three hosts.
pub fn check_url(url: &str) -> Result<Url, String> {
    let parsed = Url::parse(url).map_err(|e| format!("invalid URL: {e}"))?;
    if !host_allowed(&parsed) {
        return Err(format!("refusing to contact {}", parsed.host_str().unwrap_or("an unknown host")));
    }
    Ok(parsed)
}

fn client() -> Result<&'static reqwest::Client, String> {
    static CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .read_timeout(Duration::from_secs(30))
                .user_agent(concat!("Arbor-Bennu/", env!("CARGO_PKG_VERSION")))
                .redirect(reqwest::redirect::Policy::custom(|attempt| {
                    if attempt.previous().len() < 5 && host_allowed(attempt.url()) {
                        attempt.follow()
                    } else {
                        attempt.error("redirected to a host that is not allowed")
                    }
                }))
                .build()
                .map_err(|e| format!("could not build the HTTP client: {e}"))
        })
        .as_ref()
        .map_err(Clone::clone)
}

/// A sentence for a transport failure, in terms of what the user can do about it.
fn transport_message(host: &str, e: &reqwest::Error) -> String {
    if e.is_timeout() {
        format!("{host} did not answer in time. Check your connection and try again.")
    } else if e.is_connect() {
        format!("Could not reach {host}. You may be offline, or a proxy or firewall is blocking it.")
    } else {
        format!("The request to {host} failed: {e}")
    }
}

/// The message inside an error body — Spring and Quarkus answer `{"message": "..."}` — or the body
/// trimmed to a line.
pub fn status_message(host: &str, status: u16, body: &str) -> String {
    let detail = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| {
            ["message", "detail", "error"]
                .iter()
                .find_map(|k| v.get(*k).and_then(Value::as_str).map(str::to_string))
        })
        .unwrap_or_else(|| body.trim().chars().take(200).collect());
    if detail.is_empty() {
        format!("{host} answered with status {status}")
    } else {
        format!("{host} answered with status {status}: {detail}")
    }
}

async fn failed(host: &str, resp: reqwest::Response) -> String {
    let status = resp.status().as_u16();
    let body = resp.text().await.unwrap_or_default();
    status_message(host, status, &body)
}

/// GET `url` and parse the body as JSON.
pub async fn get_json(url: &str, accept: &str) -> Result<Value, String> {
    let parsed = check_url(url)?;
    let host = parsed.host_str().unwrap_or_default().to_string();
    let resp = client()?
        .get(parsed)
        .header("Accept", accept)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| transport_message(&host, &e))?;
    if !resp.status().is_success() {
        return Err(failed(&host, resp).await);
    }
    resp.json::<Value>().await.map_err(|e| format!("{host} sent something that isn't JSON: {e}"))
}

/// GET `url` into the file `dest`, stopping at `max_bytes` or when `cancel` is set. Returns the
/// number of bytes written.
pub async fn download_to(url: &str, dest: &Path, max_bytes: u64, cancel: &AtomicBool) -> Result<u64, String> {
    let parsed = check_url(url)?;
    let host = parsed.host_str().unwrap_or_default().to_string();
    let mut resp = client()?
        .get(parsed)
        .header("Accept", "application/zip, application/octet-stream")
        .send()
        .await
        .map_err(|e| transport_message(&host, &e))?;
    if !resp.status().is_success() {
        return Err(failed(&host, resp).await);
    }
    if resp.content_length().is_some_and(|n| n > max_bytes) {
        return Err(format!("{host} offered an archive larger than {} MB", max_bytes / (1024 * 1024)));
    }
    let mut file = std::fs::File::create(dest).map_err(|e| format!("could not write the download: {e}"))?;
    let mut total = 0u64;
    while let Some(chunk) = resp.chunk().await.map_err(|e| transport_message(&host, &e))? {
        if cancel.load(Ordering::SeqCst) {
            return Err("cancelled".into());
        }
        total += chunk.len() as u64;
        if total > max_bytes {
            return Err(format!("{host} sent more than {} MB — stopped", max_bytes / (1024 * 1024)));
        }
        file.write_all(&chunk).map_err(|e| format!("could not write the download: {e}"))?;
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_three_hosts_over_https() {
        assert!(check_url("https://start.spring.io/starter.zip?type=maven-project").is_ok());
        assert!(check_url("https://code.quarkus.io/d?g=a").is_ok());
        assert!(check_url("https://launch.micronaut.io/select-options").is_ok());
        for bad in [
            "http://start.spring.io/",
            "https://evil.example/start.spring.io",
            "https://start.spring.io.evil.example/",
            "https://user@evil.example/",
            "https://crates.io/",
            "file:///etc/passwd",
            "not a url",
        ] {
            assert!(check_url(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn userinfo_tricks_resolve_to_the_real_host() {
        // `start.spring.io@evil` — the host is `evil`, which must be refused.
        assert!(check_url("https://start.spring.io@evil.example/").is_err());
    }

    #[test]
    fn error_bodies_are_summarised() {
        assert_eq!(
            status_message("start.spring.io", 400, r#"{"message":"Invalid Spring Boot version"}"#),
            "start.spring.io answered with status 400: Invalid Spring Boot version"
        );
        assert_eq!(status_message("h", 502, ""), "h answered with status 502");
        assert!(status_message("h", 500, "plain text body").ends_with("plain text body"));
    }
}
