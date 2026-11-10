//! Spring Security configuration: a `SecurityFilterChain` (or, before Security 5.7, a
//! `WebSecurityConfigurerAdapter`) for JWT resource servers, multi-issuer APIs, form login, HTTP Basic,
//! or an API and a web UI side by side — with the matching CORS/CSRF/method-security/user-store beans.
//!
//! The DSL follows the Spring Security version on the classpath: lambda DSL (the only one left in 7),
//! the fluent `.and()` chain (deprecated since 6.1), `antMatchers` before 5.8, `authorizeRequests`
//! before it too, `@EnableGlobalMethodSecurity` before 5.6.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::code::JavaFile;
use crate::context::{GenContext, Version};
use crate::names::java_string;
use crate::output::{parse_options, require_class_name, require_inline, require_package, GenFile, Output, SourceRoot};
use crate::registry::{Generator, GeneratorMeta, Target};

pub struct SpringSecurity;

/// What a server-rendered web app serves without logging in; offered as the `ignored_paths` default.
const STATIC_RESOURCE_PATHS: [&str; 5] = ["/css/**", "/js/**", "/images/**", "/webjars/**", "/favicon.ico"];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Rule {
    /// Empty matches every HTTP method.
    pub method: String,
    pub path: String,
    /// A `AuthorizeHttpRequests` access expression: `permitAll()`, `hasRole("ADMIN")`, …
    pub access: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Issuer {
    pub issuer_uri: String,
    pub audience: String,
    /// The claim holding roles; a dotted path (`realm_access.roles`) is walked.
    pub roles_claim: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SecurityOptions {
    pub class_name: String,
    pub package: String,
    /// `jwt` | `multi` | `form` | `webapi` | `basic`
    pub scenario: String,
    /// `auto` | `lambda` | `chained`
    pub style: String,
    pub swagger: bool,
    pub health: bool,
    pub cors: bool,
    pub cors_origins: Vec<String>,
    pub cors_credentials: bool,
    pub csrf: bool,
    /// Keep the CSRF protection but hand the token to a SPA in a cookie.
    pub csrf_cookie: bool,
    pub method_security: bool,
    /// `none` | `memory` | `jdbc` | `custom`
    pub user_store: String,
    /// `none` | `bcrypt` | `delegating`
    pub password_encoder: String,
    pub h2_console: bool,
    /// 401 with no body instead of redirecting an API client to a login page.
    pub rest_entry_point: bool,
    /// `issuer` | `jwk` | `key`
    pub jwt_source: String,
    pub issuers: Vec<Issuer>,
    pub rules: Vec<Rule>,
    /// Ant patterns kept out of the filter chain altogether (`web.ignoring()`): static resources and the like.
    /// Anything that needs headers or a security context belongs in `rules` with `permitAll()` instead.
    pub ignored_paths: Vec<String>,
    /// What `anyRequest()` gets.
    pub any_request: String,
    /// Prefix added to each role taken from the token.
    pub authority_prefix: String,
}

impl Default for Rule {
    fn default() -> Self {
        Rule { method: String::new(), path: String::new(), access: "authenticated()".into() }
    }
}

impl Default for Issuer {
    fn default() -> Self {
        Issuer { issuer_uri: String::new(), audience: String::new(), roles_claim: String::new() }
    }
}

impl Default for SecurityOptions {
    fn default() -> Self {
        SecurityOptions {
            class_name: "SecurityConfig".into(),
            package: String::new(),
            scenario: "jwt".into(),
            style: "auto".into(),
            swagger: false,
            health: true,
            cors: true,
            cors_origins: vec!["http://localhost:3000".into()],
            cors_credentials: false,
            csrf: false,
            csrf_cookie: false,
            method_security: true,
            user_store: "memory".into(),
            password_encoder: "bcrypt".into(),
            h2_console: false,
            rest_entry_point: true,
            jwt_source: "issuer".into(),
            issuers: vec![Issuer { issuer_uri: "https://sso.example.com/realms/app".into(), audience: String::new(), roles_claim: String::new() }],
            rules: vec![
                Rule { method: String::new(), path: "/public/**".into(), access: "permitAll()".into() },
                Rule { method: String::new(), path: "/admin/**".into(), access: "hasRole(\"ADMIN\")".into() },
            ],
            ignored_paths: Vec::new(),
            any_request: "authenticated()".into(),
            authority_prefix: "ROLE_".into(),
        }
    }
}

impl Generator for SpringSecurity {
    fn meta(&self) -> GeneratorMeta {
        GeneratorMeta {
            id: "spring-security",
            title: "Generate Spring Security configuration",
            description: "SecurityFilterChain with JWT, multi-issuer, form login or Basic; CORS, CSRF, method security, users and password encoder.",
            icon: "ShieldCheck",
            group: "Spring",
            language: "java",
            targets: &[Target::Java, Target::Folder, Target::Project],
            related: &["spring-security-web", "spring-boot-starter-security"],
        }
    }

    fn defaults(&self, ctx: &GenContext) -> Value {
        let mut options = SecurityOptions { package: ctx.package_for("config"), ..SecurityOptions::default() };
        // Swagger is only offered public when the project actually has it.
        options.swagger = ctx.project.has("springdoc-openapi-starter-webmvc-ui") || ctx.project.has("springdoc-openapi-ui");
        options.health = ctx.project.has("spring-boot-starter-actuator");
        options.h2_console = ctx.project.has("h2");
        options.method_security = true;
        options.csrf_cookie = false;
        if !ctx.project.has("spring-boot-starter-oauth2-resource-server") && !ctx.project.has("spring-security-oauth2-resource-server") {
            // No resource server on the classpath: a form/basic setup needs nothing else, a JWT one needs the starter.
            options.scenario = if ctx.project.resolved { "form".into() } else { "jwt".into() };
        }
        if options.scenario == "form" {
            options.cors = false;
            options.csrf = true;
            options.ignored_paths = STATIC_RESOURCE_PATHS.iter().map(|p| p.to_string()).collect();
        }
        serde_json::to_value(options).unwrap_or(Value::Null)
    }

    fn generate(&self, options: &Value, ctx: &GenContext) -> Result<Output, String> {
        let o: SecurityOptions = parse_options(options)?;
        validate(&o)?;
        if Plan::new(&o, ctx).adapter && o.scenario == "webapi" {
            return Err("Two filter chains need Spring Security 5.7 or later (SecurityFilterChain beans); this project is older.".into());
        }
        let mut plan = Plan::new(&o, ctx);
        if plan.jwt && !plan.multi && !plan.issuers[0].audience.is_empty() && o.jwt_source != "issuer" {
            plan.notes.push("The audience check reads `spring.security.oauth2.resourceserver.jwt.issuer-uri`, so that key is needed as well.".to_string());
        }
        let mut f = JavaFile::new(&o.package);
        write_class(&mut f, &o, &plan, ctx);

        let mut out = Output::file(GenFile::java(SourceRoot::Main, &o.package, &o.class_name, f.render()));
        for note in &plan.notes {
            out = out.note(note.clone());
        }
        if let Some(yaml) = properties_snippet(&o, &plan) {
            out = out.extra("application.yml", "yaml", yaml);
        }
        let mut deps = vec!["org.springframework.boot:spring-boot-starter-security"];
        if plan.jwt {
            deps.push("org.springframework.boot:spring-boot-starter-oauth2-resource-server");
        }
        if o.health {
            deps.push("org.springframework.boot:spring-boot-starter-actuator");
        }
        if o.user_store == "jdbc" {
            deps.push("org.springframework.boot:spring-boot-starter-jdbc");
        }
        Ok(out.needs(ctx.project.missing(&deps)))
    }
}

fn validate(o: &SecurityOptions) -> Result<(), String> {
    require_class_name(&o.class_name, "Class name")?;
    require_package(&o.package)?;
    if !["jwt", "multi", "form", "webapi", "basic"].contains(&o.scenario.as_str()) {
        return Err(format!("Unknown scenario `{}`", o.scenario));
    }
    for r in &o.rules {
        require_inline(&r.path, "A path")?;
        require_inline(&r.access, "An access rule")?;
    }
    for p in &o.ignored_paths {
        require_inline(p, "An ignored path")?;
    }
    require_inline(&o.any_request, "The default rule")?;
    for i in &o.issuers {
        require_inline(&i.issuer_uri, "An issuer URI")?;
        require_inline(&i.roles_claim, "A roles claim")?;
    }
    Ok(())
}

/// What the project's Spring Security version allows, decided once.
struct Plan {
    /// `WebSecurityConfigurerAdapter` (before 5.7).
    adapter: bool,
    /// `.and()` chaining.
    chained: bool,
    /// `requestMatchers` / `authorizeHttpRequests` (5.8+).
    request_matchers: bool,
    /// `@EnableMethodSecurity` (5.6+).
    new_method_security: bool,
    /// `AbstractHttpConfigurer::disable` (6.1+).
    method_ref_disable: bool,
    servlet: &'static str,
    jwt: bool,
    multi: bool,
    form: bool,
    basic: bool,
    api_chain: bool,
    stateless: bool,
    issuers: Vec<Issuer>,
    notes: Vec<String>,
}

impl Plan {
    fn new(o: &SecurityOptions, ctx: &GenContext) -> Plan {
        let sec = ctx.project.spring_security().unwrap_or(Version::new(6, 2, 0));
        let adapter = sec < Version::new(5, 7, 0);
        let mut notes = Vec::new();
        let mut chained = o.style == "chained";
        if chained && sec.at_least(6, 1) {
            chained = false;
            notes.push("The `.and()` chain is deprecated since Spring Security 6.1 and gone in 7 — the lambda DSL was written instead.".to_string());
        }
        if o.style == "auto" {
            chained = false;
        }
        if adapter {
            notes.push(format!(
                "Spring Security {}.{} predates `SecurityFilterChain` beans (5.7), so the class extends `WebSecurityConfigurerAdapter`.",
                sec.major, sec.minor
            ));
        }
        let scenario = o.scenario.as_str();
        let jwt = matches!(scenario, "jwt" | "multi" | "webapi");
        let multi = scenario == "multi";
        let mut issuers = if o.issuers.is_empty() { vec![Issuer::default()] } else { o.issuers.clone() };
        if multi && issuers.len() < 2 {
            notes.push("A multi-issuer setup needs at least two issuers — add the second one.".to_string());
        }
        if !multi {
            issuers.truncate(1);
        }
        if o.cors && o.cors_credentials && o.cors_origins.iter().any(|origin| origin == "*") {
            notes.push("`allowCredentials(true)` cannot be combined with the `*` origin — name the origins.".to_string());
        }
        Plan {
            adapter,
            chained,
            request_matchers: sec.at_least(5, 8),
            new_method_security: sec.at_least(5, 6),
            method_ref_disable: sec.at_least(6, 1),
            servlet: if ctx.project.jakarta() { "jakarta" } else { "javax" },
            jwt,
            multi,
            form: matches!(scenario, "form" | "webapi"),
            basic: scenario == "basic",
            api_chain: scenario == "webapi",
            stateless: matches!(scenario, "jwt" | "multi" | "basic"),
            issuers,
            notes,
        }
    }

    fn authorize_call(&self) -> &'static str {
        if self.request_matchers { "authorizeHttpRequests" } else { "authorizeRequests" }
    }

    fn matcher_call(&self) -> &'static str {
        if self.request_matchers { "requestMatchers" } else { "antMatchers" }
    }
}

