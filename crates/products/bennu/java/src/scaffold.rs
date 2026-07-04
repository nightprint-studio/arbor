//! New-file **scaffolding** — infer a Java package from a target directory and render the initial
//! content for a new file (Java class/interface/enum/record, JSP, XML, plain). Pure: path in →
//! `(file_name, content)` out, so package inference + templates are unit-tested here (no fs, no
//! Tauri). The BE handler just writes what this returns.

use std::path::{Component, Path};

/// The kinds of file the "New…" tree menu can scaffold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewFileKind {
    JavaClass,
    JavaInterface,
    JavaEnum,
    JavaRecord,
    JavaAnnotation,
    Jsp,
    Xml,
    /// A plain file — `name` is used verbatim (extension included), content empty.
    PlainFile,
}

/// The scaffolded file: its final name (with extension) + initial content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScaffoldResult {
    pub file_name: String,
    pub content: String,
}

/// Standard Maven/Gradle source-root suffixes, **longest / most-specific first** so a path under
/// `src/main/java` yields a clean package rather than `main.java.…`.
const SOURCE_ROOTS: &[&[&str]] = &[
    &["src", "main", "java"],
    &["src", "test", "java"],
    &["src", "main", "resources"],
    &["src", "test", "resources"],
    &["src", "java"],
    &["src"],
];

/// Infer the Java package for `dir`: find the deepest known source root in the path and dot-join
/// the segments beneath it. `None` when no source root is present, or `dir` *is* the source root
/// (the default package).
pub fn infer_package(dir: &Path) -> Option<String> {
    let segs: Vec<&str> = dir
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => s.to_str(),
            _ => None,
        })
        .collect();
    for root in SOURCE_ROOTS {
        if let Some(end) = last_subsequence_end(&segs, root) {
            let pkg = &segs[end..];
            if pkg.is_empty() {
                return None;
            }
            return Some(pkg.join("."));
        }
    }
    None
}

/// Index just past the last contiguous occurrence of `needle` in `hay`, or `None`.
fn last_subsequence_end(hay: &[&str], needle: &[&str]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    let mut found = None;
    for start in 0..=hay.len() - needle.len() {
        if hay[start..start + needle.len()].iter().zip(needle).all(|(a, b)| a == b) {
            found = Some(start + needle.len());
        }
    }
    found
}

/// Render the body of a Java type declaration (no package line).
fn java_decl(kind: NewFileKind, name: &str) -> String {
    match kind {
        NewFileKind::JavaClass => format!("public class {name} {{\n}}\n"),
        NewFileKind::JavaInterface => format!("public interface {name} {{\n}}\n"),
        NewFileKind::JavaEnum => format!("public enum {name} {{\n}}\n"),
        NewFileKind::JavaRecord => format!("public record {name}() {{\n}}\n"),
        NewFileKind::JavaAnnotation => format!("public @interface {name} {{\n}}\n"),
        _ => String::new(),
    }
}

/// The full content of a Java file (optional `package` line + a blank line + the declaration).
pub fn java_template(kind: NewFileKind, package: Option<&str>, name: &str) -> String {
    let mut s = String::new();
    if let Some(p) = package.filter(|p| !p.is_empty()) {
        s.push_str(&format!("package {p};\n\n"));
    }
    s.push_str(&java_decl(kind, name));
    s
}

/// Strip a trailing `.<ext>` from `name` if present (case-insensitive), else return it unchanged.
fn strip_ext(name: &str, ext: &str) -> String {
    let suffix = format!(".{ext}");
    if name.to_ascii_lowercase().ends_with(&suffix) {
        name[..name.len() - suffix.len()].to_string()
    } else {
        name.to_string()
    }
}

