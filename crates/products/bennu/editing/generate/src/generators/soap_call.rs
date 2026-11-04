//! A Java class that makes a SOAP call, written from a pasted envelope (and, optionally, its WSDL): the
//! JDK `HttpClient` with the envelope in a text block, Spring-WS `WebServiceTemplate`, `WebClient`,
//! CXF `Dispatch` or a generated CXF port, or Apache HttpClient — with the namespace, `SOAPAction`,
//! an optional WS-Security `UsernameToken` and the response read by XPath or JAXB.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::code::{JavaFile, Lines};
use crate::context::GenContext;
use crate::generators::rust_support::Field;
use crate::names::{camel, java_string, kebab, pascal};
use crate::output::{parse_options, require_class_name, require_package, GenFile, Output, SourceRoot};
use crate::registry::{Generator, GeneratorMeta, Target};
use crate::soap::{parse_soap, parse_wsdl, Soap, Wsdl, SAMPLE_ENVELOPE};

pub struct SoapCall;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SoapOptions {
    /// The request envelope as pasted.
    pub envelope: String,
    /// The WSDL, optional: the address, the port, the SOAPAction and the service names come from it.
    pub wsdl: String,
    /// `jdk` | `webservicetemplate` | `webclient` | `cxf-dispatch` | `cxf-port` | `apache`
    pub variant: String,
    pub class_name: String,
    pub package: String,
    pub endpoint: String,
    pub soap_action: String,
    /// A `UsernameToken` with a plain-text password.
    pub ws_security: bool,
    /// `xpath` | `jaxb` | `raw`
    pub response: String,
    /// `xpath`: one value read per field — the name and the XPath expression (in `ty`).
    pub response_fields: Vec<Field>,
    /// `jaxb`: the class the first element of the response Body is unmarshalled to.
    pub response_type: String,
    pub timeout_seconds: u32,
}

impl Default for SoapOptions {
    fn default() -> Self {
        SoapOptions {
            envelope: SAMPLE_ENVELOPE.to_string(),
            wsdl: String::new(),
            variant: "jdk".into(),
            class_name: String::new(),
            package: String::new(),
            endpoint: String::new(),
            soap_action: String::new(),
            ws_security: false,
            response: "xpath".into(),
            response_fields: Vec::new(),
            response_type: String::new(),
            timeout_seconds: 30,
        }
    }
}

impl Generator for SoapCall {
    fn meta(&self) -> GeneratorMeta {
        GeneratorMeta {
            id: "soap-call",
            title: "Generate SOAP call from an envelope",
            description: "Paste a SOAP envelope (and a WSDL) and get the Java that makes the call: JDK HttpClient, Spring-WS, WebClient, CXF or Apache HttpClient.",
            icon: "FileCode2",
            group: "Web services",
            language: "java",
            targets: &[Target::Java, Target::Folder, Target::Project],
            related: &["spring-ws-core", "cxf-rt-frontend-jaxws", "jaxws-api", "jakarta.xml.ws-api"],
        }
    }

    fn defaults(&self, ctx: &GenContext) -> Value {
        let p = &ctx.project;
        let variant = if p.has("spring-ws-core") {
            "webservicetemplate"
        } else if p.has("cxf-rt-frontend-jaxws") {
            "cxf-dispatch"
        } else if p.has("spring-webflux") {
            "webclient"
        } else if p.has("httpclient5") {
            "apache"
        } else {
            "jdk"
        };
        serde_json::to_value(SoapOptions { variant: variant.into(), package: ctx.package_for("soap"), ..SoapOptions::default() }).unwrap_or(Value::Null)
    }

    fn generate(&self, options: &Value, ctx: &GenContext) -> Result<Output, String> {
        let o: SoapOptions = parse_options(options)?;
        require_package(&o.package)?;
        let soap = parse_soap(&o.envelope, o.ws_security && o.variant != "cxf-port")?;
        let wsdl = if o.wsdl.trim().is_empty() { None } else { Some(parse_wsdl(&o.wsdl)?) };
        if !["jdk", "webservicetemplate", "webclient", "cxf-dispatch", "cxf-port", "apache"].contains(&o.variant.as_str()) {
            return Err(format!("Unknown variant `{}`", o.variant));
        }
        if !["xpath", "jaxb", "raw"].contains(&o.response.as_str()) {
            return Err(format!("Unknown response reading `{}`", o.response));
        }
        if o.response == "jaxb" {
            require_class_name(&o.response_type, "Response type")?;
        }
        let class = if o.class_name.trim().is_empty() { format!("{}Client", pascal(&soap.operation)) } else { o.class_name.trim().to_string() };
        require_class_name(&class, "Class name")?;

        let c = Plan::new(&o, &soap, wsdl.as_ref(), class.clone(), ctx);
        let mut f = JavaFile::new(&o.package);
        write_class(&mut f, &c);
        let mut out = Output::file(GenFile::java(SourceRoot::Main, &o.package, &class, f.render()));
        for n in &c.notes {
            out = out.note(n.clone());
        }
        if c.spring {
            out = out.extra("application.yml", "yaml", c.properties());
        }
        Ok(out.needs(ctx.project.missing(&c.dependencies())))
    }
}

/// Everything the writers share, decided once.
struct Plan<'a> {
    o: &'a SoapOptions,
    soap: &'a Soap,
    class: String,
    endpoint: String,
    action: String,
    method: String,
    ret: String,
    spring: bool,
    ee: &'static str,
    java: u32,
    /// The template is the Body's first element rather than the whole envelope.
    payload_only: bool,
    namespace: String,
    service: String,
    port: String,
    port_type: String,
    notes: Vec<String>,
}

