//! A mapper statement as plain SQL — what a database can be asked to prepare.
//!
//! A MyBatis statement is a template: `<if>`, `<where>`, `<foreach>` decide at runtime which pieces
//! exist, and `#{}` / `${}` bind or splice values. A database understands none of that, so checking
//! a statement against one means rendering it first. The rendering is deliberately an
//! *approximation* of one possible expansion, and says so (`dynamic`):
//!
//! * every `<if>` body is kept, the first `<when>` of a `<choose>` is kept (the other branches would
//!   repeat the same clause), `<where>` / `<set>` / `<trim>` apply their prefix and suffix rules,
//!   and a `<foreach>` is written once between its `open` and `close`;
//! * `#{…}` becomes `NULL` — a bound parameter is a value, and any type accepts `NULL` when a
//!   statement is only being prepared;
//! * `${…}` is text spliced into the SQL, which cannot be known — the statement is reported as
//!   *unchecked* with that reason, never guessed at;
//! * an `<include>` of a fragment in the same mapper is expanded (with its `<property>` values);
//!   one from another mapper makes the statement unchecked, for the same reason.

use roxmltree::Node;

use crate::xml;

/// One statement, rendered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapperStatement {
    pub id: String,
    /// `select` | `insert` | `update` | `delete`.
    pub kind: String,
    /// 1-based line of the element.
    pub line: usize,
    /// The `databaseId` attribute, empty when the statement is for every database.
    pub database_id: String,
    pub sql: String,
    /// Dynamic tags were involved, so `sql` is one expansion among several.
    pub dynamic: bool,
    /// Why the statement cannot be prepared, when it cannot.
    pub unchecked: Option<String>,
    /// The `${name}` splices it contains, whether or not they were given a value.
    pub splices: Vec<String>,
    /// Byte span of the statement's opening tag (`<select id="…">`) in the source — where a finding
    /// about the statement is drawn. The statement is rendered, so a position inside its SQL has no
    /// place in the XML; the tag is the honest anchor.
    pub start: usize,
    pub end: usize,
}

const KINDS: &[&str] = &["select", "insert", "update", "delete"];

/// Every statement of the mapper in `source`. Empty when it is not a mapper document.
///
/// `values` says what a `${name}` stands for — text the project splices in at runtime that only the
/// project can know (a table name chosen by the caller, a column list). A name with a value is
/// substituted and the statement checked; one without leaves the statement unchecked, as before.
pub fn mapper_statements(source: &str, values: &[(String, String)]) -> Vec<MapperStatement> {
    let Some(doc) = xml::parse(source) else { return Vec::new() };
    let root = doc.root_element();
    if !root.has_tag_name("mapper") {
        return Vec::new();
    }
    root.children()
        .filter(Node::is_element)
        .filter(|n| KINDS.contains(&n.tag_name().name()))
        .filter_map(|n| {
            let id = n.attribute("id")?.to_string();
            let mut ctx = Render { root, dynamic: false, unchecked: None, depth: 0 };
            let raw = ctx.children(n, &[]);
            let splices = splice_names(&raw);
            let (sql, unchecked) = finish(&raw, ctx.unchecked.take(), values);
            let range = n.range();
            let tag_end = source[range.start..range.end].find('>').map_or(range.end, |i| range.start + i + 1);
            Some(MapperStatement {
                splices,
                start: range.start,
                end: tag_end,
                id,
                kind: n.tag_name().name().to_string(),
                line: doc.text_pos_at(n.range().start).row as usize,
                database_id: n.attribute("databaseId").unwrap_or_default().to_string(),
                sql,
                dynamic: ctx.dynamic,
                unchecked,
            })
        })
        .collect()
}

/// The distinct `${name}` splices written in `raw`, in order of appearance.
fn splice_names(raw: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut rest = raw;
    while let Some(at) = rest.find("${") {
        let after = &rest[at + 2..];
        let Some(end) = after.find('}') else { break };
        let name = after[..end].trim().to_string();
        if !name.is_empty() && !out.contains(&name) {
            out.push(name);
        }
        rest = &after[end + 1..];
    }
    out
}

