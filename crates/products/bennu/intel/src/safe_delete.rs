//! **Safe delete** — remove a member, or refuse and say who still needs it.
//!
//! The refactoring that makes deleting anything possible in a codebase nobody remembers writing.
//! Its whole value is the refusal: a delete that silently breaks four call sites in files you never
//! opened is worse than no delete at all, and knowing *for certain* that nothing uses a method is
//! the thing a search cannot tell you.
//!
//! ## It is the reference index, asked one question
//!
//! Nothing here re-resolves anything. [`classify_target`] says what the caret is standing on — the
//! same call rename makes, so the two can never disagree about what you pointed at — and
//! [`ReferenceIndex::usages_of`] answers who uses it. What is left is deciding what to remove and
//! when to say no.
//!
//! That reuse is the point: the hard part of a safe delete is telling a real reference from a name
//! that merely matches, and that part was finished when rename shipped.
//!
//! ## What it refuses, and why each refusal is not laziness
//!
//! * **Anything still used.** With the list — file, line, and the line's text — because "it is
//!   used" is not an answer, and the next question is always *where*.
//! * **A method that overrides something in a jar.** Deleting it does not remove the behaviour; it
//!   hands the call to the inherited implementation, which is a silent behaviour change wearing a
//!   deletion's clothes. Rename refuses across the same boundary for the same reason, through the
//!   same [`library_override`](crate::rename::library_override).
//! * **A method declared at several levels of one project hierarchy.** To every caller that is one
//!   method; removing the rung you are standing on leaves the others. The family has to go together
//!   or not at all, and "or not at all" is the honest answer until it can go together.
//! * **A constructor, and anything named in an annotation.** A constructor's callers are `new`
//!   expressions the index keys differently; an annotated member may be found reflectively by a
//!   framework, where no index can see the use at all.

use std::collections::HashMap;

use bennu_java::prelude::{parse_java, TypeResolver};

use crate::refs::{classify_target, DeclKey, LangLevel, ReferenceIndex, RenameTarget, UsageLocation};
use crate::rename::SubtypeMap;
use bennu_query::prelude::PlanFile;

/// What a safe delete would do, or why it will not.
#[derive(Debug, Clone)]
pub struct SafeDelete {
    /// A short human label — `method Order.total()`.
    pub label: String,
    /// The file the declaration lives in.
    pub file: String,
    /// The byte range to remove: the member, its documentation comment, and its whole line.
    pub start: usize,
    pub end: usize,
    /// Every use that must go first. **Non-empty means the deletion must not be applied** — this is
    /// the list the editor shows instead.
    pub usages: Vec<UsageLocation>,
    /// Why it cannot be done at all, whatever the usage count.
    pub blocked: Option<String>,
    /// The file to delete along with the declaration — a type is its file.
    pub file_delete: Option<String>,
}

impl SafeDelete {
    /// Whether the plan may be applied. The one question the caller has to ask.
    pub fn is_safe(&self) -> bool {
        self.blocked.is_none() && self.usages.is_empty()
    }
}

/// Plan a safe delete at `offset` in `file`.
///
/// `resolver` is the cheap walk resolver; `policy` is the full-classpath one, and the distinction
/// is load-bearing exactly as it is for rename — without the JDK in `policy`, every override of a
/// library method looks deletable.
#[allow(clippy::too_many_arguments)]
pub fn safe_delete_plan(
    index: &ReferenceIndex,
    file: &str,
    source: &str,
    offset: usize,
    resolver: &dyn TypeResolver,
    policy: &dyn TypeResolver,
    project_types: &HashMap<String, String>,
    subtypes: &SubtypeMap,
    java_files: &[PlanFile],
    level: LangLevel,
) -> Option<SafeDelete> {
    let target = classify_target(index, file, source, offset, resolver, project_types, level)?;
    match target {
        // A local is not the index's business: it never leaves its method, so "is it used" is a
        // question the file answers on its own — see `bennu_check`'s unused-local check, which
        // finds them without being asked.
        RenameTarget::Local { .. } => None,
        RenameTarget::Member { key } => {
            member_delete(index, source, file, &key, policy, subtypes, java_files)
        }
        RenameTarget::Type { binary, .. } => {
            type_delete(index, source, file, &binary, java_files)
        }
    }
}