impl<'a> Plan<'a> {
    fn new(o: &'a SoapOptions, soap: &'a Soap, wsdl: Option<&Wsdl>, class: String, ctx: &GenContext) -> Self {
        let mut notes = Vec::new();
        let action = if !o.soap_action.trim().is_empty() {
            o.soap_action.trim().to_string()
        } else {
            wsdl.and_then(|w| w.action_of(&soap.operation)).unwrap_or_default().to_string()
        };
        let endpoint = [o.endpoint.trim(), wsdl.and_then(|w| w.address.as_deref()).unwrap_or_default()]
            .into_iter()
            .find(|e| !e.is_empty())
            .unwrap_or("http://localhost:8080/ws")
            .to_string();
        if o.endpoint.trim().is_empty() && wsdl.and_then(|w| w.address.as_ref()).is_none() {
            notes.push("No endpoint address was given or found in a WSDL: the placeholder `http://localhost:8080/ws` is the default.".to_string());
        }
        if soap.version == "1.1" && action.is_empty() {
            notes.push("SOAP 1.1 services usually need a `SOAPAction` header: paste the WSDL or fill it in.".to_string());
        }
        let ret = match o.response.as_str() {
            "jaxb" => o.response_type.clone(),
            "raw" => "String".to_string(),
            _ if o.response_fields.len() > 1 => "Map<String, String>".to_string(),
            _ => "String".to_string(),
        };
        let ret = if o.variant == "cxf-port" { "Object".to_string() } else { ret };
        let namespace = wsdl.map(|w| w.target_ns.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| soap.operation_ns.clone());
        let named = |value: Option<&String>, fallback: &str, what: &str, notes: &mut Vec<String>| match value {
            Some(v) => v.clone(),
            None => {
                if matches!(o.variant.as_str(), "cxf-dispatch" | "cxf-port") {
                    notes.push(format!("{what} name `{fallback}` is a guess — paste the WSDL to use the real one."));
                }
                fallback.to_string()
            }
        };
        let service = named(wsdl.and_then(|w| w.service.as_ref()), "ServiceName", "Service", &mut notes);
        let port = named(wsdl.and_then(|w| w.port.as_ref()), "PortName", "Port", &mut notes);
        let port_type = wsdl.and_then(|w| w.port_type.clone()).unwrap_or_else(|| format!("{}PortType", pascal(&soap.operation)));
        match o.variant.as_str() {
            "cxf-port" => notes.push(format!(
                "Generate the stub with `wsimport` or cxf-codegen from the WSDL and import `{port_type}`; the method and its types follow the generated signature."
            )),
            "webservicetemplate" if o.timeout_seconds > 0 => notes.push("Spring-WS timeouts are set on the message sender (`HttpComponentsMessageSender`), not here.".to_string()),
            "cxf-dispatch" => notes.push("Timeouts for a CXF Dispatch are set through `HTTPConduit` or the request context.".to_string()),
            _ => {}
        }
        if o.ws_security && o.variant == "cxf-port" {
            notes.push("WS-Security uses CXF's WSS4J interceptor (`cxf-rt-ws-security`).".to_string());
        }
        Plan {
            o,
            soap,
            class,
            endpoint,
            action,
            method: camel(&soap.operation),
            ret,
            spring: matches!(o.variant.as_str(), "webservicetemplate" | "webclient"),
            ee: if ctx.project.jakarta() { "jakarta" } else { "javax" },
            java: ctx.project.java_level(),
            payload_only: o.variant == "webservicetemplate",
            namespace,
            service,
            port,
            port_type,
            notes,
        }
    }

    fn prefix(&self) -> String {
        format!("soap.{}", kebab(&self.class.trim_end_matches("Client")))
    }

    fn properties(&self) -> String {
        let mut text = format!("soap:\n  {}:\n    endpoint: {}\n", kebab(&self.class.trim_end_matches("Client")), self.endpoint);
        if self.o.ws_security {
            text.push_str("    username: change-me\n    password: change-me\n");
        }
        text
    }

    fn dependencies(&self) -> Vec<&'static str> {
        let mut out = match self.o.variant.as_str() {
            "webservicetemplate" => vec!["org.springframework.ws:spring-ws-core"],
            "webclient" => vec!["org.springframework.boot:spring-boot-starter-webflux"],
            "cxf-dispatch" => vec!["org.apache.cxf:cxf-rt-frontend-jaxws", "org.apache.cxf:cxf-rt-transports-http"],
            "cxf-port" => vec!["org.apache.cxf:cxf-rt-frontend-jaxws", "org.apache.cxf:cxf-rt-transports-http"],
            "apache" => vec!["org.apache.httpcomponents.client5:httpclient5"],
            _ => Vec::new(),
        };
        if self.o.ws_security && self.o.variant == "cxf-port" {
            out.push("org.apache.cxf:cxf-rt-ws-security");
        }
        out
    }

    /// `text/xml` for 1.1, `application/soap+xml` with the action for 1.2.
    fn content_type(&self) -> String {
        if self.soap.version == "1.2" {
            let action = if self.action.is_empty() { String::new() } else { format!("; action=\\\"{}\\\"", self.action) };
            format!("application/soap+xml; charset=utf-8{action}")
        } else {
            "text/xml; charset=utf-8".to_string()
        }
    }

    fn action_header(&self) -> Option<String> {
        (self.soap.version == "1.1").then(|| format!("\\\"{}\\\"", self.action))
    }

    fn signature_params(&self) -> String {
        self.soap.params.iter().map(|p| format!("String {}", p.name)).collect::<Vec<_>>().join(", ")
    }

    /// The arguments of `String.format(TEMPLATE, …)`.
    fn format_args(&self) -> Vec<String> {
        let mut args = Vec::new();
        if self.o.ws_security && !self.payload_only && self.o.variant != "cxf-port" {
            args.push("securityHeader()".to_string());
        }
        args.extend(self.soap.params.iter().map(|p| format!("xml({})", p.name)));
        args
    }

    fn format_call(&self, constant: &str) -> String {
        let mut parts = vec![constant.to_string()];
        parts.extend(self.format_args());
        format!("String.format({})", parts.join(", "))
    }

    fn credentials(&self) -> bool {
        self.o.ws_security
    }
}

