//! Pure Java-source scanning for a Struts **action class's bean properties** — the surface a JSP
//! form field / OGNL root binds to. Text-based (no parse), mirroring
//! [`crate::index_service`]'s `scan_setter_properties` but covering **getters** (`getX`/`isX`) as
//! well as **setters**, and able to locate an accessor's declaration for go-to.
//!
//! Two jobs, both best-effort + conservative (never a false "missing"):
//!   * [`bean_property_names`] — the set of property names the class exposes (the "known parameters"
//!     a JSP field / OGNL root is linted against);
//!   * [`find_property_member`] — the byte range of the accessor backing a property, so a JSP field /
//!     OGNL root can go to the action method it binds to.

use std::collections::BTreeSet;

use crate::index_service::bean_property_name;

/// Property names exposed by `source`'s bean accessors — the decapitalized suffix of every
/// `get<X>(` / `is<X>(` / `set<X>(`. A JSP form field / OGNL root matching NONE of these on the
/// resolved action class is likely a typo (the lint). Getters are included so a read-only OGNL
/// reference (`<s:property value="x"/>`) is recognised, not just form-bound setters.
pub fn bean_property_names(source: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for prefix in ["get", "set", "is"] {
        for (name, _) in accessors(source, prefix) {
            out.insert(name);
        }
    }
    out
}

/// The declaration **name** byte range of the accessor backing `prop` in `source` — the first of
/// `get<Prop>` / `is<Prop>` / `set<Prop>` (a getter first, the canonical read accessor). For go-to
/// from a JSP field / OGNL root to the action property. `None` when no accessor matches `prop`.
pub fn find_property_member(source: &str, prop: &str) -> Option<(usize, usize)> {
    for prefix in ["get", "is", "set"] {
        for (name, range) in accessors(source, prefix) {
            if name == prop {
                return Some(range);
            }
        }
    }
    None
}

/// A property's type + the accessor it was read from, for the JSP/OGNL hover card. Prefers the
/// **getter** (`get<Prop>`/`is<Prop>`) return type — the canonical read type an OGNL reference
/// resolves to — falling back to the **setter**'s parameter type for a write-only property. `None`
/// when no accessor backs `prop`. Best-effort text extraction (no parse), so a hover on an exotic
/// signature simply shows less, never wrong.
pub fn find_property_type(source: &str, prop: &str) -> Option<PropertyType> {
    // Getter first (the read type). `accessors` gives the accessor NAME range; the return type is the
    // text between the enclosing statement boundary and that name, minus modifiers/annotations.
    for prefix in ["get", "is"] {
        for (name, (start, _)) in accessors(source, prefix) {
            if name == prop {
                if let Some(ty) = getter_return_type(source, start) {
                    return Some(PropertyType { type_text: ty, accessor: format!("{prefix}…"), read: true });
                }
            }
        }
    }
    // Write-only property → the setter's parameter type.
    for (name, (_, end)) in accessors(source, "set") {
        if name == prop {
            if let Some(ty) = setter_param_type(source, end) {
                return Some(PropertyType { type_text: ty, accessor: "set…".to_string(), read: false });
            }
        }
    }
    None
}

/// The resolved type of a JSP-bound action property (for the hover card).
pub struct PropertyType {
    /// The written type text (`String`, `Long`, `List<Foo>`, …).
    pub type_text: String,
    /// Which accessor it came from (`get…`/`is…`/`set…`) — a hint for the card.
    pub accessor: String,
    /// `true` when read from a getter (the property is readable), `false` from a setter only.
    pub read: bool,
}

/// The return type of a getter whose NAME starts at `name_start` — the type text between the
/// enclosing statement boundary (the previous `;`/`{`/`}`) and the method name, with leading
/// modifiers and marker annotations stripped. `None` when nothing plausible remains.
fn getter_return_type(source: &str, name_start: usize) -> Option<String> {
    let pre = &source[..name_start];
    let boundary = pre.rfind([';', '{', '}']).map(|p| p + 1).unwrap_or(0);
    let decl = pre[boundary..].trim();
    let ty = strip_leading_modifiers(decl);
    let ty: String = ty.split_whitespace().collect::<Vec<_>>().join(" ");
    (!ty.is_empty()).then_some(ty)
}

/// The parameter type of a setter whose name ends at `name_end` — the first parameter of the `(…)`
/// that follows, minus a leading `final` and the parameter name. `None` when the parens / a type
/// can't be located.
fn setter_param_type(source: &str, name_end: usize) -> Option<String> {
    let rest = &source[name_end..];
    let open = rest.find('(')?;
    let close = rest[open + 1..].find(')')?;
    let inside = rest[open + 1..open + 1 + close].trim();
    if inside.is_empty() {
        return None;
    }
    // `[final] TYPE name` → drop a leading `final`, then everything before the last whitespace is the
    // type (`Map<String, Foo> m` → `Map<String, Foo>`); a lone token means the name was omitted.
    let inside = inside.strip_prefix("final ").unwrap_or(inside).trim();
    let ty = match inside.rsplit_once(char::is_whitespace) {
        Some((ty, _name)) => ty.trim(),
        None => inside,
    };
    let ty: String = ty.split_whitespace().collect::<Vec<_>>().join(" ");
    (!ty.is_empty()).then_some(ty)
}

