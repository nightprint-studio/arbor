//! MyBatis mapper-XML **validation** — red squiggles for what a mapper claims and the project
//! contradicts.
//!
//! [`bennu_web::prelude::scan_mapper`] finds the sites; this module judges them against the class
//! index and the project's other mappers:
//!
//!   - **`mybatis-property-unknown`** (error) — a result `property="…"` that is no field, getter or
//!     setter of the mapped class. MyBatis refuses the mapper at startup.
//!   - **`mybatis-ref-unresolved`** (error) — an `<include refid>`, `resultMap=`, `extends=` or
//!     association `select=` naming a declaration the mapper's namespace does not have.
//!   - **`mybatis-duplicate-id`** (error) — an id declared twice in one mapper.
//!   - **`mybatis-type-missing`** (warning) — a fully-qualified type in a package the project has,
//!     with no such class in it.
//!   - **`mybatis-statement-missing`** (error, on the **Java** side) — an abstract method of a mapper
//!     interface that no statement of its namespace answers.
//!   - **`mybatis-statement-unmapped`** (hint) — the reverse: a statement no method of the interface
//!     maps. Only a hint, since a statement may be called by name.
//!
//! The same stance as every other check in Bennu: **under-report rather than risk a false
//! positive**. A class that cannot be named for certain, a hierarchy that leaves the project (a
//! library superclass may well hold the property), a namespace no indexed mapper declares — each
//! of these is silence, not a guess.

use std::collections::HashMap;

use bennu_proto::prelude::{ClassEntry, Diagnostic};
use bennu_web::prelude::{declaration_offset, scan_mapper, FragmentKind, MapperScan};

use crate::action_props_nav::class_chain;
use crate::index_service::IndexService;

/// Diagnostics for a mapper XML buffer. Empty for anything that is not a mapper.
pub(crate) fn diagnostics_for(file: &str, source: &str) -> Vec<Diagnostic> {
    let Some(scan) = scan_mapper(source) else { return Vec::new() };
    let svc = IndexService::global();
    let classes = svc.all_project_classes();
    let mut out = Vec::new();
    duplicate_ids(&scan, &mut out);
    if !classes.is_empty() {
        unknown_properties(svc, &classes, &scan, &mut out);
        missing_types(&classes, &scan, &mut out);
        unmapped_statements(&classes, &scan, &mut out);
    }
    unresolved_refs(svc, file, source, &scan, &mut out);
    out
}

fn diagnostic(code: &str, severity: &str, message: String, span: (usize, usize)) -> Diagnostic {
    Diagnostic {
        message,
        severity: severity.to_string(),
        code: code.to_string(),
        start: span.0,
        end: span.1,
    }
}

// ── duplicate ids ───────────────────────────────────────────────────────────────────

fn duplicate_ids(scan: &MapperScan, out: &mut Vec<Diagnostic>) {
    for d in &scan.duplicates {
        out.push(diagnostic(
            "mybatis-duplicate-id",
            "error",
            format!("{} `{}` is already declared in this mapper", d.what, d.id),
            d.span,
        ));
    }
}

// ── result properties ───────────────────────────────────────────────────────────────

/// The project class a mapper names: by its qualified name, or — for a bare name, which is usually
/// a type alias — by its simple name **when that is unambiguous**. Two classes sharing the simple
/// name is not guessable from here, and guessing is what this check must not do.
fn project_class<'a>(classes: &'a [ClassEntry], name: &str) -> Option<&'a ClassEntry> {
    let name = name.trim();
    if name.contains('.') {
        let dotted = name.replace('$', ".");
        return classes.iter().find(|c| c.fqcn == dotted);
    }
    let mut same = classes.iter().filter(|c| c.simple == name);
    let first = same.next()?;
    same.next().is_none().then_some(first)
}

/// The sources of a class and its project superclasses, **only when that is the whole hierarchy**.
/// A superclass outside the project (a library base class) may hold any property, so a chain that
/// ends in one proves nothing about what is missing.
fn closed_chain(svc: &IndexService, classes: &[ClassEntry], class: &ClassEntry) -> Option<Vec<(String, String)>> {
    // A record has no setters to find, and an interface's parents are not in the `extends` this reads.
    if matches!(class.kind.as_str(), "enum" | "record" | "annotation" | "interface") {
        return None;
    }
    let chain = class_chain(svc, &class.fqcn);
    let (last_file, last_src) = chain.last()?;
    let simple = classes.iter().find(|c| same_file(&c.file, last_file)).map(|c| c.simple.as_str())?;
    let syms = bennu_java::prelude::extract_symbols(last_src);
    let decl = syms.types.iter().find(|t| t.name == simple).or_else(|| syms.types.first())?;
    let ends_here = decl.extends.as_deref().map_or(true, |e| matches!(e.trim(), "Object" | "java.lang.Object"));
    ends_here.then_some(chain)
}

