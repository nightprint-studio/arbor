//! The templates that make an `Optional`, and the ones that take one apart.
//!
//! Java 8's `Optional` has `ifPresent`, `orElseGet` and `orElseThrow(Supplier)`; `ifPresentOrElse`
//! arrived in 9. Below 9 the templates write what 8 has — an `if` over `isPresent()` — rather than
//! disappearing, and below 8 there is no `Optional` to write.

use super::body::Body;
use super::catalogue::{declared, reference, shown, value};
use super::shape::{OptionalShape, ValueShape};
use super::{PostfixContext, Subject};

/// The name a local holding an optional that had to be declared first is proposed as.
const HOLDER: &str = "found";

/// The expression, its shape and the optional it is — on a module that has `Optional` at all.
fn optional<'s>(subject: &'s Subject, ctx: &PostfixContext<'_>) -> Option<(&'s str, &'s ValueShape, &'s OptionalShape)> {
    if ctx.level < 8 {
        return None;
    }
    let (text, shape) = value(subject)?;
    shape.optional.as_ref().map(|optional| (text, shape, optional))
}

/// `Optional.ofNullable(…)` — `of` for a value that cannot be `null`, and the primitive optional for
/// an `int`, `long` or `double`, which cannot be `null` either.
pub(crate) fn wrap(s: &Subject, c: &PostfixContext<'_>) -> Option<Body> {
    let (e, v) = value(s)?;
    if c.level < 8 || v.optional.is_some() {
        return None;
    }
    let (class, factory) = match (v.primitive, v.non_null) {
        (Some("int"), _) => ("OptionalInt", "of"),
        (Some("long"), _) => ("OptionalLong", "of"),
        (Some("double"), _) => ("OptionalDouble", "of"),
        (Some(_), _) | (None, true) => ("Optional", "of"),
        (None, false) => ("Optional", "ofNullable"),
    };
    Some(
        Body::new(format!("{class}.{factory}(…)"))
            .push(class)
            .push(".")
            .push(factory)
            .push("(")
            .push(e)
            .push(")")
            .import(&format!("java.util.{class}")),
    )
}

/// `Optional.of(…)`, for a value you know is there. Not offered where `.opt` already writes `of`.
pub(crate) fn wrap_present(s: &Subject, c: &PostfixContext<'_>) -> Option<Body> {
    let (e, v) = reference(s)?;
    if c.level < 8 || v.optional.is_some() || v.non_null {
        return None;
    }
    Some(Body::new("Optional.of(…)").push("Optional.of(").push(e).push(")").import("java.util.Optional"))
}

pub(crate) fn if_present(s: &Subject, c: &PostfixContext<'_>) -> Option<Body> {
    let (e, _, o) = optional(s, c)?;
    let name = o.value_name();
    Some(
        Body::new(format!("….ifPresent({name} -> {{ … }});"))
            .push(e)
            .push(".ifPresent(")
            .stop(name, 1)
            .push(" -> {\n")
            .push(c.unit)
            .caret()
            .push("\n});"),
    )
}

/// `ifPresentOrElse` from Java 9; on 8, the same two branches as an `if`/`else`.
pub(crate) fn if_present_or_else(s: &Subject, c: &PostfixContext<'_>) -> Option<Body> {
    let (e, v, o) = optional(s, c)?;
    let name = o.value_name();
    if c.level >= 9 {
        return Some(
            Body::new(format!("….ifPresentOrElse({name} -> {{ … }}, () -> {{ … }});"))
                .push(e)
                .push(".ifPresentOrElse(")
                .stop(name, 1)
                .push(" -> {\n")
                .push(c.unit)
                .caret()
                .push("\n}, () -> {\n")
                .push(c.unit)
                .caret()
                .push("\n});"),
        );
    }
    let detail = format!("if (….isPresent()) {{ {} {name} = ….{}(); }} else {{ … }}", shown(&o.value_type(), c), o.getter());
    Some(unwrapped(Body::new(detail), e, v, o, c).push("\n} else {\n").push(c.unit).caret().push("\n}"))
}

/// The value taken out inside an `if` — the shape that reads the same at every level.
pub(crate) fn if_get(s: &Subject, c: &PostfixContext<'_>) -> Option<Body> {
    let (e, v, o) = optional(s, c)?;
    let detail = format!("if (….isPresent()) {{ {} {} = ….{}(); }}", shown(&o.value_type(), c), o.value_name(), o.getter());
    Some(unwrapped(Body::new(detail), e, v, o, c).push("\n}"))
}

pub(crate) fn or_throw(s: &Subject, c: &PostfixContext<'_>) -> Option<Body> {
    let (e, _, _) = optional(s, c)?;
    Some(
        Body::new("….orElseThrow(() -> new IllegalStateException(\"…\"))")
            .push(e)
            .push(".orElseThrow(() -> new ")
            .stop("IllegalStateException", 1)
            .push("(\"")
            .caret()
            .push("\"))"),
    )
}

pub(crate) fn or_get(s: &Subject, c: &PostfixContext<'_>) -> Option<Body> {
    let (e, _, _) = optional(s, c)?;
    Some(Body::new("….orElseGet(() -> …)").push(e).push(".orElseGet(() -> ").caret().push(")"))
}

/// `if (found.isPresent()) {`, the value taken out on the next line and the caret below it — with no
/// closing brace, which is the caller's.
///
/// The optional is named twice, so an expression that does work is declared into a local first:
/// `repo.findById(id).ifget` must still run the query once.
fn unwrapped(body: Body, e: &str, v: &ValueShape, o: &OptionalShape, c: &PostfixContext<'_>) -> Body {
    let hoisted = !v.repeatable;
    let holder = |body: Body| if hoisted { body.stop(HOLDER, 2) } else { body.push(e) };
    let body = match hoisted {
        true => declared(body, &o.holder(), c).push(" ").stop(HOLDER, 2).push(" = ").push(e).push(";\n"),
        false => body,
    };
    let body = holder(body.push("if (")).push(".isPresent()) {\n").push(c.unit);
    let body = declared(body, &o.value_type(), c).push(" ").stop(o.value_name(), 1).push(" = ");
    holder(body).push(".").push(o.getter()).push("();\n").push(c.unit).caret()
}
