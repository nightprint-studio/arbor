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
use bennu_refactor::prelude::{EditSelection, Plan};

use crate::intentions::{edit_wire, offer_of, EditWire, OfferWire};

/// The catch body *Surround with try/catch* writes: rethrown unchecked, so nothing is swallowed
/// while the user decides what handling the exception really needs.
const CATCH_BODY: &str = "throw new RuntimeException(e);";

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
        // `order.total()` where `Order` declares no `total`. The pure transform refuses this — it
        // edits one buffer and cannot say which file the method belongs in — and the resolver can.
        // The same code marks `this::total`, whose signature only the resolver can read.
        "unknown-member" => {
            let mut out = create_in_receiver_fixes(source, start, end, resolver);
            out.extend(create_for_reference_fixes(source, start, end, resolver));
            out
        }
        _ => Vec::new(),
    }
}

/// "Create method 'toIdentity'" — for a method reference to this class, `this::toIdentity` or
/// `Outer::toIdentity`.
///
/// A reference has no arguments to read a signature off, so unlike a call this needs the resolver:
/// the signature is the functional interface the reference is passed as. The imports the signature
/// needs travel in the same offer, so the member compiles the moment it is written — and all of it
/// is one undo.
fn create_for_reference_fixes(
    source: &str,
    start: usize,
    end: usize,
    resolver: &dyn TypeResolver,
) -> Vec<OfferWire> {
    let Some(tree) = bennu_java::prelude::parse_java(source) else { return Vec::new() };
    let root = tree.root_node();
    let Some(reference) = bennu_refactor::prelude::local_reference_at(root, source, start, end)
    else {
        return Vec::new();
    };
    let symbols = bennu_java::prelude::extract_symbols(source);
    let Some(referenced) = bennu_intel::prelude::reference_call(
        root,
        source,
        &symbols,
        &reference,
        resolver,
        &InferCache::new(),
    ) else {
        return Vec::new();
    };
    let Some(plan) =
        bennu_refactor::prelude::declare_local_method(root, source, reference.start, &referenced.call)
    else {
        return Vec::new();
    };
    let imports: Vec<EditWire> = referenced
        .types
        .iter()
        .filter_map(|fqn| crate::intentions::import_edit_for(source, fqn))
        .map(|(start, end, text)| EditWire { start, end, text })
        .collect();
    plan_offer(&plan, imports).into_iter().collect()
}

/// A plan as ONE offer — its edits, then `extra` (the imports its signature needs) — carrying the
/// plan's selection. The plan's edits lead the list in their own order, so the index the selection
/// names is the same edit on the wire. `None` for a plan with nothing to write.
fn plan_offer(plan: &Plan, extra: Vec<EditWire>) -> Option<OfferWire> {
    if plan.edits.is_empty() {
        return None;
    }
    let mut edits: Vec<EditWire> = plan.edits.iter().map(edit_wire).collect();
    edits.extend(extra);
    Some(offer_of(&plan.id, &plan.label, edits, plan.select))
}

