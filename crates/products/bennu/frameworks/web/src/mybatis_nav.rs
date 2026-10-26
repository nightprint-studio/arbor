//! MyBatis mapper-XML **navigation** — resolve the token under the caret in a mapper
//! `.xml` to what it points at, so Ctrl+Click / Ctrl+B works *inside* a mapper the way it
//! does in a JSP.
//!
//! Where [`crate::mybatis`] parses a mapper into records for the config graph, this module
//! answers a single go-to query against one file's DOM: given a byte `offset`, classify the
//! attribute value it sits in and return a [`MybatisRef`]. The intra-file jumps (an
//! `<include refid>` → its `<sql>`, a `resultMap="…"` → its `<resultMap>`, an association's
//! `select="…"` → its statement) are resolved here to a concrete byte offset — no index, single
//! file, exactly like [`crate::jsp_vars`]. The cross-boundary jumps (a statement `id` → the Java
//! interface method, a `type` / `resultType` → a Java class, a `property` → the Java member, a
//! **qualified** reference into another namespace) are returned **symbolically** for the
//! be-layer to resolve against the class index / mapper set.
//!
//! The same classification feeds find-usages: [`fragment_usages`] lists every reference to a
//! `<sql>` / `<resultMap>` in one file.
//!
//! Pure over [`crate::xml`] — unit-tested off in-memory fixtures.

use crate::xml;

/// A named MyBatis declaration kind — a top-level `<sql>`, `<resultMap>` or statement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FragmentKind {
    Sql,
    ResultMap,
    /// A `<select|insert|update|delete>` — what an association's `select="…"` names.
    Statement,
}

impl FragmentKind {
    /// Whether a mapper child element named `tag` declares this kind.
    pub fn declares(&self, tag: &str) -> bool {
        match self {
            FragmentKind::Sql => tag == "sql",
            FragmentKind::ResultMap => tag == "resultMap",
            FragmentKind::Statement => STATEMENTS.contains(&tag),
        }
    }

    /// The attributes whose value refers to a declaration of this kind, with the elements each
    /// appears on — what find-usages scans for.
    fn reference_attributes(&self) -> &'static [(&'static str, &'static [&'static str])] {
        match self {
            FragmentKind::Sql => &[("refid", &["include"])],
            FragmentKind::ResultMap => &[
                ("resultMap", &["select", "insert", "update", "delete", "association", "collection", "case"]),
                ("extends", &["resultMap"]),
            ],
            FragmentKind::Statement => &[("select", &["association", "collection"])],
        }
    }
}

const STATEMENTS: &[&str] = &["select", "insert", "update", "delete"];

/// Attributes whose value names a Java type (a FQCN, or an alias the be-layer resolves).
const TYPE_ATTRS: &[&str] = &["parameterType", "resultType", "javaType", "ofType", "typeHandler", "type"];

/// Elements whose `property` attribute names a member of the mapped Java type.
const PROPERTY_ELEMENTS: &[&str] = &["result", "id", "association", "collection", "arg", "idArg"];

/// What the token under the caret in a mapper XML resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MybatisRef {
    /// On a `<select|insert|update|delete id="name">` id → the Java interface **method**
    /// (`namespace#name`). The be-layer resolves `namespace` to the interface `.java` and
    /// finds the method — the XML→Java jump.
    Method { namespace: String, name: String },
    /// On the mapper `namespace="com.x.FooMapper"` value → the Java interface type.
    Interface { fqcn: String },
    /// An intra-file jump already resolved to a byte `offset` — the declaration of the
    /// referenced `<sql>` / `<resultMap>` / statement `id` in this file.
    Local { offset: usize, kind: FragmentKind, id: String },
    /// A **qualified** reference into another mapper namespace (`refid="ns.frag"` /
    /// `resultMap="ns.map"`), or a bare one declared in a sibling file. The be-layer opens that
    /// namespace's mapper file and finds the declaration.
    Fragment {
        namespace: String,
        kind: FragmentKind,
        id: String,
    },
    /// An attribute value naming a Java type (`type`, `resultType`, `parameterType`, …).
    Type { name: String },
    /// A `property="…"` of a result mapping: the member `name` of `owner`, the Java type the
    /// enclosing `<resultMap type>` (or association's `javaType`) maps — when it names one.
    Property { owner: Option<String>, name: String },
    /// On the `id` of a `<sql>` / `<resultMap>` declaration itself. Nothing to jump to — it is the
    /// target — but it is what find-usages asks about.
    Declaration { kind: FragmentKind, id: String },
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

    // Every element, the mapper root included (its `namespace`): the attribute value the caret
    // sits in decides what it is.
    for node in root.descendants().filter(roxmltree::Node::is_element) {
        for attr in node.attributes() {
            let range = attr.range_value();
            if offset < range.start || offset > range.end {
                continue;
            }
            return classify(&root, namespace, &node, attr.name(), attr.value());
        }
    }
    None
}