// ── members ──────────────────────────────────────────────────────────────────

fn member_delete(
    index: &ReferenceIndex,
    caret_source: &str,
    caret_file: &str,
    key: &DeclKey,
    policy: &dyn TypeResolver,
    subtypes: &SubtypeMap,
    java_files: &[PlanFile],
) -> Option<SafeDelete> {
    let (owner, name) = match key {
        DeclKey::Method { owner, name } | DeclKey::Field { owner, name } => (owner, name),
        DeclKey::Type { .. } => return None,
    };

    // The declaration lives in the file that declares the OWNER, which is not necessarily the
    // caret's file — you can ask to delete a method from one of its call sites.
    let (decl_file, decl_source) = declaring_source(owner, caret_file, caret_source, java_files)?;
    let (start, end) = member_span(&decl_source, key)?;

    let mut blocked = None;
    if matches!(key, DeclKey::Method { .. }) {
        if name == "<init>" {
            blocked = Some(
                "a constructor's callers are `new` expressions, which this index keys separately — \
                 deleting it here could not tell you who still constructs the type"
                    .to_string(),
            );
        } else if let Some(lib) = crate::rename::library_override(policy, owner, name) {
            blocked = Some(format!(
                "`{name}` overrides `{}`, which is not this project's to change — deleting it does \
                 not remove the behaviour, it hands every call to the inherited implementation",
                lib.replace('/', "."),
            ));
        } else if let Some(other) = project_family_member(policy, owner, name) {
            blocked = Some(format!(
                "`{name}` is also declared on `{}` in this project, and to every caller those are \
                 one method — the family has to go together",
                other.replace('/', "."),
            ));
        } else if let Some(implementor) = implementing_subtype(subtypes, policy, owner, name) {
            blocked = Some(format!(
                "`{}` in this project implements `{name}` — removing the declaration leaves an \
                 `@Override` with nothing to override",
                implementor.replace('/', "."),
            ));
        }
    }
    if blocked.is_none() {
        if let Some(reason) = reflective_risk(&decl_source, start, end) {
            blocked = Some(reason);
        }
    }

    Some(SafeDelete {
        label: key.label(),
        file: decl_file,
        start,
        end,
        usages: usages_excluding_declaration(index, key, start, end),
        blocked,
        file_delete: None,
    })
}

/// Another project type in the same hierarchy that declares the same method — the override family
/// that would be left behind.
fn project_family_member(resolver: &dyn TypeResolver, owner: &str, name: &str) -> Option<String> {
    let start = bennu_java::prelude::TypeRef::simple(owner);
    bennu_java::prelude::walk_up(resolver, &start, |a| {
        (a.depth > 0
            && resolver.is_project_type(&a.ty.binary_name)
            && a.members.methods.iter().any(|m| m.name == name))
        .then(|| a.ty.binary_name.clone())
    })
}

/// A project type BELOW `owner` that declares the same method.
///
/// The other half of the family, and the half that is easy to forget: `project_family_member` looks
/// **up**, at what this declaration might be overriding. An interface's implementors are **down**,
/// and deleting the declaration they satisfy leaves every one of them with an `@Override` that
/// overrides nothing.
///
/// Measured on `commons-lang3` before this existed: 2 of 29 deletions called safe broke the build,
/// and both were a method removed from an interface — `CircuitBreaker.open` and `DatePrinter.format`.
/// Nothing about looking upward could have caught either.
fn implementing_subtype(
    subtypes: &SubtypeMap,
    resolver: &dyn TypeResolver,
    owner: &str,
    name: &str,
) -> Option<String> {
    // Breadth-first over the subtype map, which is a graph and can be deep — a bound, and a seen
    // set, for the same reasons the supertype walk has them.
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut queue: std::collections::VecDeque<String> = subtypes.children(owner).iter().cloned().collect();
    while let Some(child) = queue.pop_front() {
        if seen.len() > 256 || !seen.insert(child.clone()) {
            continue;
        }
        if resolver
            .members_of(&child)
            .is_some_and(|cm| cm.methods.iter().any(|m| m.name == name))
        {
            return Some(child);
        }
        queue.extend(subtypes.children(&child).iter().cloned());
    }
    None
}

