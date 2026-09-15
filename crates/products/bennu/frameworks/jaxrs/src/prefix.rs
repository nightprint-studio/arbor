//! The application — the part of every route written nowhere near a resource, and the list of
//! resources it actually deploys.
//!
//! Two places declare the path. `@ApplicationPath("api")` on the `Application` subclass is the
//! modern one; a `web.xml` `<servlet-mapping>` of the JAX-RS servlet to `/api/*` is the one a legacy
//! WAR has. Both are read, the annotation first, and anything that does not settle to one value
//! leaves the routes unprefixed and says so — a panel full of confidently wrong URLs is worse than
//! one that admits it does not know the first segment.
//!
//! The deployed set matters for one reason: two resource methods only clash when both are
//! deployed. An `Application` that lists its classes by hand, or a Jersey `ResourceConfig` that
//! scans two packages, deploys less than the source tree holds.

use std::collections::BTreeSet;

use bennu_ext::prelude::ScannedFile;
use bennu_facts::prelude::TypeFacts;

use crate::known;
use crate::model::{AppPrefix, Src};
use crate::paths::simple_type;

const JERSEY_SERVLETS: &[&str] = &[
    "org.glassfish.jersey.servlet.ServletContainer",
    "com.sun.jersey.spi.container.servlet.ServletContainer",
];
const RESTEASY_SERVLET: &str = "org.jboss.resteasy.plugins.server.servlet.HttpServletDispatcher";

/// The supertypes that make a class a JAX-RS application. `ResourceConfig` is Jersey's, and it is
/// what most Jersey applications actually extend.
const APPLICATION_TYPES: &[&str] = &["Application", "ResourceConfig"];

/// Which resource classes the applications deploy, as far as the source says.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Registration {
    /// Every resource class found — the runtime scans.
    #[default]
    Scanned,
    /// Only the classes under these packages (Jersey's `packages(…)`, and its init-param).
    Packages(Vec<String>),
    /// A hand-written list, or a registration this crate cannot read: nothing is known about any
    /// one class, so nothing is claimed about two of them clashing.
    Explicit,
}

impl Registration {
    /// Whether a resource class is deployed. Meaningless for [`Registration::Explicit`], which the
    /// caller checks first.
    pub fn deploys(&self, fqcn: &str) -> bool {
        match self {
            Registration::Packages(packages) => {
                packages.iter().any(|p| fqcn.starts_with(p.as_str()) && fqcn[p.len()..].starts_with('.'))
            }
            Registration::Scanned | Registration::Explicit => true,
        }
    }

    fn combine(self, other: Registration) -> Registration {
        match (self, other) {
            (Registration::Explicit, _) | (_, Registration::Explicit) => Registration::Explicit,
            (Registration::Packages(mut a), Registration::Packages(b)) => {
                a.extend(b);
                Registration::Packages(a)
            }
            (Registration::Packages(p), Registration::Scanned)
            | (Registration::Scanned, Registration::Packages(p)) => Registration::Packages(p),
            (Registration::Scanned, Registration::Scanned) => Registration::Scanned,
        }
    }
}

/// What the project says about its applications.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Applications {
    pub prefix: AppPrefix,
    /// How many applications appear to be deployed. More than one means two routes are not
    /// necessarily in the same application, and a clash between them is not a finding.
    pub count: usize,
    pub registration: Registration,
}

