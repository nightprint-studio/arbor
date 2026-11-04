//! The Spring flavours of the HTTP client: `RestClient`, `WebClient`, `RestTemplate` and `@HttpExchange`
//! interfaces. They share the shape of a configuration class that builds the transport from the base-URL
//! property and returns the client, so the client itself carries no Spring annotations.

use crate::code::{Imports, JavaFile, Lines};
use crate::names::camel;

use super::{Op, Plan, Rendered, P};

/// How a `P` is written as a header/query value.
fn as_text(p: &P) -> String {
    if p.ty == "String" { p.var.clone() } else { format!("String.valueOf({})", p.var) }
}

fn spring6(plan: &Plan) -> bool {
    let p = &plan.ctx.project;
    p.spring_boot().is_some_and(|v| v.major >= 3) || p.version("spring-web").is_some_and(|v| v.major >= 6) || (p.spring_boot().is_none() && p.version("spring-web").is_none())
}

fn status_type(plan: &Plan, im: &mut Imports) -> &'static str {
    if spring6(plan) {
        im.add("org.springframework.http.HttpStatusCode");
        "HttpStatusCode"
    } else {
        im.add("org.springframework.http.HttpStatus");
        "HttpStatus"
    }
}

/// `.uri(...)` for a RestClient/WebClient chain: a template when only path variables are involved, a builder otherwise.
fn uri_lines(op: &Op, im: &mut Imports) -> Vec<String> {
    let args: Vec<String> = op.path_args().iter().map(|p| p.var.clone()).collect();
    if op.query.is_empty() {
        let tail = if args.is_empty() { String::new() } else { format!(", {}", args.join(", ")) };
        return vec![format!(".uri(\"{}\"{tail})", op.path)];
    }
    let mut lines = vec![".uri(uri -> uri".to_string(), format!("        .path(\"{}\")", op.path)];
    for q in &op.query {
        if q.always() {
            lines.push(format!("        .queryParam(\"{}\", {})", q.wire, q.var));
        } else {
            im.add("java.util.Optional");
            lines.push(format!("        .queryParamIfPresent(\"{}\", Optional.ofNullable({}))", q.wire, q.var));
        }
    }
    lines.push(format!("        .build({}))", args.join(", ")));
    lines
}

fn header_lines(op: &Op) -> Vec<String> {
    let mut lines = Vec::new();
    for h in &op.headers {
        if h.always() {
            lines.push(format!(".header(\"{}\", {})", h.wire, as_text(h)));
        } else {
            lines.push(".headers(headers -> {".to_string());
            lines.push(format!("    if ({} != null) {{", h.var));
            lines.push(format!("        headers.add(\"{}\", {});", h.wire, as_text(h)));
            lines.push("    }".to_string());
            lines.push("})".to_string());
        }
    }
    lines
}

/// `.accept(…)` and, with a body, `.contentType(…)` for XML — the Spring converters do the (un)marshalling.
fn xml_lines(op: &Op, xml: bool, im: &mut Imports) -> Vec<String> {
    if !xml {
        return Vec::new();
    }
    im.add("org.springframework.http.MediaType");
    let mut lines = vec![".accept(MediaType.APPLICATION_XML)".to_string()];
    if op.body.is_some() {
        lines.push(".contentType(MediaType.APPLICATION_XML)".to_string());
    }
    lines
}

fn emit_chain(l: &mut Lines, first: String, chain: Vec<String>) {
    l.line(first);
    l.indent().indent();
    for line in chain {
        l.line(line);
    }
    l.dedent().dedent();
}

fn generic(ty: &str) -> bool {
    ty.contains('<')
}

fn pt(ty: &str) -> String {
    format!("new ParameterizedTypeReference<{ty}>() {{}}")
}

/// The configuration class: one bean method, `body` writes how the transport is built.
fn config(plan: &Plan, bean: &str, builder_param: &str, bean_type: &str, body: impl FnOnce(&mut Lines, &mut Imports)) -> (String, JavaFile) {
    let name = format!("{}Config", plan.client);
    let mut f = JavaFile::new(plan.package());
    f.imports.add("org.springframework.beans.factory.annotation.Value");
    f.imports.add("org.springframework.context.annotation.Bean");
    f.imports.add("org.springframework.context.annotation.Configuration");
    let mut inner = Lines::new();
    body(&mut inner, &mut f.imports);
    let params = format!("{builder_param}, {}", plan.config_params());
    f.body.line("@Configuration");
    f.body.block(format!("public class {name}"), |l| {
        l.line("@Bean");
        l.block(format!("public {bean_type} {bean}({params})"), |l| {
            l.append(&inner);
        });
    });
    (name, f)
}

