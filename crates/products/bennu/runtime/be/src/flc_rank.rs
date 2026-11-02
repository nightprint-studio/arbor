//! The order of the popup at a call argument, learned.
//!
//! The provider's hand-written signals put a good candidate near the top; this reorders the argument popup with a softmax
//! regression over a few features of each row (where the provider put it, what kind of row it is, how recently the file
//! used the name, how much the name echoes the parameter's, whether its type is the parameter's). The weights were fitted
//! offline on public code that is not the held-out set (`docs/bennu-ml-bench.md`) and are a constant here: nothing is read at
//! run time and nothing is learned on the machine — the in-project habits stay `flc_choices` and `flc_habits`.
//!
//! It only ever **reorders**. A habit row that leads the list keeps its place, and no row is dropped or added. It runs behind the
//! same switch as the in-project habits (`flc_feedback`).

use bennu_complete::prelude::MatchCase;
use bennu_proto::prelude::CompletionItem;

use crate::index_service::IndexService;

/// Number of features.
pub(crate) const N: usize = 11;

/// Softmax regression fitted by `mlbench::m3_rank` on the popups of commons-io, commons-collections, mockito, junit5 and
/// jackson-databind (public code, none of the held-out set); order is that of [`features`]. Refit with the same test and paste
/// the line it prints.
pub(crate) const WEIGHTS: [f32; N] = [-0.957_062_2, -0.365_133_35, -4.633_431_4, -1.229_150_5, -0.042_784_37, 2.565_266_6, 3.109_048_6, -0.343_436_12, -0.134_336_59, -2.710_689_5, -0.268_605];

/// What the ranker reads of one row of the popup.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Row<'a> {
    pub name: &'a str,
    pub kind: &'a str,
    pub detail: &'a str,
    pub deprecated: bool,
    /// Occurrences of the name in the lines just above the caret.
    pub recency: usize,
    /// Position among the distinct names, as the provider ordered them.
    pub rank: usize,
}

/// The identifier a row's label starts with (`foo(int)` → `foo`).
pub(crate) fn ident_of(label: &str) -> &str {
    let end = label.find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$')).unwrap_or(label.len());
    &label[..end]
}

fn simple(binary: &str) -> &str {
    binary.rsplit(['/', '.', '$']).next().unwrap_or(binary)
}

fn tokens(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in s.chars() {
        if ch == '_' || !ch.is_alphanumeric() {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur).to_lowercase());
            }
        } else if ch.is_uppercase() && cur.chars().last().is_some_and(|l| l.is_lowercase()) {
            out.push(std::mem::take(&mut cur).to_lowercase());
            cur.push(ch);
        } else {
            cur.push(ch);
        }
    }
    if !cur.is_empty() {
        out.push(cur.to_lowercase());
    }
    out
}

/// Share of words two names have in common (0 when either has none).
fn echo(a: &str, b: &str) -> f32 {
    let (ta, tb) = (tokens(a), tokens(b));
    if ta.is_empty() || tb.is_empty() {
        return 0.0;
    }
    let common = ta.iter().filter(|t| tb.contains(t)).count();
    common as f32 / (ta.len() + tb.len() - common).max(1) as f32
}

/// The features of `row` at an argument whose parameter is `param_type param_name` (either may be empty).
pub(crate) fn features(param_type: &str, param_name: &str, row: &Row) -> [f32; N] {
    let ln = |x: f32| (1.0 + x).ln();
    let is = |k: &str| f32::from(row.kind.contains(k));
    let wanted = param_type.split('<').next().unwrap_or("").trim();
    let type_match = !wanted.is_empty() && simple(row.detail).starts_with(wanted);
    [
        ln(row.rank as f32),
        1.0 / (1.0 + row.rank as f32),
        is("method"),
        is("field"),
        ln(row.recency as f32),
        echo(row.name, param_name),
        f32::from(type_match),
        ln(row.name.len() as f32),
        f32::from(row.deprecated),
        f32::from(row.kind.contains("class") || row.kind.contains("interface") || row.kind.contains("enum")),
        f32::from(row.kind.contains("keyword")),
    ]
}

fn score(w: &[f32; N], f: &[f32; N]) -> f32 {
    let s: f32 = w.iter().zip(f).map(|(w, x)| w * x).sum();
    if s.is_nan() { f32::NEG_INFINITY } else { s }
}

/// The order (indexes into `rows`) after the first `pinned` rows, which keep their places. Ties keep the provider's order.
pub(crate) fn order_with(w: &[f32; N], param_type: &str, param_name: &str, rows: &[Row], pinned: usize) -> Vec<usize> {
    let pinned = pinned.min(rows.len());
    let scores: Vec<f32> = rows.iter().map(|r| score(w, &features(param_type, param_name, r))).collect();
    let mut rest: Vec<usize> = (pinned..rows.len()).collect();
    rest.sort_by(|&a, &b| scores[b].total_cmp(&scores[a]).then(a.cmp(&b)));
    (0..pinned).chain(rest).collect()
}

pub(crate) fn order(param_type: &str, param_name: &str, rows: &[Row], pinned: usize) -> Vec<usize> {
    order_with(&WEIGHTS, param_type, param_name, rows, pinned)
}

/// Lets the bench read the provider's own order, before this ranker touches it.
#[cfg(test)]
pub(crate) static OFF: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// A habit row that leads the list (`used N× in .callee(…)`) is not the ranker's to move.
fn is_habit_row(item: &CompletionItem) -> bool {
    item.detail.as_deref().is_some_and(|d| d.starts_with("used ") && d.contains("× in ."))
}

