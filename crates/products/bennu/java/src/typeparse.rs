//! A tiny recursive parser for Java type *text* into a simple-name type tree.
//!
//! Turns `Map<String, List<Foo>>` into `SimpleTypeRef { name: "Map", args: [String,
//! List<Foo>] }`. It does not resolve names to binary form — that's the caller's job
//! (imports + resolver). It strips array brackets and wildcards down to a best-effort
//! nominal core (Phase-1: no bounds/wildcard modelling).

/// A parsed type reference in *simple* (unresolved) name form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleTypeRef {
    /// The ELEMENT name, without brackets — what a member lookup has to ask about.
    pub name: String,
    pub args: Vec<SimpleTypeRef>,
    /// How many `[]` followed it. Kept beside the name rather than in it: every consumer that
    /// resolves members wants the element, and only a consumer that WRITES the type back into
    /// source wants the brackets. Losing them here is how an extracted `String[]` became `String`.
    pub dims: u8,
}

/// Parse a Java type text into a [`SimpleTypeRef`]. Returns `None` for empty /
/// unparseable / `void` / primitive text (primitives have no members to complete).
pub fn parse_type_text(text: &str) -> Option<SimpleTypeRef> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    // Strip trailing array brackets from the NAME — element member-access isn't Phase-1 (except
    // generics), so we complete against the raw type's members when possible — but count them, so
    // a caller writing the type back into source can put them on again.
    let base = text.split('[').next().unwrap_or(text).trim();
    let dims = text.matches('[').count().min(u8::MAX as usize) as u8;

    let (name_part, args_part) = split_generics(base);
    let name = name_part.trim();

    // Drop primitives / void — no member index.
    if matches!(
        name,
        "void" | "int" | "long" | "short" | "byte" | "char" | "boolean" | "float" | "double"
    ) {
        return None;
    }

    // Wildcard `?` — no nominal type.
    if name == "?" || name.is_empty() {
        return None;
    }

    // Keep the full dotted name (if any) — the resolver handles both dotted and
    // simple forms and decides how to bind it.
    let args = args_part.map(parse_arg_list).unwrap_or_default();
    Some(SimpleTypeRef {
        name: name.to_string(),
        args,
        dims,
    })
}

/// Split `Foo<...>` into (`Foo`, Some("...")) or (`Foo`, None).
fn split_generics(s: &str) -> (&str, Option<&str>) {
    if let Some(open) = s.find('<') {
        // The matching close is the last '>' (types are well-formed).
        if let Some(close) = s.rfind('>') {
            if close > open {
                return (&s[..open], Some(&s[open + 1..close]));
            }
        }
    }
    (s, None)
}

/// Parse a comma-separated generic argument list, respecting nested `<>`.
fn parse_arg_list(s: &str) -> Vec<SimpleTypeRef> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    let bytes = s.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'<' => depth += 1,
            b'>' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => {
                if let Some(t) = parse_arg(&s[start..i]) {
                    out.push(t);
                }
                start = i + 1;
            }
            _ => {}
        }
    }
    if let Some(t) = parse_arg(&s[start..]) {
        out.push(t);
    }
    out
}

/// Parse a single generic argument, resolving `? extends X` / `? super X` to `X` and
/// bare `?` to `Object`.
fn parse_arg(s: &str) -> Option<SimpleTypeRef> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Some(rest) = s.strip_prefix("? extends ") {
        return parse_type_text(rest.trim());
    }
    if let Some(rest) = s.strip_prefix("? super ") {
        return parse_type_text(rest.trim());
    }
    if s == "?" {
        return Some(SimpleTypeRef {
            name: "Object".to_string(),
            args: Vec::new(),
            dims: 0,
        });
    }
    parse_type_text(s)
}
