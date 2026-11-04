//! A cookie jar: the `Set-Cookie` headers answers bring, kept, and the `Cookie` header they earn the next
//! request. RFC 6265 in the part a client needs — domain and path matching, `Max-Age` / `Expires`, `Secure`,
//! host-only cookies — and nothing about the network or the clock: the caller passes the URL and `now_ms`.
//!
//! The jar is per environment and lives in the profile (never in the project's files), because a session cookie
//! is a credential.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    /// Lower-case, no leading dot.
    pub domain: String,
    pub path: String,
    /// Unix milliseconds; `None` for a session cookie (kept until the user clears the jar).
    pub expires_ms: Option<i64>,
    pub secure: bool,
    pub http_only: bool,
    /// Sent only to exactly `domain`, not to its sub-domains (no `Domain=` attribute was given).
    pub host_only: bool,
}

/// The parts of a URL a cookie cares about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Site {
    pub secure: bool,
    pub host: String,
    pub path: String,
}

pub fn site_of(url: &str) -> Option<Site> {
    let (scheme, rest) = url.split_once("://")?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    let host_port = authority.rsplit('@').next().unwrap_or(authority);
    let host = host_port.rsplit_once(':').filter(|(_, p)| p.chars().all(|c| c.is_ascii_digit())).map_or(host_port, |(h, _)| h);
    if host.is_empty() {
        return None;
    }
    let after = &rest[end..];
    let path = after.split(['?', '#']).next().unwrap_or("");
    Some(Site { secure: scheme.eq_ignore_ascii_case("https"), host: host.to_ascii_lowercase(), path: if path.is_empty() { "/".into() } else { path.to_string() } })
}

fn default_path(request_path: &str) -> String {
    match request_path.rfind('/') {
        Some(0) | None => "/".into(),
        Some(i) => request_path[..i].to_string(),
    }
}

/// Whether `host` is `domain` or a sub-domain of it (never a mere string suffix: `evilexample.com` is not).
pub fn domain_matches(host: &str, domain: &str, host_only: bool) -> bool {
    host == domain || (!host_only && host.ends_with(&format!(".{domain}")) && host.parse::<std::net::IpAddr>().is_err())
}

pub fn path_matches(request_path: &str, cookie_path: &str) -> bool {
    request_path == cookie_path
        || (request_path.starts_with(cookie_path) && (cookie_path.ends_with('/') || request_path[cookie_path.len()..].starts_with('/')))
}

/// `Expires=Wed, 21 Oct 2026 07:28:00 GMT` as unix milliseconds.
fn parse_http_date(text: &str) -> Option<i64> {
    // `Wed, 21 Oct 2026 07:28:00 GMT` or the dashed `21-Oct-26` form.
    let cleaned = text.replace('-', " ").replace(',', " ");
    let words: Vec<&str> = cleaned.split_whitespace().collect();
    let (day, month, year, clock) = match words.as_slice() {
        [_, d, m, y, c, ..] => (d, m, y, c),
        [d, m, y, c, ..] => (d, m, y, c),
        _ => return None,
    };
    let day: i64 = day.parse().ok()?;
    let month = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"].iter().position(|m| month.to_ascii_lowercase().starts_with(m))? as i64 + 1;
    let mut year: i64 = year.parse().ok()?;
    if year < 70 { year += 2000 } else if year < 100 { year += 1900 }
    let mut t = clock.split(':').map(|n| n.parse::<i64>().ok());
    let (h, m, s) = (t.next()??, t.next()??, t.next().flatten().unwrap_or(0));
    // days from civil
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(((days * 86_400) + h * 3600 + m * 60 + s) * 1000)
}

