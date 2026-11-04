//! A typed HTTP client for Rust from an endpoint description — the counterpart of the Java client
//! generator. `reqwest` (async or blocking) or `ureq`; serde structs for the payloads, an error enum
//! (thiserror or hand-written), bearer / basic / API-key authentication, written at the caret.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::code::Lines;
use crate::context::GenContext;
use crate::generators::rust_support::{at_caret, check_fields, require_ident, write_struct, Field};
use crate::names::snake;
use crate::output::{parse_options, Output};
use crate::registry::{Generator, GeneratorMeta, Target};

pub struct RustHttpClient;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Endpoint {
    /// The method's name (snake_case).
    pub name: String,
    /// `GET` | `POST` | `PUT` | `PATCH` | `DELETE`
    pub method: String,
    /// `/orders/{id}` — each `{var}` becomes an argument.
    pub path: String,
    /// Query parameters; a type starting with `Option<` is sent only when present.
    pub query: Vec<Field>,
    /// Request headers, same rule.
    pub headers: Vec<Field>,
    /// The JSON body's type; empty for none.
    pub body: String,
    /// The JSON response's type; empty for none.
    pub returns: String,
}

impl Default for Endpoint {
    fn default() -> Self {
        Endpoint { name: String::new(), method: "GET".into(), path: "/".into(), query: Vec::new(), headers: Vec::new(), body: String::new(), returns: String::new() }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct StructDef {
    pub name: String,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RustClientOptions {
    /// `reqwest` | `reqwest-blocking` | `ureq`
    pub library: String,
    pub client_name: String,
    pub error_name: String,
    pub endpoints: Vec<Endpoint>,
    pub structs: Vec<StructDef>,
    /// `none` | `bearer` | `basic` | `api-key`
    pub auth: String,
    pub api_key_header: String,
    /// `thiserror` | `std`
    pub errors: String,
    pub timeout_seconds: u32,
    /// `serde(rename_all = …)` on the structs: `camelCase`, `snake_case`, … or empty.
    pub rename_all: String,
    /// `json` | `xml` (bodies and responses through `quick-xml`).
    pub content: String,
}

impl Default for RustClientOptions {
    fn default() -> Self {
        RustClientOptions {
            library: "reqwest".into(),
            client_name: "ApiClient".into(),
            error_name: "ClientError".into(),
            endpoints: Vec::new(),
            structs: Vec::new(),
            auth: "none".into(),
            api_key_header: "X-API-Key".into(),
            errors: "thiserror".into(),
            timeout_seconds: 30,
            rename_all: String::new(),
            content: "json".into(),
        }
    }
}

impl Generator for RustHttpClient {
    fn meta(&self) -> GeneratorMeta {
        GeneratorMeta {
            id: "rust-http-client",
            title: "Generate Rust HTTP client",
            description: "A typed client from an endpoint description: reqwest (async or blocking) or ureq, serde structs, an error enum and authentication.",
            icon: "Globe",
            group: "Rust",
            language: "rust",
            targets: &[Target::Rust],
            related: &["reqwest", "ureq"],
        }
    }

    fn defaults(&self, ctx: &GenContext) -> Value {
        let library = if ctx.project.has("ureq") && !ctx.project.has("reqwest") { "ureq" } else { "reqwest" };
        let o = RustClientOptions {
            library: library.into(),
            rename_all: "camelCase".into(),
            structs: vec![
                StructDef { name: "Order".into(), fields: vec![Field::new("id", "u64"), Field::new("total", "f64"), Field::new("status", "String")] },
                StructDef { name: "NewOrder".into(), fields: vec![Field::new("customer_id", "u64"), Field::new("lines", "Vec<String>")] },
            ],
            endpoints: vec![
                Endpoint { name: "get_order".into(), path: "/orders/{id}".into(), returns: "Order".into(), ..Endpoint::default() },
                Endpoint { name: "list_orders".into(), path: "/orders".into(), query: vec![Field::new("status", "Option<&str>"), Field::new("page", "u32")], returns: "Vec<Order>".into(), ..Endpoint::default() },
                Endpoint { name: "create_order".into(), method: "POST".into(), path: "/orders".into(), body: "NewOrder".into(), returns: "Order".into(), ..Endpoint::default() },
                Endpoint { name: "delete_order".into(), method: "DELETE".into(), path: "/orders/{id}".into(), ..Endpoint::default() },
            ],
            ..RustClientOptions::default()
        };
        serde_json::to_value(o).unwrap_or(Value::Null)
    }

    fn generate(&self, options: &Value, _ctx: &GenContext) -> Result<Output, String> {
        let o: RustClientOptions = parse_options(options)?;
        validate(&o)?;
        let mut l = Lines::new();
        write_structs(&mut l, &o);
        l.gap();
        write_error(&mut l, &o);
        l.gap();
        write_client(&mut l, &o);
        let mut out = at_caret(l.finish());
        out = out.extra("Cargo.toml", "toml", cargo_snippet(&o));
        if o.library == "reqwest" {
            out = out.note("The async client needs a runtime — `#[tokio::main]` in the binary that calls it.");
        }
        if o.auth != "none" {
            out = out.note("The credentials are taken by `new`; read them from the environment or a config file, not from source.");
        }
        Ok(out)
    }
}

fn validate(o: &RustClientOptions) -> Result<(), String> {
    require_ident(&o.client_name, "Client name")?;
    require_ident(&o.error_name, "Error name")?;
    if !["reqwest", "reqwest-blocking", "ureq"].contains(&o.library.as_str()) {
        return Err(format!("Unknown library `{}`", o.library));
    }
    if o.endpoints.is_empty() {
        return Err("Describe at least one endpoint".into());
    }
    for s in &o.structs {
        require_ident(&s.name, "Struct")?;
        check_fields(&s.fields, "Field")?;
    }
    let mut seen: Vec<&str> = Vec::new();
    for e in &o.endpoints {
        require_ident(&e.name, "Method")?;
        if seen.contains(&e.name.as_str()) {
            return Err(format!("Two endpoints are called `{}`", e.name));
        }
        seen.push(&e.name);
        if !["GET", "POST", "PUT", "PATCH", "DELETE"].contains(&e.method.as_str()) {
            return Err(format!("`{}`: unknown method `{}`", e.name, e.method));
        }
        if !e.path.starts_with('/') {
            return Err(format!("`{}`: the path starts with `/`", e.name));
        }
        check_fields(&e.query, "Query parameter")?;
        for h in &e.headers {
            if h.name.trim().is_empty() || h.name.contains(['"', '\\']) {
                return Err(format!("`{}`: a header needs a name", e.name));
            }
        }
        for var in path_vars(&e.path) {
            require_ident(&var, "Path variable")?;
        }
    }
    Ok(())
}

/// `/a/{id}/b/{Item-Id:\d+}` → `["id", "item_id"]`.
pub fn path_vars(path: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = path;
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}') else { break };
        out.push(snake(rest[open + 1..open + close].split(':').next().unwrap_or("").trim()));
        rest = &rest[open + close + 1..];
    }
    out
}

/// The path as a `format!` string: `{Item-Id:\d+}` → `{item_id}`.
fn format_path(path: &str) -> String {
    let mut out = String::new();
    let mut rest = path;
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}') else { break };
        out.push_str(&rest[..open]);
        out.push_str(&format!("{{{}}}", snake(rest[open + 1..open + close].split(':').next().unwrap_or("").trim())));
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
    out
}

