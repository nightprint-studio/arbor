//! Request mappings — turning `@GetMapping("/{id}")` on a method inside
//! `@RequestMapping("/orders")` on a class into the one thing a person actually thinks in:
//! `GET /orders/{id}`.
//!
//! That join is the whole feature. A controller's routes are split across two annotations
//! by design, which means the URL a request hits appears **nowhere** in the source as a
//! single string — you reconstruct it in your head every time, for every handler, and the
//! reconstruction is where the mistakes are.
//!
//! Scope: `@RequestMapping` and its five shorthands, on classes annotated
//! `@Controller` / `@RestController` (or carrying a class-level `@RequestMapping`
//! themselves). A mapping on a class that is not a controller is not a route — Spring
//! would need the class to be a bean, and guessing that here would list handlers that
//! never handle anything.

use crate::beans::JavaUnit;
use crate::model::{join_paths, line_at, path_variables, Endpoint, EndpointParam};
use crate::scan::{AnnFacts, JavaFacts, TypeFacts};

/// The mapping annotations, with the HTTP method each one implies. `RequestMapping`
/// implies none — its methods are written in a `method =` element.
const MAPPINGS: &[(&str, &str)] = &[
    ("RequestMapping", ""),
    ("GetMapping", "GET"),
    ("PostMapping", "POST"),
    ("PutMapping", "PUT"),
    ("DeleteMapping", "DELETE"),
    ("PatchMapping", "PATCH"),
];

/// Every endpoint declared by the scan.
pub fn endpoints(units: &[JavaUnit]) -> Vec<Endpoint> {
    let mut out = Vec::new();
    for u in units {
        for t in &u.facts.types {
            if !is_controller(t, &u.facts) {
                continue;
            }
            let class_paths = mapping_of(t.annotations.as_slice(), &u.facts)
                .map(|(a, _)| paths_of(a))
                .unwrap_or_default();
            let class_path = class_paths.first().cloned().unwrap_or_default();

            for m in &t.methods {
                let Some((ann, implied)) = mapping_of(m.annotations.as_slice(), &u.facts) else {
                    continue;
                };
                let methods = http_methods(ann, implied);
                let produces = ann.pair("produces").unwrap_or_default().trim_matches('"').to_string();
                let mut paths = paths_of(ann);
                if paths.is_empty() {
                    // A bare `@GetMapping` maps the class path itself.
                    paths.push(String::new());
                }
                let params = params_of(m, &u.facts);
                for p in paths {
                    let path = join_paths(&class_path, &p);
                    out.push(Endpoint {
                        methods: methods.clone(),
                        path_vars: path_variables(&path),
                        path,
                        class_fqcn: t.fqcn.clone(),
                        handler: m.name.clone(),
                        file: u.facts.file.clone(),
                        offset: m.name_offset,
                        line: line_at(&u.text, m.name_offset),
                        produces: produces.clone(),
                        return_type: m.return_type.clone(),
                        params: params.clone(),
                    });
                }
            }
        }
    }
    out
}

/// Whether a type's handlers are routes: it is annotated as a controller, or it carries a
/// class-level `@RequestMapping` (the older style, where the stereotype sits on a parent).
fn is_controller(t: &TypeFacts, facts: &JavaFacts) -> bool {
    const MARKERS: &[&str] = &["Controller", "RestController", "RequestMapping"];
    t.annotations.iter().any(|a| crate::known::is_any(a, facts, MARKERS).is_some())
}

/// The mapping annotation on a declaration, with the HTTP method its name implies.
///
/// Resolved through `known`, so a project's own `@GetMapping` — which is legal, and would
/// otherwise produce a route that does not exist — declares nothing.
fn mapping_of<'a>(anns: &'a [AnnFacts], facts: &JavaFacts) -> Option<(&'a AnnFacts, &'static str)> {
    anns.iter().find_map(|a| {
        MAPPINGS
            .iter()
            .find(|(n, _)| crate::known::is(a, facts, n))
            .map(|(_, verb)| (a, *verb))
    })
}

