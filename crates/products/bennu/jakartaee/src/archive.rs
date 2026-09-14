//! The bean discovery mode — which classes of an archive are beans at all.
//!
//! The rules changed twice, and the reading follows the conservative side of each change:
//!
//! - **No `beans.xml`**: an implicit archive (CDI 1.1+) — only classes with a bean-defining annotation.
//! - **`bean-discovery-mode="all" | "annotated" | "none"`**: what it says.
//! - **A `beans.xml` with no mode, or an empty one**: `all` up to CDI 3, `annotated` from CDI 4. Read
//!   as `all` unless the file declares version 4 — more beans means fewer "unsatisfied" reports, which
//!   is the safe direction when the version is not written down.
//!
//! Several files that disagree, or any `none`, make the mode [`ArchiveMode::Unknown`]: a class may or
//! may not be a bean depending on which module it lives in, and nothing about injection is claimed.

use bennu_ext::prelude::ScannedFile;

use crate::text::slash;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveMode {
    /// Every class with a suitable constructor is a bean.
    All,
    /// Only classes with a bean-defining annotation.
    Annotated,
    /// Mixed or excluded archives — nothing is claimed.
    Unknown,
}

/// The project's discovery mode, from every `beans.xml` outside the test tree.
pub fn discovery_mode(xml: &[ScannedFile]) -> ArchiveMode {
    let modes: Vec<Option<ArchiveMode>> = xml
        .iter()
        .filter(|f| is_main_beans_xml(&slash(&f.path)))
        .map(|f| mode_of(&f.text))
        .collect();
    if modes.is_empty() {
        return ArchiveMode::Annotated;
    }
    let Some(Some(first)) = modes.first().copied() else { return ArchiveMode::Unknown };
    match modes.iter().all(|m| *m == Some(first)) {
        true => first,
        false => ArchiveMode::Unknown,
    }
}

/// A `beans.xml` that shapes the deployed archive — a test one does not.
fn is_main_beans_xml(path: &str) -> bool {
    path.rsplit('/').next().is_some_and(|n| n.eq_ignore_ascii_case("beans.xml"))
        && !path.contains("/src/test/")
        && !path.contains("/target/")
}

/// One file's mode. `None` is `bean-discovery-mode="none"`.
pub fn mode_of(text: &str) -> Option<ArchiveMode> {
    if text.trim().is_empty() {
        return Some(ArchiveMode::All);
    }
    match attribute(text, "bean-discovery-mode").as_deref() {
        Some("all") => Some(ArchiveMode::All),
        Some("annotated") => Some(ArchiveMode::Annotated),
        Some("none") => None,
        Some(_) => Some(ArchiveMode::Unknown),
        None if declares_cdi4(text) => Some(ArchiveMode::Annotated),
        None => Some(ArchiveMode::All),
    }
}

/// Whether the `<beans>` element declares `version="4…"`.
fn declares_cdi4(text: &str) -> bool {
    let Some(open) = text.find("<beans") else { return false };
    let tag_end = text[open..].find('>').map(|i| open + i).unwrap_or(text.len());
    attribute(&text[open..tag_end], "version").is_some_and(|v| v.starts_with('4'))
}

/// The value of the first `name="…"` (or `'…'`) in `text`.
fn attribute(text: &str, name: &str) -> Option<String> {
    let mut from = 0;
    while let Some(i) = text[from..].find(name) {
        let after = &text[from + i + name.len()..];
        let rest = after.trim_start();
        if let Some(rest) = rest.strip_prefix('=') {
            let rest = rest.trim_start();
            let quote = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
            let body = &rest[1..];
            return body.find(quote).map(|end| body[..end].trim().to_string());
        }
        from += i + name.len();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn file(path: &str, text: &str) -> ScannedFile {
        ScannedFile { path: PathBuf::from(path), text: text.to_string() }
    }

    #[test]
    fn no_beans_xml_is_an_implicit_archive() {
        assert_eq!(discovery_mode(&[file("/p/pom.xml", "<project/>")]), ArchiveMode::Annotated);
    }

    #[test]
    fn the_written_mode_wins() {
        let all = file("/p/src/main/resources/META-INF/beans.xml", r#"<beans bean-discovery-mode="all"/>"#);
        assert_eq!(discovery_mode(&[all]), ArchiveMode::All);
        let ann = file("/p/src/main/webapp/WEB-INF/beans.xml", "<beans bean-discovery-mode='annotated'/>");
        assert_eq!(discovery_mode(&[ann]), ArchiveMode::Annotated);
    }

    #[test]
    fn an_empty_or_modeless_file_is_all_unless_it_says_cdi_4() {
        assert_eq!(mode_of(""), Some(ArchiveMode::All));
        assert_eq!(mode_of(r#"<beans xmlns="http://xmlns.jcp.org/xml/ns/javaee" version="1.1"></beans>"#), Some(ArchiveMode::All));
        assert_eq!(mode_of(r#"<beans version="4.0"></beans>"#), Some(ArchiveMode::Annotated));
    }

    #[test]
    fn disagreement_or_none_is_unknown() {
        let a = file("/p/a/src/main/resources/META-INF/beans.xml", r#"<beans bean-discovery-mode="all"/>"#);
        let b = file("/p/b/src/main/resources/META-INF/beans.xml", r#"<beans bean-discovery-mode="annotated"/>"#);
        assert_eq!(discovery_mode(&[a, b]), ArchiveMode::Unknown);
        let none = file("/p/src/main/resources/META-INF/beans.xml", r#"<beans bean-discovery-mode="none"/>"#);
        assert_eq!(discovery_mode(&[none]), ArchiveMode::Unknown);
    }

    #[test]
    fn a_test_beans_xml_does_not_shape_the_deployment() {
        let test = file("/p/src/test/resources/META-INF/beans.xml", r#"<beans bean-discovery-mode="all"/>"#);
        assert_eq!(discovery_mode(&[test]), ArchiveMode::Annotated);
    }
}
