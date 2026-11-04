//! The non-`RestClient` flavours: OpenFeign, `java.net.http.HttpClient`, OkHttp and Retrofit.

use crate::code::{Imports, JavaFile, Lines};
use crate::names::{camel, kebab};

use super::{Op, Plan, Rendered};

fn as_text(var: &str, ty: &str) -> String {
    if ty == "String" { var.to_string() } else { format!("String.valueOf({var})") }
}

/// A path as the literal and variable pieces it is made of: `/a/{id}/b` → `["/a/", var id, "/b"]`.
enum Piece<'a> {
    Text(String),
    Var(&'a str),
}

fn pieces<'a>(op: &'a Op) -> Vec<Piece<'a>> {
    let mut out = Vec::new();
    let mut rest = op.path.as_str();
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}') else { break };
        if open > 0 {
            out.push(Piece::Text(rest[..open].to_string()));
        }
        let name = rest[open + 1..open + close].split(':').next().unwrap_or("").trim();
        match op.path_params.iter().find(|p| p.wire == name) {
            Some(p) => out.push(Piece::Var(&p.var)),
            None => out.push(Piece::Text(rest[open..open + close + 1].to_string())),
        }
        rest = &rest[open + close + 1..];
    }
    if !rest.is_empty() {
        out.push(Piece::Text(rest.to_string()));
    }
    out
}

fn auth_params(plan: &Plan) -> Vec<String> {
    plan.auth_props().iter().map(|a| format!("{} String {}", plan.value_expr(&a.key), a.var)).collect()
}

fn config_file(plan: &Plan, params: Vec<String>, bean_type: &str, body: impl FnOnce(&mut Lines, &mut Imports)) -> (String, JavaFile) {
    let name = format!("{}Config", plan.client);
    let mut f = JavaFile::new(plan.package());
    f.imports.add("org.springframework.beans.factory.annotation.Value");
    f.imports.add("org.springframework.context.annotation.Bean");
    f.imports.add("org.springframework.context.annotation.Configuration");
    let mut inner = Lines::new();
    body(&mut inner, &mut f.imports);
    f.body.line("@Configuration");
    f.body.block(format!("public class {name}"), |l| {
        l.line("@Bean");
        l.block(format!("public {bean_type} {}({})", camel(&plan.client), params.join(", ")), |l| {
            l.append(&inner);
        });
    });
    (name, f)
}

fn base_url_param(plan: &Plan) -> String {
    format!("{} String baseUrl", plan.value_expr("base-url"))
}

// ── OpenFeign ────────────────────────────────────────────────────────────────

fn mapping_annotation(http: &str) -> &'static str {
    match http {
        "POST" => "PostMapping",
        "PUT" => "PutMapping",
        "PATCH" => "PatchMapping",
        "DELETE" => "DeleteMapping",
        _ => "GetMapping",
    }
}

