//! Attaching an annotation to existing code: the one edit a generator makes outside its own file.
//!
//! Text-based on purpose: it finds the declaration line of a field or a type and inserts one line
//! above it, which is all `@Convert` or `@JsonSerialize` needs, and it works on a buffer that does
//! not parse (half-typed code beside the caret).

use crate::output::WireTarget;

/// A byte offset and the text to insert there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Insert {
    pub offset: usize,
    pub text: String,
}

/// Where to put `annotation` so it sits on its own line above `target`'s declaration; `None` when the
/// declaration is not found or already carries the annotation.
pub fn annotation_insert(source: &str, target: &WireTarget, annotation: &str, class: Option<&str>) -> Option<Insert> {
    let eol = if source.contains("\r\n") { "\r\n" } else { "\n" };
    let lines = line_starts(source);
    let found = lines.iter().position(|&(start, end)| {
        let line = &source[start..end];
        match target {
            WireTarget::Field(name) => declares_field(line, name),
            WireTarget::Class => declares_type(line, class),
        }
    })?;
    let (start, end) = lines[found];
    let line = &source[start..end];
    let indent: String = line.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
    let bare = annotation.split('(').next().unwrap_or(annotation);
    // Already annotated: the declaration line itself, or the annotation lines right above it.
    if line.contains(bare) {
        return None;
    }
    let mut above = found;
    while above > 0 {
        above -= 1;
        let (s, e) = lines[above];
        let previous = source[s..e].trim();
        if !previous.starts_with('@') {
            break;
        }
        if previous.starts_with(bare) {
            return None;
        }
    }
    Some(Insert { offset: start, text: format!("{indent}{annotation}{eol}") })
}

fn line_starts(source: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = 0;
    for (i, c) in source.char_indices() {
        if c == '\n' {
            out.push((start, i + 1));
            start = i + 1;
        }
    }
    if start < source.len() {
        out.push((start, source.len()));
    }
    out
}

/// `private LocalDate birthDate;` / `Long id = 0L,` — the name as a whole word, preceded by a type and followed by `;`, `=` or `,`.
fn declares_field(line: &str, name: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.starts_with('@') || trimmed.starts_with("return ") {
        return false;
    }
    let mut from = 0;
    while let Some(found) = line[from..].find(name) {
        let at = from + found;
        let before = &line[..at];
        let after = line[at + name.len()..].trim_start();
        let whole_word = !before.ends_with(|c: char| c.is_alphanumeric() || c == '_' || c == '$' || c == '.')
            && !line[at + name.len()..].starts_with(|c: char| c.is_alphanumeric() || c == '_' || c == '$');
        let before_trim = before.trim_end();
        let typed = before_trim.ends_with(|c: char| c.is_alphanumeric() || c == '>' || c == ']' || c == '_') && before.ends_with(char::is_whitespace);
        if whole_word && typed && (after.starts_with(';') || after.starts_with('=') || after.starts_with(',')) && !before_trim.contains('(') {
            return true;
        }
        from = at + name.len();
    }
    false
}

fn declares_type(line: &str, class: Option<&str>) -> bool {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.starts_with('@') {
        return false;
    }
    let words: Vec<&str> = trimmed.split(|c: char| !c.is_alphanumeric() && c != '_').filter(|w| !w.is_empty()).collect();
    words.windows(2).any(|w| matches!(w[0], "class" | "record" | "interface" | "enum") && class.map_or(w[1].starts_with(|c: char| c.is_alphabetic()), |name| w[1] == name))
        && !trimmed.contains("new ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = "package p;\n\n@Entity\npublic class Customer {\n    @Id\n    private Long id;\n\n    private LocalDate birthDate;\n\n    void f() {\n        this.birthDate = LocalDate.now();\n        LocalDate d = birthDate;\n    }\n}\n";

    #[test]
    fn a_field_annotation_goes_on_its_own_line_above_the_declaration() {
        let edit = annotation_insert(SOURCE, &WireTarget::Field("birthDate".into()), "@Convert(converter = X.class)", None).unwrap();
        let mut out = SOURCE.to_string();
        out.insert_str(edit.offset, &edit.text);
        assert!(out.contains("    @Convert(converter = X.class)\n    private LocalDate birthDate;"), "{out}");
    }

    #[test]
    fn uses_of_the_field_are_not_declarations() {
        let edit = annotation_insert(SOURCE, &WireTarget::Field("birthDate".into()), "@A", None).unwrap();
        assert_eq!(&SOURCE[edit.offset..edit.offset + 24], "    private LocalDate bi");
    }

    #[test]
    fn an_annotation_already_there_is_not_added_twice() {
        let source = "class A {\n    @Convert(converter = Y.class)\n    private int n;\n}\n";
        assert_eq!(annotation_insert(source, &WireTarget::Field("n".into()), "@Convert(converter = X.class)", None), None);
    }

    #[test]
    fn a_class_annotation_goes_above_the_declaration_and_keeps_crlf() {
        let source = "package p;\r\n\r\n@Entity\r\npublic class A {\r\n}\r\n";
        let edit = annotation_insert(source, &WireTarget::Class, "@JsonSerialize(using = S.class)", Some("A")).unwrap();
        assert!(edit.text.ends_with("\r\n"));
        assert_eq!(&source[edit.offset..edit.offset + 12], "public class");
    }

    #[test]
    fn a_missing_field_gives_nothing() {
        assert_eq!(annotation_insert(SOURCE, &WireTarget::Field("nope".into()), "@A", None), None);
    }
}
