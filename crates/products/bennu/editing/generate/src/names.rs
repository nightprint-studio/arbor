//! Identifier and type-text helpers shared by the generators.

/// `order_line` / `order-line` / `orderLine` → `OrderLine`.
pub fn pascal(text: &str) -> String {
    words(text).iter().map(|w| capitalize(w)).collect()
}

/// `OrderLine` / `order_line` → `orderLine`.
pub fn camel(text: &str) -> String {
    decapitalize(&pascal(text))
}

/// `OrderLine` → `order_line`.
pub fn snake(text: &str) -> String {
    words(text).iter().map(|w| w.to_lowercase()).collect::<Vec<_>>().join("_")
}

/// `OrderLine` → `order-line`.
pub fn kebab(text: &str) -> String {
    snake(text).replace('_', "-")
}

/// `OrderLine` → `ORDER_LINE`.
pub fn upper_snake(text: &str) -> String {
    snake(text).to_uppercase()
}

pub fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

pub fn decapitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Splits on `_`, `-`, spaces and lower→upper boundaries; an acronym run stays one word (`HTTPClient` → `HTTP`, `Client`).
fn words(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if !c.is_alphanumeric() {
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
            continue;
        }
        let boundary = c.is_uppercase()
            && !current.is_empty()
            && (chars[i - 1].is_lowercase()
                || chars[i - 1].is_ascii_digit()
                || chars.get(i + 1).is_some_and(|n| n.is_lowercase()) && chars[i - 1].is_uppercase());
        if boundary {
            out.push(std::mem::take(&mut current));
        }
        current.push(c);
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// `List<Order>` → `List`, `Map<String, List<X>>` → `Map`, `a.b.Foo` → `a.b.Foo`.
pub fn strip_generics(type_text: &str) -> &str {
    type_text.split('<').next().unwrap_or(type_text).trim()
}

/// `java.util.List` → `List`; `Outer.Inner` → `Inner`.
pub fn simple_name(type_text: &str) -> &str {
    strip_generics(type_text).rsplit('.').next().unwrap_or(type_text).trim()
}

/// The top-level generic arguments: `Map<String, List<X>>` → `["String", "List<X>"]`.
pub fn generic_args(type_text: &str) -> Vec<String> {
    let Some(open) = type_text.find('<') else { return Vec::new() };
    let Some(close) = type_text.rfind('>') else { return Vec::new() };
    if close <= open {
        return Vec::new();
    }
    let inner = &type_text[open + 1..close];
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut current = String::new();
    for c in inner.chars() {
        match c {
            '<' => {
                depth += 1;
                current.push(c);
            }
            '>' => {
                depth = depth.saturating_sub(1);
                current.push(c);
            }
            ',' if depth == 0 => out.push(std::mem::take(&mut current).trim().to_string()),
            _ => current.push(c),
        }
    }
    if !current.trim().is_empty() {
        out.push(current.trim().to_string());
    }
    out
}

/// The element of a collection / optional / response wrapper: `ResponseEntity<List<Order>>` → `Order`.
/// Anything else is returned as it is.
pub fn payload_type(type_text: &str) -> String {
    const WRAPPERS: [&str; 12] = [
        "ResponseEntity", "List", "Set", "Collection", "Iterable", "Page", "Slice", "Optional", "Mono", "Flux", "Callable", "CompletableFuture",
    ];
    let mut current = type_text.trim().to_string();
    for _ in 0..4 {
        let name = simple_name(&current).to_string();
        if WRAPPERS.contains(&name.as_str()) {
            match generic_args(&current).into_iter().next() {
                Some(inner) => current = inner,
                None => break,
            }
        } else {
            break;
        }
    }
    current
}

pub fn is_collection(type_text: &str) -> bool {
    matches!(simple_name(type_text), "List" | "Set" | "Collection" | "Iterable") || type_text.trim().ends_with("[]")
}

/// `int` → `Integer`, `Order` → `Order`.
pub fn boxed(type_text: &str) -> String {
    match type_text.trim() {
        "int" => "Integer",
        "long" => "Long",
        "double" => "Double",
        "float" => "Float",
        "boolean" => "Boolean",
        "char" => "Character",
        "byte" => "Byte",
        "short" => "Short",
        other => other,
    }
    .to_string()
}

pub fn is_primitive(type_text: &str) -> bool {
    matches!(type_text.trim(), "int" | "long" | "double" | "float" | "boolean" | "char" | "byte" | "short")
}

/// Whether `text` is a Java identifier (keywords are not checked).
pub fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars.next().is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$') && chars.all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}

/// A Java string literal for `text`.
pub fn java_string(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `/api/orders/{id}` → `["api", "orders", "{id}"]`.
pub fn path_segments(path: &str) -> Vec<&str> {
    path.split('/').filter(|s| !s.is_empty()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_conversions_keep_acronyms_together() {
        assert_eq!(pascal("order_line"), "OrderLine");
        assert_eq!(camel("OrderLine"), "orderLine");
        assert_eq!(snake("HTTPClient"), "http_client");
        assert_eq!(kebab("OrderLine"), "order-line");
        assert_eq!(upper_snake("orderLine"), "ORDER_LINE");
    }

    #[test]
    fn generics_split_at_the_top_level_only() {
        assert_eq!(generic_args("Map<String, List<X>>"), vec!["String", "List<X>"]);
        assert_eq!(strip_generics("List<Order>"), "List");
        assert_eq!(simple_name("java.util.List<Order>"), "List");
    }

    #[test]
    fn payload_unwraps_wrappers_and_collections() {
        assert_eq!(payload_type("ResponseEntity<List<Order>>"), "Order");
        assert_eq!(payload_type("Mono<Order>"), "Order");
        assert_eq!(payload_type("Order"), "Order");
        assert_eq!(payload_type("void"), "void");
    }

    #[test]
    fn string_literals_are_escaped() {
        assert_eq!(java_string("a\"b\\c\n"), "\"a\\\"b\\\\c\\n\"");
    }

    #[test]
    fn identifiers() {
        assert!(is_identifier("OrderLine2"));
        assert!(!is_identifier("2x"));
        assert!(!is_identifier("a b"));
    }
}
