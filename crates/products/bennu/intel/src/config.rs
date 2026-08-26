//! Config-graph INTEGRATION: ingest the [`bennu_web`] Struts/Spring/Tiles graph into
//! the [`bennu_index`] symbol + relation stores, then resolve the load-bearing chains
//! and the conservative "action inesistente" diagnostic off those edges.
//!
//! The parser (`bennu-web`) emits **string-keyed** records (action qualified names,
//! Spring bean-ids, Tiles def names, JSP paths); this module assigns the `u32` ids and
//! writes:
//!   - one [`Symbol`] per action (`Source::StrutsAction`) and per bean
//!     (`Source::SpringBean`), reachable in the fst under its qualified name / bean-id;
//!   - one [`Relation`] per resolvable config edge (`ActionToClass`, `ResultToView`, …),
//!     keyed by `from_id` in the relation store.
//!
//! The load-bearing chain the FE needs to navigate JSP action references (docs §10 C1):
//!
//! ```text
//!   JSP action name  →(struts)  <action class=beanId>  →(spring)  real FQCN  →(index) members
//!   <action>  →  <result type=tiles>  →  Tiles <definition>  →  JSP template
//! ```
//!
//! Wildcards (`*` in an action name, `{1}` backrefs) and Tiles indirection are
//! pervasive → those edges are CANDIDATE ([`Relation::inferred`]) and the diagnostic
//! NEVER returns an exact "missing" when a wildcard/computed path could match (docs §8).

use std::collections::HashMap;
use std::path::Path;

use crate::spring_beans::AnnotationBean;
use bennu_index::prelude::{
    serialize_symbol, BlobWriter, Relation, RelationKind, RelationReader, RelationWriter, Source,
    StoreError, Symbol, SymbolKind,
};
use bennu_web::prelude::{
    interceptor_usages, join_ns, methods_for_mapper, normalize_action_ref, resolve_action_view,
    resolve_bean_map, resolve_interceptor_ref, statement_for_method, validations_for_class,
    InterceptorDef, InterceptorRefUse, RelKind, StatementRecord, StatementTarget, ValidationRecord,
    WebConfigGraph, WildcardPattern,
};

/// File names of the four config-index files under a project's index dir.
pub const CONFIG_SYMBOL_BLOB: &str = "config-symbols.blob";
pub const CONFIG_SYMBOL_FST: &str = "config-symbols.fst";
pub const CONFIG_REL_BLOB: &str = "config-relations.blob";
pub const CONFIG_REL_FST: &str = "config-relations.fst";

/// The verdict of the "action inesistente" diagnostic for one JSP action reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionVerdict {
    /// Exactly one concrete `<action>` with this qualified name exists.
    Exists,
    /// No concrete action AND no wildcard/computed path could match → a dangling ref.
    Missing,
    /// A wildcard pattern (or a computed / OGNL path) could match → NEVER "missing".
    Inconclusive { reason: String },
}

/// Persist the config-graph into the symbol + relation stores under `index_dir`, then
/// return a [`ConfigResolver`] over them. The action/bean records get `u32` ids assigned
/// here; the relations resolve their string endpoints to those ids.
///
/// `annotation_beans` are the Spring stereotype-declared beans (`@Service`/`@Component`/…)
/// collected from the project's Java symbols (Option B: kept in a SEPARATE map here, never
/// mixed into the pure-XML `graph.beans`). They feed the C1 fallback in
/// [`ConfigResolver::resolve_action_class`] for annotation-based apps whose bean ids aren't
/// declared in any XML `<bean>`.
pub fn ingest_config_graph(
    graph: &WebConfigGraph,
    index_dir: &Path,
    annotation_beans: &[AnnotationBean],
) -> Result<ConfigResolver, StoreError> {
    let mut action_ids: HashMap<String, u32> = HashMap::new();
    let mut bean_ids: HashMap<String, u32> = HashMap::new();
    let mut symbols: HashMap<u32, Symbol> = HashMap::new();
    let mut sw = BlobWriter::new();
    let mut next_id: u32 = 0;

    // 1) Actions → StrutsAction symbols, keyed by their qualified name.
    for a in &graph.actions {
        let id = next_id;
        next_id += 1;
        let sym = Symbol {
            id,
            kind: SymbolKind::Class, // config-as-symbol; a coarse kind tag here
            simple_name: a.name.clone(),
            fqn: a.qualified_name.clone(),
            owner_id: u32::MAX,
            source: Source::StrutsAction,
            signature: format!("action {} class={}", a.qualified_name, a.class_ref),
            modifiers: if a.is_wildcard {
                "wildcard".into()
            } else {
                String::new()
            },
            loc_file: a.source_file.clone(),
            // The `<action>` element offset → go-to lands on the declaration, not line 1.
            loc_start: a.decl_offset as u32,
            loc_end: a.decl_offset as u32,
            loc_container: String::new(),
            loc_class: String::new(),
            members_json: String::new(),
        };
        let bytes = serialize_symbol(&sym).map_err(StoreError::Io)?;
        sw.append(&a.qualified_name, &bytes);
        action_ids.insert(a.qualified_name.clone(), id);
        symbols.insert(id, sym);
    }

    // 2) Beans → SpringBean symbols, keyed by their id. The impl FQCN lives in `fqn`.
    for b in &graph.beans {
        let id = next_id;
        next_id += 1;
        let sym = Symbol {
            id,
            kind: SymbolKind::Class,
            simple_name: b.id.clone(),
            fqn: b.class.clone(),
            owner_id: u32::MAX,
            source: Source::SpringBean,
            signature: format!("bean {} -> {}", b.id, b.class),
            modifiers: String::new(),
            loc_file: b.source_file.clone(),
            loc_start: 0,
            loc_end: 0,
            loc_container: String::new(),
            loc_class: String::new(),
            members_json: String::new(),
        };
        let bytes = serialize_symbol(&sym).map_err(StoreError::Io)?;
        sw.append(&b.id, &bytes);
        bean_ids.insert(b.id.clone(), id);
        symbols.insert(id, sym);
    }
    sw.finish(
        &index_dir.join(CONFIG_SYMBOL_BLOB),
        &index_dir.join(CONFIG_SYMBOL_FST),
    )?;

    // 3) Relations → resolve each endpoint to a symbol id where it names a known
    //    action/bean. `ActionToClass` (action → bean) is the load-bearing C1 edge; the
    //    result/view endpoints are synthetic ("<action>#<result>" / Tiles-def names),
    //    kept in the graph rather than the id store (a later JSP-symbol wave gives views
    //    real ids). `BeanIdToImpl`'s `to` is an FQCN — the impl lives in the bean symbol.
    let mut rw = RelationWriter::new();
    let mut relations: Vec<Relation> = Vec::new();
    for r in &graph.relations {
        let resolved = match r.kind {
            RelKind::ActionToClass => (
                action_ids.get(&r.from).copied(),
                bean_ids.get(&r.to).copied(),
            ),
            RelKind::BeanIdToImpl => (bean_ids.get(&r.from).copied(), Some(u32::MAX)),
            RelKind::ActionToResult | RelKind::ResultToView => {
                (action_ids.get(&r.from).copied(), None)
            }
            // Interceptor + MyBatis edges are resolved off the parsed graph (like Tiles),
            // not the id store: interceptor/stack names AND statement ids are package-scoped,
            // so they get no global fst symbol (a statement id like `findById` isn't globally
            // unique). `resolve_interceptor_ref` / `statement_for_method` answer over the graph.
            RelKind::InterceptorRefToDef
            | RelKind::InterceptorToClass
            | RelKind::MethodToStatement => (None, None),
        };
        if let (Some(from_id), Some(to_id)) = resolved {
            let rel = Relation {
                from_id,
                to_id,
                kind: r.kind.into_index(),
                source: match r.kind {
                    RelKind::BeanIdToImpl => Source::SpringBean,
                    _ => Source::StrutsAction,
                },
                inferred: r.inferred,
            };
            rw.add(rel.clone());
            relations.push(rel);
        }
    }
    rw.finish(
        &index_dir.join(CONFIG_REL_BLOB),
        &index_dir.join(CONFIG_REL_FST),
    )?;

    // 4) Compile wildcard patterns for the conservative diagnostic (namespace kept so a
    //    reference is matched only within the wildcard action's own namespace).
    let wildcards = graph
        .actions
        .iter()
        .filter(|a| a.is_wildcard)
        .map(|a| (WildcardPattern::compile(&a.name), a.namespace.clone()))
        .collect();

    // The annotation-bean fallback map (Option B): name → bean, kept separate from the
    // pure-XML graph. Last-writer-wins on a duplicate name (two `@Service("x")` collide) —
    // a rare, undefined-in-Spring case not worth failing the whole ingest over.
    let annotation_beans = annotation_beans
        .iter()
        .map(|b| (b.name.clone(), b.clone()))
        .collect();

    Ok(ConfigResolver {
        graph: graph.clone(),
        action_ids,
        bean_ids,
        symbols,
        relations,
        wildcards,
        annotation_beans,
    })
}