/// "Create method 'total' in Order" — for a call on another object.
///
/// The offer only. Whether the class already declares the name, whether its file can be read and
/// what imports the member needs are answered when it is RUN
/// (`bennu_create_method_in`), against the target file — which this does not have.
///
/// Offered only for a **project** type: a dependency's class has no source to write into, and a
/// decompiled stub is not a file anyone can edit.
fn create_in_receiver_fixes(
    source: &str,
    start: usize,
    end: usize,
    resolver: &dyn TypeResolver,
) -> Vec<OfferWire> {
    let Some(tree) = bennu_java::prelude::parse_java(source) else { return Vec::new() };
    let Some(call) = bennu_refactor::prelude::foreign_call_at(tree.root_node(), source, start, end)
    else {
        return Vec::new();
    };
    // `start` is the called name's first byte, which is one past the `.` — the position the
    // receiver inference expects.
    let Some(recv) = bennu_java::prelude::infer_receiver_type(source, start, resolver) else {
        return Vec::new();
    };
    if !resolver.is_project_type(&recv.binary_name) {
        return Vec::new();
    }
    let simple = recv
        .binary_name
        .rsplit(['/', '$'])
        .next()
        .unwrap_or(&recv.binary_name);
    vec![OfferWire {
        id: "create-method-in".to_string(),
        label: format!("Create method '{}' in {simple}", call.name),
        start,
        end,
        // The name, for a caller that wants to say what it wrote. The edits come from the handler.
        replacement: call.name,
        action: Some("create-method-in".to_string()),
        edits: Vec::new(),
        select: None,
    }]
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
                edits: Vec::new(),
                select: None,
            })
            .into_iter()
            .collect();
    }
    let Some(Ok(plan)) = bennu_refactor::prelude::create_method(tree.root_node(), source, start, end)
    else {
        return Vec::new(); // a refusal here is about another file; the menu says nothing
    };
    plan_offer(&plan, Vec::new()).into_iter().collect()
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
    //
    // Only a method or a constructor has a clause to extend: a lambda answers to its target's
    // signature and an initializer to none, so neither gets this offer.
    if let Some(throws_insert) = call.throws_insert {
        let after = source[throws_insert..].trim_start();
        let already_declares = after.starts_with("throws");
        if already_declares {
            // Append to the existing list, just after the word `throws`.
            if let Some(rel) = source[throws_insert..].find("throws") {
                let at = throws_insert + rel + "throws".len();
                out.push(OfferWire {
                    id: format!("declare-throws:{}", call.exception),
                    label: format!("Add `{simple}` to the `throws` clause"),
                    start: at,
                    end: at,
                    replacement: format!(" {simple},"),
                    action: None,
                    edits: Vec::new(),
                    select: None,
                });
            }
        } else {
            out.push(OfferWire {
                id: format!("declare-throws:{}", call.exception),
                label: format!("Add `throws {simple}` to the method"),
                start: throws_insert,
                end: throws_insert,
                replacement: format!(" throws {simple}"),
                action: None,
                edits: Vec::new(),
                select: None,
            });
        }
    }

    // (b) Catch it. The statement, not the call — `byte[] b = try { … }` is not Java — and only where
    // there is one: a field initializer has no statement to wrap.
    if let Some(statement) = call.statement {
        out.extend(surround_with_try(source, statement, &call.exception, &simple));
    }
    out
}

/// *Surround with try/catch* around the statement at `[s0, s1)`, with the catch body selected — the
/// line IntelliJ leaves for the user to replace with the handling the exception really needs.
fn surround_with_try(source: &str, (s0, s1): (usize, usize), exception: &str, simple: &str) -> Option<OfferWire> {
    if s1 <= s0 || s1 > source.len() {
        return None;
    }
    let indent = line_indent(source, s0);
    let unit = "    ";
    let body = &source[s0..s1];
    let text = format!(
        "try {{\n{indent}{unit}{body}\n{indent}}} catch ({simple} e) {{\n{indent}{unit}{CATCH_BODY}\n{indent}}}"
    );
    // The LAST occurrence: the wrapped statement sits above the catch and may itself rethrow that way.
    let select = text.rfind(CATCH_BODY).map(|start| EditSelection { edit: 0, start, end: start + CATCH_BODY.len() });
    Some(offer_of(
        &format!("surround-try:{exception}"),
        &format!("Surround with try/catch ({simple})"),
        vec![EditWire { start: s0, end: s1, text }],
        select,
    ))
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
    let missing: Vec<&str> = missing.iter().map(|s| s.as_str()).collect();
    missing_cases_offer(close, &indent, arrow, &missing).into_iter().collect()
}

