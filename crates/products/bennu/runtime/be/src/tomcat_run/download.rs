//! Fetching Tomcat: the list of releases, and a release's archive.
//!
//! The only hosts contacted are `archive.apache.org` (Tomcat) and `repo1.maven.org` (a JDBC driver),
//! over HTTPS — on every request and on every redirect, because a redirect is chosen by whoever
//! answered. A file is hashed as it streams and compared with the SHA-512 published beside it; a
//! download that does not match is deleted, never used.

use std::io::Write;
use std::path::Path;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha512};

const HOSTS: &[&str] = &["archive.apache.org", "repo1.maven.org"];

fn host_allowed(scheme: &str, host: Option<&str>) -> bool {
    scheme == "https" && host.is_some_and(|h| HOSTS.contains(&h))
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
                    if attempt.previous().len() < 3 && host_allowed(attempt.url().scheme(), attempt.url().host_str()) {
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

async fn get(url: &str) -> Result<reqwest::Response, String> {
    let parsed = reqwest::Url::parse(url).map_err(|e| format!("invalid URL: {e}"))?;
    if !host_allowed(parsed.scheme(), parsed.host_str()) {
        return Err(format!("refusing to contact {}", parsed.host_str().unwrap_or("an unknown host")));
    }
    let host = parsed.host_str().unwrap_or("the server").to_string();
    let resp = client()?.get(parsed).send().await.map_err(|e| {
        if e.is_timeout() {
            format!("{host} did not answer in time. Check your connection and try again.")
        } else if e.is_connect() {
            format!("Could not reach {host}. You may be offline, or a proxy or firewall is blocking it.")
        } else {
            format!("The request to {host} failed: {e}")
        }
    })?;
    if !resp.status().is_success() {
        return Err(format!("{host} answered with status {}", resp.status()));
    }
    Ok(resp)
}

/// The host of `url`, for a sentence; a URL that does not parse is `the server`.
fn host_of(url: &str) -> String {
    reqwest::Url::parse(url).ok().and_then(|u| u.host_str().map(str::to_string)).unwrap_or_else(|| "the server".into())
}

/// A small text resource (a directory listing, a checksum). Anything over `max` bytes is refused.
pub(super) async fn get_text(url: &str, max: usize) -> Result<String, String> {
    let host = host_of(url);
    let bytes = get(url).await?.bytes().await.map_err(|e| format!("reading from {host} failed: {e}"))?;
    if bytes.len() > max {
        return Err(format!("{host} sent an unexpectedly large answer"));
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Stream `url` into `dest`, stopping at `max_bytes`; `progress(done, total)` at most every 100 ms.
/// Returns the lowercase hex SHA-512 of what was written.
pub(super) async fn download(
    url: &str,
    dest: &Path,
    max_bytes: u64,
    progress: &(dyn Fn(u64, Option<u64>) + Send + Sync),
) -> Result<String, String> {
    let host = host_of(url);
    let mut resp = get(url).await?;
    let total = resp.content_length();
    if total.is_some_and(|n| n > max_bytes) {
        return Err(format!("{host} offered a file larger than {} MB", max_bytes >> 20));
    }
    let mut file = std::fs::File::create(dest).map_err(|e| format!("could not write the download: {e}"))?;
    let (mut hasher, mut done, mut last) = (Sha512::new(), 0u64, Instant::now());
    while let Some(chunk) = resp.chunk().await.map_err(|e| format!("the download was interrupted: {e}"))? {
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
    fn only_the_apache_archive_and_maven_central_over_https_are_contacted() {
        assert!(host_allowed("https", Some("archive.apache.org")));
        assert!(host_allowed("https", Some("repo1.maven.org")));
        for (scheme, host) in [("http", Some("archive.apache.org")), ("https", Some("evil.example")), ("https", Some("archive.apache.org.evil.example")), ("https", None)] {
            assert!(!host_allowed(scheme, host), "{scheme} {host:?}");
        }
    }
}
