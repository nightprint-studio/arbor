//! A client for Spring endpoints: pick controller methods from the app's request mappings and get a
//! typed client for `RestClient`, `WebClient`, `RestTemplate`, `@HttpExchange` interfaces, OpenFeign,
//! `java.net.http.HttpClient`, OkHttp or Retrofit — with the DTOs (reused or regenerated), headers,
//! authentication, error handling and a base-URL property.
//!
//! The generator turns the chosen endpoints into [`Op`]s once; each library module only decides how an
//! [`Op`] is spelled. Everything shared — method names, parameter order, DTO discovery, the config
//! class, the properties snippet — lives here.

mod others;
mod spring;

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::code::{JavaFile, Lines};
use crate::context::{ClassInfo, EndpointInfo, GenContext};
use crate::names::{camel, generic_args, is_identifier, kebab, pascal, payload_type, simple_name, strip_generics};
use crate::output::{parse_options, require_class_name, require_package, GenFile, Output, SourceRoot};
use crate::registry::{Generator, GeneratorMeta, Target};

pub struct HttpClient;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HttpClientOptions {
    /// `restclient` | `webclient` | `resttemplate` | `httpinterface` | `feign` | `jdk` | `okhttp` | `retrofit`
    pub library: String,
    /// Endpoint ids (`GET /api/orders/{id}`) the client gets a method for.
    pub endpoints: Vec<String>,
    /// Empty derives `<Controller>Client` (or `<App>Client` for endpoints of several controllers).
    pub class_name: String,
    pub package: String,
    /// Property holding the server's address.
    pub base_url_property: String,
    pub base_url_default: String,
    /// `none` | `bearer` | `basic` | `api-key`
    pub auth: String,
    pub api_key_header: String,
    /// `reuse` the server's types (shared module) or `generate` copies.
    pub dtos: String,
    /// `exception` (a generated exception carrying status and body) | `default` (the library's own)
    pub errors: String,
    /// Connect/read timeouts in the config class, in seconds; `0` leaves the library's.
    pub timeout_seconds: u32,
    /// `webclient`: return `Mono`/`Flux` (true) or block (false).
    pub reactive: bool,
    /// Generate DTOs as records when the Java level allows.
    pub records: bool,
    /// `json` | `xml` — what the bodies and responses are.
    pub content: String,
    /// `xml`: `jackson` (jackson-dataformat-xml) or `jaxb` (JAXB-annotated classes).
    pub xml_binding: String,
}

impl Default for HttpClientOptions {
    fn default() -> Self {
        HttpClientOptions {
            library: "restclient".into(),
            endpoints: Vec::new(),
            class_name: String::new(),
            package: String::new(),
            base_url_property: String::new(),
            base_url_default: "http://localhost:8080".into(),
            auth: "none".into(),
            api_key_header: "X-API-Key".into(),
            dtos: "reuse".into(),
            errors: "exception".into(),
            timeout_seconds: 10,
            reactive: true,
            records: true,
            content: "json".into(),
            xml_binding: "jackson".into(),
        }
    }
}

impl Generator for HttpClient {
    fn meta(&self) -> GeneratorMeta {
        GeneratorMeta {
            id: "http-client",
            title: "Generate HTTP client from Spring endpoints",
            description: "A typed client for chosen controller endpoints: RestClient, WebClient, RestTemplate, @HttpExchange, OpenFeign, java.net.http, OkHttp or Retrofit.",
            icon: "Network",
            group: "Spring",
            language: "java",
            targets: &[Target::Endpoint, Target::Java, Target::Folder, Target::Project],
            related: &["spring-web", "spring-boot-starter-web", "spring-boot-starter-webflux"],
        }
    }

    fn wanted_types(&self, options: &Value, ctx: &GenContext) -> Vec<String> {
        let Ok(o) = parse_options::<HttpClientOptions>(options) else { return Vec::new() };
        let Ok(ops) = build_ops(&o, ctx) else { return Vec::new() };
        let mut seen = Vec::new();
        for op in &ops {
            for text in op.body.iter().map(|b| b.ty.as_str()).chain(std::iter::once(op.ret.as_str())) {
                collect_types(text, &mut seen);
            }
        }
        seen
    }

    fn defaults(&self, ctx: &GenContext) -> Value {
        let mut o = HttpClientOptions { package: ctx.package_for("client"), ..HttpClientOptions::default() };
        let p = &ctx.project;
        let web = p.version("spring-web");
        let library = if p.has("feign-core") || p.has("spring-cloud-starter-openfeign") {
            "feign"
        } else if p.has("spring-webflux") && !p.has("spring-boot-starter-web") {
            "webclient"
        } else if web.is_some_and(|v| v.at_least(6, 1)) {
            "restclient"
        } else if web.is_some() || p.has("spring-boot-starter-web") {
            "resttemplate"
        } else {
            "jdk"
        }
        ;
        o.library = library.to_string();
        o.records = p.records();
        // The endpoints of the controller at the caret, else of the first controller.
        let controller = ctx.class.as_ref().map(|c| c.name.clone()).filter(|c| ctx.endpoints.iter().any(|e| &e.controller == c)).or_else(|| ctx.endpoints.first().map(|e| e.controller.clone()));
        if let Some(controller) = controller {
            o.endpoints = ctx.endpoints.iter().filter(|e| e.controller == controller).map(|e| e.id.clone()).collect();
        }
        o.base_url_property = default_property(&o, ctx);
        serde_json::to_value(o).unwrap_or(Value::Null)
    }

