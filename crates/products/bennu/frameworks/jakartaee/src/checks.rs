//! Every diagnostic this extension raises — each one a deployment the container refuses.
//!
//! Positions come from the live buffer; the answers from the model with the buffer laid over it
//! ([`Buffer`]). Each check lists its reasons for silence before its reason to speak.

use std::collections::HashSet;

use bennu_proto::prelude::{severity, Diagnostic};

use crate::beans::BeanOrigin;
use crate::inject::{InjectKind, InjectionPoint};
use crate::known;
use crate::matching::{candidates, resolution, Fit, Resolution};
use crate::model::{Buffer, Model};
use crate::text::simple_name;
use crate::types::TypeView;
use crate::web::{annotated_components, clashes, parse_web_xml, pattern_problem, WebComponent, WebKind};

pub const CODE_UNSATISFIED: &str = "jakartaee.unsatisfied-injection";
pub const CODE_AMBIGUOUS: &str = "jakartaee.ambiguous-injection";
pub const CODE_FINAL_FIELD: &str = "jakartaee.final-inject-field";
pub const CODE_STATIC_FIELD: &str = "jakartaee.static-inject-field";
pub const CODE_UNPROXYABLE: &str = "jakartaee.unproxyable-bean";
pub const CODE_INVALID_EJB: &str = "jakartaee.invalid-ejb-class";
pub const CODE_NOT_A_SERVLET: &str = "jakartaee.servlet-not-a-servlet";
pub const CODE_INVALID_URL: &str = "jakartaee.invalid-url-pattern";
pub const CODE_DUPLICATE_URL: &str = "jakartaee.duplicate-url-pattern";

/// Interfaces of the JDK a servlet may implement besides `Servlet` — known not to be servlets.
const HARMLESS: &[&str] = &["Object", "Runnable", "Cloneable", "Comparable", "AutoCloseable", "Iterable"];

fn diag(message: String, sev: &str, code: &str, start: usize, end: usize) -> Diagnostic {
    Diagnostic { message, severity: sev.to_string(), code: code.to_string(), start, end }
}

/// Every diagnostic for a Java buffer. `spring`: Spring answers injection in this project.
pub fn java_diagnostics(buffer: &Buffer<'_>, spring: bool) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    if !spring {
        field_modifier_checks(buffer, &mut out);
        resolution_checks(buffer, &mut out);
    }
    class_checks(buffer, &mut out);
    servlet_checks(buffer, &mut out);
    out
}

/// `@Inject` on a `final` or `static` field — the container refuses the bean outright.
fn field_modifier_checks(buffer: &Buffer<'_>, out: &mut Vec<Diagnostic>) {
    for p in buffer.own_points.iter().filter(|p| p.kind == InjectKind::Field) {
        if p.is_final {
            out.push(diag(
                format!("`{}` is final — a container cannot inject a final field, and refuses the bean", p.member),
                severity::ERROR,
                CODE_FINAL_FIELD,
                p.marker_start,
                p.marker_end,
            ));
        }
        if p.is_static {
            out.push(diag(
                format!("`{}` is static — injection into a static field is not supported, and the container refuses it", p.member),
                severity::ERROR,
                CODE_STATIC_FIELD,
                p.marker_start,
                p.marker_end,
            ));
        }
    }
}

/// Unsatisfied and ambiguous injection points of container-managed classes.
fn resolution_checks(buffer: &Buffer<'_>, out: &mut Vec<Diagnostic>) {
    let gates = buffer.model.gates();
    for p in &buffer.own_points {
        if !managed(buffer, &p.owner) {
            continue;
        }
        let (start, end) = (p.offset, p.offset + p.member.len());
        match resolution(&buffer.view, &buffer.beans, p, gates) {
            Resolution::Unsatisfied => out.push(diag(
                format!(
                    "no bean in this project is assignable to `{}`{} — the deployment fails with an unsatisfied dependency",
                    p.target.simple(),
                    wanted(p)
                ),
                severity::WARNING,
                CODE_UNSATISFIED,
                start,
                end,
            )),
            Resolution::Ambiguous(n) => {
                let names: Vec<String> = candidates(&buffer.view, &buffer.beans, p, gates.mode)
                    .into_iter()
                    .filter(|c| c.fit == Fit::Yes)
                    .take(3)
                    .map(|c| c.bean.label())
                    .collect();
                out.push(diag(
                    format!(
                        "{n} beans match `{}`{} ({}) — the deployment fails with an ambiguous dependency; add a qualifier or make all but one @Alternative",
                        p.target.simple(),
                        wanted(p),
                        names.join(", ")
                    ),
                    severity::WARNING,
                    CODE_AMBIGUOUS,
                    start,
                    end,
                ));
            }
            Resolution::Satisfied | Resolution::Unknown => {}
        }
    }
}