fn is_plain_identifier(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}

fn unknown_properties(svc: &IndexService, classes: &[ClassEntry], scan: &MapperScan, out: &mut Vec<Diagnostic>) {
    let mut chains: HashMap<String, Option<(String, Vec<(String, String)>)>> = HashMap::new();
    for site in &scan.properties {
        // `address.city` is a path; its first segment is what the mapped class must have.
        let root = site.property.split(['.', '[']).next().unwrap_or_default();
        if !is_plain_identifier(root) {
            continue;
        }
        let known = chains.entry(site.owner.clone()).or_insert_with(|| {
            let class = project_class(classes, &site.owner)?;
            Some((class.simple.clone(), closed_chain(svc, classes, class)?))
        });
        let Some((simple, chain)) = known else { continue };
        let declared = chain.iter().any(|(_, src)| crate::action_props::find_property_member(src, root).is_some());
        if !declared {
            out.push(diagnostic(
                "mybatis-property-unknown",
                "error",
                format!("`{root}` is not a property of `{simple}`: no field, getter or setter by that name"),
                site.span,
            ));
        }
    }
}

// ── types ───────────────────────────────────────────────────────────────────────────

fn missing_types(classes: &[ClassEntry], scan: &MapperScan, out: &mut Vec<Diagnostic>) {
    for site in &scan.types {
        if type_is_missing(classes, &site.name) {
            out.push(diagnostic(
                "mybatis-type-missing",
                "warning",
                format!("Class `{}` was not found in the project", site.name),
                site.span,
            ));
        }
    }
}

/// A qualified class name the project can contradict: its **package is the project's** (some class
/// lives there) and no class by that name does. A package the project does not have is a library's,
/// and the index says nothing about libraries.
fn type_is_missing(classes: &[ClassEntry], name: &str) -> bool {
    let dotted = name.replace('$', ".");
    let Some((package, simple)) = dotted.rsplit_once('.') else { return false };
    // `com.x.Outer.Inner`, or a trailing segment that is not a class name at all.
    if !simple.chars().next().is_some_and(char::is_uppercase) {
        return false;
    }
    let prefix = format!("{dotted}.");
    if classes.iter().any(|c| c.fqcn == dotted || c.fqcn.starts_with(&prefix)) {
        return false;
    }
    // A nested class written with dots: some enclosing class exists.
    let mut enclosing = package;
    while let Some((outer, _)) = enclosing.rsplit_once('.') {
        if classes.iter().any(|c| c.fqcn == enclosing) {
            return false;
        }
        enclosing = outer;
    }
    classes.iter().any(|c| c.fqcn.rsplit_once('.').is_some_and(|(p, _)| p == package))
}

// ── statements ↔ interface methods ──────────────────────────────────────────────────

/// Annotations that give a mapper method its SQL (or its role) without an XML statement.
const SQL_ANNOTATIONS: &[&str] = &[
    "Select", "Insert", "Update", "Delete", "SelectProvider", "InsertProvider", "UpdateProvider",
    "DeleteProvider", "Flush",
];

/// An interface that stands alone: it extends nothing, so every method it can have is written in
/// it. One that extends a parent may get methods (and their statements) from there, or from a
/// generic base mapper supplied at run time — which is exactly where a verdict would be a guess.
fn stands_alone(decl: &bennu_java::prelude::TypeDecl) -> bool {
    decl.extends.is_none() && decl.implements.is_empty()
}

/// The declaration of the Java type `fqcn` (dotted, nested classes included) in `symbols`.
fn declared_type<'a>(symbols: &'a bennu_java::prelude::FileSymbols, fqcn: &str) -> Option<&'a bennu_java::prelude::TypeDecl> {
    symbols.types.iter().find(|t| t.fqn == fqcn && !t.is_anonymous)
}

/// The reverse of a missing statement, as a **hint**: a statement no method of the interface maps.
///
/// Not an error, because a legacy codebase calls statements by name —
/// `sqlSession.selectList("ns.findAll")` — and a statement that is never reached through the
/// interface is still reached. It is worth a quiet line all the same: it is also what a renamed
/// method leaves behind.
fn unmapped_statements(classes: &[ClassEntry], scan: &MapperScan, out: &mut Vec<Diagnostic>) {
    let wanted = scan.namespace.replace('$', ".");
    let Some(class) = classes.iter().find(|c| c.fqcn == wanted) else { return };
    if class.kind != "interface" {
        return;
    }
    let Some(source) = read_lossy(&class.file) else { return };
    let symbols = bennu_java::prelude::extract_symbols(&source);
    let Some(decl) = declared_type(&symbols, &class.fqcn) else { return };
    if !stands_alone(decl) {
        return;
    }
    for site in &scan.statements {
        if decl.methods.iter().any(|m| m.name == site.id) {
            continue;
        }
        out.push(diagnostic(
            "mybatis-statement-unmapped",
            "weak",
            format!(
                "No method `{}` in `{}` maps this statement — fine if it is called by name",
                site.id, class.simple
            ),
            site.span,
        ));
    }
}

