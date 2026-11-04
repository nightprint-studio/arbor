//! The media types of a mapping, as values.
//!
//! `@GetMapping(produces = {MediaType.APPLICATION_JSON_VALUE, MediaType.TEXT_EVENT_STREAM_VALUE})` is source
//! text, not a media type: a tool that wants to *send* `Accept: application/json, text/event-stream` has to
//! read through the constants, the array braces and the `+` joins. This resolves what can be resolved without
//! running Java — string literals, Spring's `MediaType` constants (with or without `_VALUE`), concatenations
//! and arrays of those — and, for anything else (a constant of the project), asks the caller's `lookup`.
//! What cannot be resolved is left out rather than passed on as source text.

/// Spring's `MediaType` constants: the name without the `_VALUE` suffix, and what it stands for.
const KNOWN: &[(&str, &str)] = &[
    ("ALL", "*/*"),
    ("APPLICATION_ATOM_XML", "application/atom+xml"),
    ("APPLICATION_CBOR", "application/cbor"),
    ("APPLICATION_FORM_URLENCODED", "application/x-www-form-urlencoded"),
    ("APPLICATION_GRAPHQL", "application/graphql"),
    ("APPLICATION_GRAPHQL_RESPONSE", "application/graphql-response+json"),
    ("APPLICATION_JSON", "application/json"),
    ("APPLICATION_JSON_UTF8", "application/json;charset=UTF-8"),
    ("APPLICATION_NDJSON", "application/x-ndjson"),
    ("APPLICATION_OCTET_STREAM", "application/octet-stream"),
    ("APPLICATION_PDF", "application/pdf"),
    ("APPLICATION_PROBLEM_JSON", "application/problem+json"),
    ("APPLICATION_PROBLEM_JSON_UTF8", "application/problem+json;charset=UTF-8"),
    ("APPLICATION_PROBLEM_XML", "application/problem+xml"),
    ("APPLICATION_PROTOBUF", "application/x-protobuf"),
    ("APPLICATION_RSS_XML", "application/rss+xml"),
    ("APPLICATION_STREAM_JSON", "application/stream+json"),
    ("APPLICATION_XHTML_XML", "application/xhtml+xml"),
    ("APPLICATION_XML", "application/xml"),
    ("IMAGE_GIF", "image/gif"),
    ("IMAGE_JPEG", "image/jpeg"),
    ("IMAGE_PNG", "image/png"),
    ("MULTIPART_FORM_DATA", "multipart/form-data"),
    ("MULTIPART_MIXED", "multipart/mixed"),
    ("MULTIPART_RELATED", "multipart/related"),
    ("TEXT_EVENT_STREAM", "text/event-stream"),
    ("TEXT_HTML", "text/html"),
    ("TEXT_MARKDOWN", "text/markdown"),
    ("TEXT_PLAIN", "text/plain"),
    ("TEXT_XML", "text/xml"),
];

/// A constant of Spring's `MediaType` (or `MimeTypeUtils`), by its source spelling: `MediaType.TEXT_HTML_VALUE`,
/// `TEXT_HTML` (statically imported), `MediaType.TEXT_HTML.toString()`.
pub fn known_constant(reference: &str) -> Option<&'static str> {
    let trimmed = reference.trim().trim_end_matches("()").trim_end_matches(".toString").trim_end_matches(".getType");
    let last = trimmed.rsplit('.').next()?;
    let name = last.strip_suffix("_VALUE").unwrap_or(last);
    KNOWN.iter().find(|(k, _)| *k == name).map(|(_, v)| *v)
}