    fn generate(&self, options: &Value, ctx: &GenContext) -> Result<Output, String> {
        let o: HttpClientOptions = parse_options(options)?;
        require_package(&o.package)?;
        let ops = build_ops(&o, ctx)?;
        let client = client_name(&o, ctx);
        require_class_name(&client, "Class name")?;
        let prefix = if o.base_url_property.trim().is_empty() { default_property(&o, ctx) } else { o.base_url_property.trim().to_string() };
        let prefix = prefix.strip_suffix(".base-url").unwrap_or(&prefix).to_string();
        let plan = Plan::new(&o, ctx, ops, client, prefix);

        let rendered = match o.library.as_str() {
            "restclient" => spring::rest_client(&plan),
            "webclient" => spring::web_client(&plan),
            "resttemplate" => spring::rest_template(&plan),
            "httpinterface" => spring::http_interface(&plan),
            "feign" => others::feign(&plan),
            "jdk" => others::jdk(&plan),
            "okhttp" => others::okhttp(&plan),
            "retrofit" => others::retrofit(&plan),
            other => return Err(format!("Unknown library `{other}`")),
        };

        let mut out = Output::default();
        let mut push = |name: &str, file: JavaFile| out.files.push(GenFile::java(SourceRoot::Main, &file.package.clone(), name, file.render()));
        push(&plan.client, rendered.client);
        if let Some((name, file)) = rendered.config {
            push(&name, file);
        }
        if let Some(exception) = plan.exception_file() {
            push(&plan.exception_name(), exception);
        }
        for (name, file) in plan.dto_files() {
            push(&name, file);
        }
        for note in rendered.notes.into_iter().chain(plan.notes.iter().cloned()) {
            out = out.note(note);
        }
        out = out.extra("application.yml", "yaml", plan.properties(&rendered.properties));
        let deps: Vec<&str> = rendered.dependencies.clone();
        Ok(out.needs(ctx.project.missing(&deps)))
    }
}

/// `shop.client` for an app called `shop`; the controller's name for a client of one controller.
fn default_property(o: &HttpClientOptions, ctx: &GenContext) -> String {
    let controllers: BTreeSet<&str> = o
        .endpoints
        .iter()
        .filter_map(|id| ctx.endpoint(id))
        .map(|e| e.controller.as_str())
        .collect();
    let base = match (controllers.len(), ctx.app_name.is_empty()) {
        (1, _) => kebab(controllers.iter().next().unwrap_or(&"api").trim_end_matches("Controller")),
        (_, false) => kebab(&ctx.app_name),
        _ => "api".to_string(),
    };
    format!("{base}.client")
}

fn client_name(o: &HttpClientOptions, ctx: &GenContext) -> String {
    if !o.class_name.trim().is_empty() {
        return o.class_name.trim().to_string();
    }
    let controllers: BTreeSet<&str> = o.endpoints.iter().filter_map(|id| ctx.endpoint(id)).map(|e| e.controller.as_str()).collect();
    let suffix = if o.library == "httpinterface" || o.library == "feign" || o.library == "retrofit" { "Api" } else { "Client" };
    match controllers.len() {
        1 => format!("{}{suffix}", controllers.iter().next().unwrap_or(&"Api").trim_end_matches("Controller")),
        _ => format!("{}{suffix}", if ctx.app_name.is_empty() { "Api".to_string() } else { pascal(&ctx.app_name) }),
    }
}

/// One parameter of a client method.
#[derive(Debug, Clone)]
pub(crate) struct P {
    /// The Java variable.
    pub var: String,
    /// The name on the wire (query key, header name, path variable).
    pub wire: String,
    pub ty: String,
    pub required: bool,
}

impl P {
    fn primitive(&self) -> bool {
        crate::names::is_primitive(&self.ty)
    }

    /// Must be sent whatever its value: required, or a primitive that cannot be null.
    pub fn always(&self) -> bool {
        self.required || self.primitive()
    }
}

/// One endpoint, ready to be spelled.
#[derive(Debug, Clone)]
pub(crate) struct Op {
    pub id: String,
    pub name: String,
    /// `GET` … `DELETE`.
    pub http: String,
    pub path: String,
    pub path_params: Vec<P>,
    pub query: Vec<P>,
    pub headers: Vec<P>,
    pub body: Option<P>,
    /// `void`, `OrderDto`, `List<OrderDto>`.
    pub ret: String,
    pub produces_list: bool,
}