/// Read the applications out of the parsed sources and the project's `web.xml` files.
pub fn applications(units: &[Src<'_>], xml: &[ScannedFile]) -> Applications {
    let mut classes: Vec<String> = Vec::new();
    // `None` for an `@ApplicationPath` whose value is not a literal.
    let mut declared: Vec<Option<String>> = Vec::new();
    let mut registration = Registration::Scanned;
    for u in units {
        for t in u.facts.types.iter().filter(|t| t.kind == "class") {
            let annotated = known::find(&t.annotations, u.facts, "ApplicationPath");
            // Over-counted on purpose: this number only ever silences a check.
            if annotated.is_none() && !APPLICATION_TYPES.contains(&simple_type(&t.extends)) {
                continue;
            }
            classes.push(t.fqcn.clone());
            registration = registration.combine(class_registration(t, u.text));
            if let Some(a) = annotated {
                declared.push(known::sole_literal(a, u.text).map(|s| normalise(&s.value)));
            }
        }
    }

    let servlets = web_xml_servlets(xml, &classes);
    let count = classes.len().max(servlets.names);
    let prefix = if !declared.is_empty() {
        settle(declared)
    } else if !servlets.prefixes.is_empty() {
        settle(servlets.prefixes)
    } else {
        AppPrefix::None
    };
    Applications { prefix, count, registration: registration.combine(servlets.registration) }
}

/// What an `Application` subclass registers.
fn class_registration(t: &TypeFacts, text: &str) -> Registration {
    if t.methods.iter().any(|m| matches!(m.name.as_str(), "getClasses" | "getSingletons")) {
        return Registration::Explicit;
    }
    if simple_type(&t.extends) != "ResourceConfig" {
        return Registration::Scanned;
    }
    // Read from the file's text rather than a model of the constructor: a `register(…)` anywhere
    // in it is enough to stop claiming anything, and a `packages(…)` whose arguments are not all
    // literals is too.
    if ["register(", "registerClasses(", "registerInstances("].iter().any(|c| text.contains(c)) {
        return Registration::Explicit;
    }
    match scanned_packages(text) {
        Some(packages) if !packages.is_empty() => Registration::Packages(packages),
        _ => Registration::Explicit,
    }
}

/// The literal packages of every `packages(…)` call in `text`. `None` when a call names none.
fn scanned_packages(text: &str) -> Option<Vec<String>> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("packages(") {
        let args = &rest[at + "packages(".len()..];
        let end = args.find(')').unwrap_or(args.len());
        let literals: Vec<String> = args[..end]
            .split('"')
            .skip(1)
            .step_by(2)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if literals.is_empty() {
            return None;
        }
        out.extend(literals);
        rest = &args[end..];
    }
    Some(out)
}

/// One distinct readable value is the prefix; anything else is unknown.
fn settle(values: Vec<Option<String>>) -> AppPrefix {
    if values.iter().any(Option::is_none) {
        return AppPrefix::Unknown;
    }
    let distinct: BTreeSet<String> = values.into_iter().flatten().collect();
    match distinct.len() {
        1 => AppPrefix::Known(distinct.into_iter().next().unwrap_or_default()),
        _ => AppPrefix::Unknown,
    }
}

/// `/api/*` and `api` and `/api/` are all the prefix `api`; `/*` is the context root.
fn normalise(value: &str) -> String {
    let v = value.trim();
    let v = v.strip_suffix("/*").unwrap_or(v);
    v.trim_matches('/').to_string()
}

#[derive(Debug, Default)]
struct Servlets {
    /// JAX-RS servlets declared.
    names: usize,
    /// One per mapped url-pattern; `None` for a pattern that is not a `/x/*` prefix.
    prefixes: Vec<Option<String>>,
    registration: Registration,
}

fn web_xml_servlets(xml: &[ScannedFile], applications: &[String]) -> Servlets {
    let mut out = Servlets::default();
    for file in xml.iter().filter(|f| is_web_xml(f)) {
        let text = strip_comments(&file.text);
        let mut names = Vec::new();
        for servlet in blocks(&text, "servlet") {
            let Some(name) = child_text(servlet, "servlet-name") else { continue };
            let class = child_text(servlet, "servlet-class").unwrap_or_default();
            let Some(registration) = servlet_registration(&name, &class, servlet, &text, applications) else {
                continue;
            };
            out.registration = std::mem::take(&mut out.registration).combine(registration);
            names.push(name);
        }
        out.names += names.len();
        for mapping in blocks(&text, "servlet-mapping") {
            let Some(name) = child_text(mapping, "servlet-name") else { continue };
            if !names.contains(&name) {
                continue;
            }
            for pattern in children_text(mapping, "url-pattern") {
                out.prefixes.push(pattern.ends_with("/*").then(|| normalise(&pattern)));
            }
        }
    }
    out
}

fn is_web_xml(file: &ScannedFile) -> bool {
    let path = file.path.to_string_lossy();
    path.rsplit(['/', '\\']).next().is_some_and(|n| n.eq_ignore_ascii_case("web.xml"))
}