pub(super) fn feign(plan: &Plan) -> Rendered {
    let mut f = JavaFile::new(plan.package());
    plan.dto_imports(&mut f);
    f.imports.add("org.springframework.cloud.openfeign.FeignClient");
    let config_name = format!("{}Config", plan.client);
    let needs_config = !plan.auth_props().is_empty() || plan.exceptions();
    let service = kebab(plan.client.trim_end_matches("Api").trim_end_matches("Client"));
    let mut header = format!("@FeignClient(name = \"{service}\", url = \"${{{}}}\"", plan.base_url_key());
    if needs_config {
        header.push_str(&format!(", configuration = {config_name}.class"));
    }
    header.push(')');
    f.body.line(header);
    let mut body = Lines::new();
    for (i, op) in plan.ops.iter().enumerate() {
        if i > 0 {
            body.blank();
        }
        let annotation = mapping_annotation(&op.http);
        f.imports.add(format!("org.springframework.web.bind.annotation.{annotation}"));
        if plan.xml() {
            body.line(format!("@{annotation}(value = \"{}\", consumes = \"application/xml\", produces = \"application/xml\")", op.path));
        } else {
            body.line(format!("@{annotation}(\"{}\")", op.path));
        }
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
    f.body.block(format!("public interface {}", plan.client), |l| {
        l.append(&body);
    });

    let mut r = Rendered::new(f);
    r.dependencies = vec!["org.springframework.cloud:spring-cloud-starter-openfeign"];
    r.notes.push("Enable it with `@EnableFeignClients` on the application class (or `@EnableFeignClients(basePackages = …)`).".into());
    if needs_config {
        r.config = Some(feign_config(plan, &config_name));
    }
    r
}

/// Not a `@Configuration`: Feign applies it to this client only, which is the point of naming it in `configuration`.
fn feign_config(plan: &Plan, name: &str) -> (String, JavaFile) {
    let mut f = JavaFile::new(plan.package());
    f.imports.add("org.springframework.beans.factory.annotation.Value");
    f.imports.add("org.springframework.context.annotation.Bean");
    let mut body = Lines::new();
    if !plan.auth_props().is_empty() {
        f.imports.add("feign.RequestInterceptor");
        if let Some((header, value)) = plan.auth_header(&mut f.imports) {
            body.line("@Bean");
            body.block(format!("public RequestInterceptor authInterceptor({})", auth_params(plan).join(", ")), |l| {
                l.line(format!("return template -> template.header(\"{header}\", {value});"));
            });
        }
    }
    if plan.exceptions() {
        if !body.is_empty() {
            body.blank();
        }
        let exception = plan.exception_name();
        f.imports.extend(["feign.Response", "feign.Util", "feign.codec.ErrorDecoder", "java.io.IOException", "java.io.Reader", "java.nio.charset.StandardCharsets"]);
        body.line("@Bean");
        body.block("public ErrorDecoder errorDecoder()", |l| {
            l.line(format!("return (methodKey, response) -> new {exception}(response.status(), bodyOf(response));"));
        });
        body.blank();
        body.block("private static String bodyOf(Response response)", |l| {
            l.block("if (response.body() == null)", |l| {
                l.line("return \"\";");
            });
            l.block("try (Reader reader = response.body().asReader(StandardCharsets.UTF_8))", |l| {
                l.line("return Util.toString(reader);");
            });
            l.block_after("catch (IOException e)", |l| {
                l.line("return \"\";");
            });
        });
    }
    f.body.block(format!("public class {name}"), |l| {
        l.append(&body);
    });
    (name.to_string(), f)
}

// ── java.net.http ────────────────────────────────────────────────────────────

fn path_expression(op: &Op) -> String {
    let parts: Vec<String> = pieces(op)
        .into_iter()
        .map(|p| match p {
            Piece::Text(t) => format!("\"{t}\""),
            Piece::Var(v) => format!("encode({v})"),
        })
        .collect();
    if parts.is_empty() { "\"\"".to_string() } else { parts.join(" + ") }
}

pub(super) fn jdk(plan: &Plan) -> Rendered {
    let mut f = JavaFile::new(plan.package());
    plan.dto_imports(&mut f);
    f.imports.extend([
        "com.fasterxml.jackson.core.type.TypeReference",
        "com.fasterxml.jackson.databind.ObjectMapper",
        "java.io.IOException",
        "java.io.UncheckedIOException",
        "java.net.URI",
        "java.net.http.HttpClient",
        "java.net.http.HttpRequest",
        "java.net.http.HttpResponse",
        "java.time.Duration",
    ]);
    let mut body = Lines::new();
    body.text(
        "private final HttpClient http;\nprivate final ObjectMapper mapper;\nprivate final String baseUrl;\nprivate final Map<String, String> headers;\nprivate final Duration timeout;",
    );
    body.blank();
    body.block(
        format!("public {}(HttpClient http, ObjectMapper mapper, String baseUrl, Map<String, String> headers, Duration timeout)", plan.client),
        |l| {
            l.text("this.http = http;\nthis.mapper = mapper;\nthis.baseUrl = baseUrl;\nthis.headers = headers;\nthis.timeout = timeout;");
        },
    );
    let mut has_body = false;
    for op in &plan.ops {
        body.blank();
        has_body |= op.body.is_some();
        jdk_op(&mut body, &mut f.imports, op, plan.media_type());
    }
    body.blank();
    jdk_helpers(&mut body, &mut f.imports, plan, has_body);
    f.body.block(format!("public class {}", plan.client), |l| {
        l.append(&body);
    });

    let (name, cfg) = config_file(plan, [leading_params(plan), auth_params(plan)].concat(), &plan.client, |l, im| {
        mapper_setup(plan, l, im);
        im.add("java.net.http.HttpClient");
        im.add("java.time.Duration");
        im.add("java.util.LinkedHashMap");
        im.add("java.util.Map");
        let seconds = plan.timeout().unwrap_or(30);
        l.line(format!("Duration timeout = Duration.ofSeconds({seconds});"));
        l.line("HttpClient http = HttpClient.newBuilder().connectTimeout(timeout).build();");
        l.line("Map<String, String> headers = new LinkedHashMap<>();");
        if let Some((name, value)) = plan.auth_header(im) {
            l.line(format!("headers.put(\"{name}\", {value});"));
        }
        l.line(format!("return new {}(http, mapper, baseUrl, headers, timeout);", plan.client));
    });
    let mut r = Rendered::new(f);
    r.config = Some((name, cfg));
    r.dependencies = vec!["com.fasterxml.jackson.core:jackson-databind"];
    r
}

fn jdk_op(l: &mut Lines, im: &mut Imports, op: &Op, media: &str) {
    l.block(format!("public {} {}({})", op.ret, op.name, op.signature()), |l| {
        l.line(format!("String uriPath = {};", path_expression(op)));
        if !op.query.is_empty() {
            im.add("java.util.ArrayList");
            l.line("List<String> queryParams = new ArrayList<>();");
            for q in &op.query {
                let add = format!("queryParams.add(\"{}=\" + encode({}));", q.wire, q.var);
                if q.always() {
                    l.line(add);
                } else {
                    l.block(format!("if ({} != null)", q.var), |l| {
                        l.line(add);
                    });
                }
            }
        }
        let query = if op.query.is_empty() { "List.of()" } else { "queryParams" };
        l.line(format!("HttpRequest.Builder requestBuilder = request(uriPath, {query});"));
        for h in &op.headers {
            let set = format!("requestBuilder.header(\"{}\", {});", h.wire, as_text(&h.var, &h.ty));
            if h.always() {
                l.line(set);
            } else {
                l.block(format!("if ({} != null)", h.var), |l| {
                    l.line(set);
                });
            }
        }
        if op.body.is_some() {
            l.line(format!("requestBuilder.header(\"Content-Type\", \"{media}\");"));
        }
        im.add("java.net.http.HttpRequest.BodyPublishers");
        let publisher = match &op.body {
            Some(b) => format!("BodyPublishers.ofString(json({}))", b.var),
            None => "BodyPublishers.noBody()".to_string(),
        };
        let method = match op.http.as_str() {
            "GET" => ".GET()".to_string(),
            "DELETE" => ".DELETE()".to_string(),
            "POST" => format!(".POST({publisher})"),
            "PUT" => format!(".PUT({publisher})"),
            other => format!(".method(\"{other}\", {publisher})"),
        };
        l.line(format!("HttpRequest httpRequest = requestBuilder{method}.build();"));
        if op.is_void() {
            l.line("send(httpRequest, null);");
        } else {
            l.line(format!("return send(httpRequest, new TypeReference<{}>() {{}});", op.ret));
        }
    });
}

fn jdk_helpers(l: &mut Lines, im: &mut Imports, plan: &Plan, has_body: bool) {
    im.add("java.net.URLEncoder");
    im.add("java.nio.charset.StandardCharsets");
    l.block("private HttpRequest.Builder request(String uriPath, List<String> queryParams)", |l| {
        l.line("String query = queryParams.isEmpty() ? \"\" : \"?\" + String.join(\"&\", queryParams);");
        l.line("HttpRequest.Builder builder = HttpRequest.newBuilder(URI.create(baseUrl + uriPath + query))");
        l.line("        .timeout(timeout)");
        l.line(format!("        .header(\"Accept\", \"{}\");", plan.media_type()));
        l.line("headers.forEach(builder::header);");
        l.line("return builder;");
    });
    l.blank();
    l.block("private <T> T send(HttpRequest httpRequest, TypeReference<T> type)", |l| {
        l.block("try", |l| {
            l.line("HttpResponse<String> response = http.send(httpRequest, HttpResponse.BodyHandlers.ofString());");
            l.block("if (response.statusCode() >= 400)", |l| {
                if plan.exceptions() {
                    l.line(format!("throw new {}(response.statusCode(), response.body());", plan.exception_name()));
                } else {
                    l.line("throw new IllegalStateException(\"HTTP \" + response.statusCode() + \": \" + response.body());");
                }
            });
            l.block("if (type == null || response.body() == null || response.body().isBlank())", |l| {
                l.line("return null;");
            });
            l.line("return mapper.readValue(response.body(), type);");
        });
        l.block_after("catch (IOException e)", |l| {
            l.line("throw new UncheckedIOException(e);");
        });
        l.block_after("catch (InterruptedException e)", |l| {
            l.line("Thread.currentThread().interrupt();");
            l.line("throw new IllegalStateException(\"Interrupted\", e);");
        });
    });
    if has_body {
        im.add("com.fasterxml.jackson.core.JsonProcessingException");
        l.blank();
        l.block("private String json(Object value)", |l| {
            l.block("try", |l| {
                l.line("return mapper.writeValueAsString(value);");
            });
            l.block_after("catch (JsonProcessingException e)", |l| {
                l.line("throw new IllegalArgumentException(\"Cannot write the request body\", e);");
            });
        });
    }
    l.blank();
    l.block("private static String encode(Object value)", |l| {
        l.line("return URLEncoder.encode(String.valueOf(value), StandardCharsets.UTF_8);");
    });
}

// ── OkHttp ───────────────────────────────────────────────────────────────────

/// `OkHttpClient client = …;` with timeouts and the auth header as an interceptor — shared with Retrofit.
fn okhttp_client(plan: &Plan, l: &mut Lines, im: &mut Imports) {
    im.add("okhttp3.OkHttpClient");
    l.line("OkHttpClient.Builder builder = new OkHttpClient.Builder();");
    if let Some(seconds) = plan.timeout() {
        im.add("java.util.concurrent.TimeUnit");
        l.line(format!("builder.connectTimeout({seconds}, TimeUnit.SECONDS).readTimeout({seconds}, TimeUnit.SECONDS);"));
    }
    if let Some((name, value)) = plan.auth_header(im) {
        l.line(format!("builder.addInterceptor(chain -> chain.proceed(chain.request().newBuilder().header(\"{name}\", {value}).build()));"));
    }
    l.line("OkHttpClient client = builder.build();");
}

pub(super) fn okhttp(plan: &Plan) -> Rendered {
    let mut f = JavaFile::new(plan.package());
    plan.dto_imports(&mut f);
    f.imports.extend([
        "com.fasterxml.jackson.core.type.TypeReference",
        "com.fasterxml.jackson.databind.ObjectMapper",
        "java.io.IOException",
        "java.io.UncheckedIOException",
        "okhttp3.HttpUrl",
        "okhttp3.MediaType",
        "okhttp3.OkHttpClient",
        "okhttp3.Request",
        "okhttp3.RequestBody",
        "okhttp3.Response",
    ]);
    let has_body = plan.ops.iter().any(|op| op.body.is_some());
    let mut body = Lines::new();
    body.line(format!("private static final MediaType JSON = MediaType.get(\"{}\");", plan.media_type()));
    body.blank();
    body.text("private final OkHttpClient client;\nprivate final ObjectMapper mapper;\nprivate final HttpUrl baseUrl;");
    body.blank();
    body.block(format!("public {}(OkHttpClient client, ObjectMapper mapper, String baseUrl)", plan.client), |l| {
        l.text("this.client = client;\nthis.mapper = mapper;\nthis.baseUrl = HttpUrl.get(baseUrl);");
    });
    for op in &plan.ops {
        body.blank();
        okhttp_op(&mut body, op, plan.xml());
    }
    body.blank();
    body.block("private <T> T execute(Request request, TypeReference<T> type)", |l| {
        l.block("try (Response response = client.newCall(request).execute())", |l| {
            l.line("String body = response.body() == null ? \"\" : response.body().string();");
            l.block("if (!response.isSuccessful())", |l| {
                if plan.exceptions() {
                    l.line(format!("throw new {}(response.code(), body);", plan.exception_name()));
                } else {
                    l.line("throw new IllegalStateException(\"HTTP \" + response.code() + \": \" + body);");
                }
            });
            l.block("if (type == null || body.isBlank())", |l| {
                l.line("return null;");
            });
            l.line("return mapper.readValue(body, type);");
        });
        l.block_after("catch (IOException e)", |l| {
            l.line("throw new UncheckedIOException(e);");
        });
    });
    if has_body {
        f.imports.add("com.fasterxml.jackson.core.JsonProcessingException");
        body.blank();
        body.block("private String json(Object value)", |l| {
            l.block("try", |l| {
                l.line("return mapper.writeValueAsString(value);");
            });
            l.block_after("catch (JsonProcessingException e)", |l| {
                l.line("throw new IllegalArgumentException(\"Cannot write the request body\", e);");
            });
        });
    }
    f.body.block(format!("public class {}", plan.client), |l| {
        l.append(&body);
    });

    let (name, cfg) = config_file(plan, [leading_params(plan), auth_params(plan)].concat(), &plan.client, |l, im| {
        mapper_setup(plan, l, im);
        okhttp_client(plan, l, im);
        l.line(format!("return new {}(client, mapper, baseUrl);", plan.client));
    });
    let mut r = Rendered::new(f);
    r.config = Some((name, cfg));
    r.dependencies = vec!["com.squareup.okhttp3:okhttp", "com.fasterxml.jackson.core:jackson-databind"];
    r
}

fn okhttp_op(l: &mut Lines, op: &Op, xml: bool) {
    l.block(format!("public {} {}({})", op.ret, op.name, op.signature()), |l| {
        l.line("HttpUrl.Builder url = baseUrl.newBuilder();");
        for piece in pieces(op) {
            match piece {
                Piece::Text(t) => {
                    let t = t.trim_matches('/');
                    if !t.is_empty() {
                        l.line(format!("url.addPathSegments(\"{t}\");"));
                    }
                }
                Piece::Var(v) => {
                    l.line(format!("url.addPathSegment(String.valueOf({v}));"));
                }
            }
        }
        for q in &op.query {
            let add = format!("url.addQueryParameter(\"{}\", String.valueOf({}));", q.wire, q.var);
            if q.always() {
                l.line(add);
            } else {
                l.block(format!("if ({} != null)", q.var), |l| {
                    l.line(add);
                });
            }
        }
        l.line("Request.Builder requestBuilder = new Request.Builder().url(url.build());");
        if xml {
            l.line("requestBuilder.header(\"Accept\", \"application/xml\");");
        }
        for h in &op.headers {
            let set = format!("requestBuilder.header(\"{}\", {});", h.wire, as_text(&h.var, &h.ty));
            if h.always() {
                l.line(set);
            } else {
                l.block(format!("if ({} != null)", h.var), |l| {
                    l.line(set);
                });
            }
        }
        let payload = match &op.body {
            Some(b) => format!("RequestBody.create(json({}), JSON)", b.var),
            None => "RequestBody.create(\"\", JSON)".to_string(),
        };
        match op.http.as_str() {
            "GET" => l.line("requestBuilder.get();"),
            "DELETE" if op.body.is_none() => l.line("requestBuilder.delete();"),
            other => l.line(format!("requestBuilder.method(\"{other}\", {payload});")),
        };
        if op.is_void() {
            l.line("execute(requestBuilder.build(), null);");
        } else {
            l.line(format!("return execute(requestBuilder.build(), new TypeReference<{}>() {{}});", op.ret));
        }
    });
}

// ── Retrofit ─────────────────────────────────────────────────────────────────

pub(super) fn retrofit(plan: &Plan) -> Rendered {
    let mut f = JavaFile::new(plan.package());
    plan.dto_imports(&mut f);
    f.imports.add("retrofit2.Call");
    let mut body = Lines::new();
    for (i, op) in plan.ops.iter().enumerate() {
        if i > 0 {
            body.blank();
        }
        let verb = match op.http.as_str() {
            "POST" | "PUT" | "PATCH" | "DELETE" | "GET" => op.http.clone(),
            _ => "GET".to_string(),
        };
        f.imports.add(format!("retrofit2.http.{verb}"));
        body.line(format!("@{verb}(\"{}\")", op.path.trim_start_matches('/')));
        if plan.xml() {
            f.imports.add("retrofit2.http.Headers");
            body.line("@Headers(\"Accept: application/xml\")");
        }
        let mut params: Vec<String> = Vec::new();
        for p in &op.path_params {
            f.imports.add("retrofit2.http.Path");
            params.push(format!("@Path(\"{}\") {} {}", p.wire, p.ty, p.var));
        }
        for p in &op.query {
            f.imports.add("retrofit2.http.Query");
            params.push(format!("@Query(\"{}\") {} {}", p.wire, p.ty, p.var));
        }
        for p in &op.headers {
            f.imports.add("retrofit2.http.Header");
            params.push(format!("@Header(\"{}\") {} {}", p.wire, p.ty, p.var));
        }
        if let Some(b) = &op.body {
            f.imports.add("retrofit2.http.Body");
            params.push(format!("@Body {} {}", b.ty, b.var));
        }
        let ret = if op.is_void() { "Void".to_string() } else { op.ret.clone() };
        body.line(format!("Call<{ret}> {}({});", op.name, params.join(", ")));
    }
    f.body.block(format!("public interface {}", plan.client), |l| {
        l.append(&body);
    });

    let client = plan.client.clone();
    let (name, cfg) = config_file(plan, [leading_params(plan), auth_params(plan)].concat(), &plan.client, |l, im| {
        mapper_setup(plan, l, im);
        im.add("retrofit2.Retrofit");
        let converter = if plan.xml() {
            im.add(if plan.ctx.project.jakarta() { "retrofit2.converter.jaxb3.JaxbConverterFactory" } else { "retrofit2.converter.jaxb.JaxbConverterFactory" });
            "JaxbConverterFactory.create()"
        } else {
            im.add("retrofit2.converter.jackson.JacksonConverterFactory");
            "JacksonConverterFactory.create(mapper)"
        };
        okhttp_client(plan, l, im);
        l.line("Retrofit retrofit = new Retrofit.Builder()");
        l.line("        .baseUrl(baseUrl.endsWith(\"/\") ? baseUrl : baseUrl + \"/\")");
        l.line("        .client(client)");
        l.line(format!("        .addConverterFactory({converter})"));
        l.line("        .build();");
        l.line(format!("return retrofit.create({client}.class);"));
    });
    let mut r = Rendered::new(f);
    r.config = Some((name, cfg));
    let converter_dep = if plan.xml() { "com.squareup.retrofit2:converter-jaxb" } else { "com.squareup.retrofit2:converter-jackson" };
    r.dependencies = vec!["com.squareup.retrofit2:retrofit", converter_dep, "com.squareup.okhttp3:okhttp"];
    r
}

/// The parameters before the authentication ones: the injected mapper (JSON only — XML builds its own) and the base URL.
fn leading_params(plan: &Plan) -> Vec<String> {
    let mut params = Vec::new();
    if !plan.xml() {
        params.push("ObjectMapper mapper".to_string());
    }
    params.push(base_url_param(plan));
    params
}

/// The mapper a JDK or OkHttp client reads and writes bodies with: Jackson's `XmlMapper` for XML (Retrofit uses
/// a JAXB converter instead and needs none), the injected `ObjectMapper` for JSON.
fn mapper_setup(plan: &Plan, l: &mut Lines, im: &mut Imports) {
    if !plan.xml() {
        im.add("com.fasterxml.jackson.databind.ObjectMapper");
        return;
    }
    if plan.o.library == "retrofit" {
        return;
    }
    im.add("com.fasterxml.jackson.databind.ObjectMapper");
    im.add("com.fasterxml.jackson.dataformat.xml.XmlMapper");
    l.line("ObjectMapper mapper = new XmlMapper();");
    if plan.o.xml_binding == "jaxb" {
        im.add("com.fasterxml.jackson.module.jaxb.JaxbAnnotationModule");
        l.line("mapper.registerModule(new JaxbAnnotationModule());");
    }
}