/// One `.xxx(...)` of the chain, in the two DSL flavours.
struct Directive {
    lambda: Vec<String>,
    chained: Vec<String>,
}

fn directive(lambda: &[&str], chained: &[&str]) -> Directive {
    Directive { lambda: lambda.iter().map(|s| s.to_string()).collect(), chained: chained.iter().map(|s| s.to_string()).collect() }
}

fn write_class(f: &mut JavaFile, o: &SecurityOptions, plan: &Plan, ctx: &GenContext) {
    let im = &mut f.imports;
    im.add("org.springframework.context.annotation.Configuration");
    im.add("org.springframework.security.config.annotation.web.configuration.EnableWebSecurity");
    if plan.adapter {
        im.add("org.springframework.security.config.annotation.web.configuration.WebSecurityConfigurerAdapter");
    }
    let mut class_annotations = vec!["@Configuration".to_string(), "@EnableWebSecurity".to_string()];
    if o.method_security {
        if plan.new_method_security {
            im.add("org.springframework.security.config.annotation.method.configuration.EnableMethodSecurity");
            class_annotations.push("@EnableMethodSecurity".to_string());
        } else {
            im.add("org.springframework.security.config.annotation.method.configuration.EnableGlobalMethodSecurity");
            class_annotations.push("@EnableGlobalMethodSecurity(prePostEnabled = true)".to_string());
        }
    }
    for a in &class_annotations {
        f.body.line(a);
    }
    let header = if plan.adapter {
        format!("public class {} extends WebSecurityConfigurerAdapter", o.class_name)
    } else {
        format!("public class {}", o.class_name)
    };
    let mut body = crate::code::Lines::new();
    write_members(&mut body, &mut f.imports, o, plan, ctx);
    f.body.block(header, |l| {
        l.append(&body);
    });
}

