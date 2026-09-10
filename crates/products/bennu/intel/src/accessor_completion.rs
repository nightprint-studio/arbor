//! Completions for members that **do not exist yet** — offered where a member goes.
//!
//! Two families, and they are the same gesture: you are in a class body, you start typing a name,
//! and the thing you mean has not been written.
//!
//! * the **accessors a field is missing** — `getCust` → `getCustomer()`;
//! * the **methods this class calls and does not declare** — you wrote `randomico()` in a method,
//!   and now `rand` in the class body means the member you owe it.
//!
//! The second is deliberately NOT read off the diagnostic. A half-typed `rand` in a class body is
//! a syntax error, so the file stops validating and the squiggle on `randomico()` disappears — at
//! exactly the keystroke where the offer is wanted. The call is still in the tree, though: the
//! half-written name recovers as an ERROR node beside its siblings, and everything around it
//! parses. Reading the calls rather than the diagnostics is what makes the offer survive being
//! typed.
//!
//! ## Why a completion rather than only a dialog
//!
//! Arbor generates accessors already: **Generate** (Alt+Insert) asks which fields, in which style,
//! and writes them all. That is the right shape when you are finishing a class. It is the wrong
//! shape when you are in the middle of one: you know the name, you have typed half of it, and
//! being sent to a dialog to find the field in a checklist is exactly the interruption the popup
//! exists to avoid.
//!
//! So the same members are offered where they are reached for. Typing `getNa` in a class body
//! offers `getName()`, and accepting it writes the whole method.
//!
//! ## What makes this safe to offer unasked
//!
//! It is not a guess. A field that has no getter is a fact about the text, the accessor's name is
//! Java's own convention, and its body is the only body it could have. That is the same standard
//! the ghost text holds itself to — *derived, not predicted* — which is why the same answer can be
//! drawn ahead of the caret when exactly one candidate matches.
//!
//! Three things keep it from firing where it is not wanted:
//!
//! * **only at a member position** — inside a class body, not inside a method, a field
//!   initializer, or a lambda. `getName` written inside a method body is a call;
//! * **only for a field with no accessor already** — the one Generate would skip too;
//! * **only for a name that matches what was typed**, by the same rule every other candidate is
//!   matched by.
//!
//! ## No resolver
//!
//! Deliberately: everything it needs is in the buffer. The fields of the class being edited are
//! written in it, with their types **as written** — which is what the accessor should say, rather
//! than a binary name rendered back into a simple one — and so are the methods that already exist.
//! An accessor inherited from a superclass is not consulted, and that is right: a getter for
//! *this* class's private field is not one a supertype could have.

use bennu_complete::prelude::{MatchCase, Typed};
use bennu_intentions::prelude::{accessor_name, offers, render_accessor, Accessor, FieldSpec};
use bennu_java::prelude::{extract_symbols, parse_java, type_decl_at, TypeDecl, TypeResolver};
use bennu_proto::prelude::CompletionItem;
use tree_sitter::Node;

/// The identifier spliced at the caret so a half-written member parses — the same repair the rest
/// of completion makes.
const SITE_PLACEHOLDER: &str = "x";

/// Everything the class around `byte_offset` could declare here and has not — both families.
///
/// `resolver` is what tells an undeclared call from an **inherited** one, which is a question
/// about the whole classpath. Without it that family is skipped entirely rather than guessed at:
/// offering to write a method a supertype already declares is offering to break the build.
pub fn generated_members(
    source: &str,
    byte_offset: usize,
    case: MatchCase,
    resolver: Option<&dyn TypeResolver>,
) -> Vec<CompletionItem> {
    let mut out = accessor_completions(source, byte_offset, case);
    if let Some(resolver) = resolver {
        out.extend(missing_method_completions(source, byte_offset, case, resolver));
    }
    out
}

/// Where a member would go, and what is already there — gathered once for both families.
struct Site<'t> {
    sited: String,
    type_node: Node<'t>,
    symbols: bennu_java::prelude::FileSymbols,
    typed_text: String,
    case: MatchCase,
    indent: String,
    /// The caret, in `sited` coordinates (which are the source's, since the placeholder is spliced
    /// AT it and moves nothing before it).
    at: usize,
}

