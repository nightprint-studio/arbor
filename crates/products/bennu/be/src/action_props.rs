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

/// Every `<prefix><Upper>…(` accessor in `source`, as `(bean_property_name, name_byte_range)`.
/// `prefix` is `get`/`set`/`is`. Mirrors the setter scan's rules: the prefix must START an
/// identifier (so `reset(`/`offset`/`island` never match), the char after the prefix must be
/// upper-case (`getURL` ok, `getaway` no), and a `(` must follow the name (a method, not a field).
fn accessors(source: &str, prefix: &str) -> Vec<(String, (usize, usize))> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for (i, _) in source.match_indices(prefix) {
        if i > 0 {
            let p = bytes[i - 1];
            if p.is_ascii_alphanumeric() || p == b'_' || p == b'$' {
                continue; // the prefix is the tail of a longer identifier
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
}