/// What attribute `name` = `value` on `node` means in a mapper.
fn classify(
    root: &roxmltree::Node,
    namespace: &str,
    node: &roxmltree::Node,
    name: &str,
    value: &str,
) -> Option<MybatisRef> {
    let tag = node.tag_name().name();
    let is_statement = STATEMENTS.contains(&tag);
    if value.is_empty() {
        return None;
    }
    match name {
        "namespace" if tag == "mapper" || tag == "cache-ref" => {
            Some(MybatisRef::Interface { fqcn: value.to_string() })
        }
        "id" if is_statement => Some(MybatisRef::Method {
            namespace: namespace.to_string(),
            name: value.to_string(),
        }),
        "id" if tag == "resultMap" => {
            Some(MybatisRef::Declaration { kind: FragmentKind::ResultMap, id: value.to_string() })
        }
        "id" if tag == "sql" => {
            Some(MybatisRef::Declaration { kind: FragmentKind::Sql, id: value.to_string() })
        }
        "resultMap" if is_statement || matches!(tag, "association" | "collection" | "case") => {
            Some(resolve_fragment(root, namespace, FragmentKind::ResultMap, value))
        }
        "extends" if tag == "resultMap" => {
            Some(resolve_fragment(root, namespace, FragmentKind::ResultMap, value))
        }
        "refid" if tag == "include" => {
            Some(resolve_fragment(root, namespace, FragmentKind::Sql, value))
        }
        "select" if matches!(tag, "association" | "collection") => {
            Some(resolve_fragment(root, namespace, FragmentKind::Statement, value))
        }
        "property" if PROPERTY_ELEMENTS.contains(&tag) => Some(MybatisRef::Property {
            owner: type_context(node),
            name: value.to_string(),
        }),
        // A type written as an `${…}` / `#{…}` expression names nothing yet.
        _ if TYPE_ATTRS.contains(&name) && !value.contains(['$', '#']) => {
            Some(MybatisRef::Type { name: value.to_string() })
        }
        _ => None,
    }
}

/// The Java type the members under `node` belong to: the nearest enclosing `<resultMap type>`,
/// `<association javaType>` or `<collection ofType>`.
fn type_context(node: &roxmltree::Node) -> Option<String> {
    // The nearest container decides, even when it names no type: an `<association>` without a
    // `javaType` infers it from the owner, and the outer `<resultMap type>` is not its type.
    let container = node
        .ancestors()
        .skip(1)
        .find(|n| matches!(n.tag_name().name(), "resultMap" | "association" | "collection"))?;
    let attr = match container.tag_name().name() {
        "resultMap" => "type",
        "association" => "javaType",
        _ => "ofType",
    };
    container.attribute(attr).map(str::to_string)
}

