//! The quick-fixes that need to know types.
//!
//! [`bennu_intentions::prelude::fixes_for`] handles the ones decidable from the text at the
//! diagnostic's span — remove the import, add the `break`, use `equals`. The two here cannot be:
//! *which* exception to declare and *which* enum constants are missing are answers only the
//! resolver has.
//!
//! Both recompute from the **same analysis that produced the diagnostic** rather than reading its
//! message. That is not fastidiousness: an analysis and a fix that disagree produce an edit that
//! does not remove the red squiggle, and the user is left pressing Alt+Enter on a fix that "doesn't
//! work". One source of truth, two renderings of it.

use bennu_java::prelude::{InferCache, TypeResolver};

use crate::intentions::OfferWire;

/// The fixes for whichever diagnostic of `code` covers `offset`, using the project's resolver.
///
/// `code` and the span come from the diagnostic the editor already has; `source` is the live buffer.
/// Empty for a code with no resolver-backed fix, and empty whenever the analysis no longer agrees
/// that there is a problem there — a diagnostic can outlive the text by a keystroke.
pub(crate) fn resolver_fixes(
    code: &str,
    source: &str,
    start: usize,
    end: usize,
    resolver: &dyn TypeResolver,
) -> Vec<OfferWire> {
    match code {
        "unhandled-checked-exception" => unhandled_exception_fixes(source, start, end, resolver),
        "non-exhaustive-enum-switch" => enum_switch_fixes(source, start, end, resolver),
        _ => Vec::new(),
    }
}

/// The fixes that need only the **tree** — no resolver, because the resolver has already spoken.
///
/// *Create method* is the case: whether `handle()` exists is a question about the whole classpath,
/// and nothing here could answer it. The diagnostic already did, by existing. So this renders the
/// repair for the span it points at and asks no questions of its own — which is also why it is
/// offered on a cold index, when `resolver_fixes` cannot run at all.
pub(crate) fn tree_fixes(code: &str, source: &str, start: usize, end: usize) -> Vec<OfferWire> {
    // `unresolved-call` is the one that matters most: it is the BARE call — `report()` in the class
    // you are writing — which is where a method gets written before it exists. `unknown-member`
    // covers `this.report()` and the qualified calls, which mostly end in a refusal about another
    // file, and `unresolved-symbol` the bare identifier.
    if !matches!(
        code,
        "unresolved-call" | "unknown-member" | "unresolved-symbol" | "unresolved-type"
    ) {
        return Vec::new();
    }
    let Some(tree) = bennu_java::prelude::parse_java(source) else { return Vec::new() };
    if code == "unresolved-type" {
        // Creating a file is not an edit, so the offer carries an ACTION and the name as its
        // payload — the same shape the rename offers use.
        return bennu_refactor::prelude::missing_type_at(tree.root_node(), source, start, end)
            .map(|missing| OfferWire {
                id: "create-class".to_string(),
                label: format!("Create {} '{}'", missing.keyword.trim_start_matches('@'), missing.name),
                start,
                end,
                replacement: missing.name,
                action: Some("create-class".to_string()),
            })
            .into_iter()
            .collect();
    }
    let Some(Ok(plan)) = bennu_refactor::prelude::create_method(tree.root_node(), source, start, end)
    else {
        return Vec::new(); // a refusal here is about another file; the menu says nothing
    };
    plan.edits
        .iter()
        .map(|edit| OfferWire {
            id: plan.id.clone(),
            label: plan.label.clone(),
            start: edit.start,
            end: edit.end,
            replacement: edit.text.clone(),
            action: None,
        })
        .collect()
}

/// "Add `throws IOException`" and "Surround with try/catch", for the call the diagnostic underlines.
fn unhandled_exception_fixes(
    source: &str,
    start: usize,
    end: usize,
    resolver: &dyn TypeResolver,
) -> Vec<OfferWire> {
    let Some(tree) = bennu_java::prelude::parse_java(source) else { return Vec::new() };
    let root = tree.root_node();
    let nodes = bennu_check::prelude::collect_nodes(root);
    let symbols = bennu_java::prelude::extract_symbols(source);
    let calls = bennu_check::prelude::unhandled_calls_in(
        root,
        &nodes,
        source,
        &symbols,
        resolver,
        &InferCache::new(),
    );
    let Some(call) = calls.into_iter().find(|c| c.anchor == (start, end)) else {
        return Vec::new(); // the analysis no longer sees a problem here
    };
    let simple = call.exception.rsplit(['/', '$']).next().unwrap_or(&call.exception).to_string();

    let mut out = Vec::new();

    // (a) Declare it. One edit, and the shape is the same whether the callable already has a
    // `throws` clause or not — the insertion point is just after the `)`, so an existing clause is
    // extended by writing `, X` in front of it… except that it isn't: the existing clause starts
    // with the word `throws`. Read what follows to tell the two apart.
    let after = source[call.throws_insert..].trim_start();
    let already_declares = after.starts_with("throws");
    if already_declares {
        // Append to the existing list, just after the word `throws`.
        if let Some(rel) = source[call.throws_insert..].find("throws") {
            let at = call.throws_insert + rel + "throws".len();
            out.push(OfferWire {
                id: format!("declare-throws:{}", call.exception),
                label: format!("Add `{simple}` to the `throws` clause"),
                start: at,
                end: at,
                replacement: format!(" {simple},"),
                action: None,
            });
        }
    } else {
        out.push(OfferWire {
            id: format!("declare-throws:{}", call.exception),
            label: format!("Add `throws {simple}` to the method"),
            start: call.throws_insert,
            end: call.throws_insert,
            replacement: format!(" throws {simple}"),
            action: None,
        });
    }

    // (b) Catch it. The statement, not the call — `byte[] b = try { … }` is not Java.
    let (s0, s1) = call.statement;
    if s1 > s0 && s1 <= source.len() {
        let indent = line_indent(source, s0);
        let unit = "    ";
        let body = source[s0..s1].to_string();
        out.push(OfferWire {
            id: format!("surround-try:{}", call.exception),
            label: format!("Surround with try/catch ({simple})"),
            start: s0,
            end: s1,
            replacement: format!(
                "try {{\n{indent}{unit}{body}\n{indent}}} catch ({simple} e) {{\n\
                 {indent}{unit}throw new RuntimeException(e);\n{indent}}}"
            ),
            action: None,
        });
    }
    out
}

