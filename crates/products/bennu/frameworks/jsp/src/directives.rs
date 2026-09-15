//! The `<%@ taglib %>` directives of a page — what its prefixes are bound to.
//!
//! Small, linear, tolerant: a JSP is not XML and never parses as it (a scriptlet holds
//! Java, an attribute value holds another tag), so this walks the text looking only for
//! `<%@ … %>` blocks and reads the attributes out of the ones that say `taglib`.
//!
//! Every part carries its own span, and that is the whole point of not reusing the
//! scanner in `bennu-web` that already finds these: that one keeps the *prefix* span,
//! which is what renaming a prefix needs, and this one also keeps the **uri** span,
//! which is what Ctrl+click on a `uri="…"` needs in order to have something to be on.

/// One `<%@ taglib prefix="s" uri="/struts-tags" %>` directive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaglibDirective {
    pub prefix: String,
    pub uri: String,
    /// Span of the prefix value, inside the quotes.
    pub prefix_span: (usize, usize),
    /// Span of the uri value, inside the quotes. `(0, 0)` when the directive declares none.
    pub uri_span: (usize, usize),
    /// Span of the whole directive.
    pub span: (usize, usize),
}

/// Every taglib directive in the page, in source order.
pub fn taglib_directives(source: &str) -> Vec<TaglibDirective> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(rel) = source[i..].find("<%@") {
        let start = i + rel;
        let end = match source[start..].find("%>") {
            Some(e) => start + e + 2,
            // An unterminated directive: the page is mid-edit. Stop rather than reading the
            // rest of the file as one directive.
            None => break,
        };
        i = end;
        let body = &source[start + 3..end - 2];
        // The directive name is the first word. Anything but `taglib` (`page`, `include`)
        // declares no prefix and is not ours.
        if body.trim_start().starts_with("taglib") {
            let base = start + 3;
            let prefix = attr_span(body, "prefix", base);
            let uri = attr_span(body, "uri", base);
            if let Some((p, pspan)) = prefix {
                out.push(TaglibDirective {
                    prefix: p,
                    uri: uri.as_ref().map(|(u, _)| u.clone()).unwrap_or_default(),
                    prefix_span: pspan,
                    uri_span: uri.map(|(_, s)| s).unwrap_or((0, 0)),
                    span: (start, end),
                });
            }
        }
    }
    out
}

/// The value of `name="…"` (or `'…'`) inside a directive body, with its span in the file.
fn attr_span(body: &str, name: &str, base: usize) -> Option<(String, (usize, usize))> {
    let mut from = 0;
    while let Some(rel) = body[from..].find(name) {
        let at = from + rel;
        from = at + name.len();
        // A real attribute name is preceded by whitespace (or starts the body) and followed by
        // `=`, so `uri` does not match inside `myuri` and `prefix` does not match a value.
        let before_ok = at == 0 || body.as_bytes()[at - 1].is_ascii_whitespace();
        if !before_ok {
            continue;
        }
        let rest = body[from..].trim_start();
        let skipped = body[from..].len() - rest.len();
        let Some(rest) = rest.strip_prefix('=') else { continue };
        let value = rest.trim_start();
        let quote_at = from + skipped + 1 + (rest.len() - value.len());
        let quote = value.as_bytes().first().copied()?;
        if quote != b'"' && quote != b'\'' {
            continue;
        }
        let inner = &value[1..];
        let close = inner.find(quote as char)?;
        let start = base + quote_at + 1;
        return Some((inner[..close].to_string(), (start, start + close)));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = concat!(
        "<%@ page contentType=\"text/html\"%>\n",
        "<%@ taglib prefix=\"s\" uri=\"/struts-tags\"%>\n",
        "<%@ taglib uri='aps-core.tld' prefix='wp' %>\n",
        "<s:if test=\"%{x}\">hi</s:if>\n",
    );

    #[test]
    fn only_taglib_directives_are_collected_and_both_attribute_orders_read() {
        let d = taglib_directives(PAGE);
        assert_eq!(d.len(), 2, "the `page` directive declares no prefix");
        assert_eq!(d[0].prefix, "s");
        assert_eq!(d[0].uri, "/struts-tags");
        assert_eq!(d[1].prefix, "wp");
        assert_eq!(d[1].uri, "aps-core.tld");
    }

    #[test]
    fn every_part_points_at_itself_in_the_source() {
        let d = taglib_directives(PAGE);
        assert_eq!(&PAGE[d[0].prefix_span.0..d[0].prefix_span.1], "s");
        assert_eq!(&PAGE[d[0].uri_span.0..d[0].uri_span.1], "/struts-tags");
        // Single quotes are as valid as double ones, and legacy pages use both.
        assert_eq!(&PAGE[d[1].uri_span.0..d[1].uri_span.1], "aps-core.tld");
        assert!(PAGE[d[1].span.0..d[1].span.1].starts_with("<%@ taglib"));
    }

    #[test]
    fn an_unterminated_directive_stops_the_scan_rather_than_swallowing_the_page() {
        let d = taglib_directives("<%@ taglib prefix=\"s\" uri=\"/s\"%>\n<%@ taglib prefix=\"c\"");
        assert_eq!(d.len(), 1);
    }
}