/// Resolve a declaration reference (`refid` / `resultMap` / `select` value). Prefers a **same-file**
/// declaration (→ [`MybatisRef::Local`] at its byte offset); otherwise a dotted value is a
/// **qualified** cross-namespace reference (→ [`MybatisRef::Fragment`]); a bare value with
/// no local declaration is treated as this namespace's declaration (defined in a sibling file).
fn resolve_fragment(
    root: &roxmltree::Node,
    namespace: &str,
    kind: FragmentKind,
    value: &str,
) -> MybatisRef {
    if let Some(offset) = find_decl(root, kind, value) {
        return MybatisRef::Local { offset, kind, id: value.to_string() };
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

/// Byte offset of the `id` value of a top-level declaration child of `<mapper>`, or `None`.
fn find_decl(root: &roxmltree::Node, kind: FragmentKind, id: &str) -> Option<usize> {
    root.children()
        .filter(roxmltree::Node::is_element)
        .find(|n| kind.declares(n.tag_name().name()) && n.attribute("id") == Some(id))
        .and_then(|n| {
            n.attributes()
                .find(|a| a.name() == "id")
                .map(|a| a.range_value().start)
        })
}

/// Byte offset of the `id` value of the `kind` declaration `id` in the mapper `source`, or `None` —
/// for resolving a [`MybatisRef::Fragment`] in the sibling file that declares it.
pub fn declaration_offset(source: &str, kind: FragmentKind, id: &str) -> Option<usize> {
    let doc = xml::parse(source)?;
    let root = doc.root_element();
    root.has_tag_name("mapper").then(|| find_decl(&root, kind, id)).flatten()
}

/// The `namespace` of the mapper `source`, or `None` when it isn't one.
pub fn mapper_namespace(source: &str) -> Option<String> {
    let doc = xml::parse(source)?;
    let root = doc.root_element();
    root.has_tag_name("mapper").then(|| root.attribute("namespace").map(str::to_string)).flatten()
}

/// Every reference in the mapper `source` to the `kind` declaration `id` of namespace `target_ns`,
/// as `(start, end)` byte spans of the referencing attribute value.
///
/// A bare value refers to the file's **own** namespace, so it counts only when this file is
/// `target_ns`'s; a qualified `target_ns.id` counts anywhere.
pub fn fragment_usages(source: &str, target_ns: &str, kind: FragmentKind, id: &str) -> Vec<(usize, usize)> {
    let Some(doc) = xml::parse(source) else { return Vec::new() };
    let root = doc.root_element();
    if !root.has_tag_name("mapper") {
        return Vec::new();
    }
    let own = root.attribute("namespace").unwrap_or("");
    let qualified = format!("{target_ns}.{id}");
    let mut out = Vec::new();
    for node in root.descendants().filter(roxmltree::Node::is_element) {
        let tag = node.tag_name().name();
        for (attr_name, elements) in kind.reference_attributes() {
            if !elements.contains(&tag) {
                continue;
            }
            let Some(attr) = node.attributes().find(|a| a.name() == *attr_name) else { continue };
            let value = attr.value();
            if (own == target_ns && value == id) || value == qualified {
                let r = attr.range_value();
                out.push((r.start, r.end));
            }
        }
    }
    out
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
  <resultMap id="fooResult" type="com.x.Foo" extends="baseResult">
    <result property="name" column="NAME"/>
    <association property="owner" javaType="com.x.Owner" select="loadOwner">
      <result property="email" column="E"/>
    </association>
  </resultMap>
  <select id="findById" resultMap="fooResult" parameterType="long">
    select <include refid="cols"/> from foo where id = #{id}
  </select>
  <select id="loadOwner" resultType="com.x.Owner">select 1</select>
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
            Some(MybatisRef::Local { offset, kind, id }) => {
                // The offset must point at the `<sql id="cols">` value.
                assert_eq!(&MAPPER[offset..offset + 4], "cols");
                assert_eq!((kind, id.as_str()), (FragmentKind::Sql, "cols"));
            }
            other => panic!("expected Local, got {other:?}"),
        }
    }

    #[test]
    fn statement_result_map_jumps_to_the_local_result_map() {
        let off = value_off(MAPPER, r#"resultMap="fooResult""#);
        match resolve_mybatis_ref(MAPPER, off) {
            Some(MybatisRef::Local { offset, .. }) => {
                assert_eq!(&MAPPER[offset..offset + 9], "fooResult");
            }
            other => panic!("expected Local, got {other:?}"),
        }
    }

    #[test]
    fn an_association_select_jumps_to_the_statement() {
        let off = value_off(MAPPER, r#"select="loadOwner""#);
        match resolve_mybatis_ref(MAPPER, off) {
            Some(MybatisRef::Local { offset, kind, .. }) => {
                assert_eq!(&MAPPER[offset..offset + 9], "loadOwner");
                assert_eq!(kind, FragmentKind::Statement);
            }
            other => panic!("expected Local, got {other:?}"),
        }
    }

    #[test]
    fn a_result_map_extends_is_a_reference_to_another_result_map() {
        let off = value_off(MAPPER, r#"extends="baseResult""#);
        match resolve_mybatis_ref(MAPPER, off) {
            Some(MybatisRef::Fragment { namespace, kind, id }) => {
                assert_eq!((namespace.as_str(), kind, id.as_str()), ("com.x.FooMapper", FragmentKind::ResultMap, "baseResult"));
            }
            other => panic!("expected Fragment, got {other:?}"),
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
    fn java_type_attributes_resolve_to_a_type() {
        for (attr_eq, want) in [
            (r#"type="com.x.Foo""#, "com.x.Foo"),
            (r#"parameterType="long""#, "long"),
            (r#"resultType="com.x.Owner""#, "com.x.Owner"),
            (r#"javaType="com.x.Owner""#, "com.x.Owner"),
        ] {
            let off = value_off(MAPPER, attr_eq);
            assert_eq!(
                resolve_mybatis_ref(MAPPER, off),
                Some(MybatisRef::Type { name: want.to_string() }),
                "{attr_eq}"
            );
        }
    }

    #[test]
    fn a_property_belongs_to_the_enclosing_result_maps_type() {
        let off = value_off(MAPPER, r#"property="name""#);
        assert_eq!(
            resolve_mybatis_ref(MAPPER, off),
            Some(MybatisRef::Property { owner: Some("com.x.Foo".into()), name: "name".into() })
        );
    }

    #[test]
    fn a_property_inside_an_association_belongs_to_its_java_type() {
        let off = value_off(MAPPER, r#"property="email""#);
        assert_eq!(
            resolve_mybatis_ref(MAPPER, off),
            Some(MybatisRef::Property { owner: Some("com.x.Owner".into()), name: "email".into() })
        );
    }

    #[test]
    fn a_declaration_id_is_reported_as_one() {
        let off = value_off(MAPPER, r#"id="cols""#);
        assert_eq!(
            resolve_mybatis_ref(MAPPER, off),
            Some(MybatisRef::Declaration { kind: FragmentKind::Sql, id: "cols".into() })
        );
        let off = value_off(MAPPER, r#"id="fooResult""#);
        assert_eq!(
            resolve_mybatis_ref(MAPPER, off),
            Some(MybatisRef::Declaration { kind: FragmentKind::ResultMap, id: "fooResult".into() })
        );
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
    fn declarations_are_found_by_kind_and_id() {
        let off = declaration_offset(MAPPER, FragmentKind::ResultMap, "fooResult").unwrap();
        assert_eq!(&MAPPER[off..off + 9], "fooResult");
        assert!(declaration_offset(MAPPER, FragmentKind::Sql, "fooResult").is_none());
    }

    #[test]
    fn usages_of_a_result_map_are_its_bare_and_qualified_references() {
        let uses = fragment_usages(MAPPER, "com.x.FooMapper", FragmentKind::ResultMap, "fooResult");
        let texts: Vec<&str> = uses.iter().map(|(s, e)| &MAPPER[*s..*e]).collect();
        assert_eq!(texts, ["fooResult"]);
    }

    #[test]
    fn a_bare_reference_in_another_namespace_is_not_a_usage() {
        let other = r#"<mapper namespace="a.Other"><select id="x" resultMap="fooResult"/><select id="y" resultMap="com.x.FooMapper.fooResult"/></mapper>"#;
        let uses = fragment_usages(other, "com.x.FooMapper", FragmentKind::ResultMap, "fooResult");
        let texts: Vec<&str> = uses.iter().map(|(s, e)| &other[*s..*e]).collect();
        assert_eq!(texts, ["com.x.FooMapper.fooResult"]);
    }

    #[test]
    fn usages_of_a_sql_fragment_are_its_includes() {
        let uses = fragment_usages(MAPPER, "com.x.FooMapper", FragmentKind::Sql, "cols");
        assert_eq!(uses.len(), 1);
        assert_eq!(&MAPPER[uses[0].0..uses[0].1], "cols");
    }
}
