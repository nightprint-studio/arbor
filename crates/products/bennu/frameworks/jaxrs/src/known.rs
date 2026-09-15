//! Which annotations are JAX-RS's, and what their arguments say when they say it with certainty.
//!
//! `@Path` is not a reserved word, and `@Produces` is not even unambiguous inside Jakarta EE: CDI
//! has one too, on producer methods. A resource method recognised by name alone would put a CDI
//! producer in the Endpoints panel and check its "media types". So every annotation is resolved
//! through the file's imports ([`AnnotationTable`]) — `jakarta.ws.rs` and `javax.ws.rs` both count,
//! since a legacy project and a current one differ only in which they import.

use bennu_facts::prelude::{
    AnnFacts, AnnString, AnnotationTable, JavaFacts, KnownAnnotation, MethodFacts,
};

const WS: &[&str] = &["jakarta.ws.rs", "javax.ws.rs"];
const CORE: &[&str] = &["jakarta.ws.rs.core", "javax.ws.rs.core"];
const CONTAINER: &[&str] = &["jakarta.ws.rs.container", "javax.ws.rs.container"];

const ANNOTATIONS: &[KnownAnnotation] = &[
    KnownAnnotation { simple: "Path", packages: WS },
    KnownAnnotation { simple: "ApplicationPath", packages: WS },
    KnownAnnotation { simple: "HttpMethod", packages: WS },
    KnownAnnotation { simple: "GET", packages: WS },
    KnownAnnotation { simple: "POST", packages: WS },
    KnownAnnotation { simple: "PUT", packages: WS },
    KnownAnnotation { simple: "DELETE", packages: WS },
    KnownAnnotation { simple: "PATCH", packages: WS },
    KnownAnnotation { simple: "HEAD", packages: WS },
    KnownAnnotation { simple: "OPTIONS", packages: WS },
    KnownAnnotation { simple: "Produces", packages: WS },
    KnownAnnotation { simple: "Consumes", packages: WS },
    KnownAnnotation { simple: "PathParam", packages: WS },
    KnownAnnotation { simple: "QueryParam", packages: WS },
    KnownAnnotation { simple: "FormParam", packages: WS },
    KnownAnnotation { simple: "HeaderParam", packages: WS },
    KnownAnnotation { simple: "CookieParam", packages: WS },
    KnownAnnotation { simple: "MatrixParam", packages: WS },
    KnownAnnotation { simple: "BeanParam", packages: WS },
    KnownAnnotation { simple: "DefaultValue", packages: WS },
    KnownAnnotation { simple: "Encoded", packages: WS },
    KnownAnnotation { simple: "Context", packages: CORE },
    KnownAnnotation { simple: "Suspended", packages: CONTAINER },
];

pub const TABLE: AnnotationTable = AnnotationTable::new(ANNOTATIONS);

/// The request methods JAX-RS ships an annotation for.
pub const VERBS: &[&str] = &["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"];

/// The substring a file must contain to be worth a parse. Over-inclusive by design: a false hit is
/// one parse, a false miss is a resource that silently is not there.
pub const MARKERS: &[&str] = &["ws.rs"];

/// A project's own request-method annotation — an `@interface` meta-annotated `@HttpMethod("X")`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomVerb {
    pub fqcn: String,
    pub verb: String,
}

/// Where a parameter's value comes from, keyed by the annotation that says so.
const BINDINGS: &[(&str, &str)] = &[
    ("PathParam", "path"),
    ("QueryParam", "query"),
    ("FormParam", "form"),
    ("HeaderParam", "header"),
    ("CookieParam", "cookie"),
    ("MatrixParam", "matrix"),
    ("BeanParam", "bean"),
    ("Context", "context"),
    ("Suspended", "context"),
];