fn is_optional(ty: &str) -> bool {
    ty.trim_start().starts_with("Option<")
}

fn write_structs(l: &mut Lines, o: &RustClientOptions) {
    let attrs: Vec<String> = if o.rename_all.is_empty() { Vec::new() } else { vec![format!("#[serde(rename_all = \"{}\")]", o.rename_all)] };
    for (i, s) in o.structs.iter().enumerate() {
        if i > 0 {
            l.blank();
        }
        write_struct(l, &s.name, &s.fields, &["Debug", "Clone", "serde::Serialize", "serde::Deserialize"], &attrs);
    }
}

struct Variant {
    name: &'static str,
    ty: String,
    message: &'static str,
    from: bool,
}

fn variants(o: &RustClientOptions) -> Vec<Variant> {
    let mut out = match o.library.as_str() {
        "ureq" => vec![
            Variant { name: "Transport", ty: "Box<ureq::Error>".into(), message: "request failed: {0}", from: false },
            Variant { name: "Io", ty: "std::io::Error".into(), message: "cannot read the response: {0}", from: true },
        ],
        _ => vec![Variant { name: "Http", ty: "reqwest::Error".into(), message: "request failed: {0}", from: true }],
    };
    if o.content == "xml" {
        out.push(Variant { name: "Xml", ty: "quick_xml::DeError".into(), message: "cannot read the XML response: {0}", from: true });
        out.push(Variant { name: "XmlWrite", ty: "quick_xml::SeError".into(), message: "cannot write the XML body: {0}", from: true });
    }
    out.push(Variant { name: "Status", ty: String::new(), message: "", from: false });
    out
}

