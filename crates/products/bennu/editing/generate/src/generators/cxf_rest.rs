//! Apache CXF / JAX-RS client code → a modern REST client. Paste the JAX-RS interface (or resource class)
//! — `@Path`, `@GET`, `@PathParam`, `@QueryParam`, `@HeaderParam`, `@Consumes` / `@Produces` — and get the
//! same API as a `RestClient`, `WebClient`, `RestTemplate`, `@HttpExchange` interface, OpenFeign,
//! `java.net.http`, OkHttp or Retrofit client — or a Rust `reqwest` / `ureq` one — keeping XML bodies.
//!
//! The Java output is the HTTP client generator's own, run on the endpoints read from the pasted code, so
//! both stay one thing; this module is the reader of the CXF side and the Rust type mapping.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::context::{EndpointInfo, EndpointParam, GenContext};
use crate::generators::http_client::HttpClient;
use crate::generators::rust_http_client::RustHttpClient;
use crate::names::{generic_args, is_identifier, simple_name, snake, strip_generics};
use crate::output::{parse_options, GenFile, Output, SourceRoot};
use crate::registry::{Generator, GeneratorMeta, Target};

pub struct CxfRest;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CxfOptions {
    /// The JAX-RS interface or resource class as pasted.
    pub source: String,
    /// `restclient` | `webclient` | `resttemplate` | `httpinterface` | `feign` | `jdk` | `okhttp` | `retrofit` |
    /// `reqwest` | `reqwest-blocking` | `ureq`
    pub target: String,
    /// `auto` (XML when `@Consumes` / `@Produces` say so) | `xml` | `json`
    pub content: String,
    /// `jackson` | `jaxb`
    pub xml_binding: String,
    pub class_name: String,
    pub package: String,
    pub auth: String,
    /// `exception` | `default`
    pub errors: String,
    pub timeout_seconds: u32,
}

pub const SAMPLE: &str = r#"@Path("/orders")
@Produces(MediaType.APPLICATION_XML)
@Consumes(MediaType.APPLICATION_XML)
public interface OrderResource {

    @GET
    @Path("/{id}")
    Order get(@PathParam("id") long id, @QueryParam("expand") String expand);

    @GET
    List<Order> list(@QueryParam("status") String status);

    @POST
    Order create(Order order, @HeaderParam("X-Tenant") String tenant);

    @DELETE
    @Path("/{id}")
    void delete(@PathParam("id") long id);
}
"#;

impl Default for CxfOptions {
    fn default() -> Self {
        CxfOptions {
            source: SAMPLE.to_string(),
            target: "restclient".into(),
            content: "auto".into(),
            xml_binding: "jaxb".into(),
            class_name: String::new(),
            package: String::new(),
            auth: "none".into(),
            errors: "exception".into(),
            timeout_seconds: 30,
        }
    }
}

impl Generator for CxfRest {
    fn meta(&self) -> GeneratorMeta {
        GeneratorMeta {
            id: "cxf-rest",
            title: "Convert CXF REST client to a modern client",
            description: "Paste a JAX-RS / CXF interface and get a RestClient, WebClient, JDK, OkHttp, Retrofit … or Rust reqwest / ureq client, keeping XML bodies.",
            icon: "RefreshCw",
            group: "Web services",
            language: "java",
            targets: &[Target::Java, Target::Folder, Target::Project],
            related: &["cxf-rt-rs-client", "cxf-rt-frontend-jaxrs", "jakarta.ws.rs-api", "javax.ws.rs-api"],
        }
    }

    fn defaults(&self, ctx: &GenContext) -> Value {
        let target = if ctx.project.version("spring-web").is_some_and(|v| v.at_least(6, 1)) { "restclient" } else { "jdk" };
        serde_json::to_value(CxfOptions { target: target.into(), package: ctx.package_for("client"), ..CxfOptions::default() }).unwrap_or(Value::Null)
    }

    fn generate(&self, options: &Value, ctx: &GenContext) -> Result<Output, String> {
        let o: CxfOptions = parse_options(options)?;
        let api = parse_cxf(&o.source)?;
        let xml = match o.content.as_str() {
            "xml" => true,
            "json" => false,
            _ => api.xml,
        };
        let rust = matches!(o.target.as_str(), "reqwest" | "reqwest-blocking" | "ureq");
        let mut notes = api.notes.clone();
        let mut out = if rust { rust_output(&o, &api, xml, &mut notes)? } else { java_output(&o, &api, xml, ctx, &mut notes)? };
        for n in notes {
            out = out.note(n);
        }
        Ok(out)
    }
}

