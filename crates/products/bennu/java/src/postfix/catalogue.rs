//! The table of templates in relevance order, and the templates about statements and conditions.
//!
//! The loops live in `loops`, the `Optional` family in `optional`; what is here is everything that
//! wraps a value in one statement or one expression.

use super::body::Body;
use super::shape::ValueShape;
use super::{loops, optional};
use super::{Expansion, PostfixContext, Subject, Written};

type Build = fn(&Subject, &PostfixContext<'_>) -> Option<Body>;

struct Template {
    name: &'static str,
    build: Build,
}

/// Every template, most-used first — the provider keeps this order within the list's template band.
const TEMPLATES: &[Template] = &[
    Template { name: "var", build: declare_var },
    Template { name: "nn", build: if_not_null },
    Template { name: "notnull", build: if_not_null },
    Template { name: "null", build: if_null },
    Template { name: "if", build: if_true },
    Template { name: "else", build: if_false },
    Template { name: "for", build: loops::for_each_loop },
    Template { name: "iter", build: loops::for_each_loop },
    Template { name: "fori", build: loops::indexed },
    Template { name: "forr", build: loops::reversed },
    Template { name: "stream", build: loops::stream },
    Template { name: "forEach", build: loops::for_each_call },
    Template { name: "opt", build: optional::wrap },
    Template { name: "optof", build: optional::wrap_present },
    Template { name: "ifp", build: optional::if_present },
    Template { name: "ifpe", build: optional::if_present_or_else },
    Template { name: "ifget", build: optional::if_get },
    Template { name: "orThrow", build: optional::or_throw },
    Template { name: "orGet", build: optional::or_get },
    Template { name: "return", build: returned },
    Template { name: "sout", build: printed },
    Template { name: "soutv", build: printed_with_label },
    Template { name: "souf", build: printed_formatted },
    Template { name: "serr", build: printed_to_err },
    Template { name: "try", build: try_catch },
    Template { name: "twr", build: try_with_resources },
    Template { name: "inst", build: instance_of },
    Template { name: "instanceof", build: instance_of },
    Template { name: "cast", build: cast },
    Template { name: "castvar", build: cast_var },
    Template { name: "not", build: negated },
    Template { name: "par", build: parenthesized },
    Template { name: "arg", build: argument },
    Template { name: "lambda", build: lambda },
    Template { name: "reqnonnull", build: require_non_null },
    Template { name: "while", build: while_true },
    Template { name: "switch", build: switched },
    Template { name: "synchronized", build: synchronized_on },
    Template { name: "throw", build: thrown },
    Template { name: "assert", build: asserted },
    Template { name: "format", build: formatted },
    Template { name: "new", build: construct },
];

/// The templates `wanted` admits that apply to `subject`, expanded, in relevance order.
///
/// `wanted` is asked before a template is built, so the name filter costs nothing for the forty that
/// do not match what was typed.
pub fn expansions(subject: &Subject, ctx: &PostfixContext<'_>, wanted: &dyn Fn(&str) -> bool) -> Vec<Expansion> {
    TEMPLATES
        .iter()
        .filter(|template| wanted(template.name))
        .filter_map(|template| (template.build)(subject, ctx).map(|body| body.finish(template.name)))
        .collect()
}

/// The expression and its shape, for a template that needs a value — not a class name, not `void`.
pub(crate) fn value(subject: &Subject) -> Option<(&str, &ValueShape)> {
    match subject {
        Subject::Value { text, shape } if !shape.void => Some((text.as_str(), shape)),
        _ => None,
    }
}

/// [`value`], for a template that needs a reference: a primitive is never `null` and has no monitor.
pub(crate) fn reference(subject: &Subject) -> Option<(&str, &ValueShape)> {
    value(subject).filter(|(_, shape)| shape.primitive.is_none())
}

fn condition(subject: &Subject) -> Option<&str> {
    value(subject).filter(|(_, shape)| shape.boolean).map(|(text, _)| text)
}

/// The declaration keyword: `var` where the module has it, the written type where it does not.
pub(crate) fn declared(body: Body, ty: &Written, ctx: &PostfixContext<'_>) -> Body {
    match ctx.level >= 10 {
        true => body.push("var"),
        false => body.push(&ty.text).imports_of(ty),
    }
}

/// What [`declared`] writes, for a detail.
pub(crate) fn shown<'a>(ty: &'a Written, ctx: &PostfixContext<'_>) -> &'a str {
    match ctx.level >= 10 {
        true => "var",
        false => &ty.text,
    }
}

fn declare_var(s: &Subject, c: &PostfixContext<'_>) -> Option<Body> {
    let (e, v) = value(s)?;
    let body = declared(Body::new(format!("{} {} = …;", shown(&v.ty, c), v.name)), &v.ty, c);
    Some(body.push(" ").stop(&v.name, 1).push(" = ").push(e).push(";"))
}

fn if_not_null(s: &Subject, c: &PostfixContext<'_>) -> Option<Body> {
    let (e, v) = reference(s)?;
    (!v.non_null).then(|| Body::new("if (… != null) { … }").push("if (").push(e).push(" != null)").block(c.unit))
}

fn if_null(s: &Subject, c: &PostfixContext<'_>) -> Option<Body> {
    let (e, v) = reference(s)?;
    (!v.non_null).then(|| Body::new("if (… == null) { … }").push("if (").push(e).push(" == null)").block(c.unit))
}

fn if_true(s: &Subject, c: &PostfixContext<'_>) -> Option<Body> {
    let e = condition(s)?;
    Some(Body::new("if (…) { … }").push("if (").push(e).push(")").block(c.unit))
}

fn if_false(s: &Subject, c: &PostfixContext<'_>) -> Option<Body> {
    let e = condition(s)?;
    Some(Body::new("if (!…) { … }").push("if (!").push(e).push(")").block(c.unit))
}

fn while_true(s: &Subject, c: &PostfixContext<'_>) -> Option<Body> {
    let e = condition(s)?;
    Some(Body::new("while (…) { … }").push("while (").push(e).push(")").block(c.unit))
}

fn negated(s: &Subject, _: &PostfixContext<'_>) -> Option<Body> {
    let e = condition(s)?;
    Some(Body::new("!…").push("!").push(e))
}

fn asserted(s: &Subject, _: &PostfixContext<'_>) -> Option<Body> {
    let e = condition(s)?;
    Some(Body::new("assert …;").push("assert ").push(e).push(";"))
}

fn returned(s: &Subject, _: &PostfixContext<'_>) -> Option<Body> {
    let (e, _) = value(s)?;
    Some(Body::new("return …;").push("return ").push(e).push(";"))
}

fn printed(s: &Subject, _: &PostfixContext<'_>) -> Option<Body> {
    let (e, _) = value(s)?;
    Some(Body::new("System.out.println(…);").push("System.out.println(").push(e).push(");"))
}

fn printed_to_err(s: &Subject, _: &PostfixContext<'_>) -> Option<Body> {
    let (e, _) = value(s)?;
    Some(Body::new("System.err.println(…);").push("System.err.println(").push(e).push(");"))
}

/// `System.out.println("order.getTotal() = " + order.getTotal());` — the expression, labelled.
fn printed_with_label(s: &Subject, _: &PostfixContext<'_>) -> Option<Body> {
    let (e, _) = value(s)?;
    let label = e.replace('\\', "\\\\").replace('"', "\\\"");
    Some(
        Body::new("System.out.println(\"… = \" + …);")
            .push("System.out.println(\"")
            .push(&label)
            .push(" = \" + ")
            .push(e)
            .push(");"),
    )
}

fn printed_formatted(s: &Subject, _: &PostfixContext<'_>) -> Option<Body> {
    let (e, _) = value(s)?;
    Some(
        Body::new("System.out.printf(\"%s%n\", …);")
            .push("System.out.printf(\"")
            .stop("%s", 1)
            .push("%n\", ")
            .push(e)
            .push(");"),
    )
}

fn formatted(s: &Subject, _: &PostfixContext<'_>) -> Option<Body> {
    let (e, v) = value(s)?;
    v.string.then(|| Body::new("String.format(…, …)").push("String.format(").push(e).push(", ").caret().push(")"))
}

/// Only on a call: a bare name or a field is not a statement, and `try { order; }` does not compile.
fn try_catch(s: &Subject, c: &PostfixContext<'_>) -> Option<Body> {
    let Subject::Value { text, .. } = s else { return None };
    if !text.ends_with(')') {
        return None;
    }
    Some(
        Body::new("try { …; } catch (Exception ex) { … }")
            .push("try {\n")
            .push(c.unit)
            .push(text)
            .push(";\n} catch (")
            .stop("Exception", 1)
            .push(" ex) {\n")
            .push(c.unit)
            .caret()
            .push("\n}"),
    )
}

/// Try-with-resources arrived in Java 7.
fn try_with_resources(s: &Subject, c: &PostfixContext<'_>) -> Option<Body> {
    let (e, v) = reference(s)?;
    if !v.closeable || c.level < 7 {
        return None;
    }
    let detail = format!("try ({} {} = …) {{ … }}", shown(&v.ty, c), v.name);
    let body = declared(Body::new(detail).push("try ("), &v.ty, c);
    Some(body.push(" ").stop(&v.name, 1).push(" = ").push(e).push(")").block(c.unit))
}

/// A type pattern from Java 16; before it, the test and a cast — which names the value twice, so only
/// for an expression that does no work.
fn instance_of(s: &Subject, c: &PostfixContext<'_>) -> Option<Body> {
    let (e, v) = reference(s)?;
    if c.level >= 16 {
        let body = Body::new("if (… instanceof Type name) { … }")
            .push("if (")
            .push(e)
            .push(" instanceof ")
            .stop("Object", 1)
            .push(" ")
            .stop("value", 2)
            .push(")");
        return Some(body.block(c.unit));
    }
    v.repeatable.then(|| {
        Body::new("if (… instanceof Type) { Type name = (Type) …; }")
            .push("if (")
            .push(e)
            .push(" instanceof ")
            .stop("Object", 1)
            .push(") {\n")
            .push(c.unit)
            .stop("Object", 1)
            .push(" ")
            .stop("value", 2)
            .push(" = (")
            .stop("Object", 1)
            .push(") ")
            .push(e)
            .push(";\n")
            .push(c.unit)
            .caret()
            .push("\n}")
    })
}

fn cast(s: &Subject, _: &PostfixContext<'_>) -> Option<Body> {
    let (e, v) = value(s)?;
    let placeholder = if v.primitive.is_some() { "int" } else { "Object" };
    Some(Body::new("((Type) …)").push("((").stop(placeholder, 1).push(") ").push(e).push(")"))
}

fn cast_var(s: &Subject, _: &PostfixContext<'_>) -> Option<Body> {
    let (e, _) = reference(s)?;
    Some(
        Body::new("Type name = (Type) …;")
            .stop("Object", 1)
            .push(" ")
            .stop("value", 2)
            .push(" = (")
            .stop("Object", 1)
            .push(") ")
            .push(e)
            .push(";"),
    )
}

fn parenthesized(s: &Subject, _: &PostfixContext<'_>) -> Option<Body> {
    let (e, _) = value(s)?;
    Some(Body::new("(…)").push("(").push(e).push(")"))
}

/// The expression as the argument of a call whose name is typed where the caret lands.
fn argument(s: &Subject, _: &PostfixContext<'_>) -> Option<Body> {
    let (e, _) = value(s)?;
    Some(Body::new("call(…)").caret().push("(").push(e).push(")"))
}

fn lambda(s: &Subject, c: &PostfixContext<'_>) -> Option<Body> {
    let (e, _) = value(s)?;
    (c.level >= 8).then(|| Body::new("() -> …").push("() -> ").push(e))
}

/// `java.util.Objects` arrived in Java 7.
fn require_non_null(s: &Subject, c: &PostfixContext<'_>) -> Option<Body> {
    let (e, v) = reference(s)?;
    (c.level >= 7 && !v.non_null).then(|| {
        Body::new("Objects.requireNonNull(…)")
            .push("Objects.requireNonNull(")
            .push(e)
            .push(")")
            .import("java.util.Objects")
    })
}

/// A `switch` on a `String` needs Java 7.
fn switched(s: &Subject, c: &PostfixContext<'_>) -> Option<Body> {
    let (e, v) = value(s)?;
    (v.switchable && (!v.string || c.level >= 7))
        .then(|| Body::new("switch (…) { … }").push("switch (").push(e).push(")").block(c.unit))
}

fn synchronized_on(s: &Subject, c: &PostfixContext<'_>) -> Option<Body> {
    let (e, _) = reference(s)?;
    Some(Body::new("synchronized (…) { … }").push("synchronized (").push(e).push(")").block(c.unit))
}

fn thrown(s: &Subject, _: &PostfixContext<'_>) -> Option<Body> {
    let (e, v) = reference(s)?;
    v.throwable.then(|| Body::new("throw …;").push("throw ").push(e).push(";"))
}

fn construct(s: &Subject, _: &PostfixContext<'_>) -> Option<Body> {
    let Subject::Type { text } = s else { return None };
    Some(Body::new("new …()").push("new ").push(text).push("(").caret().push(")"))
}
