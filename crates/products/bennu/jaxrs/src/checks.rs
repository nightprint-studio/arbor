//! What is wrong with a resource method, in the ways the runtime only reports at request time — or
//! by refusing the deployment.
//!
//! ## The two that need the project
//!
//! *"Does this `@PathParam` name a variable of the route?"* looks local and is not: the route
//! starts with the class's `@Path`, possibly inherited from an interface, and a sub-resource
//! locator elsewhere can lend its own variables to whatever it returns. *"Does another method
//! already answer this route?"* is in another file by definition. Both wait for the first scan:
//! reporting them from a cold start would report the index's ignorance as the project's defect.
//!
//! ## The three that do not
//!
//! Two entities, a non-public resource method, a body on a `GET` — each is a fact about one method
//! and nothing else, so each is reported from the first keystroke.

use bennu_proto::prelude::{severity, Diagnostic};

use crate::model::{AppPrefix, JaxRsModel, Locators, Param, PathSpec, ResourceMethod, Site};
use crate::paths::{module_of, route_key};
use crate::prefix::Registration;

pub const CODE_UNKNOWN_PATH_PARAM: &str = "jaxrs.unknown-path-param";
pub const CODE_DUPLICATE_ROUTE: &str = "jaxrs.duplicate-route";
pub const CODE_SEVERAL_ENTITY_PARAMS: &str = "jaxrs.several-entity-params";
pub const CODE_NON_PUBLIC_RESOURCE_METHOD: &str = "jaxrs.non-public-resource-method";
pub const CODE_GET_WITH_BODY: &str = "jaxrs.get-with-body";

/// Every finding for the buffer `file`, whose resource methods are `buffer`.
pub fn diagnostics(
    file: &str,
    buffer: &[ResourceMethod],
    model: &JaxRsModel,
    scanned: bool,
) -> Vec<Diagnostic> {
    let prefix = &model.ctx.prefix;
    let declared = || buffer.iter().filter(|m| m.site == Site::Declared);
    let mut out = Vec::new();
    for m in declared() {
        out.extend(entity_params(m));
        out.extend(non_public(m, prefix));
    }
    if !scanned {
        return out;
    }
    // The buffer's locators as they are now, on top of the project's.
    let mut locators = model.ctx.locators.clone();
    for m in buffer {
        locators.add(m, prefix);
    }
    for m in declared() {
        out.extend(unknown_path_params(m, prefix, &locators));
    }
    out.extend(duplicate_routes(file, buffer, model));
    out
}

fn diagnostic(code: &str, level: &str, message: String, start: usize, end: usize) -> Diagnostic {
    Diagnostic { message, severity: level.to_string(), code: code.to_string(), start, end }
}

/// `@PathParam("id")` on a route with no `{id}`: the parameter is null on every request, and nothing
/// fails to say so.
///
/// Silent unless every piece of the route is a literal, the class has a class-level `@Path` (one
/// without may be a sub-resource, whose variables come from a locator), the method's annotations are
/// the ones in effect, and no locator in the project declares the name.
fn unknown_path_params(m: &ResourceMethod, prefix: &AppPrefix, locators: &Locators) -> Vec<Diagnostic> {
    if !m.resolved || !matches!(m.class_path, PathSpec::Literal(_)) {
        return Vec::new();
    }
    let Some(variables) = m.variables(prefix) else { return Vec::new() };
    m.params
        .iter()
        .filter(|p| p.binding == "path")
        .filter_map(|p| p.bound.as_ref())
        .filter(|lit| lit.file == m.file)
        .filter(|lit| !variables.iter().any(|v| v.name == lit.value) && !locators.may_bind(&lit.value))
        .map(|lit| {
            // An empty `""` has no contents to underline; the quotes are.
            let (start, end) =
                if lit.start < lit.end { (lit.start, lit.end) } else { (lit.start.saturating_sub(1), lit.end + 1) };
            diagnostic(
                CODE_UNKNOWN_PATH_PARAM,
                severity::WARNING,
                format!(
                    "`{}` is not a variable of `{}` — this parameter receives null (or its @DefaultValue) on \
                     every request",
                    lit.value,
                    m.label(prefix)
                ),
                start,
                end,
            )
        })
        .collect()
}

