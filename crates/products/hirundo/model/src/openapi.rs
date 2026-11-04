//! OpenAPI 3 (JSON) → a plan of requests.
//!
//! Pure: a text goes in, a [`Plan`] comes out — the spec's title, its server (the `{{baseUrl}}`), the auth its
//! security schemes ask for, and one [`Operation`] per path + method, grouped by tag, each already a
//! [`HttpRequest`] with its path / query / header parameters, an example body built from the schema, and the
//! spec's descriptions in the Docs. What to keep and where to put it is the caller's: the modal lets the user
//! choose, the backend writes the files.
//!
//! JSON only, because the workspace has a JSON parser and no YAML one; a YAML spec is refused in words rather
//! than half-read. `$ref`s inside the document (`#/components/...`) are followed, with a depth limit and a
//! cycle guard; references to other files are not, and are said so.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::auth::{ApiKeyIn, Auth};
use crate::kv::KeyValue;
use crate::method::Method;
use crate::request::{Body, BodyKind, HttpRequest};

/// The tag of an operation that has none.
pub const UNTAGGED: &str = "default";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Operation {
    /// Unique within the plan: `GET /pets/{id}`.
    pub id: String,
    pub tag: String,
    pub path: String,
    pub summary: String,
    pub request: HttpRequest,
}

#[derive(Debug, Clone, Serialize)]
pub struct Plan {
    pub title: String,
    pub description: String,
    /// The first server, `{variables}` filled with their defaults.
    pub base_url: Option<String>,
    /// What the spec's security schemes ask for, to be set on the folder the requests go in.
    pub auth: Auth,
    /// Tags in the order they first appear.
    pub tags: Vec<String>,
    pub operations: Vec<Operation>,
    /// What could not be carried over, in words.
    pub warnings: Vec<String>,
}

const METHODS: [&str; 7] = ["get", "put", "post", "delete", "patch", "head", "options"];
/// How deep an example goes before it stops: a recursive schema must not run away.
const MAX_DEPTH: usize = 8;

// ── Reading the document ─────────────────────────────────────────────────────────────────────────

pub fn parse(text: &str) -> Result<Plan, String> {
    let trimmed = text.trim_start();
    if !trimmed.starts_with('{') {
        let head: String = trimmed.chars().take(400).collect();
        return Err(if head.contains("openapi:") || head.contains("swagger:") {
            "This looks like a YAML spec, which is not supported yet. Convert it to JSON (for instance with `swagger-cli bundle -t json`) and import that.".to_string()
        } else {
            "This is not a JSON document. Paste an OpenAPI 3 spec in JSON, or the URL or path of one.".to_string()
        });
    }
    let root: Value = serde_json::from_str(trimmed).map_err(|e| format!("The spec is not valid JSON: {e}"))?;
    if root.get("swagger").is_some() {
        return Err("This is a Swagger 2.0 spec; only OpenAPI 3 is supported.".into());
    }
    match root.get("openapi").and_then(Value::as_str) {
        Some(v) if v.starts_with("3.") => {}
        Some(v) => return Err(format!("OpenAPI {v} is not supported; only 3.x is.")),
        None => return Err("There is no `openapi` field: this is not an OpenAPI 3 spec.".into()),
    }
    let paths = root.get("paths").and_then(Value::as_object).ok_or("The spec has no `paths`.")?;

    let mut warnings = Vec::new();
    let title = root.pointer("/info/title").and_then(Value::as_str).unwrap_or("API").trim().to_string();
    let description = root.pointer("/info/description").and_then(Value::as_str).unwrap_or("").trim().to_string();
    let base_url = server_url(&root, &mut warnings);
    let auth = auth_of(&root, &mut warnings);

    let mut operations = Vec::new();
    let mut tags: Vec<String> = Vec::new();
    for (path, item) in paths {
        let item = deref(&root, item);
        for method in METHODS {
            let Some(op) = item.get(method).filter(|o| o.is_object()) else { continue };
            let operation = operation_of(&root, path, method, item, op, &mut warnings);
            if !tags.contains(&operation.tag) {
                tags.push(operation.tag.clone());
            }
            operations.push(operation);
        }
    }
    if operations.is_empty() {
        return Err("The spec has no operations.".into());
    }
    warnings.sort();
    warnings.dedup();
    Ok(Plan { title: if title.is_empty() { "API".into() } else { title }, description, base_url, auth, tags, operations, warnings })
}