impl Op {
    pub fn is_void(&self) -> bool {
        self.ret == "void"
    }

    pub fn has_body_verb(&self) -> bool {
        matches!(self.http.as_str(), "POST" | "PUT" | "PATCH")
    }

    /// Parameters in call order: path, query, headers, body.
    pub fn params(&self) -> Vec<&P> {
        self.path_params.iter().chain(&self.query).chain(&self.headers).chain(self.body.iter()).collect()
    }

    pub fn signature(&self) -> String {
        self.params().iter().map(|p| format!("{} {}", p.ty, p.var)).collect::<Vec<_>>().join(", ")
    }

    /// Path variables in the order the path mentions them — the order `build(...)` and `exchange(...)` expand them.
    pub fn path_args(&self) -> Vec<&P> {
        path_variable_names(&self.path)
            .iter()
            .filter_map(|name| self.path_params.iter().find(|p| &p.wire == name))
            .collect()
    }

    /// The element type returned: `OrderDto` for both `OrderDto` and `List<OrderDto>`.
    pub fn element(&self) -> String {
        if self.produces_list { generic_args(&self.ret).into_iter().next().unwrap_or_else(|| self.ret.clone()) } else { self.ret.clone() }
    }
}

fn path_variable_names(path: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = path;
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}') else { break };
        let name = rest[open + 1..open + close].split(':').next().unwrap_or("").trim().to_string();
        out.push(name);
        rest = &rest[open + close + 1..];
    }
    out
}

const RESERVED: [&str; 14] = ["class", "new", "default", "int", "long", "package", "this", "public", "static", "final", "switch", "case", "return", "void"];

fn variable_name(wire: &str) -> String {
    let cleaned: String = wire.chars().map(|c| if c.is_alphanumeric() { c } else { '-' }).collect();
    let mut name = camel(&cleaned);
    if name.is_empty() || !is_identifier(&name) {
        name = "value".to_string();
    }
    if RESERVED.contains(&name.as_str()) {
        name.push_str("Value");
    }
    name
}

/// What the client returns for a handler's return type: wrappers off, `void` kept.
fn client_return(return_type: &str) -> (String, bool) {
    let unwrapped = unwrap_response(return_type.trim());
    if matches!(unwrapped.as_str(), "" | "void" | "Void") {
        return ("void".to_string(), false);
    }
    let list = matches!(simple_name(&unwrapped), "List" | "Set" | "Collection" | "Iterable" | "Page" | "Slice");
    let ty = match simple_name(&unwrapped) {
        "Page" | "Slice" | "Collection" | "Iterable" => {
            let inner = generic_args(&unwrapped).into_iter().next().unwrap_or_else(|| "Object".to_string());
            format!("List<{inner}>")
        }
        _ => unwrapped,
    };
    (ty, list)
}

/// `ResponseEntity<X>` / `Mono<X>` / `Callable<X>` / `Optional<X>` → `X`; `Flux<X>` → `List<X>`.
fn unwrap_response(type_text: &str) -> String {
    let mut current = type_text.to_string();
    for _ in 0..3 {
        match simple_name(&current) {
            "ResponseEntity" | "Mono" | "Callable" | "CompletableFuture" | "DeferredResult" | "Optional" => {
                current = generic_args(&current).into_iter().next().unwrap_or_default();
            }
            "Flux" => {
                let inner = generic_args(&current).into_iter().next().unwrap_or_else(|| "Object".to_string());
                return format!("List<{inner}>");
            }
            _ => break,
        }
    }
    current
}

fn build_ops(o: &HttpClientOptions, ctx: &GenContext) -> Result<Vec<Op>, String> {
    let selected: Vec<&EndpointInfo> = o.endpoints.iter().filter_map(|id| ctx.endpoint(id)).collect();
    if selected.is_empty() {
        return Err("Pick at least one endpoint".into());
    }
    let mut names: Vec<String> = Vec::new();
    let mut ops = Vec::new();
    for e in selected {
        let mut name = if is_identifier(&e.handler) { e.handler.clone() } else { camel(&e.handler) };
        if names.contains(&name) {
            let suffix = crate::names::path_segments(&e.path).last().map(|s| pascal(s.trim_matches(|c| c == '{' || c == '}'))).unwrap_or_default();
            name = format!("{name}{suffix}");
            let mut n = 2;
            while names.contains(&name) {
                name = format!("{}{n}", name.trim_end_matches(|c: char| c.is_ascii_digit()));
                n += 1;
            }
        }
        names.push(name.clone());
        ops.push(op_of(e, name));
    }
    Ok(ops)
}