impl Site<'_> {
    fn typed(&self) -> Typed<'_> {
        Typed::new(&self.typed_text, self.case)
    }
}

/// Read the caret as a member position, or answer `None` — which is nearly every caret.
///
/// The tree has to outlive the borrow, so this takes a closure rather than returning the site.
fn with_site<R>(
    source: &str,
    byte_offset: usize,
    case: MatchCase,
    f: impl FnOnce(&Site<'_>) -> R,
) -> Option<R> {
    let mut at = byte_offset.min(source.len());
    while at > 0 && !source.is_char_boundary(at) {
        at -= 1;
    }
    let (start, prefix) = bennu_query::prelude::split_completion_prefix(source, at);
    // `recv.getNa` is a member access — a call to something that may well exist elsewhere, and
    // never a member being declared here.
    if source[..start].trim_end().ends_with('.') {
        return None;
    }
    // Nothing at all on an empty prefix. A class body with three fields would open on six
    // generated members the moment the caret landed in it, ahead of everything real.
    if prefix.is_empty() {
        return None;
    }
    let sited = format!("{}{SITE_PLACEHOLDER}{}", &source[..at], &source[at..]);
    let tree = parse_java(&sited)?;
    let root = tree.root_node();
    let node = root.named_descendant_for_byte_range(at, (at + 1).min(sited.len()))?;
    let type_node = member_position_type(&node)?;
    let symbols = extract_symbols(&sited);
    let site = Site {
        typed_text: prefix,
        case,
        indent: line_indent(source, at),
        at,
        type_node,
        symbols,
        sited,
    };
    Some(f(&site))
}

/// The accessors the class around `byte_offset` is missing, matching what has been typed.
///
/// Empty — and cheaply so — for every caret that is not at a member position, which is nearly all
/// of them. Needs no resolver: the fields of the class being edited are written in the buffer,
/// with their types as written, and so are the methods that already exist.
pub fn accessor_completions(source: &str, byte_offset: usize, case: MatchCase) -> Vec<CompletionItem> {
    with_site(source, byte_offset, case, accessors_at).unwrap_or_default()
}

fn accessors_at(site: &Site<'_>) -> Vec<CompletionItem> {
    let Some(decl) = type_decl_at(&site.type_node, &site.symbols) else {
        return Vec::new();
    };
    let typed = site.typed();
    let indent = &site.indent;
    let mut out = Vec::new();
    for field in &decl.fields {
        // A field the language or a framework synthesized has no declaration to sit beside, and
        // writing an accessor for a Lombok field is writing the one Lombok already generated.
        if field.span.is_none() {
            continue;
        }
        let spec = FieldSpec {
            name: field.name.clone(),
            type_text: field.type_text.clone(),
            is_static: field.is_static,
            is_final: field.is_final,
            owner: decl.name.clone(),
        };
        for which in Accessor::ALL {
            if !offers(&spec, which) {
                continue;
            }
            let name = accessor_name(&spec, which);
            if !typed.matches(&name) || declares(decl, &name) {
                continue;
            }
            out.push(item(&spec, which, &name, indent));
        }
    }
    out
}

/// The methods this class **calls and does not declare**, matching what has been typed.
///
/// The name and the signature both come from the call site, which is a complete specification:
/// the arguments' declared types and names, and what the result is used as. It is the same reading
/// `create_method` makes for the Alt+Enter fix — one answer, offered in two places, so the popup
/// and the quick fix cannot describe the same member two ways.
pub fn missing_method_completions(
    source: &str,
    byte_offset: usize,
    case: MatchCase,
    resolver: &dyn TypeResolver,
) -> Vec<CompletionItem> {
    with_site(source, byte_offset, case, |site| missing_methods_at(site, resolver))
        .unwrap_or_default()
}

fn missing_methods_at(site: &Site<'_>, resolver: &dyn TypeResolver) -> Vec<CompletionItem> {
    let typed = site.typed();
    // What the class IS, so an inherited method can be told from a missing one.
    let owner = bennu_java::prelude::enclosing_type_binary(&site.sited, site.at);
    let cache = bennu_java::prelude::InferCache::new();
    let nl = if site.sited.contains("\r\n") { "\r\n" } else { "\n" };
    let mut out = Vec::new();
    for call in bennu_refactor::prelude::undeclared_calls(&site.sited, site.type_node) {
        if !typed.matches(&call.name) {
            continue;
        }
        // Inherited, or a hierarchy we could not read to the end — the same conservatism the
        // unknown-member check applies, and for the same reason: offering to write a method a
        // supertype already declares is offering to break the build.
        if let Some(owner) = owner.as_deref() {
            let found = cache.resolve_methods(resolver, owner, &call.name);
            if !found.candidates.is_empty() || !found.complete {
                continue;
            }
        }
        out.push(CompletionItem {
            label: call.name.clone(),
            kind: "generate".to_string(),
            detail: Some(call.detail()),
            insert_text: Some(call.render(&site.indent, nl)),
            ..Default::default()
        });
    }
    // And what OTHER classes ask this one for. `undeclared_calls` above reads this type's own
    // subtree, which is the whole story for a top-level class and half of it for a nested one:
    // `c.randomico()` is written in the outer class, on an instance of the inner, and standing
    // inside the inner there was nothing to offer — the one call that describes the method lives
    // outside the walk. The receiver's type is what connects them, and this is the layer that has
    // a resolver to ask.
    if let Some(owner) = owner.as_deref() {
        let mut seen: Vec<String> = out.iter().map(|i| i.label.clone()).collect();
        for call in calls_on(site, owner, resolver, &cache) {
            if !typed.matches(&call.name) || seen.contains(&call.name) {
                continue;
            }
            let found = cache.resolve_methods(resolver, owner, &call.name);
            if !found.candidates.is_empty() || !found.complete {
                continue;
            }
            seen.push(call.name.clone());
            out.push(CompletionItem {
                label: call.name.clone(),
                kind: "generate".to_string(),
                detail: Some(call.detail()),
                insert_text: Some(call.render(&site.indent, nl)),
                ..Default::default()
            });
        }
    }
    out
}

/// Every call written **outside** this type on a receiver whose type IS this type, one per name.
///
/// The walk is the whole file rather than the type's own subtree, minus that subtree — what is
/// inside it `undeclared_calls` has already read, and reading it twice would offer a private
/// `this.foo()` a second time as a public method.
///
/// Costs one type inference per call site with a receiver, memoized by the shared cache, and only
/// ever runs at a member position with something typed — which is the caret asking this exact
/// question and almost no other caret at all.
fn calls_on(
    site: &Site<'_>,
    owner: &str,
    resolver: &dyn TypeResolver,
    cache: &bennu_java::prelude::InferCache,
) -> Vec<bennu_refactor::prelude::ForeignCall> {
    let mut root = site.type_node;
    while let Some(parent) = root.parent() {
        root = parent;
    }
    let (mine_start, mine_end) = (site.type_node.start_byte(), site.type_node.end_byte());
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        let mut c = node.walk();
        for child in node.named_children(&mut c) {
            stack.push(child);
        }
        if node.kind() != "method_invocation" {
            continue;
        }
        if node.start_byte() >= mine_start && node.end_byte() <= mine_end {
            continue; // inside this type — `undeclared_calls` read it
        }
        let Some(receiver) = node.child_by_field_name("object") else { continue };
        let Some(ty) = bennu_java::prelude::infer_node_type_cached(
            &root,
            &site.sited,
            &site.symbols,
            &receiver,
            resolver,
            cache,
        ) else {
            continue;
        };
        if ty.binary_name != owner {
            continue;
        }
        let Some(name) = node.child_by_field_name("name") else { continue };
        if let Some(call) = bennu_refactor::prelude::foreign_call_at(
            root,
            &site.sited,
            name.start_byte(),
            name.end_byte(),
        ) {
            out.push(call);
        }
    }
    // Source order, so the first call site — the one whose arguments describe the method — wins a
    // tie the same way `undeclared_calls` lets it.
    out.reverse();
    out
}