/// *Add the missing cases*: an arm per constant in `missing`, inserted at `close` (the body's `}`),
/// with the FIRST arm's placeholder `throw` selected — one selection is what a keystroke replaces,
/// and the first arm is where the eye already is.
fn missing_cases_offer(close: usize, indent: &str, arrow: bool, missing: &[&str]) -> Option<OfferWire> {
    fn placeholder(name: &str) -> String {
        format!("throw new UnsupportedOperationException(\"{name}\");")
    }
    let first = placeholder(missing.first()?);
    let mut text = String::new();
    for &name in missing {
        if arrow {
            text.push_str(&format!("{indent}case {name} -> {}\n", placeholder(name)));
        } else {
            text.push_str(&format!("{indent}case {name}:\n{indent}    {}\n", placeholder(name)));
        }
    }
    let select = text.find(&first).map(|start| EditSelection { edit: 0, start, end: start + first.len() });
    let label = format!(
        "Add the missing case{} ({})",
        if missing.len() == 1 { "" } else { "s" },
        missing.join(", ")
    );
    Some(offer_of("fill-enum-switch", &label, vec![EditWire { start: close, end: close, text }], select))
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

#[cfg(test)]
mod tests {
    use super::{missing_cases_offer, plan_offer, surround_with_try, tree_fixes};
    use crate::intentions::{EditWire, OfferWire};
    use bennu_refactor::prelude::{declare_local_method, LocalCall};

    /// The text an offer's selection covers inside the edit it names: `edits[edit]`, or the
    /// replacement as edit `0` on a single-range offer.
    fn selected(offer: &OfferWire) -> &str {
        let select = offer.select.expect("the offer selects");
        let text = if offer.edits.is_empty() { &offer.replacement } else { &offer.edits[select.edit].text };
        &text[select.start..select.end]
    }

    #[test]
    fn create_method_reaches_the_offer_with_its_placeholder_selected() {
        let source = "class A {\n    void f() {\n        report();\n    }\n}\n";
        let at = source.find("report").unwrap();
        let offers = tree_fixes("unresolved-call", source, at, at + "report".len());
        let [offer] = offers.as_slice() else { panic!("one offer: {offers:?}") };
        assert_eq!(offer.id, "create-method");
        assert_eq!(selected(offer), "throw new UnsupportedOperationException(\"TODO: report\");");
        assert!(serde_json::to_value(offer).unwrap().get("select").is_some());
    }

    /// The imports a reference's signature needs travel AFTER the stub, so the selection still
    /// names the stub.
    #[test]
    fn a_reference_offer_with_imports_still_selects_the_stubs_body() {
        let source = "class A {\n    Object f(java.util.Optional<String> o) {\n        return o.map(this::shout);\n    }\n}\n";
        let tree = bennu_java::prelude::parse_java(source).unwrap();
        let call = LocalCall {
            name: "shout".to_string(),
            params: vec![("String".to_string(), "string".to_string())],
            returns: "Object".to_string(),
            is_static: false,
        };
        let plan = declare_local_method(tree.root_node(), source, source.find("this::shout").unwrap(), &call)
            .expect("a plan");
        let import = EditWire { start: 0, end: 0, text: "import java.util.List;\n".to_string() };
        let offer = plan_offer(&plan, vec![import]).expect("an offer");
        assert_eq!(offer.edits.len(), 2);
        assert_eq!(selected(&offer), "throw new UnsupportedOperationException(\"TODO: shout\");");
    }

    #[test]
    fn surround_with_try_selects_the_catch_body() {
        let source = "class A {\n    void f() {\n        read();\n    }\n}\n";
        let s0 = source.find("read();").unwrap();
        let offer = surround_with_try(source, (s0, s0 + "read();".len()), "java/io/IOException", "IOException")
            .expect("an offer");
        assert_eq!(
            offer.replacement,
            "try {\n            read();\n        } catch (IOException e) {\n            throw new RuntimeException(e);\n        }"
        );
        assert_eq!(selected(&offer), "throw new RuntimeException(e);");
    }

    #[test]
    fn the_first_missing_case_has_its_placeholder_selected() {
        for arrow in [true, false] {
            let offer = missing_cases_offer(40, "        ", arrow, &["PAID", "SHIPPED"]).expect("an offer");
            assert_eq!(selected(&offer), "throw new UnsupportedOperationException(\"PAID\");", "arrow: {arrow}");
            assert!(offer.label.ends_with("(PAID, SHIPPED)"), "{}", offer.label);
        }
    }
}