/// What a point asks for beyond its type, in words.
fn wanted(p: &InjectionPoint) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(n) = &p.quals.named {
        parts.push(format!("@Named(\"{n}\")"));
    }
    parts.extend(p.quals.custom.iter().map(|c| format!("@{}", simple_name(&c.fqcn))));
    if parts.is_empty() { String::new() } else { format!(" with {}", parts.join(" ")) }
}

/// Whether the container manages `owner` — only then is an injection point it cannot satisfy fatal.
fn managed(buffer: &Buffer<'_>, owner: &str) -> bool {
    let facts = &buffer.unit.facts;
    buffer.beans.iter().any(|b| b.origin == BeanOrigin::Class && b.id == owner && (b.explicit || b.maybe_explicit))
        || facts.types.iter().any(|t| {
            t.fqcn == owner && ["WebServlet", "WebFilter", "WebListener"].iter().any(|n| known::has(&t.annotations, facts, n))
        })
        || buffer.model.web.iter().any(|c| c.class == owner)
}

/// Proxyability of normal-scoped beans, and the EJB class rules.
fn class_checks(buffer: &Buffer<'_>, out: &mut Vec<Diagnostic>) {
    let facts = &buffer.unit.facts;
    for t in &facts.types {
        let Some(row) = buffer.view.get(&t.fqcn) else { continue };
        let name_span = (t.name_offset, t.name_offset + t.name.len());
        let normal = buffer
            .beans
            .iter()
            .find(|b| b.origin == BeanOrigin::Class && b.file == buffer.path && b.id == t.fqcn && b.normal_scoped);
        if let Some(bean) = normal {
            let scope = bean.badge;
            if row.is_final {
                out.push(diag(
                    format!("`{}` is {scope} and final — the container reaches it through a client proxy, which cannot subclass a final class", t.name),
                    severity::ERROR,
                    CODE_UNPROXYABLE,
                    name_span.0,
                    name_span.1,
                ));
            }
            for m in &row.final_methods {
                out.push(diag(
                    format!("`{}` is final in a {scope} bean — the client proxy cannot override it, and the container refuses the bean", m.name),
                    severity::ERROR,
                    CODE_UNPROXYABLE,
                    m.offset,
                    m.offset + m.name.len(),
                ));
            }
            // Quarkus relaxes this one, and Lombok may be writing the constructor nobody can see.
            let generated_ctor = t.annotations.iter().any(|a| a.name.ends_with("ArgsConstructor"));
            let no_arg = row.ctors.is_empty() || row.ctors.iter().any(|c| c.params == 0 && !c.private);
            if !no_arg && !buffer.model.quarkus && !generated_ctor {
                out.push(diag(
                    format!("`{}` is {scope} but has no non-private constructor without parameters — the client proxy cannot be created", t.name),
                    severity::ERROR,
                    CODE_UNPROXYABLE,
                    name_span.0,
                    name_span.1,
                ));
            }
        }
        if let Some(kind) = known::ejb_kind(&t.annotations, facts) {
            let why = if t.kind != "class" {
                Some(format!("a {}", t.kind))
            } else if row.is_abstract {
                Some("abstract".to_string())
            } else if row.is_final {
                Some("final".to_string())
            } else {
                None
            };
            if let (Some(why), Some(ann)) = (why, known::find(&t.annotations, facts, kind)) {
                out.push(diag(
                    format!("@{kind} on `{}`, which is {why} — an enterprise bean must be a concrete, non-final class", t.name),
                    severity::ERROR,
                    CODE_INVALID_EJB,
                    ann.start,
                    ann.end,
                ));
            }
        }
    }
}

