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

use bennu_index::prelude::{
    serialize_symbol, BlobWriter, Relation, RelationKind, RelationReader, RelationWriter, Source,
    StoreError, Symbol, SymbolKind,
};
use bennu_web::prelude::{
    join_ns, resolve_action_view, resolve_bean_map, RelKind, WebConfigGraph, WildcardPattern,
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
pub fn ingest_config_graph(
    graph: &WebConfigGraph,
    index_dir: &Path,
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
            modifiers: if a.is_wildcard { "wildcard".into() } else { String::new() },
            loc_file: a.source_file.clone(),
            loc_start: 0,
            loc_end: 0,
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
    sw.finish(&index_dir.join(CONFIG_SYMBOL_BLOB), &index_dir.join(CONFIG_SYMBOL_FST))?;

    // 3) Relations → resolve each endpoint to a symbol id where it names a known
    //    action/bean. `ActionToClass` (action → bean) is the load-bearing C1 edge; the
    //    result/view endpoints are synthetic ("<action>#<result>" / Tiles-def names),
    //    kept in the graph rather than the id store (a later JSP-symbol wave gives views
    //    real ids). `BeanIdToImpl`'s `to` is an FQCN — the impl lives in the bean symbol.
    let mut rw = RelationWriter::new();
    let mut relations: Vec<Relation> = Vec::new();
    for r in &graph.relations {
        let resolved = match r.kind {
            RelKind::ActionToClass => {
                (action_ids.get(&r.from).copied(), bean_ids.get(&r.to).copied())
            }
            RelKind::BeanIdToImpl => (bean_ids.get(&r.from).copied(), Some(u32::MAX)),
            RelKind::ActionToResult | RelKind::ResultToView => {
                (action_ids.get(&r.from).copied(), None)
            }
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
    rw.finish(&index_dir.join(CONFIG_REL_BLOB), &index_dir.join(CONFIG_REL_FST))?;

    // 4) Compile wildcard patterns for the conservative diagnostic (namespace kept so a
    //    reference is matched only within the wildcard action's own namespace).
    let wildcards = graph
        .actions
        .iter()
        .filter(|a| a.is_wildcard)
        .map(|a| (WildcardPattern::compile(&a.name), a.namespace.clone()))
        .collect();

    Ok(ConfigResolver {
        graph: graph.clone(),
        action_ids,
        bean_ids,
        symbols,
        relations,
        wildcards,
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
}

impl ConfigResolver {
    /// The C1 chain over the ingested `ActionToClass` edge: action qualified-name →
    /// bean-id → impl FQCN (the bean symbol's `fqn`, resolving the Spring parent chain
    /// if the bean has no own class). `None` when the bean-id lives in a dependency jar
    /// (unknown) or the action declares no class.
    pub fn resolve_action_class(&self, action_qname: &str) -> Option<String> {
        let action_id = *self.action_ids.get(action_qname)?;
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
        resolve_bean_map(&self.graph.beans).get(&bean_sym.simple_name).cloned()
    }

    /// The view chain: action → `<result type=tiles>` def → JSP. Answered off the parsed
    /// graph (the Tiles-def→JSP indirection isn't a symbol edge yet).
    pub fn resolve_action_view(&self, action_qname: &str) -> Option<String> {
        resolve_action_view(&self.graph, action_qname)
    }

    /// The conservative "action inesistente" diagnostic (docs §8). NEVER returns
    /// [`ActionVerdict::Missing`] when a wildcard pattern or a computed/OGNL path could
    /// match the reference.
    pub fn diagnose_action(&self, action_qname: &str) -> ActionVerdict {
        if self.action_ids.contains_key(action_qname) {
            return ActionVerdict::Exists;
        }
        // Could a wildcard action's pattern match this reference (within its namespace)?
        for (pat, ns) in &self.wildcards {
            if let Some(candidate) = strip_ns(action_qname, ns) {
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
        let id = *self.action_ids.get(action_qname)?;
        let sym = self.symbols.get(&id)?;
        Some(ActionTarget {
            config_file: sym.loc_file.clone(),
            class_fqcn: self.resolve_action_class(action_qname),
            view_jsp: self.resolve_action_view(action_qname),
        })
    }

    // ── accessors ─────────────────────────────────────────────────────────────
    pub fn action_count(&self) -> usize {
        self.action_ids.len()
    }
    pub fn bean_count(&self) -> usize {
        self.bean_ids.len()
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
        RelationReader::open(&index_dir.join(CONFIG_REL_BLOB), &index_dir.join(CONFIG_REL_FST))
    }
}

/// A resolved go-to-definition target for a JSP action reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionTarget {
    /// The struts config fragment the `<action>` is declared in.
    pub config_file: String,
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
        };
        let (graph, _report) = build_web_graph(&inputs);
        let cfg = ingest_config_graph(&graph, &dir).unwrap();

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
        assert_eq!(cfg.diagnose_action("/do/Cat/viewTree"), ActionVerdict::Exists);
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
}
