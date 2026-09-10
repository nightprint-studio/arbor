//! Checking the endpoints against each other, and against their own signatures.
//!
//! ## Two questions, and only one of them fits in a file
//!
//! *"Does this handler's `@PathVariable` match its path?"* is answerable from the method. *"Does
//! any other controller already claim this route?"* is not — the other controller is in another
//! file, in another package, quite possibly written by somebody else three years ago. Both are
//! answered here, from the endpoint model, because both are statements about the same list.
//!
//! ## Why an ambiguous mapping is worth its own check
//!
//! Spring refuses to start:
//!
//! > *Ambiguous mapping. Cannot map 'ordiniController' method … to {GET /ordini/{id}}: There is
//! > already 'legacyController' bean method … mapped.*
//!
//! It is a startup failure, so nobody ships it — but it is found by **running the application**,
//! which on a legacy reactor is a build plus a deploy plus a wait, and the message names two
//! classes without saying which one is the new one. Seeing it on the line you just wrote is a
//! different experience entirely.
//!
//! ## And why a path variable is worth more than it looks
//!
//! `@PathVariable("id")` on a method whose path has no `{id}` is not a startup failure. It is a
//! **500 on every single call** to that endpoint, with `MissingPathVariableException`, discovered
//! by the first person to use the feature. The compiler cannot see it, no test that mocks the
//! controller sees it, and the two halves — the template and the annotation — sit fifteen
//! characters apart on the screen, which is exactly the distance at which a typo survives review.

use std::collections::HashMap;

use crate::model::Endpoint;

pub const CODE_AMBIGUOUS: &str = "spring.endpoint.ambiguous";
pub const CODE_UNMATCHED_VARIABLE: &str = "spring.endpoint.unmatched-path-variable";
pub const CODE_UNBOUND_VARIABLE: &str = "spring.endpoint.unbound-path-variable";
pub const CODE_DUPLICATE_VARIABLE: &str = "spring.endpoint.duplicate-path-variable";

/// One thing wrong with an endpoint, ready to become a diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointIssue {
    pub code: &'static str,
    pub message: String,
    pub severity: &'static str,
    /// The file it is in, forward-slashed — the caller filters to the buffer in front of it.
    pub file: String,
    /// Byte span of the handler method's **name** — every issue here is about the handler as a
    /// whole (its route, its signature), and the name is the one part of it that is always
    /// present and always short enough to underline.
    pub start: usize,
    pub end: usize,
}

/// The span of a handler's name.
fn span(endpoint: &Endpoint) -> (usize, usize) {
    (endpoint.offset, endpoint.offset + endpoint.handler.len())
}

/// Every problem the endpoint list has, in one pass.
///
/// Takes the whole list rather than one endpoint because the most valuable check needs it: whether
/// a route is claimed twice cannot be answered from the route.
pub fn issues(endpoints: &[Endpoint]) -> Vec<EndpointIssue> {
    let mut out = ambiguous(endpoints);
    for endpoint in endpoints {
        out.extend(path_variables(endpoint));
    }
    out
}

/// Routes claimed by more than one handler.
///
/// A route is `(method, path)`, and an empty method list means "every verb" — so a handler with no
/// verb collides with **all** of the ones that name one. Which is the shape that actually turns up:
/// a legacy `@RequestMapping("/ordini/{id}")` with no `method`, and a new `@GetMapping` beside it.
fn ambiguous(endpoints: &[Endpoint]) -> Vec<EndpointIssue> {
    let mut by_path: HashMap<&str, Vec<&Endpoint>> = HashMap::new();
    for endpoint in endpoints {
        by_path.entry(endpoint.path.as_str()).or_default().push(endpoint);
    }

    let mut out = Vec::new();
    for (path, claimants) in by_path {
        if claimants.len() < 2 {
            continue;
        }
        for (i, endpoint) in claimants.iter().enumerate() {
            let clashing: Vec<&&Endpoint> = claimants
                .iter()
                .enumerate()
                .filter(|(j, other)| *j != i && overlap(endpoint, other))
                .map(|(_, other)| other)
                .collect();
            if clashing.is_empty() {
                continue;
            }
            // The same handler listed twice is one method with two mapping annotations, which is
            // not a collision — it is one registration.
            let other = clashing[0];
            if other.file == endpoint.file && other.offset == endpoint.offset {
                continue;
            }
            out.push(EndpointIssue {
                code: CODE_AMBIGUOUS,
                message: format!(
                    "`{path}` is already mapped by {}.{} — Spring refuses to start with two \
                     handlers for one route",
                    short(&other.class_fqcn),
                    other.handler
                ),
                severity: "error",
                file: endpoint.file.clone(),
            start: span(endpoint).0,
            end: span(endpoint).1,
            });
        }
    }
    out
}

/// Whether two mappings of the same path can both receive a request.
///
/// A mapping with no verb accepts them all, so it overlaps with everything; two that both name
/// verbs overlap only where the verbs do. `produces` is what separates two handlers of one route
/// deliberately (JSON and XML), so a difference there is not a collision.
fn overlap(a: &Endpoint, b: &Endpoint) -> bool {
    if a.produces != b.produces {
        return false;
    }
    if a.methods.is_empty() || b.methods.is_empty() {
        return true;
    }
    a.methods.iter().any(|m| b.methods.contains(m))
}