/// The one accessor the caret is **certainly** writing — the ghost-text answer.
///
/// Ghost text is drawn inline, ahead of the caret, where it reads like text that is already
/// there: being wrong there costs trust, not a keystroke, so the bar is certainty and not
/// likelihood. Exactly one candidate clears it. Two do not, however close the second is.
///
/// The other half of the bar is **enough typed to have meant it**. One letter in a class with one
/// field matches uniquely and means nothing — the caret has barely arrived. Three is where a
/// name starts being a name.
pub fn generated_hint(
    source: &str,
    byte_offset: usize,
    case: MatchCase,
    resolver: Option<&dyn TypeResolver>,
) -> Option<AccessorHint> {
    /// Fewer typed characters than this and a unique match is an accident of arithmetic.
    const MIN_TYPED: usize = 3;

    let mut at = byte_offset.min(source.len());
    while at > 0 && !source.is_char_boundary(at) {
        at -= 1;
    }
    let (start, prefix) = bennu_query::prelude::split_completion_prefix(source, at);
    if prefix.chars().count() < MIN_TYPED {
        return None;
    }
    let mut candidates = generated_members(source, at, case, resolver);
    if candidates.len() != 1 {
        return None;
    }
    let item = candidates.pop()?;
    let insert = item.insert_text?;
    Some(AccessorHint {
        // Previewed as an arrow and the result, not as the result alone: accepting REPLACES what
        // has been typed, and a method drawn butting up against the `getNa` it is about to consume
        // reads as `getNapublic String getName()`.
        preview: format!(" → {insert}"),
        insert,
        replace_start: start,
        replace_end: at,
    })
}