/// Resolves config-graph questions off the ingested symbols + edges: the C1 chain
/// (action → bean-id → FQCN), the view chain (action → Tiles def → JSP), and the
/// conservative action-existence diagnostic.
///
/// It keeps the parsed [`WebConfigGraph`] alongside the id maps: the ids are the fast
/// edge path, the graph answers the inferred/candidate questions (wildcard matching, the
/// Tiles-def→JSP indirection) the raw ids can't.
pub struct ConfigResolver {
    graph: WebConfigGraph,
    action_ids: HashMap<String, u32>,
    bean_ids: HashMap<String, u32>,
    symbols: HashMap<u32, Symbol>,
    relations: Vec<Relation>,
    wildcards: Vec<(WildcardPattern, String)>,
    /// Annotation-declared Spring beans (`@Service`/`@Component`/…), keyed by bean name.
    /// The C1 fallback (docs §10) for annotation-based apps: an XML `<bean>` isn't declared
    /// for the id, so the id resolves against a stereotype-annotated class instead. Kept
    /// SEPARATE from the pure-XML `graph.beans` (Option B) — fed from the Java symbols at
    /// ingest, never mixed into bennu-web's parse.
    annotation_beans: HashMap<String, AnnotationBean>,
}

impl ConfigResolver {
    /// The C1 chain over the ingested `ActionToClass` edge: action qualified-name →
    /// bean-id → impl FQCN (the bean symbol's `fqn`, resolving the Spring parent chain
    /// if the bean has no own class). `None` when the bean-id lives in a dependency jar
    /// (unknown) or the action declares no class.
    pub fn resolve_action_class(&self, action_qname: &str) -> Option<String> {
        let key = self.canonical_action_key(action_qname)?;
        if let Some(fqcn) = self.resolve_action_class_xml(&key) {
            return Some(fqcn);
        }
        // C1 fallback (docs §10): the XML `<bean>`s don't name this id → resolve the action's
        // raw `class=` bean id against the annotation-declared beans (`@Service("foo")`, or a
        // bare `@Service` on `FooService` → `fooService`). Lights up JSP action go-to→class in
        // annotation-based apps with zero XML `<bean>`s for the id.
        let class_ref = self.action_class_ref_id(&key)?;
        self.annotation_beans.get(class_ref).map(|b| b.fqcn.clone())
    }

    /// The XML-only C1 resolution (the original path): action → `ActionToClass` edge → bean
    /// symbol `fqn`, else the Spring parent chain. `None` when the id isn't an XML `<bean>`.
    fn resolve_action_class_xml(&self, key: &str) -> Option<String> {
        let action_id = *self.action_ids.get(key)?;
        let bean_id = self
            .relations
            .iter()
            .find(|r| r.from_id == action_id && r.kind == RelationKind::ActionToClass)
            .map(|r| r.to_id)?;
        let bean_sym = self.symbols.get(&bean_id)?;
        if !bean_sym.fqn.is_empty() {
            return Some(bean_sym.fqn.clone());
        }
        // Bean declares only `parent=` → walk the Spring parent chain via the shared map.
        resolve_bean_map(&self.graph.beans)
            .get(&bean_sym.simple_name)
            .cloned()
    }

    /// The raw `class=` attribute (the Spring bean id) of the action with the given
    /// canonicalized qualified name, off the parsed graph. `None` when the action isn't in
    /// the graph or declares no `class=`.
    fn action_class_ref_id(&self, qname: &str) -> Option<&str> {
        self.graph
            .actions
            .iter()
            .find(|a| a.qualified_name == qname)
            .map(|a| a.class_ref.as_str())
            .filter(|r| !r.is_empty())
    }

