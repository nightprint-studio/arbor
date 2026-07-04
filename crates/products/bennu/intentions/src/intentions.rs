//! The aggregation seam: [`intentions_at`] runs every transform at the caret and returns the ones
//! that apply as [`Offer`]s. The editor calls this once (one BE round-trip) and renders one Alt+Enter
//! item per offer — adding a new intention is a single registration line here.

use crate::log_param::parameterize_log_call;
use crate::np_equals::np_safe_equals;
use crate::{simplify, Edit};

/// One applicable intention at the caret: a stable `id`, a human `label`, and the edit to apply
/// (replace `source[start..end]` with `replacement`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    pub id: String,
    pub label: String,
    pub start: usize,
    pub end: usize,
    pub replacement: String,
}

fn offer(id: &str, label: &str, e: Edit) -> Offer {
    Offer { id: id.to_string(), label: label.to_string(), start: e.start, end: e.end, replacement: e.replacement }
}

/// Every intention applicable at byte `offset` in `source`, in a stable order.
pub fn intentions_at(source: &str, offset: usize) -> Vec<Offer> {
    let mut out = Vec::new();

    if let Some(rw) = parameterize_log_call(source, offset) {
        out.push(Offer {
            id: "log-parameterize".into(),
            label: "Replace concatenation with parameterized logging".into(),
            start: rw.start,
            end: rw.end,
            replacement: rw.replacement,
        });
    }
    if let Some(rw) = np_safe_equals(source, offset) {
        out.push(Offer {
            id: "np-equals".into(),
            label: "Flip to null-safe equals".into(),
            start: rw.start,
            end: rw.end,
            replacement: rw.replacement,
        });
    }
    if let Some(e) = simplify::simplify_size_check(source, offset) {
        out.push(offer("simplify-isempty", "Replace size check with isEmpty()", e));
    }
    if let Some(e) = simplify::simplify_boolean_compare(source, offset) {
        out.push(offer("simplify-boolean", "Simplify boolean comparison", e));
    }
    if let Some(e) = simplify::simplify_negated_comparison(source, offset) {
        out.push(offer("simplify-negated-cmp", "Simplify negated comparison", e));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregates_the_applicable_intention() {
        let src = "if (list.size() == 0) {";
        let offers = intentions_at(src, src.find("size").unwrap());
        assert_eq!(offers.len(), 1);
        assert_eq!(offers[0].id, "simplify-isempty");
        let o = &offers[0];
        let out = format!("{}{}{}", &src[..o.start], o.replacement, &src[o.end..]);
        assert_eq!(out, "if (list.isEmpty()) {");
    }

    #[test]
    fn nothing_applies_off_a_plain_statement() {
        assert!(intentions_at("int x = 1;", 4).is_empty());
    }

    #[test]
    fn np_equals_still_flows_through() {
        let src = r#"if (s.equals("x")) {"#;
        let offers = intentions_at(src, src.find("equals").unwrap());
        assert!(offers.iter().any(|o| o.id == "np-equals"));
    }
}