fn write_members(l: &mut crate::code::Lines, im: &mut crate::code::Imports, o: &SecurityOptions, plan: &Plan, ctx: &GenContext) {
    // The user-visible chains.
    if plan.api_chain {
        write_chain(l, im, o, plan, Chain::Api);
        l.blank();
        write_chain(l, im, o, plan, Chain::Web);
    } else {
        write_chain(l, im, o, plan, Chain::Single);
    }

    if !ignored_patterns(o).is_empty() {
        l.gap();
        write_ignored_paths(l, im, o, plan);
    }
    if o.cors {
        l.gap();
        write_cors(l, im, o);
    }
    if plan.jwt && plan.issuers.iter().any(|i| !i.roles_claim.is_empty()) && !plan.multi {
        l.gap();
        write_single_converter(l, im, o, plan);
    }
    if plan.jwt && !plan.multi && !plan.issuers[0].audience.is_empty() {
        l.gap();
        write_audience_decoder(l, im, plan);
    }
    if plan.multi {
        l.gap();
        write_multi_issuer(l, im, o, plan);
    }
    if o.password_encoder != "none" {
        l.gap();
        write_password_encoder(l, im, o);
    }
    if plan.form || plan.basic {
        match o.user_store.as_str() {
            "memory" => {
                l.gap();
                write_memory_users(l, im, o);
            }
            "jdbc" => {
                l.gap();
                write_jdbc_users(l, im);
            }
            "custom" => {
                l.gap();
                write_custom_users(l, im, ctx);
            }
            _ => {}
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Chain {
    Single,
    Api,
    Web,
}

fn write_chain(l: &mut crate::code::Lines, im: &mut crate::code::Imports, o: &SecurityOptions, plan: &Plan, which: Chain) {
    let api = which == Chain::Api;
    let (method_name, order) = match which {
        Chain::Single => ("securityFilterChain", None),
        Chain::Api => ("apiFilterChain", Some(1)),
        Chain::Web => ("webFilterChain", Some(2)),
    };
    let mut dirs: Vec<Directive> = Vec::new();

    // Authorization first: it is what a reader looks for.
    dirs.push(authorize_directive(im, o, plan, api));

    if o.cors {
        dirs.push(directive(&[".cors(Customizer.withDefaults())"], &[".cors()"]));
        im.add("org.springframework.security.config.Customizer");
    }
    match (o.csrf && !api, o.csrf_cookie && !api) {
        (true, true) => {
            im.add("org.springframework.security.web.csrf.CookieCsrfTokenRepository");
            dirs.push(directive(
                &[".csrf(csrf -> csrf.csrfTokenRepository(CookieCsrfTokenRepository.withHttpOnlyFalse()))"],
                &[".csrf().csrfTokenRepository(CookieCsrfTokenRepository.withHttpOnlyFalse())"],
            ));
        }
        (true, false) => {}
        _ => {
            if plan.method_ref_disable {
                im.add("org.springframework.security.config.annotation.web.configurers.AbstractHttpConfigurer");
                dirs.push(directive(&[".csrf(AbstractHttpConfigurer::disable)"], &[".csrf().disable()"]));
            } else {
                dirs.push(directive(&[".csrf(csrf -> csrf.disable())"], &[".csrf().disable()"]));
            }
        }
    }
    let stateless = plan.stateless || api;
    if stateless {
        im.add("org.springframework.security.config.http.SessionCreationPolicy");
        dirs.push(directive(
            &[".sessionManagement(session -> session.sessionCreationPolicy(SessionCreationPolicy.STATELESS))"],
            &[".sessionManagement().sessionCreationPolicy(SessionCreationPolicy.STATELESS)"],
        ));
    }
    if plan.jwt && which != Chain::Web {
        if plan.multi {
            dirs.push(directive(
                &[".oauth2ResourceServer(oauth2 -> oauth2.authenticationManagerResolver(issuerResolver()))"],
                &[".oauth2ResourceServer().authenticationManagerResolver(issuerResolver())"],
            ));
        } else if plan.issuers[0].roles_claim.is_empty() {
            dirs.push(directive(&[".oauth2ResourceServer(oauth2 -> oauth2.jwt(Customizer.withDefaults()))"], &[".oauth2ResourceServer().jwt()"]));
            im.add("org.springframework.security.config.Customizer");
        } else {
            dirs.push(directive(
                &[".oauth2ResourceServer(oauth2 -> oauth2.jwt(jwt -> jwt.jwtAuthenticationConverter(jwtAuthenticationConverter())))"],
                &[".oauth2ResourceServer().jwt().jwtAuthenticationConverter(jwtAuthenticationConverter())"],
            ));
        }
    }
    if plan.basic {
        dirs.push(directive(&[".httpBasic(Customizer.withDefaults())"], &[".httpBasic()"]));
        im.add("org.springframework.security.config.Customizer");
    }
    if plan.form && !api {
        dirs.push(directive(&[".formLogin(Customizer.withDefaults())"], &[".formLogin()"]));
        dirs.push(directive(&[".logout(Customizer.withDefaults())"], &[".logout()"]));
        im.add("org.springframework.security.config.Customizer");
    }
    if o.h2_console && !api {
        dirs.push(directive(
            &[".headers(headers -> headers.frameOptions(frame -> frame.sameOrigin()))"],
            &[".headers().frameOptions().sameOrigin()"],
        ));
    }
    if o.rest_entry_point && (api || stateless) {
        im.add("org.springframework.http.HttpStatus");
        im.add("org.springframework.security.web.authentication.HttpStatusEntryPoint");
        dirs.push(directive(
            &[".exceptionHandling(ex -> ex.authenticationEntryPoint(new HttpStatusEntryPoint(HttpStatus.UNAUTHORIZED)))"],
            &[".exceptionHandling().authenticationEntryPoint(new HttpStatusEntryPoint(HttpStatus.UNAUTHORIZED))"],
        ));
    }

    if let Some(n) = order {
        im.add("org.springframework.core.annotation.Order");
        l.line(format!("@Order({n})"));
    }
    im.add("org.springframework.security.config.annotation.web.builders.HttpSecurity");
    if plan.adapter {
        write_adapter_method(l, &dirs, plan, o);
        return;
    }
    im.add("org.springframework.context.annotation.Bean");
    im.add("org.springframework.security.config.annotation.web.builders.HttpSecurity");
    im.add("org.springframework.security.web.SecurityFilterChain");
    l.line("@Bean");
    l.block(format!("public SecurityFilterChain {method_name}(HttpSecurity http) throws Exception"), |l| {
        l.line("http");
        l.indent().indent();
        if api {
            let matcher = if plan.request_matchers { "securityMatcher" } else { "antMatcher" };
            l.line(format!(".{matcher}(\"/api/**\")"));
        }
        write_directives(l, &dirs, plan.chained);
        l.dedent().dedent();
        l.line("return http.build();");
    });
}

fn write_adapter_method(l: &mut crate::code::Lines, dirs: &[Directive], plan: &Plan, _o: &SecurityOptions) {
    l.line("@Override");
    l.block("protected void configure(HttpSecurity http) throws Exception", |l| {
        l.line("http");
        l.indent().indent();
        write_directives(l, dirs, plan.chained);
        l.dedent().dedent();
    });
}

fn write_directives(l: &mut crate::code::Lines, dirs: &[Directive], chained: bool) {
    let last = dirs.len().saturating_sub(1);
    for (i, d) in dirs.iter().enumerate() {
        let lines = if chained { &d.chained } else { &d.lambda };
        let count = lines.len();
        for (j, line) in lines.iter().enumerate() {
            let mut text = line.clone();
            let is_last_line = j + 1 == count;
            if is_last_line && i == last {
                text.push(';');
            } else if is_last_line && chained {
                // `.and()` closes a configurer before the next one starts.
                l.line(&text);
                l.line(".and()");
                continue;
            }
            l.line(text);
        }
    }
}

fn authorize_directive(im: &mut crate::code::Imports, o: &SecurityOptions, plan: &Plan, api: bool) -> Directive {
    let m = plan.matcher_call();
    let mut rules: Vec<String> = Vec::new();
    if o.swagger {
        rules.push(format!(".{m}(\"/v3/api-docs/**\", \"/swagger-ui/**\", \"/swagger-ui.html\").permitAll()"));
    }
    if o.health {
        im.add("org.springframework.boot.actuate.autoconfigure.security.servlet.EndpointRequest");
        im.add("org.springframework.boot.actuate.health.HealthEndpoint");
        rules.push(".requestMatchers(EndpointRequest.to(HealthEndpoint.class)).permitAll()".to_string());
    }
    if o.h2_console && !api {
        rules.push(format!(".{m}(\"/h2-console/**\").permitAll()"));
    }
    if plan.form && !api {
        rules.push(format!(".{m}(\"/login\", \"/error\").permitAll()"));
    }
    for r in o.rules.iter().filter(|r| !r.path.trim().is_empty()) {
        let verb = if r.method.trim().is_empty() {
            String::new()
        } else {
            im.add("org.springframework.http.HttpMethod");
            format!("HttpMethod.{}, ", r.method.trim().to_uppercase())
        };
        rules.push(format!(".{m}({verb}{}).{}", java_string(r.path.trim()), r.access.trim()));
    }
    let any = if api { "authenticated()" } else { o.any_request.trim() };
    let authorize = plan.authorize_call();

    let mut lambda = vec![format!(".{authorize}(auth -> auth")];
    for r in &rules {
        lambda.push(format!("    {r}"));
    }
    lambda.push(format!("    .anyRequest().{any})"));

    let mut chained = vec![format!(".{authorize}()")];
    for r in &rules {
        chained.push(format!("    {r}"));
    }
    chained.push(format!("    .anyRequest().{any}"));
    Directive { lambda, chained }
}

/// Trimmed, non-empty, de-duplicated ignored patterns, in the order given.
fn ignored_patterns(o: &SecurityOptions) -> Vec<&str> {
    let mut seen: Vec<&str> = Vec::new();
    for p in o.ignored_paths.iter().map(|p| p.trim()).filter(|p| !p.is_empty()) {
        if !seen.contains(&p) {
            seen.push(p);
        }
    }
    seen
}

/// `web.ignoring()`: the request never reaches the filter chain (no security headers, no context).
fn write_ignored_paths(l: &mut crate::code::Lines, im: &mut crate::code::Imports, o: &SecurityOptions, plan: &Plan) {
    // `antMatchers` is the only form before 5.8, and the adapter overrides `configure(WebSecurity)`.
    let matchers = ignored_patterns(o).into_iter().map(java_string).collect::<Vec<_>>().join(", ");
    let call = plan.matcher_call();
    l.line("// Skips the filter chain entirely; use a `permitAll()` rule for anything that needs security headers.");
    if plan.adapter {
        im.add("org.springframework.security.config.annotation.web.builders.WebSecurity");
        l.line("@Override");
        l.block("public void configure(WebSecurity web)", |l| {
            l.line(format!("web.ignoring().{call}({matchers});"));
        });
        return;
    }
    im.add("org.springframework.context.annotation.Bean");
    im.add("org.springframework.security.config.annotation.web.configuration.WebSecurityCustomizer");
    l.line("@Bean");
    l.block("public WebSecurityCustomizer webSecurityCustomizer()", |l| {
        l.line(format!("return web -> web.ignoring().{call}({matchers});"));
    });
}

fn write_cors(l: &mut crate::code::Lines, im: &mut crate::code::Imports, o: &SecurityOptions) {
    im.add("org.springframework.context.annotation.Bean");
    im.add("org.springframework.web.cors.CorsConfiguration");
    im.add("org.springframework.web.cors.CorsConfigurationSource");
    im.add("org.springframework.web.cors.UrlBasedCorsConfigurationSource");
    im.add("java.util.List");
    let origins: Vec<String> = o.cors_origins.iter().filter(|s| !s.trim().is_empty()).map(|s| java_string(s.trim())).collect();
    let origins = if origins.is_empty() { "\"*\"".to_string() } else { origins.join(", ") };
    l.line("@Bean");
    l.block("public CorsConfigurationSource corsConfigurationSource()", |l| {
        l.line("CorsConfiguration config = new CorsConfiguration();");
        l.line(format!("config.setAllowedOrigins(List.of({origins}));"));
        l.line("config.setAllowedMethods(List.of(\"GET\", \"POST\", \"PUT\", \"PATCH\", \"DELETE\", \"OPTIONS\"));");
        l.line("config.setAllowedHeaders(List.of(\"Authorization\", \"Content-Type\"));");
        if o.cors_credentials {
            l.line("config.setAllowCredentials(true);");
        }
        l.line("UrlBasedCorsConfigurationSource source = new UrlBasedCorsConfigurationSource();");
        l.line("source.registerCorsConfiguration(\"/**\", config);");
        l.line("return source;");
    });
}

fn write_single_converter(l: &mut crate::code::Lines, im: &mut crate::code::Imports, o: &SecurityOptions, plan: &Plan) {
    let claim = plan.issuers[0].roles_claim.clone();
    im.add("org.springframework.security.oauth2.server.resource.authentication.JwtAuthenticationConverter");
    if claim.contains('.') {
        write_claim_path_converter(l, im, o);
        l.gap();
        l.block("private JwtAuthenticationConverter jwtAuthenticationConverter()", |l| {
            l.line("JwtAuthenticationConverter converter = new JwtAuthenticationConverter();");
            l.line(format!("converter.setJwtGrantedAuthoritiesConverter(rolesFrom({}));", java_string(&claim)));
            l.line("return converter;");
        });
        return;
    }
    im.add("org.springframework.security.oauth2.server.resource.authentication.JwtGrantedAuthoritiesConverter");
    l.block("private JwtAuthenticationConverter jwtAuthenticationConverter()", |l| {
        l.line("JwtGrantedAuthoritiesConverter authorities = new JwtGrantedAuthoritiesConverter();");
        l.line(format!("authorities.setAuthoritiesClaimName({});", java_string(&claim)));
        l.line(format!("authorities.setAuthorityPrefix({});", java_string(&o.authority_prefix)));
        l.line("JwtAuthenticationConverter converter = new JwtAuthenticationConverter();");
        l.line("converter.setJwtGrantedAuthoritiesConverter(authorities);");
        l.line("return converter;");
    });
}

/// A converter that walks a dotted claim path (`realm_access.roles`), which `JwtGrantedAuthoritiesConverter` cannot.
fn write_claim_path_converter(l: &mut crate::code::Lines, im: &mut crate::code::Imports, o: &SecurityOptions) {
    im.add("org.springframework.core.convert.converter.Converter");
    im.add("org.springframework.security.oauth2.jwt.Jwt");
    im.add("org.springframework.security.core.GrantedAuthority");
    im.add("org.springframework.security.core.authority.SimpleGrantedAuthority");
    im.add("java.util.Collection");
    im.add("java.util.List");
    im.add("java.util.Map");
    l.line("// Walks a dotted claim path such as realm_access.roles.");
    l.block("private static Converter<Jwt, Collection<GrantedAuthority>> rolesFrom(String path)", |l| {
        l.block_closing("return jwt ->", "};", |l| {
            l.line("Object node = jwt.getClaims();");
            l.block("for (String key : path.split(\"\\\\.\"))", |l| {
                l.line("node = node instanceof Map<?, ?> map ? map.get(key) : null;");
            });
            l.line("Collection<?> roles = node instanceof Collection<?> c ? c : List.of();");
            l.line(format!(
                "return roles.stream().<GrantedAuthority>map(r -> new SimpleGrantedAuthority({} + r)).toList();",
                java_string(&o.authority_prefix)
            ));
        });
    });
}

fn write_audience_decoder(l: &mut crate::code::Lines, im: &mut crate::code::Imports, plan: &Plan) {
    im.add("org.springframework.context.annotation.Bean");
    im.add("org.springframework.beans.factory.annotation.Value");
    im.add("org.springframework.security.oauth2.core.DelegatingOAuth2TokenValidator");
    im.add("org.springframework.security.oauth2.jwt.JwtClaimValidator");
    im.add("org.springframework.security.oauth2.jwt.JwtDecoder");
    im.add("org.springframework.security.oauth2.jwt.JwtDecoders");
    im.add("org.springframework.security.oauth2.jwt.JwtValidators");
    im.add("org.springframework.security.oauth2.jwt.NimbusJwtDecoder");
    im.add("java.util.List");
    let audience = java_string(&plan.issuers[0].audience);
    l.line("@Bean");
    l.block("public JwtDecoder jwtDecoder(@Value(\"${spring.security.oauth2.resourceserver.jwt.issuer-uri}\") String issuerUri)", |l| {
        l.line("NimbusJwtDecoder decoder = JwtDecoders.fromIssuerLocation(issuerUri);");
        l.line("decoder.setJwtValidator(new DelegatingOAuth2TokenValidator<>(");
        l.line("        JwtValidators.createDefaultWithIssuer(issuerUri),");
        l.line(format!("        new JwtClaimValidator<List<String>>(\"aud\", aud -> aud != null && aud.contains({audience}))));"));
        l.line("return decoder;");
    });
}

fn write_multi_issuer(l: &mut crate::code::Lines, im: &mut crate::code::Imports, o: &SecurityOptions, plan: &Plan) {
    im.add("org.springframework.context.annotation.Bean");
    im.add("org.springframework.security.authentication.AuthenticationManager");
    im.add("org.springframework.security.authentication.AuthenticationManagerResolver");
    im.add("org.springframework.security.oauth2.core.DelegatingOAuth2TokenValidator");
    im.add("org.springframework.security.oauth2.jwt.JwtClaimValidator");
    im.add("org.springframework.security.oauth2.jwt.JwtDecoders");
    im.add("org.springframework.security.oauth2.jwt.JwtValidators");
    im.add("org.springframework.security.oauth2.jwt.NimbusJwtDecoder");
    im.add("org.springframework.security.oauth2.server.resource.authentication.JwtAuthenticationProvider");
    im.add("org.springframework.security.oauth2.server.resource.authentication.JwtIssuerAuthenticationManagerResolver");
    im.add(format!("{}.servlet.http.HttpServletRequest", plan.servlet));
    im.add("java.util.List");
    im.add("java.util.Map");

    l.line("// The list is fixed: a token whose `iss` is not named here is rejected.");
    l.line("@Bean");
    l.block("public AuthenticationManagerResolver<HttpServletRequest> issuerResolver()", |l| {
        l.line("Map<String, AuthenticationManager> trusted = Map.of(");
        l.indent().indent();
        let count = plan.issuers.len();
        for (i, issuer) in plan.issuers.iter().enumerate() {
            let sep = if i + 1 == count { ");" } else { "," };
            l.line(format!(
                "{}, authenticationManager({}, {}, {}){sep}",
                java_string(&issuer.issuer_uri),
                java_string(&issuer.issuer_uri),
                java_string(&issuer.audience),
                java_string(&issuer.roles_claim)
            ));
        }
        l.dedent().dedent();
        l.line("return new JwtIssuerAuthenticationManagerResolver(trusted::get);");
    });
    l.gap();
    l.block("private static AuthenticationManager authenticationManager(String issuer, String audience, String rolesClaim)", |l| {
        l.line("NimbusJwtDecoder decoder = JwtDecoders.fromIssuerLocation(issuer);");
        l.line("decoder.setJwtValidator(audience.isEmpty()");
        l.line("        ? JwtValidators.createDefaultWithIssuer(issuer)");
        l.line("        : new DelegatingOAuth2TokenValidator<>(");
        l.line("                JwtValidators.createDefaultWithIssuer(issuer),");
        l.line("                new JwtClaimValidator<List<String>>(\"aud\", aud -> aud != null && aud.contains(audience))));");
        l.line("JwtAuthenticationProvider provider = new JwtAuthenticationProvider(decoder);");
        l.line("provider.setJwtAuthenticationConverter(converterFor(rolesClaim));");
        l.line("return provider::authenticate;");
    });
    l.gap();
    // The walker serves every issuer; an empty claim falls back to the default `scope` mapping.
    im.add("org.springframework.security.oauth2.server.resource.authentication.JwtAuthenticationConverter");
    l.block("private static JwtAuthenticationConverter converterFor(String rolesClaim)", |l| {
        l.line("JwtAuthenticationConverter converter = new JwtAuthenticationConverter();");
        l.block("if (!rolesClaim.isEmpty())", |l| {
            l.line("converter.setJwtGrantedAuthoritiesConverter(rolesFrom(rolesClaim));");
        });
        l.line("return converter;");
    });
    l.gap();
    write_claim_path_converter(l, im, o);
}

fn write_password_encoder(l: &mut crate::code::Lines, im: &mut crate::code::Imports, o: &SecurityOptions) {
    im.add("org.springframework.context.annotation.Bean");
    im.add("org.springframework.security.crypto.password.PasswordEncoder");
    l.line("@Bean");
    if o.password_encoder == "delegating" {
        im.add("org.springframework.security.crypto.factory.PasswordEncoderFactories");
        l.block("public PasswordEncoder passwordEncoder()", |l| {
            l.line("return PasswordEncoderFactories.createDelegatingPasswordEncoder();");
        });
    } else {
        im.add("org.springframework.security.crypto.bcrypt.BCryptPasswordEncoder");
        l.block("public PasswordEncoder passwordEncoder()", |l| {
            l.line("return new BCryptPasswordEncoder();");
        });
    }
}

fn write_memory_users(l: &mut crate::code::Lines, im: &mut crate::code::Imports, o: &SecurityOptions) {
    im.add("org.springframework.context.annotation.Bean");
    im.add("org.springframework.security.core.userdetails.User");
    im.add("org.springframework.security.core.userdetails.UserDetailsService");
    im.add("org.springframework.security.provisioning.InMemoryUserDetailsManager");
    l.line("// Development only: replace with a real user store.");
    l.line("@Bean");
    if o.password_encoder == "none" {
        l.block("public UserDetailsService userDetailsService()", |l| {
            l.line("return new InMemoryUserDetailsManager(");
            l.line("        User.withUsername(\"admin\").password(\"{noop}change-me\").roles(\"ADMIN\").build());");
        });
    } else {
        im.add("org.springframework.security.crypto.password.PasswordEncoder");
        l.block("public UserDetailsService userDetailsService(PasswordEncoder encoder)", |l| {
            l.line("return new InMemoryUserDetailsManager(");
            l.line("        User.withUsername(\"admin\").password(encoder.encode(\"change-me\")).roles(\"ADMIN\").build());");
        });
    }
}

fn write_jdbc_users(l: &mut crate::code::Lines, im: &mut crate::code::Imports) {
    im.add("org.springframework.context.annotation.Bean");
    im.add("org.springframework.security.core.userdetails.UserDetailsService");
    im.add("org.springframework.security.provisioning.JdbcUserDetailsManager");
    im.add("javax.sql.DataSource");
    l.line("// Reads the standard `users` / `authorities` tables.");
    l.line("@Bean");
    l.block("public UserDetailsService userDetailsService(DataSource dataSource)", |l| {
        l.line("return new JdbcUserDetailsManager(dataSource);");
    });
}

fn write_custom_users(l: &mut crate::code::Lines, im: &mut crate::code::Imports, ctx: &GenContext) {
    im.add("org.springframework.context.annotation.Bean");
    im.add("org.springframework.security.core.userdetails.User");
    im.add("org.springframework.security.core.userdetails.UserDetailsService");
    im.add("org.springframework.security.core.userdetails.UsernameNotFoundException");
    let _ = ctx;
    l.line("@Bean");
    l.block("public UserDetailsService userDetailsService(/* UserRepository users */)", |l| {
        l.block_closing("return username ->", "};", |l| {
            l.line("// TODO: load the user from your repository");
            l.line("throw new UsernameNotFoundException(username);");
        });
    });
}

/// The `application.yml` keys the generated class relies on.
fn properties_snippet(o: &SecurityOptions, plan: &Plan) -> Option<String> {
    if !plan.jwt || plan.multi {
        return None;
    }
    let issuer = &plan.issuers[0].issuer_uri;
    let mut text = String::from("spring:\n  security:\n    oauth2:\n      resourceserver:\n        jwt:\n");
    match o.jwt_source.as_str() {
        "jwk" => text.push_str(&format!("          jwk-set-uri: {issuer}/protocol/openid-connect/certs\n")),
        "key" => text.push_str("          public-key-location: classpath:app.pub\n"),
        _ => text.push_str(&format!("          issuer-uri: {issuer}\n")),
    }
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

    fn run(options: serde_json::Value, ctx: &GenContext) -> Output {
        SpringSecurity.generate(&options, ctx).expect("generates")
    }

    #[test]
    fn jwt_on_security_6_uses_the_lambda_dsl_and_request_matchers() {
        let out = run(serde_json::json!({ "package": "com.acme.config" }), &sample_context());
        let text = &out.files[0].text;
        assert!(text.starts_with("package com.acme.config;"), "{text}");
        assert!(text.contains("@EnableMethodSecurity"), "{text}");
        assert!(text.contains(".authorizeHttpRequests(auth -> auth"), "{text}");
        assert!(text.contains(".requestMatchers(\"/public/**\").permitAll()"), "{text}");
        assert!(text.contains(".oauth2ResourceServer(oauth2 -> oauth2.jwt(Customizer.withDefaults()))"), "{text}");
        assert!(text.contains(".csrf(AbstractHttpConfigurer::disable)"), "{text}");
        assert!(text.contains("return http.build();"), "{text}");
        assert!(out.extras[0].text.contains("issuer-uri: https://sso.example.com/realms/app"));
    }

    #[test]
    fn the_chain_ends_with_exactly_one_semicolon() {
        let out = run(serde_json::json!({}), &sample_context());
        let text = &out.files[0].text;
        let chain = text.split("http\n").nth(1).unwrap().split("return http.build();").next().unwrap();
        assert_eq!(chain.matches(';').count(), 1, "{chain}");
    }

    #[test]
    fn security_5_6_extends_the_adapter_and_uses_ant_matchers() {
        let mut ctx = sample_context();
        ctx.project = ProjectInfoBuilder::legacy_56();
        let out = run(serde_json::json!({ "scenario": "basic", "health": false }), &ctx);
        let text = &out.files[0].text;
        assert!(text.contains("extends WebSecurityConfigurerAdapter"), "{text}");
        assert!(text.contains("protected void configure(HttpSecurity http) throws Exception"), "{text}");
        assert!(text.contains(".antMatchers(\"/public/**\").permitAll()"), "{text}");
        assert!(text.contains(".authorizeRequests(auth -> auth"), "{text}");
        assert!(text.contains("@EnableGlobalMethodSecurity(prePostEnabled = true)") || text.contains("@EnableMethodSecurity"), "{text}");
        assert!(!text.contains("return http.build();"), "{text}");
    }

    #[test]
    fn the_chained_style_is_refused_from_6_1_with_a_note() {
        let out = run(serde_json::json!({ "style": "chained" }), &sample_context());
        assert!(!out.files[0].text.contains(".and()"));
        assert!(out.notes.iter().any(|n| n.contains(".and()")));
    }

    #[test]
    fn the_chained_style_writes_and_between_configurers_on_5_8() {
        let mut ctx = sample_context();
        ctx.project = ProjectInfoBuilder::security(5, 8);
        let out = run(serde_json::json!({ "style": "chained", "scenario": "basic" }), &ctx);
        let text = &out.files[0].text;
        assert!(text.contains(".authorizeHttpRequests()"), "{text}");
        assert!(text.contains(".and()"), "{text}");
        assert!(text.contains(".httpBasic()"), "{text}");
    }

    #[test]
    fn multi_issuer_builds_a_resolver_with_a_claim_path_walker() {
        let out = run(
            serde_json::json!({
                "scenario": "multi",
                "issuers": [
                    { "issuer_uri": "https://a/realms/x", "audience": "api", "roles_claim": "realm_access.roles" },
                    { "issuer_uri": "https://b/", "audience": "", "roles_claim": "roles" }
                ]
            }),
            &sample_context(),
        );
        let text = &out.files[0].text;
        assert!(text.contains("issuerResolver()"), "{text}");
        assert!(text.contains("\"https://a/realms/x\", authenticationManager(\"https://a/realms/x\", \"api\", \"realm_access.roles\"),"), "{text}");
        assert!(text.contains("import jakarta.servlet.http.HttpServletRequest;"), "{text}");
        assert!(out.extras.is_empty());
    }

    #[test]
    fn javax_projects_get_javax_servlet() {
        let mut ctx = sample_context();
        ctx.project = legacy_project();
        let out = run(serde_json::json!({ "scenario": "multi" }), &ctx);
        assert!(out.files[0].text.contains("import javax.servlet.http.HttpServletRequest;"));
    }

    #[test]
    fn webapi_writes_two_ordered_chains() {
        let out = run(serde_json::json!({ "scenario": "webapi" }), &sample_context());
        let text = &out.files[0].text;
        assert!(text.contains("@Order(1)"), "{text}");
        assert!(text.contains("apiFilterChain"), "{text}");
        assert!(text.contains(".securityMatcher(\"/api/**\")"), "{text}");
        assert!(text.contains("@Order(2)"), "{text}");
        assert!(text.contains(".formLogin(Customizer.withDefaults())"), "{text}");
    }

    #[test]
    fn rules_with_a_method_import_http_method_and_bad_input_is_refused() {
        let out = run(
            serde_json::json!({ "rules": [{ "method": "post", "path": "/api/x", "access": "hasRole(\"ADMIN\")" }] }),
            &sample_context(),
        );
        assert!(out.files[0].text.contains(".requestMatchers(HttpMethod.POST, \"/api/x\").hasRole(\"ADMIN\")"));
        let bad = SpringSecurity.generate(&serde_json::json!({ "rules": [{ "path": "/x", "access": "permitAll(); evil()" }] }), &sample_context());
        assert!(bad.is_err());
    }

    #[test]
    fn missing_starters_are_reported_only_for_a_resolved_classpath() {
        let mut ctx = sample_context();
        ctx.project = ProjectInfoBuilder::security(6, 2);
        let out = run(serde_json::json!({ "scenario": "jwt" }), &ctx);
        assert!(out.dependencies.iter().any(|d| d.contains("oauth2-resource-server")));
    }

    #[test]
    fn the_audience_adds_a_decoder_bean() {
        let out = run(
            serde_json::json!({ "issuers": [{ "issuer_uri": "https://i/", "audience": "shop-api", "roles_claim": "" }] }),
            &sample_context(),
        );
        assert!(out.files[0].text.contains("public JwtDecoder jwtDecoder("), "{}", out.files[0].text);
    }

    #[test]
    fn no_ignored_paths_writes_no_customizer() {
        let text = &run(serde_json::json!({}), &sample_context()).files[0].text;
        assert!(!text.contains("WebSecurityCustomizer"), "{text}");
        assert!(!text.contains("ignoring()"), "{text}");
    }

    #[test]
    fn one_ignored_path_writes_a_customizer_bean() {
        let text = &run(serde_json::json!({ "ignored_paths": ["/static/**"] }), &sample_context()).files[0].text;
        assert!(text.contains("public WebSecurityCustomizer webSecurityCustomizer()"), "{text}");
        assert!(text.contains("return web -> web.ignoring().requestMatchers(\"/static/**\");"), "{text}");
        assert!(text.contains("import org.springframework.security.config.annotation.web.configuration.WebSecurityCustomizer;"), "{text}");
    }

    #[test]
    fn several_ignored_paths_are_trimmed_deduplicated_and_blank_ones_dropped() {
        let text = &run(
            serde_json::json!({ "ignored_paths": [" /css/** ", "", "/js/**", "/css/**", "   "] }),
            &sample_context(),
        )
        .files[0]
            .text;
        assert!(text.contains("web.ignoring().requestMatchers(\"/css/**\", \"/js/**\");"), "{text}");
    }

    #[test]
    fn ignored_paths_are_escaped_as_java_strings_and_inline_only() {
        let text = &run(serde_json::json!({ "ignored_paths": ["/a\"b", "/c\\d"] }), &sample_context()).files[0].text;
        assert!(text.contains("requestMatchers(\"/a\\\"b\", \"/c\\\\d\")"), "{text}");
        let bad = SpringSecurity.generate(&serde_json::json!({ "ignored_paths": ["/x\n/y"] }), &sample_context());
        assert!(bad.is_err());
    }

    #[test]
    fn the_adapter_ignores_paths_in_configure_web_security_with_ant_matchers() {
        let mut ctx = sample_context();
        ctx.project = ProjectInfoBuilder::legacy_56();
        let text = &run(serde_json::json!({ "scenario": "basic", "ignored_paths": ["/img/**"] }), &ctx).files[0].text;
        assert!(text.contains("public void configure(WebSecurity web)"), "{text}");
        assert!(text.contains("web.ignoring().antMatchers(\"/img/**\");"), "{text}");
        assert!(!text.contains("WebSecurityCustomizer"), "{text}");
    }

    /// Project fixtures with a chosen Spring Security version.
    struct ProjectInfoBuilder;
    impl ProjectInfoBuilder {
        fn security(major: u32, minor: u32) -> crate::context::ProjectInfo {
            let version = format!("{major}.{minor}.0");
            crate::context::ProjectInfo::new(
                17,
                &[
                    ("org.springframework.boot:spring-boot", "2.7.18"),
                    ("org.springframework.security:spring-security-web", version.as_str()),
                    ("org.springframework.boot:spring-boot-starter-web", "2.7.18"),
                ],
            )
        }
        fn legacy_56() -> crate::context::ProjectInfo {
            Self::security(5, 6)
        }
    }
}