/// What a servlet deploys — `None` when it is not a JAX-RS servlet at all.
///
/// A Jersey servlet is JAX-RS's by its class; a servlet with no class NAMED after an `Application`
/// subclass is the Servlet 3 pluggability form, and that class's own registration already counted.
fn servlet_registration(
    name: &str,
    class: &str,
    servlet: &str,
    web_xml: &str,
    applications: &[String],
) -> Option<Registration> {
    if applications.iter().any(|a| a == name) || name.ends_with("ws.rs.core.Application") {
        return Some(Registration::Scanned);
    }
    if class == RESTEASY_SERVLET {
        let listed = web_xml.contains("resteasy.resources");
        return Some(if listed { Registration::Explicit } else { Registration::Scanned });
    }
    if !JERSEY_SERVLETS.contains(&class) {
        return None;
    }
    let mut registration = Registration::Explicit;
    for param in blocks(servlet, "init-param") {
        let key = child_text(param, "param-name").unwrap_or_default();
        let value = child_text(param, "param-value").unwrap_or_default();
        match key.as_str() {
            "jersey.config.server.provider.packages" | "com.sun.jersey.config.property.packages" => {
                let packages: Vec<String> = value
                    .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
                    .filter(|p| !p.is_empty())
                    .map(str::to_string)
                    .collect();
                if !packages.is_empty() {
                    registration = Registration::Packages(packages);
                }
            }
            // The application class decides, and its own registration was read from its source.
            "javax.ws.rs.Application" | "jakarta.ws.rs.Application" => {
                registration = Registration::Scanned;
            }
            _ => {}
        }
    }
    Some(registration)
}

