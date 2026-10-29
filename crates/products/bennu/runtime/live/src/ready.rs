//! What "the service is up" means: a port that accepts connections, a log line, an HTTP health URL, a
//! fixed delay — and a timeout after which waiting stops being useful.
//!
//! Checks combine with AND (a service that is listening but has not logged "Started" is not ready if
//! the recipe says both). The pure parts (the regex over log text) live here; the process-bound loop
//! that polls them is `process::Manager::wait_ready`.

use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use regex::Regex;

use crate::http::{parse_local_url, send, HttpRequest};

/// One condition.
#[derive(Debug, Clone)]
pub enum Check {
    /// Something accepts connections on this loopback port.
    Port(u16),
    /// Some line the process wrote matches.
    Log(Regex),
    /// A GET answers with this status (any 2xx when `None`).
    Http { url: String, status: Option<u16> },
    /// This long after start, whatever else is true.
    Delay(Duration),
}

impl Check {
    /// A short description for status lines and timeouts.
    pub fn describe(&self) -> String {
        match self {
            Check::Port(p) => format!("port {p} accepting connections"),
            Check::Log(re) => format!("log line matching /{}/", re.as_str()),
            Check::Http { url, status } => match status {
                Some(s) => format!("GET {url} answering {s}"),
                None => format!("GET {url} answering 2xx"),
            },
            Check::Delay(d) => format!("{} ms elapsed", d.as_millis()),
        }
    }
}

/// All of `checks` must hold within `timeout`.
#[derive(Debug, Clone)]
pub struct Readiness {
    pub checks: Vec<Check>,
    pub timeout: Duration,
}

impl Default for Readiness {
    /// No checks: ready as soon as it is running.
    fn default() -> Self {
        Self { checks: Vec::new(), timeout: Duration::from_secs(60) }
    }
}

/// The default wait when a recipe gives a check but no timeout.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

/// The most a single wait may last, whatever was asked.
pub const MAX_TIMEOUT: Duration = Duration::from_secs(600);

/// Does anything accept connections on `port`? IPv4 and IPv6 loopback both count (`localhost` may
/// resolve to either, and a Node dev server binds `::1` only).
pub fn port_open(port: u16) -> bool {
    let wait = Duration::from_millis(250);
    let v4: SocketAddr = ([127, 0, 0, 1], port).into();
    let v6: SocketAddr = (std::net::Ipv6Addr::LOCALHOST, port).into();
    TcpStream::connect_timeout(&v4, wait).is_ok() || TcpStream::connect_timeout(&v6, wait).is_ok()
}

/// Does the health URL answer as wanted? A refused connection is simply "not yet".
pub fn http_ok(url: &str, status: Option<u16>) -> bool {
    if parse_local_url(url).is_err() {
        return false;
    }
    let mut req = HttpRequest::get(url);
    req.timeout_ms = 1500;
    match send(&req, 2048) {
        Ok(r) => match status {
            Some(s) => r.status == s,
            None => (200..300).contains(&r.status),
        },
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn a_listening_port_is_open_and_a_closed_one_is_not() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        assert!(port_open(port));
        drop(l);
        assert!(!port_open(port));
    }

    #[test]
    fn a_remote_health_url_is_never_probed() {
        assert!(!http_ok("http://example.com/health", None));
    }

    #[test]
    fn checks_describe_themselves() {
        assert_eq!(Check::Port(8080).describe(), "port 8080 accepting connections");
        assert!(Check::Log(Regex::new("Started .*").unwrap()).describe().contains("Started"));
        assert!(Check::Http { url: "http://localhost/h".into(), status: Some(204) }.describe().contains("204"));
    }
}
