//! From what the user wrote to what goes on the wire.
//!
//! A request in the tree is *not* what is sent: its URL has `{{baseUrl}}` in it, its auth may come from the
//! folder above, the collection adds a header, the body has `{{id}}`. [`prepare`] folds all of that, once and
//! purely, into a [`Prepared`] — a method, a URL, headers and a body, with every variable replaced. The sender
//! does nothing but send it; "copy as cURL" prints it; the preview shows it. Whatever the preview shows is
//! therefore what is sent, because it is the same value.
//!
//! Called with *no* variables the result keeps its `{{…}}`, which is how "copy as cURL, variables
//! unresolved" works.

use crate::auth::{self, Auth};
use crate::kv::{enabled_pairs, percent_encode, KeyValue};
use crate::multipart::{parts_of, FormPart};
use crate::method::Method;
use crate::request::{BodyKind, HttpRequest};
use crate::soap::{add_username_token, protocol_headers};
use crate::vars::{interpolate, ResolvedVars};

/// What the levels above a request contribute, outermost first (the collection, then its folders).
#[derive(Debug, Clone, Default)]
pub struct Inherited<'a> {
    pub auth: Vec<&'a Auth>,
    pub headers: Vec<&'a [KeyValue]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prepared {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
    /// A multipart body, as parts the sender reads the files of and frames (the `Content-Type` and its boundary are its).
    pub multipart: Option<Vec<FormPart>>,
    /// Send and keep cookies (see [`crate::cookies`]).
    pub cookie_jar: bool,
    pub timeout_ms: u64,
    pub follow_redirects: bool,
    pub verify_tls: bool,
    /// Variables that had no value, once each. A request with any of these should not be sent.
    pub unresolved: Vec<String>,
}

fn set_header(headers: &mut Vec<(String, String)>, name: String, value: String) {
    match headers.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case(&name)) {
        Some(slot) => *slot = (name, value),
        None => headers.push((name, value)),
    }
}

fn has_header(headers: &[(String, String)], name: &str) -> bool {
    headers.iter().any(|(k, _)| k.eq_ignore_ascii_case(name))
}

/// `http://` in front of a URL that names no scheme, as every client does for `localhost:8080/x`.
fn with_scheme(url: &str) -> String {
    if url.is_empty() || url.contains("://") || url.starts_with("{{") { url.to_string() } else { format!("http://{url}") }
}