/// Annotations that describe or validate a value without injecting it. A parameter carrying only
/// these is still the request entity.
///
/// A list of what is known to be inert rather than of what is known to inject, because the second
/// list has no end: Jersey's `@FormDataParam`, RESTEasy's `@MultipartForm`, Dropwizard's `@Auth` all
/// supply a parameter from somewhere other than the body, and a parameter carrying any annotation
/// not listed here is simply not counted as the entity.
const INERT_ON_ENTITY: &[&str] = &[
    "Valid", "Validated", "NotNull", "NonNull", "Nonnull", "Nullable", "NotEmpty", "NotBlank",
    "Size", "Min", "Max", "Pattern", "Parameter", "Schema", "RequestBody", "ApiParam", "Deprecated",
    "SuppressWarnings",
];

pub fn is(ann: &AnnFacts, facts: &JavaFacts, simple: &str) -> bool {
    TABLE.is(ann, facts, simple)
}

pub fn find<'a>(anns: &'a [AnnFacts], facts: &JavaFacts, simple: &str) -> Option<&'a AnnFacts> {
    TABLE.find(anns, facts, simple)
}

/// Every request-method annotation a file declares for itself.
pub fn custom_verbs_in(facts: &JavaFacts, text: &str) -> Vec<CustomVerb> {
    facts
        .types
        .iter()
        .filter(|t| t.kind == "annotation")
        .filter_map(|t| {
            let http = find(&t.annotations, facts, "HttpMethod")?;
            // `@HttpMethod("PATCH")`, or `@HttpMethod(HttpMethod.PATCH)` — the constants are named
            // after the verb they hold.
            let verb = match sole_literal(http, text) {
                Some(s) => s.value.trim().to_ascii_uppercase(),
                None => http
                    .first_positional()
                    .and_then(|p| p.strip_prefix("HttpMethod."))
                    .filter(|v| VERBS.contains(v))?
                    .to_string(),
            };
            (!verb.is_empty()).then(|| CustomVerb { fqcn: t.fqcn.clone(), verb })
        })
        .collect()
}

/// The request method a method answers, with the annotation that says so.
pub fn verb_of<'a>(
    m: &'a MethodFacts,
    facts: &JavaFacts,
    custom: &[CustomVerb],
) -> Option<(String, &'a AnnFacts)> {
    m.annotations.iter().find_map(|a| {
        if let Some(v) = VERBS.iter().find(|v| is(a, facts, v)) {
            return Some(((*v).to_string(), a));
        }
        custom.iter().find(|c| is_project_annotation(a, facts, &c.fqcn)).map(|c| (c.verb.clone(), a))
    })
}

/// A parameter's binding annotation and the word the panel shows for it.
pub fn binding_of<'a>(anns: &'a [AnnFacts], facts: &JavaFacts) -> Option<(&'a AnnFacts, &'static str)> {
    anns.iter().find_map(|a| {
        BINDINGS.iter().find(|(name, _)| is(a, facts, name)).map(|(_, kind)| (a, *kind))
    })
}

/// Whether a parameter with no binding annotation is **certainly** the request entity.
pub fn is_certain_entity(anns: &[AnnFacts]) -> bool {
    anns.iter().all(|a| INERT_ON_ENTITY.contains(&a.name.as_str()))
}

/// Whether a method carries anything JAX-RS reads at method level.
///
/// The inheritance rule turns on this: when an implementation method carries ANY JAX-RS annotation,
/// the runtime ignores every one on the interface method it implements.
pub fn has_method_annotations(m: &MethodFacts, facts: &JavaFacts, custom: &[CustomVerb]) -> bool {
    const METHOD_LEVEL: &[&str] = &["Path", "Produces", "Consumes"];
    verb_of(m, facts, custom).is_some()
        || m.annotations.iter().any(|a| METHOD_LEVEL.iter().any(|n| is(a, facts, n)))
        || m.params.iter().any(|p| {
            binding_of(&p.annotations, facts).is_some()
                || find(&p.annotations, facts, "DefaultValue").is_some()
                || find(&p.annotations, facts, "Encoded").is_some()
        })
}

/// Whether `ann` is the project annotation declared as `fqcn`.
pub fn is_project_annotation(ann: &AnnFacts, facts: &JavaFacts, fqcn: &str) -> bool {
    ann.name == fqcn.rsplit('.').next().unwrap_or(fqcn) && refers_to(&ann.qualified, facts, fqcn)
}