fn op_of(e: &EndpointInfo, name: String) -> Op {
    let mut used: Vec<String> = Vec::new();
    let mut param = |p: &crate::context::EndpointParam| {
        let mut var = variable_name(&p.name);
        while used.contains(&var) {
            var.push('_');
        }
        used.push(var.clone());
        P { var, wire: p.name.clone(), ty: p.type_text.trim().to_string(), required: p.required }
    };
    let mut path_params = Vec::new();
    let mut query = Vec::new();
    let mut headers = Vec::new();
    let mut body = None;
    for p in &e.params {
        match p.binding.as_str() {
            "path" => path_params.push(param(p)),
            "query" => query.push(param(p)),
            "header" => headers.push(param(p)),
            "body" => body = Some(param(p)),
            _ => {}
        }
    }
    let (ret, produces_list) = client_return(&e.return_type);
    Op { id: e.id.clone(), name, http: e.method().to_string(), path: e.path.clone(), path_params, query, headers, body, ret, produces_list }
}

/// A property the generated configuration reads for authentication.
pub(crate) struct AuthProp {
    pub var: String,
    pub key: String,
}

/// Everything a library module needs, decided once.
pub(crate) struct Plan<'a> {
    pub o: &'a HttpClientOptions,
    pub ctx: &'a GenContext,
    pub ops: Vec<Op>,
    pub client: String,
    pub prefix: String,
    pub notes: Vec<String>,
}

impl<'a> Plan<'a> {
    fn new(o: &'a HttpClientOptions, ctx: &'a GenContext, ops: Vec<Op>, client: String, prefix: String) -> Self {
        let mut notes = Vec::new();
        if o.errors == "exception" && matches!(o.library.as_str(), "retrofit" | "resttemplate") {
            notes.push(match o.library.as_str() {
                "retrofit" => "Retrofit returns a `Call`; check `response.isSuccessful()` where it is executed — the exception class is used by the other libraries.".to_string(),
                _ => "Errors keep the library's own exceptions (`HttpClientErrorException`, …) for this client.".to_string(),
            });
        }
        if ops.iter().any(|op| op.params().iter().any(|p| p.ty.contains("Pageable"))) {
            notes.push("A `Pageable` parameter is passed as it is — replace it with `page`, `size` and `sort` query parameters.".to_string());
        }
        if o.content == "xml" {
            notes.push(match (o.library.as_str(), o.xml_binding.as_str()) {
                ("jdk" | "okhttp" | "retrofit", "jaxb") => "XML goes through Jackson's XmlMapper with `JaxbAnnotationModule` (jackson-dataformat-xml, jackson-module-jaxb-annotations), so JAXB-annotated classes work as they are.".to_string(),
                ("jdk" | "okhttp" | "retrofit", _) => "XML goes through Jackson's `XmlMapper` (jackson-dataformat-xml).".to_string(),
                (_, "jaxb") => "Spring picks its JAXB message converter for `@XmlRootElement` classes: keep jakarta.xml.bind-api and a runtime (glassfish jaxb-runtime) on the classpath, and Jackson XML off it.".to_string(),
                _ => "Spring picks `MappingJackson2XmlHttpMessageConverter` when jackson-dataformat-xml is on the classpath.".to_string(),
            });
        }
        Plan { o, ctx, ops, client, prefix, notes }
    }

    pub fn package(&self) -> &str {
        &self.o.package
    }

    pub fn base_url_key(&self) -> String {
        format!("{}.base-url", self.prefix)
    }

    pub fn value_expr(&self, key: &str) -> String {
        format!("@Value(\"${{{}.{key}}}\")", self.prefix)
    }

    pub fn xml(&self) -> bool {
        self.o.content == "xml"
    }