/// `@WebServlet` on something that is not a servlet, bad URL patterns, and patterns two servlets claim.
fn servlet_checks(buffer: &Buffer<'_>, out: &mut Vec<Diagnostic>) {
    let own = annotated_components(&buffer.unit);
    if own.is_empty() {
        return;
    }
    for c in own.iter().filter(|c| c.kind == WebKind::Servlet) {
        if is_servlet(&buffer.view, &c.class) == Some(false) {
            out.push(diag(
                format!("`{}` is annotated @WebServlet but is not a servlet — extend HttpServlet; the container refuses to deploy it", simple_name(&c.class)),
                severity::ERROR,
                CODE_NOT_A_SERVLET,
                c.marker_start,
                c.marker_end,
            ));
        }
    }
    pattern_checks(&own, out);
    let others = buffer.model.web.iter().filter(|c| c.file != buffer.path).cloned();
    clash_checks(own, others, &buffer.model.metadata_complete, out);
}

/// Whether a class is a servlet: `Some(false)` only when its whole hierarchy was read and none of it is.
fn is_servlet(view: &TypeView<'_>, fqcn: &str) -> Option<bool> {
    let row = view.get(fqcn)?;
    if row.kind != "class" {
        return None;
    }
    let ancestry = view.ancestry(fqcn);
    if ancestry.open {
        return None;
    }
    let servlet_base = |n: &str| {
        matches!(simple_name(n), "HttpServlet" | "GenericServlet" | "Servlet")
            && (!n.contains('.') || n.starts_with("javax.servlet") || n.starts_with("jakarta.servlet"))
    };
    if ancestry.library.iter().any(|n| servlet_base(n.as_str())) {
        return Some(true);
    }
    let harmless = |n: &str| n.starts_with("java.") || (!n.contains('.') && HARMLESS.contains(&n));
    ancestry.library.iter().all(|n| harmless(n.as_str())).then_some(false)
}

fn pattern_checks(components: &[WebComponent], out: &mut Vec<Diagnostic>) {
    for c in components {
        for p in &c.patterns {
            if let Some(why) = pattern_problem(&p.value) {
                let (start, end) = if p.start == p.end { (p.start.saturating_sub(1), p.end + 1) } else { (p.start, p.end) };
                out.push(diag(
                    format!("`{}` is not a valid URL pattern: {why} — the container refuses to start", p.value),
                    severity::ERROR,
                    CODE_INVALID_URL,
                    start,
                    end,
                ));
            }
        }
    }
}

/// Clashes involving `own`, the buffer's components, against the rest of the project's.
fn clash_checks(
    own: Vec<WebComponent>,
    others: impl Iterator<Item = WebComponent>,
    metadata_complete: &HashSet<String>,
    out: &mut Vec<Diagnostic>,
) {
    let own_count = own.len();
    let mut all = own;
    all.extend(others);
    for clash in clashes(&all, metadata_complete) {
        if clash.component >= own_count {
            continue;
        }
        let p = &all[clash.component].patterns[clash.pattern];
        out.push(diag(
            format!("`{}` is also mapped to the servlet `{}` — two servlets on one pattern stop the application from starting", p.value, clash.other),
            severity::ERROR,
            CODE_DUPLICATE_URL,
            p.start,
            p.end,
        ));
    }
}