/// Scaffold a new file of `kind` named `name` (an entered base name, extension optional) in `dir`.
/// Returns the final file name + initial content; for Java kinds the package is inferred from
/// `dir`. Pure — the caller resolves the path (`dir/file_name`) and writes it.
pub fn scaffold_new_file(kind: NewFileKind, dir: &Path, name: &str) -> ScaffoldResult {
    let name = name.trim();
    match kind {
        NewFileKind::JavaClass
        | NewFileKind::JavaInterface
        | NewFileKind::JavaEnum
        | NewFileKind::JavaRecord
        | NewFileKind::JavaAnnotation => {
            let type_name = strip_ext(name, "java");
            let package = infer_package(dir);
            ScaffoldResult {
                file_name: format!("{type_name}.java"),
                content: java_template(kind, package.as_deref(), &type_name),
            }
        }
        NewFileKind::Jsp => {
            let base = strip_ext(name, "jsp");
            ScaffoldResult {
                file_name: format!("{base}.jsp"),
                content: "<%@ page contentType=\"text/html;charset=UTF-8\" language=\"java\" %>\n".to_string(),
            }
        }
        NewFileKind::Xml => {
            let base = strip_ext(name, "xml");
            ScaffoldResult {
                file_name: format!("{base}.xml"),
                content: "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n".to_string(),
            }
        }
        NewFileKind::PlainFile => ScaffoldResult { file_name: name.to_string(), content: String::new() },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn infer_package_under_maven_main_java() {
        assert_eq!(infer_package(&p("/proj/src/main/java/com/acme/web")).as_deref(), Some("com.acme.web"));
    }

    #[test]
    fn infer_package_under_test_java() {
        assert_eq!(infer_package(&p("/proj/src/test/java/com/acme")).as_deref(), Some("com.acme"));
    }

    #[test]
    fn infer_package_prefers_deepest_source_root() {
        // Both `src` and `src/main/java` are present — the specific one wins (no `main.java.` prefix).
        assert_eq!(infer_package(&p("/x/src/main/java/a/b")).as_deref(), Some("a.b"));
    }

    #[test]
    fn infer_package_plain_src_root() {
        assert_eq!(infer_package(&p("/x/src/com/acme")).as_deref(), Some("com.acme"));
    }

    #[test]
    fn infer_package_none_at_source_root_or_no_root() {
        assert_eq!(infer_package(&p("/x/src/main/java")), None); // default package
        assert_eq!(infer_package(&p("/x/random/dir")), None); // no source root
    }

    #[test]
    fn infer_package_resources_root_has_no_package() {
        assert_eq!(infer_package(&p("/x/src/main/resources")), None);
    }

    #[test]
    fn windows_style_path_segments() {
        assert_eq!(
            infer_package(&p("C:/dev/proj/src/main/java/org/foo")).as_deref(),
            Some("org.foo")
        );
    }

    #[test]
    fn java_class_template_with_package() {
        let out = java_template(NewFileKind::JavaClass, Some("com.acme"), "Foo");
        assert_eq!(out, "package com.acme;\n\npublic class Foo {\n}\n");
    }

    #[test]
    fn java_interface_without_package() {
        let out = java_template(NewFileKind::JavaInterface, None, "Bar");
        assert_eq!(out, "public interface Bar {\n}\n");
    }

    #[test]
    fn java_enum_and_record_and_annotation() {
        assert!(java_template(NewFileKind::JavaEnum, None, "E").contains("public enum E {"));
        assert!(java_template(NewFileKind::JavaRecord, None, "R").contains("public record R() {"));
        assert!(java_template(NewFileKind::JavaAnnotation, None, "A").contains("public @interface A {"));
    }

    #[test]
    fn scaffold_java_class_infers_package_and_extension() {
        let r = scaffold_new_file(NewFileKind::JavaClass, &p("/x/src/main/java/com/acme"), "LoginAction");
        assert_eq!(r.file_name, "LoginAction.java");
        assert!(r.content.starts_with("package com.acme;\n\npublic class LoginAction {"));
    }

    #[test]
    fn scaffold_strips_typed_extension() {
        let r = scaffold_new_file(NewFileKind::JavaClass, &p("/x/src/main/java"), "Foo.java");
        assert_eq!(r.file_name, "Foo.java");
        assert!(r.content.contains("public class Foo {"));
    }

    #[test]
    fn scaffold_jsp_and_xml_have_headers() {
        let jsp = scaffold_new_file(NewFileKind::Jsp, &p("/x/web"), "index");
        assert_eq!(jsp.file_name, "index.jsp");
        assert!(jsp.content.contains("<%@ page"));
        let xml = scaffold_new_file(NewFileKind::Xml, &p("/x"), "beans");
        assert_eq!(xml.file_name, "beans.xml");
        assert!(xml.content.starts_with("<?xml"));
    }

    #[test]
    fn scaffold_plain_file_is_verbatim_and_empty() {
        let r = scaffold_new_file(NewFileKind::PlainFile, &p("/x"), "notes.txt");
        assert_eq!(r.file_name, "notes.txt");
        assert_eq!(r.content, "");
    }
}