fn write_class(f: &mut JavaFile, c: &Plan) {
    let mut body = Lines::new();
    write_constants(&mut body, c);
    write_state(&mut body, &mut f.imports, c);
    body.blank();
    write_constructor(&mut body, &mut f.imports, c);
    body.blank();
    write_call(&mut body, &mut f.imports, c);
    if c.o.variant != "cxf-port" {
        if c.o.ws_security && !c.payload_only {
            body.blank();
            write_security_header(&mut body, c);
        }
        body.blank();
        write_helpers(&mut body, &mut f.imports, c);
    }
    if c.spring {
        f.imports.add("org.springframework.stereotype.Component");
        f.body.line("@Component");
    }
    f.body.block(format!("public class {}", c.class), |l| {
        l.append(&body);
    });
}

/// A `String` constant holding `text`: a text block from Java 15, concatenated literals before.
fn write_template(l: &mut Lines, name: &str, text: &str, java: u32) {
    if java >= 15 {
        l.line(format!("private static final String {name} = \"\"\""));
        l.indent().indent();
        for line in text.lines() {
            l.line(line.replace('\\', "\\\\").replace("\"\"\"", "\\\"\\\"\\\"").trim_end());
        }
        l.line("\"\"\";");
        l.dedent().dedent();
        return;
    }
    l.line(format!("private static final String {name} ="));
    l.indent().indent();
    let lines: Vec<&str> = text.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let literal = format!("{}\\n", line.replace('\\', "\\\\").replace('"', "\\\""));
        l.line(format!("{}\"{literal}\"{}", if i == 0 { "" } else { "+ " }, if i + 1 == lines.len() { ";" } else { "" }));
    }
    l.dedent().dedent();
}

fn write_constants(l: &mut Lines, c: &Plan) {
    if !c.payload_only && c.o.variant != "cxf-port" {
        l.line(format!("private static final String ENV_NS = {};", java_string(&c.soap.env_ns)));
    }
    if !matches!(c.o.variant.as_str(), "cxf-port") {
        let (name, text) = if c.payload_only { ("PAYLOAD", &c.soap.payload_template) } else { ("ENVELOPE", &c.soap.envelope_template) };
        write_template(l, name, text, c.java);
    }
    if c.o.variant == "cxf-dispatch" {
        l.line(format!("private static final String NAMESPACE = {};", java_string(&c.namespace)));
    }
    l.blank();
}

fn write_state(l: &mut Lines, im: &mut crate::code::Imports, c: &Plan) {
    match c.o.variant.as_str() {
        "jdk" => {
            im.add("java.net.http.HttpClient");
            l.line("private final HttpClient http;");
        }
        "webservicetemplate" => {
            im.add("org.springframework.ws.client.core.WebServiceTemplate");
            l.line("private final WebServiceTemplate template;");
        }
        "webclient" => {
            im.add("org.springframework.web.reactive.function.client.WebClient");
            l.line("private final WebClient webClient;");
        }
        "apache" => {
            im.add("org.apache.hc.client5.http.impl.classic.CloseableHttpClient");
            l.line("private final CloseableHttpClient client;");
        }
        "cxf-port" => {
            l.line(format!("private final {} port;", c.port_type));
        }
        _ => {}
    }
    if c.o.variant != "cxf-port" {
        l.line("private final String endpoint;");
    }
    if c.credentials() && c.o.variant != "cxf-port" {
        l.line("private final String username;");
        l.line("private final String password;");
    }
}

fn constructor_params(c: &Plan) -> Vec<String> {
    let value = |key: &str| if c.spring { format!("@Value(\"${{{}.{key}}}\") ", c.prefix()) } else { String::new() };
    let mut params = Vec::new();
    if c.o.variant == "webclient" {
        params.push("WebClient.Builder builder".to_string());
    }
    params.push(format!("{}String endpoint", value("endpoint")));
    if c.credentials() {
        params.push(format!("{}String username", value("username")));
        params.push(format!("{}String password", value("password")));
    }
    params
}

