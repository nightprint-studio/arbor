//! The two writes every other feature here is built out of: declare a property, and list a module.
//!
//! Both have the same shape — put an element inside a container that may not exist yet — and both
//! are idempotent in the way that matters: asked for something the pom already says, they return
//! no edit rather than a duplicate. That is what makes them safe to call from a button somebody
//! may press twice.

use bennu_xml::prelude::Doc;

use crate::edit::{append_child, block, indent_unit, insert_child_before, shift, Edit};

/// Where Maven's own documentation puts `<properties>`: after the coordinates and the description,
/// before anything that would use one.
const PROPERTIES_BEFORE: &[&str] =
    &["dependencyManagement", "dependencies", "build", "reporting", "profiles", "repositories"];

/// And `<modules>`, which an aggregator writes before its properties.
const MODULES_BEFORE: &[&str] = &[
    "properties",
    "dependencyManagement",
    "dependencies",
    "build",
    "reporting",
    "profiles",
    "repositories",
];

/// Declare `key` = `value` in this pom's `<properties>`, creating the block if there is none.
///
/// `None` when the pom already declares `key` with exactly `value` — there is nothing to write —
/// and a **replacement** when it declares it as something else, because two `<properties>` entries
/// with the same name is a pom where the second silently wins.
pub fn set_property(source: &str, key: &str, value: &str) -> Option<Edit> {
    let doc = Doc::new(source);
    let root = doc.root()?;
    let unit = indent_unit(&doc);

    let Some(props) = doc.child(root, "properties") else {
        let snippet = block("properties", &[format!("<{key}>{value}</{key}>")], &unit);
        return insert_child_before(&doc, root, PROPERTIES_BEFORE, &shift(&snippet, &unit));
    };

    if let Some(existing) = doc.children(props).into_iter().find(|c| doc.name(*c) == key) {
        return match doc.text_span(existing) {
            Some((start, end)) if &source[start..end] == value => None,
            Some((start, end)) => Some(Edit::replace(start, end, value)),
            // Declared empty (`<key/>` or `<key></key>`): there is no text span to overwrite, so
            // the value goes between the tags.
            None => doc.inner_span(existing).map(|(s, e)| Edit::replace(s, e, value)),
        };
    }
    append_child(&doc, props, &format!("<{key}>{value}</{key}>"))
}

/// Add `name` to this pom's `<modules>`, creating the block if there is none.
///
/// `None` when the module is already listed. Nothing here checks that the directory exists — the
/// caller creates it, and a `<module>` written for a directory that is about to appear is the
/// normal order of events.
pub fn add_module(source: &str, name: &str) -> Option<Edit> {
    let doc = Doc::new(source);
    let root = doc.root()?;
    let unit = indent_unit(&doc);

    let Some(modules) = doc.child(root, "modules") else {
        let snippet = block("modules", &[format!("<module>{name}</module>")], &unit);
        return insert_child_before(&doc, root, MODULES_BEFORE, &shift(&snippet, &unit));
    };

    let listed = doc
        .children(modules)
        .into_iter()
        .any(|c| doc.name(c) == "module" && doc.text(c) == name);
    if listed {
        return None;
    }
    append_child(&doc, modules, &format!("<module>{name}</module>"))
}

/// The pom's `<packaging>`, defaulted the way Maven defaults it.
pub fn packaging_of(source: &str) -> String {
    let doc = Doc::new(source);
    let Some(root) = doc.root() else { return "jar".to_string() };
    let written = doc.child_text(root, "packaging");
    if written.is_empty() {
        "jar".to_string()
    } else {
        written
    }
}

/// Set `<packaging>` to `value`, adding the element when the pom leaves it to the default.
///
/// The one edit an aggregator needs on itself: a pom that grows a `<modules>` has to say `pom`, and
/// a jar module that lists modules is a build that fails at the first `mvn install` with a message
/// about a packaging that cannot have them.
pub fn set_packaging(source: &str, value: &str) -> Option<Edit> {
    let doc = Doc::new(source);
    let root = doc.root()?;
    if let Some(existing) = doc.child(root, "packaging") {
        return match doc.text_span(existing) {
            Some((start, end)) if &source[start..end] == value => None,
            Some((start, end)) => Some(Edit::replace(start, end, value)),
            None => doc.inner_span(existing).map(|(s, e)| Edit::replace(s, e, value)),
        };
    }
    // After the artifactId/version it belongs to, before everything else — which is both Maven's
    // convention and the only position where it reads as part of the coordinates.
    insert_child_before(
        &doc,
        root,
        &["name", "description", "modules", "properties", "dependencyManagement", "dependencies", "build"],
        &format!("<packaging>{value}</packaging>"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::apply;

    const BARE: &str = "<project>\n  <artifactId>app</artifactId>\n</project>";

    #[test]
    fn a_property_creates_the_block_it_needs() {
        let edit = set_property(BARE, "test", "TestSuite").unwrap();
        assert_eq!(
            apply(BARE, &[edit]),
            "<project>\n  <artifactId>app</artifactId>\n  <properties>\n    <test>TestSuite</test>\n  </properties>\n</project>"
        );
    }

    #[test]
    fn a_property_joins_a_block_that_exists() {
        let src = "<project>\n  <properties>\n    <java.version>8</java.version>\n  </properties>\n</project>";
        let edit = set_property(src, "test", "Suite").unwrap();
        assert!(apply(src, &[edit]).contains("<java.version>8</java.version>\n    <test>Suite</test>"));
    }

    #[test]
    fn the_same_value_twice_is_not_an_edit() {
        let src = "<project>\n  <properties>\n    <test>Suite</test>\n  </properties>\n</project>";
        assert_eq!(set_property(src, "test", "Suite"), None);
    }

    #[test]
    fn a_different_value_is_replaced_rather_than_declared_twice() {
        let src = "<project>\n  <properties>\n    <test>Old</test>\n  </properties>\n</project>";
        let out = apply(src, &[set_property(src, "test", "New").unwrap()]);
        assert!(out.contains("<test>New</test>"));
        assert_eq!(out.matches("<test>").count(), 1);
    }

    #[test]
    fn modules_are_created_before_the_properties_that_follow_them() {
        let src = "<project>\n  <artifactId>a</artifactId>\n  <properties>\n    <x>1</x>\n  </properties>\n</project>";
        let out = apply(src, &[add_module(src, "core").unwrap()]);
        assert!(out.find("<modules>").unwrap() < out.find("<properties>").unwrap());
        assert!(out.contains("<module>core</module>"));
    }

    #[test]
    fn a_module_already_listed_is_not_listed_twice() {
        let src = "<project>\n  <modules>\n    <module>core</module>\n  </modules>\n</project>";
        assert_eq!(add_module(src, "core"), None);
    }

    #[test]
    fn packaging_defaults_the_way_maven_defaults_it() {
        assert_eq!(packaging_of(BARE), "jar");
        assert_eq!(packaging_of("<project><packaging>war</packaging></project>"), "war");
    }

    #[test]
    fn packaging_is_added_when_the_pom_left_it_implicit() {
        let out = apply(BARE, &[set_packaging(BARE, "pom").unwrap()]);
        assert!(out.contains("<packaging>pom</packaging>"));
        assert_eq!(set_packaging(&out, "pom"), None);
    }
}