fn exception_handler_rest(plan: &Plan, l: &mut Lines, im: &mut Imports) {
    if !plan.exceptions() {
        return;
    }
    let status = status_type(plan, im);
    im.add("java.nio.charset.StandardCharsets");
    let name = plan.exception_name();
    l.line(format!("        .defaultStatusHandler({status}::isError, (request, response) -> {{"));
    l.line(format!("            throw new {name}(response.getStatusCode().value(), new String(response.getBody().readAllBytes(), StandardCharsets.UTF_8));"));
    l.line("        })");
}

// ── RestClient ───────────────────────────────────────────────────────────────

/// `RestClient restClient = builder…build();` — shared with the `@HttpExchange` flavour.
fn rest_client_build(plan: &Plan, l: &mut Lines, im: &mut Imports) {
    im.add("org.springframework.web.client.RestClient");
    if let Some(seconds) = plan.timeout() {
        im.add("org.springframework.http.client.SimpleClientHttpRequestFactory");
        l.line("SimpleClientHttpRequestFactory requestFactory = new SimpleClientHttpRequestFactory();");
        l.line(format!("requestFactory.setConnectTimeout({});", seconds * 1000));
        l.line(format!("requestFactory.setReadTimeout({});", seconds * 1000));
    }
    l.line("RestClient restClient = builder");
    l.line("        .baseUrl(baseUrl)");
    if plan.timeout().is_some() {
        l.line("        .requestFactory(requestFactory)");
    }
    if let Some((name, value)) = plan.auth_header(im) {
        l.line(format!("        .defaultHeader(\"{name}\", {value})"));
    }
    exception_handler_rest(plan, l, im);
    l.line("        .build();");
}

pub(super) fn rest_client(plan: &Plan) -> Rendered {
    let mut f = JavaFile::new(plan.package());
    plan.dto_imports(&mut f);
    f.imports.add("org.springframework.web.client.RestClient");
    let mut body = Lines::new();
    body.line("private final RestClient restClient;");
    body.blank();
    body.block(format!("public {}(RestClient restClient)", plan.client), |l| {
        l.line("this.restClient = restClient;");
    });
    for op in &plan.ops {
        body.blank();
        rest_client_op(&mut body, &mut f.imports, op, plan.xml());
    }
    f.body.block(format!("public class {}", plan.client), |l| {
        l.append(&body);
    });

    let (name, cfg) = config(plan, &camel(&plan.client), "RestClient.Builder builder", &plan.client, |l, im| {
        rest_client_build(plan, l, im);
        l.line(format!("return new {}(restClient);", plan.client));
    });
    let mut r = Rendered::new(f);
    r.config = Some((name, cfg));
    if plan.ctx.project.version("spring-web").is_some_and(|v| !v.at_least(6, 1)) {
        r.notes.push("`RestClient` needs Spring Framework 6.1 (Boot 3.2) — this project's spring-web is older; pick `RestTemplate` or `WebClient`.".into());
    }
    r.dependencies = vec!["org.springframework.boot:spring-boot-starter-web"];
    r
}

fn rest_client_op(l: &mut Lines, im: &mut Imports, op: &Op, xml: bool) {
    let ret = &op.ret;
    l.block(format!("public {ret} {}({})", op.name, op.signature()), |l| {
        let mut chain = uri_lines(op, im);
        chain.extend(header_lines(op));
        chain.extend(xml_lines(op, xml, im));
        if let Some(b) = &op.body {
            chain.push(format!(".body({})", b.var));
        }
        chain.push(".retrieve()".into());
        chain.push(if op.is_void() {
            ".toBodilessEntity();".to_string()
        } else if generic(ret) {
            im.add("org.springframework.core.ParameterizedTypeReference");
            format!(".body({});", pt(ret))
        } else {
            format!(".body({ret}.class);")
        });
        let verb = op.http.to_lowercase();
        emit_chain(l, format!("{}restClient.{verb}()", if op.is_void() { "" } else { "return " }), chain);
    });
}