/// Diagnostics for a `.java` buffer: an abstract method of a mapper interface that no statement
/// answers. MyBatis fails the first call with `Invalid bound statement (not found)`.
///
/// A mapper is an interface whose name some indexed mapper XML declares as its `namespace` — without
/// one the interface may be annotation-only or have its XML in a module this project has not
/// opened, and the check stays quiet.
pub(crate) fn java_diagnostics(file: &str, source: &str) -> Vec<Diagnostic> {
    let symbols = bennu_java::prelude::extract_symbols(source);
    let svc = IndexService::global();
    let mut out = Vec::new();
    for decl in symbols
        .types
        .iter()
        .filter(|t| t.kind == bennu_java::prelude::TypeKind::Interface && !t.is_anonymous && stands_alone(t))
    {
        let Some(ids) = svc.mapper_statement_ids(file, &decl.fqn) else { continue };
        for method in &decl.methods {
            let answered = !method.is_abstract
                || method.is_static
                || method.is_default
                || SQL_ANNOTATIONS.iter().any(|a| method.has_annotation(a))
                || ids.iter().any(|id| id == &method.name);
            if answered {
                continue;
            }
            let Some(span) = method.span.as_ref().and_then(|s| name_span(source, (s.start, s.end), &method.name)) else {
                continue;
            };
            out.push(diagnostic(
                "mybatis-statement-missing",
                "error",
                format!(
                    "No <select|insert|update|delete> with id `{}` in the mapper XML of `{}` — calling it throws BindingException",
                    method.name, decl.name
                ),
                span,
            ));
        }
    }
    out
}

/// Where the method **name** sits inside the method's span: the first whole word `name` followed by
/// `(`. The span starts at the annotations, so a word in one of them is skipped by the `(` rule only
/// when it is not itself a call — which is why annotations with an argument list are passed over.
fn name_span(source: &str, span: (usize, usize), name: &str) -> Option<(usize, usize)> {
    let text = source.get(span.0..span.1)?;
    let bytes = text.as_bytes();
    let mut from = 0;
    while let Some(rel) = text[from..].find(name) {
        let at = from + rel;
        let end = at + name.len();
        let before_ok = at == 0 || !(bytes[at - 1].is_ascii_alphanumeric() || matches!(bytes[at - 1], b'_' | b'$'));
        if before_ok && text[end..].trim_start().starts_with('(') && !in_annotation(text, at) {
            return Some((span.0 + at, span.0 + end));
        }
        from = end;
    }
    None
}

/// Whether byte `at` of `text` is inside an annotation's argument list (`@Options(…)`), found by
/// counting the parentheses open before it on a line that begins with `@`.
fn in_annotation(text: &str, at: usize) -> bool {
    let line_start = text[..at].rfind('\n').map_or(0, |i| i + 1);
    let head = &text[line_start..at];
    head.trim_start().starts_with('@') && head.matches('(').count() > head.matches(')').count()
}

// ── references ──────────────────────────────────────────────────────────────────────

fn same_file(a: &str, b: &str) -> bool {
    a.replace('\\', "/").eq_ignore_ascii_case(&b.replace('\\', "/"))
}

/// A mapper in Cp1252 is a legitimate file; failing to decode it must not read as "declares nothing".
fn read_lossy(path: &str) -> Option<String> {
    std::fs::read(path).ok().map(|b| String::from_utf8_lossy(&b).into_owned())
}

fn kind_label(kind: FragmentKind) -> &'static str {
    match kind {
        FragmentKind::Sql => "sql fragment",
        FragmentKind::ResultMap => "resultMap",
        FragmentKind::Statement => "statement",
    }
}