/// Bindings to `NULL`, a `${name}` to the value it was given, and a leftover `${…}` to the reason
/// the statement cannot be checked.
fn finish(raw: &str, unchecked: Option<String>, values: &[(String, String)]) -> (String, Option<String>) {
    if let Some(reason) = unchecked {
        return (raw.trim().to_string(), Some(reason));
    }
    let mut spliced = raw.to_string();
    for (name, value) in values {
        spliced = spliced.replace(&format!("${{{name}}}"), value);
    }
    let raw = spliced.as_str();
    let missing = splice_names(raw);
    if !missing.is_empty() {
        let list = missing.iter().map(|n| format!("${{{n}}}")).collect::<Vec<_>>().join(", ");
        return (raw.trim().to_string(), Some(format!("{list} spliced in at runtime — give it a value to check this statement")));
    }
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(at) = rest.find("#{") {
        out.push_str(&rest[..at]);
        match rest[at..].find('}') {
            Some(end) => {
                out.push_str("NULL");
                rest = &rest[at + end + 1..];
            }
            None => {
                rest = &rest[at..];
                break;
            }
        }
    }
    out.push_str(rest);
    (out.trim().to_string(), None)
}

struct Render<'a, 'i> {
    root: Node<'a, 'i>,
    dynamic: bool,
    unchecked: Option<String>,
    /// Guards a fragment that includes itself.
    depth: usize,
}

type Props = [(String, String)];

impl<'a, 'i> Render<'a, 'i> {
    fn children(&mut self, node: Node<'a, 'i>, props: &Props) -> String {
        let mut out = String::new();
        for child in node.children() {
            if child.is_text() {
                out.push_str(&substitute(child.text().unwrap_or_default(), props));
            } else if child.is_element() {
                out.push_str(&self.element(child, props));
            }
        }
        out
    }

    fn element(&mut self, node: Node<'a, 'i>, props: &Props) -> String {
        match node.tag_name().name() {
            "if" | "when" | "otherwise" => {
                self.dynamic = true;
                self.children(node, props)
            }
            "choose" => {
                self.dynamic = true;
                let pick = node
                    .children()
                    .find(|c| c.has_tag_name("when"))
                    .or_else(|| node.children().find(|c| c.has_tag_name("otherwise")));
                pick.map(|n| self.children(n, props)).unwrap_or_default()
            }
            "where" => {
                self.dynamic = true;
                let inner = self.children(node, props);
                let inner = strip_prefix_ci(inner.trim(), &["AND", "OR"]);
                if inner.is_empty() { String::new() } else { format!(" WHERE {inner} ") }
            }
            "set" => {
                self.dynamic = true;
                let inner = self.children(node, props);
                let inner = inner.trim().trim_end_matches(',').trim();
                if inner.is_empty() { String::new() } else { format!(" SET {inner} ") }
            }
            "trim" => {
                self.dynamic = true;
                let inner = self.children(node, props);
                let mut inner = inner.trim().to_string();
                let overrides = |attr: &str| -> Vec<String> {
                    node.attribute(attr).unwrap_or_default().split('|').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect()
                };
                let prefixes = overrides("prefixOverrides");
                inner = strip_prefix_ci(&inner, &prefixes.iter().map(String::as_str).collect::<Vec<_>>());
                let suffixes = overrides("suffixOverrides");
                for s in &suffixes {
                    if inner.to_uppercase().ends_with(&s.to_uppercase()) {
                        inner.truncate(inner.len() - s.len());
                        inner = inner.trim_end().to_string();
                        break;
                    }
                }
                if inner.is_empty() {
                    return String::new();
                }
                format!(" {} {inner} {} ", node.attribute("prefix").unwrap_or_default(), node.attribute("suffix").unwrap_or_default())
            }
            "foreach" => {
                self.dynamic = true;
                let inner = self.children(node, props);
                format!("{}{inner}{}", node.attribute("open").unwrap_or_default(), node.attribute("close").unwrap_or_default())
            }
            "bind" | "selectKey" => String::new(),
            "include" => self.include(node, props),
            _ => self.children(node, props),
        }
    }

    fn include(&mut self, node: Node<'a, 'i>, props: &Props) -> String {
        self.dynamic = true;
        let refid = substitute(node.attribute("refid").unwrap_or_default(), props);
        let fragment = self
            .root
            .children()
            .find(|c| c.has_tag_name("sql") && c.attribute("id") == Some(refid.as_str()));
        let Some(fragment) = fragment.filter(|_| self.depth < 8) else {
            self.unchecked.get_or_insert_with(|| format!("includes {refid}, which is not a fragment of this mapper"));
            return String::new();
        };
        // The include's own properties win over those of the including statement.
        let mut inner: Vec<(String, String)> = node
            .children()
            .filter(|c| c.has_tag_name("property"))
            .filter_map(|p| Some((p.attribute("name")?.to_string(), substitute(p.attribute("value")?, props))))
            .collect();
        inner.extend(props.iter().cloned());
        self.depth += 1;
        let out = self.children(fragment, &inner);
        self.depth -= 1;
        out
    }
}