fn write_error(l: &mut Lines, o: &RustClientOptions) {
    let name = &o.error_name;
    let vs = variants(o);
    if o.errors == "thiserror" {
        l.line("#[derive(Debug, thiserror::Error)]");
        l.block(format!("pub enum {name}"), |l| {
            for v in &vs {
                if v.name == "Status" {
                    l.line("#[error(\"the server answered {status}: {body}\")]");
                    l.line("Status { status: u16, body: String },");
                } else {
                    l.line(format!("#[error(\"{}\")]", v.message));
                    l.line(format!("{}({}),", v.name, if v.from { format!("#[from] {}", v.ty) } else { v.ty.clone() }));
                }
            }
        });
        return;
    }
    l.line("#[derive(Debug)]");
    l.block(format!("pub enum {name}"), |l| {
        for v in &vs {
            if v.name == "Status" {
                l.line("Status { status: u16, body: String },");
            } else {
                l.line(format!("{}({}),", v.name, v.ty));
            }
        }
    });
    l.blank();
    l.block(format!("impl std::fmt::Display for {name}"), |l| {
        l.block("fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result", |l| {
            l.block("match self", |l| {
                for v in &vs {
                    if v.name == "Status" {
                        l.line("Self::Status { status, body } => write!(f, \"the server answered {status}: {body}\"),");
                    } else {
                        l.line(format!("Self::{}(e) => write!(f, \"{}\", e),", v.name, v.message.replace("{0}", "{}")));
                    }
                }
            });
        });
    });
    l.blank();
    l.block(format!("impl std::error::Error for {name}"), |l| {
        l.block("fn source(&self) -> Option<&(dyn std::error::Error + 'static)>", |l| {
            l.block("match self", |l| {
                for v in vs.iter().filter(|v| v.name != "Status") {
                    l.line(format!("Self::{}(e) => Some(e),", v.name));
                }
                l.line("_ => None,");
            });
        });
    });
    for v in vs.iter().filter(|v| v.from) {
        l.blank();
        l.block(format!("impl From<{}> for {name}", v.ty), |l| {
            l.block(format!("fn from(e: {}) -> Self", v.ty), |l| {
                l.line(format!("Self::{}(e)", v.name));
            });
        });
    }
}

fn credentials(o: &RustClientOptions) -> Vec<Field> {
    match o.auth.as_str() {
        "bearer" => vec![Field::new("token", "impl Into<String>")],
        "basic" => vec![Field::new("username", "impl Into<String>"), Field::new("password", "impl Into<String>")],
        "api-key" => vec![Field::new("api_key", "impl Into<String>")],
        _ => Vec::new(),
    }
}

fn write_client(l: &mut Lines, o: &RustClientOptions) {
    let name = &o.client_name;
    let err = &o.error_name;
    let is_async = o.library == "reqwest";
    let (kw, aw) = if is_async { ("async ", ".await") } else { ("", "") };
    let creds = credentials(o);
    let http_ty = match o.library.as_str() {
        "reqwest" => "reqwest::Client",
        "reqwest-blocking" => "reqwest::blocking::Client",
        _ => "ureq::Agent",
    };
    l.block(format!("pub struct {name}"), |l| {
        l.line(format!("http: {http_ty},"));
        l.line("base_url: String,");
        for c in &creds {
            l.line(format!("{}: String,", c.name));
        }
    });
    l.blank();
    l.block(format!("impl {name}"), |l| {
        // constructor
        let mut params = vec!["base_url: impl Into<String>".to_string()];
        params.extend(creds.iter().map(|c| format!("{}: {}", c.name, c.ty)));
        let ret = if o.library == "ureq" { "Self".to_string() } else { format!("Result<Self, {err}>") };
        l.block(format!("pub fn new({}) -> {ret}", params.join(", ")), |l| {
            match o.library.as_str() {
                "ureq" => {
                    l.line(format!("let http = ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs({})).build();", o.timeout_seconds));
                }
                lib => {
                    let client = if lib == "reqwest" { "reqwest::Client" } else { "reqwest::blocking::Client" };
                    l.line(format!("let http = {client}::builder().timeout(std::time::Duration::from_secs({})).build()?;", o.timeout_seconds));
                }
            }
            let mut fields = vec!["http".to_string(), "base_url: base_url.into().trim_end_matches('/').to_string()".to_string()];
            fields.extend(creds.iter().map(|c| format!("{0}: {0}.into()", c.name)));
            let built = format!("Self {{ {} }}", fields.join(", "));
            l.line(if o.library == "ureq" { built } else { format!("Ok({built})") });
        });
        l.blank();
        write_request_helper(l, o);
        for e in &o.endpoints {
            l.blank();
            write_endpoint(l, o, e, kw, aw);
        }
        l.blank();
        write_senders(l, o, kw, aw);
    });
}

