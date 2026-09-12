//! The imports a code template's output asked for, put where they belong: into a new file's text, or as edits to
//! the class that members are inserted into — applied with the insertion, as one undo step.
//!
//! Only Java files get them. Where one goes is [`crate::intentions::import_edit_for`]'s, the placement the
//! "Import class" intention and auto-import on completion use, so an import lands where those put it and is
//! skipped where they would skip it: `java.lang`, the file's own package, a wildcard that covers it, one there
//! already.

use std::collections::BTreeMap;

use serde::Serialize;

/// A byte range of a file and its replacement — an insertion, for an import.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TemplateEdit {
    pub start: usize,
    pub end: usize,
    pub replacement: String,
}

/// The edits that add `imports` to `source`.
///
/// All computed against the same text, so imports landing on the same line are joined into one edit: two
/// insertions at one offset would leave their order to whoever applies them.
pub(crate) fn import_edits(source: &str, imports: &[String]) -> Vec<TemplateEdit> {
    let mut at: BTreeMap<usize, String> = BTreeMap::new();
    for fqn in imports {
        let Some((start, _, text)) = crate::intentions::import_edit_for(source, fqn) else { continue };
        match at.get_mut(&start) {
            // After a `package` line each import brings its own blank line; the first one already did.
            Some(joined) => joined.push_str(text.trim_start_matches(['\r', '\n'])),
            None => {
                at.insert(start, text);
            }
        }
    }
    at.into_iter().map(|(start, replacement)| TemplateEdit { start, end: start, replacement }).collect()
}

/// `text` with `imports` added, when the file it is is Java — the output of a template that writes a whole file.
pub(crate) fn with_imports(path: &str, text: String, imports: &[String]) -> String {
    if !is_java(path) {
        return text;
    }
    let mut out = text.clone();
    for edit in import_edits(&text, imports).into_iter().rev() {
        out.replace_range(edit.start..edit.end, &edit.replacement);
    }
    out
}

/// What to say about text that is copied rather than written into a file, where no import can be added.
pub(crate) fn imports_note(imports: &[String]) -> Option<String> {
    let lines: Vec<String> = imports.iter().map(|fqn| format!("`import {fqn};`")).collect();
    (!lines.is_empty()).then(|| format!("It needs {}.", lines.join(", ")))
}

fn is_java(path: &str) -> bool {
    path.rsplit('.').next().is_some_and(|extension| extension.eq_ignore_ascii_case("java"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_join_the_block_in_order_and_one_already_there_is_left_alone() {
        let text = "package p;\n\nimport java.util.List;\n\nclass A {}\n".to_string();
        let imports = ["java.time.LocalDate".to_string(), "java.util.List".to_string(), "lombok.val".to_string()];
        assert_eq!(
            with_imports("A.java", text, &imports),
            "package p;\n\nimport java.time.LocalDate;\nimport java.util.List;\nimport lombok.val;\n\nclass A {}\n"
        );
    }

    #[test]
    fn only_a_java_file_gets_imports_and_copied_text_says_what_it_needs() {
        let yaml = "app:\n  name: x\n".to_string();
        assert_eq!(with_imports("application.yml", yaml.clone(), &["lombok.val".to_string()]), yaml);
        assert_eq!(imports_note(&["lombok.val".to_string()]).as_deref(), Some("It needs `import lombok.val;`."));
        assert_eq!(imports_note(&[]), None);
    }
}
