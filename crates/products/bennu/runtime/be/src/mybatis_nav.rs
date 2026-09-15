//! `mybatis_nav` domain — go-to **inside** a MyBatis mapper XML: `bennu_mybatis_nav`.
//!
//! Where `bennu_mapper_definition` answers the Java→XML jump (a mapper interface method →
//! its `<select|…>` statement), this handler answers the jumps that originate **in the
//! mapper XML** — the ones the FE never had behind Ctrl+B before:
//!   - a statement `id="…"` → the Java interface **method** (XML→Java);
//!   - the mapper `namespace="…"` → the Java interface **type**;
//!   - an `<include refid="…">` → the `<sql id>` fragment it pulls in;
//!   - a statement `resultMap="…"` → the `<resultMap id>` it uses.
//!
//! The intra-file jumps resolve to a byte offset in the same file (no index) via
//! `bennu-web`'s tested [`resolve_mybatis_ref`]; the XML→Java jumps resolve the mapper
//! namespace (an interface FQCN) to the declaring `.java` through the project class index,
//! then locate the method line. A qualified cross-namespace `refid`/`resultMap` isn't
//! resolved yet (needs the `<sql>`/`<resultMap>` declarations in the config graph) — it
//! returns `None`, so the FE just does nothing (no regression).

use bennu_core::prelude::BennuState;
use bennu_web::prelude::{resolve_mybatis_ref, MybatisRef};
use serde::{Deserialize, Serialize};

use crate::index_service::IndexService;

/// Args for [`bennu_mybatis_nav`].
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

/// A resolved go-to target from inside a mapper XML.
#[derive(Serialize)]
pub struct MybatisNavResult {
    /// Absolute path (forward slashes) of the file to open.
    pub file: String,
    /// Byte offset to jump to (an intra-file jump into the same mapper); `0` when the target
    /// is expressed as a `line` instead (a cross-file jump into a `.java`).
    pub offset: usize,
    /// 1-based line to jump to (a cross-file jump into a `.java`); `0` when `offset` is used.
    pub line: usize,
}

/// Resolve the mapper-XML token under the caret to its go-to target. Returns `None` (never an
/// error) when the caret isn't on a navigable reference or the target can't be resolved (no
/// project index yet, an unknown namespace, an as-yet-unsupported cross-namespace fragment).
#[arbor_rpc::handler]
fn bennu_mybatis_nav(
    _ctx: &BennuState,
    args: MybatisNavArgs,
) -> Result<Option<MybatisNavResult>, String> {
    Ok(resolve(&args.file, &args.source, args.offset))
}

fn resolve(file: &str, source: &str, offset: usize) -> Option<MybatisNavResult> {
    match resolve_mybatis_ref(source, offset)? {
        // Intra-file: jump within this mapper to the `<sql>` / `<resultMap>` declaration.
        MybatisRef::Local { offset } => Some(MybatisNavResult {
            file: file.to_string(),
            offset,
            line: 0,
        }),
        // XML→Java: the mapper namespace is the interface FQCN.
        MybatisRef::Interface { fqcn } => java_target(file, &fqcn, None),
        MybatisRef::Method { namespace, name } => java_target(file, &namespace, Some(&name)),
        // A qualified cross-namespace fragment — not resolved yet (needs sql/resultMap decls
        // in the config graph). Graceful no-op.
        MybatisRef::Fragment { .. } => None,
    }
}

/// Resolve an interface FQCN to its declaring `.java` via the project class index, landing on
/// `method`'s line when given (else the type declaration line).
fn java_target(from_file: &str, fqcn: &str, method: Option<&str>) -> Option<MybatisNavResult> {
    let svc = IndexService::global();
    let root = svc.root_for_file(from_file)?;
    let classes = svc.class_index(&root)?;
    let entry = classes.iter().find(|c| c.fqcn == fqcn)?;

    let mut line = entry.line;
    if let Some(name) = method {
        if let Ok(src) = std::fs::read_to_string(&entry.file) {
            if let Some(l) = find_method_line(&src, name) {
                line = l;
            }
        }
    }
    Some(MybatisNavResult {
        file: entry.file.clone(),
        offset: 0,
        line,
    })
}

/// 1-based line of the first declaration of method `name` in `source` — a line where `name`
/// appears as a whole word immediately followed by `(`. A mapper interface has only method
/// declarations (no call sites), so this is unambiguous there. `None` when not found.
fn find_method_line(source: &str, name: &str) -> Option<usize> {
    source
        .lines()
        .position(|line| line_declares_method(line, name))
        .map(|i| i + 1)
}

fn line_declares_method(line: &str, name: &str) -> bool {
    let bytes = line.as_bytes();
    let mut from = 0;
    while let Some(rel) = line[from..].find(name) {
        let at = from + rel;
        let before_ok = at == 0 || !is_ident_byte(bytes[at - 1]);
        let after = &line[at + name.len()..];
        let after_ok = after.trim_start().starts_with('(');
        if before_ok && after_ok {
            return true;
        }
        from = at + name.len();
    }
    false
}

fn is_ident_byte(b: u8) -> bool {
    b == b'_' || b == b'$' || b.is_ascii_alphanumeric()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_method_declaration_line() {
        let src = "package x;\ninterface FooMapper {\n    Foo findById(Long id);\n    int count();\n}\n";
        assert_eq!(find_method_line(src, "findById"), Some(3));
        assert_eq!(find_method_line(src, "count"), Some(4));
        assert_eq!(find_method_line(src, "missing"), None);
    }

    #[test]
    fn does_not_match_a_substring_or_a_non_call() {
        // `find` must not match inside `findById`, and a bare mention (no `(`) isn't a decl.
        let src = "    Foo findById(Long id);\n    // see find in docs\n";
        assert_eq!(find_method_line(src, "find"), None);
    }

    #[test]
    fn tolerates_space_before_the_paren() {
        let src = "    void save (Foo f);\n";
        assert_eq!(find_method_line(src, "save"), Some(1));
    }
}
