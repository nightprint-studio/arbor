//! MyBatis mapper-XML **validation sites** — the places in a mapper whose claim the project can
//! check: a result `property` that names a member of the mapped class, a Java type written out in
//! full, a reference to a `<resultMap>` / `<sql>` / statement, an id declared twice.
//!
//! Pure over [`crate::xml`], like [`crate::mybatis_nav`]: this module only **finds** the sites and
//! says what each one claims. Whether the claim holds is the be-layer's question (it owns the class
//! index and the other mappers), which keeps the scan testable off in-memory fixtures.
//!
//! The scan is deliberately **strict about what it hands on**: a site whose owner it cannot name for
//! certain (an `<association>` with no `javaType`, a `<case>` with its own result type) is not
//! emitted at all, because the be-layer reports every site it receives that fails to resolve.

use crate::mybatis_nav::FragmentKind;
use crate::xml;

/// A result `property="…"` and the Java type it must be a member of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertySite {
    /// The type as written (`com.x.Foo`, or an alias such as `Foo`).
    pub owner: String,
    /// The property as written, a path (`address.city`) included.
    pub property: String,
    /// Byte span of the attribute **value**.
    pub span: (usize, usize),
}

/// A Java type written out in an attribute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeSite {
    pub name: String,
    pub span: (usize, usize),
}

/// A reference to a declaration (`refid`, `resultMap`, `extends`, an association's `select`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefSite {
    pub kind: FragmentKind,
    /// The value as written — bare (`byId`) or qualified (`com.x.OtherMapper.byId`).
    pub value: String,
    pub span: (usize, usize),
}

/// A declaration whose id was already taken earlier in the same mapper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateId {
    pub id: String,
    /// What was declared twice (`statement`, `resultMap`, `sql`).
    pub what: &'static str,
    pub span: (usize, usize),
}

/// A `<select|insert|update|delete id="…">` declared directly in the mapper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatementSite {
    pub id: String,
    /// Byte span of the `id` attribute **value**.
    pub span: (usize, usize),
}

/// Everything in one mapper the project can be asked about.
#[derive(Debug, Default)]
pub struct MapperScan {
    pub namespace: String,
    /// Statements by id; one per distinct id (a `databaseId` variant of an id already listed is not
    /// repeated), so "the interface has no method for it" is said once.
    pub statements: Vec<StatementSite>,
    pub properties: Vec<PropertySite>,
    pub types: Vec<TypeSite>,
    pub refs: Vec<RefSite>,
    pub duplicates: Vec<DuplicateId>,
}

const STATEMENTS: &[&str] = &["select", "insert", "update", "delete"];

/// `(attribute, elements it is checked on)` for a Java type written out in full. `typeHandler` and
/// `<cache type>` name library or app classes the project index may legitimately not hold.
const TYPE_ATTRIBUTES: &[(&str, &[&str])] = &[
    ("type", &["resultMap", "parameterMap"]),
    ("resultType", &["select", "case"]),
    ("parameterType", &["select", "insert", "update", "delete"]),
    ("javaType", &["association", "result", "id", "arg", "idArg"]),
    ("ofType", &["collection"]),
];

/// Scan a mapper. `None` when `source` is not a `<mapper namespace=…>` document.
pub fn scan_mapper(source: &str) -> Option<MapperScan> {
    let doc = xml::parse(source)?;
    let root = doc.root_element();
    if !root.has_tag_name("mapper") {
        return None;
    }
    let mut scan = MapperScan { namespace: root.attribute("namespace")?.to_string(), ..Default::default() };
    scan.duplicates = duplicate_ids(&root);
    scan.statements = statement_sites(&root);
    for node in root.descendants().filter(roxmltree::Node::is_element) {
        collect_property(&node, &mut scan);
        collect_types(&node, &mut scan);
        collect_refs(&node, &mut scan);
    }
    Some(scan)
}

fn value_span(node: &roxmltree::Node, name: &str) -> Option<((usize, usize), String)> {
    let attr = node.attributes().find(|a| a.name() == name)?;
    let range = attr.range_value();
    Some(((range.start, range.end), attr.value().to_string()))
}

