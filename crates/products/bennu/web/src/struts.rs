//! Struts2 / XWork config parser.
//!
//! Parses a `struts.xml` (or any `*-struts-plugin.xml` root) and follows
//! `<include file="classpath/name.xml">` across the project resource tree — in this
//! vendored Entando app the fragments are on disk under `src/main/resources/<pkg>/…`
//! (docs §8 #3). Extracts `<package namespace>` + `<action name method class>` +
//! `<result name type>`. Wildcard action names (`*`) are kept as patterns and their
//! `{n}` backrefs (method / result target) are flagged inferred (docs §7).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use roxmltree::Node;

use crate::model::{ActionRecord, RelKind, Relation, ResultRecord};
use crate::xml;

/// Result of parsing the struts include-graph.
#[derive(Debug, Default)]
pub struct StrutsParse {
    pub actions: Vec<ActionRecord>,
    pub results: Vec<ResultRecord>,
    pub relations: Vec<Relation>,
    /// Include targets that could not be resolved on disk (would come from a dependency
    /// jar on a non-vendored install) — reported, never fatal (docs §8 #3, §8 lesson 10).
    pub unresolved_includes: Vec<String>,
}

/// Parse the struts config rooted at `root_xml`, resolving `<include>`s against
/// `resource_roots` (classpath roots, typically `src/main/resources`). Cycles and
/// re-includes are de-duplicated.
pub fn parse_include_graph(root_xml: &Path, resource_roots: &[PathBuf]) -> StrutsParse {
    let mut out = StrutsParse::default();
    let mut visited: HashSet<PathBuf> = HashSet::new();
    parse_file(root_xml, resource_roots, &mut out, &mut visited);
    out
}

fn parse_file(
    file: &Path,
    resource_roots: &[PathBuf],
    out: &mut StrutsParse,
    visited: &mut HashSet<PathBuf>,
) {
    let canon = file.canonicalize().unwrap_or_else(|_| file.to_path_buf());
    if !visited.insert(canon) {
        return; // already parsed (cycle / diamond include)
    }
    let text = match std::fs::read_to_string(file) {
        Ok(t) => t,
        Err(_) => return,
    };
    // Parse errors (rare, malformed fragment) are non-fatal: skip the file, keep going.
    let doc = match xml::parse(&text) {
        Some(d) => d,
        None => return,
    };
    let root = doc.root_element();
    let source_file = file.display().to_string();

    for pkg in root.children().filter(|n| n.has_tag_name("package")) {
        let namespace = pkg.attribute("namespace").unwrap_or("").to_string();
        for action in pkg.children().filter(|n| n.has_tag_name("action")) {
            parse_action(&action, &namespace, &source_file, out);
        }
    }

    // Follow includes (they can appear at <struts> top level).
    for inc in root.children().filter(|n| n.has_tag_name("include")) {
        if let Some(file_attr) = inc.attribute("file") {
            match resolve_include(file_attr, resource_roots) {
                Some(path) => parse_file(&path, resource_roots, out, visited),
                None => out.unresolved_includes.push(file_attr.to_string()),
            }
        }
    }
}