/// Split `text` at the top-level occurrences of `sep` (outside quotes and brackets).
fn split_top(text: &str, sep: char) -> Vec<&str> {
    let (mut parts, mut depth, mut quoted, mut start) = (Vec::new(), 0i32, false, 0usize);
    let mut escaped = false;
    for (i, c) in text.char_indices() {
        if quoted {
            if escaped { escaped = false; } else if c == '\\' { escaped = true; } else if c == '"' { quoted = false; }
            continue;
        }
        match c {
            '"' => quoted = true,
            '(' | '{' | '[' => depth += 1,
            ')' | '}' | ']' => depth -= 1,
            c if c == sep && depth == 0 => {
                parts.push(&text[start..i]);
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    parts
}

/// One term of a concatenation: a string literal or a reference.
fn term_value(term: &str, lookup: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    let term = term.trim();
    if term.len() >= 2 && term.starts_with('"') && term.ends_with('"') {
        return Some(term[1..term.len() - 1].replace("\\\"", "\""));
    }
    if term.is_empty() {
        return None;
    }
    known_constant(term).map(str::to_string).or_else(|| lookup(term))
}

/// One element: its terms joined (`MediaType.APPLICATION_JSON_VALUE + ";charset=UTF-8"`); unresolved if any term is.
fn element_value(element: &str, lookup: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    split_top(element, '+').into_iter().map(|t| term_value(t, lookup)).collect::<Option<Vec<_>>>().map(|v| v.concat())
}

/// The media types a `produces` / `consumes` element names, as `application/json, text/event-stream`.
/// `raw` is the annotation element's source (`"application/json"`, `{A, B}`, `MediaType.X_VALUE`); `lookup`
/// resolves a reference this module does not know (a constant of the project) to its value.
pub fn resolve(raw: &str, lookup: &dyn Fn(&str) -> Option<String>) -> String {
    let body = raw.trim();
    let body = body.strip_prefix('{').and_then(|b| b.strip_suffix('}')).unwrap_or(body);
    split_top(body, ',').into_iter().filter_map(|e| element_value(e, lookup)).collect::<Vec<_>>().join(", ")
}

/// [`resolve`] with nothing beyond Spring's own constants.
pub fn resolve_known(raw: &str) -> String {
    resolve(raw, &|_| None)
}

/// The initializer of `static final … NAME = …;` in `source`, as written (a literal, a constant, a concatenation).
pub fn constant_initializer(source: &str, name: &str) -> Option<String> {
    let is_ident = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    for (at, _) in source.match_indices(name) {
        let before = source[..at].chars().next_back();
        let after = source[at + name.len()..].chars().next();
        if before.is_some_and(is_ident) || after.is_some_and(is_ident) {
            continue;
        }
        let line_start = source[..at].rfind('\n').map_or(0, |i| i + 1);
        let prefix = &source[line_start..at];
        if !(prefix.contains("static") && prefix.contains("final")) {
            continue;
        }
        let rest = source[at + name.len()..].trim_start();
        let Some(init) = rest.strip_prefix('=') else { continue };
        let (mut quoted, mut escaped) = (false, false);
        for (i, c) in init.char_indices() {
            if quoted {
                if escaped { escaped = false; } else if c == '\\' { escaped = true; } else if c == '"' { quoted = false; }
            } else if c == '"' {
                quoted = true;
            } else if c == ';' {
                return Some(init[..i].trim().to_string());
            }
        }
    }
    None
}

fn lookup_reference(reference: &str, read: &dyn Fn(Option<&str>) -> Option<String>, depth: u8) -> Option<String> {
    if depth > 3 {
        return None;
    }
    let parts: Vec<&str> = reference.trim().split('.').collect();
    let name = parts.last()?;
    let class = (parts.len() >= 2).then(|| parts[parts.len() - 2]);
    let source = read(class)?;
    let initializer = constant_initializer(&source, name)?;
    let value = resolve(&initializer, &|r| lookup_reference(r, read, depth + 1));
    (!value.is_empty()).then_some(value)
}

/// [`resolve`] reading the project's constants from source: `read(None)` is the file the annotation is in,
/// `read(Some("Api"))` the source of the class `Api`. A constant defined through another constant is followed a few levels.
pub fn resolve_with_sources(raw: &str, read: &dyn Fn(Option<&str>) -> Option<String>) -> String {
    resolve(raw, &|r| lookup_reference(r, read, 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_element_is_empty() {
        assert_eq!(resolve_known(""), "");
    }

    #[test]
    fn a_string_literal_is_its_content() {
        assert_eq!(resolve_known("\"application/xml\""), "application/xml");
    }

    #[test]
    fn a_spring_constant_is_its_value_with_or_without_the_suffix() {
        assert_eq!(resolve_known("MediaType.APPLICATION_JSON_VALUE"), "application/json");
        assert_eq!(resolve_known("MediaType.APPLICATION_JSON"), "application/json");
        assert_eq!(resolve_known("TEXT_EVENT_STREAM_VALUE"), "text/event-stream");
        assert_eq!(resolve_known("MediaType.TEXT_HTML.toString()"), "text/html");
    }

    #[test]
    fn an_array_of_constants_is_a_list() {
        assert_eq!(
            resolve_known("{MediaType.APPLICATION_JSON_VALUE, MediaType.TEXT_EVENT_STREAM_VALUE}"),
            "application/json, text/event-stream"
        );
        assert_eq!(resolve_known("{\"a/b\", MediaType.TEXT_PLAIN_VALUE}"), "a/b, text/plain");
    }

    #[test]
    fn a_concatenation_is_joined() {
        assert_eq!(resolve_known("MediaType.APPLICATION_JSON_VALUE + \";charset=UTF-8\""), "application/json;charset=UTF-8");
        assert_eq!(resolve_known("\"application/\" + \"json\""), "application/json");
    }

    #[test]
    fn a_comma_inside_a_literal_does_not_split() {
        assert_eq!(resolve_known("\"text/plain;q=0.5,x\""), "text/plain;q=0.5,x");
    }

    #[test]
    fn a_project_constant_goes_through_the_lookup_and_unknown_ones_are_dropped() {
        let lookup = |r: &str| (r == "Api.VND").then(|| "application/vnd.shop.v1+json".to_string());
        assert_eq!(resolve("{Api.VND, MediaType.APPLICATION_XML_VALUE}", &lookup), "application/vnd.shop.v1+json, application/xml");
        assert_eq!(resolve("{Other.THING, MediaType.APPLICATION_XML_VALUE}", &lookup), "application/xml");
    }

    #[test]
    fn a_static_final_string_is_found_by_name() {
        let src = "class Api {
  public static final String JSON_V1 = \"application/vnd.shop.v1+json\";
  String other = \"x\";
  static final String BOTH = MediaType.APPLICATION_XML_VALUE + \";charset=UTF-8\";
}";
        assert_eq!(constant_initializer(src, "JSON_V1").as_deref(), Some("\"application/vnd.shop.v1+json\""));
        assert_eq!(constant_initializer(src, "other"), None, "not static final");
        assert_eq!(constant_initializer(src, "JSON"), None, "a whole word only");
        assert!(constant_initializer(src, "BOTH").unwrap().contains("MediaType.APPLICATION_XML_VALUE"));
    }

    #[test]
    fn constants_of_the_project_are_read_from_source_through_other_constants() {
        let read = |class: Option<&str>| match class {
            Some("Api") => Some("class Api { static final String V1 = \"application/vnd.shop.v1+json\"; static final String XML = MediaType.APPLICATION_XML_VALUE + \";charset=UTF-8\"; }".to_string()),
            None => Some("class C { private static final String LOCAL = Api.V1; }".to_string()),
            _ => None,
        };
        assert_eq!(resolve_with_sources("{Api.V1, LOCAL, Api.XML, Missing.X}", &read), "application/vnd.shop.v1+json, application/vnd.shop.v1+json, application/xml;charset=UTF-8");
    }
}