// ── WebClient ────────────────────────────────────────────────────────────────

pub(super) fn web_client(plan: &Plan) -> Rendered {
    let mut f = JavaFile::new(plan.package());
    plan.dto_imports(&mut f);
    f.imports.add("org.springframework.web.reactive.function.client.WebClient");
    let mut body = Lines::new();
    body.line("private final WebClient webClient;");
    body.blank();
    body.block(format!("public {}(WebClient webClient)", plan.client), |l| {
        l.line("this.webClient = webClient;");
    });
    for op in &plan.ops {
        body.blank();
        web_client_op(&mut body, &mut f.imports, op, plan.o.reactive, plan.xml());
    }
    f.body.block(format!("public class {}", plan.client), |l| {
        l.append(&body);
    });

    let (name, cfg) = config(plan, &camel(&plan.client), "WebClient.Builder builder", &plan.client, |l, im| {
        im.add("org.springframework.web.reactive.function.client.WebClient");
        l.line("WebClient.Builder configured = builder.baseUrl(baseUrl);");
        if let Some(seconds) = plan.timeout() {
            im.add("io.netty.channel.ChannelOption");
            im.add("java.time.Duration");
            im.add("org.springframework.http.client.reactive.ReactorClientHttpConnector");
            im.add("reactor.netty.http.client.HttpClient");
            l.line("HttpClient httpClient = HttpClient.create()");
            l.line(format!("        .option(ChannelOption.CONNECT_TIMEOUT_MILLIS, {})", seconds * 1000));
            l.line(format!("        .responseTimeout(Duration.ofSeconds({seconds}));"));
            l.line("configured.clientConnector(new ReactorClientHttpConnector(httpClient));");
        }
        if let Some((name, value)) = plan.auth_header(im) {
            l.line(format!("configured.defaultHeader(\"{name}\", {value});"));
        }
        if plan.exceptions() {
            let status = status_type(plan, im);
            im.add("reactor.core.publisher.Mono");
            let exception = plan.exception_name();
            l.line(format!("configured.defaultStatusHandler({status}::isError, response -> response.bodyToMono(String.class)"));
            l.line("        .defaultIfEmpty(\"\")");
            l.line(format!("        .flatMap(errorBody -> Mono.error(new {exception}(response.statusCode().value(), errorBody))));"));
        }
        l.line(format!("return new {}(configured.build());", plan.client));
    });
    let mut r = Rendered::new(f);
    r.config = Some((name, cfg));
    r.dependencies = vec!["org.springframework.boot:spring-boot-starter-webflux"];
    if !plan.o.reactive {
        r.notes.push("Blocking calls (`block()`) must not run on a Netty event-loop thread.".into());
    }
    r
}

fn web_client_op(l: &mut Lines, im: &mut Imports, op: &Op, reactive: bool, xml: bool) {
    let element = op.element();
    let ret = match (reactive, op.is_void(), op.produces_list) {
        (true, true, _) => {
            im.add("reactor.core.publisher.Mono");
            "Mono<Void>".to_string()
        }
        (true, false, true) => {
            im.add("reactor.core.publisher.Flux");
            format!("Flux<{element}>")
        }
        (true, false, false) => {
            im.add("reactor.core.publisher.Mono");
            format!("Mono<{}>", op.ret)
        }
        (false, true, _) => "void".to_string(),
        (false, false, _) => op.ret.clone(),
    };
    l.block(format!("public {ret} {}({})", op.name, op.signature()), |l| {
        let mut chain = uri_lines(op, im);
        chain.extend(header_lines(op));
        chain.extend(xml_lines(op, xml, im));
        if let Some(b) = &op.body {
            chain.push(format!(".bodyValue({})", b.var));
        }
        chain.push(".retrieve()".into());
        let type_arg = |ty: &str, im: &mut Imports| {
            if generic(ty) {
                im.add("org.springframework.core.ParameterizedTypeReference");
                pt(ty)
            } else {
                format!("{ty}.class")
            }
        };
        let mut tail = if op.is_void() {
            vec![".bodyToMono(Void.class)".to_string()]
        } else if op.produces_list {
            vec![format!(".bodyToFlux({})", type_arg(&element, im))]
        } else {
            vec![format!(".bodyToMono({})", type_arg(&op.ret, im))]
        };
        if !reactive {
            if op.produces_list {
                tail.push(".collectList()".into());
            }
            tail.push(".block()".into());
        }
        chain.extend(tail);
        if let Some(last) = chain.last_mut() {
            last.push(';');
        }
        let returns = if reactive || !op.is_void() { "return " } else { "" };
        emit_chain(l, format!("{returns}webClient.{}()", op.http.to_lowercase()), chain);
    });
}

