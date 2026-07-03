//! MyBatis mapper-XML parser — `<mapper namespace="com.x.FooMapper">` + its
//! `<select|insert|update|delete id="bar">` statements.
//!
//! A mapper XML is the interceptor pattern, one layer flatter: a package-scoped named
//! record set (`<mapper namespace=>`, like an `<interceptors>` block) whose children are
//! name-keyed statements (`<... id=>`, like an `<interceptor name=>` def). The Java→XML
//! link is `interface FQCN + method name → statement id` — exactly the interceptor model:
//! **package-scoped names, no global symbol, resolved off the parsed graph by name**. So
//! statements get no fst id; go-to rides the parsed [`StatementRecord`] byte offsets.
//!
//! Unlike a JSP (a linear scan — not valid XML), a mapper file *is* valid XML, so we DOM
//! parse via [`crate::xml`] like [`crate::spring`] / [`crate::tiles`]. Each statement
//! carries the byte-offset **span** of its `id` value (start + end) so the FE has a real
//! navigation/selection target. MyBatis has no wildcards, so nothing here is ever inferred.

use std::path::Path;

use crate::model::{MapperRecord, RelKind, Relation, StatementKind, StatementRecord};
use crate::xml;

/// Result of parsing one mapper XML file: its `<mapper>` record + every statement + the
/// `MethodToStatement` edges (method key `<FQCN>#<id>` → owning mapper namespace). Mirrors
/// [`crate::interceptors::InterceptorParse`].
#[derive(Debug, Default)]
pub struct MyBatisParse {
    pub mappers: Vec<MapperRecord>,
    pub statements: Vec<StatementRecord>,
    /// `RelKind::MethodToStatement` edges — one per statement. Resolved graph-only (both
    /// endpoints drop to `None` at ingest, like the interceptor edges), so these exist for
    /// schema parity; `resolve` rides the parsed [`StatementRecord`]s directly.
    pub relations: Vec<Relation>,
}

/// Parse mapper XML `source`. Doubles as the "is this a mapper file?" sniff: a document
/// whose root is **not** `<mapper>` with a `namespace` attribute yields an empty parse
/// (skip-and-continue), the same gate [`crate::validation::split_validation_filename`]
/// applies by file name. The `source_file` is stamped onto every record.
pub fn parse_mybatis(source: &str, source_file: &str) -> MyBatisParse {
    let mut out = MyBatisParse::default();
    let Some(doc) = xml::parse(source) else {
        return out;
    };
    let root = doc.root_element();
    // Gate: only a `<mapper namespace=…>` root is a MyBatis mapper.
    if !root.has_tag_name("mapper") {
        return out;
    }
    let Some(namespace) = root.attribute("namespace") else {
        return out;
    };

    out.mappers.push(MapperRecord {
        namespace: namespace.to_string(),
        source_file: source_file.to_string(),
        namespace_offset: attr_value_offset(&root, "namespace"),
    });

    for stmt in root.children() {
        let kind = match stmt.tag_name().name() {
            "select" => StatementKind::Select,
            "insert" => StatementKind::Insert,
            "update" => StatementKind::Update,
            "delete" => StatementKind::Delete,
            // Skip <sql>, <resultMap>, <cache>, whitespace text, comments, …
            _ => continue,
        };
        let Some(id) = stmt.attribute("id") else { continue };
        let (start, end) = attr_value_span(&stmt, "id");
        out.statements.push(StatementRecord {
            mapper_namespace: namespace.to_string(),
            id: id.to_string(),
            kind,
            start,
            end,
        });
        // Method → statement edge. `from` is the `<FQCN>#<method>` join key; `to` is the
        // owning mapper namespace. Never inferred (MyBatis has no wildcards).
        out.relations.push(Relation {
            from: format!("{namespace}#{id}"),
            to: namespace.to_string(),
            kind: RelKind::MethodToStatement,
            inferred: false,
        });
    }

    out
}

/// Parse a mapper XML `file`. Returns `None` when the file can't be read, doesn't parse,
/// or isn't a `<mapper namespace=…>` root (skip-and-continue) — like
/// [`crate::validation::parse_file`]. The `source_file` on every record is `file`.
pub fn parse_mybatis_file(file: &Path) -> Option<MyBatisParse> {
    let text = std::fs::read_to_string(file).ok()?;
    let parse = parse_mybatis(&text, &file.display().to_string());
    if parse.mappers.is_empty() {
        return None;
    }
    Some(parse)
}

/// Byte offset of an attribute's *value* (inside the quotes) on `node`, or 0 if absent.
/// roxmltree ranges are into the parsed source text (same helper as
/// [`crate::interceptors`] / [`crate::validation`] — a follow-up should lift this into
/// [`crate::xml`] and de-duplicate all three call sites).
fn attr_value_offset(node: &roxmltree::Node, attr: &str) -> usize {
    node.attributes()
        .find(|a| a.name() == attr)
        .map(|a| a.range_value().start)
        .unwrap_or(0)
}