// ── The CXF side ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Path,
    Query,
    Header,
    Body,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CxfParam {
    pub kind: Kind,
    /// The name on the wire (the annotation's argument); the variable for a body.
    pub name: String,
    pub ty: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CxfEndpoint {
    pub http: String,
    pub path: String,
    pub name: String,
    pub params: Vec<CxfParam>,
    pub returns: String,
}

#[derive(Debug, Clone, Default)]
pub struct CxfApi {
    pub class_name: String,
    pub endpoints: Vec<CxfEndpoint>,
    pub xml: bool,
    pub notes: Vec<String>,
}

/// Statements, declarations and class headers of `source`, split at `;`, `{` and `}` outside strings and comments.
fn chunks(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = source.chars().peekable();
    let mut quote: Option<char> = None;
    while let Some(c) = chars.next() {
        if let Some(q) = quote {
            cur.push(c);
            if c == '\\' {
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            } else if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => {
                quote = Some(c);
                cur.push(c);
            }
            '/' if chars.peek() == Some(&'/') => {
                for n in chars.by_ref() {
                    if n == '\n' {
                        break;
                    }
                }
                cur.push(' ');
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut prev = ' ';
                for n in chars.by_ref() {
                    if prev == '*' && n == '/' {
                        break;
                    }
                    prev = n;
                }
                cur.push(' ');
            }
            ';' | '{' | '}' => {
                if !cur.trim().is_empty() {
                    out.push(cur.trim().to_string());
                }
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

/// The `@Name(args)` annotations a text starts with, and what follows them.
fn leading_annotations(text: &str) -> (Vec<(String, String)>, &str) {
    let mut rest = text.trim_start();
    let mut out = Vec::new();
    while rest.starts_with('@') && !rest.starts_with("@interface") {
        let name_end = rest[1..].find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.')).map_or(rest.len(), |i| i + 1);
        let name = rest[1..name_end].rsplit('.').next().unwrap_or("").to_string();
        let mut after = rest[name_end..].trim_start();
        let mut arg = String::new();
        if after.starts_with('(') {
            let mut depth = 0;
            let mut quote: Option<char> = None;
            let mut end = after.len();
            for (i, c) in after.char_indices() {
                match (quote, c) {
                    (Some(q), c) if c == q => quote = None,
                    (Some(_), _) => {}
                    (None, '"') => quote = Some('"'),
                    (None, '(') => depth += 1,
                    (None, ')') => {
                        depth -= 1;
                        if depth == 0 {
                            end = i;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            arg = after[1..end].to_string();
            after = after[(end + 1).min(after.len())..].trim_start();
        }
        out.push((name, arg));
        rest = after;
    }
    (out, rest)
}

/// The text of an annotation argument: its first string literal, or the media type a `MediaType.X` constant means.
fn string_arg(arg: &str) -> String {
    if let Some(start) = arg.find('"') {
        if let Some(len) = arg[start + 1..].find('"') {
            return arg[start + 1..start + 1 + len].to_string();
        }
    }
    let upper = arg.to_uppercase();
    for (constant, media) in [("APPLICATION_XML", "application/xml"), ("APPLICATION_JSON", "application/json"), ("TEXT_XML", "text/xml"), ("TEXT_PLAIN", "text/plain"), ("APPLICATION_ATOM_XML", "application/atom+xml")] {
        if upper.contains(constant) {
            return media.to_string();
        }
    }
    arg.trim().to_string()
}

fn annotation<'a>(anns: &'a [(String, String)], name: &str) -> Option<&'a str> {
    anns.iter().find(|(n, _)| n == name).map(|(_, a)| a.as_str())
}

fn join_paths(base: &str, path: &str) -> String {
    let joined = format!("/{}/{}", base.trim_matches('/'), path.trim_matches('/'));
    let joined = joined.replace("//", "/");
    let trimmed = joined.trim_end_matches('/');
    if trimmed.is_empty() { "/".to_string() } else { trimmed.to_string() }
}

/// Split at the commas that are not inside `<>` or `()`.
fn split_top_level(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for c in text.chars() {
        match c {
            '<' | '(' => depth += 1,
            '>' | ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(std::mem::take(&mut cur));
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

const MODIFIERS: [&str; 7] = ["public", "protected", "private", "abstract", "static", "final", "default"];

fn parse_param(text: &str) -> Option<CxfParam> {
    let (anns, rest) = leading_annotations(text);
    let rest = rest.trim().trim_start_matches("final ").trim();
    let split = rest.rfind(char::is_whitespace)?;
    let (ty, var) = (rest[..split].trim().to_string(), rest[split..].trim().to_string());
    let named = |kind: Kind, key: &str| annotation(&anns, key).map(|a| CxfParam { kind, name: string_arg(a), ty: ty.clone() });
    named(Kind::Path, "PathParam")
        .or_else(|| named(Kind::Query, "QueryParam"))
        .or_else(|| named(Kind::Header, "HeaderParam"))
        .or_else(|| {
            ["FormParam", "MatrixParam", "CookieParam", "Context", "Suspended", "BeanParam"]
                .iter()
                .find(|k| annotation(&anns, k).is_some())
                .map(|k| CxfParam { kind: Kind::Other((*k).to_string()), name: var.clone(), ty: ty.clone() })
        })
        .or(Some(CxfParam { kind: Kind::Body, name: var, ty }))
}

fn parse_method(chunk: &str, base: &str, class_xml: bool, api: &mut CxfApi) {
    let (anns, rest) = leading_annotations(chunk);
    let Some(http) = ["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD"].iter().find(|v| annotation(&anns, v).is_some()) else { return };
    let Some(open) = rest.find('(') else { return };
    let Some(close) = rest.rfind(')') else { return };
    let head = rest[..open].trim();
    let Some(split) = head.rfind(char::is_whitespace) else { return };
    let name = head[split..].trim().to_string();
    let mut returns = head[..split].trim().to_string();
    while let Some(first) = returns.split_whitespace().next().map(str::to_string) {
        if MODIFIERS.contains(&first.as_str()) {
            returns = returns[first.len()..].trim_start().to_string();
        } else {
            break;
        }
    }
    if !is_identifier(&name) || returns.is_empty() {
        return;
    }
    let params: Vec<CxfParam> = split_top_level(&rest[open + 1..close]).iter().filter_map(|p| parse_param(p)).collect();
    let path = annotation(&anns, "Path").map(string_arg).unwrap_or_default();
    let media = |key: &str| annotation(&anns, key).map(string_arg).unwrap_or_default();
    if class_xml || media("Consumes").contains("xml") || media("Produces").contains("xml") {
        api.xml = true;
    }
    api.endpoints.push(CxfEndpoint { http: (*http).to_string(), path: join_paths(base, &path), name, params, returns });
}

pub fn parse_cxf(source: &str) -> Result<CxfApi, String> {
    let mut api = CxfApi::default();
    let parts = chunks(source);
    let mut base = String::new();
    let mut class_xml = false;
    for chunk in &parts {
        let (anns, rest) = leading_annotations(chunk);
        let words: Vec<&str> = rest.split_whitespace().collect();
        if let Some(i) = words.iter().position(|w| matches!(*w, "interface" | "class")) {
            if api.class_name.is_empty() {
                api.class_name = words.get(i + 1).map(|w| strip_generics(w).to_string()).unwrap_or_default();
                base = annotation(&anns, "Path").map(string_arg).unwrap_or_default();
                class_xml = ["Consumes", "Produces"].iter().any(|k| annotation(&anns, k).is_some_and(|a| string_arg(a).contains("xml")));
                continue;
            }
        }
        if anns.iter().any(|(n, _)| ["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD"].contains(&n.as_str())) {
            parse_method(chunk, &base, class_xml, &mut api);
        }
    }
    if api.endpoints.is_empty() {
        return Err("No JAX-RS methods found: paste an interface or class with @GET / @POST / … methods".into());
    }
    api.xml |= class_xml;
    for e in &api.endpoints {
        for p in &e.params {
            if let Kind::Other(k) = &p.kind {
                let note = format!("`@{k}` on `{}` is not carried over: form, matrix, cookie and context parameters are not supported.", e.name);
                if !api.notes.contains(&note) {
                    api.notes.push(note);
                }
            }
        }
        if e.returns == "Response" || e.returns.ends_with(".Response") {
            let note = format!("`{}` returns a JAX-RS `Response`: the client returns its body as a `String`.", e.name);
            api.notes.push(note);
        }
    }
    Ok(api)
}

/// The Java return type as the generated client states it.
fn java_return(ty: &str) -> String {
    let t = ty.trim();
    match simple_name(t) {
        "Response" => "String".to_string(),
        "Future" | "CompletableFuture" | "CompletionStage" => generic_args(t).into_iter().next().unwrap_or_else(|| "Object".to_string()),
        _ => t.to_string(),
    }
}

// ── Java target ──────────────────────────────────────────────────────────────

fn endpoint_infos(api: &CxfApi) -> Vec<EndpointInfo> {
    let mut seen: Vec<String> = Vec::new();
    api.endpoints
        .iter()
        .map(|e| {
            let mut id = format!("{} {}", e.http, e.path);
            while seen.contains(&id) {
                id.push('\'');
            }
            seen.push(id.clone());
            EndpointInfo {
                id,
                methods: vec![e.http.clone()],
                path: e.path.clone(),
                controller: api.class_name.clone(),
                handler: e.name.clone(),
                return_type: java_return(&e.returns),
                produces: String::new(),
                consumes: String::new(),
                params: e
                    .params
                    .iter()
                    .filter_map(|p| {
                        let binding = match p.kind {
                            Kind::Path => "path",
                            Kind::Query => "query",
                            Kind::Header => "header",
                            Kind::Body => "body",
                            Kind::Other(_) => return None,
                        };
                        Some(EndpointParam { name: p.name.clone(), type_text: p.ty.clone(), binding: binding.to_string(), required: true })
                    })
                    .collect(),
            }
        })
        .collect()
}

fn client_name(o: &CxfOptions, api: &CxfApi) -> String {
    if !o.class_name.trim().is_empty() {
        return o.class_name.trim().to_string();
    }
    let mut base = api.class_name.as_str();
    for suffix in ["Resource", "Rest", "Service", "Api", "Client"] {
        base = base.strip_suffix(suffix).unwrap_or(base);
    }
    let suffix = if matches!(o.target.as_str(), "httpinterface" | "feign" | "retrofit") { "Api" } else { "Client" };
    format!("{}{suffix}", if base.is_empty() { "Remote" } else { base })
}

fn java_output(o: &CxfOptions, api: &CxfApi, xml: bool, ctx: &GenContext, notes: &mut Vec<String>) -> Result<Output, String> {
    let mut derived = ctx.clone();
    derived.endpoints = endpoint_infos(api);
    let options = json!({
        "library": o.target,
        "endpoints": derived.endpoints.iter().map(|e| e.id.clone()).collect::<Vec<_>>(),
        "class_name": client_name(o, api),
        "package": o.package,
        "content": if xml { "xml" } else { "json" },
        "xml_binding": o.xml_binding,
        "auth": o.auth,
        "errors": o.errors,
        "timeout_seconds": o.timeout_seconds,
        "dtos": "reuse",
    });
    notes.push("The payload classes are the ones the CXF client already used (JAXB-annotated): import them in the generated client.".to_string());
    HttpClient.generate(&options, &derived)
}

// ── Rust target ──────────────────────────────────────────────────────────────

/// The Rust spelling of a Java type; custom names are collected in `custom`.
fn rust_type(ty: &str, custom: &mut Vec<String>) -> String {
    let t = ty.trim();
    if let Some(inner) = t.strip_suffix("[]") {
        let inner = rust_type(inner, custom);
        return if inner == "i8" { "Vec<u8>".to_string() } else { format!("Vec<{inner}>") };
    }
    let args = generic_args(t);
    match simple_name(strip_generics(t)) {
        "String" | "BigDecimal" | "BigInteger" | "UUID" | "Date" | "LocalDate" | "LocalDateTime" | "Instant" | "OffsetDateTime" | "ZonedDateTime" | "XMLGregorianCalendar" => "String".into(),
        "long" | "Long" => "i64".into(),
        "int" | "Integer" => "i32".into(),
        "short" | "Short" => "i16".into(),
        "byte" | "Byte" => "i8".into(),
        "double" | "Double" => "f64".into(),
        "float" | "Float" => "f32".into(),
        "boolean" | "Boolean" => "bool".into(),
        "char" | "Character" => "char".into(),
        "List" | "Set" | "Collection" | "ArrayList" | "HashSet" => {
            format!("Vec<{}>", args.first().map(|a| rust_type(a, custom)).unwrap_or_else(|| "String".into()))
        }
        "Map" | "HashMap" => format!(
            "std::collections::HashMap<{}, {}>",
            args.first().map(|a| rust_type(a, custom)).unwrap_or_else(|| "String".into()),
            args.get(1).map(|a| rust_type(a, custom)).unwrap_or_else(|| "String".into())
        ),
        "Response" | "Object" => "String".into(),
        other => {
            if !custom.iter().any(|c| c == other) {
                custom.push(other.to_string());
            }
            other.to_string()
        }
    }
}

fn borrowed(ty: &str) -> String {
    if ty == "String" { "&str".to_string() } else { ty.to_string() }
}

fn rust_output(o: &CxfOptions, api: &CxfApi, xml: bool, notes: &mut Vec<String>) -> Result<Output, String> {
    let mut custom: Vec<String> = Vec::new();
    let mut endpoints = Vec::new();
    for e in &api.endpoints {
        let field = |p: &CxfParam, custom: &mut Vec<String>| json!({ "name": snake(&p.name), "ty": borrowed(&rust_type(&p.ty, custom)) });
        let query: Vec<Value> = e.params.iter().filter(|p| p.kind == Kind::Query).map(|p| field(p, &mut custom)).collect();
        let headers: Vec<Value> = e.params.iter().filter(|p| p.kind == Kind::Header).map(|p| json!({ "name": p.name, "ty": borrowed(&rust_type(&p.ty, &mut custom)) })).collect();
        let body = e.params.iter().find(|p| p.kind == Kind::Body).map(|p| rust_type(&p.ty, &mut custom)).unwrap_or_default();
        let returns = match java_return(&e.returns).as_str() {
            "void" | "Void" => String::new(),
            other => rust_type(other, &mut custom),
        };
        endpoints.push(json!({ "name": snake(&e.name), "method": e.http, "path": e.path, "query": query, "headers": headers, "body": body, "returns": returns }));
    }
    // Path variables keep the names the pattern gives them; only the method's own name is snake-cased above.
    let structs: Vec<Value> = custom.iter().map(|name| json!({ "name": name, "fields": [] })).collect();
    let client = client_name(o, api);
    let options = json!({
        "library": o.target,
        "client_name": client,
        "endpoints": endpoints,
        "structs": structs,
        "auth": o.auth,
        "errors": "thiserror",
        "timeout_seconds": o.timeout_seconds,
        "content": if xml { "xml" } else { "json" },
        "rename_all": "",
    });
    let mut out = RustHttpClient.generate(&options, &GenContext::default())?;
    if let Some(members) = out.members.take() {
        out.files.push(GenFile { root: SourceRoot::Beside, package: String::new(), name: format!("{}.rs", snake(&client)), text: members.text, language: "rust" });
    }
    if !custom.is_empty() {
        notes.push(format!("Fill in the fields of {}: they are empty here, take them from the JAXB classes (an XML element per field, `#[serde(rename = \"…\")]` for the names).", custom.join(", ")));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::sample_context;

    #[test]
    fn the_sample_is_read_into_endpoints_with_joined_paths_and_kinds() {
        let api = parse_cxf(SAMPLE).unwrap();
        assert_eq!(api.class_name, "OrderResource");
        assert!(api.xml);
        let names: Vec<(&str, &str, &str)> = api.endpoints.iter().map(|e| (e.http.as_str(), e.path.as_str(), e.name.as_str())).collect();
        assert_eq!(names, [("GET", "/orders/{id}", "get"), ("GET", "/orders", "list"), ("POST", "/orders", "create"), ("DELETE", "/orders/{id}", "delete")]);
        let get = &api.endpoints[0];
        assert_eq!(get.returns, "Order");
        assert_eq!(get.params[0], CxfParam { kind: Kind::Path, name: "id".into(), ty: "long".into() });
        assert_eq!(get.params[1].kind, Kind::Query);
        let create = &api.endpoints[2];
        assert_eq!(create.params[0].kind, Kind::Body);
        assert_eq!(create.params[1], CxfParam { kind: Kind::Header, name: "X-Tenant".into(), ty: "String".into() });
        assert_eq!(api.endpoints[1].returns, "List<Order>");
    }

    #[test]
    fn braces_in_a_path_and_comments_do_not_confuse_the_reader() {
        let source = "/** doc with { and ; */\n@Path(\"/a\")\npublic class R {\n  // @GET fake\n  @GET @Path(\"/{x:\\\\d+}\")\n  public String one(@PathParam(\"x\") int x) { return \"\"; }\n}";
        let api = parse_cxf(source).unwrap();
        assert_eq!(api.endpoints.len(), 1);
        assert_eq!(api.endpoints[0].path, "/a/{x:\\\\d+}");
        assert_eq!(api.endpoints[0].returns, "String");
        assert!(!api.xml);
    }

    #[test]
    fn unsupported_parameters_and_response_returns_are_noted() {
        let source = "@Path(\"/f\") interface F { @POST Response upload(@FormParam(\"a\") String a); }";
        let api = parse_cxf(source).unwrap();
        assert_eq!(api.endpoints[0].params[0].kind, Kind::Other("FormParam".into()));
        assert!(api.notes.iter().any(|n| n.contains("FormParam")));
        assert!(api.notes.iter().any(|n| n.contains("Response")));
        assert!(parse_cxf("class Nothing {}").is_err());
    }

    #[test]
    fn media_types_are_read_from_literals_and_constants() {
        assert_eq!(string_arg("MediaType.APPLICATION_XML"), "application/xml");
        assert_eq!(string_arg("{\"application/xml\", \"text/xml\"}"), "application/xml");
        assert_eq!(string_arg("value = \"/x\""), "/x");
        assert_eq!(join_paths("/orders/", "/{id}"), "/orders/{id}");
        assert_eq!(join_paths("", ""), "/");
    }

    fn gen(extra: serde_json::Value) -> Output {
        let mut options = CxfRest.defaults(&sample_context());
        if let (Some(base), Some(add)) = (options.as_object_mut(), extra.as_object()) {
            for (k, v) in add {
                base.insert(k.clone(), v.clone());
            }
        }
        CxfRest.generate(&options, &sample_context()).expect("generates")
    }

    #[test]
    fn a_rest_client_keeps_the_xml_content_type_and_the_api() {
        let out = gen(serde_json::json!({}));
        let client = &out.files[0];
        assert_eq!(client.name, "OrderClient.java");
        assert!(client.text.contains("public Order get(long id, String expand)"), "{}", client.text);
        assert!(client.text.contains("public List<Order> list(String status)"), "{}", client.text);
        assert!(client.text.contains(".accept(MediaType.APPLICATION_XML)"), "{}", client.text);
        assert!(client.text.contains(".contentType(MediaType.APPLICATION_XML)"), "{}", client.text);
        assert!(client.text.contains("public void delete(long id)"), "{}", client.text);
        assert!(out.notes.iter().any(|n| n.contains("JAXB")));
    }

    #[test]
    fn the_target_library_is_the_http_client_generators() {
        let out = gen(serde_json::json!({ "target": "okhttp" }));
        assert!(out.files[0].text.contains("new Request.Builder()"), "{}", out.files[0].text);
        let json = gen(serde_json::json!({ "content": "json", "target": "restclient" }));
        assert!(!json.files[0].text.contains("APPLICATION_XML"));
        let feign = gen(serde_json::json!({ "target": "feign" }));
        assert_eq!(feign.files[0].name, "OrderApi.java");
    }

    #[test]
    fn a_rust_target_writes_a_client_file_with_xml_through_quick_xml() {
        let out = gen(serde_json::json!({ "target": "reqwest" }));
        let file = out.files.iter().find(|f| f.name == "order_client.rs").expect("rust file");
        assert_eq!(file.root, SourceRoot::Beside);
        assert!(file.text.contains("pub async fn get(&self, id: impl std::fmt::Display, expand: &str)"), "{}", file.text);
        assert!(file.text.contains("quick_xml::de::from_str"), "{}", file.text);
        assert!(file.text.contains("pub struct Order;"), "{}", file.text);
        assert!(out.notes.iter().any(|n| n.contains("Fill in the fields of Order")), "{:?}", out.notes);
        assert!(out.extras.iter().any(|x| x.text.contains("quick-xml")));
        assert!(out.members.is_none());
    }

    #[test]
    fn java_types_map_to_rust() {
        let mut custom = Vec::new();
        assert_eq!(rust_type("Map<String, List<Item>>", &mut custom), "std::collections::HashMap<String, Vec<Item>>");
        assert_eq!(rust_type("byte[]", &mut custom), "Vec<u8>");
        assert_eq!(rust_type("long", &mut custom), "i64");
        assert_eq!(custom, ["Item"]);
    }

    #[test]
    fn empty_input_is_refused() {
        let ctx = sample_context();
        assert!(CxfRest.generate(&serde_json::json!({ "source": "" }), &ctx).is_err());
    }
}