/// More than one entity parameter, and an entity on a method whose requests carry none.
fn entity_params(m: &ResourceMethod) -> Vec<Diagnostic> {
    let Some(verb) = m.verb.as_deref() else { return Vec::new() };
    let entities: Vec<&Param> = m.params.iter().filter(|p| p.binding == "body").collect();
    let mut out = Vec::new();
    for extra in entities.iter().skip(1) {
        out.push(diagnostic(
            CODE_SEVERAL_ENTITY_PARAMS,
            severity::ERROR,
            format!(
                "`{}` is a second request entity on `{}` — a request has one body, and an unannotated \
                 parameter is where it goes; annotate the others with where their value comes from",
                extra.name, m.method
            ),
            extra.name_offset,
            extra.name_offset + extra.name.len(),
        ));
    }
    if matches!(verb, "GET" | "HEAD") {
        for p in &entities {
            out.push(diagnostic(
                CODE_GET_WITH_BODY,
                severity::WEAK,
                format!(
                    "`{}` reads a request body on a {verb} method — clients, proxies and caches routinely drop \
                     one, so it is usually empty",
                    p.name
                ),
                p.name_offset,
                p.name_offset + p.name.len(),
            ));
        }
    }
    out
}

/// A resource method the runtime silently skips. Interface methods are public by definition.
fn non_public(m: &ResourceMethod, prefix: &AppPrefix) -> Option<Diagnostic> {
    if m.verb.is_none() || m.in_interface || m.is_public {
        return None;
    }
    Some(diagnostic(
        CODE_NON_PUBLIC_RESOURCE_METHOD,
        severity::WARNING,
        format!(
            "`{}` is not public, so JAX-RS never dispatches to it — `{}` is simply not there, and nothing \
             logs why",
            m.method,
            m.label(prefix)
        ),
        m.offset,
        m.offset + m.method.len(),
    ))
}

/// What makes two resource methods one route to the runtime.
#[derive(Debug, PartialEq, Eq)]
struct RouteKey {
    verb: String,
    module: String,
    class_path: String,
    method_path: Option<String>,
    produces: String,
    consumes: String,
}

/// A method's route identity — `None` for anything this check will not judge.
///
/// Routes declared on an interface are left out entirely: an interface nothing implements is, more
/// often than not, a REST *client* mirroring a server resource, and it would clash with the very
/// class it describes. The class path is compared on its own rather than folded into the full path
/// because the runtime matches a class first: `a` + `b/c` and `a/b` + `c` are the same URL and not
/// an ambiguity.
fn route_identity(m: &ResourceMethod) -> Option<RouteKey> {
    if !m.listed || !m.resolved || m.in_interface {
        return None;
    }
    let verb = m.verb.clone()?;
    let PathSpec::Literal(class) = &m.class_path else { return None };
    let method_path = match &m.method_path {
        PathSpec::Absent => None,
        PathSpec::Literal(l) => Some(route_key(&l.value)?),
        PathSpec::Opaque => return None,
    };
    Some(RouteKey {
        verb,
        module: module_of(&m.file).to_string(),
        class_path: route_key(&class.value)?,
        method_path,
        produces: m.produces.clone(),
        consumes: m.consumes.clone(),
    })
}

/// Two methods answering one route in one application — the deployment is refused.
fn duplicate_routes(file: &str, buffer: &[ResourceMethod], model: &JaxRsModel) -> Vec<Diagnostic> {
    let ctx = &model.ctx;
    if ctx.applications > 1 || ctx.prefix == AppPrefix::Unknown || ctx.registration == Registration::Explicit {
        return Vec::new();
    }
    let deployed = |m: &&ResourceMethod| ctx.registration.deploys(&m.class_fqcn);
    let candidates: Vec<(&ResourceMethod, RouteKey)> = model
        .methods
        .iter()
        .filter(|m| m.file != file)
        .chain(buffer.iter())
        .filter(deployed)
        .filter_map(|m| route_identity(m).map(|k| (m, k)))
        .collect();

    let mut out = Vec::new();
    for m in buffer.iter().filter(deployed) {
        let Some(key) = route_identity(m) else { continue };
        let clash = candidates.iter().find(|(other, k)| *k == key && !(other.file == m.file && other.offset == m.offset));
        let Some((other, _)) = clash else { continue };
        out.push(diagnostic(
            CODE_DUPLICATE_ROUTE,
            severity::ERROR,
            format!(
                "`{}` is also answered by {} — two resource methods for one request are ambiguous, and Jersey \
                 refuses to deploy the application",
                m.label(&ctx.prefix),
                other.handler()
            ),
            m.offset,
            m.offset + m.method.len(),
        ));
    }
    out
}