fn parse_action(action: &Node, namespace: &str, source_file: &str, out: &mut StrutsParse) {
    let name = action.attribute("name").unwrap_or("").to_string();
    if name.is_empty() {
        return;
    }
    let class_ref = action.attribute("class").unwrap_or("").to_string();
    let method = action.attribute("method").unwrap_or("").to_string();
    let is_wildcard = name.contains('*');
    let qualified_name = join_ns(namespace, &name);

    // ActionToClass — the class is a Spring bean-id here (docs §10 C1). Inferred only
    // when the action itself is a wildcard (the class is still a literal bean-id).
    if !class_ref.is_empty() {
        out.relations.push(Relation {
            from: qualified_name.clone(),
            to: class_ref.clone(),
            kind: RelKind::ActionToClass,
            inferred: is_wildcard,
        });
    }

    // Results.
    for res in action.children().filter(|n| n.has_tag_name("result")) {
        let res_name = res.attribute("name").unwrap_or("").to_string();
        let result_type = res.attribute("type").unwrap_or("").to_string();
        let target = res.text().map(|t| t.trim().to_string()).unwrap_or_default();
        let backref = target.contains('{') || namespace_has_backref(&method);
        let res_name_final =
            if res_name.is_empty() { "success".to_string() } else { res_name.clone() };

        out.results.push(ResultRecord {
            action_qualified_name: qualified_name.clone(),
            name: res_name_final.clone(),
            result_type: result_type.clone(),
            target: target.clone(),
            is_inferred: backref || is_wildcard,
        });
        out.relations.push(Relation {
            from: qualified_name.clone(),
            to: format!("{qualified_name}#{res_name_final}"),
            kind: RelKind::ActionToResult,
            inferred: is_wildcard,
        });
        // ResultToView: for tiles the target is a Tiles def name; for dispatcher a JSP.
        // Emit the edge; the graph join resolves tiles→jsp. Mark inferred on wildcard /
        // backref (target computed at runtime).
        if !target.is_empty() {
            out.relations.push(Relation {
                from: format!("{qualified_name}#{res_name_final}"),
                to: target.clone(),
                kind: RelKind::ResultToView,
                inferred: is_wildcard || backref,
            });
        }
    }

    out.actions.push(ActionRecord {
        qualified_name,
        namespace: namespace.to_string(),
        name,
        class_ref,
        method,
        is_wildcard,
        source_file: source_file.to_string(),
    });
}

/// Does a method string carry a `{n}` backref (wildcard-synthesized method name)?
fn namespace_has_backref(method: &str) -> bool {
    method.contains('{')
}

/// Join a namespace and action name into a single lookup key. Struts action URLs are
/// `<namespace>/<name>`; a missing namespace yields just the name.
pub fn join_ns(namespace: &str, name: &str) -> String {
    if namespace.is_empty() {
        name.to_string()
    } else {
        format!("{}/{}", namespace.trim_end_matches('/'), name)
    }
}

/// Resolve an `<include file="a/b/c.xml">` classpath name against the resource roots.
/// Returns the first existing file, or `None` (→ would come from a dependency jar).
fn resolve_include(file_attr: &str, resource_roots: &[PathBuf]) -> Option<PathBuf> {
    let rel = file_attr.replace('\\', "/");
    for root in resource_roots {
        let cand = root.join(&rel);
        if cand.is_file() {
            return Some(cand);
        }
    }
    None
}

/// A compiled wildcard action pattern for matching a runtime action name to candidates.
/// Struts `*` matches a URL segment; `{1}`, `{2}` … are the captured groups, substituted
/// into `method`/result targets. We model this as a candidate matcher — never an exact
/// "this action does not exist" verdict (docs §7, §8).
#[derive(Debug, Clone)]
pub struct WildcardPattern {
    /// The literal prefix before the first `*` (e.g. `editAttribute` for `editAttribute*`).
    pub prefix: String,
    /// The literal segments between/after stars (for `a*b*c` → `["b", "c"]` after prefix).
    pub segments: Vec<String>,
    /// The original raw pattern (`editAttribute*`).
    pub raw: String,
}

impl WildcardPattern {
    /// Compile a `name` attribute into a pattern. Only meaningful when it contains `*`.
    pub fn compile(name: &str) -> Self {
        let parts: Vec<&str> = name.split('*').collect();
        let prefix = parts.first().copied().unwrap_or("").to_string();
        let segments = parts.iter().skip(1).map(|s| s.to_string()).collect();
        WildcardPattern { prefix, segments, raw: name.to_string() }
    }

    /// Does a concrete action `candidate` name plausibly match this pattern? Conservative:
    /// prefix must match and each subsequent literal segment must appear in order.
    pub fn matches(&self, candidate: &str) -> bool {
        if !candidate.starts_with(&self.prefix) {
            return false;
        }
        let mut rest = &candidate[self.prefix.len()..];
        for seg in &self.segments {
            if seg.is_empty() {
                continue;
            }
            match rest.find(seg.as_str()) {
                Some(idx) => rest = &rest[idx + seg.len()..],
                None => return false,
            }
        }
        true
    }

