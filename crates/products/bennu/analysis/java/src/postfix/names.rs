//! The names a template proposes for the variables it declares.
//!
//! Always a selected tab stop, so a poor guess costs the keystrokes of typing over it, never a wrong
//! program. The aim is the name IntelliJ would propose: the one the value already reads as —
//! `getTotal()` is a `total`, one of the `orders` is an `order` — and the type's own name when the
//! expression says nothing.

pub(crate) const KEYWORDS: &[&str] = &[
    "abstract", "assert", "boolean", "break", "byte", "case", "catch", "char", "class", "const",
    "continue", "default", "do", "double", "else", "enum", "extends", "false", "final", "finally",
    "float", "for", "goto", "if", "implements", "import", "instanceof", "int", "interface", "long",
    "native", "new", "null", "package", "private", "protected", "public", "record", "return",
    "short", "static", "strictfp", "super", "switch", "synchronized", "this", "throw", "throws",
    "transient", "true", "try", "var", "void", "volatile", "while", "yield",
];

/// Calls that name an action rather than the value it returns: `findById(id)` gives no name.
const ACTIONS: &[&str] = &[
    "apply", "build", "call", "clone", "compute", "copy", "create", "execute", "fetch", "find", "from",
    "get", "load", "of", "parse", "read", "run", "valueOf",
];

/// The accessor prefixes a property name hides behind: `getTotal` is a `total`, `isActive` an `active`.
const ACCESSORS: &[&str] = &["get", "is", "to", "as"];

/// The name the value of `expr` reads as, or `None` when the expression does not say one.
pub(crate) fn value_name(expr: &str) -> Option<String> {
    let (name, called) = last_name(expr)?;
    let base = match called {
        false => name.to_string(),
        true => match accessor_property(name) {
            Some(property) => property,
            None if is_action(name) => return None,
            None => name.to_string(),
        },
    };
    usable(decapitalize(&base))
}

/// A name for one element of a container: the singular of the container's own name when it has one
/// (`orders` → `order`), else the element type's name.
pub(crate) fn element_name(container: Option<&str>, element_binary: &str) -> String {
    container
        .and_then(singular)
        .and_then(usable)
        .unwrap_or_else(|| type_name_hint(element_binary))
}

/// The name a value of type `binary` reads as when nothing better is known.
pub(crate) fn type_name_hint(binary: &str) -> String {
    let short = match binary {
        "int" => "i",
        "long" => "l",
        "short" => "s",
        "byte" | "boolean" => "b",
        "char" => "c",
        "float" => "f",
        "double" => "d",
        "java/lang/String" => "s",
        "java/lang/Object" => "o",
        "java/lang/Class" => "clazz",
        _ => {
            let simple = binary.rsplit(['/', '$']).next().unwrap_or(binary);
            return usable(decapitalize(simple)).unwrap_or_else(|| "value".to_string());
        }
    };
    short.to_string()
}

/// `order` → `orders`, `entry` → `entries`, `box` → `boxes`.
pub(crate) fn plural(name: &str) -> String {
    if let Some(stem) = name.strip_suffix('y') {
        if !stem.is_empty() && !stem.ends_with(['a', 'e', 'i', 'o', 'u']) {
            return format!("{stem}ies");
        }
    }
    if name.ends_with(['s', 'x']) || name.ends_with("ch") || name.ends_with("sh") {
        return format!("{name}es");
    }
    format!("{name}s")
}

/// `orders` → `order`, `entries` → `entry`, `addresses` → `address`; `None` for a name that does not
/// read as a plural (`status`, `data`).
fn singular(name: &str) -> Option<String> {
    if let Some(stem) = name.strip_suffix("ies").filter(|stem| !stem.is_empty()) {
        return Some(format!("{stem}y"));
    }
    if ["sses", "xes", "ches", "shes"].iter().any(|suffix| name.ends_with(suffix)) {
        return Some(name[..name.len() - 2].to_string());
    }
    let stem = name.strip_suffix('s')?;
    (!stem.is_empty() && !stem.ends_with(['s', 'u', 'i'])).then(|| stem.to_string())
}

