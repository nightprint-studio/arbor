//! The context block in front of the code in every prompt: the members of the types the code is about, in the
//! shape the model was trained on. Training built it with this same logic (the lab's `header.rs`), which is why
//! this module copies its rules to the letter — a header the model never met is a header it reads badly.
//!
//! ```text
//! //@ vars: items:List<Item> order:Order
//! //@ Order: getId() pay(Payment,int) status
//! //@ Item: sku qty getSku()
//! ```
//!
//! The `vars` line names the variables the nearby code uses whose declaration lies **above** the window the prompt
//! carries (a field, a parameter of an earlier method), with their types; the types of those variables get member
//! lines like any other type the code mentions.
//!
//! One line per type the code mentioned lately (most recent first), the type under the caret's receiver first of
//! all with more room; members ranked by the name being typed, then by names the code already uses, then in
//! declaration order; at most [`HEADER_BUDGET`] bytes in all. The host supplies the members through
//! [`TypeLookup`]: this crate has no index.

/// The header never exceeds this many bytes: what training allowed, leaving the prompt room for code.
pub const HEADER_BUDGET: usize = 256;
const MEMBERS_PER_TYPE: usize = 10;
const MEMBERS_FOCUS: usize = 24;
/// Names looked up at most; each one costs the host an index query, and the budget fits a handful of lines.
const MAX_LOOKUPS: usize = 12;
/// Variables the `vars` line names at most, and the bytes it may take of the header.
const MAX_VARS: usize = 6;
const VARS_BUDGET: usize = 96;
const OBJECT_METHODS: [&str; 9] = ["toString", "equals", "hashCode", "getClass", "notify", "notifyAll", "wait", "clone", "finalize"];

/// One member of a type as the header lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderMember {
    pub name: String,
    pub is_method: bool,
    /// Simple parameter type names, generics dropped (`Map`, `String[]`, `String...`); methods only.
    pub params: Vec<String>,
}

/// A type the host resolved: an identity (two names for one type must give the same key) and its members, own
/// first and then inherited, in declaration order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeInfo {
    pub key: String,
    pub members: Vec<HeaderMember>,
}

/// What the host knows about types.
pub trait TypeLookup {
    /// The type a simple (or qualified) name means at the caret's file; `None` when it resolves to nothing.
    fn resolve(&self, name: &str) -> Option<TypeInfo>;
}

fn is_ident(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'$'
}

