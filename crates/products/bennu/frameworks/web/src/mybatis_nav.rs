//! MyBatis mapper-XML **navigation** — resolve the token under the caret in a mapper
//! `.xml` to what it points at, so Ctrl+Click / Ctrl+B works *inside* a mapper the way it
//! does in a JSP.
//!
//! Where [`crate::mybatis`] parses a mapper into records for the config graph, this module
//! answers a single go-to query against one file's DOM: given a byte `offset`, classify the
//! attribute value it sits in and return a [`MybatisRef`]. The intra-file jumps (an
//! `<include refid>` → its `<sql>`, a statement `resultMap="…"` → its `<resultMap>`) are
//! resolved here to a concrete byte offset — no index, single file, exactly like
//! [`crate::jsp_vars`]. The cross-boundary jumps (a statement `id` → the Java interface
//! method, the mapper `namespace` → the Java interface, a **qualified** `refid`/`resultMap`
//! into another namespace) are returned **symbolically** for the be-layer to resolve against
//! the class index / mapper set.
//!
//! Pure over [`crate::xml`] — unit-tested off in-memory fixtures.

use crate::xml;

/// A named cross-file MyBatis fragment kind — a top-level `<sql>` or `<resultMap>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FragmentKind {
    Sql,
    ResultMap,
}

impl FragmentKind {
    /// The mapper child element name (`sql` / `resultMap`).
    pub fn as_tag(&self) -> &'static str {
        match self {
            FragmentKind::Sql => "sql",
            FragmentKind::ResultMap => "resultMap",
        }
    }
}

/// What the token under the caret in a mapper XML resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MybatisRef {
    /// On a `<select|insert|update|delete id="name">` id → the Java interface **method**
    /// (`namespace#name`). The be-layer resolves `namespace` to the interface `.java` and
    /// finds the method — the XML→Java jump.
    Method { namespace: String, name: String },
    /// On the mapper `namespace="com.x.FooMapper"` value → the Java interface type.
    Interface { fqcn: String },
    /// An intra-file jump already resolved to a byte `offset` (an `<include refid>` → the
    /// `<sql id>` in this file, a statement `resultMap="…"` → the `<resultMap id>` here).
    Local { offset: usize },
    /// A **qualified** reference into another mapper namespace (`refid="ns.frag"` /
    /// `resultMap="ns.map"`). The be-layer opens that namespace's mapper file and finds the
    /// `<sql|resultMap id>`.
    Fragment {
        namespace: String,
        kind: FragmentKind,
        id: String,
    },
}

/// Resolve the mapper-XML token at byte `offset`. Returns `None` when `source` isn't a
/// `<mapper namespace=…>` document or the caret isn't on a navigable attribute value.
pub fn resolve_mybatis_ref(source: &str, offset: usize) -> Option<MybatisRef> {
    let doc = xml::parse(source)?;
    let root = doc.root_element();
    if !root.has_tag_name("mapper") {
        return None;
    }
    let namespace = root.attribute("namespace")?;

    // 1. On the mapper `namespace` value → the Java interface type.
    if attr_range_contains(&root, "namespace", offset) {
        return Some(MybatisRef::Interface { fqcn: namespace.to_string() });
    }

    // 2. Walk every element; find the attribute value the caret sits in.
    for node in root.descendants().filter(roxmltree::Node::is_element) {
        match node.tag_name().name() {
            "select" | "insert" | "update" | "delete" => {
                // The statement id → the Java interface method (XML→Java).
                if attr_range_contains(&node, "id", offset) {
                    if let Some(id) = node.attribute("id") {
                        return Some(MybatisRef::Method {
                            namespace: namespace.to_string(),
                            name: id.to_string(),
                        });
                    }
                }
                // A `resultMap="…"` on the statement → its `<resultMap>` declaration.
                if attr_range_contains(&node, "resultMap", offset) {
                    if let Some(rm) = node.attribute("resultMap") {
                        return Some(resolve_fragment(
                            &root,
                            namespace,
                            FragmentKind::ResultMap,
                            rm,
                        ));
                    }
                }
            }
            // `<include refid="…">` → the `<sql>` fragment.
            "include" => {
                if attr_range_contains(&node, "refid", offset) {
                    if let Some(refid) = node.attribute("refid") {
                        return Some(resolve_fragment(&root, namespace, FragmentKind::Sql, refid));
                    }
                }
            }
            _ => {}
        }
    }
    None
}

/// Resolve a fragment reference (`refid` / `resultMap` value). Prefers a **same-file**
/// declaration (→ [`MybatisRef::Local`] at its byte offset); otherwise a dotted value is a
/// **qualified** cross-namespace reference (→ [`MybatisRef::Fragment`]); a bare value with
/// no local declaration is treated as this namespace's fragment (defined in a sibling file).
fn resolve_fragment(
    root: &roxmltree::Node,
    namespace: &str,
    kind: FragmentKind,
    value: &str,
) -> MybatisRef {
    if let Some(offset) = find_decl(root, kind.as_tag(), value) {
        return MybatisRef::Local { offset };
    }
    if let Some((ns, id)) = value.rsplit_once('.') {
        return MybatisRef::Fragment {
            namespace: ns.to_string(),
            kind,
            id: id.to_string(),
        };
    }
    MybatisRef::Fragment {
        namespace: namespace.to_string(),
        kind,
        id: value.to_string(),
    }
}