    /// The view chain: action → `<result type=tiles>` def → JSP. Answered off the parsed
    /// graph (the Tiles-def→JSP indirection isn't a symbol edge yet).
    pub fn resolve_action_view(&self, action_qname: &str) -> Option<String> {
        let key = self.canonical_action_key(action_qname)?;
        resolve_action_view(&self.graph, &key)
    }

    /// The actions whose resolved result **view** is the JSP at `jsp_file` — the REVERSE of
    /// [`resolve_action_view`]. For a view-only JSP (OGNL, no `<form>`) this is how we discover which
    /// action's properties the page reads. `jsp_file` is the absolute path being edited; an action's
    /// view (`/WEB-INF/x/y.jsp`) matches when it's a segment-aligned suffix of it. Returns
    /// `(action_qname, class_fqcn?)` de-duplicated by qname, in graph order. O(actions) — the caller
    /// fetches it per JSP (dropdown / lint), not per keystroke.
    pub fn actions_for_view(&self, jsp_file: &str) -> Vec<(String, Option<String>)> {
        let needle = jsp_file.replace('\\', "/");
        let mut out: Vec<(String, Option<String>)> = Vec::new();
        // Iterate RESULTS (not just tiles-resolved views): a result is either a `<result type="tiles">`
        // (resolve through the Tiles chain) or a DIRECT dispatcher `<result>/WEB-INF/x.jsp</result>`
        // whose target IS the JSP — the common legacy shape `resolve_action_view` deliberately skips.
        for r in &self.graph.results {
            let qname = &r.action_qualified_name;
            if out.iter().any(|(q, _)| q == qname) {
                continue; // this action already matched via one of its other results
            }
            let view: Option<String> = if r.result_type == "tiles" {
                self.resolve_action_view(qname)
            } else if is_jsp_path(&r.target) {
                Some(r.target.clone())
            } else {
                None // a chain / redirect / redirectAction target is another action, not a view
            };
            let Some(view) = view else { continue };
            if view_path_matches(&needle, &view) {
                out.push((qname.clone(), self.resolve_action_class(qname)));
            }
        }
        out
    }

    /// The conservative "action inesistente" diagnostic (docs §8). NEVER returns
    /// [`ActionVerdict::Missing`] when a wildcard pattern or a computed/OGNL path could
    /// match the reference. Stays STRICT (no unambiguous-suffix guessing like the go-to
    /// path): only a trailing `.action`/`.do`/query the editor may pass verbatim is stripped
    /// before the exact check, so a genuinely-dangling absolute ref is still `Missing`.
    pub fn diagnose_action(&self, action_qname: &str) -> ActionVerdict {
        // Suffix/query-normalize the raw ref for the wildcard check below.
        let norm = normalize_action_ref(action_qname).unwrap_or_else(|| action_qname.to_string());
        // EXISTS iff the tolerant go-to resolution binds it (docs: NEVER a false positive). Being
        // STRICTER than go-to here was the bug: an Entando `/ExtStr2/do/…/prevQC.action` URL that
        // Ctrl+B resolves fine (the `/ExtStr2` servlet prefix dropped, `.action` stripped) was flagged
        // "action does not exist". `canonical_action_key` covers exact + normalized + servlet-prefix
        // suffix + unique-trailing-name, so anything go-to can reach is `Exists` here too. Its loosest
        // step (unique trailing name) can only cause a false NEGATIVE (a dangling ref slipping through),
        // never a false positive — the correct trade for a squiggle.
        if self.canonical_action_key(action_qname).is_some() {
            return ActionVerdict::Exists;
        }
        // Could a wildcard action's pattern match this reference (within its namespace)?
        for (pat, ns) in &self.wildcards {
            if let Some(candidate) = strip_ns(&norm, ns) {
                if pat.matches(candidate) {
                    return ActionVerdict::Inconclusive {
                        reason: format!("matches wildcard `{}` in ns `{}`", pat.raw, ns),
                    };
                }
            }
        }
        // A computed action name (OGNL `%{…}` or a `{n}` backref) is never a hard miss.
        if action_qname.contains("%{") || action_qname.contains('{') {
            return ActionVerdict::Inconclusive {
                reason: "computed action name (OGNL / backref)".into(),
            };
        }
        ActionVerdict::Missing
    }

    /// Resolve a JSP form/link action reference to its target location: the config
    /// fragment + the class FQCN it maps to (for go-to-definition). `None` when the
    /// action is unknown.
    pub fn action_class_ref(&self, action_qname: &str) -> Option<ActionTarget> {
        let key = self.canonical_action_key(action_qname)?;
        let id = *self.action_ids.get(&key)?;
        let sym = self.symbols.get(&id)?;
        Some(ActionTarget {
            config_file: sym.loc_file.clone(),
            config_offset: sym.loc_start as usize,
            class_fqcn: self.resolve_action_class(&key),
            view_jsp: self.resolve_action_view(&key),
        })
    }

    /// Resolve a Spring **bean id** (as written in a struts `<action class="beanId">` or a
    /// spring `<bean ref>`) to the implementation class FQCN — for go-to on a config XML.
    /// Tries the XML bean map (resolving the `parent=` chain), then the annotation-declared
    /// bean map (Option B). `None` when the id names no known bean.
    pub fn resolve_bean_class(&self, bean_id: &str) -> Option<String> {
        if let Some(fqcn) = resolve_bean_map(&self.graph.beans).get(bean_id) {
            if !fqcn.is_empty() {
                return Some(fqcn.clone());
            }
        }
        self.annotation_beans.get(bean_id).map(|b| b.fqcn.clone())
    }

