//! Path templates — joining the three places a JAX-RS route is written, and reading the
//! `{name}` / `{name: regex}` variables out of the result.
//!
//! Pure string work on purpose: nothing here knows what a resource is, so every rule about how the
//! runtime reads a template is tested once, here, rather than rediscovered by each check.

/// One template variable inside a path, located in that path's own text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    pub name: String,
    /// The regex after the colon, trimmed. Empty when the template has none — which means the
    /// runtime's default, `[^/]+`.
    pub regex: String,
    /// Byte offset of the opening `{`.
    pub start: usize,
    /// Byte offset one past the closing `}`.
    pub end: usize,
    /// Byte offset of the name itself — where a go-to lands.
    pub name_start: usize,
}

/// Every template variable of `path`, in order.
///
/// `None` when the text is not a template this crate can read with certainty: an unclosed brace,
/// or a name outside JAX-RS's `\w[\w.-]*`. The difference matters to the caller — "this path has no
/// `{id}`" is only a finding when every variable it does have was understood.
pub fn templates(path: &str) -> Option<Vec<Template>> {
    let b = path.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'{' {
            i += 1;
            continue;
        }
        // Depth-counted, because a regex may carry its own braces: `{code: [A-Z]{3}}`.
        let mut depth = 1;
        let mut j = i + 1;
        while j < b.len() && depth > 0 {
            match b[j] {
                b'{' => depth += 1,
                b'}' => depth -= 1,
                _ => {}
            }
            j += 1;
        }
        if depth != 0 {
            return None;
        }
        let inner = &path[i + 1..j - 1];
        let (name_part, regex) = match inner.find(':') {
            Some(colon) => (&inner[..colon], inner[colon + 1..].trim()),
            None => (inner, ""),
        };
        let name = name_part.trim();
        if !is_template_name(name) {
            return None;
        }
        let lead = name_part.len() - name_part.trim_start().len();
        out.push(Template {
            name: name.to_string(),
            regex: regex.to_string(),
            start: i,
            end: j,
            name_start: i + 1 + lead,
        });
        i = j;
    }
    Some(out)
}

/// JAX-RS's template name grammar: a word character, then word characters, dots and dashes.
fn is_template_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else { return false };
    (first.is_alphanumeric() || first == '_')
        && chars.all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

/// Join path pieces into one route: exactly one slash between non-empty pieces, one in front, none
/// at the end.
///
/// The runtime ignores a leading slash on `@Path` and treats the application path, the class path
/// and the method path as consecutive segments — so `api` + `/orders/` + `{id}` and `/api/` +
/// `orders` + `/{id}` are the same route, and the panel should say so in one spelling.
pub fn join(pieces: &[&str]) -> String {
    let parts: Vec<&str> = pieces
        .iter()
        .map(|p| p.trim().trim_matches('/'))
        .filter(|p| !p.is_empty())
        .collect();
    format!("/{}", parts.join("/"))
}

/// A path reduced to what makes two routes the same route to the runtime: the leading slash
/// dropped and every variable's NAME erased, its regex kept.
///
/// `{id}` and `{orderId}` match exactly the same requests, so two methods differing only there are
/// ambiguous. `{id: \d+}` and `{id}` do not match the same requests, and `{id: \d+}` against
/// `{id: [0-9]+}` might — but proving two regexes equal is not something to guess at, so only equal
/// regex TEXT counts. A trailing slash is kept for the same reason: under-reporting is the price of
/// never being wrong.
///
/// `None` when the path is not a template this crate reads with certainty.
pub fn route_key(path: &str) -> Option<String> {
    let path = path.trim().trim_start_matches('/');
    let mut key = String::with_capacity(path.len());
    let mut cursor = 0;
    for t in templates(path)? {
        key.push_str(&path[cursor..t.start]);
        if t.regex.is_empty() {
            key.push_str("{}");
        } else {
            key.push_str("{:");
            key.push_str(&t.regex);
            key.push('}');
        }
        cursor = t.end;
    }
    key.push_str(&path[cursor..]);
    Some(key)
}

/// The 1-based line containing `offset`.
pub fn line_at(text: &str, offset: usize) -> u32 {
    let end = offset.min(text.len());
    text.as_bytes()[..end].iter().filter(|&&c| c == b'\n').count() as u32 + 1
}