// ── types ────────────────────────────────────────────────────────────────────

fn type_delete(
    index: &ReferenceIndex,
    caret_source: &str,
    caret_file: &str,
    binary: &str,
    java_files: &[PlanFile],
) -> Option<SafeDelete> {
    let (decl_file, decl_source) = declaring_source(binary, caret_file, caret_source, java_files)?;
    let key = DeclKey::Type { binary: binary.to_string() };
    let (start, end) = type_span(&decl_source, binary)?;

    // A top-level type IS its file, so the deletion takes the file. A nested one is a member of
    // the file and only its own span goes.
    let whole_file = start == 0 || decl_source[..start].trim_start().starts_with("package");
    Some(SafeDelete {
        label: key.label(),
        file: decl_file.clone(),
        start,
        end,
        usages: usages_excluding_declaration(index, &key, start, end),
        blocked: None,
        file_delete: whole_file.then_some(decl_file),
    })
}

// ── the shared pieces ────────────────────────────────────────────────────────

/// The uses of `key` that are not the declaration itself.
///
/// The index records the declaration's own name among the references — it has to, or renaming from
/// a use site would not rewrite the declaration — so a member used nowhere still reports one usage.
/// Filtering by span rather than by count: a declaration whose name appears twice on its own line
/// (`int total() { return total; }`) would otherwise look used by itself.
fn usages_excluding_declaration(
    index: &ReferenceIndex,
    key: &DeclKey,
    decl_start: usize,
    decl_end: usize,
) -> Vec<UsageLocation> {
    index
        .usages_of(key)
        .iter()
        .filter(|u| !(u.start >= decl_start && u.end <= decl_end))
        .cloned()
        .collect()
}

/// The source of the file that declares `binary`, falling back to the caret's own file.
fn declaring_source(
    binary: &str,
    caret_file: &str,
    caret_source: &str,
    java_files: &[PlanFile],
) -> Option<(String, String)> {
    let simple = binary.rsplit(['/', '$']).next().unwrap_or(binary);
    let owner_file = java_files.iter().find(|f| {
        std::path::Path::new(&f.path)
            .file_stem()
            .is_some_and(|stem| stem == simple)
    });
    match owner_file {
        Some(f) => Some((f.path.clone(), f.source.clone())),
        None => Some((caret_file.to_string(), caret_source.to_string())),
    }
}

/// The full span of the member `key` names, documentation comment and trailing newline included.
fn member_span(source: &str, key: &DeclKey) -> Option<(usize, usize)> {
    let spans = crate::rename::find_member_name_spans(source, key);
    let (name_start, _) = spans.first().copied()?;
    let tree = parse_java(source)?;
    let node = tree.root_node().descendant_for_byte_range(name_start, name_start)?;
    let member = ancestor_of(
        node,
        &[
            "method_declaration",
            "field_declaration",
            "constructor_declaration",
            "constant_declaration",
        ],
    )?;
    Some(with_doc_and_line(source, member.start_byte(), member.end_byte()))
}

fn type_span(source: &str, binary: &str) -> Option<(usize, usize)> {
    let simple = binary.rsplit(['/', '$']).next().unwrap_or(binary);
    let (name_start, _) = bennu_java::prelude::find_type_name_span(source, simple)?;
    let tree = parse_java(source)?;
    let node = tree.root_node().descendant_for_byte_range(name_start, name_start)?;
    let decl = ancestor_of(
        node,
        &[
            "class_declaration",
            "interface_declaration",
            "enum_declaration",
            "record_declaration",
            "annotation_type_declaration",
        ],
    )?;
    Some(with_doc_and_line(source, decl.start_byte(), decl.end_byte()))
}

