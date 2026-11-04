//! Authentication, and where it comes from.
//!
//! Auth is set on the collection, on a folder, or on one request. Whoever is below inherits it, may replace
//! it, or may say "none". [`effective`] picks the one that counts; [`apply`] puts it on the request.
//! Values here may hold `{{variables}}` (a token is usually `{{token}}`); they are interpolated before
//! [`apply`] runs, in [`crate::prepare`].
//!
//! Not here yet: OAuth 2 (code + PKCE, client credentials) and JWT with automatic refresh. Each needs a
//! flow with a browser or a token endpoint, and a place to keep the refresh token — the keyring.

use serde::{Deserialize, Serialize};

use crate::kv::percent_encode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiKeyIn {
    #[default]
    Header,
    Query,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Auth {
    /// Whatever the level above says. The default of a request and a folder.
    #[default]
    Inherit,
    /// Explicitly nothing, even if a level above has auth.
    #[serde(rename = "none")]
    NoAuth,
    Bearer {
        #[serde(default)]
        token: String,
    },
    Basic {
        #[serde(default)]
        username: String,
        #[serde(default)]
        password: String,
    },
    ApiKey {
        #[serde(default)]
        name: String,
        #[serde(default)]
        value: String,
        #[serde(default)]
        location: ApiKeyIn,
    },
    /// A WS-Security `UsernameToken` in the SOAP envelope's Header (applied to the body, not to a header).
    WsSecurity {
        #[serde(default)]
        username: String,
        #[serde(default)]
        password: String,
    },
}

impl Auth {
    /// The label a tab or a badge shows.
    pub fn label(&self) -> &'static str {
        match self {
            Auth::Inherit => "Inherit",
            Auth::NoAuth => "No auth",
            Auth::Bearer { .. } => "Bearer",
            Auth::Basic { .. } => "Basic",
            Auth::ApiKey { .. } => "API key",
            Auth::WsSecurity { .. } => "WS-Security",
        }
    }
}

/// The index, in `chain` (outermost first: collection, folders…, request), of the level whose auth counts —
/// the innermost that is not `Inherit`. `None` when every level inherits and so there is no auth at all.
pub fn effective_index(chain: &[&Auth]) -> Option<usize> {
    chain.iter().rposition(|a| !matches!(a, Auth::Inherit))
}

/// The auth that counts for `chain`, or `None` when there is none (or it is explicitly [`Auth::NoAuth`]).
pub fn effective<'a>(chain: &[&'a Auth]) -> Option<&'a Auth> {
    effective_index(chain).map(|i| chain[i]).filter(|a| !matches!(a, Auth::NoAuth))
}

/// Put `auth` on a request: headers and query pairs, both already interpolated. A header the request sets
/// itself is not overwritten — an explicit `Authorization` beats an inherited one.
pub fn apply(auth: &Auth, headers: &mut Vec<(String, String)>, query: &mut Vec<(String, String)>) {
    let has = |headers: &[(String, String)], name: &str| headers.iter().any(|(k, _)| k.eq_ignore_ascii_case(name));
    match auth {
        // WS-Security lives in the envelope, not in a header: `prepare` puts it there.
        Auth::Inherit | Auth::NoAuth | Auth::WsSecurity { .. } => {}
        Auth::Bearer { token } => {
            if !token.trim().is_empty() && !has(headers, "Authorization") {
                headers.push(("Authorization".into(), format!("Bearer {}", token.trim())));
            }
        }
        Auth::Basic { username, password } => {
            if !has(headers, "Authorization") {
                use base64::Engine;
                let encoded = base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"));
                headers.push(("Authorization".into(), format!("Basic {encoded}")));
            }
        }
        Auth::ApiKey { name, value, location } => {
            if name.trim().is_empty() {
                return;
            }
            match location {
                ApiKeyIn::Header => {
                    if !has(headers, name.trim()) {
                        headers.push((name.trim().into(), value.clone()));
                    }
                }
                ApiKeyIn::Query => query.push((percent_encode(name.trim()), percent_encode(value))),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bearer(t: &str) -> Auth {
        Auth::Bearer { token: t.into() }
    }

    #[test]
    fn the_innermost_level_that_says_something_wins() {
        let (coll, folder, req) = (bearer("c"), Auth::Inherit, Auth::Inherit);
        assert_eq!(effective(&[&coll, &folder, &req]), Some(&coll));
        let own = Auth::Basic { username: "u".into(), password: "p".into() };
        assert_eq!(effective(&[&coll, &folder, &own]), Some(&own));
        assert_eq!(effective_index(&[&coll, &bearer("f"), &req]), Some(1));
    }

    #[test]
    fn none_cuts_the_inheritance_and_all_inherit_means_no_auth() {
        let (coll, none) = (bearer("c"), Auth::NoAuth);
        assert_eq!(effective(&[&coll, &none]), None, "explicitly no auth");
        assert_eq!(effective_index(&[&coll, &none]), Some(1), "but the level that said so is known");
        assert_eq!(effective(&[&Auth::Inherit, &Auth::Inherit]), None);
        assert_eq!(effective_index(&[]), None);
    }

    #[test]
    fn bearer_and_basic_set_the_authorization_header() {
        let (mut h, mut q) = (vec![], vec![]);
        apply(&bearer(" abc "), &mut h, &mut q);
        assert_eq!(h, [("Authorization".to_string(), "Bearer abc".to_string())]);
        let mut h = vec![];
        apply(&Auth::Basic { username: "aladdin".into(), password: "opensesame".into() }, &mut h, &mut q);
        assert_eq!(h[0].1, "Basic YWxhZGRpbjpvcGVuc2VzYW1l");
    }

    #[test]
    fn an_explicit_authorization_header_is_not_overwritten() {
        let mut h = vec![("authorization".to_string(), "Custom x".to_string())];
        apply(&bearer("abc"), &mut h, &mut vec![]);
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].1, "Custom x");
    }

    #[test]
    fn an_api_key_goes_where_it_says() {
        let (mut h, mut q) = (vec![], vec![]);
        apply(&Auth::ApiKey { name: "X-Api-Key".into(), value: "k".into(), location: ApiKeyIn::Header }, &mut h, &mut q);
        apply(&Auth::ApiKey { name: "api key".into(), value: "a b".into(), location: ApiKeyIn::Query }, &mut h, &mut q);
        assert_eq!(h, [("X-Api-Key".to_string(), "k".to_string())]);
        assert_eq!(q, [("api%20key".to_string(), "a%20b".to_string())]);
    }

    #[test]
    fn an_empty_bearer_adds_nothing() {
        let mut h = vec![];
        apply(&bearer("  "), &mut h, &mut vec![]);
        assert!(h.is_empty());
    }

    #[test]
    fn auth_is_written_with_a_type_tag() {
        assert_eq!(serde_json::to_string(&bearer("t")).unwrap(), r#"{"type":"bearer","token":"t"}"#);
        assert_eq!(serde_json::to_string(&Auth::NoAuth).unwrap(), r#"{"type":"none"}"#);
        assert_eq!(serde_json::from_str::<Auth>(r#"{"type":"inherit"}"#).unwrap(), Auth::Inherit);
    }
}