/// "Add the missing cases" for a `switch` the exhaustiveness check flagged.
fn enum_switch_fixes(
    source: &str,
    start: usize,
    end: usize,
    resolver: &dyn TypeResolver,
) -> Vec<OfferWire> {
    let Some(tree) = bennu_java::prelude::parse_java(source) else { return Vec::new() };
    let root = tree.root_node();
    let symbols = bennu_java::prelude::extract_symbols(source);
    let cache = InferCache::new();

    // The diagnostic spans the whole `switch`, so the node is found by its span rather than searched
    // for — no ambiguity about which switch is meant when two are nested. The climb is for the case
    // where the smallest node covering the span is a wrapper around it (an expression statement, a
    // declarator) rather than the switch itself.
    let Some(mut switch) = root.descendant_for_byte_range(start, end) else { return Vec::new() };
    while switch.kind() != "switch_expression" {
        match switch.parent() {
            Some(p) => switch = p,
            None => return Vec::new(),
        }
    }
    let (Some(cond), Some(body)) =
        (switch.child_by_field_name("condition"), switch.child_by_field_name("body"))
    else {
        return Vec::new();
    };
    let Some(sel) =
        bennu_java::prelude::infer_node_type_cached(&root, source, &symbols, &cond, resolver, &cache)
    else {
        return Vec::new();
    };
    let Some(members) = resolver.members_of(&sel.binary_name) else { return Vec::new() };
    if !members.flags.is_enum {
        return Vec::new();
    }
    let constants = bennu_check::prelude::enum_constants(&members, &sel.binary_name);
    if constants.is_empty() {
        return Vec::new();
    }
    let covered = covered_labels(body, source.as_bytes());
    let missing: Vec<&String> = constants.iter().filter(|c| !covered.contains(*c)).collect();
    if missing.is_empty() {
        return Vec::new();
    }

    // Inserted just before the closing `}`, matching the arms already there: an arrow switch gets
    // arrows and a colon switch gets colons, because mixing the two forms in one switch does not
    // compile.
    let close = body.end_byte().saturating_sub(1);
    let arrow = source[body.start_byte()..body.end_byte()].contains("->");
    let indent = format!("{}    ", line_indent(source, switch.start_byte()));
    let mut text = String::new();
    for name in &missing {
        if arrow {
            text.push_str(&format!("{indent}case {name} -> throw new UnsupportedOperationException(\"{name}\");\n"));
        } else {
            text.push_str(&format!("{indent}case {name}:\n{indent}    throw new UnsupportedOperationException(\"{name}\");\n"));
        }
    }
    vec![OfferWire {
        id: "fill-enum-switch".to_string(),
        label: format!(
            "Add the missing case{} ({})",
            if missing.len() == 1 { "" } else { "s" },
            missing.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
        ),
        start: close,
        end: close,
        replacement: text,
        action: None,
    }]
}

/// The constant names the switch's labels already name.
///
/// A body-scoped read of the same two label shapes `bennu-check` recognises (bare `A`, qualified
/// `Status.A`); anything else contributes nothing, which can only make the fix offer a case that is
/// already there rather than miss one.
fn covered_labels(body: tree_sitter::Node, bytes: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut bc = body.walk();
    for arm in body.named_children(&mut bc) {
        if !matches!(arm.kind(), "switch_rule" | "switch_block_statement_group") {
            continue;
        }
        let mut ac = arm.walk();
        for label in arm.named_children(&mut ac) {
            if label.kind() != "switch_label" {
                continue;
            }
            let mut lc = label.walk();
            for cst in label.named_children(&mut lc) {
                let name = match cst.kind() {
                    "identifier" => cst.utf8_text(bytes).ok(),
                    "field_access" => cst
                        .child_by_field_name("field")
                        .and_then(|f| f.utf8_text(bytes).ok()),
                    _ => None,
                };
                if let Some(n) = name {
                    out.push(n.to_string());
                }
            }
        }
    }
    out
}

/// The leading whitespace of the line `offset` sits on — so an inserted block lines up with the code
/// it is replacing rather than starting at column zero.
fn line_indent(source: &str, offset: usize) -> String {
    let line_start = source[..offset].rfind('\n').map(|i| i + 1).unwrap_or(0);
    source[line_start..].chars().take_while(|c| *c == ' ' || *c == '\t').collect()
}