fn write_constructor(l: &mut Lines, im: &mut crate::code::Imports, c: &Plan) {
    if c.spring {
        im.add("org.springframework.beans.factory.annotation.Value");
    }
    let params = constructor_params(c).join(", ");
    let throws = if c.o.variant == "webservicetemplate" { format!(" throws {}.xml.soap.SOAPException", c.ee) } else { String::new() };
    l.block(format!("public {}({params}){throws}", c.class), |l| {
        match c.o.variant.as_str() {
            "jdk" => {
                im.add("java.time.Duration");
                l.line(format!("this.http = HttpClient.newBuilder().connectTimeout(Duration.ofSeconds({})).build();", c.o.timeout_seconds));
            }
            "webclient" => {
                l.line("this.webClient = builder.build();");
            }
            "apache" => {
                im.add("org.apache.hc.client5.http.config.RequestConfig");
                im.add("org.apache.hc.client5.http.impl.classic.HttpClients");
                im.add("org.apache.hc.core5.util.Timeout");
                l.line("this.client = HttpClients.custom()");
                l.line(format!("        .setDefaultRequestConfig(RequestConfig.custom().setResponseTimeout(Timeout.ofSeconds({})).build())", c.o.timeout_seconds));
                l.line("        .build();");
            }
            "webservicetemplate" => {
                im.add("org.springframework.ws.soap.saaj.SaajSoapMessageFactory");
                im.add(format!("{}.xml.soap.MessageFactory", c.ee));
                im.add(format!("{}.xml.soap.SOAPConstants", c.ee));
                let protocol = if c.soap.version == "1.2" { "SOAP_1_2_PROTOCOL" } else { "SOAP_1_1_PROTOCOL" };
                l.line(format!("SaajSoapMessageFactory messageFactory = new SaajSoapMessageFactory(MessageFactory.newInstance(SOAPConstants.{protocol}));"));
                l.line("messageFactory.afterPropertiesSet();");
                l.line("this.template = new WebServiceTemplate(messageFactory);");
                l.line("this.template.setDefaultUri(endpoint);");
            }
            "cxf-port" => write_port_factory(l, im, c),
            _ => {}
        }
        if c.o.variant != "cxf-port" {
            l.line("this.endpoint = endpoint;");
        }
        if c.credentials() && c.o.variant != "cxf-port" {
            l.line("this.username = username;");
            l.line("this.password = password;");
        }
    });
}

fn write_port_factory(l: &mut Lines, im: &mut crate::code::Imports, c: &Plan) {
    im.add("org.apache.cxf.jaxws.JaxWsProxyFactoryBean");
    l.line("JaxWsProxyFactoryBean factory = new JaxWsProxyFactoryBean();");
    l.line(format!("factory.setServiceClass({}.class);", c.port_type));
    l.line("factory.setAddress(endpoint);");
    if c.credentials() {
        im.extend([
            "java.util.HashMap",
            "java.util.Map",
            "javax.security.auth.callback.Callback",
            "javax.security.auth.callback.CallbackHandler",
            "org.apache.cxf.ws.security.wss4j.WSS4JOutInterceptor",
            "org.apache.wss4j.common.ConfigurationConstants",
            "org.apache.wss4j.common.ext.WSPasswordCallback",
            "org.apache.wss4j.dom.WSConstants",
        ]);
        l.line("Map<String, Object> security = new HashMap<>();");
        l.line("security.put(ConfigurationConstants.ACTION, ConfigurationConstants.USERNAME_TOKEN);");
        l.line("security.put(ConfigurationConstants.USER, username);");
        l.line("security.put(ConfigurationConstants.PASSWORD_TYPE, WSConstants.PW_TEXT);");
        l.block_closing("security.put(ConfigurationConstants.PW_CALLBACK_REF, (CallbackHandler) callbacks ->", "});", |l| {
            l.block("for (Callback callback : callbacks)", |l| {
                l.block("if (callback instanceof WSPasswordCallback)", |l| {
                    l.line("((WSPasswordCallback) callback).setPassword(password);");
                });
            });
        });
        l.line("factory.getOutInterceptors().add(new WSS4JOutInterceptor(security));");
    }
    l.line(format!("this.port = ({}) factory.create();", c.port_type));
}

fn write_call(l: &mut Lines, im: &mut crate::code::Imports, c: &Plan) {
    let ret = if c.o.variant == "webclient" {
        im.add("reactor.core.publisher.Mono");
        format!("Mono<{}>", c.ret)
    } else {
        c.ret.clone()
    };
    let throws = match c.o.variant.as_str() {
        "jdk" => " throws IOException, InterruptedException",
        "apache" => " throws IOException",
        _ => "",
    };
    l.block(format!("public {ret} {}({}){throws}", c.method, c.signature_params()), |l| match c.o.variant.as_str() {
        "jdk" => call_jdk(l, im, c),
        "apache" => call_apache(l, im, c),
        "webclient" => call_webclient(l, im, c),
        "webservicetemplate" => call_template(l, im, c),
        "cxf-dispatch" => call_dispatch(l, im, c),
        _ => call_port(l, c),
    });
}

fn call_jdk(l: &mut Lines, im: &mut crate::code::Imports, c: &Plan) {
    im.extend(["java.io.IOException", "java.net.URI", "java.net.http.HttpRequest", "java.net.http.HttpResponse", "java.time.Duration"]);
    l.line(format!("String body = {};", c.format_call("ENVELOPE")));
    l.line("HttpRequest request = HttpRequest.newBuilder(URI.create(endpoint))");
    l.indent().indent();
    l.line(format!(".timeout(Duration.ofSeconds({}))", c.o.timeout_seconds));
    l.line(format!(".header(\"Content-Type\", \"{}\")", c.content_type()));
    if let Some(a) = c.action_header() {
        l.line(format!(".header(\"SOAPAction\", \"{a}\")"));
    }
    l.line(".POST(HttpRequest.BodyPublishers.ofString(body))");
    l.line(".build();");
    l.dedent().dedent();
    l.line("HttpResponse<String> response = http.send(request, HttpResponse.BodyHandlers.ofString());");
    l.block("if (response.statusCode() >= 400 && !response.body().contains(\"Fault\"))", |l| {
        l.line("throw new IllegalStateException(\"HTTP \" + response.statusCode());");
    });
    l.line("return parseResponse(response.body());");
}