fn unresolved_refs(svc: &IndexService, file: &str, source: &str, scan: &MapperScan, out: &mut Vec<Diagnostic>) {
    // No mapper set yet (the config graph is still building): nothing can be said.
    let mappers = svc.mapper_files(file);
    if mappers.is_empty() {
        return;
    }
    for site in &scan.refs {
        // A value with a dot is `namespace.id`; a bare one belongs to this mapper's namespace.
        let (namespace, id) = site
            .value
            .rsplit_once('.')
            .unwrap_or((scan.namespace.as_str(), site.value.as_str()));
        if namespace == scan.namespace && declaration_offset(source, site.kind, id).is_some() {
            continue;
        }
        let others: Vec<&String> = mappers
            .iter()
            .filter(|(ns, path)| ns == namespace && !same_file(path, file))
            .map(|(_, path)| path)
            .collect();
        // A namespace no indexed mapper declares belongs to another module or a generator: unknown.
        if namespace != scan.namespace && others.is_empty() {
            continue;
        }
        let texts: Vec<Option<String>> = others.iter().map(|p| read_lossy(p)).collect();
        // A sibling that cannot be read might be the one that declares it.
        if texts.iter().any(Option::is_none) {
            continue;
        }
        if texts.iter().flatten().any(|t| declaration_offset(t, site.kind, id).is_some()) {
            continue;
        }
        out.push(diagnostic(
            "mybatis-ref-unresolved",
            "error",
            format!("{} `{}` is not declared in `{}`", kind_label(site.kind), id, namespace),
            site.span,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn class(fqcn: &str, kind: &str) -> ClassEntry {
        ClassEntry {
            fqcn: fqcn.to_string(),
            simple: fqcn.rsplit('.').next().unwrap().to_string(),
            file: format!("/p/{fqcn}.java"),
            line: 1,
            kind: kind.to_string(),
        }
    }

    #[test]
    fn a_qualified_name_resolves_exactly_and_a_bare_one_only_when_unique() {
        let classes = [class("a.Foo", "class"), class("b.Foo", "class"), class("a.Bar", "class")];
        assert_eq!(project_class(&classes, "a.Foo").map(|c| c.fqcn.as_str()), Some("a.Foo"));
        assert!(project_class(&classes, "Foo").is_none(), "two classes share the simple name");
        assert_eq!(project_class(&classes, "Bar").map(|c| c.fqcn.as_str()), Some("a.Bar"));
        assert!(project_class(&classes, "Missing").is_none());
    }

    #[test]
    fn a_class_missing_from_a_project_package_is_flagged() {
        let classes = [class("com.x.Foo", "class")];
        assert!(type_is_missing(&classes, "com.x.Gone"));
        assert!(!type_is_missing(&classes, "com.x.Foo"));
    }

    #[test]
    fn a_library_package_is_never_flagged() {
        let classes = [class("com.x.Foo", "class")];
        assert!(!type_is_missing(&classes, "java.util.HashMap"));
        assert!(!type_is_missing(&classes, "org.springframework.Thing"));
    }

    #[test]
    fn a_nested_class_written_with_a_dollar_or_a_dot_is_not_flagged() {
        let classes = [class("com.x.Outer", "class"), class("com.x.Outer.Inner", "class")];
        assert!(!type_is_missing(&classes, "com.x.Outer$Inner"));
        assert!(!type_is_missing(&classes, "com.x.Outer.Inner"));
        assert!(!type_is_missing(&classes, "com.x.Outer.Other"), "an enclosing class exists: not judged");
    }

    #[test]
    fn a_lowercase_tail_is_not_a_class_name() {
        let classes = [class("com.x.Foo", "class")];
        assert!(!type_is_missing(&classes, "com.x.y"));
    }

    #[test]
    fn the_method_name_is_found_past_the_return_type_and_annotations() {
        let src = "interface M {\n    @Options(useGeneratedKeys = true)\n    List<Foo> findAll (int x);\n}";
        let start = src.find("@Options").unwrap();
        let end = src.find(';').unwrap();
        let (a, b) = name_span(src, (start, end), "findAll").unwrap();
        assert_eq!(&src[a..b], "findAll");
    }

    #[test]
    fn a_name_inside_an_annotation_call_is_not_the_method() {
        let src = "interface M {\n    @Cache(find(1))\n    Foo find(int x);\n}";
        let start = src.find("@Cache").unwrap();
        let (a, _) = name_span(src, (start, src.find(';').unwrap()), "find").unwrap();
        assert!(a > src.find("Foo").unwrap(), "landed on the annotation's `find(`");
    }

    #[test]
    fn a_substring_is_not_the_method_name() {
        let src = "interface M { Foo findById(int x); }";
        assert!(name_span(src, (0, src.len()), "find").is_none());
    }

    #[test]
    fn an_interface_with_parents_does_not_stand_alone() {
        let symbols = bennu_java::prelude::extract_symbols(
            "package a;\ninterface Solo { void x(); }\ninterface Child extends Solo { void y(); }\n",
        );
        let solo = declared_type(&symbols, "a.Solo").unwrap();
        let child = declared_type(&symbols, "a.Child").unwrap();
        assert!(stands_alone(solo));
        assert!(!stands_alone(child));
    }

    #[test]
    fn only_a_plain_identifier_is_a_property_root() {
        assert!(is_plain_identifier("name"));
        assert!(!is_plain_identifier(""));
        assert!(!is_plain_identifier("a-b"));
    }
}
