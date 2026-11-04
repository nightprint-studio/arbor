//! `bennu_endpoints` — one row shape for every framework that answers a URL.

use bennu_ext::prelude::ExtEntry;
use bennu_spring::prelude::Endpoint;
use serde::Serialize;

use super::has;

/// One parameter of a handler, with where its value comes from.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ParamRow {
    /// The name the request addresses it by (the annotation's, else the parameter's).
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    /// `path` | `query` | `body` | `header` | `cookie` | `part` | `model` | `arg`.
    pub binding: String,
    pub required: bool,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct EndpointRow {
    /// `spring` | `jaxrs` | `servlet` | `struts`.
    pub framework: String,
    /// HTTP methods; empty means the route answers any of them.
    pub methods: Vec<String>,
    pub path: String,
    pub class: String,
    pub handler: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub returns: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub produces: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub consumes: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<ParamRow>,
    /// Framework-specific extras (a Struts action's interceptors and results, a JAX-RS media type).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
}

#[derive(Debug, Default)]
pub struct EndpointFilter {
    pub path: Option<String>,
    pub method: Option<String>,
    pub class: Option<String>,
    pub framework: Option<String>,
}

impl EndpointFilter {
    pub fn keeps(&self, row: &EndpointRow) -> bool {
        let method_ok = match self.method.as_deref().filter(|m| !m.is_empty()) {
            None => true,
            // A route that names no method answers every one of them.
            Some(m) => row.methods.is_empty() || row.methods.iter().any(|x| x.eq_ignore_ascii_case(m)),
        };
        let framework_ok = self.framework.as_deref().filter(|f| !f.is_empty()).is_none_or(|f| row.framework.eq_ignore_ascii_case(f));
        method_ok
            && framework_ok
            && has(&row.path, &self.path)
            && (has(&row.class, &self.class) || has(&row.handler, &self.class))
    }
}

pub fn from_spring(e: &Endpoint) -> EndpointRow {
    EndpointRow {
        framework: "spring".into(),
        methods: e.methods.clone(),
        path: e.path.clone(),
        class: e.class_fqcn.clone(),
        handler: e.handler.clone(),
        returns: e.return_type.clone(),
        produces: e.produces.clone(),
        consumes: e.consumes.clone(),
        params: e
            .params
            .iter()
            .map(|p| ParamRow {
                name: p.effective_name().to_string(),
                type_name: p.type_text.clone(),
                binding: p.binding.clone(),
                required: p.required,
            })
            .collect(),
        details: Vec::new(),
        file: Some(e.file.clone()),
        line: Some(e.line),
    }
}

/// A catalog row of another framework as an endpoint. Those extensions publish display rows only:
/// the badge is the HTTP verb(s), `primary` the path, `secondary` `Class#method`, and the children
/// whatever the framework nests under a route.
pub fn from_entry(framework: &str, e: &ExtEntry) -> EndpointRow {
    // A servlet's badge is its kind (`servlet`, `filter`), not a verb: it answers every method.
    let verbs = framework != "servlet" && !e.kind.eq_ignore_ascii_case("ANY") && !e.kind.is_empty();
    let methods = if verbs { e.kind.split('|').map(|m| m.trim().to_string()).collect() } else { Vec::new() };
    let (class, handler) = e.secondary.split_once('#').map(|(c, h)| (c.to_string(), h.to_string())).unwrap_or_else(|| (e.secondary.clone(), String::new()));
    let mut details = Vec::new();
    if !verbs && !e.kind.is_empty() && !e.kind.eq_ignore_ascii_case("ANY") {
        details.push(e.kind.clone());
    }
    details.extend(e.tags.iter().cloned());
    details.extend(e.children.iter().map(|c| [c.kind.as_str(), c.primary.as_str(), c.secondary.as_str()].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join(" ")));
    EndpointRow {
        framework: framework.into(),
        methods,
        path: e.primary.clone(),
        class,
        handler,
        returns: String::new(),
        produces: String::new(),
        consumes: String::new(),
        params: Vec::new(),
        details,
        file: e.file.clone(),
        line: e.line,
    }
}