/// The last name in `expr` and whether it is called: `repo.findAll()` → (`findAll`, true), `rows[0]` →
/// (`rows`, false). `None` for a literal, `this`, a constructor call or a parenthesised expression.
fn last_name(expr: &str) -> Option<(&str, bool)> {
    let expr = expr.trim();
    if expr.starts_with("new ") || expr == "this" {
        return None;
    }
    let bytes = expr.as_bytes();
    let mut end = bytes.len();
    let mut called = false;
    // Trailing groups come off first: `rows[0]`, `find(id)`, `get(0)[1]`.
    while end > 0 && matches!(bytes[end - 1], b')' | b']') {
        let closer = bytes[end - 1];
        let opener = if closer == b')' { b'(' } else { b'[' };
        let mut depth = 0usize;
        let mut open = None;
        for i in (0..end).rev() {
            if bytes[i] == closer {
                depth += 1;
            } else if bytes[i] == opener {
                depth -= 1;
                if depth == 0 {
                    open = Some(i);
                    break;
                }
            }
        }
        called |= closer == b')';
        end = open?;
    }
    let head = &expr[..end];
    let start = head
        .char_indices()
        .rev()
        .find(|(_, c)| !(c.is_alphanumeric() || *c == '_' || *c == '$'))
        .map_or(0, |(i, c)| i + c.len_utf8());
    let name = &head[start..];
    (!name.is_empty() && !name.starts_with(|c: char| c.is_ascii_digit())).then_some((name, called))
}

fn accessor_property(name: &str) -> Option<String> {
    ACCESSORS.iter().find_map(|prefix| {
        name.strip_prefix(prefix)
            .filter(|rest| rest.starts_with(|c: char| c.is_ascii_uppercase()))
            .map(decapitalize)
    })
}

fn is_action(name: &str) -> bool {
    ACTIONS.iter().any(|action| {
        name == *action
            || name
                .strip_prefix(action)
                .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_uppercase()))
    })
}

/// `OrderLine` → `orderLine`, `URL` → `url`, `URLBuilder` → `urlBuilder`.
pub(crate) fn decapitalize(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let upper = chars.iter().take_while(|c| c.is_uppercase()).count();
    let lowered = match upper {
        0 => return name.to_string(),
        n if n == chars.len() => n,
        1 => 1,
        // An acronym runs into the next word: its last capital starts that word.
        n => n - 1,
    };
    chars
        .iter()
        .enumerate()
        .flat_map(|(i, c)| if i < lowered { c.to_lowercase().collect::<Vec<_>>() } else { vec![*c] })
        .collect()
}

/// `name`, when it can be declared: an identifier that is not a keyword.
pub(crate) fn usable(name: impl Into<String>) -> Option<String> {
    let name = name.into();
    let starts_well = name.starts_with(|c: char| c.is_alphabetic() || c == '_' || c == '$');
    (starts_well && !KEYWORDS.contains(&name.as_str())).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_accessor_call_reads_as_its_property() {
        assert_eq!(value_name("order.getTotal()").as_deref(), Some("total"));
        assert_eq!(value_name("user.isActive()").as_deref(), Some("active"));
        assert_eq!(value_name("this.orders").as_deref(), Some("orders"));
        assert_eq!(value_name("point.x()").as_deref(), Some("x"));
    }

    #[test]
    fn a_call_that_names_an_action_gives_no_name() {
        assert_eq!(value_name("repo.findById(id)"), None);
        assert_eq!(value_name("list.get(0)"), None);
        assert_eq!(value_name("Order.of(1)"), None);
    }

    #[test]
    fn literals_constructors_and_this_give_no_name() {
        assert_eq!(value_name("\"text\""), None);
        assert_eq!(value_name("new Order()"), None);
        assert_eq!(value_name("this"), None);
        assert_eq!(value_name("(a + b)"), None);
    }

    #[test]
    fn an_indexed_name_is_the_name() {
        assert_eq!(value_name("rows[i]").as_deref(), Some("rows"));
    }

    #[test]
    fn one_of_a_plural_is_its_singular() {
        assert_eq!(element_name(Some("orders"), "com/acme/Order"), "order");
        assert_eq!(element_name(Some("entries"), "java/util/Map$Entry"), "entry");
        assert_eq!(element_name(Some("addresses"), "com/acme/Address"), "address");
    }

    #[test]
    fn a_container_whose_name_is_not_plural_names_its_elements_after_their_type() {
        assert_eq!(element_name(Some("status"), "com/acme/OrderLine"), "orderLine");
        assert_eq!(element_name(None, "java/util/Map$Entry"), "entry");
        // `classes` singularises to a keyword, which cannot be declared.
        assert_eq!(element_name(Some("classes"), "java/lang/Class"), "clazz");
    }

    #[test]
    fn a_type_name_is_decapitalised_acronyms_included() {
        assert_eq!(type_name_hint("java/net/URL"), "url");
        assert_eq!(type_name_hint("com/acme/URLBuilder"), "urlBuilder");
        assert_eq!(type_name_hint("int"), "i");
        // A class named like a keyword cannot give its name to a variable.
        assert_eq!(type_name_hint("com/acme/Default"), "value");
    }

    #[test]
    fn plurals_follow_english_spelling() {
        assert_eq!(plural("order"), "orders");
        assert_eq!(plural("entry"), "entries");
        assert_eq!(plural("key"), "keys");
        assert_eq!(plural("box"), "boxes");
    }
}