    /// Canonicalize a **raw** JSP action reference (the attribute value the editor sends
    /// verbatim) to a qualified-name key actually present in the graph, or `None`. Tolerant
    /// in four steps, cheapest first — the fix for go-to / find-usages silently failing on
    /// every `.action`/`.do` URL, servlet-prefixed path, or namespace-less reference:
    ///   1. **exact** — the ref is already a stored qname (`/do/Cat/edit`);
    ///   2. **normalized** — strip a trailing `.action`/`.do` + `?query` (`/do/Cat/edit.action`
    ///      → `/do/Cat/edit`), then exact-match;
    ///   3. **segment-aligned suffix** — a JSP action URL often carries a servlet/filter
    ///      prefix the Struts namespace doesn't (e.g. Entando's
    ///      `/ExtStr2/do/FrontEnd/…/processPage.action`), so drop leading path segments and
    ///      match the LONGEST suffix that is a known action qname;
    ///   4. **unique trailing name** — as a last resort, a UNIQUE action whose trailing name
    ///      segment matches (the namespace is unknown); ambiguous → `None` (never guess).
    fn canonical_action_key(&self, raw: &str) -> Option<String> {
        if self.action_ids.contains_key(raw) {
            return Some(raw.to_string());
        }
        let norm = normalize_action_ref(raw)?;
        if self.action_ids.contains_key(&norm) {
            return Some(norm);
        }
        let segs: Vec<&str> = norm
            .trim_start_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();
        // 3. Longest known-action suffix: drop 1..n leading segments (e.g. the `/ExtStr2`
        //    servlet prefix) and take the first (= longest) suffix that is a stored qname.
        for start in 1..segs.len() {
            let cand = format!("/{}", segs[start..].join("/"));
            if self.action_ids.contains_key(&cand) {
                return Some(cand);
            }
        }
        // 4. Unique trailing name segment (subsumes the bare-name case). A top-level action
        //    with empty namespace is keyed by the bare name; a namespaced one by `…/name`.
        let last = segs.last()?;
        let suffix = format!("/{last}");
        let mut hit: Option<String> = None;
        for k in self.action_ids.keys() {
            if k == last || k.ends_with(&suffix) {
                if hit.is_some() {
                    return None; // ambiguous across namespaces
                }
                hit = Some(k.clone());
            }
        }
        hit
    }

    // ── interceptors ──────────────────────────────────────────────────────────

    /// Resolve an `<interceptor-ref name>` to its `<interceptor>` / `<interceptor-stack>`
    /// declaration (file + offset + impl class). `None` when the name is provided only by a
    /// framework/dependency jar (e.g. the built-in `defaultStack`).
    pub fn resolve_interceptor(&self, name: &str) -> Option<InterceptorDef<'_>> {
        resolve_interceptor_ref(&self.graph, name)
    }

    /// Every `<interceptor-ref>` use of `name` (in a stack, an action, or a package
    /// default) — the find-usages set for an interceptor / stack def.
    pub fn interceptor_usages(&self, name: &str) -> Vec<&InterceptorRefUse> {
        interceptor_usages(&self.graph, name)
    }

    /// The conservative "interceptor inesistente" diagnostic. NEVER returns
    /// [`ActionVerdict::Missing`]: the load-bearing built-in stacks (`defaultStack`,
    /// `paramsPrepareParamsStack`, …) are declared in `struts-default.xml` inside a
    /// framework jar we don't parse, so an unresolved ref is always [`Inconclusive`], not a
    /// hard miss.
    ///
    /// [`Inconclusive`]: ActionVerdict::Inconclusive
    pub fn diagnose_interceptor_ref(&self, name: &str) -> ActionVerdict {
        if resolve_interceptor_ref(&self.graph, name).is_some() {
            ActionVerdict::Exists
        } else {
            ActionVerdict::Inconclusive {
                reason: "interceptor may be a framework/jar-provided stack (e.g. defaultStack)"
                    .into(),
            }
        }
    }

    // ── validation ────────────────────────────────────────────────────────────

    /// The validation rulesets bound to an action class by its **simple name** (`FooAction`)
    /// — usually one base ruleset plus any per-alias `<Class>-<alias>-validation.xml`. The
    /// `<field name>`s inside each name action properties (resolved to getters/setters by the
    /// Java index at the call site).
    pub fn validations_for_class(&self, simple_name: &str) -> Vec<&ValidationRecord> {
        validations_for_class(&self.graph, simple_name)
    }

    // ── mybatis ───────────────────────────────────────────────────────────────

    /// Go-to XML: mapper interface method → its `<select|...|delete id>` statement (file +
    /// byte offset). `None` when the interface isn't a known mapper or has no such id.
    pub fn statement_for_method(
        &self,
        interface_fqcn: &str,
        method: &str,
    ) -> Option<StatementTarget<'_>> {
        statement_for_method(&self.graph, interface_fqcn, method)
    }

    /// find-usages / outline: every statement declared in a mapper interface's XML.
    pub fn methods_for_mapper(&self, interface_fqcn: &str) -> Vec<&StatementRecord> {
        methods_for_mapper(&self.graph, interface_fqcn)
    }

    /// Conservative "orphan statement" diagnostic: a `<select id="bar">` whose owning
    /// interface declares no matching method `bar`. NEVER returns [`ActionVerdict::Missing`]
    /// unless the interface is a KNOWN project type whose method set we can trust.
    ///
    /// `known_methods` is the interface's declared method-name set, resolved by the CALLER
    /// from the Java index (this crate never parses Java — the same boundary
    /// [`Self::validations_for_class`] respects). `None` = interface not a known project
    /// type → always [`ActionVerdict::Inconclusive`].
    pub fn diagnose_orphan_statement(
        &self,
        _interface_fqcn: &str,
        statement_id: &str,
        known_methods: Option<&std::collections::HashSet<String>>,
    ) -> ActionVerdict {
        match known_methods {
            None => ActionVerdict::Inconclusive {
                reason: "mapper interface is not a known project type".into(),
            },
            Some(methods) if methods.contains(statement_id) => ActionVerdict::Exists,
            Some(_) => ActionVerdict::Missing,
        }
    }

    // ── accessors ─────────────────────────────────────────────────────────────
    pub fn action_count(&self) -> usize {
        self.action_ids.len()
    }

    /// Total interceptor + stack definitions ingested (accessor for tests / status).
    pub fn interceptor_count(&self) -> usize {
        self.graph.interceptors.len() + self.graph.interceptor_stacks.len()
    }

    /// Total validation rulesets ingested (accessor for tests / status).
    pub fn validation_count(&self) -> usize {
        self.graph.validations.len()
    }

    /// Total MyBatis mappers ingested (accessor for tests / status).
    pub fn mapper_count(&self) -> usize {
        self.graph.mappers.len()
    }

    /// Total MyBatis statements ingested (accessor for tests / status).
    pub fn statement_count(&self) -> usize {
        self.graph.statements.len()
    }
    pub fn bean_count(&self) -> usize {
        self.bean_ids.len()
    }

    /// Look up an annotation-declared Spring bean (`@Service`/`@Component`/…) by its name —
    /// the C1 fallback map (Option B). For a future `@Autowired`-by-name go-to consumer + the
    /// index inspector. `None` when no stereotype-annotated class registers that name.
    pub fn resolve_bean(&self, name: &str) -> Option<&AnnotationBean> {
        self.annotation_beans.get(name)
    }

    /// Total annotation-declared beans ingested (accessor for tests / stats).
    pub fn annotation_bean_count(&self) -> usize {
        self.annotation_beans.len()
    }
    pub fn relation_count(&self) -> usize {
        self.relations.len()
    }
    pub fn graph(&self) -> &WebConfigGraph {
        &self.graph
    }

    /// Re-open a [`ConfigResolver`]'s **edge reader** off persisted files (for a consumer
    /// that only needs the raw out-edges of a node id, e.g. a references query). The
    /// full resolver (with the string maps) is built by [`ingest_config_graph`].
    pub fn open_edges(index_dir: &Path) -> Result<RelationReader, StoreError> {
        RelationReader::open(
            &index_dir.join(CONFIG_REL_BLOB),
            &index_dir.join(CONFIG_REL_FST),
        )
    }
}

