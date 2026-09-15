//! The JAX-RS model — what the extension knows about a project after a scan.
//!
//! Everything is owned and flat. A resource method is recorded **where it is written** (a
//! declaration) and, when an implementation class inherits it from an annotated interface, again
//! **where it is served** (an inherited entry pointing at the implementation). The two are kept
//! apart because two different questions read them: "is this annotation right" is asked of the file
//! it is written in, "which class answers this URL" of the one the runtime instantiates.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bennu_facts::prelude::JavaFacts;

use crate::known::CustomVerb;
use crate::paths::{join, simple_name, templates, Template};
use crate::prefix::Registration;

/// One parsed Java file kept by the model — only the ones declaring an annotated interface, which a
/// buffer implementing that interface needs to read its routes from.
#[derive(Debug, Clone)]
pub struct Unit {
    pub facts: JavaFacts,
    pub text: String,
}

impl Unit {
    pub fn src(&self) -> Src<'_> {
        Src { facts: &self.facts, text: &self.text }
    }
}

/// A parsed file as the extraction reads it: the facts, and the text their spans index.
#[derive(Debug, Clone, Copy)]
pub struct Src<'a> {
    pub facts: &'a JavaFacts,
    pub text: &'a str,
}

/// A string literal and where it is written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lit {
    pub value: String,
    /// Forward-slashed path of the file the span indexes.
    pub file: String,
    pub start: usize,
    pub end: usize,
}

/// A `@Path` as far as it can be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathSpec {
    /// Not written — the class or method adds nothing to the route.
    Absent,
    Literal(Lit),
    /// Written as something other than one literal: a constant, a concatenation.
    Opaque,
}

impl PathSpec {
    /// The text this piece contributes to a route. `None` when it cannot be known.
    pub fn text(&self) -> Option<&str> {
        match self {
            PathSpec::Absent => Some(""),
            PathSpec::Literal(l) => Some(&l.value),
            PathSpec::Opaque => None,
        }
    }

    pub fn is_written(&self) -> bool {
        !matches!(self, PathSpec::Absent)
    }
}

/// Whether an entry is the annotated declaration itself or a route it hands to an implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Site {
    Declared,
    Inherited,
}

/// One parameter of a resource method.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub name: String,
    pub type_text: String,
    pub name_offset: usize,
    /// `path` | `query` | `form` | `header` | `cookie` | `matrix` | `bean` | `context` — or `body`
    /// for the request entity, or `arg` for a parameter some other library supplies.
    pub binding: &'static str,
    /// The binding annotation's value, when written as one literal (`@PathParam("id")`).
    pub bound: Option<Lit>,
    /// `@DefaultValue` is present.
    pub optional: bool,
}

/// A method JAX-RS dispatches to: a resource method (with a verb) or a sub-resource locator
/// (a `@Path` and no verb).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceMethod {
    pub class_fqcn: String,
    pub method: String,
    /// Where the row points — forward-slashed.
    pub file: String,
    /// Byte offset of the method's name in [`Self::file`].
    pub offset: usize,
    pub line: u32,
    pub return_type: String,
    /// `None` for a sub-resource locator.
    pub verb: Option<String>,
    /// Span of the verb annotation — only for a declaration, where it is in [`Self::file`].
    pub verb_span: Option<(usize, usize)>,
    /// Span of the method-level `@Path` annotation — only for a declaration.
    pub path_span: Option<(usize, usize)>,
    pub class_path: PathSpec,
    pub method_path: PathSpec,
    /// Effective `@Produces` / `@Consumes` text (method over class), empty when neither says.
    pub produces: String,
    pub consumes: String,
    pub params: Vec<Param>,
    pub site: Site,
    pub in_interface: bool,
    pub is_public: bool,
    /// A row of the Endpoints catalog: the class answers requests on its own (it has a class-level
    /// path and is instantiable), and no other entry already stands for this route.
    pub listed: bool,
    /// Every annotation in play was resolved — no interface with several implementations, no
    /// abstract class whose subclasses decide. Checks that judge the route read only these.
    pub resolved: bool,
}

