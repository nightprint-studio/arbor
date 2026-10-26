//! `mybatis_nav` domain — go-to and find-usages **inside** a MyBatis mapper XML:
//! `bennu_mybatis_nav`, `bennu_mybatis_usages`, and the Java→XML `bennu_mapper_statement`.
//!
//! Where `bennu_mapper_definition` answers the Java→XML jump (a mapper interface method →
//! its `<select|…>` statement), this module answers the jumps that originate **in the
//! mapper XML**:
//!   - a statement `id="…"` → the Java interface **method** (XML→Java);
//!   - the mapper `namespace="…"` → the Java interface **type**;
//!   - an `<include refid="…">` → the `<sql id>` fragment it pulls in;
//!   - a `resultMap="…"` / `extends="…"` → the `<resultMap id>` it uses;
//!   - an association's `select="…"` → its statement;
//!   - `type` / `resultType` / `parameterType` / `javaType` / `ofType` → the Java class;
//!   - a result `property="…"` → the field or accessor on the mapped class.
//!
//! References into **another namespace** (qualified, or declared in a sibling file) resolve through
//! the project's mapper set. Every target is reported as a **line and column**, never only a byte
//! offset: the editor's navigation history records places by line, so a jump it cannot place is a
//! jump Back cannot return from.
//!
//! Find-usages on a `<resultMap>` / `<sql>` lists the references across all the project's mappers; on
//! a statement id it lists the Java usages of the interface method.

use bennu_core::prelude::BennuState;
use bennu_proto::prelude::{ClassEntry, UsageHit, UsagesResult};
use bennu_web::prelude::{
    declaration_offset, fragment_usages, mapper_namespace, resolve_mybatis_ref, FragmentKind,
    MybatisRef,
};
use serde::{Deserialize, Serialize};

use crate::index_service::IndexService;

/// Args for the caret-driven mapper-XML handlers.
#[derive(Deserialize)]
pub struct MybatisNavArgs {
    /// Absolute path (forward slashes) of the mapper `.xml` the caret is in.
    pub file: String,
    /// The current (possibly-unsaved) buffer text — the caret is classified against it (not
    /// the on-disk file), so navigation is correct even with pending edits.
    pub source: String,
    /// UTF-8 byte offset of the caret.
    pub offset: usize,
}

/// A resolved go-to target.
#[derive(Serialize)]
pub struct MybatisNavResult {
    /// Absolute path (forward slashes) of the file to open.
    pub file: String,
    /// Byte offset of the target in `file` (an XML target); `0` for a Java target, which is
    /// located by line.
    pub offset: usize,
    /// 1-based line of the target. Always set.
    pub line: usize,
    /// 1-based column of the target. Always set.
    pub col: usize,
}

/// Resolve the mapper-XML token under the caret to its go-to target. Returns `None` (never an
/// error) when the caret isn't on a navigable reference or the target can't be resolved (no
/// project index yet, an unknown namespace or class).
#[arbor_rpc::handler]
fn bennu_mybatis_nav(
    _ctx: &BennuState,
    args: MybatisNavArgs,
) -> Result<Option<MybatisNavResult>, String> {
    Ok(resolve(&args.file, &args.source, args.offset))
}

/// Find-usages from inside a mapper XML. `None` when the caret isn't on something whose uses can be
/// listed: a `<sql>` / `<resultMap>` (its declaration or a reference to it) or a statement id.
#[arbor_rpc::handler]
fn bennu_mybatis_usages(
    _ctx: &BennuState,
    args: MybatisNavArgs,
) -> Result<Option<UsagesResult>, String> {
    Ok(usages(&args.file, &args.source, args.offset))
}

/// Args for [`bennu_mapper_statement`].
#[derive(Deserialize)]
pub struct MapperStatementArgs {
    /// Absolute path (forward slashes) of the `.java` the caret is in.
    pub file: String,
    /// The current (possibly-unsaved) buffer — the caret is classified against it.
    pub source: String,
    /// UTF-8 byte offset of the caret.
    pub offset: usize,
}