fn call_apache(l: &mut Lines, im: &mut crate::code::Imports, c: &Plan) {
    im.extend([
        "java.io.IOException",
        "java.nio.charset.StandardCharsets",
        "org.apache.hc.client5.http.classic.methods.HttpPost",
        "org.apache.hc.core5.http.io.entity.EntityUtils",
        "org.apache.hc.core5.http.io.entity.StringEntity",
    ]);
    l.line(format!("String body = {};", c.format_call("ENVELOPE")));
    l.line("HttpPost post = new HttpPost(endpoint);");
    l.line(format!("post.setHeader(\"Content-Type\", \"{}\");", c.content_type()));
    if let Some(a) = c.action_header() {
        l.line(format!("post.setHeader(\"SOAPAction\", \"{a}\");"));
    }
    l.line("post.setEntity(new StringEntity(body, StandardCharsets.UTF_8));");
    l.line("return client.execute(post, response -> parseResponse(EntityUtils.toString(response.getEntity(), StandardCharsets.UTF_8)));");
}

fn call_webclient(l: &mut Lines, im: &mut crate::code::Imports, c: &Plan) {
    im.extend(["java.time.Duration", "org.springframework.http.MediaType"]);
    l.line(format!("String body = {};", c.format_call("ENVELOPE")));
    l.line("return webClient.post()");
    l.indent().indent();
    l.line(".uri(endpoint)");
    l.line(format!(".contentType(MediaType.parseMediaType(\"{}\"))", c.content_type()));
    if let Some(a) = c.action_header() {
        l.line(format!(".header(\"SOAPAction\", \"{a}\")"));
    }
    l.line(".bodyValue(body)");
    l.line(".exchangeToMono(response -> response.bodyToMono(String.class))");
    l.line(format!(".timeout(Duration.ofSeconds({}))", c.o.timeout_seconds));
    l.line(format!(".map({}::parseResponse);", c.class));
    l.dedent().dedent();
}

fn call_template(l: &mut Lines, im: &mut crate::code::Imports, c: &Plan) {
    im.extend(["java.io.StringReader", "java.io.StringWriter", "javax.xml.transform.stream.StreamResult", "javax.xml.transform.stream.StreamSource"]);
    l.line(format!("StreamSource request = new StreamSource(new StringReader({}));", c.format_call("PAYLOAD")));
    l.line("StringWriter response = new StringWriter();");
    l.line("template.sendSourceAndReceiveToResult(endpoint, request, callback(), new StreamResult(response));");
    l.line("return parseResponse(response.toString());");
}

fn call_dispatch(l: &mut Lines, im: &mut crate::code::Imports, c: &Plan) {
    let ee = c.ee;
    im.extend([
        "java.io.StringReader".to_string(),
        "javax.xml.namespace.QName".to_string(),
        "javax.xml.transform.Source".to_string(),
        "javax.xml.transform.stream.StreamSource".to_string(),
        format!("{ee}.xml.ws.BindingProvider"),
        format!("{ee}.xml.ws.Dispatch"),
        format!("{ee}.xml.ws.Service"),
        format!("{ee}.xml.ws.soap.SOAPBinding"),
    ]);
    let binding = if c.soap.version == "1.2" { "SOAP12HTTP_BINDING" } else { "SOAP11HTTP_BINDING" };
    l.line(format!("String body = {};", c.format_call("ENVELOPE")));
    l.line(format!("QName service = new QName(NAMESPACE, {});", java_string(&c.service)));
    l.line(format!("QName port = new QName(NAMESPACE, {});", java_string(&c.port)));
    l.line("Service jaxws = Service.create(service);");
    l.line(format!("jaxws.addPort(port, SOAPBinding.{binding}, endpoint);"));
    l.line("Dispatch<Source> dispatch = jaxws.createDispatch(port, Source.class, Service.Mode.MESSAGE);");
    if !c.action.is_empty() {
        l.line("dispatch.getRequestContext().put(BindingProvider.SOAPACTION_USE_PROPERTY, true);");
        l.line(format!("dispatch.getRequestContext().put(BindingProvider.SOAPACTION_URI_PROPERTY, {});", java_string(&c.action)));
    }
    l.line("Source response = dispatch.invoke(new StreamSource(new StringReader(body)));");
    l.line("return parseResponse(serialize(response));");
}

fn call_port(l: &mut Lines, c: &Plan) {
    let args = c.soap.params.iter().map(|p| p.name.clone()).collect::<Vec<_>>().join(", ");
    l.line(format!("return port.{}({args});", c.method));
}

fn write_security_header(l: &mut Lines, c: &Plan) {
    let text = format!(
        "<wsse:Security xmlns:wsse=\"http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-secext-1.0.xsd\" xmlns:env=\"{}\" env:mustUnderstand=\"1\">\n    <wsse:UsernameToken>\n        <wsse:Username>%s</wsse:Username>\n        <wsse:Password Type=\"http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-username-token-profile-1.0#PasswordText\">%s</wsse:Password>\n    </wsse:UsernameToken>\n</wsse:Security>",
        c.soap.env_ns
    );
    l.block("private String securityHeader()", |l| {
        l.line("return String.format(SECURITY, xml(username), xml(password));");
    });
    l.blank();
    write_template(l, "SECURITY", &text, c.java);
}