pub fn collect(root: &str, filter: &EndpointFilter) -> Vec<EndpointRow> {
    let mut rows: Vec<EndpointRow> = crate::frameworks::spring_model_of(root)
        .map(|m| m.endpoints.iter().map(from_spring).collect())
        .unwrap_or_default();
    // The namespaced kinds, not the bare `endpoints`: that one would repeat Spring's rows.
    for (framework, kind) in [("jaxrs", "jaxrs.endpoints"), ("servlet", "jakartaee.endpoints"), ("struts", "struts.endpoints")] {
        rows.extend(crate::frameworks::catalog_of(root, kind).iter().map(|e| from_entry(framework, e)));
    }
    rows.retain(|r| filter.keeps(r));
    rows.sort_by(|a, b| a.path.cmp(&b.path).then_with(|| a.methods.cmp(&b.methods)));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_spring::prelude::EndpointParam;

    fn spring() -> Endpoint {
        Endpoint {
            methods: vec!["POST".into()],
            path: "/orders/{id}".into(),
            class_fqcn: "com.acme.OrderController".into(),
            handler: "update".into(),
            file: "/p/OrderController.java".into(),
            offset: 10,
            line: 12,
            path_vars: vec!["id".into()],
            produces: "application/json".into(),
            consumes: "application/json".into(),
            return_type: "ResponseEntity<Order>".into(),
            params: vec![
                EndpointParam { name: "id".into(), type_text: "long".into(), binding: "path".into(), bound_name: String::new(), required: true },
                EndpointParam { name: "q".into(), type_text: "String".into(), binding: "query".into(), bound_name: "search".into(), required: false },
            ],
        }
    }

    #[test]
    fn a_spring_route_keeps_its_media_types_and_parameter_bindings() {
        let row = from_spring(&spring());
        assert_eq!((row.consumes.as_str(), row.produces.as_str()), ("application/json", "application/json"));
        assert_eq!(row.params[1].name, "search", "addressed by the annotation's name");
        assert!(!row.params[1].required);
        assert_eq!(row.line, Some(12));
    }

    #[test]
    fn filters_combine_and_a_methodless_route_answers_every_verb() {
        let row = from_spring(&spring());
        let f = |path: Option<&str>, method: Option<&str>, class: Option<&str>| EndpointFilter {
            path: path.map(Into::into),
            method: method.map(Into::into),
            class: class.map(Into::into),
            framework: None,
        };
        assert!(f(Some("/orders"), Some("post"), Some("ordercontroller")).keeps(&row));
        assert!(f(None, None, Some("update")).keeps(&row), "the handler name matches too");
        assert!(!f(None, Some("GET"), None).keeps(&row));
        let any = EndpointRow { methods: Vec::new(), ..from_spring(&spring()) };
        assert!(f(None, Some("DELETE"), None).keeps(&any));
        assert!(!EndpointFilter { framework: Some("jaxrs".into()), ..EndpointFilter::default() }.keeps(&row));
    }

    #[test]
    fn a_catalog_row_splits_verbs_handler_and_children() {
        let entry = ExtEntry {
            id: "GET|POST /a".into(),
            primary: "/a".into(),
            secondary: "com.acme.A#run".into(),
            kind: "GET|POST".into(),
            tags: vec!["application/json".into()],
            children: vec![ExtEntry { kind: "result".into(), primary: "success".into(), secondary: "/ok.jsp".into(), ..ExtEntry::default() }],
            ..ExtEntry::default()
        };
        let row = from_entry("jaxrs", &entry);
        assert_eq!(row.methods, vec!["GET", "POST"]);
        assert_eq!((row.class.as_str(), row.handler.as_str()), ("com.acme.A", "run"));
        assert_eq!(row.details, vec!["application/json", "result success /ok.jsp"]);
        let any = from_entry("struts", &ExtEntry { kind: "ANY".into(), ..entry.clone() });
        assert!(any.methods.is_empty());
        let filter = from_entry("servlet", &ExtEntry { kind: "filter".into(), ..entry });
        assert!(filter.methods.is_empty(), "a servlet kind is not an HTTP verb");
        assert_eq!(filter.details[0], "filter");
    }
}
