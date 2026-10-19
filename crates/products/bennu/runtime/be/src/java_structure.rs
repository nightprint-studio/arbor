//! `java_structure` domain — what a Java file declares, for the Structure panel and the File
//! Structure popup.
//!
//! Read off the parse tree ([`bennu_java::prelude::extract_symbols`]) rather than scanned line by
//! line in the frontend. A line scan cannot see a signature wrapped over several lines, a return
//! type with nested generics (`Map<String, List<Foo>>`), a brace on its own line or a brace inside
//! a text block — which is how a class came to list a handful of its methods. The parser sees all
//! of them, and the same model already feeds go-to, completion and validation.

use bennu_core::prelude::BennuState;
use bennu_java::prelude::{extract_symbols, Span, TypeDecl, Visibility};
use serde::{Deserialize, Serialize};

/// Args for [`bennu_java_structure`].
#[derive(Deserialize, schemars::JsonSchema)]
pub struct JavaStructureArgs {
    /// The buffer's current text — the editor's, not the disk's.
    pub source: String,
}

/// One declaration, in source order. The same shape the frontend's line scan produced, so either
/// can feed the same tree builder.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StructureEntry {
    /// `class` / `interface` / `enum` / `record` / `annotation`, or `method` / `field`.
    pub kind: String,
    pub name: String,
    /// A method's return type, a field's type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// 1-based line of the declaration.
    pub line: u32,
    pub visibility: &'static str,
    /// A method carrying `@Override`.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub overrides: bool,
    /// How many types enclose this declaration: a top-level type is `0`, its members `1`.
    pub depth: usize,
    pub is_type: bool,
}

/// The declarations of a Java buffer, in source order, each with its nesting depth.
#[arbor_rpc::handler]
fn bennu_java_structure(
    _ctx: &BennuState,
    args: JavaStructureArgs,
) -> Result<Vec<StructureEntry>, String> {
    Ok(structure_of(&args.source))
}

fn structure_of(source: &str) -> Vec<StructureEntry> {
    let symbols = extract_symbols(source);
    let lines = LineIndex::new(source);

    // Anonymous bodies carry a number for a name; nobody typed it and nobody would look for it.
    let types: Vec<&TypeDecl> = symbols.types.iter().filter(|t| !t.is_anonymous).collect();

    // (where it starts, what it is) — sorted afterwards, because `types` lists nested types
    // after their owner's members and the panel wants the file's own order.
    let mut found: Vec<(usize, StructureEntry)> = Vec::new();
    for t in &types {
        let Some(span) = t.span else { continue };
        found.push((
            span.start,
            StructureEntry {
                kind: t.kind.slug().to_string(),
                name: t.name.clone(),
                detail: None,
                line: lines.line_of(span.start),
                visibility: "package",
                overrides: false,
                depth: depth_of(span, &types),
                is_type: true,
            },
        ));
        let member_depth = depth_of(span, &types) + 1;
        // A member synthesized by the language or Lombok has no span — nothing to point at, and
        // not something the file declares.
        for m in t.methods.iter().filter(|m| !m.name.is_empty()) {
            let Some(ms) = m.span else { continue };
            found.push((
                ms.start,
                StructureEntry {
                    kind: "method".to_string(),
                    name: m.name.clone(),
                    detail: Some(m.return_type_text.clone()).filter(|d| !d.is_empty()),
                    line: lines.line_of(ms.start),
                    visibility: visibility_slug(m.visibility),
                    overrides: m.has_annotation("Override"),
                    depth: member_depth,
                    is_type: false,
                },
            ));
        }
        for f in &t.fields {
            let Some(fs) = f.span else { continue };
            found.push((
                fs.start,
                StructureEntry {
                    kind: "field".to_string(),
                    name: f.name.clone(),
                    detail: Some(f.type_text.clone()).filter(|d| !d.is_empty()),
                    line: lines.line_of(fs.start),
                    visibility: visibility_slug(f.visibility),
                    overrides: false,
                    depth: member_depth,
                    is_type: false,
                },
            ));
        }
    }
    found.sort_by_key(|(start, _)| *start);
    found.into_iter().map(|(_, e)| e).collect()
}

/// How many other (named) types' bodies `span` sits inside.
fn depth_of(span: Span, types: &[&TypeDecl]) -> usize {
    types
        .iter()
        .filter_map(|t| t.span)
        .filter(|o| o.start < span.start && span.end <= o.end)
        .count()
}

fn visibility_slug(v: Visibility) -> &'static str {
    match v {
        Visibility::Public => "public",
        Visibility::Protected => "protected",
        Visibility::Private => "private",
        Visibility::Package => "package",
    }
}

/// Byte offset → 1-based line, by binary search over the line starts.
struct LineIndex {
    starts: Vec<usize>,
}

impl LineIndex {
    fn new(source: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(source.match_indices('\n').map(|(i, _)| i + 1));
        Self { starts }
    }

    fn line_of(&self, offset: usize) -> u32 {
        match self.starts.binary_search(&offset) {
            Ok(i) => i as u32 + 1,
            Err(i) => i as u32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(entries: &[StructureEntry]) -> Vec<(&str, usize, u32)> {
        entries.iter().map(|e| (e.name.as_str(), e.depth, e.line)).collect()
    }

    #[test]
    fn a_wrapped_signature_and_nested_generics_are_listed() {
        let src = "class A {\n  Map<String, List<Foo>> index(\n      int a,\n      int b)\n  {\n    return null;\n  }\n  void next() {}\n}\n";
        let out = structure_of(src);
        let methods: Vec<&str> =
            out.iter().filter(|e| e.kind == "method").map(|e| e.name.as_str()).collect();
        assert_eq!(methods, vec!["index", "next"], "{out:?}");
    }

    #[test]
    fn nested_types_sit_one_level_down_and_members_follow_their_owner() {
        let src = "class Outer {\n  int a;\n  class Inner {\n    void f() {}\n  }\n  void g() {}\n}\n";
        let out = structure_of(src);
        assert_eq!(
            names(&out),
            vec![("Outer", 0, 1), ("a", 1, 2), ("Inner", 1, 3), ("f", 2, 4), ("g", 1, 6)]
        );
    }

    #[test]
    fn override_and_visibility_are_read_from_the_declaration() {
        let src = "class A {\n  @Override\n  public String toString() { return \"\"; }\n  private int n;\n}\n";
        let out = structure_of(src);
        let to_string = out.iter().find(|e| e.name == "toString").unwrap();
        assert!(to_string.overrides);
        assert_eq!(to_string.visibility, "public");
        assert_eq!(out.iter().find(|e| e.name == "n").unwrap().visibility, "private");
    }
}
