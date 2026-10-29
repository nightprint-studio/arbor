//! Which Rust type a `.ron` file is an instance of.
//!
//! Not invented here: Studio already binds a file to a type with a first-line comment —
//! `// ron-studio: schema = "src/lib.rs", root = "crate::Config"` — or a `.arbor/studio.toml`
//! sidecar with globs. Reusing it means a file Studio opens with its schema is checked in the
//! editor against the same type, and one directive serves both.

use arbor_studio_ron::prelude::{detect_schema_hint, SchemaHint};

/// The binding for `text`, whether from the inline directive or a sidecar next to `path`.
pub fn schema_hint(text: &str, path: &str) -> Option<SchemaHint> {
    detect_schema_hint(text, Some(path))
}

/// Byte span of the line holding the `ron-studio:` directive, for a problem with the binding
/// itself (a crate that cannot be found, a type that is not in it). `None` for a sidecar binding,
/// which has no line in this file to point at.
pub fn directive_span(text: &str) -> Option<(usize, usize)> {
    // Studio's rule, kept exactly: the FIRST non-blank line decides, and it is either the
    // directive or there is none. Looking further would find a comment Studio ignores.
    let mut at = 0usize;
    for line in text.split_inclusive('\n').take(20) {
        let trimmed = line.trim_end_matches(['\r', '\n']);
        let body = trimmed.trim_start();
        if !body.is_empty() {
            let rest = body.strip_prefix("//!").or_else(|| body.strip_prefix("//"))?;
            if !rest.trim_start().starts_with("ron-studio:") {
                return None;
            }
            let lead = trimmed.len() - body.len();
            return Some((at + lead, at + trimmed.len()));
        }
        at += line.len();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_directive_line_is_found_with_its_offsets() {
        let text = "\n// ron-studio: schema=\"src/lib.rs\", root=\"crate::A\"\n(x: 1)";
        let (s, e) = directive_span(text).unwrap();
        assert_eq!(&text[s..e], "// ron-studio: schema=\"src/lib.rs\", root=\"crate::A\"");
    }

    #[test]
    fn a_file_without_one_has_no_span() {
        assert!(directive_span("(x: 1)\n// ron-studio: late").is_none());
        assert!(directive_span("// just a comment\n(x: 1)").is_none());
    }

    #[test]
    fn an_inline_directive_binds_the_file() {
        let text = "// ron-studio: schema=\"lib.rs\", root=\"crate::Config\"\n(a: 1)";
        let hint = schema_hint(text, "/p/assets/config.ron").unwrap();
        assert_eq!(hint.root_type, "crate::Config");
        assert!(hint.rs_file.ends_with("lib.rs"));
    }
}