/// Strip leading modifier keywords and marker annotations from a declaration prefix, leaving the
/// type text. (`@Override public Map<String, Foo>` → `Map<String, Foo>`.)
fn strip_leading_modifiers(decl: &str) -> &str {
    const MODS: &[&str] = &[
        "public", "protected", "private", "static", "final", "abstract", "synchronized", "native",
        "default", "strictfp",
    ];
    let mut rest = decl.trim_start();
    loop {
        let tok_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let tok = &rest[..tok_end];
        if tok.starts_with('@') || MODS.contains(&tok) {
            rest = rest[tok_end..].trim_start();
        } else {
            break;
        }
    }
    rest
}

/// A per-byte "is real code" mask for a Java `source`: `false` for every byte inside a `//` line
/// comment, a `/* … */` block comment, a `"…"` string, or a `'…'` char literal (escapes handled), so
/// the text scans below never match an accessor name that only appears in a comment / string. `true`
/// everywhere else. A single linear pass; ASCII delimiters, so byte indexing is safe.
fn code_mask(source: &str) -> Vec<bool> {
    let b = source.as_bytes();
    let n = b.len();
    let mut mask = vec![true; n];
    let mut i = 0;
    while i < n {
        match b[i] {
            b'/' if i + 1 < n && b[i + 1] == b'/' => {
                while i < n && b[i] != b'\n' {
                    mask[i] = false;
                    i += 1;
                }
            }
            b'/' if i + 1 < n && b[i + 1] == b'*' => {
                mask[i] = false;
                i += 1;
                while i < n && !(b[i] == b'*' && i + 1 < n && b[i + 1] == b'/') {
                    mask[i] = false;
                    i += 1;
                }
                // Mask the closing `*/` too (if present).
                for _ in 0..2 {
                    if i < n {
                        mask[i] = false;
                        i += 1;
                    }
                }
            }
            q @ (b'"' | b'\'') => {
                mask[i] = false;
                i += 1;
                while i < n && b[i] != q {
                    // A backslash escape consumes the next byte (so `"\""` doesn't end early).
                    if b[i] == b'\\' && i + 1 < n {
                        mask[i] = false;
                        mask[i + 1] = false;
                        i += 2;
                        continue;
                    }
                    mask[i] = false;
                    i += 1;
                }
                if i < n {
                    mask[i] = false; // the closing quote
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
    mask
}

/// Every `<prefix><Upper>…(` accessor in `source`, as `(bean_property_name, name_byte_range)`.
/// `prefix` is `get`/`set`/`is`. Mirrors the setter scan's rules: the prefix must START an
/// identifier (so `reset(`/`offset`/`island` never match), the char after the prefix must be
/// upper-case (`getURL` ok, `getaway` no), and a `(` must follow the name (a method, not a field).
///
/// Only real DECLARATIONS are matched: a match inside a `//`/`/* */` comment or a string literal is
/// skipped (a commented-out `helper.getIscrittoCCIAA()` used to be a go-to target — landing the
/// caret in a comment), and a match immediately preceded by `.` is a method CALL, not a declaration,
/// so it's skipped too.
fn accessors(source: &str, prefix: &str) -> Vec<(String, (usize, usize))> {
    let bytes = source.as_bytes();
    let code = code_mask(source);
    let mut out = Vec::new();
    for (i, _) in source.match_indices(prefix) {
        if !code[i] {
            continue; // inside a comment / string literal → not a declaration
        }
        if i > 0 {
            let p = bytes[i - 1];
            if p.is_ascii_alphanumeric() || p == b'_' || p == b'$' {
                continue; // the prefix is the tail of a longer identifier
            }
            if p == b'.' {
                continue; // `obj.getX()` is a method CALL, not a declaration
            }
        }
        let rest = &source[i + prefix.len()..];
        let mut it = rest.char_indices();
        let Some((_, first)) = it.next() else { continue };
        if !first.is_ascii_uppercase() {
            continue;
        }
        let mut end = first.len_utf8();
        for (off, c) in it {
            if c.is_alphanumeric() || c == '_' || c == '$' {
                end = off + c.len_utf8();
            } else {
                break;
            }
        }
        // A method: whitespace then `(`. A field (`getX`) or a bare word never matches.
        if !rest[end..].trim_start().starts_with('(') {
            continue;
        }
        let name = bean_property_name(&rest[..end]);
        // The accessor identifier's range (`getUser`), so go-to lands on the method name.
        out.push((name, (i, i + prefix.len() + end)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = r#"
        package com.acme;
        public class OrderAction {
            private String customer;
            public String getCustomer() { return customer; }
            public void setCustomer(String c) { this.customer = c; }
            public boolean isPaid() { return paid; }
            public void setTotalAmount(int t) {}
            public void reset() {}          // NOT a setter (no <Upper> after `set`)
        }
    "#;

    #[test]
    fn collects_get_set_is_properties() {
        let props = bean_property_names(SRC);
        assert!(props.contains("customer"), "{props:?}");
        assert!(props.contains("paid"), "{props:?}"); // isPaid → paid
        assert!(props.contains("totalAmount"), "{props:?}");
        // `reset()` is not `set<Upper>` → no property "reset"; but `getset()` → "set".
        assert!(!props.contains("reset"), "{props:?}");
    }

    #[test]
    fn finds_setter_for_write_only_property() {
        // `totalAmount` has only a setter — go-to still resolves to `setTotalAmount`.
        let (start, end) = find_property_member(SRC, "totalAmount").expect("member");
        assert_eq!(&SRC[start..end], "setTotalAmount");
    }

    #[test]
    fn prefers_getter_when_both_exist() {
        let (start, end) = find_property_member(SRC, "customer").expect("member");
        assert_eq!(&SRC[start..end], "getCustomer");
    }

    #[test]
    fn absent_property_resolves_to_none() {
        assert!(find_property_member(SRC, "nope").is_none());
        assert!(!bean_property_names(SRC).contains("nope"));
    }

    #[test]
    fn is_getter_range_is_the_method_name() {
        let (start, end) = find_property_member(SRC, "paid").expect("member");
        assert_eq!(&SRC[start..end], "isPaid");
    }

    #[test]
    fn property_type_from_getter_return() {
        let t = find_property_type(SRC, "customer").expect("type");
        assert_eq!(t.type_text, "String");
        assert!(t.read, "read from a getter");
        // `isPaid()` → boolean.
        assert_eq!(find_property_type(SRC, "paid").unwrap().type_text, "boolean");
    }

    #[test]
    fn property_type_from_setter_param_for_write_only() {
        // `totalAmount` has only `setTotalAmount(int t)` → the param type is the property type.
        let t = find_property_type(SRC, "totalAmount").expect("type");
        assert_eq!(t.type_text, "int");
        assert!(!t.read, "write-only → from the setter");
    }

    #[test]
    fn property_type_handles_generics_and_modifiers() {
        let src = r#"
            public class C {
                @Override public final java.util.List<com.acme.Item> getItems() { return null; }
                public void setMap(java.util.Map<String, Long> m) {}
            }
        "#;
        // Getter return type keeps generics, drops the annotation + modifiers.
        assert_eq!(find_property_type(src, "items").unwrap().type_text, "java.util.List<com.acme.Item>");
        // Setter param type keeps generics with the interior space, drops the param name.
        assert_eq!(find_property_type(src, "map").unwrap().type_text, "java.util.Map<String, Long>");
    }

    #[test]
    fn absent_property_has_no_type() {
        assert!(find_property_type(SRC, "nope").is_none());
    }

    #[test]
    fn accessor_in_comment_or_call_is_not_a_declaration() {
        // The REAL declaration is `getIscrittoCCIAA` on line 5; the commented-out line and the method
        // CALL on `helper` must NOT be matched (the go-to used to land in the comment).
        let src = r#"
            public class Foo {
                // if (x && StringUtils.isEmpty(helper.getIscrittoCCIAA())) {
                private String iscrittoCCIAA;
                public String getIscrittoCCIAA() { return iscrittoCCIAA; }
                void use() { String s = helper.getIscrittoCCIAA(); }
            }
        "#;
        let (start, end) = find_property_member(src, "iscrittoCCIAA").expect("real getter");
        assert_eq!(&src[start..end], "getIscrittoCCIAA");
        // It resolves to the DECLARATION (has a return type + `{` body), not the commented call.
        let after = &src[end..];
        assert!(after.trim_start().starts_with('('), "landed on the declaration name");
        // The matched offset is on line 5 (the declaration), not line 3 (the comment).
        let line = src[..start].matches('\n').count() + 1;
        assert_eq!(line, 5, "must be the declaration line, not the comment");

        // A property that appears ONLY in a comment / call is not a real property.
        let only_commented = r#"
            public class Bar {
                // public String getGhost() { return null; }
                void m() { obj.getPhantom(); }
            }
        "#;
        assert!(find_property_member(only_commented, "ghost").is_none(), "commented decl ignored");
        assert!(find_property_member(only_commented, "phantom").is_none(), "method call ignored");
        assert!(!bean_property_names(only_commented).contains("ghost"));
    }
}
