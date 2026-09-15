//! Turning computed edits into the set the seam carries.

use bennu_ext::prelude::ExtEdit;
use bennu_intentions::prelude::Edit;

pub(crate) fn from_edit(edit: Edit) -> ExtEdit {
    ExtEdit::replace(edit.start, edit.end, edit.replacement)
}

/// Sort `edits` by position and fold insertions at the same point into one.
///
/// Two insertions at one offset are the case the seam's "must not overlap" leaves undefined — which
/// goes first is up to the host. They happen here for real: in a file with no imports and no package,
/// the import and the class annotation are both inserted at offset 0. Folding them, in the order they
/// were produced, makes the order ours.
pub(crate) fn merge_inserts(mut edits: Vec<ExtEdit>) -> Vec<ExtEdit> {
    // Stable: equal offsets keep the order they were pushed in.
    edits.sort_by_key(|e| e.start);
    let mut out: Vec<ExtEdit> = Vec::with_capacity(edits.len());
    for edit in edits {
        if let Some(last) = out.last_mut() {
            if last.start == last.end && edit.start == edit.end && last.start == edit.start {
                last.text.push_str(&edit.text);
                continue;
            }
        }
        out.push(edit);
    }
    out
}

/// Apply a set of edits the way the host does: every offset into the original text.
#[cfg(test)]
pub(crate) fn apply(source: &str, edits: &[ExtEdit]) -> String {
    let mut sorted = edits.to_vec();
    sorted.sort_by_key(|e| std::cmp::Reverse(e.start));
    let mut out = source.to_string();
    for edit in sorted {
        out.replace_range(edit.start..edit.end, &edit.text);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insertions_at_one_point_are_folded_in_the_order_produced() {
        let merged = merge_inserts(vec![
            ExtEdit::insert(4, "b"),
            ExtEdit::insert(0, "import x;\n"),
            ExtEdit::insert(0, "@A\n"),
        ]);
        assert_eq!(merged.len(), 2);
        assert_eq!(apply("class", &merged), "import x;\n@A\nclasbs");
    }
}
