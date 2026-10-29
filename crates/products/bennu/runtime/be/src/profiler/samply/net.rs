//! The installer's only network access: GETs to GitHub's release hosts.
//!
//! The allow-list ([`super::asset::host_allowed`]) is enforced on every URL and on every redirect
//! the client is asked to follow (a release asset is served through a CDN redirect, and a redirect
//! is chosen by someone else). The archive is hashed as it streams, so the digest costs no second
//! pass over the file.

use std::io::Write;
use std::path::Path;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use reqwest::Url;
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::asset::host_allowed;

fn client() -> Result<&'static reqwest::Client, String> {
    static CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .read_timeout(Duration::from_secs(30))
                .user_agent(concat!("Arbor-Bennu/", env!("CARGO_PKG_VERSION")))
                .redirect(reqwest::redirect::Policy::custom(|attempt| {
                    if attempt.previous().len() < 5 && host_allowed(attempt.url().scheme(), attempt.url().host_str()) {
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

fn check_url(url: &str) -> Result<Url, String> {
    let parsed = Url::parse(url).map_err(|e| format!("invalid URL: {e}"))?;
    if !host_allowed(parsed.scheme(), parsed.host_str()) {
        return Err(format!("refusing to contact {}", parsed.host_str().unwrap_or("an unknown host")));
    }
    Ok(parsed)
}

fn transport_message(host: &str, e: &reqwest::Error) -> String {
    if e.is_timeout() {
        format!("{host} did not answer in time. Check your connection and try again.")
    } else if e.is_connect() {
        format!("Could not reach {host}. You may be offline, or a proxy or firewall is blocking it.")
    } else {
        format!("The request to {host} failed: {e}")
    }
}

async fn get(url: &str, accept: &str) -> Result<(reqwest::Response, String), String> {
    let parsed = check_url(url)?;
    let host = parsed.host_str().unwrap_or_default().to_string();
    let resp = client()?.get(parsed).header("Accept", accept).send().await.map_err(|e| transport_message(&host, &e))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let hint = if status.as_u16() == 403 || status.as_u16() == 429 {
            " (GitHub rate-limits anonymous requests; try again later)"
        } else {
            ""
        };
        return Err(format!("{host} answered with status {status}{hint}"));
    }
    Ok((resp, host))
}

pub async fn get_json(url: &str) -> Result<Value, String> {
    let (resp, host) = get(url, "application/vnd.github+json").await?;
    resp.json::<Value>().await.map_err(|e| format!("{host} sent something that isn't JSON: {e}"))
}

/// A small text file (a checksum). Anything over `max` bytes is refused.
pub async fn get_text(url: &str, max: usize) -> Result<String, String> {
    let (resp, host) = get(url, "text/plain, application/octet-stream").await?;
    let bytes = resp.bytes().await.map_err(|e| transport_message(&host, &e))?;
    if bytes.len() > max {
        return Err(format!("{host} sent an unexpectedly large checksum file"));
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Stream `url` into `dest`, stopping at `max_bytes`. `progress(done, total)` is called at most
/// every 100 ms. Returns the lowercase hex SHA-256 of what was written.
pub async fn download(
    url: &str,
    dest: &Path,
    max_bytes: u64,
    progress: &(dyn Fn(u64, Option<u64>) + Send + Sync),
) -> Result<String, String> {
    let (mut resp, host) = get(url, "application/octet-stream").await?;
    let total = resp.content_length();
    if total.is_some_and(|n| n > max_bytes) {
        return Err(format!("{host} offered a file larger than {} MB", max_bytes >> 20));
    }
    let mut file = std::fs::File::create(dest).map_err(|e| format!("could not write the download: {e}"))?;
    let (mut hasher, mut done, mut last) = (Sha256::new(), 0u64, Instant::now());
    while let Some(chunk) = resp.chunk().await.map_err(|e| transport_message(&host, &e))? {
        done += chunk.len() as u64;
        if done > max_bytes {
            return Err(format!("{host} sent more than {} MB — stopped", max_bytes >> 20));
        }
        hasher.update(&chunk);
        file.write_all(&chunk).map_err(|e| format!("could not write the download: {e}"))?;
        if last.elapsed() >= Duration::from_millis(100) {
            progress(done, total);
            last = Instant::now();
        }
    }
    file.flush().map_err(|e| format!("could not write the download: {e}"))?;
    progress(done, total);
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_github_over_https_is_contacted() {
        assert!(check_url("https://api.github.com/repos/mstange/samply/releases/latest").is_ok());
        assert!(check_url("https://github.com/mstange/samply/releases/download/t/f.zip").is_ok());
        for bad in ["http://github.com/x", "https://evil.example/", "https://github.com@evil.example/", "file:///etc/passwd"] {
            assert!(check_url(bad).is_err(), "{bad}");
        }
    }
}
