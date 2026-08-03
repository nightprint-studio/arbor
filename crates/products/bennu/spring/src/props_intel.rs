//! Editor answers for an `application*.yml` / `.properties` buffer.
//!
//! The other direction of the same relationship: from Java you ask "what does this key resolve
//! to", and here you ask "does anything still read this line, and what".
//!
//! It is delivered through the **gutter**, with the count as the glyph — `2` beside a key means
//! two places read it, clicking picks between them. A key nothing reads gets no mark at all,
//! which is the useful signal: on a legacy `application.yml` the unmarked lines are the ones
//! nobody has dared delete.
//!
//! Keys are matched in Spring's own terms: `app.readTimeout` and `app.read-timeout` are one key,
//! normalised on both sides ([`crate::usages::canonical_key`]).

use bennu_ext::prelude::{ExtGutterMark, ExtHover, ExtTarget};

use crate::model::SpringModel;
use crate::props::parse_property_file;
use crate::usages::canonical_key;

/// Whether this file is a property source the extension answers for.
pub fn is_property_source(path: &str) -> bool {
    let name = path.replace('\\', "/").rsplit('/').next().unwrap_or_default().to_string();
    crate::props::is_property_file(&name)
}

/// The keys declared in the buffer, each with the places that read it. Parsed from the LIVE
/// text, so a key you just typed is counted without saving.
fn declared_with_usages<'a>(
    model: &'a SpringModel,
    path: &str,
    source: &str,
) -> Vec<(crate::props::PropertyEntry, Vec<&'a crate::model::PropertyUsage>)> {
    let Some(file) = parse_property_file(path, source) else { return Vec::new() };
    file.entries
        .into_iter()
        .map(|e| {
            let usages = model.usages_of(&canonical_key(&e.key));
            (e, usages)
        })
        .collect()
}

/// A gutter mark on every key something reads, with the count as its glyph.
pub fn gutter(model: &SpringModel, path: &str, source: &str) -> Vec<ExtGutterMark> {
    declared_with_usages(model, path, source)
        .into_iter()
        .filter(|(_, u)| !u.is_empty())
        .map(|(entry, usages)| ExtGutterMark {
            line: entry.line,
            kind: "usage".to_string(),
            tooltip: match usages.len() {
                1 => format!("1 usage — {} ({})", usages[0].label, usages[0].kind),
                n => format!("{n} usages of `{}`", entry.key),
            },
            targets: usages.iter().map(|u| target_of(u)).collect(),
        })
        .collect()
}

fn target_of(u: &crate::model::PropertyUsage) -> ExtTarget {
    ExtTarget {
        file: u.file.clone(),
        offset: u.offset,
        label: u.label.clone(),
        detail: u.kind.clone(),
    }
}

/// Go-to from a key in a property file → the places that read it.
pub fn navigate(model: &SpringModel, path: &str, source: &str, offset: usize) -> Vec<ExtTarget> {
    declared_with_usages(model, path, source)
        .into_iter()
        .find(|(e, _)| offset >= e.key_start && offset <= e.key_end)
        .map(|(_, usages)| usages.iter().map(|u| target_of(u)).collect())
        .unwrap_or_default()
}

/// Hover on a key: its value, and who reads it.
pub fn hover(model: &SpringModel, path: &str, source: &str, offset: usize) -> Option<ExtHover> {
    let (entry, usages) = declared_with_usages(model, path, source)
        .into_iter()
        .find(|(e, _)| offset >= e.key_start && offset <= e.key_end)?;
    let doc = match usages.len() {
        // Said plainly, because it is the interesting answer: nothing in this project reads
        // this line. It may still be read from outside — a starter, an env override — so it is
        // stated as a fact about the project rather than as a verdict on the key.
        0 => "Nothing in this project reads this key.".to_string(),
        1 => format!("Read by {} ({})", usages[0].label, usages[0].kind),
        n => {
            let names: Vec<&str> = usages.iter().take(4).map(|u| u.label.as_str()).collect();
            let more = if n > 4 { format!(", +{} more", n - 4) } else { String::new() };
            format!("Read by {n}: {}{more}", names.join(", "))
        }
    };
    Some(ExtHover {
        title: entry.key.clone(),
        signature: if entry.value.is_empty() { "(empty)".to_string() } else { entry.value },
        doc,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::beans::JavaUnit;
    use crate::model::SpringModel;
    use crate::scan::scan_java;

    const YAML_PATH: &str = "/p/src/main/resources/application.yml";

    fn model(java: &str) -> SpringModel {
        let text = format!("import org.springframework.beans.factory.annotation.*;\n{java}");
        let u = JavaUnit { facts: scan_java("/p/S.java", &text).unwrap(), text };
        let units = std::slice::from_ref(&u);
        SpringModel {
            property_usages: crate::usages::property_usages(units, &[], &[]),
            ..SpringModel::default()
        }
    }

    #[test]
    fn a_read_key_gets_a_gutter_mark_whose_glyph_is_the_count() {
        let m = model("class S { @Value(\"${app.timeout}\") int a; @Value(\"${app.timeout}\") int b; }");
        let yaml = "app:\n  timeout: 30\n  unused: x\n";
        let marks = gutter(&m, YAML_PATH, yaml);
        assert_eq!(marks.len(), 1, "only the key something reads is marked");
        assert_eq!(marks[0].line, 2);
        assert_eq!(marks[0].targets.len(), 2);
        assert!(marks[0].tooltip.starts_with("2 usages"));
    }

    #[test]
    fn the_relaxed_spelling_still_matches() {
        let m = model("class S { @Value(\"${app.readTimeout}\") int a; }");
        let yaml = "app:\n  read-timeout: 30\n";
        assert_eq!(gutter(&m, YAML_PATH, yaml).len(), 1, "one key, two spellings");
    }

    #[test]
    fn go_to_from_a_key_lands_on_the_reader() {
        let m = model("class S { @Value(\"${app.timeout}\") int a; }");
        let yaml = "app:\n  timeout: 30\n";
        let at = yaml.find("timeout").unwrap() + 1;
        let t = navigate(&m, YAML_PATH, yaml, at);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].label, "S.a");
        assert_eq!(t[0].file, "/p/S.java");
    }

    #[test]
    fn hover_says_who_reads_it_and_admits_when_nobody_does() {
        let m = model("class S { @Value(\"${app.timeout}\") int a; }");
        let yaml = "app:\n  timeout: 30\n  unused: x\n";
        let h = hover(&m, YAML_PATH, yaml, yaml.find("timeout").unwrap() + 1).unwrap();
        assert_eq!(h.title, "app.timeout");
        assert_eq!(h.signature, "30");
        assert!(h.doc.contains("S.a"));

        let dead = hover(&m, YAML_PATH, yaml, yaml.find("unused").unwrap() + 1).unwrap();
        assert!(dead.doc.starts_with("Nothing in this project reads"));
    }

    #[test]
    fn a_caret_off_any_key_answers_nothing() {
        let m = model("class S {}");
        let yaml = "app:\n  timeout: 30\n";
        assert!(hover(&m, YAML_PATH, yaml, yaml.find("30").unwrap()).is_none());
        assert!(navigate(&m, YAML_PATH, yaml, yaml.find("30").unwrap()).is_empty());
    }

    #[test]
    fn only_spring_property_files_are_answered_for() {
        assert!(is_property_source("/p/application.yml"));
        assert!(is_property_source(r"C:\p\application-dev.properties"));
        assert!(!is_property_source("/p/messages.properties"));
    }
}