/// Java→XML go-to by caret: the mapper method under the caret (its declaration in the interface,
/// or a call through a mapper) → its `<select|insert|update|delete>` statement. `None` (never an
/// error) when the caret isn't on a method, or the method's owner has no mapper statement of that
/// name — the FE then carries on with its ordinary go-to / find-usages.
#[arbor_rpc::handler]
fn bennu_mapper_statement(
    _ctx: &BennuState,
    args: MapperStatementArgs,
) -> Result<Option<MybatisNavResult>, String> {
    let svc = IndexService::global();
    let Some((owner, name)) = svc.method_at(&args.file, &args.source, args.offset) else {
        return Ok(None);
    };
    let Some(def) = mapper_statement(&args.file, &owner, &name) else { return Ok(None) };
    let text = std::fs::read_to_string(&def.config_file).ok();
    Ok(Some(match text {
        Some(text) => at_offset(&def.config_file, &text, def.offset),
        None => MybatisNavResult { file: slash(&def.config_file), offset: def.offset, line: 1, col: 1 },
    }))
}

/// The mapper statement for method `name` of the type with JVM binary name `owner`.
pub(crate) fn mapper_statement(
    file: &str,
    owner: &str,
    name: &str,
) -> Option<crate::index_service::MapperDefinition> {
    IndexService::global().definition_mapper(file, &fqcn_of_binary(owner), name)
}

/// `com/x/Outer$Inner` → `com.x.Outer.Inner` — the spelling a mapper `namespace` uses.
fn fqcn_of_binary(binary: &str) -> String {
    binary.replace(['/', '$'], ".")
}

fn slash(path: &str) -> String {
    path.replace('\\', "/")
}

// ── go-to ─────────────────────────────────────────────────────────────────────────

fn resolve(file: &str, source: &str, offset: usize) -> Option<MybatisNavResult> {
    match resolve_mybatis_ref(source, offset)? {
        // Intra-file: jump within this mapper to the declaration.
        MybatisRef::Local { offset, .. } => Some(at_offset(file, source, offset)),
        // XML→Java: the mapper namespace is the interface FQCN.
        MybatisRef::Interface { fqcn } => type_target(file, &fqcn),
        MybatisRef::Method { namespace, name } => {
            let entry = class_entry(file, &namespace)?;
            member_target(&entry, |src| find_method_position(src, &name))
        }
        MybatisRef::Type { name } => type_target(file, &name),
        MybatisRef::Property { owner, name } => {
            let entry = class_entry(file, &owner?)?;
            member_target(&entry, |src| find_property_position(src, &name))
        }
        MybatisRef::Fragment { namespace, kind, id } => fragment_target(file, &namespace, kind, &id),
        // The target itself: there is nothing to jump to, only usages to list.
        MybatisRef::Declaration { .. } => None,
    }
}

/// A target at byte `offset` of `text` (the content of `file`).
fn at_offset(file: &str, text: &str, offset: usize) -> MybatisNavResult {
    let (line, col) = position_of(text, offset);
    MybatisNavResult { file: slash(file), offset, line, col }
}

/// 1-based `(line, column)` of byte `offset` in `text`; the column counts characters.
fn position_of(text: &str, offset: usize) -> (usize, usize) {
    let mut at = offset.min(text.len());
    while at > 0 && !text.is_char_boundary(at) {
        at -= 1;
    }
    let head = &text[..at];
    let line = head.bytes().filter(|b| *b == b'\n').count() + 1;
    let line_start = head.rfind('\n').map_or(0, |i| i + 1);
    (line, head[line_start..].chars().count() + 1)
}

/// A declaration in another mapper: the file(s) of `namespace` are searched for the `kind` / `id`.
fn fragment_target(
    from_file: &str,
    namespace: &str,
    kind: FragmentKind,
    id: &str,
) -> Option<MybatisNavResult> {
    let svc = IndexService::global();
    svc.mapper_files(from_file)
        .into_iter()
        .filter(|(ns, _)| ns == namespace)
        .find_map(|(_, path)| {
            let text = std::fs::read_to_string(&path).ok()?;
            let offset = declaration_offset(&text, kind, id)?;
            Some(at_offset(&path, &text, offset))
        })
}

/// The project class a mapper names — by its fully-qualified name, or, for a bare name (a MyBatis
/// type alias is usually the simple class name), by the simple name. A primitive or a built-in
/// alias (`long`, `map`) matches no project class and resolves to nothing.
fn class_entry(from_file: &str, name: &str) -> Option<ClassEntry> {
    let svc = IndexService::global();
    let root = svc.root_for_file(from_file)?;
    let classes = svc.class_index(&root)?;
    let name = name.trim();
    if name.contains('.') {
        classes.into_iter().find(|c| c.fqcn == name)
    } else {
        classes.into_iter().find(|c| c.simple == name)
    }
}