/// A cookie from one `Set-Cookie` header, as received from `url`. `None` for one that must be refused: no
/// name, or a `Domain` that is not the host or one of its parents.
pub fn parse_set_cookie(header: &str, url: &str, now_ms: i64) -> Option<Cookie> {
    let site = site_of(url)?;
    let mut parts = header.split(';');
    let (name, value) = parts.next()?.split_once('=')?;
    if name.trim().is_empty() {
        return None;
    }
    let mut cookie = Cookie {
        name: name.trim().to_string(),
        value: value.trim().to_string(),
        domain: site.host.clone(),
        path: default_path(&site.path),
        expires_ms: None,
        secure: false,
        http_only: false,
        host_only: true,
    };
    let mut max_age: Option<i64> = None;
    for attr in parts {
        let (k, v) = attr.split_once('=').map_or((attr.trim(), ""), |(k, v)| (k.trim(), v.trim()));
        match k.to_ascii_lowercase().as_str() {
            "domain" if !v.is_empty() => {
                let d = v.trim_start_matches('.').to_ascii_lowercase();
                if !domain_matches(&site.host, &d, false) {
                    return None;
                }
                cookie.domain = d;
                cookie.host_only = false;
            }
            "path" if v.starts_with('/') => cookie.path = v.to_string(),
            "expires" => cookie.expires_ms = parse_http_date(v),
            "max-age" => max_age = v.parse().ok(),
            "secure" => cookie.secure = true,
            "httponly" => cookie.http_only = true,
            _ => {}
        }
    }
    // Max-Age wins over Expires; zero or less deletes.
    if let Some(secs) = max_age {
        cookie.expires_ms = Some(now_ms + secs.saturating_mul(1000));
    }
    Some(cookie)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Jar {
    pub cookies: Vec<Cookie>,
}

impl Jar {
    fn expired(c: &Cookie, now_ms: i64) -> bool {
        c.expires_ms.is_some_and(|e| e <= now_ms)
    }

    /// Keep a cookie, replacing the one with the same name, domain and path; an expired one removes it.
    pub fn store(&mut self, cookie: Cookie, now_ms: i64) {
        self.cookies.retain(|c| !(c.name == cookie.name && c.domain == cookie.domain && c.path == cookie.path));
        if !Self::expired(&cookie, now_ms) {
            self.cookies.push(cookie);
        }
    }

    /// Everything a response set, from its `Set-Cookie` headers.
    pub fn store_all<'a>(&mut self, headers: impl IntoIterator<Item = &'a str>, url: &str, now_ms: i64) {
        for h in headers {
            if let Some(c) = parse_set_cookie(h, url, now_ms) {
                self.store(c, now_ms);
            }
        }
    }

    pub fn purge(&mut self, now_ms: i64) {
        self.cookies.retain(|c| !Self::expired(c, now_ms));
    }

    /// The cookies to send to `url`: matching domain and path, `Secure` only over https, unexpired; longer
    /// paths first, as the RFC asks.
    pub fn matching(&self, url: &str, now_ms: i64) -> Vec<&Cookie> {
        let Some(site) = site_of(url) else { return Vec::new() };
        let mut found: Vec<&Cookie> = self
            .cookies
            .iter()
            .filter(|c| !Self::expired(c, now_ms) && domain_matches(&site.host, &c.domain, c.host_only) && path_matches(&site.path, &c.path) && (!c.secure || site.secure))
            .collect();
        found.sort_by_key(|c| std::cmp::Reverse(c.path.len()));
        found
    }

    /// The `Cookie` header value for `url`, if any cookie applies.
    pub fn header_for(&self, url: &str, now_ms: i64) -> Option<String> {
        let found = self.matching(url, now_ms);
        (!found.is_empty()).then(|| found.iter().map(|c| format!("{}={}", c.name, c.value)).collect::<Vec<_>>().join("; "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_762_248_600_000; // 2025-11-04 09:30 UTC

    #[test]
    fn a_url_is_split_into_what_a_cookie_cares_about() {
        let s = site_of("https://User:pw@Api.Example.com:8443/a/b?x=1#f").unwrap();
        assert_eq!((s.secure, s.host.as_str(), s.path.as_str()), (true, "api.example.com", "/a/b"));
        assert_eq!(site_of("http://localhost").unwrap().path, "/");
        assert!(site_of("nope").is_none());
    }

    #[test]
    fn a_set_cookie_is_read_with_its_attributes() {
        let c = parse_set_cookie("sid=abc123; Path=/api; Domain=.Example.com; Secure; HttpOnly; Max-Age=3600", "https://app.example.com/login", NOW).unwrap();
        assert_eq!((c.name.as_str(), c.value.as_str(), c.domain.as_str(), c.path.as_str()), ("sid", "abc123", "example.com", "/api"));
        assert!(c.secure && c.http_only && !c.host_only);
        assert_eq!(c.expires_ms, Some(NOW + 3_600_000));
    }

    #[test]
    fn without_a_domain_the_cookie_is_for_that_host_only_and_the_path_is_the_directory() {
        let c = parse_set_cookie("a=1", "http://example.com/x/y/z", NOW).unwrap();
        assert!(c.host_only);
        assert_eq!(c.path, "/x/y");
        assert!(domain_matches("example.com", "example.com", true));
        assert!(!domain_matches("sub.example.com", "example.com", true));
    }

    #[test]
    fn a_domain_that_is_not_the_hosts_own_or_a_parent_is_refused() {
        assert!(parse_set_cookie("a=1; Domain=other.com", "http://example.com/", NOW).is_none());
        assert!(parse_set_cookie("a=1; Domain=ample.com", "http://example.com/", NOW).is_none(), "a string suffix is not a parent domain");
        assert!(parse_set_cookie("=1", "http://example.com/", NOW).is_none());
    }

    #[test]
    fn expires_is_read_as_a_date_and_max_age_wins() {
        let c = parse_set_cookie("a=1; Expires=Wed, 04 Nov 2026 09:30:00 GMT", "http://x.com/", NOW).unwrap();
        assert_eq!(c.expires_ms, Some(NOW + 365 * 86_400_000));
        let both = parse_set_cookie("a=1; Expires=Wed, 04 Nov 2026 09:30:00 GMT; Max-Age=10", "http://x.com/", NOW).unwrap();
        assert_eq!(both.expires_ms, Some(NOW + 10_000));
    }

    #[test]
    fn the_jar_sends_what_matches_and_nothing_else() {
        let mut jar = Jar::default();
        jar.store_all(["sid=1; Path=/", "api=2; Path=/api", "sec=3; Secure; Path=/", "dom=4; Domain=example.com; Path=/"], "https://www.example.com/", NOW);
        // host-only cookies belong to www.example.com; the domain one also to its parent's other sub-domains.
        let h = jar.header_for("https://www.example.com/api/items", NOW).unwrap();
        assert_eq!(h, "api=2; sid=1; sec=3; dom=4", "longest path first, then in order");
        assert_eq!(jar.header_for("http://www.example.com/api", NOW).unwrap(), "api=2; sid=1; dom=4", "Secure stays on https");
        assert_eq!(jar.header_for("https://static.example.com/", NOW).unwrap(), "dom=4", "host-only cookies do not go to sub-domains");
        assert!(jar.header_for("https://other.com/", NOW).is_none());
        assert!(jar.header_for("https://www.example.com/apiary", NOW).unwrap().starts_with("sid=1"), "/api does not match /apiary");
    }

    #[test]
    fn a_later_cookie_replaces_and_an_expired_one_deletes() {
        let mut jar = Jar::default();
        jar.store_all(["sid=1; Path=/"], "http://x.com/", NOW);
        jar.store_all(["sid=2; Path=/"], "http://x.com/", NOW);
        assert_eq!(jar.cookies.len(), 1);
        assert_eq!(jar.header_for("http://x.com/", NOW).unwrap(), "sid=2");
        jar.store_all(["sid=; Path=/; Max-Age=0"], "http://x.com/", NOW);
        assert!(jar.cookies.is_empty());
    }

    #[test]
    fn time_removes_cookies_that_expire() {
        let mut jar = Jar::default();
        jar.store_all(["a=1; Max-Age=60", "b=2"], "http://x.com/", NOW);
        assert_eq!(jar.header_for("http://x.com/", NOW + 120_000).unwrap(), "b=2");
        jar.purge(NOW + 120_000);
        assert_eq!(jar.cookies.len(), 1);
    }
}
