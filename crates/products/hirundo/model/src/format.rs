//! The files: what `.hirundo/` holds, readable and diffable.
//!
//! ```text
//! .hirundo/
//!   collections/
//!     bandi/                    one folder per collection
//!       collection.toml         name, auth, shared headers, variables
//!       lista-bandi.toml        one file per request
//!       auth/                   a sub-folder
//!         folder.toml
//!         login.toml
//!   environments/staging.toml   one file per environment
//!   env-types.toml              the environment types (absent: the defaults)
//!   local.toml                  this machine's values and the active environment — not committed
//! ```
//!
//! Params, headers and variables are the bulk-edit text of [`crate::kv`] inside a multi-line string, so a
//! request reads like the request it is:
//!
//! ```toml
//! name = "Lista bandi"
//! method = "GET"
//! url = "{{baseUrl}}/api/bandi"
//! params = '''
//! stato: APERTO
//! page: 0 # zero-based
//! '''
//! ```
//!
//! Secrets are never here: a variable whose value is `@secret` is looked up in the keyring.

use serde::Deserialize;

use crate::auth::{ApiKeyIn, Auth};
use crate::env::{EnvType, Environment};
use crate::kv::{self, KeyValue};
use crate::method::Method;
use crate::request::{Body, BodyKind, HttpRequest, Settings};
use crate::soap::SoapSettings;
use crate::tomlw::Writer;

// ── Shared pieces ────────────────────────────────────────────────────────────────────────────────

fn write_auth(w: &mut Writer, auth: &Auth) {
    if matches!(auth, Auth::Inherit) {
        return;
    }
    w.table("auth");
    match auth {
        Auth::Inherit => {}
        Auth::NoAuth => w.string("type", "none"),
        Auth::Bearer { token } => {
            w.string("type", "bearer");
            w.string("token", token);
        }
        Auth::Basic { username, password } => {
            w.string("type", "basic");
            w.string("username", username);
            w.string("password", password);
        }
        Auth::WsSecurity { username, password } => {
            w.string("type", "ws_security");
            w.string("username", username);
            w.string("password", password);
        }
        Auth::ApiKey { name, value, location } => {
            w.string("type", "api_key");
            w.string("name", name);
            w.string("value", value);
            w.string("location", if *location == ApiKeyIn::Query { "query" } else { "header" });
        }
    }
}

fn parse_error(what: &str, e: toml::de::Error) -> String {
    format!("{what} is not valid: {e}")
}

// ── A request ────────────────────────────────────────────────────────────────────────────────────