/// What a handler says about its path variables, against what its path actually has.
fn path_variables(endpoint: &Endpoint) -> Vec<EndpointIssue> {
    let mut out = Vec::new();
    let bound: Vec<&str> = endpoint
        .params
        .iter()
        .filter(|p| p.binding == "path")
        .map(|p| p.effective_name())
        .collect();

    // ── the certain one: an annotation naming a variable the path does not have ───────────
    for name in &bound {
        // A path built from a property placeholder (`@GetMapping("${api.base}/{id}")`) has a
        // template this cannot resolve, so nothing is claimed about it in either direction.
        if endpoint.path.contains("${") {
            break;
        }
        if endpoint.path_vars.iter().any(|v| v == name) {
            continue;
        }
        out.push(EndpointIssue {
            code: CODE_UNMATCHED_VARIABLE,
            message: format!(
                "`{}` has no `{{{name}}}` in it, so @PathVariable(\"{name}\") binds nothing — every \
                 call to this endpoint fails with MissingPathVariableException",
                endpoint.path
            ),
            severity: "error",
            file: endpoint.file.clone(),
            start: span(endpoint).0,
            end: span(endpoint).1,
        });
    }

    // ── the same variable named twice ────────────────────────────────────────
    for (i, name) in bound.iter().enumerate() {
        if bound[..i].contains(name) {
            out.push(EndpointIssue {
                code: CODE_DUPLICATE_VARIABLE,
                message: format!(
                    "two parameters of `{}` both bind `{{{name}}}` — they will receive the same \
                     value, which is unlikely to be what was meant",
                    endpoint.handler
                ),
                severity: "warning",
                file: endpoint.file.clone(),
            start: span(endpoint).0,
            end: span(endpoint).1,
            });
        }
    }

    // ── a template variable nothing binds ────────────────────────────────────
    //
    // Legal on its own: a handler may ignore a variable, and plenty do. What is *not* ordinary is
    // a handler that binds some of them and misses one — that is a misspelling, and it is the only
    // shape reported, because reporting the other would fire on every `/{version}/…` prefix in the
    // project.
    if !bound.is_empty() && !endpoint.path.contains("${") {
        for variable in &endpoint.path_vars {
            if bound.contains(&variable.as_str()) {
                continue;
            }
            out.push(EndpointIssue {
                code: CODE_UNBOUND_VARIABLE,
                message: format!(
                    "`{{{variable}}}` is in the path and no parameter binds it, while the others \
                     do — a misspelled @PathVariable name looks exactly like this"
                ),
                severity: "warning",
                file: endpoint.file.clone(),
            start: span(endpoint).0,
            end: span(endpoint).1,
            });
        }
    }

    out
}