/// The binding annotations, mapped to the short word the UI shows.
const BINDINGS: &[(&str, &str)] = &[
    ("PathVariable", "path"),
    ("RequestParam", "query"),
    ("RequestBody", "body"),
    ("RequestHeader", "header"),
    ("CookieValue", "cookie"),
    ("RequestPart", "part"),
    ("ModelAttribute", "model"),
];

/// A handler's parameters, each with where its value comes from.
///
/// An **unannotated** parameter is not an error and not a query parameter: Spring injects
/// `HttpServletRequest`, `Model`, `Principal`, `Pageable` and friends by type, and a simple type
/// with no annotation binds as a query parameter only under `-parameters` compilation. Calling
/// either one wrong in a panel would teach something false, so they are `arg` — "Spring supplies
/// this" — and left at that.
fn params_of(m: &crate::scan::MethodFacts, facts: &JavaFacts) -> Vec<EndpointParam> {
    m.params
        .iter()
        .map(|p| {
            let bound = p.annotations.iter().find_map(|a| {
                BINDINGS
                    .iter()
                    .find(|(name, _)| crate::known::is(a, facts, name))
                    .map(|(_, kind)| (a, *kind))
            });
            match bound {
                Some((ann, kind)) => EndpointParam {
                    name: p.name.clone(),
                    type_text: p.type_text.clone(),
                    binding: kind.to_string(),
                    bound_name: ann
                        .value()
                        .map(|s| s.value.clone())
                        .filter(|v| *v != p.name)
                        .unwrap_or_default(),
                    // Only an explicit `required = false` makes it optional.
                    required: ann.pair("required").map(|v| v.trim() != "false").unwrap_or(true),
                },
                None => EndpointParam {
                    name: p.name.clone(),
                    type_text: p.type_text.clone(),
                    binding: "arg".to_string(),
                    bound_name: String::new(),
                    required: true,
                },
            }
        })
        .collect()
}

/// The paths a mapping declares — `value` and `path` are aliases, and either may be an
/// array.
fn paths_of(ann: &AnnFacts) -> Vec<String> {
    let mut out: Vec<String> = ann
        .strings
        .iter()
        .filter(|s| s.element.is_empty() || s.element == "value" || s.element == "path")
        .map(|s| s.value.clone())
        .collect();
    out.retain(|p| !p.is_empty());
    out
}