    /// Substitute a wildcard's captured group into a `{n}` template (e.g. `open{1}`
    /// with capture `Error` → `openError`). Single-star only (n=1); higher n left as-is
    /// when not captured. Best-effort — the result is a *candidate* method/result name.
    pub fn expand_backrefs(template: &str, captures: &[&str]) -> String {
        let mut out = template.to_string();
        for (i, cap) in captures.iter().enumerate() {
            out = out.replace(&format!("{{{}}}", i + 1), cap);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_action_namespace_and_default_result_name() {
        let xml = r#"<struts><package name="p" namespace="/do/Cat" extends="japs-default">
            <action name="viewTree" class="categoryAction">
              <result type="tiles">admin.Cat.viewTree</result>
            </action>
            <action name="save" class="categoryAction" method="save">
              <result name="input" type="tiles">admin.Cat.entry</result>
              <result type="tiles">admin.Cat.viewTree</result>
            </action>
          </package></struts>"#;
        let file = crate::test_support::tmp("struts-basic.xml", xml);
        let parse = parse_include_graph(&file, &[]);

        assert_eq!(parse.actions.len(), 2);
        let vt = parse.actions.iter().find(|a| a.name == "viewTree").unwrap();
        assert_eq!(vt.qualified_name, "/do/Cat/viewTree");
        assert_eq!(vt.class_ref, "categoryAction");
        assert!(!vt.is_wildcard);
        // unnamed result defaults to "success"
        assert!(parse.results.iter().any(|r| r.name == "success" && r.result_type == "tiles"));
        assert!(parse.results.iter().any(|r| r.name == "input"));
        // concrete ActionToClass edge is not inferred
        assert!(parse
            .relations
            .iter()
            .any(|r| r.kind == RelKind::ActionToClass && r.to == "categoryAction" && !r.inferred));
    }

    #[test]
    fn wildcard_action_is_marked_and_edges_inferred() {
        let xml = r#"<struts><package name="p" namespace="/do/E">
            <action name="open*" class="pageAction" method="open{1}">
              <result type="tiles">admin.{1}</result>
            </action>
          </package></struts>"#;
        let file = crate::test_support::tmp("struts-wild.xml", xml);
        let parse = parse_include_graph(&file, &[]);
        let a = &parse.actions[0];
        assert!(a.is_wildcard);
        assert_eq!(a.method, "open{1}");
        // every edge from a wildcard action is a candidate (inferred), never exact
        assert!(parse.relations.iter().all(|r| r.inferred));
        assert!(parse.results.iter().all(|r| r.is_inferred));
    }

    #[test]
    fn include_graph_follows_and_dedups() {
        let dir = crate::test_support::tmp_dir("struts-inc");
        std::fs::create_dir_all(dir.join("com/x")).unwrap();
        std::fs::write(
            dir.join("com/x/frag.xml"),
            r#"<struts><package name="f" namespace="/do/F"><action name="a" class="beanA"/></package></struts>"#,
        )
        .unwrap();
        let root = dir.join("root.xml");
        std::fs::write(
            &root,
            r#"<struts><include file="com/x/frag.xml"/><include file="com/x/frag.xml"/><include file="com/missing/nope.xml"/></struts>"#,
        )
        .unwrap();

        let parse = parse_include_graph(&root, &[dir.clone()]);
        // frag included twice but parsed once
        assert_eq!(parse.actions.len(), 1);
        assert_eq!(parse.actions[0].qualified_name, "/do/F/a");
        // missing include reported, not fatal
        assert_eq!(parse.unresolved_includes, vec!["com/missing/nope.xml".to_string()]);
    }

    #[test]
    fn wildcard_pattern_matches_and_expands_backrefs() {
        let p = WildcardPattern::compile("editAttribute*");
        assert!(p.matches("editAttributeName"));
        assert!(p.matches("editAttribute"));
        assert!(!p.matches("removeAttributeName"));
        assert_eq!(p.prefix, "editAttribute");

        assert_eq!(WildcardPattern::expand_backrefs("open{1}", &["Error"]), "openError");
        assert_eq!(WildcardPattern::expand_backrefs("admin.{1}", &["Category"]), "admin.Category");

        let p2 = WildcardPattern::compile("save*Config*");
        assert!(p2.matches("saveUserConfigNow"));
        assert!(!p2.matches("saveUserSettings"));
    }

    #[test]
    fn join_ns_handles_empty_namespace() {
        assert_eq!(join_ns("/do/Cat", "viewTree"), "/do/Cat/viewTree");
        assert_eq!(join_ns("/do/Cat/", "viewTree"), "/do/Cat/viewTree");
        assert_eq!(join_ns("", "viewTree"), "viewTree");
    }
}