/// The target of a `$ref` like `#/components/schemas/Pet`.
fn pointer<'a>(root: &'a Value, reference: &str) -> Option<&'a Value> {
    root.pointer(reference.strip_prefix('#')?)
}

/// `v`, or what its `$ref` points at (followed a few times, so a ref to a ref works).
fn deref<'a>(root: &'a Value, v: &'a Value) -> &'a Value {
    let mut current = v;
    for _ in 0..8 {
        match current.get("$ref").and_then(Value::as_str).and_then(|r| pointer(root, r)) {
            Some(next) => current = next,
            None => break,
        }
    }
    current
}

fn server_url(root: &Value, warnings: &mut Vec<String>) -> Option<String> {
    let server = root.pointer("/servers/0")?;
    let mut url = server.get("url").and_then(Value::as_str)?.to_string();
    if let Some(vars) = server.get("variables").and_then(Value::as_object) {
        for (name, def) in vars {
            if let Some(default) = def.get("default").and_then(Value::as_str) {
                url = url.replace(&format!("{{{name}}}"), default);
            }
        }
    }
    if !url.contains("://") {
        warnings.push(format!("The server URL `{url}` is relative: put the host in {{{{baseUrl}}}}."));
    }
    Some(url.trim_end_matches('/').to_string())
}

// ── Auth ─────────────────────────────────────────────────────────────────────────────────────────

/// The auth of the first security requirement, as the closest Hirundo auth: bearer and OAuth2 / OpenID tokens
/// as `{{token}}`, basic as `{{username}}` / `{{password}}`, an API key as `{{apiKey}}`.
fn auth_of(root: &Value, warnings: &mut Vec<String>) -> Auth {
    let Some(name) = root.pointer("/security/0").and_then(Value::as_object).and_then(|o| o.keys().next()) else { return Auth::Inherit };
    let Some(scheme) = root.pointer(&format!("/components/securitySchemes/{name}")).map(|s| deref(root, s)) else {
        warnings.push(format!("The security scheme `{name}` is not defined."));
        return Auth::Inherit;
    };
    let kind = scheme.get("type").and_then(Value::as_str).unwrap_or("");
    match kind {
        "http" => match scheme.get("scheme").and_then(Value::as_str).unwrap_or("").to_ascii_lowercase().as_str() {
            "bearer" => Auth::Bearer { token: "{{token}}".into() },
            "basic" => Auth::Basic { username: "{{username}}".into(), password: "{{password}}".into() },
            other => {
                warnings.push(format!("HTTP auth scheme `{other}` is not supported; the requests have no auth."));
                Auth::Inherit
            }
        },
        "apiKey" => match scheme.get("in").and_then(Value::as_str) {
            Some("header") => Auth::ApiKey { name: scheme.get("name").and_then(Value::as_str).unwrap_or("X-API-Key").into(), value: "{{apiKey}}".into(), location: ApiKeyIn::Header },
            Some("query") => Auth::ApiKey { name: scheme.get("name").and_then(Value::as_str).unwrap_or("api_key").into(), value: "{{apiKey}}".into(), location: ApiKeyIn::Query },
            _ => {
                warnings.push("An API key in a cookie is not supported; the requests have no auth.".into());
                Auth::Inherit
            }
        },
        "oauth2" | "openIdConnect" => {
            warnings.push("OAuth 2 / OpenID Connect is used as a bearer `{{token}}`: get the token yourself and set it.".into());
            Auth::Bearer { token: "{{token}}".into() }
        }
        other => {
            warnings.push(format!("Security scheme type `{other}` is not supported."));
            Auth::Inherit
        }
    }
}

// ── Examples from schemas ────────────────────────────────────────────────────────────────────────

/// A value that satisfies `schema`: its own example / default / const / first enum value when it has one,
/// otherwise built from its shape. A `$ref` that leads back to itself ends in `null`.
pub fn example(root: &Value, schema: &Value) -> Value {
    build(root, schema, 0, &mut Vec::new())
}