/// The HTTP methods a mapping accepts: the one its name implies, else the
/// `RequestMethod.X` constants named in its `method =` element.
fn http_methods(ann: &AnnFacts, implied: &str) -> Vec<String> {
    if !implied.is_empty() {
        return vec![implied.to_string()];
    }
    let Some(raw) = ann.pair("method") else { return Vec::new() };
    // `method = RequestMethod.POST` or `method = {RequestMethod.GET, RequestMethod.PUT}`.
    raw.split(',')
        .filter_map(|p| p.rsplit('.').next())
        .map(|v| v.trim_matches(|c: char| !c.is_ascii_alphabetic()).to_ascii_uppercase())
        .filter(|v| !v.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::scan_java;

    /// The web + stereotype imports, on ONE line so line numbers a test asserts on don't
    /// move. `known` resolves each mapping annotation through them: a bare `@GetMapping`
    /// with no import is somebody's own annotation, not Spring's.
    const IMPORTS: &str =
        "import org.springframework.web.bind.annotation.*; import org.springframework.stereotype.*;";

    fn unit(src: &str) -> JavaUnit {
        let text = match src.find('\n') {
            Some(nl) if src.trim_start().starts_with("package") => {
                format!("{}{IMPORTS}{}", &src[..nl], &src[nl..])
            }
            _ => format!("{IMPORTS}\n{src}"),
        };
        JavaUnit { facts: scan_java("/p/C.java", &text).unwrap(), text }
    }

    #[test]
    fn class_and_method_mappings_join_into_one_route() {
        let e = endpoints(&[unit(
            "package p;\n@RestController @RequestMapping(\"/orders\")\nclass C {\n  @GetMapping(\"/{id}\") String get(String id) { return null; }\n}\n",
        )]);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].label(), "GET /orders/{id}");
        assert_eq!(e[0].path_vars, ["id"]);
        assert_eq!(e[0].handler, "get");
        assert_eq!(e[0].class_fqcn, "p.C");
        assert_eq!(e[0].line, 4);
    }

    #[test]
    fn a_bare_method_mapping_maps_the_class_path() {
        let e = endpoints(&[unit(
            "package p;\n@RestController @RequestMapping(\"/health\") class C { @GetMapping String ok() { return null; } }\n",
        )]);
        assert_eq!(e[0].label(), "GET /health");
    }

    #[test]
    fn request_mapping_reads_its_verbs_from_the_method_element() {
        let e = endpoints(&[unit(
            "package p;\n@Controller class C {\n  @RequestMapping(value = \"/a\", method = RequestMethod.POST) void a() {}\n  @RequestMapping(value = \"/b\", method = {RequestMethod.GET, RequestMethod.PUT}) void b() {}\n  @RequestMapping(\"/c\") void c() {}\n}\n",
        )]);
        assert_eq!(e[0].label(), "POST /a");
        assert_eq!(e[1].label(), "GET|PUT /b");
        assert_eq!(e[2].label(), "ANY /c", "no method element means every verb");
    }

    #[test]
    fn an_array_of_paths_yields_one_endpoint_each() {
        let e = endpoints(&[unit(
            "package p;\n@RestController class C { @GetMapping({\"/a\", \"/b\"}) void m() {} }\n",
        )]);
        assert_eq!(e.iter().map(|x| x.path.as_str()).collect::<Vec<_>>(), ["/a", "/b"]);
    }

    #[test]
    fn path_and_value_are_aliases() {
        let e = endpoints(&[unit(
            "package p;\n@RestController class C { @GetMapping(path = \"/x\", produces = \"application/json\") void m() {} }\n",
        )]);
        assert_eq!(e[0].path, "/x");
        assert_eq!(e[0].produces, "application/json");
    }

    #[test]
    fn a_projects_own_mapping_annotation_declares_no_route() {
        // Bypasses `unit` on purpose: the import is the whole point.
        let src = "package p;\nimport com.acme.web.GetMapping;\nimport com.acme.web.RestController;\n@RestController class C { @GetMapping(\"/x\") void m() {} }\n";
        let u = JavaUnit { facts: scan_java("/p/C.java", src).unwrap(), text: src.to_string() };
        assert!(endpoints(&[u]).is_empty());
    }

    #[test]
    fn a_class_that_is_not_a_controller_declares_no_routes() {
        let e = endpoints(&[unit("package p;\n@Service class S { @GetMapping(\"/x\") void m() {} }\n")]);
        assert!(e.is_empty(), "a mapping on a non-controller is not a route");
    }

    #[test]
    fn a_class_level_request_mapping_is_enough_to_be_a_controller() {
        let e = endpoints(&[unit(
            "package p;\n@RequestMapping(\"/legacy\") class C { @GetMapping(\"/x\") void m() {} }\n",
        )]);
        assert_eq!(e[0].path, "/legacy/x");
    }

    #[test]
    fn parameters_carry_where_their_value_comes_from() {
        let e = endpoints(&[unit(
            "package p;\n@RestController class C {\n  @PostMapping(\"/o/{id}\")\n  Order save(@PathVariable Long id, @RequestParam(value = \"q\", required = false) String query, @RequestBody Order body, HttpServletRequest req) { return null; }\n}\n",
        )]);
        let p = &e[0].params;
        assert_eq!(e[0].return_type, "Order");
        assert_eq!(
            p.iter().map(|x| x.binding.as_str()).collect::<Vec<_>>(),
            ["path", "query", "body", "arg"]
        );
        assert_eq!(p[1].effective_name(), "q", "the annotation renames it");
        assert!(!p[1].required);
        assert!(p[0].required && p[2].required);
        assert_eq!(p[3].type_text, "HttpServletRequest");
        assert_eq!(p[0].effective_name(), "id", "no rename → the parameter's own name");
    }

    #[test]
    fn a_regex_constrained_variable_names_itself() {
        let e = endpoints(&[unit(
            "package p;\n@RestController class C { @GetMapping(\"/f/{id:[0-9]+}\") void m() {} }\n",
        )]);
        assert_eq!(e[0].path_vars, ["id"]);
    }
}