/// `com.acme.web.OrdiniController` → `OrdiniController`.
fn short(fqcn: &str) -> &str {
    fqcn.rsplit('.').next().unwrap_or(fqcn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::EndpointParam;

    fn param(name: &str, binding: &str, bound: &str) -> EndpointParam {
        EndpointParam {
            name: name.to_string(),
            type_text: "String".to_string(),
            binding: binding.to_string(),
            bound_name: bound.to_string(),
            required: true,
        }
    }

    fn endpoint(path: &str, methods: &[&str], handler: &str, params: Vec<EndpointParam>) -> Endpoint {
        let path_vars = path
            .split('{')
            .skip(1)
            .filter_map(|rest| rest.split('}').next())
            .map(|v| v.split(':').next().unwrap_or(v).to_string())
            .collect();
        Endpoint {
            methods: methods.iter().map(|m| m.to_string()).collect(),
            path: path.to_string(),
            class_fqcn: format!("com.acme.{handler}Controller"),
            handler: handler.to_string(),
            file: format!("/p/{handler}.java"),
            offset: 100,
            line: 10,
            path_vars,
            produces: String::new(),
            return_type: "String".to_string(),
            params,
        }
    }

    // ── ambiguous mappings ───────────────────────────────────────────────────

    #[test]
    fn two_handlers_for_one_route_is_a_startup_failure() {
        let found = issues(&[
            endpoint("/ordini/{id}", &["GET"], "nuovo", vec![]),
            endpoint("/ordini/{id}", &["GET"], "legacy", vec![]),
        ]);
        let clashes: Vec<&EndpointIssue> =
            found.iter().filter(|i| i.code == CODE_AMBIGUOUS).collect();
        assert_eq!(clashes.len(), 2, "both sites are told: {found:?}");
        assert_eq!(clashes[0].severity, "error");
        assert!(clashes.iter().any(|i| i.message.contains("legacyController")));
    }

    /// The shape that actually turns up: a legacy `@RequestMapping` with no verb, which accepts
    /// them all, beside a new `@GetMapping`.
    #[test]
    fn a_mapping_with_no_verb_collides_with_every_verb() {
        let found = issues(&[
            endpoint("/ordini", &[], "legacy", vec![]),
            endpoint("/ordini", &["POST"], "nuovo", vec![]),
        ]);
        assert_eq!(found.iter().filter(|i| i.code == CODE_AMBIGUOUS).count(), 2);
    }

    #[test]
    fn different_verbs_on_one_path_are_the_ordinary_case() {
        let found = issues(&[
            endpoint("/ordini", &["GET"], "elenco", vec![]),
            endpoint("/ordini", &["POST"], "crea", vec![]),
        ]);
        assert!(found.is_empty(), "{found:?}");
    }

    /// Two handlers of one route separated by `produces` are a deliberate pair — JSON and XML —
    /// and Spring maps them both.
    #[test]
    fn a_different_media_type_is_not_a_collision() {
        let mut json = endpoint("/ordini", &["GET"], "json", vec![]);
        json.produces = "application/json".to_string();
        let mut xml = endpoint("/ordini", &["GET"], "xml", vec![]);
        xml.produces = "application/xml".to_string();
        assert!(issues(&[json, xml]).is_empty());
    }

    /// One method with two mapping annotations is one registration, not two handlers.
    #[test]
    fn one_handler_listed_twice_is_not_a_collision() {
        let a = endpoint("/ordini", &["GET"], "elenco", vec![]);
        let b = a.clone();
        assert!(issues(&[a, b]).is_empty());
    }

    // ── path variables ───────────────────────────────────────────────────────

    /// The 500 on every call: the annotation names a variable the template does not have.
    #[test]
    fn a_path_variable_the_path_does_not_have_is_an_error() {
        let found = issues(&[endpoint(
            "/ordini/{ordineId}",
            &["GET"],
            "uno",
            vec![param("id", "path", "id")],
        )]);
        let issue = found.iter().find(|i| i.code == CODE_UNMATCHED_VARIABLE).expect("the mismatch");
        assert_eq!(issue.severity, "error");
        assert!(issue.message.contains("MissingPathVariableException"));
    }

    #[test]
    fn a_matching_pair_is_silent() {
        let found = issues(&[endpoint(
            "/ordini/{id}",
            &["GET"],
            "uno",
            vec![param("id", "path", "")],
        )]);
        assert!(found.is_empty(), "{found:?}");
    }

    /// The name the annotation gives wins over the parameter's own — which is the whole reason
    /// `effective_name` exists, and the case a naive check gets backwards.
    #[test]
    fn the_annotations_name_is_the_one_that_binds() {
        let found = issues(&[endpoint(
            "/ordini/{ordineId}",
            &["GET"],
            "uno",
            vec![param("id", "path", "ordineId")],
        )]);
        assert!(found.is_empty(), "{found:?}");
    }

    /// A handler that binds some variables and misses one is a misspelling. A handler that binds
    /// none is a deliberate "I do not need it", and reporting it would fire on every `/{version}/`
    /// prefix in the project.
    #[test]
    fn an_unbound_variable_is_reported_only_beside_bound_ones() {
        let found = issues(&[endpoint(
            "/ordini/{ordineId}/righe/{rigaId}",
            &["GET"],
            "riga",
            vec![param("ordineId", "path", "")],
        )]);
        let issue = found.iter().find(|i| i.code == CODE_UNBOUND_VARIABLE).expect("the unbound one");
        assert!(issue.message.contains("rigaId"), "{}", issue.message);
        assert_eq!(issue.severity, "warning");

        let ignored = issues(&[endpoint("/{version}/ordini", &["GET"], "elenco", vec![])]);
        assert!(ignored.is_empty(), "binds none on purpose: {ignored:?}");
    }

    #[test]
    fn two_parameters_binding_one_variable_is_reported() {
        let found = issues(&[endpoint(
            "/ordini/{id}",
            &["GET"],
            "uno",
            vec![param("id", "path", ""), param("altro", "path", "id")],
        )]);
        assert!(found.iter().any(|i| i.code == CODE_DUPLICATE_VARIABLE), "{found:?}");
    }

    /// A path assembled from configuration has a template this cannot resolve, and claiming
    /// anything about it would be claiming something about a value nobody here has seen.
    #[test]
    fn a_path_built_from_a_placeholder_is_not_judged() {
        let found = issues(&[endpoint(
            "${api.base}/ordini/{id}",
            &["GET"],
            "uno",
            vec![param("id", "path", "id"), param("tenant", "path", "tenant")],
        )]);
        assert!(found.is_empty(), "{found:?}");
    }

    /// A typed template variable is still that variable.
    #[test]
    fn a_regex_constraint_does_not_change_the_variables_name() {
        let found = issues(&[endpoint(
            "/ordini/{id:[0-9]+}",
            &["GET"],
            "uno",
            vec![param("id", "path", "")],
        )]);
        assert!(found.is_empty(), "{found:?}");
    }

    /// Query and body parameters are not path variables, and must not be read as one.
    #[test]
    fn only_path_bindings_are_path_variables() {
        let found = issues(&[endpoint(
            "/ordini",
            &["POST"],
            "crea",
            vec![param("q", "query", ""), param("ordine", "body", "")],
        )]);
        assert!(found.is_empty(), "{found:?}");
    }
}