impl ResourceMethod {
    /// `GET` or `LOCATOR`.
    pub fn kind(&self) -> &str {
        self.verb.as_deref().unwrap_or("LOCATOR")
    }

    /// The full route, when every piece of it is a literal.
    pub fn full_path(&self, prefix: &AppPrefix) -> Option<String> {
        Some(join(&[prefix.text(), self.class_path.text()?, self.method_path.text()?]))
    }

    /// The route as the panel shows it — an unreadable piece becomes `…` rather than hiding the row.
    pub fn display_path(&self, prefix: &AppPrefix) -> String {
        join(&[
            prefix.text(),
            self.class_path.text().unwrap_or("…"),
            self.method_path.text().unwrap_or("…"),
        ])
    }

    /// `GET /api/orders/{id}` — the label the panel, the gutter and a hover share.
    pub fn label(&self, prefix: &AppPrefix) -> String {
        format!("{} {}", self.kind(), self.display_path(prefix))
    }

    /// `OrderResource#find`.
    pub fn handler(&self) -> String {
        format!("{}#{}", simple_name(&self.class_fqcn), self.method)
    }

    /// Every template variable of the full route, or `None` when some piece is not a literal or
    /// not a template this crate reads with certainty.
    pub fn variables(&self, prefix: &AppPrefix) -> Option<Vec<Template>> {
        let mut out = templates(prefix.text())?;
        out.extend(templates(self.class_path.text()?)?);
        out.extend(templates(self.method_path.text()?)?);
        Some(out)
    }
}

/// The application path every route starts with.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum AppPrefix {
    /// Nothing declares one — routes start at the context root.
    #[default]
    None,
    /// One value, normalised (`api`, or empty for `/*`).
    Known(String),
    /// Several distinct values, or one that is not a literal.
    Unknown,
}

impl AppPrefix {
    pub fn text(&self) -> &str {
        match self {
            AppPrefix::Known(p) => p,
            AppPrefix::None | AppPrefix::Unknown => "",
        }
    }
}

/// A class implementing an annotated interface, as much as pairing it needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Implementor {
    pub fqcn: String,
    /// It carries its own class-level `@Path`, which replaces the interface's.
    pub has_class_path: bool,
    /// Every `(name, arity)` it declares.
    pub methods: Vec<(String, usize)>,
    /// The `(name, arity)` pairs it re-annotates — the interface's annotations on those are ignored.
    pub reannotated: Vec<(String, usize)>,
}

/// What the project's sub-resource locators could bind.
///
/// A class reached through a locator sees the locator's template variables too, and nothing says
/// statically which classes a locator returns (`Object` is a legal return type). So rather than
/// guessing the class, the check asks this: a `@PathParam` name that some locator's route declares
/// might be bound through it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Locators {
    pub any: bool,
    /// Some locator's route could not be read, so any name might be bound through it.
    pub opaque: bool,
    pub vars: HashSet<String>,
}

impl Locators {
    pub fn add(&mut self, rm: &ResourceMethod, prefix: &AppPrefix) {
        if rm.verb.is_some() {
            return;
        }
        self.any = true;
        match rm.variables(prefix) {
            Some(vars) => self.vars.extend(vars.into_iter().map(|t| t.name)),
            None => self.opaque = true,
        }
    }

    pub fn from_methods<'a>(methods: impl IntoIterator<Item = &'a ResourceMethod>, prefix: &AppPrefix) -> Self {
        let mut out = Self::default();
        for m in methods {
            out.add(m, prefix);
        }
        out
    }

    /// Whether `name` might reach a sub-resource method through a locator.
    pub fn may_bind(&self, name: &str) -> bool {
        self.any && (self.opaque || self.vars.contains(name))
    }
}