fn build(root: &Value, schema: &Value, depth: usize, seen: &mut Vec<String>) -> Value {
    if depth > MAX_DEPTH || !schema.is_object() {
        return Value::Null;
    }
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        if seen.iter().any(|s| s == reference) {
            return Value::Null;
        }
        let Some(target) = pointer(root, reference) else { return Value::Null };
        seen.push(reference.to_string());
        let out = build(root, target, depth + 1, seen);
        seen.pop();
        return out;
    }
    for key in ["example", "default", "const"] {
        if let Some(v) = schema.get(key) {
            return v.clone();
        }
    }
    if let Some(v) = schema.get("examples").and_then(Value::as_array).and_then(|a| a.first()) {
        return v.clone();
    }
    if let Some(v) = schema.get("enum").and_then(Value::as_array).and_then(|a| a.first()) {
        return v.clone();
    }
    if let Some(parts) = schema.get("allOf").and_then(Value::as_array) {
        let mut merged = Map::new();
        for part in parts {
            if let Value::Object(o) = build(root, part, depth + 1, seen) {
                merged.extend(o);
            }
        }
        if !merged.is_empty() {
            return Value::Object(merged);
        }
    }
    for key in ["oneOf", "anyOf"] {
        if let Some(first) = schema.get(key).and_then(Value::as_array).and_then(|a| a.first()) {
            return build(root, first, depth + 1, seen);
        }
    }
    let ty = schema.get("type").and_then(Value::as_str).or_else(|| schema.get("properties").map(|_| "object"));
    match ty {
        Some("object") => {
            let mut out = Map::new();
            if let Some(props) = schema.get("properties").and_then(Value::as_object) {
                for (name, prop) in props {
                    // A server-assigned field is not something the client sends.
                    if deref(root, prop).get("readOnly").and_then(Value::as_bool) == Some(true) {
                        continue;
                    }
                    out.insert(name.clone(), build(root, prop, depth + 1, seen));
                }
            }
            Value::Object(out)
        }
        Some("array") => {
            let item = schema.get("items").map(|i| build(root, i, depth + 1, seen)).unwrap_or(Value::Null);
            Value::Array(if item.is_null() { Vec::new() } else { vec![item] })
        }
        Some("integer") => Value::from(0),
        Some("number") => Value::from(0.0),
        Some("boolean") => Value::Bool(false),
        Some("string") => Value::String(
            match schema.get("format").and_then(Value::as_str) {
                Some("date") => "2024-01-01",
                Some("date-time") => "2024-01-01T00:00:00Z",
                Some("uuid") => "00000000-0000-0000-0000-000000000000",
                Some("email") => "user@example.com",
                Some("uri") | Some("url") => "https://example.com",
                Some("byte") => "",
                _ => "string",
            }
            .to_string(),
        ),
        _ => Value::Null,
    }
}

// ── One operation ────────────────────────────────────────────────────────────────────────────────

fn text_of(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// A parameter's value for the request: its example, its schema's, or empty.
fn param_value(root: &Value, p: &Value) -> String {
    let v = p.get("example").cloned().or_else(|| p.get("schema").map(|s| example(root, s))).unwrap_or(Value::Null);
    match v {
        Value::Array(items) => items.iter().map(text_of).collect::<Vec<_>>().join(","),
        Value::Object(_) => String::new(),
        other => text_of(&other),
    }
}

/// The parameters of an operation: the path item's, then its own, an operation's replacing one of the same
/// name and place.
fn parameters(root: &Value, item: &Value, op: &Value) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    for list in [item.get("parameters"), op.get("parameters")].into_iter().flatten().filter_map(Value::as_array) {
        for p in list {
            let p = deref(root, p).clone();
            let key = (p.get("name").and_then(Value::as_str).unwrap_or("").to_string(), p.get("in").and_then(Value::as_str).unwrap_or("").to_string());
            out.retain(|q| (q.get("name").and_then(Value::as_str).unwrap_or("").to_string(), q.get("in").and_then(Value::as_str).unwrap_or("").to_string()) != key);
            out.push(p);
        }
    }
    out
}