pub fn request_to_text(r: &HttpRequest) -> String {
    let mut w = Writer::new();
    w.string("name", &r.name);
    w.int("order", r.order);
    w.string("method", r.method.as_str());
    w.string("url", &r.url);
    w.text("params", &kv::to_text(&r.params));
    w.text("headers", &kv::to_text(&r.headers));
    w.text("docs", &r.docs);
    w.text("tests", &r.tests);
    write_auth(&mut w, &r.auth);
    if r.body.kind != BodyKind::Empty {
        w.table("body");
        w.string(
            "kind",
            match r.body.kind {
                BodyKind::Empty => "none",
                BodyKind::Json => "json",
                BodyKind::Text => "text",
                BodyKind::Xml => "xml",
                BodyKind::Form => "form",
                BodyKind::Soap => "soap",
                BodyKind::Multipart => "multipart",
            },
        );
        w.text("text", &r.body.text);
        w.text("form", &kv::to_text(&r.body.form));
    }
    if r.body.kind == BodyKind::Soap || r.soap != SoapSettings::default() {
        w.table("soap");
        w.string("version", r.soap.version.as_str());
        w.string("action", &r.soap.action);
    }
    if r.settings != Settings::default() {
        w.table("settings");
        w.int("timeout_ms", r.settings.timeout_ms as i64);
        w.bool("follow_redirects", r.settings.follow_redirects);
        w.bool("verify_tls", r.settings.verify_tls);
        w.bool("cookie_jar", r.settings.cookie_jar);
    }
    w.finish()
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct BodyFile {
    kind: BodyKind,
    text: String,
    form: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RequestFile {
    name: String,
    order: i64,
    method: Method,
    url: String,
    params: String,
    headers: String,
    docs: String,
    tests: String,
    auth: Auth,
    body: BodyFile,
    soap: SoapSettings,
    settings: Settings,
}

pub fn request_from_text(text: &str) -> Result<HttpRequest, String> {
    let f: RequestFile = toml::from_str(text).map_err(|e| parse_error("the request file", e))?;
    Ok(HttpRequest {
        name: f.name,
        order: f.order,
        method: f.method,
        url: f.url,
        params: kv::parse_text(&f.params),
        headers: kv::parse_text(&f.headers),
        auth: f.auth,
        body: Body { kind: f.body.kind, text: f.body.text, form: kv::parse_text(&f.body.form) },
        soap: f.soap,
        settings: f.settings,
        docs: f.docs,
        tests: f.tests,
    })
}

// ── A collection or a folder ─────────────────────────────────────────────────────────────────────

/// What a collection or a folder says about everything under it.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, Deserialize)]
#[serde(default)]
pub struct Meta {
    pub name: String,
    pub order: i64,
    pub description: String,
    pub auth: Auth,
    /// Headers every request below inherits.
    pub headers: Vec<KeyValue>,
    /// Variables for every request below.
    pub vars: Vec<KeyValue>,
    /// Switch the cookie jar off for every request below (a request's own setting cannot switch it back on).
    pub no_cookies: bool,
}

pub fn meta_to_text(m: &Meta) -> String {
    let mut w = Writer::new();
    w.string("name", &m.name);
    w.int("order", m.order);
    w.text("description", &m.description);
    w.text("headers", &kv::to_text(&m.headers));
    w.text("vars", &kv::to_text(&m.vars));
    if m.no_cookies {
        w.bool("no_cookies", true);
    }
    write_auth(&mut w, &m.auth);
    w.finish()
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct MetaFile {
    no_cookies: bool,
    name: String,
    order: i64,
    description: String,
    auth: Auth,
    headers: String,
    vars: String,
}

pub fn meta_from_text(text: &str) -> Result<Meta, String> {
    let f: MetaFile = toml::from_str(text).map_err(|e| parse_error("the collection file", e))?;
    Ok(Meta { name: f.name, order: f.order, description: f.description, auth: f.auth, headers: kv::parse_text(&f.headers), vars: kv::parse_text(&f.vars), no_cookies: f.no_cookies })
}

// ── Environments and their types ─────────────────────────────────────────────────────────────────

pub fn env_types_to_text(types: &[EnvType]) -> String {
    let mut w = Writer::new();
    for t in types {
        w.array_table("type");
        w.string("id", &t.id);
        w.string("name", &t.name);
        w.string("color", &t.color);
        w.bool("confirm_mutating", t.confirm_mutating);
        w.text("vars", &kv::to_text(&t.vars));
    }
    w.finish()
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct TypeDto {
    id: String,
    name: String,
    color: String,
    confirm_mutating: bool,
    vars: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct TypesFile {
    #[serde(rename = "type")]
    types: Vec<TypeDto>,
}

pub fn env_types_from_text(text: &str) -> Result<Vec<EnvType>, String> {
    let f: TypesFile = toml::from_str(text).map_err(|e| parse_error("the environment types file", e))?;
    Ok(f.types.into_iter().map(|t| EnvType { id: t.id, name: t.name, color: t.color, confirm_mutating: t.confirm_mutating, vars: kv::parse_text(&t.vars) }).collect())
}

pub fn environment_to_text(e: &Environment) -> String {
    let mut w = Writer::new();
    w.string("name", &e.name);
    w.string("type", &e.type_id);
    w.text("vars", &kv::to_text(&e.vars));
    w.finish()
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct EnvironmentFile {
    name: String,
    #[serde(rename = "type")]
    type_id: String,
    vars: String,
}

/// An environment's file; its id is the file name, which only the caller knows.
pub fn environment_from_text(id: &str, text: &str) -> Result<Environment, String> {
    let f: EnvironmentFile = toml::from_str(text).map_err(|e| parse_error("the environment file", e))?;
    Ok(Environment { id: id.to_string(), name: if f.name.trim().is_empty() { id.to_string() } else { f.name }, type_id: f.type_id, vars: kv::parse_text(&f.vars) })
}

// ── This machine's own ───────────────────────────────────────────────────────────────────────────

/// What is kept on this machine only (`local.toml`, ignored by git).
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, Deserialize)]
#[serde(default)]
pub struct Local {
    /// The environment requests are sent in.
    pub active_env: String,
    /// Variables that win over every environment: a personal token, a port.
    pub vars: Vec<KeyValue>,
}

pub fn local_to_text(l: &Local) -> String {
    let mut w = Writer::new();
    w.string("active_env", &l.active_env);
    w.text("vars", &kv::to_text(&l.vars));
    w.finish()
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct LocalFile {
    active_env: String,
    vars: String,
}

pub fn local_from_text(text: &str) -> Result<Local, String> {
    let f: LocalFile = toml::from_str(text).map_err(|e| parse_error("local.toml", e))?;
    Ok(Local { active_env: f.active_env, vars: kv::parse_text(&f.vars) })
}

/// The `.gitignore` that keeps `local.toml` out of the repository.
pub const GITIGNORE: &str = "local.toml\n";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::default_types;

    fn lista_bandi() -> HttpRequest {
        HttpRequest {
            name: "Lista bandi".into(),
            order: 1,
            method: Method::Get,
            url: "{{baseUrl}}/api/bandi".into(),
            params: vec![KeyValue::new("stato", "APERTO"), KeyValue { description: "zero-based".into(), ..KeyValue::new("page", "0") }],
            headers: vec![KeyValue::new("Accept", "application/json"), KeyValue { enabled: false, ..KeyValue::new("X-Debug", "1") }],
            ..Default::default()
        }
    }

    #[test]
    fn a_request_file_reads_like_the_request() {
        let text = request_to_text(&lista_bandi());
        assert_eq!(
            text,
            "name = \"Lista bandi\"\norder = 1\nmethod = \"GET\"\nurl = \"{{baseUrl}}/api/bandi\"\n\
             params = '''\nstato: APERTO\npage: 0 # zero-based'''\n\
             headers = '''\nAccept: application/json\n// X-Debug: 1'''\n"
        );
    }

    #[test]
    fn a_request_round_trips_with_everything_set() {
        let mut r = lista_bandi();
        r.method = Method::Post;
        r.auth = Auth::ApiKey { name: "X-Api-Key".into(), value: "{{key}}".into(), location: ApiKeyIn::Query };
        r.body = Body { kind: BodyKind::Json, text: "{\n  \"titolo\": \"it's \\\"quoted\\\"\"\n}".into(), form: vec![] };
        r.settings = Settings { timeout_ms: 1500, follow_redirects: false, verify_tls: false, cookie_jar: false };
        r.docs = "Creates a bando.\n\nSee the wiki.".into();
        r.tests = "status == 201\nbody.id exists".into();
        assert_eq!(request_from_text(&request_to_text(&r)).unwrap(), r);

        let mut form = lista_bandi();
        form.body = Body { kind: BodyKind::Form, text: String::new(), form: vec![KeyValue::new("user", "mario"), KeyValue::new("pw", "x")] };
        form.auth = Auth::Basic { username: "u".into(), password: "p".into() };
        assert_eq!(request_from_text(&request_to_text(&form)).unwrap(), form);
        for auth in [Auth::NoAuth, Auth::Bearer { token: "{{token}}".into() }] {
            let r = HttpRequest { auth, ..lista_bandi() };
            assert_eq!(request_from_text(&request_to_text(&r)).unwrap().auth, r.auth);
        }
    }

    #[test]
    fn a_minimal_hand_written_file_is_enough() {
        let r = request_from_text("name = \"Ping\"\nurl = \"http://x/ping\"\n").unwrap();
        assert_eq!((r.method, r.settings, r.auth), (Method::Get, Settings::default(), Auth::Inherit));
        assert_eq!(request_from_text("").unwrap(), HttpRequest::default());
    }

    #[test]
    fn a_broken_file_says_which_one() {
        assert!(request_from_text("name = ").unwrap_err().starts_with("the request file is not valid"));
        assert!(request_from_text("method = \"FETCH\"").is_err());
        assert!(meta_from_text("auth = 3").unwrap_err().contains("collection file"));
    }

    #[test]
    fn defaults_are_not_written() {
        let text = request_to_text(&HttpRequest { name: "x".into(), url: "http://x".into(), ..Default::default() });
        assert!(!text.contains("[auth]") && !text.contains("[body]") && !text.contains("[settings]"), "{text}");
    }

    #[test]
    fn a_collection_keeps_its_auth_headers_and_variables() {
        let meta = Meta {
            name: "Bandi".into(),
            order: 2,
            description: "The bandi API".into(),
            auth: Auth::Bearer { token: "{{token}}".into() },
            headers: vec![KeyValue::new("X-Team", "bandi")],
            vars: vec![KeyValue::new("baseUrl", "http://localhost:8080"), KeyValue::new("token", "@secret")],
            no_cookies: false,
        };
        let text = meta_to_text(&meta);
        assert!(text.contains("token: @secret"), "{text}");
        assert!(text.contains("[auth]\ntype = \"bearer\""), "{text}");
        assert_eq!(meta_from_text(&text).unwrap(), meta);
    }

    #[test]
    fn environment_types_round_trip_including_their_variables() {
        let mut types = default_types();
        types.push(EnvType { id: "qa".into(), name: "QA".into(), color: "#a371f7".into(), confirm_mutating: true, vars: vec![KeyValue::new("timeout", "5000")] });
        let text = env_types_to_text(&types);
        assert!(text.contains("[[type]]\nid = \"prod\""), "{text}");
        assert_eq!(env_types_from_text(&text).unwrap(), types);
        assert!(env_types_from_text("").unwrap().is_empty(), "an empty file is no types; the caller falls back to the defaults");
    }

    #[test]
    fn an_environment_takes_its_id_from_the_file_name() {
        let env = Environment { id: "stg-eu".into(), name: "Staging EU".into(), type_id: "staging".into(), vars: vec![KeyValue::new("baseUrl", "https://stg-eu.example.it"), KeyValue::new("token", "@secret")] };
        assert_eq!(environment_from_text("stg-eu", &environment_to_text(&env)).unwrap(), env);
        assert_eq!(environment_from_text("dev", "type = \"dev\"").unwrap().name, "dev", "no name: the id stands in");
    }

    #[test]
    fn local_values_round_trip() {
        let l = Local { active_env: "staging".into(), vars: vec![KeyValue::new("port", "9090")] };
        assert_eq!(local_from_text(&local_to_text(&l)).unwrap(), l);
        assert_eq!(local_from_text("").unwrap(), Local::default());
    }
}

#[cfg(test)]
mod cookie_switch_tests {
    use super::*;

    #[test]
    fn a_collection_can_switch_the_cookie_jar_off_and_it_stays_off_in_the_file() {
        let m = Meta { name: "Bandi".into(), no_cookies: true, auth: Auth::Bearer { token: "{{t}}".into() }, ..Default::default() };
        let text = meta_to_text(&m);
        assert!(text.contains("no_cookies = true"), "{text}");
        assert_eq!(meta_from_text(&text).unwrap(), m);
        assert!(!meta_to_text(&Meta::default()).contains("no_cookies"), "the default is not written");
    }
}
