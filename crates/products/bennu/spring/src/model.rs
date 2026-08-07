//! The Spring model — what the extension knows about a project after a scan.
//!
//! Built once per index pass from the project's Java sources, its bean XMLs and its
//! property files; every editor query then answers against this plus a fresh parse of the
//! buffer the user is actually looking at. That split matters: the model is
//! project-shaped and rebuilt on a schedule, the buffer is unsaved text that changes on
//! every keystroke, and pretending the first can answer for the second is how an editor
//! ends up navigating to where a symbol *used to be*.

use std::collections::BTreeMap;

use crate::props::PropertySources;
use crate::xml::XmlBeanFile;

/// How a bean came to exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeanKind {
    /// A stereotype annotation on a class (`@Service`, `@Component`, …).
    Stereotype,
    /// A `@Bean` factory method inside a `@Configuration` class.
    Factory,
    /// A `<bean>` element in a Spring XML.
    Xml,
}

impl BeanKind {
    pub fn as_str(self) -> &'static str {
        match self {
            BeanKind::Stereotype => "stereotype",
            BeanKind::Factory => "factory",
            BeanKind::Xml => "xml",
        }
    }
}

/// One `@ConditionalOn…` on a bean — the reason it might not exist.
///
/// Worth modelling rather than ignoring: in a codebase that leans on injection to abstract, a
/// bean list that says "these exist" while half of them are behind a property flag is describing
/// a context nobody ever builds. A conditional bean should say so, and its condition should be
/// something you can read the current value of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeanCondition {
    /// The annotation's simple name (`ConditionalOnProperty`).
    pub name: String,
    /// A one-line reading of it (`app.feature.enabled = true`, `bean DataSource is present`).
    pub summary: String,
    /// The property key it tests, when it tests one — so hovering it can say whether the
    /// condition currently holds.
    pub property_key: String,
}

/// One bean definition, from any of the three sources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeanDef {
    /// The bean name Spring registers it under — the explicit id, else the convention
    /// (decapitalized simple class name / the factory method's name).
    pub name: String,
    /// Dotted implementation FQCN. Empty for an XML bean that inherits its class from a
    /// `parent` we could not follow.
    pub fqcn: String,
    pub kind: BeanKind,
    /// What was written (`@Service`, `@Bean`, `<bean>`) — the badge in the panel.
    pub stereotype: String,
    /// Declaration site, forward-slashed.
    pub file: String,
    /// Byte offset of the declaring name — where go-to lands.
    pub offset: usize,
    /// 1-based line of the declaration.
    pub line: u32,
    /// `@Scope` / `scope=` when written; empty means singleton.
    pub scope: String,
    pub primary: bool,
    /// `@Profile` / the XML `<beans profile=>`, empty when unconditional.
    pub profile: String,
    pub lazy: bool,
    /// An `abstract="true"` XML template — never instantiated, so it is not a candidate
    /// for injection even though it is a definition.
    pub is_abstract: bool,
    /// Supertypes as written (`extends` + `implements`, simple or qualified) — what an
    /// injection point of an interface type is matched against.
    pub supertypes: Vec<String>,
    /// The `@ConditionalOn…` annotations gating it. Empty for an unconditional bean.
    pub conditions: Vec<BeanCondition>,
}

/// One parameter of a handler method, with what it binds to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointParam {
    /// The parameter's name in the source.
    pub name: String,
    pub type_text: String,
    /// Where the value comes from: `path` | `query` | `body` | `header` | `cookie` | `part` |
    /// `model` | `arg` (an injected infrastructure argument — `HttpServletRequest`, `Model`).
    pub binding: String,
    /// The name it binds under when the annotation names one different from the parameter
    /// (`@RequestParam("q") String query`). Empty when they agree.
    pub bound_name: String,
    /// `false` only when the annotation says `required = false`.
    pub required: bool,
}

impl EndpointParam {
    /// The name this parameter is addressed by — the annotation's, else the parameter's.
    pub fn effective_name(&self) -> &str {
        if self.bound_name.is_empty() {
            &self.name
        } else {
            &self.bound_name
        }
    }
}