fn write_request_helper(l: &mut Lines, o: &RustClientOptions) {
    let auth_lines = |l: &mut Lines, var: &str| {
        match o.auth.as_str() {
        "bearer" if o.library == "ureq" => l.line(format!("{var}.set(\"Authorization\", &format!(\"Bearer {{}}\", self.token))")),
        "bearer" => l.line(format!("{var}.bearer_auth(&self.token)")),
        "basic" if o.library == "ureq" => {
            l.line(format!("{var}.set(\"Authorization\", &format!(\"Basic {{}}\", basic_credentials(&self.username, &self.password)))"))
        }
        "basic" => l.line(format!("{var}.basic_auth(&self.username, Some(&self.password))")),
        "api-key" if o.library == "ureq" => l.line(format!("{var}.set(\"{}\", &self.api_key)", o.api_key_header)),
        "api-key" => l.line(format!("{var}.header(\"{}\", &self.api_key)", o.api_key_header)),
        _ => l.line(var),
        };
    };
    if o.library == "ureq" {
        l.block("fn request(&self, method: &str, path: &str) -> ureq::Request", |l| {
            l.line("let request = self.http.request(method, &format!(\"{}{}\", self.base_url, path));");
            auth_lines(l, "request");
        });
        return;
    }
    let (method, builder) = if o.library == "reqwest" { ("reqwest::Method", "reqwest::RequestBuilder") } else { ("reqwest::Method", "reqwest::blocking::RequestBuilder") };
    l.block(format!("fn request(&self, method: {method}, path: &str) -> {builder}"), |l| {
        l.line("let request = self.http.request(method, format!(\"{}{}\", self.base_url, path));");
        auth_lines(l, "request");
    });
}

fn write_endpoint(l: &mut Lines, o: &RustClientOptions, e: &Endpoint, kw: &str, aw: &str) {
    let err = &o.error_name;
    let mut params = vec!["&self".to_string()];
    params.extend(path_vars(&e.path).iter().map(|v| format!("{v}: impl std::fmt::Display")));
    params.extend(e.query.iter().map(|q| format!("{}: {}", q.name, q.ty)));
    params.extend(e.headers.iter().map(|h| format!("{}: {}", snake(&h.name), if h.ty.trim().is_empty() { "&str" } else { h.ty.trim() })));
    if !e.body.is_empty() {
        params.push(format!("body: &{}", e.body));
    }
    let ret = if e.returns.is_empty() { "()".to_string() } else { e.returns.clone() };
    let path = format_path(&e.path);
    l.block(format!("pub {kw}fn {}({}) -> Result<{ret}, {err}>", e.name, params.join(", ")), |l| {
        if o.library == "ureq" {
            write_ureq_body(l, o, e, &path);
            return;
        }
        l.line(format!("let mut request = self.request(reqwest::Method::{}, &format!(\"{path}\"));", e.method));
        for q in &e.query {
            if is_optional(&q.ty) {
                l.line(format!("request = request.query(&[(\"{0}\", {0})]);", q.name));
            } else {
                l.line(format!("request = request.query(&[(\"{0}\", &{0})]);", q.name));
            }
        }
        for h in &e.headers {
            let var = snake(&h.name);
            if is_optional(&h.ty) {
                l.block(format!("if let Some(value) = {var}"), |l| {
                    l.line(format!("request = request.header(\"{}\", value);", h.name));
                });
            } else {
                l.line(format!("request = request.header(\"{}\", {var});", h.name));
            }
        }
        if o.content == "xml" {
            l.line("request = request.header(\"Accept\", \"application/xml\");");
        }
        if !e.body.is_empty() {
            if o.content == "xml" {
                l.line("request = request.header(\"Content-Type\", \"application/xml\").body(quick_xml::se::to_string(body)?);");
            } else {
                l.line("request = request.json(body);");
            }
        }
        let send = if e.returns.is_empty() { "send_empty" } else { "send" };
        l.line(format!("self.{send}(request){aw}"));
    });
}

