//! The templates that walk a value — `for`, `fori`, `forr`, `forEach` — and `stream`, which is how
//! Java 8 walks one.

use super::body::Body;
use super::catalogue::{declared, shown, value};
use super::shape::{Element, Length, OptionalShape};
use super::{PostfixContext, Subject};

/// The expression and what a for-each over it declares: an array's element or an `Iterable`'s.
fn walked(subject: &Subject) -> Option<(&str, &Element)> {
    let (text, shape) = value(subject)?;
    shape.array_element.as_ref().or(shape.iterable_element.as_ref()).map(|element| (text, element))
}

pub(crate) fn for_each_loop(s: &Subject, c: &PostfixContext<'_>) -> Option<Body> {
    let (e, element) = walked(s)?;
    let detail = format!("for ({} {} : …) {{ … }}", shown(&element.ty, c), element.name);
    let body = declared(Body::new(detail).push("for ("), &element.ty, c);
    Some(body.push(" ").stop(&element.name, 1).push(" : ").push(e).push(")").block(c.unit))
}

/// How far an indexed loop over `e` counts.
fn bound(e: &str, length: Length) -> String {
    match length {
        Length::Field => format!("{e}.length"),
        Length::Size => format!("{e}.size()"),
        Length::Chars => format!("{e}.length()"),
        Length::Itself => e.to_string(),
    }
}

/// The bound is evaluated on every iteration, so an expression that does work cannot be it —
/// `repo.findAll().fori` would run the query once per row.
pub(crate) fn indexed(s: &Subject, c: &PostfixContext<'_>) -> Option<Body> {
    let (e, v) = value(s)?;
    let length = v.length?;
    if !v.repeatable {
        return None;
    }
    let counter = v.counter.unwrap_or("int");
    let detail = format!("for ({counter} i = 0; i < {}; i++) {{ … }}", bound("…", length));
    Some(
        Body::new(detail)
            .push("for (")
            .push(counter)
            .push(" ")
            .stop("i", 1)
            .push(" = 0; ")
            .stop("i", 1)
            .push(" < ")
            .push(&bound(e, length))
            .push("; ")
            .stop("i", 1)
            .push("++)")
            .block(c.unit),
    )
}

/// Counting down evaluates the bound once, so any expression will do.
pub(crate) fn reversed(s: &Subject, c: &PostfixContext<'_>) -> Option<Body> {
    let (e, v) = value(s)?;
    let length = v.length?;
    let counter = v.counter.unwrap_or("int");
    let detail = format!("for ({counter} i = {} - 1; i >= 0; i--) {{ … }}", bound("…", length));
    Some(
        Body::new(detail)
            .push("for (")
            .push(counter)
            .push(" ")
            .stop("i", 1)
            .push(" = ")
            .push(&bound(e, length))
            .push(" - 1; ")
            .stop("i", 1)
            .push(" >= 0; ")
            .stop("i", 1)
            .push("--)")
            .block(c.unit),
    )
}

/// A stream of the value, where writing one takes more than calling `stream()`.
///
/// A collection is not offered this: `stream()` is its member, right above in the same list, and a
/// template writing the same call would be the same row twice. Neither is an `Optional` from Java 9.
pub(crate) fn stream(s: &Subject, c: &PostfixContext<'_>) -> Option<Body> {
    let (e, v) = value(s)?;
    if c.level < 8 {
        return None;
    }
    if let Some(element) = &v.array_element {
        // `Arrays.stream` is overloaded for object arrays and `int[]`, `long[]`, `double[]` only.
        if element.primitive.is_some_and(|p| !matches!(p, "int" | "long" | "double")) {
            return None;
        }
        return Some(Body::new("Arrays.stream(…)").push("Arrays.stream(").push(e).push(")").import("java.util.Arrays"));
    }
    if let Some(optional) = &v.optional {
        return optional_stream(e, v.repeatable, optional, c);
    }
    if v.map {
        return Some(Body::new("….entrySet().stream()").push(e).push(".entrySet().stream()"));
    }
    if v.iterable_element.is_some() && !v.collection {
        return Some(
            Body::new("StreamSupport.stream(….spliterator(), false)")
                .push("StreamSupport.stream(")
                .push(e)
                .push(".spliterator(), false)")
                .import("java.util.stream.StreamSupport"),
        );
    }
    None
}

/// Java 8 has no `Optional.stream()`; what it has is `map` and the stream factories.
fn optional_stream(e: &str, repeatable: bool, optional: &OptionalShape, c: &PostfixContext<'_>) -> Option<Body> {
    if c.level >= 9 {
        return None;
    }
    let Some(stream) = optional.primitive_stream() else {
        return Some(
            Body::new("….map(Stream::of).orElseGet(Stream::empty)")
                .push(e)
                .push(".map(Stream::of).orElseGet(Stream::empty)")
                .import("java.util.stream.Stream"),
        );
    };
    // A primitive optional has no `map`: the value is tested and taken, which names it twice.
    if !repeatable {
        return None;
    }
    Some(
        Body::new(format!("….isPresent() ? {stream}.of(…) : {stream}.empty()"))
            .push(e)
            .push(".isPresent() ? ")
            .push(&stream)
            .push(".of(")
            .push(e)
            .push(".")
            .push(optional.getter())
            .push("()) : ")
            .push(&stream)
            .push(".empty()")
            .import(&format!("java.util.stream.{stream}")),
    )
}

/// `forEach` with a lambda to fill in — `(key, value)` for a map. Java 8.
pub(crate) fn for_each_call(s: &Subject, c: &PostfixContext<'_>) -> Option<Body> {
    let (e, v) = value(s)?;
    if c.level < 8 {
        return None;
    }
    if v.map {
        return Some(
            Body::new("….forEach((key, value) -> { … });")
                .push(e)
                .push(".forEach((")
                .stop("key", 1)
                .push(", ")
                .stop("value", 2)
                .push(") -> {\n")
                .push(c.unit)
                .caret()
                .push("\n});"),
        );
    }
    let element = v.iterable_element.as_ref()?;
    Some(
        Body::new(format!("….forEach({} -> {{ … }});", element.name))
            .push(e)
            .push(".forEach(")
            .stop(&element.name, 1)
            .push(" -> {\n")
            .push(c.unit)
            .caret()
            .push("\n});"),
    )
}