/// A request-mapped handler method.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    /// HTTP methods; empty means the mapping accepts all of them.
    pub methods: Vec<String>,
    /// The full path — the controller's class-level mapping joined with the method's.
    pub path: String,
    pub class_fqcn: String,
    /// The handler method's name.
    pub handler: String,
    pub file: String,
    pub offset: usize,
    pub line: u32,
    /// The `{name}` template variables in [`Self::path`], in order.
    pub path_vars: Vec<String>,
    /// `produces = …` when written.
    pub produces: String,
    /// The handler's return type as written (`ResponseEntity<Order>`, `void`).
    pub return_type: String,
    /// Its parameters, in declaration order.
    pub params: Vec<EndpointParam>,
}

impl Endpoint {
    /// `GET /orders/{id}` — the label a panel and a URL search both key on.
    pub fn label(&self) -> String {
        let verb = if self.methods.is_empty() { "ANY".to_string() } else { self.methods.join("|") };
        format!("{verb} {}", self.path)
    }
}

/// Where a dependency is injected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InjectionKind {
    Field,
    Constructor,
    Setter,
}

impl InjectionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            InjectionKind::Field => "field",
            InjectionKind::Constructor => "constructor",
            InjectionKind::Setter => "setter",
        }
    }
}

/// One place a bean is asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InjectionPoint {
    pub owner_fqcn: String,
    /// Field / parameter name.
    pub member: String,
    /// Declared type as written (`OrderRepository`, `List<Handler>`).
    pub type_text: String,
    /// `@Qualifier("…")` when written, else empty.
    pub qualifier: String,
    pub kind: InjectionKind,
    pub file: String,
    pub offset: usize,
    pub line: u32,
}

/// What we know about a project type — enough to check an XML `<property name=>` against
/// it without pretending to be the Java resolver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeInfo {
    pub fqcn: String,
    pub file: String,
    pub offset: usize,
    pub line: u32,
    /// Writable property names (`setFoo` → `foo`, public fields, Lombok-generated ones).
    pub properties: Vec<String>,
    /// Byte offset of each property's declaring member, parallel to [`Self::properties`].
    pub property_offsets: Vec<usize>,
    /// Whether [`Self::properties`] can be trusted as **complete**.
    ///
    /// False when the type extends something outside the scan, or carries a Lombok
    /// annotation whose generated members we did not model. A `<property name=>` check
    /// runs only when this is true — an incomplete list would turn "I don't know" into
    /// "that property doesn't exist", which is the one thing this crate must never say.
    pub properties_complete: bool,
}

/// One `@ConfigurationProperties`-bound field, with the **full** key it binds.
///
/// The path is the interesting part and the reason this exists: a field called `timeout` on a
/// class three nesting levels below the root binds `app.http.client.timeout`, and that string
/// appears nowhere in the source — you assemble it in your head from the prefix, the field
/// names between here and the root, and Spring's relaxed-binding rules, every time you need to
/// write it in a yaml.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigBinding {
    /// Dotted FQCN of the type declaring the field.
    pub owner_fqcn: String,
    /// The field name in the source.
    pub field: String,
    /// The full canonical key (`app.http.client.timeout`, `app.clients.<key>.url`).
    pub path: String,
    /// The field's declared type, as written.
    pub type_text: String,
    /// The prefix of the `@ConfigurationProperties` root this path starts from.
    pub root_prefix: String,
    /// Where the field is declared — so a key in a yaml can navigate *to* it.
    pub file: String,
    /// Byte offset of the field's name.
    pub offset: usize,
}