fn body_of(root: &Value, op: &Value, request: &mut HttpRequest, warnings: &mut Vec<String>) {
    let Some(content) = op.get("requestBody").map(|b| deref(root, b)).and_then(|b| b.get("content")).and_then(Value::as_object) else { return };
    let pick = ["application/json"]
        .into_iter()
        .find(|m| content.contains_key(*m))
        .or_else(|| content.keys().map(String::as_str).find(|m| m.contains("json")))
        .or_else(|| content.keys().map(String::as_str).find(|m| m.contains("x-www-form-urlencoded")))
        .or_else(|| content.keys().map(String::as_str).find(|m| m.contains("xml")))
        .or_else(|| content.keys().next().map(String::as_str));
    let Some(media) = pick else { return };
    let spec = &content[media];
    let sample = spec
        .get("example")
        .cloned()
        .or_else(|| spec.get("examples").and_then(Value::as_object).and_then(|e| e.values().next()).and_then(|e| deref(root, e).get("value")).cloned())
        .or_else(|| spec.get("schema").map(|s| example(root, s)))
        .unwrap_or(Value::Null);
    if media.contains("json") {
        let text = if sample.is_null() { String::new() } else { serde_json::to_string_pretty(&sample).unwrap_or_default() };
        request.body = Body { kind: BodyKind::Json, text, form: Vec::new() };
        if media != "application/json" && !media.ends_with("+json") {
            request.headers.push(KeyValue::new("Content-Type", media));
        }
    } else if media.contains("x-www-form-urlencoded") {
        let form = sample.as_object().map(|o| o.iter().map(|(k, v)| KeyValue::new(k, &text_of(v))).collect()).unwrap_or_default();
        request.body = Body { kind: BodyKind::Form, text: String::new(), form };
    } else if media.contains("xml") {
        request.body = Body { kind: BodyKind::Xml, text: String::new(), form: Vec::new() };
    } else {
        request.body = Body { kind: BodyKind::Text, text: if sample.is_null() { String::new() } else { text_of(&sample) }, form: Vec::new() };
        request.headers.push(KeyValue::new("Content-Type", media));
        if media.starts_with("multipart/") {
            warnings.push("A multipart body is not supported yet: it is imported as plain text.".into());
        }
    }
}