fn write_ureq_body(l: &mut Lines, o: &RustClientOptions, e: &Endpoint, path: &str) {
    l.line(format!("let mut request = self.request(\"{}\", &format!(\"{path}\"));", e.method));
    for q in &e.query {
        if is_optional(&q.ty) {
            l.block(format!("if let Some(value) = {}", q.name), |l| {
                l.line(format!("request = request.query(\"{}\", &value.to_string());", q.name));
            });
        } else {
            l.line(format!("request = request.query(\"{0}\", &{0}.to_string());", q.name));
        }
    }
    for h in &e.headers {
        let var = snake(&h.name);
        if is_optional(&h.ty) {
            l.block(format!("if let Some(value) = {var}"), |l| {
                l.line(format!("request = request.set(\"{}\", value);", h.name));
            });
        } else {
            l.line(format!("request = request.set(\"{}\", {var});", h.name));
        }
    }
    let xml = o.content == "xml";
    if xml {
        l.line("request = request.set(\"Accept\", \"application/xml\");");
    }
    let call = match (e.body.is_empty(), xml) {
        (true, _) => "request.call()".to_string(),
        (false, true) => "request.set(\"Content-Type\", \"application/xml\").send_string(&quick_xml::se::to_string(body)?)".to_string(),
        (false, false) => "request.send_json(body)".to_string(),
    };
    if e.returns.is_empty() {
        l.line(format!("Self::check({call})?;"));
        l.line("Ok(())");
    } else if xml {
        l.line(format!("Ok(quick_xml::de::from_str(&Self::check({call})?.into_string()?)?)"));
    } else {
        l.line(format!("Ok(Self::check({call})?.into_json()?)"));
    }
}

fn write_senders(l: &mut Lines, o: &RustClientOptions, kw: &str, aw: &str) {
    let err = &o.error_name;
    if o.library == "ureq" {
        l.block(format!("fn check(result: Result<ureq::Response, ureq::Error>) -> Result<ureq::Response, {err}>"), |l| {
            l.block("match result", |l| {
                l.line("Ok(response) => Ok(response),");
                l.line(format!("Err(ureq::Error::Status(status, response)) => Err({err}::Status {{ status, body: response.into_string().unwrap_or_default() }}),"));
                l.line(format!("Err(e) => Err({err}::Transport(Box::new(e))),"));
            });
        });
        if o.auth == "basic" {
            l.blank();
            l.block("fn basic_credentials(username: &str, password: &str) -> String", |l| {
                l.line("// Base64 of `username:password`, written out to avoid a dependency for one header.");
                l.line("const TABLE: &[u8; 64] = b\"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/\";");
                l.line("let bytes = format!(\"{username}:{password}\").into_bytes();");
                l.line("let mut out = String::new();");
                l.block("for chunk in bytes.chunks(3)", |l| {
                    l.line("let n = chunk.iter().enumerate().fold(0u32, |acc, (i, b)| acc | (*b as u32) << (16 - 8 * i));");
                    l.block("for i in 0..4", |l| {
                        l.block("if i <= chunk.len()", |l| {
                            l.line("out.push(TABLE[(n >> (18 - 6 * i) & 63) as usize] as char);");
                        });
                        l.block("else", |l| {
                            l.line("out.push('=');");
                        });
                    });
                });
                l.line("out");
            });
        }
        return;
    }
    let (req, resp) = if o.library == "reqwest" { ("reqwest::RequestBuilder", "reqwest::Response") } else { ("reqwest::blocking::RequestBuilder", "reqwest::blocking::Response") };
    l.block(format!("{kw}fn send<T: serde::de::DeserializeOwned>(&self, request: {req}) -> Result<T, {err}>"), |l| {
        l.line(format!("let response = Self::check(request.send(){aw}?){aw}?;"));
        if o.content == "xml" {
            l.line(format!("let text = response.text(){aw}?;"));
            l.line("Ok(quick_xml::de::from_str(&text)?)");
        } else {
            l.line(format!("Ok(response.json(){aw}?)"));
        }
    });
    l.blank();
    l.block(format!("{kw}fn send_empty(&self, request: {req}) -> Result<(), {err}>"), |l| {
        l.line(format!("Self::check(request.send(){aw}?){aw}?;"));
        l.line("Ok(())");
    });
    l.blank();
    l.block(format!("{kw}fn check(response: {resp}) -> Result<{resp}, {err}>"), |l| {
        l.line("let status = response.status();");
        l.block("if status.is_success()", |l| {
            l.line("return Ok(response);");
        });
        l.line(format!("let body = response.text(){aw}.unwrap_or_default();"));
        l.line(format!("Err({err}::Status {{ status: status.as_u16(), body }})"));
    });
}