/// Whether a type name as `written` in a file means the PROJECT type `fqcn`, resolved the way the
/// compiler would: written qualified, imported by name, imported on demand, or declared in the same
/// package.
///
/// The project-type twin of [`bennu_facts::prelude::resolves_to`], which answers the same question
/// for a framework's packages and — rightly, for those — rejects the same-package case. A nested
/// type reached by its simple name from inside its owner is not recognised: an under-report.
pub fn refers_to(written: &str, facts: &JavaFacts, fqcn: &str) -> bool {
    let written = written.trim();
    if written.contains('.') {
        return written == fqcn;
    }
    if fqcn.rsplit('.').next() != Some(written) {
        return false;
    }
    let package = fqcn.rsplit_once('.').map(|(p, _)| p).unwrap_or("");
    let suffix = format!(".{written}");
    if let Some(import) = facts.imports.iter().find(|i| i.ends_with(&suffix)) {
        return import == fqcn;
    }
    facts.package == package || facts.imports.iter().any(|i| i.strip_suffix(".*") == Some(package))
}

/// The annotation's value when it is written as exactly ONE plain string literal — `@Path("x")` or
/// `@Path(value = "x")` — and `None` for anything else.
///
/// The scan collects every literal inside an argument, so `@Path(Paths.BASE + "/{id}")` arrives
/// looking like `"/{id}"`. Reading that as the path would check a parameter against half a
/// template, which is exactly how a false "no such variable" is produced. So the source between the
/// parentheses is read back, and anything but the quoted literal — a constant, a concatenation, a
/// text block, a comment — makes the value unknown.
pub fn sole_literal<'a>(ann: &'a AnnFacts, text: &str) -> Option<&'a AnnString> {
    let [s] = ann.strings.as_slice() else { return None };
    if !(s.element.is_empty() || s.element == "value") {
        return None;
    }
    let written = text.get(ann.start..ann.end)?;
    let open = written.find('(')? + ann.start;
    let close = written.rfind(')')? + ann.start;
    let quote_before = s.start.checked_sub(1)?;
    if quote_before <= open || s.end + 1 > close {
        return None;
    }
    if text.get(quote_before..s.start)? != "\"" || text.get(s.end..s.end + 1)? != "\"" {
        return None;
    }
    let before = text.get(open + 1..quote_before)?.trim();
    let after = text.get(s.end + 1..close)?.trim();
    let named_ok =
        before.is_empty() || before.strip_prefix("value").is_some_and(|rest| rest.trim() == "=");
    (named_ok && after.is_empty()).then_some(s)
}