/// Comments and string/char literal contents replaced by spaces, offsets kept.
fn blank_comments_and_strings(b: &[u8]) -> Vec<u8> {
    let mut o = b.to_vec();
    let (mut i, n) = (0, b.len());
    while i < n {
        match (b[i], b.get(i + 1).copied()) {
            (b'/', Some(b'/')) => {
                while i < n && b[i] != b'\n' {
                    o[i] = b' ';
                    i += 1;
                }
            }
            (b'/', Some(b'*')) => {
                while i < n && !(b[i] == b'*' && b.get(i + 1) == Some(&b'/')) {
                    if b[i] != b'\n' {
                        o[i] = b' ';
                    }
                    i += 1;
                }
                if i < n {
                    o[i] = b' ';
                    o[i + 1] = b' ';
                    i += 2;
                }
            }
            (q @ (b'"' | b'\''), _) => {
                i += 1;
                while i < n && b[i] != q && b[i] != b'\n' {
                    if b[i] == b'\\' && i + 1 < n {
                        o[i] = b' ';
                        i += 1;
                    }
                    o[i] = b' ';
                    i += 1;
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    o
}

/// Capitalised identifiers of `code`, most recently mentioned first, without duplicates; one or two letters
/// (`T`, `ID`) are type parameters and acronyms and are left out.
pub fn mentioned_types(code: &str) -> Vec<&str> {
    let blanked = blank_comments_and_strings(code.as_bytes());
    let b = &blanked[..];
    let mut found: Vec<(usize, &str)> = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_uppercase() && (i == 0 || !is_ident(b[i - 1])) {
            let mut j = i;
            while j < b.len() && is_ident(b[j]) {
                j += 1;
            }
            let w = &code[i..j];
            if w.len() > 2 {
                match found.iter_mut().find(|(_, x)| *x == w) {
                    Some(slot) => slot.0 = i,
                    None => found.push((i, w)),
                }
            }
            i = j;
        } else {
            i += 1;
        }
    }
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found.into_iter().map(|(_, w)| w).collect()
}

/// The declarations `Type name` followed by `;`, `=`, `,`, `)` or `:` in `text`, in source order: `(name, type)` with
/// the type unqualified and its generics squeezed (`List<Item>`; dropped when long).
fn declarations(text: &str) -> Vec<(String, String)> {
    let blanked = blank_comments_and_strings(text.as_bytes());
    let b = &blanked[..];
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if (b[i].is_ascii_lowercase() || b[i] == b'_') && (i == 0 || !is_ident(b[i - 1])) {
            let mut j = i;
            while j < b.len() && is_ident(b[j]) {
                j += 1;
            }
            let mut k = j;
            while k < b.len() && (b[k] == b' ' || b[k] == b'\t') {
                k += 1;
            }
            let ends_declaration = match b.get(k) {
                Some(b';' | b',' | b')' | b':') => true,
                Some(b'=') => b.get(k + 1) != Some(&b'='),
                _ => false,
            };
            let mut s = i;
            while s > 0 && (b[s - 1] == b' ' || b[s - 1] == b'\t') {
                s -= 1;
            }
            if ends_declaration && s < i && s > 0 {
                if let Some(ty) = type_before(text, b, s) {
                    out.push((text[i..j].to_string(), ty));
                }
            }
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

/// The type written right before `end` (`List<Item>`, `Order[]`, `java.util.Map<K,V>`), when it is one: the last
/// segment of its base name starts with a capital.
fn type_before(text: &str, b: &[u8], end: usize) -> Option<String> {
    let mut e = end;
    while e >= 2 && b[e - 1] == b']' && b[e - 2] == b'[' {
        e -= 2;
    }
    let args_end = e;
    let mut base_end = e;
    if base_end > 0 && b[base_end - 1] == b'>' {
        let (mut depth, mut k) = (0i32, base_end);
        loop {
            if k == 0 {
                return None;
            }
            k -= 1;
            match b[k] {
                b'>' => depth += 1,
                b'<' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                b'\n' | b';' | b'{' | b'}' => return None,
                _ => {}
            }
        }
        base_end = k;
    }
    let mut start = base_end;
    while start > 0 && (is_ident(b[start - 1]) || b[start - 1] == b'.') {
        start -= 1;
    }
    if start == base_end {
        return None;
    }
    let base = &text[start..base_end];
    let simple = base.rsplit('.').next()?;
    if !simple.starts_with(|c: char| c.is_ascii_uppercase()) {
        return None;
    }
    let tail: String = text[base_end..end].chars().filter(|c| !c.is_whitespace()).collect();
    let _ = args_end;
    let full = format!("{simple}{tail}");
    Some(if full.len() <= 28 { full } else { format!("{simple}{}", tail.rsplit_once('>').map_or("", |(_, arr)| arr)) })
}

/// The variables the code of `window` uses whose declaration is in `outside` (the file above the window) and nowhere
/// in the window itself, most recently used first, with their declared types.
pub fn outside_vars(outside: &str, window: &str) -> Vec<(String, String)> {
    if outside.is_empty() {
        return Vec::new();
    }
    let declared_here: Vec<String> = declarations(window).into_iter().map(|(n, _)| n).collect();
    let above = declarations(outside);
    let blanked = blank_comments_and_strings(window.as_bytes());
    let b = &blanked[..];
    let mut used: Vec<(usize, &str)> = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if (b[i].is_ascii_lowercase() || b[i] == b'_') && (i == 0 || !is_ident(b[i - 1])) {
            let mut j = i;
            while j < b.len() && is_ident(b[j]) {
                j += 1;
            }
            // `a.name` is a member of something else; `this.name` is the field itself
            let member_of_other = i > 0 && b[i - 1] == b'.' && !window[..i].ends_with("this.");
            let w = &window[i..j];
            if !member_of_other && !declared_here.iter().any(|d| d == w) && above.iter().any(|(n, _)| n == w) {
                match used.iter_mut().find(|(_, x)| *x == w) {
                    Some(slot) => slot.0 = i,
                    None => used.push((i, w)),
                }
            }
            i = j;
        } else {
            i += 1;
        }
    }
    used.sort_by(|a, b| b.0.cmp(&a.0));
    used.into_iter()
        .take(MAX_VARS)
        .filter_map(|(_, w)| above.iter().rev().find(|(n, _)| n == w).map(|(n, t)| (n.clone(), t.clone())))
        .collect()
}

/// `//@ vars: a:A b:B`, cut at a pair boundary to [`VARS_BUDGET`] bytes.
fn vars_line(vars: &[(String, String)]) -> Option<String> {
    let mut line = String::from("//@ vars:");
    for (name, ty) in vars {
        let piece = format!(" {name}:{ty}");
        if line.len() + piece.len() + 1 > VARS_BUDGET {
            break;
        }
        line.push_str(&piece);
    }
    (line.len() > "//@ vars:".len()).then(|| line + "\n")
}

fn render(m: &HeaderMember) -> String {
    if m.is_method { format!("{}({})", m.name, m.params.join(",")) } else { m.name.clone() }
}

/// The name a type is shown under: the last segment of its key.
fn display_name(key: &str) -> &str {
    key.rsplit(['.', '/', '$']).next().unwrap_or(key)
}

/// One `//@ Type: m(P) f ...` line, members ranked by what the code at hand points to.
fn type_line(info: &TypeInfo, code: &str, partial: &str, cap: usize) -> Option<String> {
    if info.members.len() < 2 {
        return None;
    }
    let mut ranked: Vec<(u8, &HeaderMember)> = info
        .members
        .iter()
        .filter(|m| !OBJECT_METHODS.contains(&m.name.as_str()))
        .map(|m| {
            let rank = if !partial.is_empty() && m.name.starts_with(partial) {
                0
            } else if code.contains(&m.name) {
                1
            } else {
                2
            };
            (rank, m)
        })
        .collect();
    ranked.sort_by_key(|(r, _)| *r);
    let mut shown: Vec<String> = Vec::new();
    for (_, m) in ranked {
        let r = render(m);
        if !shown.contains(&r) {
            shown.push(r);
        }
        if shown.len() == cap {
            break;
        }
    }
    (!shown.is_empty()).then(|| format!("//@ {}: {}\n", display_name(&info.key), shown.join(" ")))
}

/// The header for the code in the prompt's window. `focus` is the type of the receiver when the caret follows a
/// `.`, with `partial` the half-typed member name; both empty otherwise.
pub fn build(lookup: &dyn TypeLookup, code: &str, focus: Option<&TypeInfo>, partial: &str) -> String {
    build_in(lookup, "", code, focus, partial)
}

/// [`build`] for a window that is only the end of the file: `outside` is the text above it, where the declarations of
/// the variables the window uses may be.
pub fn build_in(lookup: &dyn TypeLookup, outside: &str, code: &str, focus: Option<&TypeInfo>, partial: &str) -> String {
    let mut out = String::new();
    let add = |line: String, out: &mut String| {
        let fits = out.len() + line.len() <= HEADER_BUDGET;
        if fits {
            out.push_str(&line);
        }
        fits
    };
    if let Some(info) = focus {
        if let Some(mut line) = type_line(info, code, partial, MEMBERS_FOCUS) {
            // an oversized focus line is cut at a member boundary rather than dropped
            while line.len() > HEADER_BUDGET {
                match line.trim_end().rfind(' ') {
                    Some(p) => line = format!("{}\n", &line[..p]),
                    None => break,
                }
            }
            add(line, &mut out);
        }
    }
    let vars = outside_vars(outside, code);
    if let Some(line) = vars_line(&vars) {
        add(line, &mut out);
    }
    // the types of those variables come first: the code uses them and does not name them
    let var_types: Vec<String> = vars
        .iter()
        .map(|(_, t)| t.split(['<', '[']).next().unwrap_or(t).to_string())
        .filter(|t| t.len() > 2)
        .collect();
    let mut done: Vec<String> = focus.map(|f| f.key.clone()).into_iter().collect();
    let names: Vec<&str> = var_types.iter().map(String::as_str).chain(mentioned_types(code)).collect();
    for simple in names.into_iter().take(MAX_LOOKUPS) {
        let Some(info) = lookup.resolve(simple) else { continue };
        if done.contains(&info.key) {
            continue;
        }
        done.push(info.key.clone());
        if let Some(line) = type_line(&info, code, partial, MEMBERS_PER_TYPE) {
            if !add(line, &mut out) {
                break;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct Fixed(HashMap<String, TypeInfo>);

    impl TypeLookup for Fixed {
        fn resolve(&self, name: &str) -> Option<TypeInfo> {
            self.0.get(name).cloned()
        }
    }

    fn method(name: &str, params: &[&str]) -> HeaderMember {
        HeaderMember { name: name.into(), is_method: true, params: params.iter().map(|p| p.to_string()).collect() }
    }

    fn lookup() -> Fixed {
        let mono = TypeInfo { key: "demo.Mono".into(), members: vec![method("just", &["T"]), method("map", &["Function"])] };
        Fixed(HashMap::from([("Mono".to_string(), mono)]))
    }

    #[test]
    fn mentioned_types_come_most_recent_first_without_short_names_or_comments() {
        let code = "// Skipped here\nOrder a; T t; Item b; String s = \"Hidden\"; Order again;";
        assert_eq!(mentioned_types(code), ["Order", "String", "Item"]);
    }

    #[test]
    fn a_type_with_two_members_gets_a_line() {
        let h = build(&lookup(), "return Mono.just(1).map(x -> x);", None, "");
        assert_eq!(h, "//@ Mono: just(T) map(Function)\n");
        assert!(build(&lookup(), "int a = 1;", None, "").is_empty());
    }

    #[test]
    fn declarations_are_read_with_their_types() {
        let d = declarations("private final List<Item> items;\nOrder order = null;\nvoid f(Map<String, Integer> m, int n, String... xs) { Foo[] arr, other; }");
        let names: Vec<(&str, &str)> = d.iter().map(|(n, t)| (n.as_str(), t.as_str())).collect();
        assert!(names.contains(&("items", "List<Item>")), "{names:?}");
        assert!(names.contains(&("order", "Order")), "{names:?}");
        assert!(names.contains(&("m", "Map<String,Integer>")), "{names:?}");
        assert!(!names.iter().any(|(n, _)| *n == "n"), "a primitive is not a capitalised type: {names:?}");
    }

    #[test]
    fn a_variable_declared_above_the_window_is_named_with_its_type() {
        let outside = "class A {\n    private final TabellatoService service;\n    void other(Repo repo) {}\n";
        let window = "    void run(String id) {\n        var x = service.find(id);\n        this.repo.save(x);\n        id.length();\n";
        let vars = outside_vars(outside, window);
        // most recently used first; `this.repo` is read as the field, whatever else `repo` was elsewhere
        assert_eq!(vars, [("repo".to_string(), "Repo".to_string()), ("service".to_string(), "TabellatoService".to_string())]);
        // a variable the window declares itself, or a member of another object, is not repeated
        let window = "    void run(Repo repo) {\n        repo.save(a.service);\n";
        assert!(outside_vars(outside, window).is_empty(), "{:?}", outside_vars(outside, window));
    }

    #[test]
    fn the_vars_line_and_the_type_lines_of_those_variables_lead_the_header() {
        let svc = TypeInfo { key: "demo.TabellatoService".into(), members: vec![method("find", &["String"]), method("save", &["Tabellato"])] };
        let lookup = Fixed(HashMap::from([("TabellatoService".to_string(), svc)]));
        let h = build_in(&lookup, "    private final TabellatoService service;\n", "        var x = service.find(id);\n", None, "");
        assert_eq!(h, "//@ vars: service:TabellatoService\n//@ TabellatoService: find(String) save(Tabellato)\n");
    }

    #[test]
    fn the_name_being_typed_ranks_first_in_the_focus_line() {
        let a = TypeInfo { key: "demo.A".into(), members: vec![method("alpha", &[]), method("beta", &[])] };
        let h = build(&lookup(), "B b; A a;", Some(&a), "be");
        assert!(h.starts_with("//@ A: beta() alpha()"), "{h}");
    }
}