/// Byte-offset `(start, end)` span of an attribute's *value* (inside the quotes) on
/// `node`, or `(0, 0)` if absent. Unlike [`attr_value_offset`] this keeps both ends so a
/// statement `id` is a selectable navigation target.
fn attr_value_span(node: &roxmltree::Node, attr: &str) -> (usize, usize) {
    node.attributes()
        .find(|a| a.name() == attr)
        .map(|a| {
            let r = a.range_value();
            (r.start, r.end)
        })
        .unwrap_or((0, 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_namespace_and_statements() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
            <!DOCTYPE mapper PUBLIC "-//mybatis.org//DTD Mapper 3.0//EN" "http://mybatis.org/dtd/mybatis-3-mapper.dtd">
            <mapper namespace="com.x.FooMapper">
              <resultMap id="fooResult" type="com.x.Foo"/>
              <select id="findById" resultType="com.x.Foo">select * from foo where id = #{id}</select>
              <insert id="insert">insert into foo (a) values (#{a})</insert>
              <update id="update">update foo set a = #{a} where id = #{id}</update>
              <delete id="deleteById">delete from foo where id = #{id}</delete>
              <sql id="cols">a, b, c</sql>
            </mapper>"#;
        let file = crate::test_support::tmp("foo-mapper.xml", xml);
        let parse = parse_mybatis_file(&file).unwrap();

        // one mapper, four statements (resultMap/sql skipped).
        assert_eq!(parse.mappers.len(), 1);
        let mapper = &parse.mappers[0];
        assert_eq!(mapper.namespace, "com.x.FooMapper");
        assert!(mapper.namespace_offset > 0);

        assert_eq!(parse.statements.len(), 4);
        let by_id = |id: &str| parse.statements.iter().find(|s| s.id == id).unwrap();

        let sel = by_id("findById");
        assert_eq!(sel.kind, StatementKind::Select);
        assert_eq!(sel.mapper_namespace, "com.x.FooMapper");
        assert_eq!(by_id("insert").kind, StatementKind::Insert);
        assert_eq!(by_id("update").kind, StatementKind::Update);
        assert_eq!(by_id("deleteById").kind, StatementKind::Delete);
    }

    #[test]
    fn id_offset_points_at_the_value() {
        let xml = r#"<mapper namespace="com.x.FooMapper"><select id="findById">x</select></mapper>"#;
        let parse = parse_mybatis(xml, "mem.xml");
        let stmt = &parse.statements[0];
        // the span must be non-empty and the sliced text must be exactly the id value.
        assert!(stmt.start > 0);
        assert!(stmt.end > stmt.start);
        assert_eq!(&xml[stmt.start..stmt.end], "findById");
    }

    #[test]
    fn non_mapper_root_is_empty() {
        // a Spring beans doc, a validators doc, and a bare element must all yield nothing.
        assert!(parse_mybatis("<beans><bean id=\"a\" class=\"C\"/></beans>", "b.xml").mappers.is_empty());
        assert!(parse_mybatis("<validators><field name=\"x\"/></validators>", "v.xml").mappers.is_empty());
        // <mapper> without a namespace is not a mapper.
        assert!(parse_mybatis("<mapper><select id=\"a\">x</select></mapper>", "m.xml").mappers.is_empty());
    }

    #[test]
    fn statement_missing_id_is_skipped() {
        let xml = r#"<mapper namespace="com.x.FooMapper">
            <select>select 1</select>
            <select id="ok">select 2</select>
          </mapper>"#;
        let parse = parse_mybatis(xml, "mem.xml");
        // the id-less <select> is skipped, no panic; only the valid one survives.
        assert_eq!(parse.statements.len(), 1);
        assert_eq!(parse.statements[0].id, "ok");
    }

    #[test]
    fn malformed_xml_is_skipped() {
        // an unbalanced document must not panic — just yield an empty parse.
        let parse = parse_mybatis("<mapper namespace=\"com.x.FooMapper\"><select id=\"a\">", "bad.xml");
        assert!(parse.mappers.is_empty());
        assert!(parse.statements.is_empty());
    }

    #[test]
    fn emits_method_to_statement_edges() {
        let xml = r#"<mapper namespace="com.x.FooMapper">
            <select id="findById">select 1</select>
            <insert id="insert">insert</insert>
          </mapper>"#;
        let parse = parse_mybatis(xml, "mem.xml");
        // one edge per statement, keyed `<FQCN>#<method>`, never inferred.
        assert_eq!(parse.relations.len(), 2);
        let edge = parse
            .relations
            .iter()
            .find(|r| r.from == "com.x.FooMapper#findById")
            .expect("findById edge");
        assert_eq!(edge.to, "com.x.FooMapper");
        assert_eq!(edge.kind, RelKind::MethodToStatement);
        assert!(!edge.inferred);
    }

    #[test]
    fn parse_file_returns_none_for_non_mapper() {
        let dir = crate::test_support::tmp_dir("mybatis-nonmapper");
        let file = dir.join("random.xml");
        std::fs::write(&file, "<beans/>").unwrap();
        assert!(parse_mybatis_file(&file).is_none());
    }
}