/// A value that is an expression (`${…}` / `#{…}`) or blank names nothing yet.
fn is_literal(value: &str) -> bool {
    !value.trim().is_empty() && !value.contains(['$', '#'])
}

// ── properties ──────────────────────────────────────────────────────────────────────

fn collect_property(node: &roxmltree::Node, scan: &mut MapperScan) {
    if !matches!(node.tag_name().name(), "result" | "id" | "association" | "collection") {
        return;
    }
    let Some((span, property)) = value_span(node, "property") else { return };
    if !is_literal(&property) {
        return;
    }
    if let Some(owner) = strict_owner(node) {
        scan.properties.push(PropertySite { owner, property, span });
    }
}

/// The Java type the `property` of `node` belongs to — the **nearest** enclosing result container,
/// and `None` unless that container states its type outright.
///
/// Not the nearest one that happens to carry a type: an `<association>` with no `javaType` infers it
/// from the owner's property, and reading past it to the outer `<resultMap type>` would judge the
/// nested properties against the wrong class — a false error on every one of them. A `<discriminator>`
/// / `<case>` is the same story, with a result type of its own.
fn strict_owner(node: &roxmltree::Node) -> Option<String> {
    for ancestor in node.ancestors().skip(1) {
        let attr = match ancestor.tag_name().name() {
            "resultMap" => "type",
            "association" => "javaType",
            "collection" => "ofType",
            "discriminator" | "case" => return None,
            _ => continue,
        };
        return ancestor.attribute(attr).filter(|v| is_literal(v)).map(str::to_string);
    }
    None
}

// ── types ───────────────────────────────────────────────────────────────────────────

fn collect_types(node: &roxmltree::Node, scan: &mut MapperScan) {
    let tag = node.tag_name().name();
    for (attr, elements) in TYPE_ATTRIBUTES {
        if !elements.contains(&tag) {
            continue;
        }
        let Some((span, name)) = value_span(node, attr) else { continue };
        // An alias or a built-in (`long`, `map`) is not written with a package; only a qualified
        // name is something the project index can contradict.
        if is_literal(&name) && name.contains('.') && !name.contains(char::is_whitespace) {
            scan.types.push(TypeSite { name, span });
        }
    }
}

// ── references ──────────────────────────────────────────────────────────────────────

fn collect_refs(node: &roxmltree::Node, scan: &mut MapperScan) {
    let tag = node.tag_name().name();
    let mut push = |kind, attr: &str| {
        if let Some((span, value)) = value_span(node, attr) {
            if is_literal(&value) {
                scan.refs.push(RefSite { kind, value, span });
            }
        }
    };
    if tag == "include" {
        push(FragmentKind::Sql, "refid");
    }
    if STATEMENTS.contains(&tag) || matches!(tag, "association" | "collection" | "case") {
        push(FragmentKind::ResultMap, "resultMap");
    }
    if tag == "resultMap" {
        push(FragmentKind::ResultMap, "extends");
    }
    if matches!(tag, "association" | "collection") {
        push(FragmentKind::Statement, "select");
    }
}

// ── statements ──────────────────────────────────────────────────────────────────────

fn statement_sites(root: &roxmltree::Node) -> Vec<StatementSite> {
    let mut seen = std::collections::HashSet::new();
    root.children()
        .filter(roxmltree::Node::is_element)
        .filter(|n| STATEMENTS.contains(&n.tag_name().name()))
        .filter_map(|n| value_span(&n, "id"))
        .filter(|(_, id)| is_literal(id) && seen.insert(id.clone()))
        .map(|(span, id)| StatementSite { id, span })
        .collect()
}

// ── duplicates ──────────────────────────────────────────────────────────────────────