/// Whether `view` (a webapp-relative JSP path like `/WEB-INF/x/y.jsp`) is a segment-aligned suffix
/// of the absolute `file` path (both forward-slashed). The `/`-anchored compare avoids a false match
/// where one path's tail is a substring of the other (`…/tree.jsp` vs `…/subtree.jsp`).
fn view_path_matches(file: &str, view: &str) -> bool {
    let v = view.trim_start_matches('/');
    !v.is_empty() && (file == v || file.ends_with(&format!("/{v}")))
}

/// Whether `target` looks like a JSP page path (a direct dispatcher result), vs a Tiles def name or
/// an action reference (a `chain`/`redirect` target). Case-insensitive on the extension.
fn is_jsp_path(target: &str) -> bool {
    let t = target.to_ascii_lowercase();
    t.ends_with(".jsp") || t.ends_with(".jspf") || t.ends_with(".jspx")
}

/// A resolved go-to-definition target for a JSP action reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionTarget {
    /// The struts config fragment the `<action>` is declared in.
    pub config_file: String,
    /// Byte offset of the `<action>` element in `config_file` — the FE jumps here so go-to
    /// lands on the declaration, not the top of the file.
    pub config_offset: usize,
    /// The resolved implementation class FQCN (the C1 chain), if resolvable.
    pub class_fqcn: Option<String>,
    /// The resolved view JSP (the Tiles chain), if resolvable.
    pub view_jsp: Option<String>,
}

/// Strip a namespace prefix off an action qualified-name, returning the trailing name
/// segment a wildcard `name=` pattern is matched against. `/do/E/openError` in ns
/// `/do/E` → `openError`. `None` if the ref isn't under the namespace.
fn strip_ns<'a>(qname: &'a str, ns: &str) -> Option<&'a str> {
    if ns.is_empty() {
        return Some(qname);
    }
    let ns = ns.trim_end_matches('/');
    let prefix = format!("{ns}/");
    qname.strip_prefix(&prefix)
}

