//! Which spans of a mapper get which colour.
//!
//! The markup scan says where the tags, attributes and text are; this decides what each one *is* in a
//! mapper — a statement to declare, a fragment to reference, a condition to evaluate, SQL to lex.

use bennu_ext::prelude::ExtHighlight;

use crate::sql;
use crate::xml_scan::{scan, Attr, Event, Span};

/// Elements whose body is SQL, directly or through the dynamic tags nested in them.
const SQL_CONTAINERS: &[&str] = &["select", "insert", "update", "delete", "sql", "selectKey"];

/// The dynamic-SQL vocabulary: tags that decide which pieces of the statement exist.
const DYNAMIC_TAGS: &[&str] =
    &["if", "choose", "when", "otherwise", "where", "set", "trim", "foreach", "bind", "include"];

/// Elements that declare a name other elements refer to.
const DECLARING: &[&str] = &["select", "insert", "update", "delete", "sql", "resultMap", "parameterMap"];

/// Attributes that name a Java type.
const TYPE_ATTRS: &[&str] =
    &["parameterType", "resultType", "type", "javaType", "ofType", "typeHandler", "namespace"];

/// Attributes that point at another declaration of the mapper.
const REFERENCE_ATTRS: &[&str] = &["refid", "resultMap", "select"];

/// Every highlight of a mapper, or nothing when `source` is not one.
///
/// "A mapper" is a `<mapper namespace="…">` root — `mybatis-config.xml` also has `<mapper>` elements
/// (a resource pointer each), and colouring those as SQL would be wrong in the one place it is
/// loudest.
pub(crate) fn highlights(source: &str) -> Vec<ExtHighlight> {
    if !source.contains("<mapper") {
        return Vec::new();
    }
    let mut out: Vec<ExtHighlight> = Vec::new();
    let mut stack: Vec<&str> = Vec::new();
    let mut is_mapper = false;

    for event in scan(source) {
        match event {
            Event::Open { name, name_span, attrs, self_closing } => {
                is_mapper |= name == "mapper" && attrs.iter().any(|a| a.name == "namespace");
                let in_sql = stack.iter().any(|n| SQL_CONTAINERS.contains(n));
                if in_sql && DYNAMIC_TAGS.contains(&name) {
                    push(&mut out, name_span, "dynamic");
                }
                for attr in &attrs {
                    if let Some(kind) = attribute_kind(name, attr, in_sql) {
                        push(&mut out, attr.value, kind);
                    }
                }
                if !self_closing {
                    stack.push(name);
                }
            }
            Event::Close { name, name_span } => {
                let in_sql = stack.iter().any(|n| SQL_CONTAINERS.contains(n));
                if in_sql && DYNAMIC_TAGS.contains(&name) {
                    push(&mut out, name_span, "dynamic");
                }
                // Pop to the nearest match: the buffer is mid-edit and routinely unbalanced.
                if let Some(at) = stack.iter().rposition(|n| *n == name) {
                    stack.truncate(at);
                }
            }
            Event::Text((start, end)) => {
                if stack.iter().any(|n| SQL_CONTAINERS.contains(n)) {
                    sql::highlight(source, start, end, &mut out);
                }
            }
        }
    }
    if is_mapper { out } else { Vec::new() }
}

/// What an attribute's value is, when it is worth saying.
fn attribute_kind(element: &str, attr: &Attr<'_>, in_sql: bool) -> Option<&'static str> {
    let name = attr.name;
    match name {
        "id" if DECLARING.contains(&element) => Some("decl"),
        // An OGNL condition, or the expression a `<bind>` computes.
        "test" if in_sql => Some("expr"),
        "value" if element == "bind" => Some("expr"),
        "collection" if element == "foreach" => Some("expr"),
        "extends" if element == "resultMap" => Some("ref"),
        _ if REFERENCE_ATTRS.contains(&name) => Some("ref"),
        _ if TYPE_ATTRS.contains(&name) => Some("type"),
        // The Java side of a result mapping; its `column` is the database's and stays plain.
        "property" => Some("property"),
        _ => None,
    }
}

fn push(out: &mut Vec<ExtHighlight>, (start, end): Span, kind: &str) {
    if end > start {
        out.push(ExtHighlight { start, end, kind: format!("mybatis.{kind}") });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAPPER: &str = r#"<?xml version="1.0"?>
<mapper namespace="com.x.OrderMapper">
  <resultMap id="orderMap" type="com.x.Order">
    <result property="total" column="TOTAL"/>
  </resultMap>
  <select id="findById" resultMap="orderMap" parameterType="long">
    SELECT <include refid="cols"/> FROM orders
    <where>
      <if test="id != null">AND id = #{id}</if>
      AND name LIKE '%${name}%'
    </where>
  </select>
</mapper>"#;

    fn spans(src: &str, kind: &str) -> Vec<String> {
        highlights(src)
            .into_iter()
            .filter(|h| h.kind == format!("mybatis.{kind}"))
            .map(|h| src[h.start..h.end].to_string())
            .collect()
    }

    #[test]
    fn declarations_references_types_and_properties_are_each_their_own_kind() {
        assert_eq!(spans(MAPPER, "decl"), ["orderMap", "findById"]);
        assert_eq!(spans(MAPPER, "ref"), ["orderMap", "cols"]);
        assert_eq!(spans(MAPPER, "type"), ["com.x.OrderMapper", "com.x.Order", "long"]);
        assert_eq!(spans(MAPPER, "property"), ["total"]);
    }

    #[test]
    fn dynamic_tags_are_coloured_on_both_the_opening_and_the_closing_tag() {
        assert_eq!(spans(MAPPER, "dynamic"), ["include", "where", "if", "if", "where"]);
        assert_eq!(spans(MAPPER, "expr"), ["id != null"]);
    }

    #[test]
    fn the_sql_of_a_statement_is_lexed_and_the_markup_around_it_is_not() {
        assert!(spans(MAPPER, "sql.keyword").contains(&"SELECT".to_string()));
        assert!(spans(MAPPER, "sql.keyword").contains(&"LIKE".to_string()));
        assert_eq!(spans(MAPPER, "param"), ["#{id}"]);
        assert_eq!(spans(MAPPER, "param.raw"), ["${name}"]);
    }

    #[test]
    fn text_outside_a_statement_is_not_sql() {
        let src = r#"<mapper namespace="a.B"><resultMap id="m" type="a.C">select from where</resultMap></mapper>"#;
        assert!(spans(src, "sql.keyword").is_empty());
    }

    #[test]
    fn a_cdata_section_is_sql_like_any_other_text() {
        let src = r#"<mapper namespace="a.B"><select id="s"><![CDATA[ select 1 where a < 2 ]]></select></mapper>"#;
        assert_eq!(spans(src, "sql.keyword"), ["select", "where"]);
    }

    #[test]
    fn a_config_file_with_mapper_elements_is_not_a_mapper() {
        let src = r#"<configuration><mappers><mapper resource="a/B.xml"/></mappers></configuration>"#;
        assert!(highlights(src).is_empty());
    }

    #[test]
    fn an_unbalanced_buffer_still_colours_what_it_can() {
        let src = r#"<mapper namespace="a.B"><select id="s">select 1 <if test="x">"#;
        assert!(spans(src, "sql.keyword").contains(&"select".to_string()));
    }
}
