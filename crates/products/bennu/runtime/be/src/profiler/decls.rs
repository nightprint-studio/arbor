//! The methods a Java buffer declares, in the shape the metrics matcher wants.

use bennu_java::prelude::extract_symbols;
use bennu_profile::prelude::DeclaredMethod;

/// Every method and constructor with a source position, each pinned to its **signature line** —
/// the line carrying `name(`, not an annotation or a modifier line above it.
pub fn declared_methods(source: &str) -> Vec<DeclaredMethod> {
    let symbols = extract_symbols(source);
    let lines: Vec<&str> = source.lines().collect();
    let mut starts = vec![0usize];
    for (i, b) in source.bytes().enumerate() {
        if b == b'\n' {
            starts.push(i + 1);
        }
    }
    let line_of = |offset: usize| starts.partition_point(|s| *s <= offset).max(1);

    let mut out = Vec::new();
    for t in symbols.types.iter().filter(|t| !t.is_anonymous) {
        for m in t.methods.iter().filter(|m| !m.name.is_empty()) {
            let Some(span) = m.span else { continue };
            let first = line_of(span.start);
            let needle = format!("{}(", m.name);
            let signature = (first..first + 8)
                .find(|l| lines.get(l - 1).is_some_and(|text| text.contains(&needle)))
                .unwrap_or(first);
            out.push(DeclaredMethod {
                class: t.fqn.clone(),
                name: m.name.clone(),
                arity: m.params.len() as u32,
                line: signature as u32,
                is_constructor: m.name == t.name && m.return_type_text.is_empty(),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn methods_are_pinned_to_their_signature_line_with_arity() {
        let src = "package a.b;\n\npublic class Foo {\n    @Override\n    @SuppressWarnings(\"x\")\n    public int bar(int a, String b) {\n        return 1;\n    }\n\n    static void baz() {}\n}\n";
        let d = declared_methods(src);
        let bar = d.iter().find(|m| m.name == "bar").expect("bar");
        assert_eq!((bar.class.as_str(), bar.arity, bar.line), ("a.b.Foo", 2, 6));
        let baz = d.iter().find(|m| m.name == "baz").expect("baz");
        assert_eq!((baz.arity, baz.line), (0, 10));
    }
}
