//! Which page a screenshot may be taken of, and the limits on how.
//!
//! The model gets to point a browser at a URL, so the rule is narrow: **a page on this machine**. Either an
//! `http://` URL on `localhost`, `127.0.0.1` or `[::1]` (the same rule as the `http_request` tool), or the
//! port of a process this session started. Anything else — another host, `file://`, `https://` — is
//! refused before a browser is looked for.

use bennu_live::prelude::parse_local_url;
use serde_json::Value;

pub(super) const DEFAULT_WIDTH: u32 = 1280;
pub(super) const DEFAULT_HEIGHT: u32 = 800;
const MIN_SIDE: u32 = 200;
const MAX_WIDTH: u32 = 1920;
const MAX_HEIGHT: u32 = 1600;
/// The tallest a full-page capture is allowed to get: the provider refuses very tall images.
pub(super) const MAX_FULL_PAGE_HEIGHT: u32 = 4_000;

/// What to capture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Target {
    pub url: String,
    pub port: u16,
}

/// The page named by `url`, or by `port` (+ optional `path`) when the port belongs to one of `managed_ports`.
pub(super) fn resolve(input: &Value, managed_ports: &[u16]) -> Result<Target, String> {
    if let Some(url) = input.get("url").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()) {
        let parsed = parse_local_url(url).map_err(|e| format!("only a page on this machine can be captured: {e}"))?;
        return Ok(Target { url: url.to_string(), port: parsed.port });
    }
    let Some(port) = input.get("port").and_then(Value::as_u64) else {
        return Err("give `url` (http://localhost:PORT/...) or the `port` of a process you started".into());
    };
    let port = u16::try_from(port).map_err(|_| format!("`{port}` is not a port"))?;
    if !managed_ports.contains(&port) {
        let known = if managed_ports.is_empty() { "no managed process declares a port".to_string() } else { format!("the ports of managed processes are {}", managed_ports.iter().map(u16::to_string).collect::<Vec<_>>().join(", ")) };
        return Err(format!("port {port} is not one a managed process declared ({known}). Use `url` for another page on localhost."));
    }
    let path = input.get("path").and_then(Value::as_str).map(str::trim).filter(|p| !p.is_empty()).unwrap_or("/");
    let path = if path.starts_with('/') { path.to_string() } else { format!("/{path}") };
    Ok(Target { url: format!("http://localhost:{port}{path}"), port })
}

/// The window size, clamped to what is useful and what the provider reads.
pub(super) fn viewport(input: &Value) -> (u32, u32) {
    let side = |key: &str, default: u32, max: u32| {
        input.pointer(&format!("/viewport/{key}")).and_then(Value::as_u64).map(|v| (v.min(u64::from(max)) as u32).max(MIN_SIDE)).unwrap_or(default)
    };
    (side("w", DEFAULT_WIDTH, MAX_WIDTH), side("h", DEFAULT_HEIGHT, MAX_HEIGHT))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_page_on_this_machine_is_accepted() {
        for url in ["http://localhost:3000/", "http://127.0.0.1:8080/app?x=1", "http://[::1]:5173"] {
            assert!(resolve(&json!({"url": url}), &[]).is_ok(), "{url}");
        }
        assert_eq!(resolve(&json!({"url": "http://localhost:3000/a"}), &[]).unwrap().port, 3000);
    }

    #[test]
    fn anything_else_is_refused_before_a_browser_is_looked_for() {
        for url in ["https://localhost:3000/", "http://example.com/", "file:///etc/passwd", "http://192.168.0.5/", "http://user@localhost/", "javascript:alert(1)"] {
            assert!(resolve(&json!({"url": url}), &[3000]).is_err(), "{url}");
        }
    }

    #[test]
    fn a_port_must_belong_to_a_managed_process() {
        let ok = resolve(&json!({"port": 8080, "path": "admin"}), &[8080]).unwrap();
        assert_eq!(ok.url, "http://localhost:8080/admin");
        let err = resolve(&json!({"port": 9999}), &[8080]).unwrap_err();
        assert!(err.contains("8080") && err.contains("url"), "{err}");
        assert!(resolve(&json!({"port": 70000}), &[]).is_err());
        assert!(resolve(&json!({}), &[]).unwrap_err().contains("`url`"));
    }

    #[test]
    fn the_viewport_is_clamped_and_has_a_default() {
        assert_eq!(viewport(&json!({})), (1280, 800));
        assert_eq!(viewport(&json!({"viewport": {"w": 390, "h": 844}})), (390, 844));
        assert_eq!(viewport(&json!({"viewport": {"w": 99999, "h": 5}})), (1920, 200));
    }
}