fn ancestor_of<'t>(
    node: tree_sitter::Node<'t>,
    kinds: &[&str],
) -> Option<tree_sitter::Node<'t>> {
    let mut current = Some(node);
    while let Some(n) = current {
        if kinds.contains(&n.kind()) {
            return Some(n);
        }
        current = n.parent();
    }
    None
}

/// Widen a member's span to take its documentation comment and its whole line with it.
///
/// A deletion that leaves the javadoc behind leaves a comment describing nothing, and one that
/// leaves the indentation behind leaves a blank line with trailing spaces. Both are the sort of
/// residue that makes people stop trusting a refactoring.
fn with_doc_and_line(source: &str, start: usize, end: usize) -> (usize, usize) {
    let mut from = line_start(source, start);
    // Walk back over an immediately preceding block comment and any annotation lines.
    loop {
        let before = source[..from].trim_end();
        if let Some(doc_end) = before.strip_suffix("*/") {
            if let Some(doc_start) = doc_end.rfind("/*") {
                from = line_start(source, doc_start);
                continue;
            }
        }
        break;
    }
    let mut to = end;
    // Take the rest of the line, and the newline that ends it.
    while to < source.len() && !source[to..].starts_with('\n') {
        to += source[to..].chars().next().map_or(1, char::len_utf8);
    }
    if to < source.len() {
        to += 1;
    }
    (from, to)
}

fn line_start(source: &str, offset: usize) -> usize {
    source[..offset].rfind('\n').map(|i| i + 1).unwrap_or(0)
}

/// Why deleting this span is not something an index can vouch for.
///
/// An annotated member may be found by name at run time — JPA reads a field, Jackson reads a
/// getter, Struts reads a setter, a test runner reads a method — and none of those uses is in any
/// source file. This is the one case where "no usages" is a fact about the index rather than about
/// the program, and saying so is more useful than a deletion that compiles and then fails at boot.
fn reflective_risk(source: &str, start: usize, end: usize) -> Option<String> {
    let head = source.get(start..end)?;
    let annotation = head
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with('@'))?;
    let name = annotation
        .trim_start_matches('@')
        .split(['(', ' '])
        .next()
        .unwrap_or("")
        .to_string();
    Some(format!(
        "it carries `@{name}`, so a framework may reach it by name at run time — a use no index can see"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_doc_comment_goes_with_the_member_it_describes() {
        let src = "class A {\n    /** Adds. */\n    int add() { return 1; }\n}\n";
        let start = src.find("int add").unwrap();
        let end = start + src[start..].find('}').unwrap() + 1;
        let (from, to) = with_doc_and_line(src, start, end);
        let removed = &src[from..to];
        assert!(removed.contains("/** Adds. */"), "the javadoc came too: {removed:?}");
        assert!(removed.ends_with('\n'), "and the line ended: {removed:?}");
        // What is left is a class body with nothing in it, not a stranded comment.
        let rest = format!("{}{}", &src[..from], &src[to..]);
        assert_eq!(rest, "class A {\n}\n");
    }

    /// The one case where "no usages" is a fact about the index, not about the program.
    #[test]
    fn an_annotated_member_says_an_index_cannot_vouch_for_it() {
        let src = "    @Column(name = \"total\")\n    private int total;\n";
        let reason = reflective_risk(src, 0, src.len()).expect("flagged");
        assert!(reason.contains("@Column"), "{reason}");
        assert!(reason.contains("no index can see"), "{reason}");
    }

    #[test]
    fn a_plain_member_carries_no_reflective_warning() {
        let src = "    private int total;\n";
        assert!(reflective_risk(src, 0, src.len()).is_none());
    }
}