/// `${name}` → the include property of that name; anything else is left for {@link finish}.
fn substitute(text: &str, props: &Props) -> String {
    let mut out = text.to_string();
    for (name, value) in props {
        out = out.replace(&format!("${{{name}}}"), value);
    }
    out
}

/// Drop a leading keyword (`AND`, `OR`, …), whole-word and case-insensitively.
fn strip_prefix_ci(text: &str, words: &[&str]) -> String {
    for w in words {
        if text.len() >= w.len() && text[..w.len()].eq_ignore_ascii_case(w) {
            let rest = &text[w.len()..];
            // `ANDROID_ID = 1` must not lose its first three letters.
            if w.chars().last().is_some_and(char::is_alphabetic)
                && rest.chars().next().is_some_and(|c| c.is_alphanumeric() || c == '_')
            {
                continue;
            }
            return rest.trim_start().to_string();
        }
    }
    text.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(body: &str) -> MapperStatement {
        let src = format!(r#"<mapper namespace="a.B">{body}</mapper>"#);
        mapper_statements(&src, &[]).remove(0)
    }

    #[test]
    fn a_splice_with_a_value_is_checked_and_one_without_is_named() {
        let src = r#"<mapper namespace="a.B"><select id="x">select ${cols} from ${table} where id = #{id}</select></mapper>"#;
        let values = vec![("cols".to_string(), "id, name".to_string()), ("table".to_string(), "users".to_string())];
        let s = mapper_statements(src, &values).remove(0);
        assert_eq!(s.sql, "select id, name from users where id = NULL");
        assert!(s.unchecked.is_none());
        assert_eq!(s.splices, vec!["cols", "table"]);

        let partial = mapper_statements(src, &values[..1]).remove(0);
        assert!(partial.unchecked.unwrap().contains("${table}"));
    }

    #[test]
    fn bindings_become_null() {
        let s = one(r#"<select id="x">select * from t where id = #{id, jdbcType=INTEGER}</select>"#);
        assert_eq!(s.sql, "select * from t where id = NULL");
        assert!(!s.dynamic && s.unchecked.is_none());
    }

    #[test]
    fn where_drops_the_leading_and() {
        let s = one(r#"<select id="x">select * from t <where><if test="a">AND a = #{a}</if><if test="b">AND b = #{b}</if></where></select>"#);
        assert_eq!(s.sql.split_whitespace().collect::<Vec<_>>().join(" "), "select * from t WHERE a = NULL AND b = NULL");
        assert!(s.dynamic);
    }

    #[test]
    fn a_word_that_starts_with_and_keeps_its_letters() {
        let s = one(r#"<select id="x">select * from t <where>android_id = #{a}</where></select>"#);
        assert!(s.sql.contains("WHERE android_id"));
    }

    #[test]
    fn set_drops_the_trailing_comma() {
        let s = one(r#"<update id="x">update t <set><if test="a">a = #{a},</if></set> where id = #{id}</update>"#);
        assert!(s.sql.contains("SET a = NULL "));
    }

    #[test]
    fn foreach_is_written_once_between_open_and_close() {
        let s = one(r#"<select id="x">select * from t where id in <foreach collection="ids" item="i" open="(" separator="," close=")">#{i}</foreach></select>"#);
        assert!(s.sql.ends_with("in (NULL)"));
    }

    #[test]
    fn a_dollar_binding_makes_the_statement_unchecked() {
        let s = one(r#"<select id="x">select * from ${table}</select>"#);
        assert!(s.unchecked.is_some());
    }

    #[test]
    fn a_local_include_is_expanded_with_its_properties() {
        let src = r#"<mapper namespace="a.B"><sql id="cols">${p}.id, ${p}.name</sql><select id="x">select <include refid="cols"><property name="p" value="t"/></include> from t</select></mapper>"#;
        let s = mapper_statements(src, &[]).remove(0);
        assert_eq!(s.sql.split_whitespace().collect::<Vec<_>>().join(" "), "select t.id, t.name from t");
        assert!(s.unchecked.is_none());
    }

    #[test]
    fn an_include_from_another_mapper_is_unchecked() {
        let s = one(r#"<select id="x">select <include refid="other.cols"/> from t</select>"#);
        assert!(s.unchecked.is_some());
    }

    #[test]
    fn only_the_first_when_of_a_choose_is_kept() {
        let s = one(r#"<select id="x">select * from t order by <choose><when test="a">a</when><when test="b">b</when><otherwise>c</otherwise></choose></select>"#);
        assert!(s.sql.ends_with("order by a"));
    }
}
