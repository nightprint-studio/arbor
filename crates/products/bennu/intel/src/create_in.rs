//! Where the method a call is asking for goes, and what it says — in the **other** file.
//!
//! ## Why this is separate from the service
//!
//! `IndexService::create_method_in` does three things a test cannot: it resolves the receiver's
//! type against a live index, it asks which file declares it, and it reads that file off disk in
//! the project's encoding. What is left after those three is the part that can actually be wrong
//! in an interesting way — where in the class body the member goes, what it says, and which
//! imports the target is missing — and it is a function of two strings.
//!
//! So it is a function of two strings. The service resolves and reads; this decides.
//!
//! ## The imports
//!
//! Computed against the **original** target buffer, and returned alongside the member rather than
//! after it. A generated method usually names types the target file has never mentioned — the
//! parameter types come from the *call site*, which is a different file with different imports —
//! and a member inserted without them is code that does not compile, produced by a fix.
//!
//! Only an **unambiguous** simple name is imported. A name several packages declare would be a
//! guess about which one the call site meant, and a wrong import is worse than a missing one: it
//! compiles, against the wrong type.

use bennu_refactor::prelude::ForeignCall;

/// One edit to the target file: a byte range and what replaces it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetEdit {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

/// The edits that write `call` into `target_source`, whose type is named `type_simple`.
///
/// Empty when the class already declares that name at any arity — a wrong overload is a different
/// problem, and adding a second one beside the first is not what "create method" means — or when
/// the file's class body cannot be located.
///
/// `fqn_of` maps a simple type name to the one fully-qualified name that declares it, or `None`
/// when it is ambiguous, unknown, or needs no import. Passed in rather than resolved here so the
/// classpath stays the service's business and this stays a function of its arguments.
pub fn foreign_member_edits(
    target_source: &str,
    type_simple: &str,
    call: &ForeignCall,
    fqn_of: &dyn Fn(&str) -> Option<String>,
    import_edit: &dyn Fn(&str, &str) -> Option<(usize, usize, String)>,
) -> Vec<TargetEdit> {
    if declares_method(target_source, &call.name) {
        return Vec::new();
    }
    let Some((_, name_end)) = bennu_java::prelude::find_type_name_span(target_source, type_simple)
    else {
        return Vec::new();
    };
    // A position INSIDE the body, not the type's name: the insertion point is found by walking out
    // to the innermost enclosing brace pair, and a class name sits before its own `{`. The first
    // brace after the name opens the body — what may stand between them is a type parameter list,
    // an `extends` and an `implements`, none of which contains one.
    let Some(open) = target_source[name_end..].find('{').map(|i| name_end + i) else {
        return Vec::new();
    };
    let Some((close, indent)) =
        bennu_intentions::prelude::class_body_insertion(target_source, open + 1)
    else {
        return Vec::new();
    };
    // The file's own line ending. Writing `\n` into a CRLF file leaves one line that is not like
    // the others, which every diff after it will show.
    let nl = if target_source.contains("\r\n") { "\r\n" } else { "\n" };
    let member = call.render(&indent, nl);

    let mut out = vec![TargetEdit {
        start: close,
        end: close,
        text: format!("{nl}{indent}{member}{nl}"),
    }];
    for fqn in import_names(call, fqn_of) {
        if let Some((start, end, text)) = import_edit(target_source, &fqn) {
            out.push(TargetEdit { start, end, text });
        }
    }
    out
}

/// The fully-qualified names the member's signature needs importing.
///
/// A generic type is imported by its **raw** name and its arguments are their own names, so
/// `List<Order>` needs both. A lowercase segment is a primitive or a type variable and is neither.
fn import_names(call: &ForeignCall, fqn_of: &dyn Fn(&str) -> Option<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for written in call.type_names() {
        for part in written.split(['<', '>', ',', '[', ']']) {
            let part = part.trim();
            if part.is_empty() || !part.starts_with(|c: char| c.is_uppercase()) {
                continue;
            }
            if let Some(fqn) = fqn_of(part) {
                if !out.contains(&fqn) {
                    out.push(fqn);
                }
            }
        }
    }
    out
}