/// One place a configuration key is read.
///
/// The reverse of everything else here: the model normally answers "what does this Java thing
/// bind", and this answers "who reads this key" — which is the question you have when you are
/// looking at a yaml wondering whether a line still matters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyUsage {
    pub key: String,
    /// Absolute path, forward-slashed.
    pub file: String,
    /// Byte offset to jump to.
    pub offset: usize,
    /// How it is read (`@Value`, `@ConditionalOnProperty`, `@ConfigurationProperties`, `<bean>`).
    pub kind: String,
    /// What to call it in a picker (`OrderService.timeout`).
    pub label: String,
    /// The Java type the reader declares (`int`, `java.time.Duration`, `List<String>`).
    ///
    /// This is how a property in a yaml gets a type at all: the file itself says nothing —
    /// `30` could be a number, a string or a duration — but the field it is injected into
    /// says exactly. Empty when the reader has no single type (a placeholder inside an XML
    /// value, a key named by `@ConditionalOnProperty`).
    pub type_text: String,
}

/// Everything the extension knows about a project.
#[derive(Debug, Default)]
pub struct SpringModel {
    pub beans: Vec<BeanDef>,
    pub endpoints: Vec<Endpoint>,
    pub injections: Vec<InjectionPoint>,
    pub props: PropertySources,
    /// What Spring and the project's libraries say their own properties are — parsed from the
    /// `spring-configuration-metadata.json` the host reads out of the dependency jars, with a
    /// curated table standing in until those arrive. See [`crate::metadata`].
    pub metadata: crate::metadata::MetadataIndex,
    /// The parsed Spring bean XMLs, kept for their spans (go-to, validation).
    pub xml_files: Vec<XmlBeanFile>,
    /// Project types by dotted FQCN.
    pub types: BTreeMap<String, TypeInfo>,
    /// Simple name → every FQCN declaring it (a name can be declared in two packages).
    pub simple_names: BTreeMap<String, Vec<String>>,
    /// Every `@ConfigurationProperties`-bound field and the key it binds.
    pub config_bindings: Vec<ConfigBinding>,
    /// Every place a configuration key is read.
    pub property_usages: Vec<PropertyUsage>,
}

impl SpringModel {
    /// The bean registered under `name`.
    pub fn bean(&self, name: &str) -> Option<&BeanDef> {
        self.beans.iter().find(|b| b.name == name)
    }

    /// Whether any bean is registered under `name`.
    pub fn has_bean(&self, name: &str) -> bool {
        self.beans.iter().any(|b| b.name == name)
    }

    /// Beans that could satisfy an injection point of `type_text`, honouring a qualifier.
    ///
    /// Matching is by **simple name** against the bean's own class and its declared
    /// supertypes. That is looser than the Java resolver would be — two same-named classes
    /// in different packages both match — and deliberately so: this drives navigation, a
    /// picker the user reads, not a diagnostic. Being too generous costs an extra row;
    /// being too strict costs the feature.
    pub fn candidates(&self, type_text: &str, qualifier: &str) -> Vec<&BeanDef> {
        let wanted_owned = injected_type(type_text);
        let wanted = wanted_owned.as_str();
        if wanted.is_empty() {
            return Vec::new();
        }
        let mut out: Vec<&BeanDef> = self
            .beans
            .iter()
            .filter(|b| !b.is_abstract)
            .filter(|b| {
                simple_name(&b.fqcn) == wanted
                    || b.supertypes.iter().any(|s| simple_name(&strip_generics(s)) == wanted)
            })
            .collect();
        if !qualifier.is_empty() {
            // A qualifier is exact: if it names one of the candidates, it IS the answer.
            if let Some(exact) = out.iter().find(|b| b.name == qualifier) {
                return vec![*exact];
            }
        }
        // `@Primary` first, then declaration order — the order Spring would pick in.
        out.sort_by_key(|b| !b.primary);
        out
    }

    /// The type declaring `fqcn`, or the unique one whose simple name matches.
    pub fn type_of(&self, name: &str) -> Option<&TypeInfo> {
        if let Some(t) = self.types.get(name) {
            return Some(t);
        }
        match self.simple_names.get(simple_name(name)) {
            Some(fqcns) if fqcns.len() == 1 => self.types.get(&fqcns[0]),
            _ => None,
        }
    }