    /// `application/json` or `application/xml`.
    pub fn media_type(&self) -> &'static str {
        if self.xml() { "application/xml" } else { "application/json" }
    }

    pub fn exceptions(&self) -> bool {
        self.o.errors == "exception"
    }

    pub fn exception_name(&self) -> String {
        format!("{}Exception", self.client.trim_end_matches("Client").trim_end_matches("Api"))
    }

    pub fn timeout(&self) -> Option<u32> {
        (self.o.timeout_seconds > 0).then_some(self.o.timeout_seconds)
    }

    /// The properties the configured authentication reads.
    pub fn auth_props(&self) -> Vec<AuthProp> {
        let p = |var: &str, key: &str| AuthProp { var: var.to_string(), key: key.to_string() };
        match self.o.auth.as_str() {
            "bearer" => vec![p("token", "token")],
            "basic" => vec![p("username", "username"), p("password", "password")],
            "api-key" => vec![p("apiKey", "api-key")],
            _ => Vec::new(),
        }
    }

    /// `@Value` parameters for the config methods: base URL, then the auth properties.
    pub fn config_params(&self) -> String {
        let mut params = vec![format!("{} String baseUrl", self.value_expr("base-url"))];
        for a in self.auth_props() {
            params.push(format!("{} String {}", self.value_expr(&a.key), a.var));
        }
        params.join(", ")
    }

    /// The request header the authentication sets, as `(name, value expression)`.
    pub fn auth_header(&self, imports: &mut crate::code::Imports) -> Option<(String, String)> {
        match self.o.auth.as_str() {
            "bearer" => Some(("Authorization".to_string(), "\"Bearer \" + token".to_string())),
            "basic" => {
                imports.add("java.nio.charset.StandardCharsets");
                imports.add("java.util.Base64");
                Some((
                    "Authorization".to_string(),
                    "\"Basic \" + Base64.getEncoder().encodeToString((username + \":\" + password).getBytes(StandardCharsets.UTF_8))".to_string(),
                ))
            }
            "api-key" => Some((self.o.api_key_header.trim().to_string(), "apiKey".to_string())),
            _ => None,
        }
    }

    fn properties(&self, extra: &[(String, String)]) -> String {
        let mut keys: Vec<(String, String)> = vec![("base-url".to_string(), self.o.base_url_default.clone())];
        for a in self.auth_props() {
            keys.push((a.key, "change-me".to_string()));
        }
        keys.extend(extra.iter().cloned());
        let mut text = String::new();
        let mut depth = 0;
        let mut segments: Vec<&str> = self.prefix.split('.').collect();
        for s in segments.drain(..) {
            text.push_str(&format!("{}{s}:\n", "  ".repeat(depth)));
            depth += 1;
        }
        for (k, v) in keys {
            text.push_str(&format!("{}{k}: {v}\n", "  ".repeat(depth)));
        }
        text
    }

    fn exception_file(&self) -> Option<JavaFile> {
        let uses = matches!(self.o.library.as_str(), "restclient" | "webclient" | "feign" | "jdk" | "okhttp" | "httpinterface");
        if !self.exceptions() || !uses {
            return None;
        }
        let name = self.exception_name();
        let mut f = JavaFile::new(self.package());
        f.body.block(format!("public class {name} extends RuntimeException"), |l| {
            l.line("private final int status;");
            l.line("private final String body;");
            l.blank();
            l.block(format!("public {name}(int status, String body)"), |l| {
                l.line("super(\"HTTP \" + status + (body == null || body.isBlank() ? \"\" : \": \" + body));");
                l.line("this.status = status;");
                l.line("this.body = body;");
            });
            l.blank();
            l.block("public int getStatus()", |l| {
                l.line("return status;");
            });
            l.blank();
            l.block("public String getBody()", |l| {
                l.line("return body;");
            });
        });
        Some(f)
    }

    /// The type names the endpoints mention that are not JDK types, in first-seen order.
    pub fn mentioned_types(&self) -> Vec<String> {
        let mut seen = Vec::new();
        for op in &self.ops {
            for text in op.body.iter().map(|b| b.ty.as_str()).chain(std::iter::once(op.ret.as_str())) {
                collect_types(text, &mut seen);
            }
        }
        seen
    }

    /// Imports for the DTOs a client reuses from the server's module.
    pub fn dto_imports(&self, f: &mut JavaFile) {
        if self.o.dtos == "generate" {
            for name in self.mentioned_types() {
                f.imports.add(format!("{}.dto.{name}", self.package()));
            }
            return;
        }
        for name in self.mentioned_types() {
            if let Some(class) = self.ctx.types.get(&name) {
                if !class.package.is_empty() {
                    f.imports.add(class.fqn());
                }
            }
        }
    }

    /// The DTOs of `dtos = generate`: every mentioned type, and those their fields lead to.
    fn dto_files(&self) -> Vec<(String, JavaFile)> {
        if self.o.dtos != "generate" {
            return Vec::new();
        }
        let package = format!("{}.dto", self.package());
        let mut pending = self.mentioned_types();
        let mut done: Vec<String> = Vec::new();
        let mut files = Vec::new();
        while let Some(name) = pending.pop() {
            if done.contains(&name) {
                continue;
            }
            done.push(name.clone());
            let class = self.ctx.types.get(&name).cloned().unwrap_or_else(|| ClassInfo::new(&name, "", Vec::new()));
            for field in &class.fields {
                collect_types(&field.type_name, &mut pending);
            }
            files.push((name.clone(), dto_file(&package, &class, self.o.records && self.ctx.project.records(), self.ctx.project.lombok())));
        }
        files.sort_by(|a, b| a.0.cmp(&b.0));
        files
    }
}

/// The non-JDK type names a type text mentions, outermost first: `List<OrderDto>` → `["OrderDto"]`.
pub fn type_names(type_text: &str) -> Vec<String> {
    let mut seen = Vec::new();
    collect_types(type_text, &mut seen);
    seen
}

fn is_jdk_type(name: &str) -> bool {
    matches!(
        name,
        "String" | "Long" | "Integer" | "int" | "long" | "boolean" | "Boolean" | "Double" | "double" | "float" | "Float" | "short" | "Short" | "byte" | "Byte" | "char" | "Character"
            | "Object" | "Void" | "void" | "BigDecimal" | "BigInteger" | "UUID" | "LocalDate" | "LocalDateTime" | "LocalTime" | "Instant" | "OffsetDateTime" | "ZonedDateTime" | "Duration"
            | "Date" | "List" | "Set" | "Map" | "Collection" | "Optional" | "Resource" | "MultipartFile" | "Pageable" | "Sort" | "Page" | "Slice" | "ResponseEntity" | "Mono" | "Flux"
    ) || name.is_empty()
        || !name.starts_with(|c: char| c.is_uppercase())
}