/// The struts qualified-name join, re-exported so a caller building a reference key
/// uses the same convention the parser did.
pub fn action_qname(namespace: &str, name: &str) -> String {
    join_ns(namespace, name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("bennu-cfg-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Build a small graph via bennu-web, ingest it, and prove the C1 chain, the view
    /// chain, and the conservative diagnostic over the ingested edges.
    #[test]
    fn ingest_resolves_chains_and_conservative_diagnostic() {
        use bennu_web::prelude::{build_web_graph, WebInputs};

        let dir = tmp_dir();
        let struts = dir.join("s.xml");
        std::fs::write(
            &struts,
            r#"<struts><package name="p" namespace="/do/Cat" extends="japs-default">
                <action name="viewTree" class="categoryAction">
                  <result type="tiles">admin.Cat.viewTree</result>
                </action>
                <action name="edit*" class="editAction" method="edit{1}">
                  <result type="tiles">admin.{1}</result>
                </action>
              </package></struts>"#,
        )
        .unwrap();
        let beans = dir.join("b.xml");
        std::fs::write(
            &beans,
            r#"<beans>
                <bean id="abstractBase" abstract="true" class="com.x.Base"/>
                <bean id="categoryAction" parent="abstractBase" class="com.x.CategoryAction"/>
                <bean id="editAction" class="com.x.EditAction"/>
              </beans>"#,
        )
        .unwrap();
        let tiles = dir.join("t.xml");
        std::fs::write(
            &tiles,
            r#"<tiles-definitions>
                <definition name="main.layout" template="/WEB-INF/layout.jsp"/>
                <definition name="admin.Cat.viewTree" extends="main.layout">
                  <put-attribute name="body" value="/WEB-INF/cat/tree.jsp"/>
                </definition>
              </tiles-definitions>"#,
        )
        .unwrap();

        let inputs = WebInputs {
            struts_roots: vec![struts],
            resource_roots: vec![],
            spring_files: vec![beans],
            tiles_files: vec![tiles],
            validation_files: vec![],
            mapper_files: vec![],
        };
        let (graph, _report) = build_web_graph(&inputs);
        let cfg = ingest_config_graph(&graph, &dir, &[]).unwrap();

        // C1: action → bean-id → FQCN, over the ingested ActionToClass edge.
        assert_eq!(
            cfg.resolve_action_class("/do/Cat/viewTree").as_deref(),
            Some("com.x.CategoryAction")
        );
        // view chain.
        assert_eq!(
            cfg.resolve_action_view("/do/Cat/viewTree").as_deref(),
            Some("/WEB-INF/cat/tree.jsp")
        );

        // diagnostic: the concrete action exists.
        assert_eq!(
            cfg.diagnose_action("/do/Cat/viewTree"),
            ActionVerdict::Exists
        );
        // a reference the `edit*` wildcard would match → Inconclusive, NEVER Missing.
        match cfg.diagnose_action("/do/Cat/editUser") {
            ActionVerdict::Inconclusive { .. } => {}
            other => panic!("wildcard candidate must be Inconclusive, got {other:?}"),
        }
        // a genuinely absent, non-wildcard, non-computed reference → Missing.
        assert_eq!(cfg.diagnose_action("/do/Cat/nope"), ActionVerdict::Missing);

        // the persisted edge store re-opens and yields the action's out-edges.
        let reader = ConfigResolver::open_edges(&dir).unwrap();
        assert!(reader.node_count() >= 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn view_path_and_jsp_predicates() {
        // Segment-aligned suffix match (the reverse-lookup path compare).
        assert!(view_path_matches(
            "c:/p/webapp/WEB-INF/cat/tree.jsp",
            "/WEB-INF/cat/tree.jsp"
        ));
        assert!(view_path_matches(
            "c:/p/webapp/WEB-INF/cat/tree.jsp",
            "WEB-INF/cat/tree.jsp"
        ));
        // `tree.jsp` must NOT match `subtree.jsp` (the `/`-anchor guards the substring trap).
        assert!(!view_path_matches(
            "c:/p/webapp/WEB-INF/cat/subtree.jsp",
            "/WEB-INF/cat/tree.jsp"
        ));
        assert!(!view_path_matches("c:/p/x.jsp", ""));
        // JSP-path recognition (direct dispatcher result vs a Tiles def name / action ref).
        assert!(is_jsp_path("/WEB-INF/x.jsp"));
        assert!(is_jsp_path("a/b.JSPF"));
        assert!(!is_jsp_path("admin.Cat.viewTree")); // a Tiles def name
        assert!(!is_jsp_path("/do/Other")); // a chain target (another action)
    }

    /// The reverse view→action lookup resolves BOTH a Tiles-mapped result AND a DIRECT
    /// `<result>/WEB-INF/x.jsp</result>` dispatcher result (the common legacy shape that
    /// `resolve_action_view` deliberately skips — the "no dropdown" bug).
    #[test]
    fn actions_for_view_covers_tiles_and_direct_results() {
        use bennu_web::prelude::{build_web_graph, WebInputs};

        let dir = std::env::temp_dir().join(format!("bennu-cfg-rev-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let struts = dir.join("s.xml");
        std::fs::write(
            &struts,
            r#"<struts><package name="p" namespace="/do" extends="d">
                <action name="viewTree" class="catAction"><result type="tiles">tree.def</result></action>
                <action name="list" class="listAction"><result>/WEB-INF/list.jsp</result></action>
              </package></struts>"#,
        )
        .unwrap();
        let beans = dir.join("b.xml");
        std::fs::write(
            &beans,
            r#"<beans>
                <bean id="catAction" class="com.x.CatAction"/>
                <bean id="listAction" class="com.x.ListAction"/>
              </beans>"#,
        )
        .unwrap();
        let tiles = dir.join("t.xml");
        std::fs::write(
            &tiles,
            r#"<tiles-definitions>
                <definition name="tree.def" template="/WEB-INF/cat/tree.jsp"/>
              </tiles-definitions>"#,
        )
        .unwrap();

        let inputs = WebInputs {
            struts_roots: vec![struts],
            resource_roots: vec![],
            spring_files: vec![beans],
            tiles_files: vec![tiles],
            validation_files: vec![],
            mapper_files: vec![],
        };
        let (graph, _r) = build_web_graph(&inputs);
        let cfg = ingest_config_graph(&graph, &dir, &[]).unwrap();

        // Tiles-resolved view → its action + class.
        let tiles_hit = cfg.actions_for_view("C:/proj/webapp/WEB-INF/cat/tree.jsp");
        assert!(
            tiles_hit
                .iter()
                .any(|(q, c)| q == "/do/viewTree" && c.as_deref() == Some("com.x.CatAction")),
            "tiles reverse-lookup: {tiles_hit:?}"
        );
        // DIRECT dispatcher result → its action + class (the fixed case).
        let direct_hit = cfg.actions_for_view("C:/proj/webapp/WEB-INF/list.jsp");
        assert!(
            direct_hit
                .iter()
                .any(|(q, c)| q == "/do/list" && c.as_deref() == Some("com.x.ListAction")),
            "direct reverse-lookup: {direct_hit:?}"
        );
        // A JSP that no action renders → no candidates.
        assert!(cfg
            .actions_for_view("C:/proj/webapp/WEB-INF/nope.jsp")
            .is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The go-to path tolerates the **raw** attribute value the editor sends verbatim: a
    /// `.action`/`.do` suffix, a `?query`, and a namespace-less bare name all still resolve
    /// (the bug where JSP go-to / find-usages silently failed on every real-world ref).
    #[test]
    fn resolves_raw_action_refs_suffix_query_and_bare_name() {
        use bennu_web::prelude::{build_web_graph, WebInputs};

        let dir = std::env::temp_dir().join(format!("bennu-cfg-raw-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let struts = dir.join("s.xml");
        std::fs::write(
            &struts,
            r#"<struts>
                <package name="cat" namespace="/do/Cat" extends="japs-default">
                  <action name="viewTree" class="categoryAction"><result type="tiles">v</result></action>
                </package>
                <package name="sec" namespace="/do/Sec" extends="japs-default">
                  <action name="edit" class="secAction"><result type="tiles">v</result></action>
                </package>
                <package name="usr" namespace="/do/Usr" extends="japs-default">
                  <action name="edit" class="usrAction"><result type="tiles">v</result></action>
                </package>
              </struts>"#,
        )
        .unwrap();
        let beans = dir.join("b.xml");
        std::fs::write(
            &beans,
            r#"<beans>
                <bean id="categoryAction" class="com.x.CategoryAction"/>
                <bean id="secAction" class="com.x.SecAction"/>
                <bean id="usrAction" class="com.x.UsrAction"/>
              </beans>"#,
        )
        .unwrap();

        let inputs = WebInputs {
            struts_roots: vec![struts],
            resource_roots: vec![],
            spring_files: vec![beans],
            tiles_files: vec![],
            validation_files: vec![],
            mapper_files: vec![],
        };
        let (graph, _report) = build_web_graph(&inputs);
        let cfg = ingest_config_graph(&graph, &dir, &[]).unwrap();

        let fqcn =
            |cfg: &ConfigResolver, r: &str| cfg.action_class_ref(r).and_then(|t| t.class_fqcn);

        // 1. exact absolute qname (already worked).
        assert_eq!(
            fqcn(&cfg, "/do/Cat/viewTree").as_deref(),
            Some("com.x.CategoryAction")
        );
        // 2. trailing `.action` / `.do` stripped.
        assert_eq!(
            fqcn(&cfg, "/do/Cat/viewTree.action").as_deref(),
            Some("com.x.CategoryAction")
        );
        // 3. query string dropped.
        assert_eq!(
            fqcn(&cfg, "/do/Cat/viewTree.action?x=1&y=2").as_deref(),
            Some("com.x.CategoryAction")
        );
        // 4. bare name with a UNIQUE `…/viewTree` action → resolves (namespace inferred).
        assert_eq!(
            fqcn(&cfg, "viewTree").as_deref(),
            Some("com.x.CategoryAction")
        );
        assert_eq!(
            fqcn(&cfg, "viewTree.action").as_deref(),
            Some("com.x.CategoryAction")
        );
        // 5. bare name that is AMBIGUOUS (`edit` in both /do/Sec and /do/Usr) → no guess.
        assert!(
            cfg.action_class_ref("edit").is_none(),
            "ambiguous bare name must not resolve"
        );
        // …but the absolute form disambiguates.
        assert_eq!(
            fqcn(&cfg, "/do/Sec/edit").as_deref(),
            Some("com.x.SecAction")
        );
        // 6. a genuinely-unknown ref stays unresolved.
        assert!(cfg.action_class_ref("/do/Cat/ghost").is_none());
        // 7. servlet/filter-prefixed URL (Entando `<wp:action path="/ExtStr2/...">`): the
        //    `/ExtStr2` prefix isn't in the Struts namespace, so the longest known-action
        //    suffix (`/do/Cat/viewTree`) matches after dropping leading segments.
        assert_eq!(
            fqcn(&cfg, "/ExtStr2/do/Cat/viewTree.action").as_deref(),
            Some("com.x.CategoryAction")
        );
        // …and a prefixed AMBIGUOUS tail still refuses to guess.
        assert!(cfg.action_class_ref("/ExtStr2/do/edit.action").is_none());

        // The go-to target carries the `<action>` byte offset (non-zero) so it lands on the
        // declaration, not line 1.
        let tgt = cfg.action_class_ref("/do/Cat/viewTree").unwrap();
        assert!(tgt.config_offset > 0, "action decl offset must be captured");
        assert!(tgt.config_file.ends_with("s.xml"));
        // A struts `class="beanId"` resolves to its impl FQCN (config-XML go-to).
        assert_eq!(
            cfg.resolve_bean_class("categoryAction").as_deref(),
            Some("com.x.CategoryAction")
        );
        assert_eq!(cfg.resolve_bean_class("nope").as_deref(), None);

        // diagnose_action is now as tolerant as go-to: the concrete action (with a raw `.action`)
        // is Exists; a dangling absolute ref is Missing.
        assert_eq!(
            cfg.diagnose_action("/do/Cat/viewTree.action"),
            ActionVerdict::Exists
        );
        assert_eq!(cfg.diagnose_action("/do/Cat/ghost"), ActionVerdict::Missing);
        // REGRESSION: an Entando `/ExtStr2`-prefixed URL that go-to resolves must NOT be a false
        // "action does not exist" — being stricter than go-to here was the bug.
        assert_eq!(
            cfg.diagnose_action("/ExtStr2/do/Cat/viewTree.action"),
            ActionVerdict::Exists,
            "a servlet-prefixed URL that Ctrl+B resolves must not be flagged Missing",
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Option B C1 fallback: an `<action class="fooService">` with NO matching XML `<bean>`
    /// resolves through the annotation-declared bean map (`@Service("fooService")`), lighting
    /// up JSP action go-to→class in annotation-based apps. An XML bean still wins (no
    /// regression), and the resolved FQCN flows all the way into `action_class_ref().class_fqcn`
    /// (the struct the FE go-to reads).
    #[test]
    fn ingest_annotation_bean_fallback_resolves_action_class() {
        use bennu_web::prelude::{build_web_graph, WebInputs};

        let dir = std::env::temp_dir().join(format!("bennu-cfg-annbean-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // Two actions: `edit` → `fooService` has NO XML bean (annotation-only); `list` →
        // `barService` DOES have an XML bean (must still win, no regression).
        let struts = dir.join("s.xml");
        std::fs::write(
            &struts,
            r#"<struts><package name="p" namespace="/do/X" extends="japs-default">
                <action name="edit" class="fooService"><result type="tiles">v</result></action>
                <action name="list" class="barService"><result type="tiles">v</result></action>
              </package></struts>"#,
        )
        .unwrap();
        let beans = dir.join("b.xml");
        std::fs::write(
            &beans,
            r#"<beans><bean id="barService" class="com.x.XmlBarService"/></beans>"#,
        )
        .unwrap();

        let inputs = WebInputs {
            struts_roots: vec![struts],
            resource_roots: vec![],
            spring_files: vec![beans],
            tiles_files: vec![],
            validation_files: vec![],
            mapper_files: vec![],
        };
        let (graph, _report) = build_web_graph(&inputs);

        // The annotation-declared beans (as `collect_annotation_beans` would produce them):
        // `fooService` is annotation-only, `barService` also exists as XML (the XML must win).
        let ann = vec![
            AnnotationBean {
                name: "fooService".into(),
                fqcn: "com.x.FooService".into(),
                source_file: "com/x/FooService.java".into(),
            },
            AnnotationBean {
                name: "barService".into(),
                fqcn: "com.x.AnnotationBarService".into(),
                source_file: "com/x/AnnotationBarService.java".into(),
            },
        ];
        let cfg = ingest_config_graph(&graph, &dir, &ann).unwrap();

        assert_eq!(cfg.annotation_bean_count(), 2);

        // The annotation-bean fallback: `fooService` has no XML bean → resolves to the
        // `@Service` class.
        assert_eq!(
            cfg.resolve_action_class("/do/X/edit").as_deref(),
            Some("com.x.FooService")
        );
        // No regression: the XML `<bean>` for `barService` still wins over the annotation bean.
        assert_eq!(
            cfg.resolve_action_class("/do/X/list").as_deref(),
            Some("com.x.XmlBarService")
        );

        // The bean is retrievable by name (for the future @Autowired go-to + inspector).
        assert_eq!(
            cfg.resolve_bean("fooService").map(|b| b.fqcn.as_str()),
            Some("com.x.FooService")
        );
        assert!(cfg.resolve_bean("nope").is_none());

        // End-to-end: the go-to target struct carries the annotation FQCN (the FE path).
        let target = cfg.action_class_ref("/do/X/edit").expect("action resolves");
        assert_eq!(target.class_fqcn.as_deref(), Some("com.x.FooService"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Interceptors (defs + stacks + refs) and a validation ruleset resolve over the
    /// ingested graph: go-to a ref → its def + class, find-usages a def → its refs, the
    /// conservative diagnostic, and the class→ruleset binding.
    #[test]
    fn ingest_resolves_interceptors_and_validation() {
        use bennu_web::prelude::{build_web_graph, WebInputs};

        let dir = std::env::temp_dir().join(format!("bennu-cfg-iv-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let struts = dir.join("s.xml");
        std::fs::write(
            &struts,
            r#"<struts><package name="p" namespace="/do/Sec" extends="japs-default">
                <interceptors>
                  <interceptor name="auth" class="com.x.AuthInterceptor"/>
                  <interceptor-stack name="secureStack">
                    <interceptor-ref name="defaultStack"/>
                    <interceptor-ref name="auth"/>
                  </interceptor-stack>
                </interceptors>
                <default-interceptor-ref name="secureStack"/>
                <action name="edit" class="editAction">
                  <interceptor-ref name="secureStack"/>
                  <result type="tiles">x</result>
                </action>
              </package></struts>"#,
        )
        .unwrap();

        let validation = dir.join("LoginAction-validation.xml");
        std::fs::write(
            &validation,
            r#"<validators>
                <field name="username"><field-validator type="requiredstring"><message>r</message></field-validator></field>
                <field name="password"><field-validator type="requiredstring"><message>r</message></field-validator></field>
              </validators>"#,
        )
        .unwrap();

        let inputs = WebInputs {
            struts_roots: vec![struts],
            resource_roots: vec![],
            spring_files: vec![],
            tiles_files: vec![],
            validation_files: vec![validation],
            mapper_files: vec![],
        };
        let (graph, _report) = build_web_graph(&inputs);
        let cfg = ingest_config_graph(&graph, &dir, &[]).unwrap();

        assert_eq!(cfg.interceptor_count(), 2, "1 interceptor + 1 stack");
        assert_eq!(cfg.validation_count(), 1);

        // go-to `<interceptor-ref name="auth">` → the interceptor def + its impl class.
        let def = cfg.resolve_interceptor("auth").expect("auth resolves");
        assert!(!def.is_stack);
        assert_eq!(def.class, Some("com.x.AuthInterceptor"));

        // go-to a stack ref → the stack def (no class).
        let stack = cfg
            .resolve_interceptor("secureStack")
            .expect("stack resolves");
        assert!(stack.is_stack);
        assert_eq!(stack.class, None);

        // find-usages of `secureStack`: the package default + the action ref = 2.
        assert_eq!(cfg.interceptor_usages("secureStack").len(), 2);
        // find-usages of `auth`: the one stack member ref.
        assert_eq!(cfg.interceptor_usages("auth").len(), 1);

        // the built-in `defaultStack` (jar-provided) is NEVER a hard miss.
        match cfg.diagnose_interceptor_ref("defaultStack") {
            ActionVerdict::Inconclusive { .. } => {}
            other => panic!("jar-provided stack must be Inconclusive, got {other:?}"),
        }
        // a locally-declared interceptor exists.
        assert_eq!(cfg.diagnose_interceptor_ref("auth"), ActionVerdict::Exists);

        // the validation ruleset binds to the action class simple-name; its fields name
        // action properties.
        let vals = cfg.validations_for_class("LoginAction");
        assert_eq!(vals.len(), 1);
        let field_names: Vec<&str> = vals[0].fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(field_names, vec!["username", "password"]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// MyBatis mappers resolve over the ingested graph: go-to method → statement (file +
    /// offset + kind), find-usages a mapper → its statements, and the three-way
    /// conservative orphan-statement diagnostic (`Exists`/`Missing`/`Inconclusive`).
    #[test]
    fn ingest_resolves_mybatis_statements() {
        use bennu_web::prelude::{build_web_graph, WebInputs};
        use std::collections::HashSet;

        let dir = std::env::temp_dir().join(format!("bennu-cfg-mb-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let mapper = dir.join("FooMapper.xml");
        std::fs::write(
            &mapper,
            r#"<mapper namespace="com.x.FooMapper">
                <select id="findById" resultType="com.x.Foo">select * from foo where id = #{id}</select>
                <insert id="insert">insert into foo (a) values (#{a})</insert>
                <update id="update">update foo set a = #{a}</update>
                <delete id="deleteById">delete from foo where id = #{id}</delete>
              </mapper>"#,
        )
        .unwrap();

        let inputs = WebInputs {
            struts_roots: vec![],
            resource_roots: vec![],
            spring_files: vec![],
            tiles_files: vec![],
            validation_files: vec![],
            mapper_files: vec![mapper],
        };
        let (graph, _report) = build_web_graph(&inputs);
        let cfg = ingest_config_graph(&graph, &dir, &[]).unwrap();

        assert_eq!(cfg.mapper_count(), 1);
        assert_eq!(cfg.statement_count(), 4);

        // go-to a method → its statement (file + offset + kind).
        let target = cfg
            .statement_for_method("com.x.FooMapper", "findById")
            .expect("findById resolves");
        assert_eq!(target.kind, bennu_web::prelude::StatementKind::Select);
        assert!(target.offset > 0);
        assert!(target.file.ends_with("FooMapper.xml"));
        // an unknown method / unknown interface → no target.
        assert!(cfg
            .statement_for_method("com.x.FooMapper", "ghost")
            .is_none());
        assert!(cfg
            .statement_for_method("com.x.Unknown", "findById")
            .is_none());

        // find-usages / outline: every statement in the mapper.
        assert_eq!(cfg.methods_for_mapper("com.x.FooMapper").len(), 4);

        // the conservative diagnostic: known project type + matching method → Exists.
        let methods: HashSet<String> = ["findById", "insert", "update", "deleteById"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            cfg.diagnose_orphan_statement("com.x.FooMapper", "findById", Some(&methods)),
            ActionVerdict::Exists
        );
        // known project type but NO matching method → a genuine orphan → Missing.
        let only_find: HashSet<String> = ["findById"].iter().map(|s| s.to_string()).collect();
        assert_eq!(
            cfg.diagnose_orphan_statement("com.x.FooMapper", "insert", Some(&only_find)),
            ActionVerdict::Missing
        );
        // interface not a known project type (None) → NEVER Missing → Inconclusive.
        match cfg.diagnose_orphan_statement("com.x.FooMapper", "anything", None) {
            ActionVerdict::Inconclusive { .. } => {}
            other => panic!("unknown interface must be Inconclusive, got {other:?}"),
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}