// ── RestTemplate ─────────────────────────────────────────────────────────────

pub(super) fn rest_template(plan: &Plan) -> Rendered {
    let mut f = JavaFile::new(plan.package());
    plan.dto_imports(&mut f);
    f.imports.add("org.springframework.web.client.RestTemplate");
    f.imports.add("org.springframework.http.HttpMethod");
    let mut body = Lines::new();
    body.line("private final RestTemplate restTemplate;");
    body.blank();
    body.block(format!("public {}(RestTemplate restTemplate)", plan.client), |l| {
        l.line("this.restTemplate = restTemplate;");
    });
    for op in &plan.ops {
        body.blank();
        rest_template_op(&mut body, &mut f.imports, op, plan.xml());
    }
    f.body.block(format!("public class {}", plan.client), |l| {
        l.append(&body);
    });

    let (name, cfg) = config(plan, &camel(&plan.client), "RestTemplateBuilder builder", &plan.client, |l, im| {
        im.add("org.springframework.boot.web.client.RestTemplateBuilder");
        im.add("org.springframework.web.client.RestTemplate");
        l.line("RestTemplateBuilder configured = builder.rootUri(baseUrl);");
        if let Some(seconds) = plan.timeout() {
            im.add("java.time.Duration");
            l.line(format!("configured = configured.setConnectTimeout(Duration.ofSeconds({seconds})).setReadTimeout(Duration.ofSeconds({seconds}));"));
        }
        if let Some((name, value)) = plan.auth_header(im) {
            l.line(format!("configured = configured.defaultHeader(\"{name}\", {value});"));
        }
        l.line("RestTemplate restTemplate = configured.build();");
        l.line(format!("return new {}(restTemplate);", plan.client));
    });
    let mut r = Rendered::new(f);
    r.config = Some((name, cfg));
    r.dependencies = vec!["org.springframework.boot:spring-boot-starter-web"];
    r
}

fn rest_template_op(l: &mut Lines, im: &mut Imports, op: &Op, xml: bool) {
    let ret = &op.ret;
    l.block(format!("public {ret} {}({})", op.name, op.signature()), |l| {
        if op.query.is_empty() {
            l.line(format!("String url = \"{}\";", op.path));
        } else {
            im.add("org.springframework.web.util.UriComponentsBuilder");
            l.line(format!("String url = UriComponentsBuilder.fromPath(\"{}\")", op.path));
            l.indent().indent();
            for q in &op.query {
                if q.always() {
                    l.line(format!(".queryParam(\"{}\", {})", q.wire, q.var));
                } else {
                    im.add("java.util.Optional");
                    l.line(format!(".queryParamIfPresent(\"{}\", Optional.ofNullable({}))", q.wire, q.var));
                }
            }
            l.line(".encode()");
            l.line(".toUriString();");
            l.dedent().dedent();
        }
        let entity = if op.headers.is_empty() && op.body.is_none() && !xml {
            "null".to_string()
        } else {
            im.add("org.springframework.http.HttpEntity");
            let mut headers = "headers".to_string();
            if op.headers.is_empty() && !xml {
                headers = String::new();
            } else {
                im.add("org.springframework.http.HttpHeaders");
                l.line("HttpHeaders headers = new HttpHeaders();");
                for h in &op.headers {
                    if h.always() {
                        l.line(format!("headers.set(\"{}\", {});", h.wire, as_text(h)));
                    } else {
                        l.line(format!("if ({} != null) {{", h.var));
                        l.line(format!("    headers.set(\"{}\", {});", h.wire, as_text(h)));
                        l.line("}");
                    }
                }
                if xml {
                    im.add("org.springframework.http.MediaType");
                    l.line("headers.setAccept(List.of(MediaType.APPLICATION_XML));");
                    if op.body.is_some() {
                        l.line("headers.setContentType(MediaType.APPLICATION_XML);");
                    }
                }
            }
            match (&op.body, headers.is_empty()) {
                (Some(b), false) => {
                    l.line(format!("HttpEntity<{}> entity = new HttpEntity<>({}, headers);", b.ty, b.var));
                }
                (Some(b), true) => {
                    l.line(format!("HttpEntity<{}> entity = new HttpEntity<>({});", b.ty, b.var));
                }
                (None, _) => {
                    l.line("HttpEntity<Void> entity = new HttpEntity<>(headers);");
                }
            }
            "entity".to_string()
        };
        let response_type = if op.is_void() {
            "Void.class".to_string()
        } else if generic(ret) {
            im.add("org.springframework.core.ParameterizedTypeReference");
            pt(ret)
        } else {
            format!("{ret}.class")
        };
        let vars: String = op.path_args().iter().map(|p| format!(", {}", p.var)).collect();
        let call = format!("restTemplate.exchange(url, HttpMethod.{}, {entity}, {response_type}{vars})", op.http);
        if op.is_void() {
            l.line(format!("{call};"));
        } else {
            l.line(format!("return {call}.getBody();"));
        }
    });
}