fn collect_types(text: &str, into: &mut Vec<String>) {
    let name = simple_name(strip_generics(text)).trim_end_matches("[]").to_string();
    if !is_jdk_type(&name) && !into.contains(&name) {
        into.push(name);
    }
    for arg in generic_args(text) {
        collect_types(&arg, into);
    }
}

fn dto_file(package: &str, class: &ClassInfo, record: bool, lombok: bool) -> JavaFile {
    let mut f = JavaFile::new(package);
    let name = &class.name;
    let java_util = |f: &mut JavaFile, ty: &str| {
        match simple_name(strip_generics(ty)) {
            "List" => f.imports.add("java.util.List"),
            "Set" => f.imports.add("java.util.Set"),
            "Map" => f.imports.add("java.util.Map"),
            "BigDecimal" => f.imports.add("java.math.BigDecimal"),
            "BigInteger" => f.imports.add("java.math.BigInteger"),
            "UUID" => f.imports.add("java.util.UUID"),
            "LocalDate" => f.imports.add("java.time.LocalDate"),
            "LocalDateTime" => f.imports.add("java.time.LocalDateTime"),
            "Instant" => f.imports.add("java.time.Instant"),
            "OffsetDateTime" => f.imports.add("java.time.OffsetDateTime"),
            _ => &mut f.imports,
        };
        for arg in generic_args(ty) {
            match simple_name(strip_generics(&arg)) {
                "List" => f.imports.add("java.util.List"),
                "BigDecimal" => f.imports.add("java.math.BigDecimal"),
                "LocalDate" => f.imports.add("java.time.LocalDate"),
                _ => &mut f.imports,
            };
        }
    };
    for field in &class.fields {
        java_util(&mut f, &field.type_name);
    }
    if class.is_enum() {
        f.body.block(format!("public enum {name}"), |l| {
            l.line(if class.constants.is_empty() { "// TODO: constants".to_string() } else { format!("{};", class.constants.join(", ")) });
        });
        return f;
    }
    if record {
        let components: Vec<String> = class.fields.iter().map(|x| format!("{} {}", x.type_name, x.name)).collect();
        f.body.line(format!("public record {name}({}) {{}}", components.join(", ")));
        return f;
    }
    if lombok {
        f.imports.extend(["lombok.AllArgsConstructor", "lombok.Data", "lombok.NoArgsConstructor"]);
        f.body.line("@Data").line("@NoArgsConstructor").line("@AllArgsConstructor");
        f.body.block(format!("public class {name}"), |l| {
            for field in &class.fields {
                l.line(format!("private {} {};", field.type_name, field.name));
            }
        });
        return f;
    }
    let mut body = Lines::new();
    for field in &class.fields {
        body.line(format!("private {} {};", field.type_name, field.name));
    }
    for field in &class.fields {
        body.blank();
        body.block(format!("public {} get{}()", field.type_name, pascal(&field.name)), |l| {
            l.line(format!("return {};", field.name));
        });
        body.blank();
        body.block(format!("public void set{}({} {})", pascal(&field.name), field.type_name, field.name), |l| {
            l.line(format!("this.{0} = {0};", field.name));
        });
    }
    f.body.block(format!("public class {name}"), |l| {
        l.append(&body);
    });
    f
}

/// What a library module hands back.
pub(crate) struct Rendered {
    pub client: JavaFile,
    pub config: Option<(String, JavaFile)>,
    pub notes: Vec<String>,
    /// `group:artifact` the output needs.
    pub dependencies: Vec<&'static str>,
    /// Extra `application.yml` keys under the client's prefix.
    pub properties: Vec<(String, String)>,
}