/// Ids declared twice among the mapper's children. A `databaseId` makes two declarations of one id
/// legitimate (one per vendor), so it is part of the key.
fn duplicate_ids(root: &roxmltree::Node) -> Vec<DuplicateId> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for node in root.children().filter(roxmltree::Node::is_element) {
        let tag = node.tag_name().name();
        let what = match tag {
            t if STATEMENTS.contains(&t) => "statement",
            "resultMap" => "resultMap",
            "sql" => "sql",
            _ => continue,
        };
        let Some((span, id)) = value_span(&node, "id") else { continue };
        let database = node.attribute("databaseId").unwrap_or("").to_string();
        if !seen.insert((what, id.clone(), database)) {
            out.push(DuplicateId { id, what, span });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text<'a>(src: &'a str, span: (usize, usize)) -> &'a str {
        &src[span.0..span.1]
    }

    const MAPPER: &str = r#"<mapper namespace="com.x.FooMapper">
  <resultMap id="fooResult" type="com.x.Foo">
    <id property="id" column="ID"/>
    <result property="name" column="NAME"/>
    <association property="owner" javaType="com.x.Owner">
      <result property="email" column="E"/>
    </association>
    <association property="inferred" select="loadOwner">
      <result property="mystery" column="M"/>
    </association>
    <discriminator column="K"><case value="1" resultType="com.x.Special"><result property="special" column="S"/></case></discriminator>
  </resultMap>
  <select id="findById" resultMap="fooResult" parameterType="com.x.Query">select <include refid="cols"/></select>
  <select id="findById" databaseId="oracle">select 1</select>
  <select id="findById">select 2</select>
</mapper>"#;

    #[test]
    fn a_property_is_judged_against_its_nearest_container() {
        let scan = scan_mapper(MAPPER).unwrap();
        let sites: Vec<(&str, &str)> =
            scan.properties.iter().map(|p| (p.owner.as_str(), text(MAPPER, p.span))).collect();
        assert!(sites.contains(&("com.x.Foo", "name")));
        assert!(sites.contains(&("com.x.Foo", "owner")), "an association's own property belongs to the outer map");
        assert!(sites.contains(&("com.x.Owner", "email")));
    }

    #[test]
    fn an_association_without_a_java_type_leaves_its_children_unjudged() {
        let scan = scan_mapper(MAPPER).unwrap();
        assert!(scan.properties.iter().all(|p| text(MAPPER, p.span) != "mystery"));
    }

    #[test]
    fn a_discriminator_case_is_not_judged_against_the_outer_type() {
        let scan = scan_mapper(MAPPER).unwrap();
        assert!(scan.properties.iter().all(|p| text(MAPPER, p.span) != "special"));
    }

    #[test]
    fn only_a_qualified_type_is_a_type_site() {
        let src = r#"<mapper namespace="a.M"><select id="x" resultType="long" parameterType="a.b.Q"/></mapper>"#;
        let scan = scan_mapper(src).unwrap();
        let names: Vec<&str> = scan.types.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["a.b.Q"]);
    }

    #[test]
    fn references_are_collected_by_kind() {
        let scan = scan_mapper(MAPPER).unwrap();
        let kinds: Vec<(FragmentKind, &str)> = scan.refs.iter().map(|r| (r.kind, r.value.as_str())).collect();
        assert!(kinds.contains(&(FragmentKind::ResultMap, "fooResult")));
        assert!(kinds.contains(&(FragmentKind::Sql, "cols")));
        assert!(kinds.contains(&(FragmentKind::Statement, "loadOwner")));
    }

    #[test]
    fn an_expression_is_not_a_reference() {
        let src = r#"<mapper namespace="a.M"><select id="x"><include refid="${frag}"/></select></mapper>"#;
        assert!(scan_mapper(src).unwrap().refs.is_empty());
    }

    #[test]
    fn a_database_id_makes_a_repeated_id_legitimate() {
        let scan = scan_mapper(MAPPER).unwrap();
        // The default and the oracle variant coexist; only the repeated *default* is a duplicate...
        // here `findById` appears first without a databaseId, then with one, then without again.
        assert_eq!(scan.duplicates.len(), 1);
        assert_eq!((scan.duplicates[0].id.as_str(), scan.duplicates[0].what), ("findById", "statement"));
    }

    #[test]
    fn a_statement_is_listed_once_however_many_database_variants_it_has() {
        let scan = scan_mapper(MAPPER).unwrap();
        let ids: Vec<&str> = scan.statements.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["findById"]);
        assert_eq!(text(MAPPER, scan.statements[0].span), "findById");
    }

    #[test]
    fn a_document_that_is_not_a_mapper_has_no_sites() {
        assert!(scan_mapper(r#"<beans><bean id="a"/></beans>"#).is_none());
    }
}