fn type_target(from_file: &str, name: &str) -> Option<MybatisNavResult> {
    let entry = class_entry(from_file, name)?;
    Some(MybatisNavResult { file: slash(&entry.file), offset: 0, line: entry.line, col: 1 })
}

/// A member of the class `entry`, located by `find` in its source; the type declaration when the
/// member is not there (a Lombok property has no accessor to land on, but the class is still the
/// right place to end up).
fn member_target(
    entry: &ClassEntry,
    find: impl FnOnce(&str) -> Option<(usize, usize)>,
) -> Option<MybatisNavResult> {
    let found = std::fs::read_to_string(&entry.file).ok().and_then(|src| find(&src));
    let (line, col) = found.unwrap_or((entry.line, 1));
    Some(MybatisNavResult { file: slash(&entry.file), offset: 0, line, col })
}

/// `(line, column)` of the first declaration of method `name` — a line where `name` appears as a
/// whole word immediately followed by `(`. A mapper interface has only method declarations (no call
/// sites), so this is unambiguous there.
fn find_method_position(source: &str, name: &str) -> Option<(usize, usize)> {
    source.lines().enumerate().find_map(|(i, line)| {
        method_column(line, name).map(|col| (i + 1, col))
    })
}

/// 1-based column of method `name` where `line` declares or calls it.
fn method_column(line: &str, name: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    let mut from = 0;
    while let Some(rel) = line[from..].find(name) {
        let at = from + rel;
        let before_ok = at == 0 || !is_ident_byte(bytes[at - 1]);
        let after_ok = line[at + name.len()..].trim_start().starts_with('(');
        if before_ok && after_ok {
            return Some(line[..at].chars().count() + 1);
        }
        from = at + name.len();
    }
    None
}

/// Where a result `property` lives on its class: the field, else the setter / getter.
fn find_property_position(source: &str, property: &str) -> Option<(usize, usize)> {
    let field = source.lines().enumerate().find_map(|(i, line)| {
        field_column(line, property).map(|col| (i + 1, col))
    });
    if field.is_some() {
        return field;
    }
    let cap = capitalised(property);
    ["set", "get", "is"].iter().find_map(|prefix| {
        find_method_position(source, &format!("{prefix}{cap}"))
    })
}

/// 1-based column of `name` when `line` declares it as a field: the word, then `;`, `=` or `,`, on
/// a line with no `(` (so a method call or declaration is never mistaken for one).
fn field_column(line: &str, name: &str) -> Option<usize> {
    if line.contains('(') || line.trim_start().starts_with("return") {
        return None;
    }
    let bytes = line.as_bytes();
    let mut from = 0;
    while let Some(rel) = line[from..].find(name) {
        let at = from + rel;
        let before_ok = at > 0 && !is_ident_byte(bytes[at - 1]) && bytes[at - 1] != b'.';
        let after = line[at + name.len()..].trim_start();
        if before_ok && (after.starts_with(';') || after.starts_with('=') || after.starts_with(',')) {
            return Some(line[..at].chars().count() + 1);
        }
        from = at + name.len();
    }
    None
}