impl Rendered {
    pub fn new(client: JavaFile) -> Self {
        Rendered { client, config: None, notes: Vec::new(), dependencies: Vec::new(), properties: Vec::new() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

    fn gen_with(library: &str, extra: serde_json::Value) -> Output {
        let ctx = sample_context();
        let mut options = serde_json::json!({
            "library": library,
            "package": "com.acme.client",
            "endpoints": ["GET /api/orders/{id}", "POST /api/orders", "GET /api/orders", "DELETE /api/orders/{id}"],
        });
        if let (Some(base), Some(add)) = (options.as_object_mut(), extra.as_object()) {
            for (k, v) in add {
                base.insert(k.clone(), v.clone());
            }
        }
        HttpClient.generate(&options, &ctx).expect("generates")
    }

    fn client_text(out: &Output) -> &str {
        &out.files[0].text
    }

    #[test]
    fn the_client_is_named_after_the_controller_and_defaults_pick_its_endpoints() {
        let defaults = HttpClient.defaults(&sample_context());
        assert_eq!(defaults["library"], "restclient");
        assert_eq!(defaults["endpoints"].as_array().unwrap().len(), 4);
        assert_eq!(defaults["base_url_property"], "order.client");
        let out = gen_with("restclient", serde_json::json!({}));
        assert_eq!(out.files[0].name, "OrderClient.java");
    }

    #[test]
    fn rest_client_builds_uris_with_optional_queries_and_typed_bodies() {
        let out = gen_with("restclient", serde_json::json!({}));
        let text = client_text(&out);
        assert!(text.contains("public OrderDto get(Long id, String expand)"), "{text}");
        assert!(text.contains(".queryParamIfPresent(\"expand\", Optional.ofNullable(expand))"), "{text}");
        assert!(text.contains(".build(id))"), "{text}");
        assert!(text.contains("new ParameterizedTypeReference<List<OrderDto>>() {}"), "{text}");
        assert!(text.contains(".body(request)"), "{text}");
        assert!(text.contains("public void delete(Long id)"), "{text}");
        assert!(text.contains("import com.acme.shop.api.OrderDto;"), "{text}");
        assert!(out.files.iter().any(|f| f.name.ends_with("Config.java")), "{:?}", out.files.iter().map(|f| &f.name).collect::<Vec<_>>());
        assert!(out.files.iter().any(|f| f.name == "OrderException.java"));
    }

    #[test]
    fn the_config_reads_the_base_url_and_wires_bearer_auth() {
        let out = gen_with("restclient", serde_json::json!({ "auth": "bearer" }));
        let config = out.files.iter().find(|f| f.name.ends_with("Config.java")).unwrap();
        assert!(config.text.contains("@Value(\"${order.client.base-url}\") String baseUrl"), "{}", config.text);
        assert!(config.text.contains("@Value(\"${order.client.token}\") String token"), "{}", config.text);
        assert!(config.text.contains(".defaultHeader(\"Authorization\", \"Bearer \" + token)"), "{}", config.text);
        assert!(out.extras[0].text.contains("base-url: http://localhost:8080"));
        assert!(out.extras[0].text.contains("token: change-me"));
        assert!(out.extras[0].text.starts_with("order:\n  client:\n"), "{}", out.extras[0].text);
    }

    #[test]
    fn web_client_is_reactive_unless_told_to_block() {
        let reactive = gen_with("webclient", serde_json::json!({}));
        let text = client_text(&reactive);
        assert!(text.contains("public Mono<OrderDto> get(Long id, String expand)"), "{text}");
        assert!(text.contains("public Flux<OrderDto> list(String status, int page)"), "{text}");
        assert!(text.contains(".bodyValue(request)"), "{text}");
        let blocking = gen_with("webclient", serde_json::json!({ "reactive": false }));
        let text = client_text(&blocking);
        assert!(text.contains("public OrderDto get(Long id, String expand)"), "{text}");
        assert!(text.contains(".bodyToMono(OrderDto.class)\n                .block()"), "{text}");
    }

    #[test]
    fn rest_template_exchanges_with_a_parameterized_type_for_lists() {
        let out = gen_with("resttemplate", serde_json::json!({}));
        let text = client_text(&out);
        assert!(text.contains("restTemplate.exchange("), "{text}");
        assert!(text.contains("new ParameterizedTypeReference<List<OrderDto>>() {}"), "{text}");
        assert!(text.contains("UriComponentsBuilder.fromPath(\"/api/orders/{id}\")"), "{text}");
    }

    #[test]
    fn http_interface_declares_exchange_annotations_with_explicit_names() {
        let out = gen_with("httpinterface", serde_json::json!({}));
        let text = client_text(&out);
        assert_eq!(out.files[0].name, "OrderApi.java");
        assert!(text.contains("@GetExchange(\"/api/orders/{id}\")"), "{text}");
        assert!(text.contains("OrderDto get(@PathVariable(\"id\") Long id, @RequestParam(name = \"expand\", required = false) String expand);"), "{text}");
        assert!(text.contains("@PostExchange(\"/api/orders\")"), "{text}");
        assert!(text.contains("@RequestBody CreateOrderRequest request"), "{text}");
        assert!(text.contains("@RequestHeader(\"X-Tenant\") String xTenant"), "{text}");
    }

    #[test]
    fn feign_uses_the_mapping_annotations_and_the_base_url_property() {
        let out = gen_with("feign", serde_json::json!({}));
        let text = client_text(&out);
        assert!(text.contains("@FeignClient(name = \"order\", url = \"${order.client.base-url}\""), "{text}");
        assert!(text.contains("@GetMapping(\"/api/orders/{id}\")"), "{text}");
        assert!(text.contains("@PathVariable(\"id\") Long id"), "{text}");
        assert!(text.contains("@RequestBody CreateOrderRequest request"), "{text}");
    }

    #[test]
    fn jdk_client_sends_and_maps_errors_to_the_generated_exception() {
        let out = gen_with("jdk", serde_json::json!({}));
        let text = client_text(&out);
        assert!(text.contains("HttpRequest.newBuilder("), "{text}");
        assert!(text.contains("throw new OrderException(response.statusCode(), response.body());"), "{text}");
        assert!(text.contains("new TypeReference<List<OrderDto>>() {}"), "{text}");
        assert!(text.contains("\"/api/orders/\" + encode(id)"), "{text}");
    }

    #[test]
    fn okhttp_builds_requests_with_a_json_body() {
        let out = gen_with("okhttp", serde_json::json!({}));
        let text = client_text(&out);
        assert!(text.contains("new Request.Builder()"), "{text}");
        assert!(text.contains("RequestBody.create(mapper.writeValueAsString(request), JSON)") || text.contains("RequestBody.create(json(request), JSON)"), "{text}");
        assert!(text.contains("client.newCall(request).execute()"), "{text}");
    }

    #[test]
    fn retrofit_paths_are_relative_and_calls_are_returned() {
        let out = gen_with("retrofit", serde_json::json!({}));
        let text = client_text(&out);
        assert!(text.contains("@GET(\"api/orders/{id}\")"), "{text}");
        assert!(text.contains("Call<OrderDto> get(@Path(\"id\") Long id, @Query(\"expand\") String expand);"), "{text}");
        assert!(text.contains("Call<List<OrderDto>> list("), "{text}");
        assert!(text.contains("@Body CreateOrderRequest request"), "{text}");
        assert!(text.contains("@Header(\"X-Tenant\") String xTenant"), "{text}");
    }

    #[test]
    fn generated_dtos_cover_the_types_the_endpoints_mention() {
        let out = gen_with("restclient", serde_json::json!({ "dtos": "generate" }));
        let dto = out.files.iter().find(|f| f.name == "OrderDto.java").expect("OrderDto");
        assert_eq!(dto.package, "com.acme.client.dto");
        assert!(dto.text.contains("public record OrderDto(Long id, BigDecimal total) {}"), "{}", dto.text);
        assert!(dto.text.contains("import java.math.BigDecimal;"), "{}", dto.text);
        assert!(out.files.iter().any(|f| f.name == "CreateOrderRequest.java"));
        assert!(client_text(&out).contains("import com.acme.client.dto.OrderDto;"));
    }

    #[test]
    fn pre_java_16_dtos_are_classes_with_accessors() {
        let mut ctx = sample_context();
        ctx.project.java = 11;
        let options = serde_json::json!({ "library": "restclient", "package": "p", "endpoints": ["GET /api/orders/{id}"], "dtos": "generate" });
        let out = HttpClient.generate(&options, &ctx).unwrap();
        let dto = out.files.iter().find(|f| f.name == "OrderDto.java").unwrap();
        assert!(dto.text.contains("public class OrderDto"), "{}", dto.text);
        assert!(dto.text.contains("public Long getId()"), "{}", dto.text);
    }

    #[test]
    fn no_endpoint_is_an_error_and_duplicate_handlers_get_distinct_names() {
        let ctx = sample_context();
        assert!(HttpClient.generate(&serde_json::json!({ "endpoints": [] }), &ctx).is_err());
        let mut ctx = sample_context();
        let mut dup = ctx.endpoints[0].clone();
        dup.id = "GET /api/orders/{id}/lines".into();
        dup.path = "/api/orders/{id}/lines".into();
        ctx.endpoints.push(dup);
        let ops = build_ops(
            &HttpClientOptions { endpoints: vec!["GET /api/orders/{id}".into(), "GET /api/orders/{id}/lines".into()], ..Default::default() },
            &ctx,
        )
        .unwrap();
        assert_ne!(ops[0].name, ops[1].name);
    }

    #[test]
    fn return_types_lose_their_wrappers() {
        assert_eq!(client_return("ResponseEntity<OrderDto>"), ("OrderDto".to_string(), false));
        assert_eq!(client_return("Flux<OrderDto>"), ("List<OrderDto>".to_string(), true));
        assert_eq!(client_return("Page<OrderDto>"), ("List<OrderDto>".to_string(), true));
        assert_eq!(client_return("ResponseEntity<Void>"), ("void".to_string(), false));
        assert_eq!(client_return("void"), ("void".to_string(), false));
    }

    #[test]
    fn path_variables_keep_the_order_the_path_mentions_them() {
        assert_eq!(path_variable_names("/a/{x}/b/{y:\\d+}"), vec!["x", "y"]);
    }

    #[test]
    fn basic_auth_reads_two_properties() {
        let out = gen_with("restclient", serde_json::json!({ "auth": "basic" }));
        let config = out.files.iter().find(|f| f.name.ends_with("Config.java")).unwrap();
        assert!(config.text.contains("String username"), "{}", config.text);
        assert!(config.text.contains("String password"), "{}", config.text);
        assert!(config.text.contains("Base64.getEncoder()"), "{}", config.text);
    }
}

#[cfg(test)]
mod xml_tests;
