//! HTTP requests in logs: access-log lines (Apache/Tomcat/Undertow/Quarkus common format, actix) and
//! tower-http / axum traces (`method=GET uri=/x ... status=200 latency=3 ms`). One line, one request:
//! method, path, status and, when the line has it, how long it took.

use std::sync::LazyLock;

use regex::Regex;

use crate::logrecord::Record;
use crate::logtime::to_ms;

#[derive(Debug, Clone, PartialEq)]
pub struct Access {
    pub method: String,
    /// The path without its query, with ids replaced by `{id}`: the endpoint, not the call.
    pub endpoint: String,
    pub status: u16,
    pub ms: Option<f64>,
}

const METHODS: &str = "GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS|TRACE|CONNECT";

static CLF: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!(r#""({METHODS}) (\S+)(?: HTTP/[\d.]+)?" (\d{{3}})\b"#)).expect("clf regex"));
static TOWER_METHOD: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!(r"\bmethod=({METHODS})\b")).expect("method regex"));
static TOWER_URI: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\buri=(\S+?)[\s}]").expect("uri regex"));
static TOWER_STATUS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\bstatus[=:]\s*(\d{3})\b|Status\((\d{3})\)").expect("status regex"));
static UNIT_DURATION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\d+(?:\.\d+)?)\s*(ms|µs|us|s)\b").expect("duration regex"));
static TRAILING_SECONDS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#""\s+(\d+\.\d+)\s*$"#).expect("seconds regex"));

fn is_id_segment(seg: &str) -> bool {
    let digits = seg.chars().filter(char::is_ascii_digit).count();
    let hex_dash = seg.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
    (!seg.is_empty() && digits == seg.len())
        || (seg.len() == 36 && seg.matches('-').count() == 4 && hex_dash)
        || (seg.len() >= 16 && seg.chars().all(|c| c.is_ascii_hexdigit()))
}

/// `/orders/17/items/9?x=1` -> `/orders/{id}/items/{id}`.
pub fn endpoint_of(uri: &str) -> String {
    let path = uri.split(['?', '#']).next().unwrap_or("");
    // A full URL (a proxy log, a client trace) keeps only its path.
    let path = path.find("://").map(|i| path[i + 3..].find('/').map(|j| &path[i + 3 + j..]).unwrap_or("/")).unwrap_or(path);
    let segments: Vec<String> = path.split('/').map(|s| if is_id_segment(s) { "{id}".to_string() } else { s.to_string() }).collect();
    let joined = segments.join("/");
    if joined.is_empty() { "/".into() } else { joined }
}

fn duration_after(text: &str) -> Option<f64> {
    if let Some(c) = UNIT_DURATION.captures(text) {
        return Some(to_ms(c[1].parse().ok()?, &c[2]));
    }
    // actix `%T`: a bare number of seconds after the quoted referer and user agent.
    TRAILING_SECONDS.captures(text).and_then(|c| c[1].parse::<f64>().ok()).map(|s| s * 1000.0)
}

/// The request a record logs, if it logs one.
pub fn extract(rec: &Record) -> Option<Access> {
    let head = rec.head.as_str();
    if let Some(c) = CLF.captures(head) {
        let end = c.get(0)?.end();
        return Some(Access {
            method: c[1].to_string(),
            endpoint: endpoint_of(&c[2]),
            status: c[3].parse().ok()?,
            ms: duration_after(&head[end..]),
        });
    }
    let method = TOWER_METHOD.captures(head)?[1].to_string();
    let uri = TOWER_URI.captures(head)?[1].to_string();
    let status = TOWER_STATUS.captures(head).and_then(|c| c.get(1).or(c.get(2))?.as_str().parse().ok())?;
    let ms = head.find("latency=").and_then(|i| duration_after(&head[i + 8..]));
    Some(Access { method, endpoint: endpoint_of(&uri), status, ms })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logbuf::Stream;
    use crate::logrecord::{build_records, RawLine};
    use arbor_logscan::prelude::RuleSet;

    fn record(text: &str) -> Record {
        let lines = [RawLine { seq: 1, ts_ms: None, stream: Stream::Stdout, text: text.to_string() }];
        build_records(&RuleSet::java(), &lines).remove(0)
    }

    #[test]
    fn an_axum_tower_http_trace_is_a_request() {
        let r = record("2026-08-05T12:33:01.123456Z  INFO request{method=GET uri=/api/orders/17?expand=items version=HTTP/1.1}: tower_http::trace::on_response: finished processing request latency=3 ms status=200");
        assert_eq!(extract(&r), Some(Access { method: "GET".into(), endpoint: "/api/orders/{id}".into(), status: 200, ms: Some(3.0) }));
    }

    #[test]
    fn a_failed_tower_response_gets_its_status_from_the_classification() {
        let r = record("2026-08-05T12:33:01Z ERROR request{method=POST uri=/api/pay version=HTTP/1.1}: tower_http::trace::on_failure: response failed classification=Status(500) latency=120 ms");
        let a = extract(&r).unwrap();
        assert_eq!((a.status, a.ms), (500, Some(120.0)));
    }

    #[test]
    fn a_common_log_format_line_has_method_path_status() {
        let r = record(r#"127.0.0.1 - - [05/Aug/2026:12:33:01 +0000] "GET /health HTTP/1.1" 200 2 12 ms"#);
        let a = extract(&r).unwrap();
        assert_eq!((a.method.as_str(), a.endpoint.as_str(), a.status, a.ms), ("GET", "/health", 200, Some(12.0)));
    }

    #[test]
    fn a_quarkus_access_log_behind_a_logger_prefix_works() {
        let r = record(r#"2026-08-05 12:33:01,123 INFO  [io.quarkus.http.access-log] (executor-thread-1) 10.0.0.2 - - [05/Aug/2026:12:33:01 +0000] "POST /orders/6f1c2a3e-1111-2222-3333-444455556666 HTTP/1.1" 404 77"#);
        let a = extract(&r).unwrap();
        assert_eq!((a.endpoint.as_str(), a.status), ("/orders/{id}", 404));
    }

    #[test]
    fn actix_logs_the_time_as_bare_seconds() {
        let r = record(r#"127.0.0.1 "GET /a HTTP/1.1" 200 12 "-" "curl/8" 0.002345"#);
        assert!((extract(&r).unwrap().ms.unwrap() - 2.345).abs() < 1e-9);
    }

    #[test]
    fn an_ordinary_line_is_not_a_request() {
        assert!(extract(&record("2026-08-05 12:33:01 INFO Started App in 3 seconds")).is_none());
    }

    #[test]
    fn endpoints_drop_queries_hosts_and_ids() {
        assert_eq!(endpoint_of("/a/12/b?x=1"), "/a/{id}/b");
        assert_eq!(endpoint_of("http://localhost:8080/users/9"), "/users/{id}");
        assert_eq!(endpoint_of("/"), "/");
    }
}
