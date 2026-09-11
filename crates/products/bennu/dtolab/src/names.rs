//! The spellings a template needs and has no way to produce itself: a constraint name as a method
//! name, a field name in another case, and any text as a Java string literal.
//!
//! Exposed to templates as filters (`snake`, `camel`, `pascal`, `java_string`) — see
//! [`crate::template::render`].

/// `NotBlank` → `not_blank`, `customerName` → `customer_name`, `URLValue` → `url_value`.
pub fn snake(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if !c.is_alphanumeric() {
            if !out.is_empty() && !out.ends_with('_') {
                out.push('_');
            }
            continue;
        }
        if c.is_uppercase() {
            let prev = i.checked_sub(1).map(|p| chars[p]);
            let next = chars.get(i + 1);
            // A word starts at a capital after a lowercase letter or a digit (`nameA`), or at the
            // last capital of a run that a lowercase letter follows (`URLValue` → `url_value`).
            let starts_word = prev.is_some_and(|p| p.is_lowercase() || p.is_ascii_digit())
                || (prev.is_some_and(char::is_uppercase) && next.is_some_and(|n| n.is_lowercase()));
            if starts_word && !out.is_empty() && !out.ends_with('_') {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out.trim_end_matches('_').to_string()
}

/// `customer_name` / `CustomerName` → `customerName`.
pub fn camel(text: &str) -> String {
    let pascal = pascal(text);
    let mut chars = pascal.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// `customer_name` / `customerName` → `CustomerName`.
pub fn pascal(text: &str) -> String {
    snake(text)
        .split('_')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                None => String::new(),
            }
        })
        .collect()
}

/// Any text as a Java string literal, quotes included.
pub fn java_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_are_found_in_every_spelling_a_java_name_comes_in() {
        assert_eq!(snake("NotBlank"), "not_blank");
        assert_eq!(snake("customerName"), "customer_name");
        assert_eq!(snake("URLValue"), "url_value");
        assert_eq!(snake("line2Total"), "line2_total");
        assert_eq!(camel("customer_name"), "customerName");
        assert_eq!(pascal("customerName"), "CustomerName");
    }

    #[test]
    fn a_java_string_survives_quotes_and_backslashes() {
        assert_eq!(java_string(r#"a "b" \c"#), r#""a \"b\" \\c""#);
        assert_eq!(java_string("line\nbreak"), "\"line\\nbreak\"");
    }
}