/// Whether `source` already declares a method called `name`, at any arity — the same question
/// `create_method` asks of the file it is editing, asked of the file it is writing into.
pub fn declares_method(source: &str, name: &str) -> bool {
    bennu_java::prelude::extract_symbols(source)
        .types
        .iter()
        .any(|t| t.methods.iter().any(|m| m.name == name))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TARGET: &str = "package shop;\n\npublic class Order {\n    private String title;\n\n    public String getTitle() {\n        return title;\n    }\n}\n";

    /// The call site, read the way the editor reads it.
    fn call_in(source: &str, call: &str) -> ForeignCall {
        let tree = bennu_java::prelude::parse_java(source).expect("parse");
        let name = call.split('(').next().unwrap().rsplit('.').next().unwrap();
        let at = source.find(call).unwrap() + call.find(name).unwrap();
        bennu_refactor::prelude::foreign_call_at(tree.root_node(), source, at, at + name.len())
            .expect("a foreign call")
    }

    fn no_imports(_: &str) -> Option<String> {
        None
    }

    /// Stand-in for the real inserter, which lives in the backend: it answers a range and a line.
    fn stub_import(_source: &str, fqn: &str) -> Option<(usize, usize, String)> {
        Some((14, 14, format!("import {fqn};\n")))
    }

    fn edits(target: &str, call: &ForeignCall) -> Vec<TargetEdit> {
        foreign_member_edits(target, "Order", call, &no_imports, &stub_import)
    }

    /// Applying the edits back to front, which is how the editor applies them.
    fn applied(target: &str, mut es: Vec<TargetEdit>) -> String {
        es.sort_by(|a, b| b.start.cmp(&a.start));
        let mut out = target.to_string();
        for e in es {
            out.replace_range(e.start..e.end, &e.text);
        }
        out
    }

    #[test]
    fn the_member_lands_inside_the_target_class() {
        let site = "package shop;\nclass Till {\n    void f(Order order, String label) {\n        order.total(label, 3);\n    }\n}";
        let out = applied(TARGET, edits(TARGET, &call_in(site, "order.total(label")));
        assert!(out.contains("public void total(String label, int arg2) {"), "{out}");
        // Inside the body, after the last member — not after the closing brace.
        let member = out.find("public void total").unwrap();
        assert!(member > out.find("public String getTitle").unwrap(), "{out}");
        assert!(member < out.rfind('}').unwrap(), "{out}");
        // And the class still closes exactly once.
        assert_eq!(out.matches("public class Order").count(), 1);
    }

    /// The generated member is indented like the ones around it. A method at column zero inside a
    /// class body is the tell that a generator wrote it.
    #[test]
    fn the_member_is_indented_like_the_class_it_joins() {
        let site = "package shop;\nclass Till {\n    void f(Order order) {\n        order.ship();\n    }\n}";
        let out = applied(TARGET, edits(TARGET, &call_in(site, "order.ship()")));
        assert!(out.contains("\n    public void ship() {\n        throw new"), "{out:?}");
    }

    /// A wrong overload is a different problem, and adding a second `getTitle` beside the first is
    /// not what "create method" means.
    #[test]
    fn a_name_the_class_already_declares_produces_nothing() {
        let site = "package shop;\nclass Till {\n    void f(Order order) {\n        String s = order.getTitle();\n    }\n}";
        assert!(edits(TARGET, &call_in(site, "order.getTitle()")).is_empty());
    }

    /// The parameter types come from the CALL SITE, which is a different file with different
    /// imports. A member inserted without them is code that does not compile, produced by a fix.
    #[test]
    fn the_types_the_signature_names_are_imported() {
        let site = "package shop;\nclass Till {\n    void f(Order order, Customer c) {\n        Invoice i = order.bill(c);\n    }\n}";
        let fqn = |simple: &str| match simple {
            "Customer" => Some("shop.people.Customer".to_string()),
            "Invoice" => Some("shop.billing.Invoice".to_string()),
            _ => None,
        };
        let es = foreign_member_edits(TARGET, "Order", &call_in(site, "order.bill(c"), &fqn, &stub_import);
        let out = applied(TARGET, es);
        assert!(out.contains("import shop.people.Customer;"), "{out}");
        assert!(out.contains("import shop.billing.Invoice;"), "{out}");
    }

    /// A name several packages declare is a guess about which one the call site meant, and a wrong
    /// import is worse than a missing one: it compiles, against the wrong type.
    #[test]
    fn an_ambiguous_type_is_not_imported() {
        let site = "package shop;\nclass Till {\n    void f(Order order, List<String> xs) {\n        order.take(xs);\n    }\n}";
        // `None` is what the caller answers for a name it cannot pin to one package.
        let es = edits(TARGET, &call_in(site, "order.take(xs"));
        assert_eq!(es.len(), 1, "the member only: {es:?}");
    }

    /// A generic type is imported by its raw name AND its arguments — `List<Order>` needs both.
    #[test]
    fn a_generic_type_asks_for_both_halves() {
        let site = "package shop;\nclass Till {\n    void f(Order order, List<Customer> cs) {\n        order.take(cs);\n    }\n}";
        let asked = std::cell::RefCell::new(Vec::<String>::new());
        let fqn = |simple: &str| {
            asked.borrow_mut().push(simple.to_string());
            None
        };
        let _ = foreign_member_edits(TARGET, "Order", &call_in(site, "order.take(cs"), &fqn, &stub_import);
        let asked = asked.into_inner();
        assert!(asked.contains(&"List".to_string()), "{asked:?}");
        assert!(asked.contains(&"Customer".to_string()), "{asked:?}");
    }

    /// Writing `\n` into a CRLF file leaves one line that is not like the others, which every diff
    /// after it will show.
    #[test]
    fn a_crlf_file_keeps_its_line_endings() {
        let crlf = TARGET.replace('\n', "\r\n");
        let site = "package shop;\nclass Till {\n    void f(Order order) {\n        order.ship();\n    }\n}";
        let es = edits(&crlf, &call_in(site, "order.ship()"));
        assert!(!es[0].text.contains('\n') || es[0].text.contains("\r\n"), "{:?}", es[0].text);
        assert!(!es[0].text.replace("\r\n", "").contains('\n'), "{:?}", es[0].text);
    }
}