fn operation_of(root: &Value, path: &str, method: &str, item: &Value, op: &Value, warnings: &mut Vec<String>) -> Operation {
    let method_enum = Method::parse(&method.to_ascii_uppercase()).unwrap_or(Method::Get);
    let summary = op.get("summary").and_then(Value::as_str).unwrap_or("").trim().to_string();
    let id = format!("{} {path}", method_enum.as_str());
    let name = if !summary.is_empty() { summary.clone() } else { op.get("operationId").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| id.clone()) };
    let tag = op.get("tags").and_then(Value::as_array).and_then(|t| t.first()).and_then(Value::as_str).unwrap_or(UNTAGGED).to_string();

    let mut request = HttpRequest { name, method: method_enum, ..HttpRequest::default() };
    // `{id}` → `{{id}}`: a variable to fill in.
    request.url = format!("{{{{baseUrl}}}}{}", path.replace('{', "{{").replace('}', "}}"));

    for p in parameters(root, item, op) {
        let name = p.get("name").and_then(Value::as_str).unwrap_or("").to_string();
        if name.is_empty() {
            continue;
        }
        let required = p.get("required").and_then(Value::as_bool).unwrap_or(false);
        let row = KeyValue { key: name.clone(), value: param_value(root, &p), description: p.get("description").and_then(Value::as_str).unwrap_or("").lines().next().unwrap_or("").to_string(), enabled: required };
        match p.get("in").and_then(Value::as_str) {
            Some("query") => request.params.push(row),
            // Accept / Content-Type / Authorization are not parameters, whatever the spec says.
            Some("header") if !["accept", "content-type", "authorization"].contains(&name.to_ascii_lowercase().as_str()) => request.headers.push(row),
            Some("cookie") => warnings.push(format!("The cookie parameter `{name}` is not carried over.")),
            _ => {}
        }
    }
    body_of(root, op, &mut request, warnings);

    let mut docs = Vec::new();
    if !summary.is_empty() {
        docs.push(summary.clone());
    }
    if let Some(d) = op.get("description").and_then(Value::as_str).filter(|d| !d.trim().is_empty()) {
        docs.push(d.trim().to_string());
    }
    if let Some(responses) = op.get("responses").and_then(Value::as_object) {
        let list: Vec<String> = responses.iter().map(|(code, r)| format!("- {code}: {}", deref(root, r).get("description").and_then(Value::as_str).unwrap_or("").trim())).collect();
        if !list.is_empty() {
            docs.push(format!("Responses\n\n{}", list.join("\n")));
        }
    }
    request.docs = docs.join("\n\n");
    Operation { id, tag, path: path.to_string(), summary, request }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: &str = r##"{
      "openapi": "3.0.3",
      "info": { "title": "Pet Store", "description": "Pets." },
      "servers": [{ "url": "https://{env}.pets.example/v1/", "variables": { "env": { "default": "api" } } }],
      "security": [{ "bearer": [] }],
      "paths": {
        "/pets": {
          "get": { "tags": ["pets"], "summary": "List pets", "parameters": [
            { "name": "limit", "in": "query", "required": true, "schema": { "type": "integer", "default": 20 }, "description": "How many.\nMore." },
            { "name": "status", "in": "query", "schema": { "type": "string", "enum": ["open", "closed"] } },
            { "name": "X-Trace", "in": "header", "schema": { "type": "string" } },
            { "name": "Accept", "in": "header", "schema": { "type": "string" } } ],
            "responses": { "200": { "description": "A list" } } },
          "post": { "tags": ["pets"], "operationId": "addPet", "requestBody": { "$ref": "#/components/requestBodies/PetBody" }, "responses": { "201": { "description": "Created" } } }
        },
        "/pets/{id}": {
          "parameters": [{ "$ref": "#/components/parameters/PetId" }],
          "get": { "summary": "One pet", "responses": {} },
          "delete": { "tags": ["admin"], "responses": {} }
        }
      },
      "components": {
        "securitySchemes": { "bearer": { "type": "http", "scheme": "bearer" }, "key": { "type": "apiKey", "in": "header", "name": "X-Key" } },
        "parameters": { "PetId": { "name": "id", "in": "path", "required": true, "schema": { "type": "integer" } } },
        "requestBodies": { "PetBody": { "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Pet" } } } } },
        "schemas": {
          "Pet": { "type": "object", "required": ["name"], "properties": {
            "id": { "type": "integer", "readOnly": true },
            "name": { "type": "string", "example": "Rex" },
            "born": { "type": "string", "format": "date" },
            "tags": { "type": "array", "items": { "type": "string" } },
            "owner": { "$ref": "#/components/schemas/Owner" },
            "parent": { "$ref": "#/components/schemas/Pet" } } },
          "Owner": { "allOf": [{ "type": "object", "properties": { "email": { "type": "string", "format": "email" } } }, { "type": "object", "properties": { "level": { "type": "integer", "enum": [3, 4] } } }] }
        }
      }
    }"##;

    fn plan() -> Plan {
        parse(SPEC).unwrap()
    }

    #[test]
    fn the_title_the_server_and_the_tags_come_out() {
        let p = plan();
        assert_eq!(p.title, "Pet Store");
        assert_eq!(p.base_url.as_deref(), Some("https://api.pets.example/v1"), "variables get their defaults and the trailing slash goes");
        assert_eq!(p.tags, ["pets", "default", "admin"]);
        assert_eq!(p.operations.len(), 4);
    }

    #[test]
    fn a_path_becomes_a_url_on_base_url_with_its_variables() {
        let one = plan().operations.into_iter().find(|o| o.id == "GET /pets/{id}").unwrap();
        assert_eq!(one.request.url, "{{baseUrl}}/pets/{{id}}");
        assert_eq!(one.request.name, "One pet");
        assert_eq!(one.tag, UNTAGGED);
    }

    #[test]
    fn query_and_header_parameters_are_rows_and_only_required_ones_are_on() {
        let list = plan().operations.into_iter().find(|o| o.id == "GET /pets").unwrap().request;
        assert_eq!(list.params.len(), 2);
        assert_eq!((list.params[0].key.as_str(), list.params[0].value.as_str(), list.params[0].enabled), ("limit", "20", true));
        assert_eq!(list.params[0].description, "How many.");
        assert_eq!((list.params[1].value.as_str(), list.params[1].enabled), ("open", false), "an enum offers its first value");
        assert_eq!(list.headers.iter().map(|h| h.key.as_str()).collect::<Vec<_>>(), ["X-Trace"], "Accept is not a parameter");
        assert!(list.docs.contains("- 200: A list"));
    }

    #[test]
    fn a_ref_to_a_request_body_gives_an_example_that_skips_read_only_fields_and_cycles() {
        let add = plan().operations.into_iter().find(|o| o.id == "POST /pets").unwrap().request;
        assert_eq!(add.name, "addPet", "the operation id names it when there is no summary");
        assert_eq!(add.body.kind, BodyKind::Json);
        let body: Value = serde_json::from_str(&add.body.text).unwrap();
        assert_eq!(body["name"], "Rex");
        assert_eq!(body["born"], "2024-01-01");
        assert_eq!(body["tags"], serde_json::json!(["string"]));
        assert!(body.get("id").is_none(), "readOnly");
        assert_eq!(body["owner"]["email"], "user@example.com", "allOf is merged");
        assert_eq!(body["owner"]["level"], 3);
        assert!(body["parent"].is_null(), "a schema that contains itself stops");
    }

    #[test]
    fn security_schemes_map_to_a_folder_auth() {
        assert_eq!(plan().auth, Auth::Bearer { token: "{{token}}".into() });
        let with = |scheme: &str| SPEC.replacen("\"security\": [{ \"bearer\": [] }]", &format!("\"security\": [{{ \"{scheme}\": [] }}]"), 1);
        assert_eq!(parse(&with("key")).unwrap().auth, Auth::ApiKey { name: "X-Key".into(), value: "{{apiKey}}".into(), location: ApiKeyIn::Header });
        let basic = SPEC.replace("\"scheme\": \"bearer\"", "\"scheme\": \"basic\"");
        assert_eq!(parse(&basic).unwrap().auth, Auth::Basic { username: "{{username}}".into(), password: "{{password}}".into() });
        let oauth = SPEC.replace("{ \"type\": \"http\", \"scheme\": \"bearer\" }", "{ \"type\": \"oauth2\", \"flows\": {} }");
        let p = parse(&oauth).unwrap();
        assert_eq!(p.auth, Auth::Bearer { token: "{{token}}".into() });
        assert!(p.warnings.iter().any(|w| w.contains("OAuth 2")));
    }

    #[test]
    fn examples_prefer_what_the_schema_says() {
        let root = serde_json::json!({});
        assert_eq!(example(&root, &serde_json::json!({ "type": "integer", "example": 7 })), 7);
        assert_eq!(example(&root, &serde_json::json!({ "type": "string", "default": "x", "enum": ["y"] })), "x");
        assert_eq!(example(&root, &serde_json::json!({ "oneOf": [{ "type": "boolean" }, { "type": "string" }] })), false);
        assert_eq!(example(&root, &serde_json::json!({ "type": "array", "items": { "type": "number" } })), serde_json::json!([0.0]));
    }

    #[test]
    fn a_form_body_becomes_form_rows() {
        let spec = r##"{"openapi":"3.0.0","info":{"title":"t"},"paths":{"/login":{"post":{"requestBody":{"content":{"application/x-www-form-urlencoded":{"schema":{"type":"object","properties":{"user":{"type":"string","example":"bob"},"pass":{"type":"string"}}}}}}}}}}"##;
        let r = parse(spec).unwrap().operations.remove(0).request;
        assert_eq!(r.body.kind, BodyKind::Form);
        let mut rows: Vec<(&str, &str)> = r.body.form.iter().map(|f| (f.key.as_str(), f.value.as_str())).collect();
        rows.sort();
        assert_eq!(rows, [("pass", "string"), ("user", "bob")]);
    }

    #[test]
    fn a_spec_that_cannot_be_read_is_refused_in_words() {
        assert!(parse("openapi: 3.0.0\ninfo:\n  title: x").unwrap_err().contains("YAML"));
        assert!(parse("hello").unwrap_err().contains("not a JSON"));
        assert!(parse("{\"openapi\": \"3.0.0\", ").unwrap_err().contains("not valid JSON"));
        assert!(parse("{\"swagger\": \"2.0\"}").unwrap_err().contains("Swagger 2.0"));
        assert!(parse("{\"openapi\": \"4.0.0\"}").unwrap_err().contains("not supported"));
        assert!(parse("{\"info\": {}}").unwrap_err().contains("`openapi`"));
        assert!(parse("{\"openapi\": \"3.0.0\", \"paths\": {}}").unwrap_err().contains("no operations"));
    }
}