fn recency_of(source: &str, offset: usize, name: &str) -> usize {
    if name.is_empty() {
        return 0;
    }
    let upto = &source[..offset];
    let from = upto.rmatch_indices('\n').nth(60).map_or(0, |(i, _)| i);
    let window = &upto[from..];
    window
        .match_indices(name)
        .filter(|(i, _)| {
            let before = window[..*i].chars().next_back();
            let after = window[i + name.len()..].chars().next();
            !before.is_some_and(|c| c.is_alphanumeric() || c == '_') && !after.is_some_and(|c| c.is_alphanumeric() || c == '_')
        })
        .count()
}

/// Reorders `items` when the caret is at the start of a call argument (or in the first letters of a plain name there).
/// Does nothing anywhere else: after a dot, in a declaration, outside a call.
pub(crate) fn rerank(svc: &IndexService, file: &str, source: &str, offset: usize, _case: MatchCase, items: &mut Vec<CompletionItem>) {
    #[cfg(test)]
    if OFF.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    if items.len() < 2 || !crate::flc_choices::enabled() {
        return;
    }
    let Some((_, _, start)) = bennu_query::prelude::call_site_name_and_index(source, offset) else { return };
    let Some(typed) = source.get(start..offset) else { return };
    let typed = typed.trim_start();
    if !typed.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$') {
        return;
    }
    let (mut param_type, mut param_name) = (String::new(), String::new());
    if let Some(help) = svc.signature_at(file, source, offset) {
        if let Some((a, b)) = help.params.get(help.active).copied() {
            if let Some((ty, name)) = help.label.get(a..b).map(str::trim).and_then(|p| p.rsplit_once(' ')) {
                param_type = ty.trim().to_string();
                param_name = name.trim().to_string();
            }
        }
    }
    // distinct names, in the provider's order: the rank a row has is the rank of its name
    let mut seen: Vec<&str> = Vec::new();
    let names: Vec<&str> = items.iter().map(|i| ident_of(&i.label)).collect();
    let ranks: Vec<usize> = names
        .iter()
        .map(|n| match seen.iter().position(|s| s == n) {
            Some(p) => p,
            None => {
                seen.push(n);
                seen.len() - 1
            }
        })
        .collect();
    let rows: Vec<Row> = items
        .iter()
        .enumerate()
        .map(|(k, it)| Row { name: names[k], kind: &it.kind, detail: it.detail.as_deref().unwrap_or(""), deprecated: it.deprecated, recency: recency_of(source, offset, names[k]), rank: ranks[k] })
        .collect();
    let pinned = items.iter().take_while(|i| is_habit_row(i)).count();
    let order = order(&param_type, &param_name, &rows, pinned);
    if order.iter().enumerate().all(|(i, &k)| i == k) {
        return;
    }
    let mut reordered: Vec<CompletionItem> = order.iter().map(|&k| items[k].clone()).collect();
    // the selection follows the new first row: a stale preselect further down would jump the highlight
    let led_by_habit = pinned > 0 && reordered[0].preselect;
    for (i, it) in reordered.iter_mut().enumerate() {
        if !(led_by_habit && i < pinned) {
            it.preselect = false;
        }
    }
    *items = reordered;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row<'a>(name: &'a str, kind: &'a str, detail: &'a str, rank: usize) -> Row<'a> {
        Row { name, kind, detail, deprecated: false, recency: 0, rank }
    }

    #[test]
    fn a_variable_of_the_parameters_type_and_name_beats_a_method_that_came_first() {
        let rows = [row("toString", "method", "String", 0), row("customerId", "local", "long", 1), row("other", "local", "String", 2)];
        let o = order("long", "customerId", &rows, 0);
        assert_eq!(o[0], 1);
    }

    #[test]
    fn it_is_deterministic_and_ties_keep_the_providers_order() {
        let rows = [row("a", "local", "int", 0), row("b", "local", "int", 0), row("c", "local", "int", 0)];
        let first = order_with(&[0.0; N], "", "", &rows, 0);
        assert_eq!(first, vec![0, 1, 2]);
        assert_eq!(order("int", "x", &rows, 0), order("int", "x", &rows, 0));
    }

    #[test]
    fn nothing_and_one_row_have_nothing_to_order() {
        assert!(order("int", "x", &[], 0).is_empty());
        assert_eq!(order("int", "x", &[row("a", "local", "int", 0)], 0), vec![0]);
    }

    #[test]
    fn pinned_rows_keep_their_places_and_a_pin_past_the_end_is_harmless() {
        let rows = [row("habit", "method", "", 0), row("x", "method", "", 1), row("y", "local", "int", 2)];
        assert_eq!(order("int", "", &rows, 1)[0], 0);
        assert_eq!(order("int", "", &rows, 99).len(), 3);
    }

    #[test]
    fn degenerate_rows_do_not_panic() {
        let weird = Row { name: "", kind: "", detail: "", deprecated: true, recency: usize::MAX / 2, rank: usize::MAX / 2 };
        let o = order("", "", &[weird, weird], 0);
        assert_eq!(o, vec![0, 1]);
        let nan = [f32::NAN; N];
        assert_eq!(order_with(&nan, "", "", &[weird, weird], 0), vec![0, 1]);
    }

    #[test]
    fn the_embedded_weights_are_finite_and_trust_the_provider_a_little() {
        assert!(WEIGHTS.iter().all(|w| w.is_finite()));
        assert!(WEIGHTS[0] < 0.0, "a worse provider rank must not score higher");
    }

    #[test]
    fn labels_are_cut_at_their_identifier_and_names_echo_by_words() {
        assert_eq!(ident_of("getFoo(int)"), "getFoo");
        assert!(echo("customerId", "customer_id") > 0.99);
        assert_eq!(echo("", "x"), 0.0);
        assert_eq!(recency_of("a\nfoo(x)\nfoo(", 13, "foo"), 2);
        assert_eq!(recency_of("a", 1, ""), 0);
    }
}
