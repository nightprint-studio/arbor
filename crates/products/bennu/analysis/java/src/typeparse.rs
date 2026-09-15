//! A tiny recursive parser for Java type *text* into a simple-name type tree.
//!
//! Turns `Map<String, List<Foo>>` into `SimpleTypeRef { name: "Map", args: [String,
//! List<Foo>] }`. It does not resolve names to binary form — that's the caller's job
//! (imports + resolver). It strips array brackets and collapses wildcards onto their bound —
//! which is the nominal core every member lookup needs — while recording that it did, so a
//! consumer WRITING the type back into source can decline. See [`SimpleTypeRef::wildcard`].

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
    /// Whether this node is a `?`, `? extends X` or `? super X` collapsed onto its bound.
    ///
    /// Same reason as [`Self::dims`], and the same shape of bug: the collapse is what a member
    /// lookup needs, and it is exactly wrong for the one consumer that writes the type back out.
    /// `Class<? extends Annotation>` is not `Class<Annotation>` at a declaration.
    pub wildcard: bool,
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
        wildcard: false,
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
        return parse_type_text(rest.trim()).map(mark_wildcard);
    }
    if let Some(rest) = s.strip_prefix("? super ") {
        return parse_type_text(rest.trim()).map(mark_wildcard);
    }
    if s == "?" {
        return Some(SimpleTypeRef {
            name: "Object".to_string(),
            args: Vec::new(),
            dims: 0,
                wildcard: true,
        });
    }
    parse_type_text(s)
}

/// The bound of a wildcard, marked as standing in for one.
fn mark_wildcard(mut t: SimpleTypeRef) -> SimpleTypeRef {
    t.wildcard = true;
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A wildcard argument keeps its bound — which is what a member lookup asks for — and says it
    /// is one, which is what a declaration has to know before writing it down.
    #[test]
    fn a_wildcard_argument_keeps_its_bound_and_is_marked() {
        let t = parse_type_text("Class<? extends Annotation>").unwrap();
        assert_eq!(t.name, "Class");
        assert!(!t.wildcard);
        assert_eq!(t.args[0].name, "Annotation");
        assert!(t.args[0].wildcard);

        let bare = parse_type_text("Class<?>").unwrap();
        assert_eq!(bare.args[0].name, "Object");
        assert!(bare.args[0].wildcard);

        let sup = parse_type_text("Consumer<? super Order>").unwrap();
        assert_eq!(sup.args[0].name, "Order");
        assert!(sup.args[0].wildcard);

        // An ordinary argument is not a wildcard and stays writable.
        let plain = parse_type_text("List<String>").unwrap();
        assert!(!plain.args[0].wildcard);
    }
}