/// `com.acme.OrderResource` → `OrderResource`.
pub fn simple_name(fqcn: &str) -> &str {
    fqcn.rsplit('.').next().unwrap_or(fqcn)
}

/// `ResponseEntity<List<Order>>` → `ResponseEntity`, `Order[]` → `Order`, `java.util.List` → `List`.
pub fn simple_type(type_text: &str) -> &str {
    let head = type_text.split('<').next().unwrap_or(type_text).trim();
    simple_name(head.trim_end_matches("[]").trim())
}

/// The build module a source belongs to — everything before its `src/main/` (or `src/`).
///
/// Two resources in two modules are two deployments far more often than one, and the runtime only
/// refuses routes that clash inside ONE application. Empty when the layout says nothing.
pub fn module_of(file: &str) -> &str {
    let cut = file.rfind("/src/main/").or_else(|| file.rfind("/src/"));
    cut.map(|i| &file[..i]).unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pieces_join_with_exactly_one_slash_whatever_was_written() {
        assert_eq!(join(&["api", "/orders/", "{id}"]), "/api/orders/{id}");
        assert_eq!(join(&["/api/", "orders", "/{id}"]), "/api/orders/{id}");
        assert_eq!(join(&["", "orders", ""]), "/orders");
        assert_eq!(join(&["", "", ""]), "/");
        assert_eq!(join(&["/", "/"]), "/");
    }

    #[test]
    fn templates_name_themselves_with_or_without_a_regex() {
        let t = templates("/orders/{id}/items/{ item : [0-9]+ }").unwrap();
        assert_eq!(t.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(), ["id", "item"]);
        assert_eq!(t[0].regex, "");
        assert_eq!(t[1].regex, "[0-9]+");
        let src = "/orders/{id}/items/{ item : [0-9]+ }";
        assert_eq!(&src[t[1].name_start..t[1].name_start + 4], "item");
        assert_eq!(&src[t[0].start..t[0].end], "{id}");
    }

    #[test]
    fn a_regex_may_carry_its_own_braces() {
        let t = templates("/codes/{code: [A-Z]{3}}/x").unwrap();
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].name, "code");
        assert_eq!(t[0].regex, "[A-Z]{3}");
    }

    #[test]
    fn a_template_that_cannot_be_read_is_not_read_at_all() {
        assert!(templates("/orders/{id").is_none(), "unclosed");
        assert!(templates("/orders/{}").is_none(), "no name");
        assert!(templates("/orders/{a b}").is_none(), "not a name");
        assert_eq!(templates("/static/path").unwrap(), Vec::new());
        assert_eq!(templates("/v1.0/{api-version}").unwrap()[0].name, "api-version");
    }

    #[test]
    fn a_route_key_erases_names_and_keeps_regexes() {
        assert_eq!(route_key("/orders/{id}"), route_key("orders/{orderId}"));
        assert_ne!(route_key("/orders/{id}"), route_key("/orders/{id: \\d+}"));
        assert_eq!(route_key("/o/{id: \\d+}"), route_key("/o/{x:\\d+}"));
        assert_ne!(route_key("/o/{id: \\d+}"), route_key("/o/{id: [0-9]+}"), "regex text, not meaning");
        assert_ne!(route_key("orders"), route_key("orders/"), "a trailing slash is kept");
        assert!(route_key("/o/{id").is_none());
    }

    #[test]
    fn names_reduce_to_what_a_person_reads() {
        assert_eq!(simple_type("Response"), "Response");
        assert_eq!(simple_type("java.util.List<Order>"), "List");
        assert_eq!(simple_type("Order[]"), "Order");
        assert_eq!(simple_name("com.acme.OrderResource"), "OrderResource");
    }

    #[test]
    fn a_module_is_what_precedes_its_source_root() {
        assert_eq!(module_of("/w/orders/src/main/java/a/B.java"), "/w/orders");
        assert_eq!(module_of("/w/src/a/B.java"), "/w");
        assert_eq!(module_of("/w/a/B.java"), "");
    }

    #[test]
    fn line_at_counts_from_one_and_clamps() {
        assert_eq!(line_at("a\nb", 0), 1);
        assert_eq!(line_at("a\nb", 2), 2);
        assert_eq!(line_at("a\nb", 99), 2);
    }
}
