//! The fix for mixed matchers: every plain argument wrapped in `eq(…)`.
//!
//! The only judgement in it is how to spell `eq` so the file still compiles — bare when it is already
//! reachable, qualified the way the call's own matchers are when they are written qualified, and bare
//! with a new static import otherwise. When none of those is certain (the file declares an `eq` of
//! its own, say) nothing is offered.

use bennu_ext::prelude::ExtEdit;
use bennu_intentions::prelude::insert_static_import_edit;

use crate::edits::{from_edit, merge_inserts};
use crate::file::JavaFile;
use crate::matchers::MixedCall;
use crate::owners::{ARGUMENT_MATCHERS, EQ};

pub(crate) fn wrap_in_eq(file: &JavaFile<'_>, call: &MixedCall) -> Option<Vec<ExtEdit>> {
    let source = file.source;
    // `eq(null)` picks an overload by inference and can stop a call from compiling; `isNull()` is
    // what was meant, and that is a choice, not a mechanical rewrite.
    if call.raw.iter().any(|&(start, end)| source[start..end].trim() == "null") {
        return None;
    }
    let spelling = eq_spelling(file, &call.qualifiers)?;
    let mut edits: Vec<ExtEdit> = call
        .raw
        .iter()
        .map(|&(start, end)| {
            ExtEdit::replace(start, end, format!("{}({})", spelling.callee, &source[start..end]))
        })
        .collect();
    if spelling.needs_import {
        // `None` means an on-demand import of ArgumentMatchers is there and `eq` still did not
        // resolve with certainty — another static on-demand import could be declaring one.
        edits.push(from_edit(insert_static_import_edit(source, ARGUMENT_MATCHERS, "eq")?));
    }
    Some(merge_inserts(edits))
}

struct Spelling {
    callee: String,
    needs_import: bool,
}

fn eq_spelling(file: &JavaFile<'_>, qualifiers: &[String]) -> Option<Spelling> {
    if file.resolves("eq", None, EQ) {
        return Some(Spelling { callee: "eq".to_string(), needs_import: false });
    }
    if let Some(qualifier) = qualifiers.iter().find(|q| file.resolves("eq", Some(q.as_str()), EQ)) {
        return Some(Spelling { callee: format!("{qualifier}.eq"), needs_import: false });
    }
    // A method of the file hides any import, and a static import of somebody else's `eq` would sit
    // beside ours as an overload — neither is a rewrite that certainly compiles.
    let taken = file.declares_method("eq") || file.facts.imports.iter().any(|i| i.ends_with(".eq"));
    if taken {
        return None;
    }
    Some(Spelling { callee: "eq".to_string(), needs_import: true })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edits::apply;
    use crate::matchers::mixed_calls;

    fn fixed(src: &str) -> Option<String> {
        let file = JavaFile::parse(src).expect("parses");
        let call = mixed_calls(&file).into_iter().next().expect("a mixed call");
        wrap_in_eq(&file, &call).map(|edits| apply(src, &edits))
    }

    #[test]
    fn plain_values_are_wrapped_and_eq_is_imported_beside_the_other_static_imports() {
        let src = "package p;\n\nimport static org.mockito.ArgumentMatchers.any;\nimport static org.mockito.Mockito.verify;\n\nclass T {\n    void t() {\n        verify(repo).save(any(), 5, \"x\");\n    }\n}\n";
        assert_eq!(
            fixed(src).unwrap(),
            "package p;\n\nimport static org.mockito.ArgumentMatchers.any;\nimport static org.mockito.Mockito.verify;\nimport static org.mockito.ArgumentMatchers.eq;\n\nclass T {\n    void t() {\n        verify(repo).save(any(), eq(5), eq(\"x\"));\n    }\n}\n"
        );
    }

    #[test]
    fn an_eq_already_reachable_needs_no_import() {
        let src = "import static org.mockito.Mockito.*;\n\nclass T {\n    void t() {\n        when(repo.find(anyString(), 10)).thenReturn(null);\n    }\n}\n";
        assert_eq!(
            fixed(src).unwrap(),
            "import static org.mockito.Mockito.*;\n\nclass T {\n    void t() {\n        when(repo.find(anyString(), eq(10))).thenReturn(null);\n    }\n}\n"
        );
    }

    #[test]
    fn qualified_matchers_get_a_qualified_eq() {
        let src = "import org.mockito.ArgumentMatchers;\nimport org.mockito.Mockito;\n\nclass T {\n    void t() {\n        Mockito.verify(repo).save(ArgumentMatchers.any(), id);\n    }\n}\n";
        assert_eq!(
            fixed(src).unwrap(),
            "import org.mockito.ArgumentMatchers;\nimport org.mockito.Mockito;\n\nclass T {\n    void t() {\n        Mockito.verify(repo).save(ArgumentMatchers.any(), ArgumentMatchers.eq(id));\n    }\n}\n"
        );
    }

    #[test]
    fn nothing_is_offered_when_eq_cannot_be_spelled_with_certainty() {
        let declared = "import static org.mockito.ArgumentMatchers.any;\nimport static org.mockito.Mockito.verify;\nclass T { boolean eq(int a) { return true; } void t() { verify(repo).save(any(), 5); } }";
        assert_eq!(fixed(declared), None, "the file's own eq hides the import");
        let null = "import static org.mockito.Mockito.*;\nclass T { void t() { verify(repo).save(any(), null); } }";
        assert_eq!(fixed(null), None, "eq(null) is not a mechanical rewrite");
    }
}