/// What the editor draws ahead of the caret, and what accepting it writes.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AccessorHint {
    /// The greyed text. Shown, never necessarily written.
    pub preview: String,
    /// The member accepting writes.
    pub insert: String,
    /// The byte range accepting replaces — the half-written name.
    pub replace_start: usize,
    pub replace_end: usize,
}

/// One candidate. The inserted text is the whole member; the label is only its name, so the row
/// reads like every other method in the list.
fn item(spec: &FieldSpec, which: Accessor, name: &str, indent: &str) -> CompletionItem {
    let ty = spec.type_text.trim();
    let detail = match which {
        Accessor::Get => format!("() : {ty}"),
        Accessor::Set => format!("({ty}) : void"),
        Accessor::With => format!("({ty}) : {}", spec.owner),
    };
    CompletionItem {
        label: name.to_string(),
        // Its own kind, not `method`: this row does not name something that exists, and a list
        // that drew it identically to one that does would be claiming it did.
        kind: "generate".to_string(),
        detail: Some(detail),
        insert_text: Some(render_accessor(spec, which, indent)),
        ..Default::default()
    }
}

/// Whether `decl` already declares a method called `name`. Arity is not consulted: a class with a
/// `getName(Locale)` still has a `getName`, and offering to generate a second one beside it is
/// offering an overload nobody asked for.
fn declares(decl: &TypeDecl, name: &str) -> bool {
    decl.methods.iter().any(|m| m.name == name)
}

/// The type declaration the caret is at a **member position** of — directly in its body, rather
/// than inside a method, a field initializer, a lambda or an initializer block. `None` for every
/// other caret, which is nearly all of them.
///
/// Walked outwards, and the FIRST of the two sets to be reached decides. A caret in a half-written
/// member is inside an ERROR node — the ordinary state here, since `getNa` on its own is not a
/// declaration yet — and an ERROR's parent is the body it was written in, which is exactly the
/// answer. The declaration NODE is returned rather than a boolean because that is what
/// [`type_decl_at`] matches on: it keys the extracted model by the declaration's own start, so a
/// descendant of it finds nothing.
fn member_position_type<'t>(node: &Node<'t>) -> Option<Node<'t>> {
    let mut cur = Some(*node);
    let mut in_body = false;
    while let Some(n) = cur {
        match n.kind() {
            "class_body" | "enum_body" | "enum_body_declarations" | "interface_body"
            | "annotation_type_body" => in_body = true,
            "method_declaration"
            | "constructor_declaration"
            | "block"
            | "lambda_expression"
            | "static_initializer"
            | "field_declaration"
            | "variable_declarator" => return None,
            "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration"
            | "annotation_type_declaration"
                if in_body =>
            {
                return Some(n)
            }
            _ => {}
        }
        cur = n.parent();
    }
    None
}

/// The whitespace the caret's line starts with — the indentation the reader chose, which every
/// line of the generated member after the first is written at.
fn line_indent(source: &str, at: usize) -> String {
    let line_start = source[..at].rfind('\n').map_or(0, |i| i + 1);
    source[line_start..at]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect()
}