fn strip_comments(xml: &str) -> String {
    let mut out = String::with_capacity(xml.len());
    let mut rest = xml;
    while let Some(open) = rest.find("<!--") {
        out.push_str(&rest[..open]);
        match rest[open..].find("-->") {
            Some(close) => rest = &rest[open + close + 3..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// The contents of every `<tag>…</tag>`. The character after the name must end it, or `<servlet`
/// would match `<servlet-mapping>`.
fn blocks<'a>(xml: &'a str, tag: &str) -> Vec<&'a str> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(found) = xml[from..].find(&open) {
        let at = from + found + open.len();
        let ends_name = xml[at..].chars().next().is_some_and(|c| c == '>' || c.is_whitespace());
        if !ends_name {
            from = at;
            continue;
        }
        let Some(gt) = xml[at..].find('>') else { break };
        let body_start = at + gt + 1;
        let Some(end) = xml[body_start..].find(&close) else { break };
        out.push(&xml[body_start..body_start + end]);
        from = body_start + end + close.len();
    }
    out
}

fn children_text(block: &str, tag: &str) -> Vec<String> {
    blocks(block, tag).into_iter().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect()
}

fn child_text(block: &str, tag: &str) -> Option<String> {
    children_text(block, tag).into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Unit;
    use bennu_facts::prelude::scan_java;
    use std::path::PathBuf;

    fn web_xml(body: &str) -> ScannedFile {
        ScannedFile {
            path: PathBuf::from("/p/src/main/webapp/WEB-INF/web.xml"),
            text: format!("<web-app>{body}</web-app>"),
        }
    }

    fn read(sources: &[String], xml: &[ScannedFile]) -> Applications {
        let units: Vec<Unit> = sources
            .iter()
            .map(|s| Unit { facts: scan_java("/p/src/main/java/App.java", s).unwrap(), text: s.clone() })
            .collect();
        let srcs: Vec<Src<'_>> = units.iter().map(Unit::src).collect();
        applications(&srcs, xml)
    }

    fn app(annotation: &str, body: &str) -> String {
        format!(
            "package com.acme;\nimport javax.ws.rs.ApplicationPath;\nimport javax.ws.rs.core.Application;\n{annotation} class App extends Application {{ {body} }}"
        )
    }

    const JERSEY: &str = "<servlet><servlet-name>jersey</servlet-name><servlet-class>org.glassfish.jersey.servlet.ServletContainer</servlet-class>\
        <init-param><param-name>jersey.config.server.provider.packages</param-name><param-value>com.acme.api</param-value></init-param></servlet>";

    #[test]
    fn one_application_path_is_the_prefix() {
        let a = read(&[app("@ApplicationPath(\"/api/\")", "")], &[]);
        assert_eq!(a.prefix, AppPrefix::Known("api".into()));
        assert_eq!(a.count, 1);
        assert_eq!(a.registration, Registration::Scanned);
    }

    #[test]
    fn two_different_application_paths_are_unknown() {
        let a = read(&[app("@ApplicationPath(\"api\")", ""), app("@ApplicationPath(\"admin\")", "")], &[]);
        assert_eq!(a.prefix, AppPrefix::Unknown);
        assert_eq!(a.count, 2);
    }

    #[test]
    fn a_non_literal_application_path_is_unknown() {
        assert_eq!(read(&[app("@ApplicationPath(Paths.API)", "")], &[]).prefix, AppPrefix::Unknown);
    }

    #[test]
    fn a_jersey_servlet_mapping_is_the_prefix_when_nothing_is_annotated() {
        let xml = web_xml(&format!(
            "<!-- <servlet-mapping><servlet-name>jersey</servlet-name><url-pattern>/old/*</url-pattern></servlet-mapping> -->\
             {JERSEY}\
             <servlet><servlet-name>other</servlet-name><servlet-class>com.acme.Upload</servlet-class></servlet>\
             <servlet-mapping><servlet-name>other</servlet-name><url-pattern>/upload/*</url-pattern></servlet-mapping>\
             <servlet-mapping>\n  <servlet-name>jersey</servlet-name>\n  <url-pattern>/rest/*</url-pattern>\n</servlet-mapping>"
        ));
        let a = read(&[], &[xml]);
        assert_eq!(a.prefix, AppPrefix::Known("rest".into()), "the commented-out and the foreign mapping are ignored");
        assert_eq!(a.count, 1);
        assert_eq!(a.registration, Registration::Packages(vec!["com.acme.api".into()]));
        assert!(a.registration.deploys("com.acme.api.v1.Orders"));
        assert!(!a.registration.deploys("com.acme.apix.Orders"), "a package, not a string prefix");
    }

    #[test]
    fn a_servlet_named_after_the_application_class_counts() {
        let xml = web_xml(
            "<servlet><servlet-name>com.acme.App</servlet-name></servlet>\
             <servlet-mapping><servlet-name>com.acme.App</servlet-name><url-pattern>/api/*</url-pattern></servlet-mapping>",
        );
        let a = read(&[app("", "")], &[xml]);
        assert_eq!(a.prefix, AppPrefix::Known("api".into()));
        assert_eq!(a.count, 1);
    }

    #[test]
    fn an_annotation_beats_the_descriptor_and_nothing_means_no_prefix() {
        let xml = web_xml(&format!(
            "{JERSEY}<servlet-mapping><servlet-name>jersey</servlet-name><url-pattern>/rest/*</url-pattern></servlet-mapping>"
        ));
        assert_eq!(read(&[app("@ApplicationPath(\"api\")", "")], &[xml]).prefix, AppPrefix::Known("api".into()));
        assert_eq!(read(&[], &[]).prefix, AppPrefix::None);
    }

    #[test]
    fn an_exact_url_pattern_is_not_a_prefix() {
        let xml = web_xml(&format!(
            "{JERSEY}<servlet-mapping><servlet-name>jersey</servlet-name><url-pattern>/api</url-pattern></servlet-mapping>"
        ));
        assert_eq!(read(&[], &[xml]).prefix, AppPrefix::Unknown);
    }

    #[test]
    fn a_hand_written_class_list_deploys_nothing_this_crate_can_name() {
        let a = read(&[app("@ApplicationPath(\"api\")", "public Set<Class<?>> getClasses() { return null; }")], &[]);
        assert_eq!(a.registration, Registration::Explicit);
    }

    #[test]
    fn a_resource_config_is_read_for_its_packages_or_given_up_on() {
        let config = |body: &str| {
            format!("package com.acme;\nimport javax.ws.rs.ApplicationPath;\n@ApplicationPath(\"api\") class Cfg extends ResourceConfig {{ Cfg() {{ {body} }} }}")
        };
        let scanned = read(&[config("packages(\"com.acme.web\", \"com.acme.admin\");")], &[]);
        assert_eq!(scanned.registration, Registration::Packages(vec!["com.acme.web".into(), "com.acme.admin".into()]));
        assert_eq!(read(&[config("register(Orders.class);")], &[]).registration, Registration::Explicit);
        assert_eq!(read(&[config("packages(Pkg.API);")], &[]).registration, Registration::Explicit);
    }
}