/// The project-wide facts a buffer is re-read against.
#[derive(Debug, Clone, Default)]
pub struct Context {
    /// Files declaring an annotated interface.
    pub interfaces: Vec<Arc<Unit>>,
    /// Interface FQCN → the project classes implementing it.
    pub implementors: HashMap<String, Vec<Implementor>>,
    pub custom_verbs: Vec<CustomVerb>,
    pub prefix: AppPrefix,
    /// How many JAX-RS applications the project appears to deploy.
    pub applications: usize,
    /// Which resource classes those applications deploy.
    pub registration: Registration,
    pub locators: Locators,
}

impl Context {
    /// Simple names of the annotated interfaces — what makes a file that never mentions `ws.rs`
    /// worth reading (it may implement one).
    pub fn interface_names(&self) -> Vec<&str> {
        self.interfaces
            .iter()
            .flat_map(|u| u.facts.types.iter().filter(|t| t.kind == "interface").map(|t| t.name.as_str()))
            .collect()
    }
}

/// Everything the extension knows.
#[derive(Debug, Clone, Default)]
pub struct JaxRsModel {
    pub methods: Vec<ResourceMethod>,
    pub ctx: Context,
}

impl JaxRsModel {
    /// The Endpoints rows' source: every listed entry.
    pub fn routes(&self) -> impl Iterator<Item = &ResourceMethod> {
        self.methods.iter().filter(|m| m.listed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lit(value: &str) -> PathSpec {
        PathSpec::Literal(Lit { value: value.into(), file: "/p/R.java".into(), start: 0, end: 0 })
    }

    fn rm(class_path: PathSpec, method_path: PathSpec, verb: Option<&str>) -> ResourceMethod {
        ResourceMethod {
            class_fqcn: "com.acme.OrderResource".into(),
            method: "find".into(),
            file: "/p/R.java".into(),
            offset: 0,
            line: 1,
            return_type: "Order".into(),
            verb: verb.map(str::to_string),
            verb_span: None,
            path_span: None,
            class_path,
            method_path,
            produces: String::new(),
            consumes: String::new(),
            params: Vec::new(),
            site: Site::Declared,
            in_interface: false,
            is_public: true,
            listed: true,
            resolved: true,
        }
    }

    #[test]
    fn the_application_path_class_path_and_method_path_join_into_one_route() {
        let m = rm(lit("orders"), lit("{id: \\d+}"), Some("GET"));
        let prefix = AppPrefix::Known("api".into());
        assert_eq!(m.full_path(&prefix).as_deref(), Some("/api/orders/{id: \\d+}"));
        assert_eq!(m.label(&prefix), "GET /api/orders/{id: \\d+}");
        assert_eq!(m.handler(), "OrderResource#find");
        assert_eq!(m.variables(&prefix).unwrap()[0].name, "id");
        assert_eq!(m.label(&AppPrefix::None), "GET /orders/{id: \\d+}");
    }

    #[test]
    fn an_opaque_piece_hides_nothing_but_claims_nothing() {
        let m = rm(PathSpec::Opaque, lit("{id}"), None);
        assert!(m.full_path(&AppPrefix::None).is_none());
        assert!(m.variables(&AppPrefix::None).is_none());
        assert_eq!(m.label(&AppPrefix::None), "LOCATOR /…/{id}");
    }

    #[test]
    fn a_locator_lends_its_variables_to_whatever_it_returns() {
        let mut l = Locators::default();
        assert!(!l.may_bind("id"), "no locator, nothing borrowed");
        l.add(&rm(lit("orders"), lit("{orderId}"), Some("GET")), &AppPrefix::None);
        assert!(!l.any, "a resource method is not a locator");
        l.add(&rm(lit("orders"), lit("{orderId}/items"), None), &AppPrefix::None);
        assert!(l.may_bind("orderId") && !l.may_bind("id"));
        l.add(&rm(PathSpec::Opaque, lit("x"), None), &AppPrefix::None);
        assert!(l.may_bind("id"), "an unreadable locator could bind anything");
    }
}