pub fn prepare(req: &HttpRequest, inherited: &Inherited<'_>, vars: &ResolvedVars) -> Prepared {
    let mut unresolved: Vec<String> = Vec::new();
    let mut fill = |text: &str| -> String {
        let done = interpolate(text, vars);
        for name in done.unresolved {
            if !unresolved.contains(&name) {
                unresolved.push(name);
            }
        }
        done.text
    };

    // Headers: the levels above first, so a request's own header replaces an inherited one of the same name.
    let mut headers: Vec<(String, String)> = Vec::new();
    for rows in inherited.headers.iter().copied().chain(std::iter::once(req.headers.as_slice())) {
        for (k, v) in enabled_pairs(rows) {
            set_header(&mut headers, fill(&k), fill(&v));
        }
    }

    // Query: the request's own params, then whatever the auth adds (an API key in the query).
    let mut query: Vec<(String, String)> = enabled_pairs(&req.params).into_iter().map(|(k, v)| (percent_encode(&fill(&k)), percent_encode(&fill(&v)))).collect();

    let mut ws_security: Option<(String, String)> = None;
    let mut chain: Vec<&Auth> = inherited.auth.clone();
    chain.push(&req.auth);
    if let Some(found) = auth::effective(&chain) {
        let filled = match found {
            Auth::Bearer { token } => Auth::Bearer { token: fill(token) },
            Auth::Basic { username, password } => Auth::Basic { username: fill(username), password: fill(password) },
            Auth::ApiKey { name, value, location } => Auth::ApiKey { name: fill(name), value: fill(value), location: *location },
            Auth::WsSecurity { username, password } => {
                ws_security = Some((fill(username), fill(password)));
                Auth::NoAuth
            }
            other => other.clone(),
        };
        auth::apply(&filled, &mut headers, &mut query);
    }

    let base = with_scheme(fill(req.url.trim()).trim());
    let url = if query.is_empty() {
        base
    } else {
        let joined = query.iter().map(|(k, v)| if v.is_empty() { k.clone() } else { format!("{k}={v}") }).collect::<Vec<_>>().join("&");
        format!("{base}{}{joined}", if base.contains('?') { '&' } else { '?' })
    };

    let mut multipart: Option<Vec<FormPart>> = None;
    let (body, content_type) = match req.body.kind {
        BodyKind::Multipart => {
            multipart = Some(parts_of(&req.body.form, &mut fill));
            (None, None)
        }
        BodyKind::Empty => (None, None),
        BodyKind::Json => (Some(fill(&req.body.text)), Some("application/json")),
        BodyKind::Text => (Some(fill(&req.body.text)), Some("text/plain")),
        BodyKind::Xml => (Some(fill(&req.body.text)), Some("application/xml")),
        BodyKind::Soap => {
            // The protocol decides the content type (and, in 1.1, the SOAPAction header); an explicit header still wins.
            let action = fill(&req.soap.action);
            for (k, v) in protocol_headers(req.soap.version, &action) {
                if !has_header(&headers, &k) {
                    headers.push((k, v));
                }
            }
            let envelope = fill(&req.body.text);
            let envelope = match &ws_security {
                Some((user, pass)) => add_username_token(&envelope, user, pass),
                None => envelope,
            };
            (Some(envelope), None)
        }
        BodyKind::Form => {
            let pairs: Vec<String> = enabled_pairs(&req.body.form)
                .into_iter()
                .map(|(k, v)| {
                    let (k, v) = (percent_encode(&fill(&k)), percent_encode(&fill(&v)));
                    format!("{k}={v}")
                })
                .collect();
            (Some(pairs.join("&")), Some("application/x-www-form-urlencoded"))
        }
    };
    if let (Some(ct), true) = (content_type, body.is_some()) {
        if !has_header(&headers, "Content-Type") {
            headers.push(("Content-Type".into(), ct.into()));
        }
    }

    Prepared {
        method: req.method,
        url,
        headers,
        body,
        multipart,
        cookie_jar: req.settings.cookie_jar,
        timeout_ms: req.settings.timeout_ms,
        follow_redirects: req.settings.follow_redirects,
        verify_tls: req.settings.verify_tls,
        unresolved,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request::Body;
    use crate::vars::{resolve, VarLayer, VarScope, VarSource};

    fn vars(pairs: &[(&str, &str)]) -> ResolvedVars {
        let layer = VarLayer { source: VarSource::new(VarScope::Environment, "e"), vars: pairs.iter().map(|(k, v)| KeyValue::new(k, v)).collect() };
        resolve(&[layer], &|_, _| None)
    }

    fn req() -> HttpRequest {
        HttpRequest { name: "Lista bandi".into(), url: "{{baseUrl}}/api/bandi".into(), params: vec![KeyValue::new("stato", "APERTO"), KeyValue::new("page", "0")], ..Default::default() }
    }

    #[test]
    fn the_url_is_filled_in_and_the_params_make_its_query() {
        let p = prepare(&req(), &Inherited::default(), &vars(&[("baseUrl", "https://stg.example.it")]));
        assert_eq!(p.url, "https://stg.example.it/api/bandi?stato=APERTO&page=0");
        assert_eq!(p.method, Method::Get);
        assert!(p.unresolved.is_empty());
        assert!(p.body.is_none());
    }

    #[test]
    fn params_are_encoded_and_an_existing_query_is_extended() {
        let mut r = req();
        r.url = "http://x/a?fixed=1".into();
        r.params = vec![KeyValue::new("q", "hello world"), KeyValue { enabled: false, ..KeyValue::new("off", "1") }];
        assert_eq!(prepare(&r, &Inherited::default(), &vars(&[])).url, "http://x/a?fixed=1&q=hello%20world");
    }

    #[test]
    fn a_url_with_no_scheme_gets_http() {
        let mut r = req();
        r.url = "localhost:8080/ping".into();
        r.params.clear();
        assert_eq!(prepare(&r, &Inherited::default(), &vars(&[])).url, "http://localhost:8080/ping");
    }

    #[test]
    fn what_has_no_value_is_reported_and_left_visible() {
        let p = prepare(&req(), &Inherited::default(), &vars(&[]));
        assert_eq!(p.unresolved, ["baseUrl"]);
        assert!(p.url.starts_with("http://{{baseUrl}}") || p.url.starts_with("{{baseUrl}}"), "{}", p.url);
    }

    #[test]
    fn auth_comes_from_the_nearest_level_and_is_filled_in() {
        let coll = Auth::Bearer { token: "{{token}}".into() };
        let inherited = Inherited { auth: vec![&coll, &Auth::Inherit], headers: vec![] };
        let p = prepare(&req(), &inherited, &vars(&[("baseUrl", "http://x"), ("token", "abc")]));
        assert!(p.headers.contains(&("Authorization".to_string(), "Bearer abc".to_string())));

        let mut own = req();
        own.auth = Auth::NoAuth;
        let p = prepare(&own, &inherited, &vars(&[("baseUrl", "http://x"), ("token", "abc")]));
        assert!(!p.headers.iter().any(|(k, _)| k == "Authorization"), "the request opted out");
    }

    #[test]
    fn an_api_key_in_the_query_joins_the_query() {
        let key = Auth::ApiKey { name: "key".into(), value: "{{k}}".into(), location: crate::auth::ApiKeyIn::Query };
        let mut r = req();
        r.auth = key;
        let p = prepare(&r, &Inherited::default(), &vars(&[("baseUrl", "http://x"), ("k", "s3")]));
        assert_eq!(p.url, "http://x/api/bandi?stato=APERTO&page=0&key=s3");
    }

    #[test]
    fn a_request_header_replaces_an_inherited_one_of_the_same_name() {
        let above = vec![KeyValue::new("Accept", "text/plain"), KeyValue::new("X-Team", "bandi")];
        let inherited = Inherited { auth: vec![], headers: vec![above.as_slice()] };
        let mut r = req();
        r.headers = vec![KeyValue::new("accept", "application/json")];
        let p = prepare(&r, &inherited, &vars(&[("baseUrl", "http://x")]));
        assert_eq!(p.headers, [("accept".to_string(), "application/json".to_string()), ("X-Team".to_string(), "bandi".to_string())]);
    }

    #[test]
    fn a_json_body_sets_its_content_type_unless_the_request_did() {
        let mut r = req();
        r.method = Method::Post;
        r.body = Body { kind: BodyKind::Json, text: "{\"id\": {{id}}}".into(), form: vec![] };
        let p = prepare(&r, &Inherited::default(), &vars(&[("baseUrl", "http://x"), ("id", "42")]));
        assert_eq!(p.body.as_deref(), Some("{\"id\": 42}"));
        assert!(p.headers.contains(&("Content-Type".to_string(), "application/json".to_string())));

        r.headers = vec![KeyValue::new("content-type", "application/vnd.api+json")];
        let p = prepare(&r, &Inherited::default(), &vars(&[("baseUrl", "http://x"), ("id", "1")]));
        assert_eq!(p.headers.iter().filter(|(k, _)| k.eq_ignore_ascii_case("content-type")).count(), 1);
    }

    #[test]
    fn a_form_body_is_urlencoded() {
        let mut r = req();
        r.method = Method::Post;
        r.body = Body { kind: BodyKind::Form, text: String::new(), form: vec![KeyValue::new("user", "mario rossi"), KeyValue::new("pw", "a&b")] };
        let p = prepare(&r, &Inherited::default(), &vars(&[("baseUrl", "http://x")]));
        assert_eq!(p.body.as_deref(), Some("user=mario%20rossi&pw=a%26b"));
        assert!(p.headers.contains(&("Content-Type".to_string(), "application/x-www-form-urlencoded".to_string())));
    }

    #[test]
    fn the_request_settings_travel_with_it() {
        let mut r = req();
        r.settings.timeout_ms = 1500;
        r.settings.verify_tls = false;
        let p = prepare(&r, &Inherited::default(), &vars(&[]));
        assert_eq!((p.timeout_ms, p.follow_redirects, p.verify_tls), (1500, true, false));
    }
}