fn cargo_snippet(o: &RustClientOptions) -> String {
    let mut text = String::from("serde = { version = \"1\", features = [\"derive\"] }\n");
    match o.library.as_str() {
        "reqwest" => text.push_str("reqwest = { version = \"0.12\", features = [\"json\"] }\ntokio = { version = \"1\", features = [\"rt-multi-thread\", \"macros\"] }\n"),
        "reqwest-blocking" => text.push_str("reqwest = { version = \"0.12\", features = [\"json\", \"blocking\"] }\n"),
        _ => text.push_str("ureq = { version = \"2\", features = [\"json\"] }\n"),
    }
    if o.errors == "thiserror" {
        text.push_str("thiserror = \"1\"\n");
    }
    if o.content == "xml" {
        text.push_str("quick-xml = { version = \"0.37\", features = [\"serialize\"] }\n");
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::sample_context;

    fn gen(extra: serde_json::Value) -> Output {
        let mut options = RustHttpClient.defaults(&sample_context());
        if let (Some(base), Some(add)) = (options.as_object_mut(), extra.as_object()) {
            for (k, v) in add {
                base.insert(k.clone(), v.clone());
            }
        }
        RustHttpClient.generate(&options, &sample_context()).expect("generates")
    }

    fn text(out: &Output) -> String {
        out.members.as_ref().unwrap().text.clone()
    }

    #[test]
    fn path_variables_become_snake_case_arguments_and_format_captures() {
        assert_eq!(path_vars("/a/{id}/b/{Item-Id:\\d+}"), vec!["id", "item_id"]);
        assert_eq!(format_path("/a/{id}/b/{Item-Id:\\d+}"), "/a/{id}/b/{item_id}");
    }

    #[test]
    fn the_async_client_sends_through_shared_helpers() {
        let t = text(&gen(serde_json::json!({})));
        assert!(t.contains("pub async fn get_order(&self, id: impl std::fmt::Display) -> Result<Order, ClientError> {"), "{t}");
        assert!(t.contains("let mut request = self.request(reqwest::Method::GET, &format!(\"/orders/{id}\"));"), "{t}");
        assert!(t.contains("self.send(request).await"), "{t}");
        assert!(t.contains("self.send_empty(request).await"), "{t}");
        assert!(t.contains("request = request.json(body);"), "{t}");
        assert!(t.contains("async fn check(response: reqwest::Response)"), "{t}");
    }

    #[test]
    fn optional_query_parameters_are_passed_as_options() {
        let t = text(&gen(serde_json::json!({})));
        assert!(t.contains("request = request.query(&[(\"status\", status)]);"), "{t}");
        assert!(t.contains("request = request.query(&[(\"page\", &page)]);"), "{t}");
    }

    #[test]
    fn structs_derive_serde_and_rename() {
        let t = text(&gen(serde_json::json!({})));
        assert!(t.contains("#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]\n#[serde(rename_all = \"camelCase\")]\npub struct Order {"), "{t}");
        assert!(t.contains("pub id: u64,"), "{t}");
    }

    #[test]
    fn thiserror_derives_and_the_std_style_writes_the_impls() {
        let t = text(&gen(serde_json::json!({})));
        assert!(t.contains("#[derive(Debug, thiserror::Error)]"), "{t}");
        assert!(t.contains("Http(#[from] reqwest::Error),"), "{t}");
        let plain = text(&gen(serde_json::json!({ "errors": "std" })));
        assert!(plain.contains("impl std::error::Error for ClientError"), "{plain}");
        assert!(plain.contains("impl From<reqwest::Error> for ClientError"), "{plain}");
        assert!(!plain.contains("thiserror"), "{plain}");
    }

    #[test]
    fn the_blocking_client_has_no_await() {
        let t = text(&gen(serde_json::json!({ "library": "reqwest-blocking" })));
        assert!(t.contains("pub fn get_order(&self"), "{t}");
        assert!(!t.contains(".await"), "{t}");
        assert!(t.contains("reqwest::blocking::Client::builder()"), "{t}");
    }

    #[test]
    fn ureq_maps_status_errors_and_reads_json() {
        let out = gen(serde_json::json!({ "library": "ureq" }));
        let t = text(&out);
        assert!(t.contains("Err(ureq::Error::Status(status, response)) => Err(ClientError::Status"), "{t}");
        assert!(t.contains("Ok(Self::check(request.send_json(body))?.into_json()?)"), "{t}");
        assert!(t.contains("Transport(Box<ureq::Error>),"), "{t}");
        assert!(out.extras[0].text.contains("ureq = { version = \"2\""));
    }

    #[test]
    fn authentication_adds_constructor_arguments_and_headers() {
        let t = text(&gen(serde_json::json!({ "auth": "bearer" })));
        assert!(t.contains("pub fn new(base_url: impl Into<String>, token: impl Into<String>) -> Result<Self, ClientError>"), "{t}");
        assert!(t.contains("request.bearer_auth(&self.token)"), "{t}");
        let key = text(&gen(serde_json::json!({ "auth": "api-key", "api_key_header": "X-Key" })));
        assert!(key.contains("request.header(\"X-Key\", &self.api_key)"), "{key}");
        let basic = text(&gen(serde_json::json!({ "auth": "basic", "library": "ureq" })));
        assert!(basic.contains("fn basic_credentials("), "{basic}");
    }

    #[test]
    fn headers_optional_ones_are_sent_only_when_present() {
        let t = text(&gen(serde_json::json!({ "endpoints": [{ "name": "ping", "path": "/ping", "headers": [{ "name": "X-Tenant", "ty": "Option<&str>" }] }] })));
        assert!(t.contains("if let Some(value) = x_tenant {"), "{t}");
        assert!(t.contains("x_tenant: Option<&str>"), "{t}");
    }

    #[test]
    fn bad_descriptions_are_refused() {
        let ctx = sample_context();
        let bad = |v: serde_json::Value| RustHttpClient.generate(&v, &ctx).is_err();
        assert!(bad(serde_json::json!({ "endpoints": [] })));
        assert!(bad(serde_json::json!({ "endpoints": [{ "name": "fn", "path": "/" }] })));
        assert!(bad(serde_json::json!({ "endpoints": [{ "name": "a", "path": "no-slash" }] })));
        assert!(bad(serde_json::json!({ "endpoints": [{ "name": "a", "path": "/", "method": "FETCH" }] })));
        assert!(bad(serde_json::json!({ "endpoints": [{ "name": "a", "path": "/" }, { "name": "a", "path": "/b" }] })));
    }

    #[test]
    fn xml_bodies_go_through_quick_xml_with_their_own_error_variants() {
        let out = gen(serde_json::json!({ "content": "xml" }));
        let t = text(&out);
        assert!(t.contains("request = request.header(\"Accept\", \"application/xml\");"), "{t}");
        assert!(t.contains(".body(quick_xml::se::to_string(body)?);"), "{t}");
        assert!(t.contains("Ok(quick_xml::de::from_str(&text)?)"), "{t}");
        assert!(t.contains("Xml(#[from] quick_xml::DeError),"), "{t}");
        assert!(out.extras[0].text.contains("quick-xml ="), "{}", out.extras[0].text);
        let ureq = text(&gen(serde_json::json!({ "content": "xml", "library": "ureq" })));
        assert!(ureq.contains(".send_string(&quick_xml::se::to_string(body)?)"), "{ureq}");
        assert!(ureq.contains("quick_xml::de::from_str(&Self::check("), "{ureq}");
    }

    #[test]
    fn the_cargo_snippet_follows_the_library() {
        let out = gen(serde_json::json!({ "errors": "std" }));
        let c = &out.extras[0].text;
        assert!(c.contains("tokio ="), "{c}");
        assert!(!c.contains("thiserror"), "{c}");
    }
}