/// Byte offset of the `id` value of a top-level `<tag id="id">` child of `<mapper>`, or
/// `None`. `<sql>` / `<resultMap>` are declared as direct mapper children.
fn find_decl(root: &roxmltree::Node, tag: &str, id: &str) -> Option<usize> {
    root.children()
        .filter(roxmltree::Node::is_element)
        .find(|n| n.tag_name().name() == tag && n.attribute("id") == Some(id))
        .and_then(|n| {
            n.attributes()
                .find(|a| a.name() == "id")
                .map(|a| a.range_value().start)
        })
}

/// True when `offset` falls within (inclusive of both ends of) the value span of `node`'s
/// `attr` attribute.
fn attr_range_contains(node: &roxmltree::Node, attr: &str, offset: usize) -> bool {
    node.attributes()
        .find(|a| a.name() == attr)
        .map(|a| {
            let r = a.range_value();
            offset >= r.start && offset <= r.end
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Byte offset of the first character INSIDE the quoted value of `attr_eq` (a substring
    /// like `id="findById"`) — lands the caret on the value, not the attribute name.
    fn value_off(src: &str, attr_eq: &str) -> usize {
        let start = src.find(attr_eq).expect("attr present");
        start + attr_eq.find('"').expect("quote") + 1
    }

    const MAPPER: &str = r#"<mapper namespace="com.x.FooMapper">
  <sql id="cols">a, b, c</sql>
  <resultMap id="fooResult" type="com.x.Foo"/>
  <select id="findById" resultMap="fooResult">
    select <include refid="cols"/> from foo where id = #{id}
  </select>
  <select id="all" resultMap="shared.BarMap">
    select <include refid="other.NsMapper.baseCols"/> from bar
  </select>
</mapper>"#;

    #[test]
    fn statement_id_resolves_to_the_java_method() {
        let off = value_off(MAPPER, r#"id="findById""#);
        match resolve_mybatis_ref(MAPPER, off) {
            Some(MybatisRef::Method { namespace, name }) => {
                assert_eq!(namespace, "com.x.FooMapper");
                assert_eq!(name, "findById");
            }
            other => panic!("expected Method, got {other:?}"),
        }
    }

    #[test]
    fn namespace_resolves_to_the_interface() {
        let off = value_off(MAPPER, r#"namespace="com.x.FooMapper""#);
        match resolve_mybatis_ref(MAPPER, off) {
            Some(MybatisRef::Interface { fqcn }) => assert_eq!(fqcn, "com.x.FooMapper"),
            other => panic!("expected Interface, got {other:?}"),
        }
    }

    #[test]
    fn include_refid_jumps_to_the_local_sql_fragment() {
        let off = value_off(MAPPER, r#"refid="cols""#);
        match resolve_mybatis_ref(MAPPER, off) {
            Some(MybatisRef::Local { offset }) => {
                // The offset must point at the `<sql id="cols">` value.
                assert_eq!(&MAPPER[offset..offset + 4], "cols");
            }
            other => panic!("expected Local, got {other:?}"),
        }
    }

    #[test]
    fn statement_result_map_jumps_to_the_local_result_map() {
        let off = value_off(MAPPER, r#"resultMap="fooResult""#);
        match resolve_mybatis_ref(MAPPER, off) {
            Some(MybatisRef::Local { offset }) => {
                assert_eq!(&MAPPER[offset..offset + 9], "fooResult");
            }
            other => panic!("expected Local, got {other:?}"),
        }
    }

    #[test]
    fn qualified_include_refid_is_a_cross_namespace_fragment() {
        let off = value_off(MAPPER, r#"refid="other.NsMapper.baseCols""#);
        match resolve_mybatis_ref(MAPPER, off) {
            Some(MybatisRef::Fragment { namespace, kind, id }) => {
                assert_eq!(namespace, "other.NsMapper");
                assert_eq!(kind, FragmentKind::Sql);
                assert_eq!(id, "baseCols");
            }
            other => panic!("expected Fragment, got {other:?}"),
        }
    }

    #[test]
    fn qualified_result_map_is_a_cross_namespace_fragment() {
        let off = value_off(MAPPER, r#"resultMap="shared.BarMap""#);
        match resolve_mybatis_ref(MAPPER, off) {
            Some(MybatisRef::Fragment { namespace, kind, id }) => {
                assert_eq!(namespace, "shared");
                assert_eq!(kind, FragmentKind::ResultMap);
                assert_eq!(id, "BarMap");
            }
            other => panic!("expected Fragment, got {other:?}"),
        }
    }

    #[test]
    fn caret_off_any_reference_is_none() {
        // Inside the SQL text body, not on a navigable attribute value.
        let off = MAPPER.find("from foo").unwrap() + 1;
        assert_eq!(resolve_mybatis_ref(MAPPER, off), None);
    }

    #[test]
    fn non_mapper_document_is_none() {
        let beans = r#"<beans><bean id="a" class="C"/></beans>"#;
        assert_eq!(resolve_mybatis_ref(beans, 10), None);
    }

    #[test]
    fn sql_declaration_id_itself_is_not_navigable() {
        // The caret on the `<sql id="cols">` declaration (not the <include>) shouldn't
        // resolve — it's the target, not a reference. (Find-usages is a separate flow.)
        let decl = r#"<mapper namespace="com.x.M"><sql id="cols">a</sql></mapper>"#;
        let off = decl.find(r#"id="cols""#).unwrap() + 5;
        assert_eq!(resolve_mybatis_ref(decl, off), None);
    }
}
