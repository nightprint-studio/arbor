//! The imports a template's output needs — added to the file for it, so a template writes `LocalDate` rather
//! than an `{% if %}` about whether the file imports it already.
//!
//! Two things ask for one: `style.local(type)` when it writes Lombok's `val`, which is a type in `lombok` like
//! any other, and the `imported` filter, which takes a fully-qualified name and writes the simple one. Where
//! the imports go is the kind's business — into a new file's text, or beside members inserted into a class —
//! so a render only collects them.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

/// The imports one render asked for. A handle, because the style object and the filter that record them are
/// built before the render and read after it.
#[derive(Debug, Clone, Default)]
pub(crate) struct Imports(Arc<Mutex<BTreeSet<String>>>);

impl Imports {
    pub(crate) fn require(&self, fqn: &str) {
        if let Ok(mut imports) = self.0.lock() {
            imports.insert(fqn.to_string());
        }
    }

    /// `java.time.LocalDate` → `LocalDate`, with the import recorded; `java.util.List<String>` → `List<String>`,
    /// importing `java.util.List`. A name without a package, or one in `java.lang`, is written as it is and
    /// imports nothing.
    pub(crate) fn imported(&self, name: &str) -> String {
        let name = name.trim();
        let (qualified, rest) = name.split_at(name.find(['<', '[']).unwrap_or(name.len()));
        match qualified.rsplit_once('.') {
            Some((package, simple)) if !package.is_empty() && !simple.is_empty() => {
                if package != "java.lang" {
                    self.require(qualified);
                }
                format!("{simple}{rest}")
            }
            _ => name.to_string(),
        }
    }

    /// Every import asked for, sorted, each once.
    pub(crate) fn list(&self) -> Vec<String> {
        self.0.lock().map(|imports| imports.iter().cloned().collect()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use crate::engine::render_code;
    use crate::facts::TemplateFacts;
    use serde_json::json;

    #[test]
    fn a_type_written_through_imported_is_its_simple_name_and_an_import() {
        let template = "{{ \"java.time.LocalDate\" | imported }} a; {{ \"java.util.List<String>\" | imported }} b; \
                        {{ \"java.lang.String\" | imported }} c; {{ \"int\" | imported }} d";
        let out = render_code(template, &json!({}), &TemplateFacts::default(), None).unwrap();
        assert_eq!(out.text, "LocalDate a; List<String> b; String c; int d");
        assert_eq!(out.imports, ["java.time.LocalDate", "java.util.List"]);
    }

    #[test]
    fn a_val_imports_lombok_and_a_type_imports_nothing() {
        let mut facts = TemplateFacts::default();
        let out = render_code("{{ style.local(\"Order\") }}", &json!({}), &facts, None).unwrap();
        assert!(out.imports.is_empty());
        facts.style.lombok_val = true;
        let out = render_code("{{ style.local(\"Order\") }}", &json!({}), &facts, None).unwrap();
        assert_eq!(out.text, "val");
        assert_eq!(out.imports, ["lombok.val"]);
    }
}