fn capitalised(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn is_ident_byte(b: u8) -> bool {
    b == b'_' || b == b'$' || b.is_ascii_alphanumeric()
}

// ── find usages ───────────────────────────────────────────────────────────────────

fn usages(file: &str, source: &str, offset: usize) -> Option<UsagesResult> {
    match resolve_mybatis_ref(source, offset)? {
        MybatisRef::Declaration { kind, id } | MybatisRef::Local { kind, id, .. } => {
            let ns = mapper_namespace(source)?;
            Some(fragment_usages_result(file, source, &ns, kind, &id))
        }
        MybatisRef::Fragment { namespace, kind, id } => {
            Some(fragment_usages_result(file, source, &namespace, kind, &id))
        }
        MybatisRef::Method { namespace, name } => method_usages(file, &namespace, &name),
        _ => None,
    }
}

/// Every reference to the `kind` / `id` declaration of `namespace` across the project's mappers. The
/// open buffer stands in for its own file on disk, so a reference typed a moment ago is listed.
fn fragment_usages_result(
    file: &str,
    source: &str,
    namespace: &str,
    kind: FragmentKind,
    id: &str,
) -> UsagesResult {
    let current = slash(file).to_ascii_lowercase();
    let mut files: Vec<(String, String)> = Vec::new();
    for (_, path) in IndexService::global().mapper_files(file) {
        let key = slash(&path).to_ascii_lowercase();
        if key == current {
            continue; // the live buffer goes in below
        }
        if let Ok(text) = std::fs::read_to_string(&path) {
            files.push((slash(&path), text));
        }
    }
    files.insert(0, (slash(file), source.to_string()));

    let mut usages = Vec::new();
    for (path, text) in &files {
        for (start, end) in fragment_usages(text, namespace, kind, id) {
            usages.push(hit_at(path, text, start, end));
        }
    }
    let label = match kind {
        FragmentKind::Sql => "sql",
        FragmentKind::ResultMap => "resultMap",
        FragmentKind::Statement => "statement",
    };
    UsagesResult { target_label: format!("{label} {id}"), usages }
}

/// The Java usages of mapper method `name` of `namespace` — what calls the statement.
fn method_usages(file: &str, namespace: &str, name: &str) -> Option<UsagesResult> {
    let entry = class_entry(file, namespace)?;
    let src = std::fs::read_to_string(&entry.file).ok()?;
    let (line, col) = find_method_position(&src, name)?;
    let offset = line_offset(&src, line) + src.lines().nth(line - 1)?.char_indices().nth(col - 1)?.0;
    let found = IndexService::global().find_usages(&slash(&entry.file), &src, offset)?;
    Some(crate::references::usages_result_of(found))
}

/// Byte offset where 1-based `line` starts.
fn line_offset(text: &str, line: usize) -> usize {
    text.split_inclusive('\n').take(line.saturating_sub(1)).map(str::len).sum()
}

fn hit_at(file: &str, text: &str, start: usize, end: usize) -> UsageHit {
    let (line, col) = position_of(text, start);
    let line_start = text[..start.min(text.len())].rfind('\n').map_or(0, |i| i + 1);
    let line_end = text[line_start..].find('\n').map_or(text.len(), |i| line_start + i);
    UsageHit {
        file: slash(file),
        start,
        end,
        line,
        col,
        preview: text[line_start..line_end].trim().to_string(),
        via: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_method_declaration_position() {
        let src = "package x;\ninterface FooMapper {\n    Foo findById(Long id);\n    int count();\n}\n";
        assert_eq!(find_method_position(src, "findById"), Some((3, 9)));
        assert_eq!(find_method_position(src, "count"), Some((4, 9)));
        assert_eq!(find_method_position(src, "missing"), None);
    }

    #[test]
    fn does_not_match_a_substring_or_a_non_call() {
        // `find` must not match inside `findById`, and a bare mention (no `(`) isn't a decl.
        let src = "    Foo findById(Long id);\n    // see find in docs\n";
        assert_eq!(find_method_position(src, "find"), None);
    }

    #[test]
    fn tolerates_space_before_the_paren() {
        assert_eq!(find_method_position("    void save (Foo f);\n", "save"), Some((1, 10)));
    }

    #[test]
    fn a_property_lands_on_its_field_before_its_accessors() {
        let src = "class Foo {\n    private String name;\n    public String getName() { return name; }\n}\n";
        assert_eq!(find_property_position(src, "name"), Some((2, 20)));
    }

    #[test]
    fn a_property_without_a_field_lands_on_its_setter() {
        let src = "class Foo {\n    public void setTotal(int t) {}\n}\n";
        assert_eq!(find_property_position(src, "total"), Some((2, 17)));
    }

    #[test]
    fn a_return_or_a_member_access_is_not_a_field_declaration() {
        assert_eq!(field_column("        return name;", "name"), None);
        assert_eq!(field_column("        this.name = name;", "name"), Some(21));
    }

    #[test]
    fn positions_count_characters_not_bytes() {
        let text = "è\n  ab";
        assert_eq!(position_of(text, text.find('b').unwrap()), (2, 4));
    }

    #[test]
    fn a_line_offset_is_where_that_line_starts() {
        let text = "ab\ncde\nf";
        assert_eq!(line_offset(text, 1), 0);
        assert_eq!(line_offset(text, 2), 3);
        assert_eq!(line_offset(text, 3), 7);
    }

    #[test]
    fn a_usage_hit_carries_its_line_and_preview() {
        let text = "<a>\n  <select resultMap=\"m\"/>\n</a>";
        let start = text.find("\"m\"").unwrap() + 1;
        let hit = hit_at("/p/x.xml", text, start, start + 1);
        assert_eq!((hit.line, hit.preview.as_str()), (2, "<select resultMap=\"m\"/>"));
    }
}