    /// Every key `owner_fqcn.field` binds. More than one when the declaring type is reached
    /// from several `@ConfigurationProperties` roots, which is legitimate and worth showing
    /// in full rather than picking one.
    pub fn config_bindings_for(&self, owner_fqcn: &str, field: &str) -> Vec<&ConfigBinding> {
        self.config_bindings
            .iter()
            .filter(|b| b.owner_fqcn == owner_fqcn && b.field == field)
            .collect()
    }

    /// Everywhere `key` is read.
    ///
    /// Exact-match only. Relaxed binding means `app.readTimeout` and `app.read-timeout` are the
    /// same key to Spring, so both spellings are normalised to the canonical one when the index
    /// is built — comparing them here would be the wrong place to do it.
    pub fn usages_of(&self, key: &str) -> Vec<&PropertyUsage> {
        self.property_usages.iter().filter(|u| u.key == key).collect()
    }

    /// Every endpoint whose path or verb matches `query` loosely — the URL navigator.
    pub fn find_endpoints(&self, query: &str) -> Vec<&Endpoint> {
        let q = query.trim().to_ascii_lowercase();
        if q.is_empty() {
            return self.endpoints.iter().collect();
        }
        self.endpoints.iter().filter(|e| e.label().to_ascii_lowercase().contains(&q)).collect()
    }
}

/// Wrappers whose FIRST type argument is what an injection point is really asking for.
///
/// `@Autowired List<OrderService> all` injects every `OrderService` bean — the collection is the
/// shape of the answer, not the type being asked for. Matching on `List` matched nothing, which
/// is the one injection style that has no other way to be resolved.
const ELEMENT_WRAPPERS: &[&str] =
    &["List", "Set", "Collection", "Stream", "Optional", "ObjectProvider", "Provider"];

/// The type an injection point asks for: the element of a collection or provider, the VALUE of a
/// `Map` (Spring keys those by bean name), the type itself otherwise. Always a simple name —
/// which is what [`SpringModel::candidates`] compares.
pub fn injected_type(type_text: &str) -> String {
    let outer = simple_name(&strip_generics(type_text)).to_string();
    let arg = |n: usize| type_argument(type_text, n).map(|a| simple_name(&strip_generics(&a)).to_string());
    let inner = match outer.as_str() {
        w if ELEMENT_WRAPPERS.contains(&w) => arg(0),
        "Map" => arg(1),
        _ => None,
    };
    inner.filter(|s| !s.is_empty()).unwrap_or(outer)
}

/// The `n`th type argument of `Foo<A, B>`, respecting nesting. `None` when there is none.
pub fn type_argument(type_text: &str, n: usize) -> Option<String> {
    let open = type_text.find('<')?;
    let close = type_text.rfind('>')?;
    if close <= open + 1 {
        return None;
    }
    let mut depth = 0usize;
    let mut current = String::new();
    let mut args: Vec<String> = Vec::new();
    for ch in type_text[open + 1..close].chars() {
        match ch {
            '<' => {
                depth += 1;
                current.push(ch);
            }
            '>' => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            ',' if depth == 0 => args.push(std::mem::take(&mut current)),
            _ => current.push(ch),
        }
    }
    args.push(current);
    args.get(n).map(|a| a.trim().to_string()).filter(|a| !a.is_empty())
}

/// The last dotted segment of a name (`com.acme.Foo` → `Foo`), and the whole string when
/// there is no dot.
pub fn simple_name(fqcn: &str) -> &str {
    fqcn.rsplit('.').next().unwrap_or(fqcn)
}

/// Drop a generic argument list and array brackets: `List<Foo>` → `List`, `Foo[]` → `Foo`.
pub fn strip_generics(type_text: &str) -> String {
    let head = type_text.split('<').next().unwrap_or(type_text);
    head.trim().trim_end_matches("[]").trim().to_string()
}

/// The 1-based line containing `offset`.
pub fn line_at(text: &str, offset: usize) -> u32 {
    let end = offset.min(text.len());
    text.as_bytes()[..end].iter().filter(|&&c| c == b'\n').count() as u32 + 1
}

