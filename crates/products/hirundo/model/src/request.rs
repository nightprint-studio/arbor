//! The request itself.

use serde::{Deserialize, Serialize};

use crate::auth::Auth;
use crate::kv::KeyValue;
use crate::method::Method;
use crate::soap::SoapSettings;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BodyKind {
    #[default]
    #[serde(rename = "none")]
    Empty,
    Json,
    Text,
    Xml,
    /// `application/x-www-form-urlencoded` from [`Body::form`].
    Form,
    /// A SOAP envelope in [`Body::text`]; the protocol headers come from [`HttpRequest::soap`].
    Soap,
    /// `multipart/form-data` from [`Body::form`]: a value starting with `@` is a file, the description is the part's content type.
    Multipart,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Body {
    pub kind: BodyKind,
    /// The raw text of a JSON, text or XML body.
    pub text: String,
    /// The fields of a form body.
    pub form: Vec<KeyValue>,
}

/// Per-request transport settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub timeout_ms: u64,
    pub follow_redirects: bool,
    /// Off accepts any TLS certificate (a development server with a self-signed one).
    pub verify_tls: bool,
    /// Send the cookies the environment's jar holds, and keep the ones the answer sets.
    pub cookie_jar: bool,
}

pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;

impl Default for Settings {
    fn default() -> Self {
        Self { timeout_ms: DEFAULT_TIMEOUT_MS, follow_redirects: true, verify_tls: true, cookie_jar: true }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct HttpRequest {
    pub name: String,
    /// Position among its siblings in the tree.
    pub order: i64,
    pub method: Method,
    /// The URL without its query: that is [`HttpRequest::params`]. May hold `{{variables}}`.
    pub url: String,
    pub params: Vec<KeyValue>,
    pub headers: Vec<KeyValue>,
    pub auth: Auth,
    pub body: Body,
    /// Version and action of a call whose body is [`BodyKind::Soap`]; ignored otherwise.
    pub soap: SoapSettings,
    pub settings: Settings,
    /// Notes, shown in the Docs tab.
    pub docs: String,
    /// Declarative assertions and Lua, run after the response (the runner reads them; sending ignores them).
    pub tests: String,
}

/// A URL split at its query: `("https://x/a", [("b","1")])`. A fragment stays with the path.
pub fn split_query(url: &str) -> (String, Vec<KeyValue>) {
    match url.split_once('?') {
        None => (url.to_string(), Vec::new()),
        Some((path, query)) => {
            let (query, fragment) = query.split_once('#').map_or((query, ""), |(q, f)| (q, f));
            let rows = crate::kv::parse_text(query);
            let rows = if rows.is_empty() && !query.is_empty() { vec![KeyValue::new(query, "")] } else { rows };
            let path = if fragment.is_empty() { path.to_string() } else { format!("{path}#{fragment}") };
            (path, rows)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blank_request_is_a_get_with_the_usual_settings() {
        let r = HttpRequest::default();
        assert_eq!(r.method, Method::Get);
        assert_eq!(r.settings, Settings { timeout_ms: 30_000, follow_redirects: true, verify_tls: true, cookie_jar: true });
        assert_eq!(r.auth, Auth::Inherit);
        assert_eq!(r.body.kind, BodyKind::Empty);
    }

    #[test]
    fn a_partial_json_request_fills_in_the_rest() {
        let r: HttpRequest = serde_json::from_str(r#"{"name":"x","method":"POST","url":"http://a","settings":{"timeout_ms":5}}"#).unwrap();
        assert_eq!(r.method, Method::Post);
        assert_eq!(r.settings.timeout_ms, 5);
        assert!(r.settings.follow_redirects, "unset settings keep their defaults");
    }

    #[test]
    fn a_url_is_split_at_its_query() {
        let (path, rows) = split_query("https://x.it/api/bandi?stato=APERTO&page=0");
        assert_eq!(path, "https://x.it/api/bandi");
        assert_eq!(rows, vec![KeyValue::new("stato", "APERTO"), KeyValue::new("page", "0")]);
        assert_eq!(split_query("https://x.it/a"), ("https://x.it/a".to_string(), vec![]));
        let (path, rows) = split_query("https://x.it/a?b=1#top");
        assert_eq!((path.as_str(), rows.len()), ("https://x.it/a#top", 1));
    }
}