/// Diagnostics for a `web.xml` buffer: its patterns, and the servlets they collide with.
pub fn web_xml_diagnostics(model: &Model, path: &str, source: &str) -> Vec<Diagnostic> {
    let parsed = parse_web_xml(path, source);
    let mut complete = model.metadata_complete.clone();
    complete.remove(&parsed.module);
    if parsed.metadata_complete {
        complete.insert(parsed.module.clone());
    }
    let mut out = Vec::new();
    pattern_checks(&parsed.components, &mut out);
    let others = model.web.iter().filter(|c| c.file != path).cloned();
    clash_checks(parsed.components, others, &complete, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::Project;

    const USE: &str = "/p/a/Use.java";

    fn java(p: &Project, path: &str, spring: bool) -> Vec<Diagnostic> {
        let buffer = p.model.buffer(path, p.text(path)).expect("parses");
        java_diagnostics(&buffer, spring)
    }

    fn codes(d: &[Diagnostic]) -> Vec<&str> {
        d.iter().map(|x| x.code.as_str()).collect()
    }

    fn unsatisfied_project(extra: &[(&str, &str)], xml: &[(&str, &str)]) -> Project {
        let mut files = vec![
            ("/p/a/Svc.java", "package a;\npublic interface Svc {}\n"),
            ("/p/a/Impl.java", "package a;\npublic class Impl implements Svc {}\n"),
            (USE, "package a;\n@RequestScoped public class Use { @Inject Svc svc; }\n"),
        ];
        files.extend_from_slice(extra);
        Project::cdi_with_xml(&files, xml)
    }

    #[test]
    fn an_implementation_without_a_bean_defining_annotation_leaves_the_point_unsatisfied() {
        let p = unsatisfied_project(&[], &[]);
        let d = java(&p, USE, false);
        assert_eq!(codes(&d), [CODE_UNSATISFIED]);
        assert_eq!(&p.text(USE)[d[0].start..d[0].end], "svc");
    }

    #[test]
    fn unsatisfied_goes_quiet_for_every_reason_it_should() {
        assert!(java(&unsatisfied_project(&[], &[]), USE, true).is_empty(), "Spring answers @Inject here");
        let all = [("/p/src/main/resources/META-INF/beans.xml", "<beans bean-discovery-mode=\"all\"/>")];
        assert!(java(&unsatisfied_project(&[], &all), USE, false).is_empty(), "in an `all` archive Impl is a bean");
        let ext = [("/p/a/Ext.java", "package a;\nimport jakarta.enterprise.inject.spi.Extension;\npublic class Ext implements Extension {}\n")];
        assert!(java(&unsatisfied_project(&ext, &[]), USE, false).is_empty(), "a portable extension may add the bean");
        let quarkus = [("/p/pom.xml", "<project><dependency><groupId>io.quarkus</groupId></dependency></project>")];
        assert!(java(&unsatisfied_project(&[], &quarkus), USE, false).is_empty());
        let view_scoped = [("/p/a/Impl.java", "package a;\nimport jakarta.faces.view.ViewScoped;\n@ViewScoped public class Impl implements Svc {}\n")];
        let p = Project::cdi(&[("/p/a/Svc.java", "package a;\npublic interface Svc {}\n"), view_scoped[0], (USE, "package a;\n@RequestScoped public class Use { @Inject Svc svc; }\n")]);
        assert!(java(&p, USE, false).is_empty(), "a scope this crate does not catalogue may be bean-defining");
    }

    #[test]
    fn generic_library_and_built_in_types_are_never_judged() {
        let p = Project::cdi(&[
            ("/p/a/Repo.java", "package a;\npublic interface Repo<T> {}\n"),
            (USE, "package a;\nimport javax.persistence.EntityManager;\n@RequestScoped public class Use { @Inject Repo<String> repo; @Inject EntityManager em; @Inject Instance<Repo> all; }\n"),
        ]);
        assert!(java(&p, USE, false).is_empty());
    }

    #[test]
    fn a_jax_rs_produces_is_no_producer_and_javax_is_read_like_jakarta() {
        let p = Project::new(&[
            ("/p/a/Svc.java", "package a;\npublic interface Svc {}\n"),
            ("/p/a/Res.java", "package a;\nimport javax.ws.rs.Produces;\npublic class Res { @Produces(\"text/plain\") public Svc make() { return null; } }\n"),
            (USE, "package a;\nimport javax.enterprise.context.RequestScoped;\nimport javax.inject.Inject;\n@RequestScoped public class Use { @Inject Svc svc; }\n"),
        ]);
        assert_eq!(codes(&java(&p, USE, false)), [CODE_UNSATISFIED]);
    }

    #[test]
    fn two_enabled_beans_are_ambiguous_unless_an_alternative_is_in_play() {
        let files = |b: &'static str| {
            Project::cdi(&[
                ("/p/a/Svc.java", "package a;\npublic interface Svc {}\n"),
                ("/p/a/A.java", "package a;\n@ApplicationScoped public class A implements Svc {}\n"),
                ("/p/a/B.java", b),
                (USE, "package a;\n@RequestScoped public class Use { @Inject Svc svc; }\n"),
            ])
        };
        let d = java(&files("package a;\n@ApplicationScoped public class B implements Svc {}\n"), USE, false);
        assert_eq!(codes(&d), [CODE_AMBIGUOUS]);
        assert!(d[0].message.contains("A, B"), "{}", d[0].message);
        let quiet = java(&files("package a;\n@Alternative @ApplicationScoped public class B implements Svc {}\n"), USE, false);
        assert!(quiet.is_empty());
    }

    #[test]
    fn final_and_static_inject_fields_are_errors_unless_spring_answers() {
        let p = Project::cdi(&[
            ("/p/a/Svc.java", "package a;\n@ApplicationScoped public class Svc {}\n"),
            (USE, "package a;\n@RequestScoped public class Use { @Inject final Svc a = null; @Inject static Svc b; }\n"),
        ]);
        let d = java(&p, USE, false);
        assert!(codes(&d).contains(&CODE_FINAL_FIELD) && codes(&d).contains(&CODE_STATIC_FIELD), "{d:?}");
        assert!(java(&p, USE, true).is_empty());
    }

    #[test]
    fn an_unproxyable_normal_scoped_bean_is_an_error() {
        let path = "/p/a/S.java";
        let p = Project::cdi(&[(path, "package a;\n@ApplicationScoped public final class S {\n  public final void go() {}\n  private final void ok() {}\n  S(int x) {}\n}\n")]);
        let d = java(&p, path, false);
        assert_eq!(codes(&d), [CODE_UNPROXYABLE, CODE_UNPROXYABLE, CODE_UNPROXYABLE]);
        let dependent = Project::cdi(&[(path, "package a;\n@Dependent public final class S { S(int x) {} }\n")]);
        assert!(java(&dependent, path, false).is_empty(), "a pseudo-scope has no proxy");
    }

    #[test]
    fn quarkus_relaxes_only_the_constructor_rule() {
        let path = "/p/a/S.java";
        let pom = [("/p/pom.xml", "<project><groupId>io.quarkus</groupId></project>")];
        let p = Project::cdi_with_xml(&[(path, "package a;\n@ApplicationScoped public class S { S(int x) {} }\n")], &pom);
        assert!(java(&p, path, false).is_empty());
        let p = Project::cdi_with_xml(&[(path, "package a;\n@ApplicationScoped public final class S {}\n")], &pom);
        assert_eq!(codes(&java(&p, path, false)), [CODE_UNPROXYABLE]);
    }

    #[test]
    fn an_abstract_session_bean_is_an_error_and_the_inject_singleton_is_not_one() {
        let path = "/p/a/E.java";
        let p = Project::new(&[(path, "package a;\nimport jakarta.ejb.Stateless;\n@Stateless public abstract class E {}\n")]);
        assert_eq!(codes(&java(&p, path, false)), [CODE_INVALID_EJB]);
        let p = Project::new(&[(path, "package a;\nimport javax.inject.Singleton;\n@Singleton public abstract class E {}\n")]);
        assert!(java(&p, path, false).is_empty());
    }

    #[test]
    fn a_web_servlet_must_be_a_servlet_when_the_hierarchy_says_so() {
        let path = "/p/a/S.java";
        let bad = Project::cdi(&[(path, "package a;\n@WebServlet(\"/s\") public class S {}\n")]);
        assert_eq!(codes(&java(&bad, path, false)), [CODE_NOT_A_SERVLET]);
        let good = Project::cdi(&[(path, "package a;\nimport jakarta.servlet.http.HttpServlet;\n@WebServlet(\"/s\") public class S extends HttpServlet {}\n")]);
        assert!(java(&good, path, false).is_empty());
        let unknown = Project::cdi(&[(path, "package a;\nimport org.springframework.web.servlet.FrameworkServlet;\n@WebServlet(\"/s\") public class S extends FrameworkServlet {}\n")]);
        assert!(java(&unknown, path, false).is_empty(), "a library base may well be a servlet");
    }

    #[test]
    fn bad_and_duplicate_url_patterns_are_errors() {
        let path = "/p/web/src/main/java/a/S.java";
        let xml = [(
            "/p/web/src/main/webapp/WEB-INF/web.xml",
            "<web-app><servlet-mapping><servlet-name>legacy</servlet-name><url-pattern>/api/*</url-pattern></servlet-mapping></web-app>",
        )];
        let p = Project::cdi_with_xml(
            &[(path, "package a;\nimport jakarta.servlet.http.HttpServlet;\n@WebServlet({\"api\", \"/api/*\"}) public class S extends HttpServlet {}\n")],
            &xml,
        );
        let d = java(&p, path, false);
        assert_eq!(codes(&d), [CODE_INVALID_URL, CODE_DUPLICATE_URL]);
        assert_eq!(&p.text(path)[d[1].start..d[1].end], "/api/*");
        let xml_side = web_xml_diagnostics(&p.model, xml[0].0, xml[0].1);
        assert_eq!(codes(&xml_side), [CODE_DUPLICATE_URL]);
    }
}