/// The `{name}` template variables of a mapping path, in order. `{id:\\d+}` yields `id` —
/// Spring allows a regex after a colon and the variable name is what precedes it.
pub fn path_variables(path: &str) -> Vec<String> {
    let mut out = Vec::new();
    let b = path.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'{' {
            i += 1;
            continue;
        }
        let mut depth = 1;
        let start = i + 1;
        let mut j = start;
        while j < b.len() && depth > 0 {
            match b[j] {
                b'{' => depth += 1,
                b'}' => depth -= 1,
                _ => {}
            }
            j += 1;
        }
        if depth == 0 {
            let inner = &path[start..j - 1];
            let name = inner.split(':').next().unwrap_or(inner).trim();
            if !name.is_empty() {
                out.push(name.to_string());
            }
        }
        i = j.max(i + 1);
    }
    out
}

/// Join a class-level mapping with a method-level one, the way Spring does: exactly one
/// slash between them, and a bare class mapping when the method adds nothing.
pub fn join_paths(class_path: &str, method_path: &str) -> String {
    let a = class_path.trim().trim_end_matches('/');
    let b = method_path.trim();
    if b.is_empty() || b == "/" {
        return if a.is_empty() { "/".to_string() } else { a.to_string() };
    }
    let b = b.trim_start_matches('/');
    if a.is_empty() {
        format!("/{b}")
    } else {
        format!("{a}/{b}")
    }
}

/// A field name in the **canonical** form a configuration key is written in: lower-case, words
/// separated by `-` (`maxPoolSize` → `max-pool-size`).
///
/// Spring's relaxed binding accepts the camelCase spelling too, so this is a presentation
/// choice — but it is the one the reference documentation and the metadata files use, and
/// showing two spellings of the same key would be worse than picking the canonical one.
pub fn canonical_key_segment(field: &str) -> String {
    let mut out = String::with_capacity(field.len() + 4);
    for (i, c) in field.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                out.push('-');
            }
            out.extend(c.to_lowercase());
        } else if c == '_' {
            out.push('-');
        } else {
            out.push(c);
        }
    }
    out
}

