//! Text mechanics: lines, module roots, type erasure, whole-word mentions.

use std::collections::HashSet;

/// 1-based line of a byte offset.
pub fn line_of(text: &str, offset: usize) -> u32 {
    let end = offset.min(text.len());
    text.as_bytes()[..end].iter().filter(|&&b| b == b'\n').count() as u32 + 1
}

/// A path, forward-slashed.
pub fn slash(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// The module a file belongs to: everything before its `/src/` segment.
///
/// What decides whether two URL mappings can collide — two wars in one repository may both map
/// `/api/*` and never meet — so a file with no `/src/` shares the root module rather than being
/// guessed into one.
pub fn module_root(path: &str) -> &str {
    match path.find("/src/") {
        Some(i) => &path[..i],
        None => "",
    }
}

/// The last segment of a dotted name.
pub fn simple_name(name: &str) -> &str {
    name.rsplit('.').next().unwrap_or(name)
}

/// `orderService` from `OrderService` — the name CDI gives a `@Named` bean that names nothing.
pub fn decapitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// A written type with its type arguments and array dimensions taken off.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Erased {
    /// The raw name, whitespace removed (`java.util.List`, `Outer.Inner`).
    pub raw: String,
    /// Written with type arguments.
    pub parameterized: bool,
    /// An array or a varargs.
    pub array: bool,
}

/// Erase a written type: `Map<String, List<Foo>>[]` → `Map`, parameterized, array.
pub fn erase(written: &str) -> Erased {
    let mut raw = String::new();
    let mut depth = 0i32;
    let mut parameterized = false;
    for c in strip_type_annotations(written).chars() {
        match c {
            '<' => {
                depth += 1;
                parameterized = true;
            }
            '>' => depth -= 1,
            _ if depth > 0 => {}
            c if c.is_whitespace() => {}
            c => raw.push(c),
        }
    }
    let array = raw.contains('[') || raw.ends_with("...");
    let raw = raw.trim_end_matches("...").replace("[]", "");
    Erased { raw, parameterized, array }
}

/// A written type without the type annotations in front of it (`@NonNull Foo`, `@Size(max = 3) Foo`)
/// — they are not part of its name, and removing whitespace first would glue them onto it.
fn strip_type_annotations(written: &str) -> &str {
    let mut rest = written.trim_start();
    while let Some(after_at) = rest.strip_prefix('@') {
        let name_end = after_at
            .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.' || c == '$'))
            .unwrap_or(after_at.len());
        let mut tail = after_at[name_end..].trim_start();
        if tail.starts_with('(') {
            let mut depth = 0i32;
            let mut close = tail.len();
            for (i, c) in tail.char_indices() {
                match c {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            close = i + 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            tail = tail[close..].trim_start();
        }
        rest = tail;
    }
    rest
}

pub fn is_primitive(name: &str) -> bool {
    matches!(name, "int" | "long" | "short" | "byte" | "char" | "boolean" | "float" | "double" | "void")
}

/// Whether any identifier in `text` is one of `words` — a whole-word test, so `OrderServiceImpl`
/// is not a mention of `OrderService`.
pub fn mentions_word(text: &str, words: &HashSet<String>) -> bool {
    if words.is_empty() {
        return false;
    }
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_alphabetic() || b == b'_' || b == b'$' {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'$') {
                i += 1;
            }
            if words.contains(&text[start..i]) {
                return true;
            }
        } else if b.is_ascii_digit() {
            while i < bytes.len() && bytes[i].is_ascii_alphanumeric() {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    false
}

/// The file stem of a path (`OrderService` for `…/OrderService.java`).
pub fn stem(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.strip_suffix(".java").unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn erasure_keeps_the_raw_name_and_says_what_it_took() {
        assert_eq!(erase("Map<String, List<Foo>>").raw, "Map");
        assert!(erase("Map<String, List<Foo>>").parameterized);
        let a = erase("Foo[]");
        assert_eq!((a.raw.as_str(), a.array), ("Foo", true));
        assert_eq!(erase("java.util.List").raw, "java.util.List");
        assert!(!erase("OrderService").parameterized);
        assert_eq!(erase("@NonNull Foo").raw, "Foo");
        assert_eq!(erase("@Size(max = 3) java.util.List<X>").raw, "java.util.List");
    }

    #[test]
    fn a_mention_is_a_whole_word() {
        let words: HashSet<String> = ["OrderService".to_string()].into_iter().collect();
        assert!(mentions_word("class A implements OrderService {}", &words));
        assert!(!mentions_word("class OrderServiceImpl {}", &words));
    }

    #[test]
    fn a_module_is_what_comes_before_src() {
        assert_eq!(module_root("/r/web/src/main/webapp/WEB-INF/web.xml"), "/r/web");
        assert_eq!(module_root("/r/web.xml"), "");
    }

    #[test]
    fn lines_count_from_one() {
        assert_eq!(line_of("a\nb", 2), 2);
        assert_eq!(line_of("a", 0), 1);
    }
}