fn write_helpers(l: &mut Lines, im: &mut crate::code::Imports, c: &Plan) {
    im.extend([
        "java.io.IOException".to_string(),
        "java.io.StringReader".to_string(),
        "javax.xml.parsers.DocumentBuilderFactory".to_string(),
        "javax.xml.parsers.ParserConfigurationException".to_string(),
        "org.w3c.dom.Document".to_string(),
        "org.w3c.dom.Node".to_string(),
        "org.w3c.dom.NodeList".to_string(),
        "org.xml.sax.InputSource".to_string(),
        "org.xml.sax.SAXException".to_string(),
    ]);
    if c.o.variant == "webservicetemplate" {
        l.block("private WebServiceMessageCallback callback()", |l| {
            im.extend(["org.springframework.ws.client.core.WebServiceMessageCallback", "org.springframework.ws.soap.SoapMessage"]);
            l.block_closing("return message ->", "};", |l| {
                l.line("SoapMessage soap = (SoapMessage) message;");
                l.line(format!("soap.setSoapAction({});", java_string(&c.action)));
                if c.credentials() {
                    im.extend(["java.io.StringReader", "javax.xml.transform.TransformerFactory", "javax.xml.transform.stream.StreamSource"]);
                    l.line("TransformerFactory.newInstance().newTransformer()");
                    l.line("        .transform(new StreamSource(new StringReader(securityHeader())), soap.getSoapHeader().getResult());");
                }
            });
        });
        l.blank();
        if c.credentials() {
            write_security_header(l, c);
            l.blank();
        }
    }
    write_parse(l, im, c);
    l.blank();
    l.block("private static Document document(String xml)", |l| {
        l.block("try", |l| {
            l.line("DocumentBuilderFactory factory = DocumentBuilderFactory.newInstance();");
            l.line("factory.setNamespaceAware(true);");
            l.line("factory.setFeature(\"http://apache.org/xml/features/disallow-doctype-decl\", true);");
            l.line("return factory.newDocumentBuilder().parse(new InputSource(new StringReader(xml)));");
        });
        l.block_after("catch (ParserConfigurationException | SAXException | IOException e)", |l| {
            l.line("throw new IllegalStateException(\"Unreadable SOAP response\", e);");
        });
    });
    if !c.payload_only {
        l.blank();
        l.block("private static void failOnFault(Document document)", |l| {
            l.line("NodeList faults = document.getElementsByTagNameNS(ENV_NS, \"Fault\");");
            l.block("if (faults.getLength() > 0)", |l| {
                l.line("throw new SoapFaultException(faults.item(0).getTextContent().trim());");
            });
        });
    }
    l.blank();
    l.block("private static String xml(String value)", |l| {
        l.line("return value == null ? \"\" : value.replace(\"&\", \"&amp;\").replace(\"<\", \"&lt;\").replace(\">\", \"&gt;\");");
    });
    if c.o.variant == "cxf-dispatch" {
        l.blank();
        l.block("private static String serialize(Source source)", |l| {
            im.extend(["java.io.StringWriter", "javax.xml.transform.TransformerException", "javax.xml.transform.TransformerFactory", "javax.xml.transform.stream.StreamResult"]);
            l.block("try", |l| {
                l.line("StringWriter out = new StringWriter();");
                l.line("TransformerFactory.newInstance().newTransformer().transform(source, new StreamResult(out));");
                l.line("return out.toString();");
            });
            l.block_after("catch (TransformerException e)", |l| {
                l.line("throw new IllegalStateException(\"Cannot read the SOAP response\", e);");
            });
        });
    }
    if !c.payload_only {
        l.blank();
        l.line("/** A SOAP fault the service answered with. */");
        l.block("public static class SoapFaultException extends RuntimeException", |l| {
            l.block("public SoapFaultException(String message)", |l| {
                l.line("super(message);");
            });
        });
    }
}

fn write_parse(l: &mut Lines, im: &mut crate::code::Imports, c: &Plan) {
    let ee = c.ee;
    l.block(format!("private static {} parseResponse(String xml)", c.ret), |l| {
        if c.o.response == "raw" {
            l.line("return xml;");
            return;
        }
        l.line("Document document = document(xml);");
        if !c.payload_only {
            l.line("failOnFault(document);");
        }
        if c.o.response == "jaxb" {
            im.extend([format!("{ee}.xml.bind.JAXBContext"), format!("{ee}.xml.bind.JAXBException")]);
            if c.payload_only {
                l.line("Node payload = document.getDocumentElement();");
            } else {
                l.line("Node body = document.getElementsByTagNameNS(ENV_NS, \"Body\").item(0);");
                l.line("Node payload = null;");
                l.block("for (Node node = body.getFirstChild(); node != null && payload == null; node = node.getNextSibling())", |l| {
                    l.block("if (node.getNodeType() == Node.ELEMENT_NODE)", |l| {
                        l.line("payload = node;");
                    });
                });
                l.block("if (payload == null)", |l| {
                    l.line("throw new IllegalStateException(\"Empty SOAP body\");");
                });
            }
            l.block("try", |l| {
                l.line(format!("return JAXBContext.newInstance({0}.class).createUnmarshaller().unmarshal(payload, {0}.class).getValue();", c.o.response_type));
            });
            l.block_after("catch (JAXBException e)", |l| {
                l.line("throw new IllegalStateException(\"Cannot read the response\", e);");
            });
            return;
        }
        let fields = response_fields(c);
        if fields.len() == 1 {
            l.line(format!("return xpath(document, {});", java_string(&fields[0].1)));
        } else {
            im.add("java.util.LinkedHashMap");
            l.line("Map<String, String> result = new LinkedHashMap<>();");
            for (name, expr) in &fields {
                l.line(format!("result.put({}, xpath(document, {}));", java_string(name), java_string(expr)));
            }
            l.line("return result;");
        }
    });
    if c.o.response == "xpath" {
        im.extend(["javax.xml.xpath.XPathConstants", "javax.xml.xpath.XPathExpressionException", "javax.xml.xpath.XPathFactory"]);
        l.blank();
        l.block("private static String xpath(Document document, String expression)", |l| {
            l.block("try", |l| {
                l.line("return (String) XPathFactory.newInstance().newXPath().evaluate(expression, document, XPathConstants.STRING);");
            });
            l.block_after("catch (XPathExpressionException e)", |l| {
                l.line("throw new IllegalStateException(\"Bad XPath: \" + expression, e);");
            });
        });
    }
}