/// A `@Produces` / `@Consumes` argument as comparable, readable text: quotes, braces, `value =` and
/// whitespace outside the literals dropped, entries joined by `", "`.
///
/// `{"application/json"}`, `"application/json"` and `value = "application/json"` all read the same.
/// A constant stays its name (`MediaType.APPLICATION_JSON`), which is deliberately NOT equated with
/// the string it holds: two texts that differ are treated as two media types, and the check that
/// compares them under-reports rather than guessing the constant's value.
pub fn media_text(ann: &AnnFacts, text: &str) -> String {
    let Some(written) = text.get(ann.start..ann.end) else { return String::new() };
    let (Some(open), Some(close)) = (written.find('('), written.rfind(')')) else {
        return String::new();
    };
    if close <= open {
        return String::new();
    }
    let mut compact = String::new();
    let mut in_string = false;
    for c in written[open + 1..close].chars() {
        if c == '"' {
            in_string = !in_string;
        } else if in_string || !(c.is_whitespace() || c == '{' || c == '}') {
            compact.push(c);
        }
    }
    let body = compact.strip_prefix("value=").unwrap_or(&compact);
    body.split(',').map(str::trim).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_facts::prelude::scan_java;

    fn method(src: &str) -> (JavaFacts, MethodFacts) {
        let f = scan_java("/p/R.java", src).unwrap();
        let m = f.types[0].methods[0].clone();
        (f, m)
    }

    #[test]
    fn javax_and_jakarta_verbs_both_resolve_and_a_projects_own_does_not() {
        for pkg in ["javax.ws.rs", "jakarta.ws.rs"] {
            let (f, m) = method(&format!("import {pkg}.GET;\nclass R {{ @GET String a() {{ return null; }} }}"));
            assert_eq!(verb_of(&m, &f, &[]).map(|v| v.0), Some("GET".to_string()), "{pkg}");
        }
        let (f, m) = method("package p;\nimport com.acme.GET;\nclass R { @GET String a() { return null; } }");
        assert!(verb_of(&m, &f, &[]).is_none());
    }

    #[test]
    fn cdi_produces_is_not_a_media_type() {
        let (f, m) = method("import javax.enterprise.inject.Produces;\nclass R { @Produces String a() { return null; } }");
        assert!(!has_method_annotations(&m, &f, &[]));
    }

    #[test]
    fn a_project_verb_is_read_from_its_http_method_meta_annotation() {
        let decl = "package p;\nimport javax.ws.rs.HttpMethod;\n@HttpMethod(\"lock\") @interface Lock {}\n";
        let f = scan_java("/p/Lock.java", decl).unwrap();
        let custom = custom_verbs_in(&f, decl);
        assert_eq!(custom, [CustomVerb { fqcn: "p.Lock".into(), verb: "LOCK".into() }]);

        let (rf, m) = method("package p;\nclass R { @Lock void a() {} }");
        assert_eq!(verb_of(&m, &rf, &custom).map(|v| v.0), Some("LOCK".to_string()), "same package");
        let (of, om) = method("package q;\nimport com.other.Lock;\nclass R { @Lock void a() {} }");
        assert!(verb_of(&om, &of, &custom).is_none(), "somebody else's Lock");
    }

    #[test]
    fn only_a_plain_literal_is_a_value() {
        let lit = |src: &str| {
            let f = scan_java("/p/R.java", src).unwrap();
            let a = f.types[0].annotations[0].clone();
            sole_literal(&a, src).map(|s| s.value.clone())
        };
        assert_eq!(lit("@Path(\"/orders\") class R {}"), Some("/orders".into()));
        assert_eq!(lit("@Path( value = \"/orders\" ) class R {}"), Some("/orders".into()));
        assert_eq!(lit("@Path(Paths.BASE + \"/orders\") class R {}"), None, "concatenation");
        assert_eq!(lit("@Path(Paths.BASE) class R {}"), None, "constant");
        assert_eq!(lit("@Path(\"/a\" + \"/b\") class R {}"), None, "two literals");
    }

    #[test]
    fn media_text_reads_the_same_however_it_is_spelled() {
        let media = |src: &str| {
            let f = scan_java("/p/R.java", src).unwrap();
            media_text(&f.types[0].annotations[0], src)
        };
        assert_eq!(media("@Produces(\"application/json\") class R {}"), "application/json");
        assert_eq!(media("@Produces({ \"application/json\" }) class R {}"), "application/json");
        assert_eq!(media("@Produces(value = {\"a/b\", \"c/d\"}) class R {}"), "a/b, c/d");
        assert_eq!(media("@Produces(MediaType.APPLICATION_JSON) class R {}"), "MediaType.APPLICATION_JSON");
    }

    #[test]
    fn an_entity_is_a_parameter_nothing_injects() {
        let (f, m) = method(
            "import javax.ws.rs.*;\nimport javax.ws.rs.core.Context;\nclass R { @POST void a(@Valid Order o, @FormDataParam(\"f\") InputStream in, @Context UriInfo u) {} }",
        );
        assert!(binding_of(&m.params[0].annotations, &f).is_none() && is_certain_entity(&m.params[0].annotations));
        assert!(!is_certain_entity(&m.params[1].annotations), "an unknown annotation may inject it");
        assert_eq!(binding_of(&m.params[2].annotations, &f).map(|b| b.1), Some("context"));
    }
}