/// The bean name Spring derives from a class name when none is given: the simple name with
/// its first letter lowercased — *unless* the first two are both capitals, in which case
/// it is left alone (`URLService` stays `URLService`, per `java.beans.Introspector`).
pub fn default_bean_name(fqcn: &str) -> String {
    let simple = simple_name(fqcn);
    let mut chars = simple.chars();
    let Some(first) = chars.next() else { return String::new() };
    if simple.chars().nth(1).is_some_and(|c| c.is_uppercase()) {
        return simple.to_string();
    }
    first.to_lowercase().chain(chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bean_name_convention_matches_the_introspector_rule() {
        assert_eq!(default_bean_name("com.acme.OrderService"), "orderService");
        assert_eq!(default_bean_name("com.acme.URLService"), "URLService", "two capitals: verbatim");
        assert_eq!(default_bean_name("A"), "a");
        assert_eq!(default_bean_name(""), "");
    }

    #[test]
    fn paths_join_with_exactly_one_slash() {
        assert_eq!(join_paths("/orders", "/{id}"), "/orders/{id}");
        assert_eq!(join_paths("/orders/", "{id}"), "/orders/{id}");
        assert_eq!(join_paths("/orders", ""), "/orders");
        assert_eq!(join_paths("", "/list"), "/list");
        assert_eq!(join_paths("", ""), "/");
        assert_eq!(join_paths("/orders", "/"), "/orders");
    }

    #[test]
    fn path_variables_survive_a_regex_constraint() {
        assert_eq!(path_variables("/orders/{id}/items/{itemId}"), ["id", "itemId"]);
        assert_eq!(path_variables("/f/{id:[0-9]+}"), ["id"]);
        assert!(path_variables("/static/path").is_empty());
        assert!(path_variables("/unclosed/{id").is_empty(), "an unclosed brace names nothing");
    }

    #[test]
    fn type_text_reduces_to_the_matchable_name() {
        assert_eq!(strip_generics("List<Foo>"), "List");
        assert_eq!(strip_generics("Foo[]"), "Foo");
        assert_eq!(simple_name("com.acme.Foo"), "Foo");
        assert_eq!(simple_name("Foo"), "Foo");
    }

    #[test]
    fn line_at_counts_from_one() {
        let t = "a\nb\nc";
        assert_eq!(line_at(t, 0), 1);
        assert_eq!(line_at(t, 2), 2);
        assert_eq!(line_at(t, 4), 3);
        assert_eq!(line_at(t, 9999), 3, "past the end clamps rather than panics");
    }

    fn bean(name: &str, fqcn: &str, supers: &[&str]) -> BeanDef {
        BeanDef {
            name: name.to_string(),
            fqcn: fqcn.to_string(),
            kind: BeanKind::Stereotype,
            stereotype: "@Service".to_string(),
            file: "/p/X.java".to_string(),
            offset: 0,
            line: 1,
            scope: String::new(),
            primary: false,
            profile: String::new(),
            lazy: false,
            is_abstract: false,
            supertypes: supers.iter().map(|s| s.to_string()).collect(),
            conditions: Vec::new(),
        }
    }

    #[test]
    fn candidates_match_the_implemented_interface() {
        let mut m = SpringModel::default();
        m.beans = vec![
            bean("orderServiceImpl", "com.acme.OrderServiceImpl", &["OrderService"]),
            bean("other", "com.acme.Unrelated", &[]),
        ];
        let c = m.candidates("OrderService", "");
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].name, "orderServiceImpl");
        assert!(m.candidates("List<OrderService>", "")[0].name == "orderServiceImpl");
    }

    #[test]
    fn a_qualifier_that_names_a_candidate_settles_it() {
        let mut m = SpringModel::default();
        m.beans = vec![
            bean("fast", "com.acme.FastImpl", &["Engine"]),
            bean("slow", "com.acme.SlowImpl", &["Engine"]),
        ];
        assert_eq!(m.candidates("Engine", "").len(), 2);
        let picked = m.candidates("Engine", "slow");
        assert_eq!(picked.len(), 1);
        assert_eq!(picked[0].name, "slow");
    }

    #[test]
    fn primary_is_offered_first() {
        let mut m = SpringModel::default();
        let mut p = bean("second", "com.acme.Second", &["Engine"]);
        p.primary = true;
        m.beans = vec![bean("first", "com.acme.First", &["Engine"]), p];
        assert_eq!(m.candidates("Engine", "")[0].name, "second");
    }

    #[test]
    fn an_abstract_xml_template_is_never_a_candidate() {
        let mut m = SpringModel::default();
        let mut t = bean("base", "com.acme.Base", &["Engine"]);
        t.is_abstract = true;
        m.beans = vec![t];
        assert!(m.candidates("Engine", "").is_empty());
    }

    #[test]
    fn canonical_key_segments_are_kebab_case() {
        assert_eq!(canonical_key_segment("maxPoolSize"), "max-pool-size");
        assert_eq!(canonical_key_segment("url"), "url");
        assert_eq!(canonical_key_segment("read_timeout"), "read-timeout");
        // A leading capital must not produce a leading dash.
        assert_eq!(canonical_key_segment("URL"), "u-r-l");
        assert_eq!(canonical_key_segment(""), "");
    }

    #[test]
    fn endpoint_label_reads_like_a_route() {
        let e = Endpoint {
            methods: vec!["GET".into()],
            path: "/orders/{id}".into(),
            class_fqcn: "c.A".into(),
            handler: "get".into(),
            file: "/p/A.java".into(),
            offset: 0,
            line: 1,
            path_vars: vec!["id".into()],
            produces: String::new(),
            return_type: "String".into(),
            params: Vec::new(),
        };
        assert_eq!(e.label(), "GET /orders/{id}");
        let any = Endpoint { methods: vec![], ..e };
        assert_eq!(any.label(), "ANY /orders/{id}");
    }
}