/// `(name, xpath)` per value read; one `string(...)` of the response's first child when none were asked for.
fn response_fields(c: &Plan) -> Vec<(String, String)> {
    let fields: Vec<(String, String)> = c.o.response_fields.iter().filter(|f| !f.name.trim().is_empty() && !f.ty.trim().is_empty()).map(|f| (f.name.clone(), f.ty.clone())).collect();
    if fields.is_empty() {
        vec![("result".to_string(), format!("string(//*[local-name()='{}Response']/*[1])", c.soap.operation))]
    } else {
        fields
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::sample_context;

    fn gen(extra: serde_json::Value) -> Output {
        let mut options = SoapCall.defaults(&sample_context());
        if let (Some(base), Some(add)) = (options.as_object_mut(), extra.as_object()) {
            for (k, v) in add {
                base.insert(k.clone(), v.clone());
            }
        }
        SoapCall.generate(&options, &sample_context()).expect("generates")
    }

    fn text(out: &Output) -> String {
        out.files[0].text.clone()
    }

    #[test]
    fn the_jdk_variant_formats_the_envelope_and_posts_it() {
        let out = gen(serde_json::json!({ "soap_action": "http://example.com/GetWeather" }));
        let t = text(&out);
        assert_eq!(out.files[0].name, "GetWeatherClient.java");
        assert!(t.contains("private static final String ENVELOPE = \"\"\""), "{t}");
        assert!(t.contains("<wea:City>%s</wea:City>"), "{t}");
        assert!(t.contains("public String getWeather(String city, String country) throws IOException, InterruptedException {"), "{t}");
        assert!(t.contains("String body = String.format(ENVELOPE, xml(city), xml(country));"), "{t}");
        assert!(t.contains(".header(\"Content-Type\", \"text/xml; charset=utf-8\")"), "{t}");
        assert!(t.contains(".header(\"SOAPAction\", \"\\\"http://example.com/GetWeather\\\"\")"), "{t}");
        assert!(t.contains("string(//*[local-name()='GetWeatherResponse']/*[1])"), "{t}");
        assert!(t.contains("class SoapFaultException extends RuntimeException"), "{t}");
    }

    #[test]
    fn soap_12_puts_the_action_in_the_content_type_and_drops_the_header() {
        let envelope = SAMPLE_ENVELOPE.replace(crate::soap::SOAP11, crate::soap::SOAP12);
        let t = text(&gen(serde_json::json!({ "envelope": envelope, "soap_action": "urn:x" })));
        assert!(t.contains("application/soap+xml; charset=utf-8; action=\\\"urn:x\\\""), "{t}");
        assert!(!t.contains("\"SOAPAction\""), "{t}");
    }

    #[test]
    fn ws_security_adds_the_header_method_and_credentials() {
        let t = text(&gen(serde_json::json!({ "ws_security": true })));
        assert!(t.contains("public GetWeatherClient(String endpoint, String username, String password)"), "{t}");
        assert!(t.contains("String.format(ENVELOPE, securityHeader(), xml(city), xml(country))"), "{t}");
        assert!(t.contains("private String securityHeader() {"), "{t}");
        assert!(t.contains("wsse:UsernameToken"), "{t}");
        assert!(t.contains("<soapenv:Header>%s</soapenv:Header>"), "{t}");
    }

    #[test]
    fn before_java_15_the_envelope_is_concatenated() {
        let mut ctx = sample_context();
        ctx.project.java = 11;
        let mut options = SoapCall.defaults(&ctx);
        options["package"] = "p".into();
        let t = SoapCall.generate(&options, &ctx).unwrap().files[0].text.clone();
        assert!(!t.contains("\"\"\""), "{t}");
        assert!(t.contains("private static final String ENVELOPE ="), "{t}");
        assert!(t.contains("+ \"   <soapenv:Header/>\\n\"") || t.contains("+ \"<soapenv:Header/>\\n\"") || t.contains("Header/>"), "{t}");
    }

    #[test]
    fn spring_ws_sends_the_payload_with_a_soap_action_callback() {
        let t = text(&gen(serde_json::json!({ "variant": "webservicetemplate", "soap_action": "urn:a", "ws_security": true })));
        assert!(t.contains("@Component"), "{t}");
        assert!(t.contains("@Value(\"${soap.get-weather.endpoint}\") String endpoint"), "{t}");
        assert!(t.contains("private static final String PAYLOAD = \"\"\""), "{t}");
        assert!(!t.contains("ENV_NS"), "{t}");
        assert!(t.contains("template.sendSourceAndReceiveToResult(endpoint, request, callback(), new StreamResult(response));"), "{t}");
        assert!(t.contains("soap.setSoapAction(\"urn:a\");"), "{t}");
        assert!(t.contains("soap.getSoapHeader().getResult()"), "{t}");
        assert!(t.contains("throws jakarta.xml.soap.SOAPException"), "{t}");
    }

    #[test]
    fn web_client_returns_a_mono_and_reads_the_body_of_any_status() {
        let t = text(&gen(serde_json::json!({ "variant": "webclient" })));
        assert!(t.contains("public Mono<String> getWeather(String city, String country) {"), "{t}");
        assert!(t.contains(".exchangeToMono(response -> response.bodyToMono(String.class))"), "{t}");
        assert!(t.contains(".map(GetWeatherClient::parseResponse);"), "{t}");
    }

    #[test]
    fn cxf_dispatch_uses_the_wsdl_names_and_the_action() {
        let wsdl = r#"<wsdl:definitions xmlns:wsdl="http://schemas.xmlsoap.org/wsdl/" xmlns:soap="http://schemas.xmlsoap.org/wsdl/soap/" targetNamespace="http://example.com/weather">
<wsdl:portType name="WeatherPortType"/>
<wsdl:binding name="B" type="x"><wsdl:operation name="GetWeather"><soap:operation soapAction="urn:gw"/></wsdl:operation></wsdl:binding>
<wsdl:service name="WeatherService"><wsdl:port name="WeatherPort" binding="b"><soap:address location="http://h/ws"/></wsdl:port></wsdl:service>
</wsdl:definitions>"#;
        let out = gen(serde_json::json!({ "variant": "cxf-dispatch", "wsdl": wsdl }));
        let t = text(&out);
        assert!(t.contains("new QName(NAMESPACE, \"WeatherService\")"), "{t}");
        assert!(t.contains("new QName(NAMESPACE, \"WeatherPort\")"), "{t}");
        assert!(t.contains("jaxws.addPort(port, SOAPBinding.SOAP11HTTP_BINDING, endpoint);"), "{t}");
        assert!(t.contains("BindingProvider.SOAPACTION_URI_PROPERTY, \"urn:gw\""), "{t}");
        assert!(t.contains("import jakarta.xml.ws.Dispatch;"), "{t}");
        assert!(!out.notes.iter().any(|n| n.contains("is a guess")), "{:?}", out.notes);
    }

    #[test]
    fn cxf_port_calls_the_generated_stub_and_wires_wss4j() {
        let t = text(&gen(serde_json::json!({ "variant": "cxf-port", "ws_security": true })));
        assert!(t.contains("private final GetWeatherPortType port;"), "{t}");
        assert!(t.contains("factory.setServiceClass(GetWeatherPortType.class);"), "{t}");
        assert!(t.contains("return port.getWeather(city, country);"), "{t}");
        assert!(t.contains("new WSS4JOutInterceptor(security)"), "{t}");
        assert!(!t.contains("ENVELOPE"), "{t}");
    }

    #[test]
    fn apache_executes_a_post_with_a_response_handler() {
        let t = text(&gen(serde_json::json!({ "variant": "apache" })));
        assert!(t.contains("HttpPost post = new HttpPost(endpoint);"), "{t}");
        assert!(t.contains("return client.execute(post, response -> parseResponse(EntityUtils.toString(response.getEntity(), StandardCharsets.UTF_8)));"), "{t}");
    }

    #[test]
    fn jaxb_unmarshals_the_first_body_element_and_several_xpaths_make_a_map() {
        let t = text(&gen(serde_json::json!({ "response": "jaxb", "response_type": "WeatherResult" })));
        assert!(t.contains("private static WeatherResult parseResponse(String xml) {"), "{t}");
        assert!(t.contains("unmarshal(payload, WeatherResult.class).getValue()"), "{t}");
        assert!(t.contains("import jakarta.xml.bind.JAXBContext;"), "{t}");
        let many = text(&gen(serde_json::json!({ "response_fields": [{ "name": "temp", "ty": "//*[local-name()='Temp']" }, { "name": "sky", "ty": "//*[local-name()='Sky']" }] })));
        assert!(many.contains("public Map<String, String> getWeather("), "{many}");
        assert!(many.contains("result.put(\"temp\", xpath(document, \"//*[local-name()='Temp']\"));"), "{many}");
    }

    #[test]
    fn a_missing_endpoint_and_action_are_noted_and_bad_input_refused() {
        let out = gen(serde_json::json!({}));
        assert!(out.notes.iter().any(|n| n.contains("No endpoint")), "{:?}", out.notes);
        assert!(out.notes.iter().any(|n| n.contains("SOAPAction")), "{:?}", out.notes);
        let ctx = sample_context();
        assert!(SoapCall.generate(&serde_json::json!({ "envelope": "<a/>" }), &ctx).is_err());
        assert!(SoapCall.generate(&serde_json::json!({ "variant": "soapui" }), &ctx).is_err());
        assert!(SoapCall.generate(&serde_json::json!({ "response": "jaxb" }), &ctx).is_err());
    }
}