// ── @HttpExchange interface ──────────────────────────────────────────────────

fn exchange_annotation(http: &str) -> &'static str {
    match http {
        "POST" => "PostExchange",
        "PUT" => "PutExchange",
        "PATCH" => "PatchExchange",
        "DELETE" => "DeleteExchange",
        _ => "GetExchange",
    }
}

pub(super) fn http_interface(plan: &Plan) -> Rendered {
    let mut f = JavaFile::new(plan.package());
    plan.dto_imports(&mut f);
    let mut body = Lines::new();
    for (i, op) in plan.ops.iter().enumerate() {
        if i > 0 {
            body.blank();
        }
        let annotation = exchange_annotation(&op.http);
        f.imports.add(format!("org.springframework.web.service.annotation.{annotation}"));
        body.line(format!("@{annotation}(\"{}\")", op.path));
        let mut params: Vec<String> = Vec::new();
        for p in &op.path_params {
            f.imports.add("org.springframework.web.bind.annotation.PathVariable");
            params.push(format!("@PathVariable(\"{}\") {} {}", p.wire, p.ty, p.var));
        }
        for p in &op.query {
            f.imports.add("org.springframework.web.bind.annotation.RequestParam");
            let required = if p.always() { String::new() } else { ", required = false".to_string() };
            params.push(format!("@RequestParam(name = \"{}\"{required}) {} {}", p.wire, p.ty, p.var));
        }
        for p in &op.headers {
            f.imports.add("org.springframework.web.bind.annotation.RequestHeader");
            params.push(format!("@RequestHeader(\"{}\") {} {}", p.wire, p.ty, p.var));
        }
        if let Some(b) = &op.body {
            f.imports.add("org.springframework.web.bind.annotation.RequestBody");
            params.push(format!("@RequestBody {} {}", b.ty, b.var));
        }
        body.line(format!("{} {}({});", op.ret, op.name, params.join(", ")));
    }
    if plan.xml() {
        f.imports.add("org.springframework.http.MediaType");
        f.imports.add("org.springframework.web.service.annotation.HttpExchange");
        f.body.line("@HttpExchange(accept = MediaType.APPLICATION_XML_VALUE, contentType = MediaType.APPLICATION_XML_VALUE)");
    }
    f.body.block(format!("public interface {}", plan.client), |l| {
        l.append(&body);
    });

    let client = plan.client.clone();
    let (name, cfg) = config(plan, &camel(&plan.client), "RestClient.Builder builder", &plan.client, |l, im| {
        rest_client_build(plan, l, im);
        im.add("org.springframework.web.client.support.RestClientAdapter");
        im.add("org.springframework.web.service.invoker.HttpServiceProxyFactory");
        l.line("HttpServiceProxyFactory factory = HttpServiceProxyFactory.builderFor(RestClientAdapter.create(restClient)).build();");
        l.line(format!("return factory.createClient({client}.class);"));
    });
    let mut r = Rendered::new(f);
    r.config = Some((name, cfg));
    r.dependencies = vec!["org.springframework.boot:spring-boot-starter-web"];
    if plan.ctx.project.version("spring-web").is_some_and(|v| !v.at_least(6, 1)) {
        r.notes.push("`@HttpExchange` over `RestClient` needs Spring Framework 6.1; on 6.0 build the proxy with `WebClientAdapter`.".into());
    }
    r
}
