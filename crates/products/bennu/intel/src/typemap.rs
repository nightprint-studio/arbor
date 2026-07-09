//! Parse a written Java type text (`Map<String, Object>`) into a [`bennu_java`] seam
//! [`TypeRef`] with binary names, resolving simple names via imports + same-project
//! types.
//!
//! This is the build-time counterpart of `bennu-java`'s internal `typeparse` +
//! `simple_to_binary` (not on its public prelude), used to bake a project type's
//! resolved [`bennu_java::prelude::ClassMembers`] into the index at ingest time. At
//! query time the resolver reads that baked shape straight back — no re-parse.

use std::collections::BTreeMap;

use bennu_java::prelude::{Import, TypeRef};

/// A parsed simple-name type tree (before binary-name resolution).
#[derive(Debug, Clone)]
struct Parsed {
    name: String,
    args: Vec<Parsed>,
}

/// Convert a written type `text` to a resolved [`TypeRef`]. Falls back to a bare token
/// on unparseable input (an unknown binary name is a benign resolver miss). `is_project` tests a
/// candidate binary for project membership, so a wildcard import (`import pkg.*;`) can pin the exact
/// package of a same-simple-name type.
pub fn type_text_to_ref(
    text: &str,
    imports: &[Import],
    project_types: &BTreeMap<String, String>,
    is_project: &dyn Fn(&str) -> bool,
) -> TypeRef {
    let trimmed = text.trim();
    match parse_type(trimmed) {
        Some(p) => to_binary_ref(&p, imports, project_types, is_project),
        None => TypeRef::simple(trimmed.replace('.', "/")),
    }
}

fn to_binary_ref(
    p: &Parsed,
    imports: &[Import],
    project_types: &BTreeMap<String, String>,
    is_project: &dyn Fn(&str) -> bool,
) -> TypeRef {
    TypeRef {
        binary_name: simple_to_binary(&p.name, imports, project_types, is_project),
        type_args: p.args.iter().map(|a| to_binary_ref(a, imports, project_types, is_project)).collect(),
    }
}

/// Resolve a simple type name to a binary name, mirroring Java name lookup: dotted→slashed; a
/// single-type import; a **wildcard import of a project package** (`import com.x.*;` → `com/x/Foo`
/// when that's a real project type — this disambiguates a simple name that collides across packages,
/// the JAXB `*Type` case); then the project-wide simple→binary map (collision-prone, so it comes after
/// the imports); then the java.lang fallback; else the bare token.
fn simple_to_binary(
    simple: &str,
    imports: &[Import],
    project_types: &BTreeMap<String, String>,
    is_project: &dyn Fn(&str) -> bool,
) -> String {
    if simple.contains('.') {
        return simple.replace('.', "/");
    }
    if simple.ends_with("[]") || is_primitive(simple) {
        return simple.to_string();
    }
    // A single-type import wins over the collision-prone project map.
    for imp in imports {
        if imp.simple_name() == Some(simple) {
            return imp.path.replace('.', "/");
        }
    }
    // A non-static wildcard import that brings in a PROJECT type of this simple name pins its package.
    for imp in imports {
        if imp.star && !imp.static_ {
            let candidate = format!("{}/{simple}", imp.path.replace('.', "/"));
            if is_project(&candidate) {
                return candidate;
            }
        }
    }
    if let Some(b) = project_types.get(simple) {
        return b.clone();
    }
    match simple {
        "String" | "Object" | "Integer" | "Long" | "Boolean" | "Double" | "Float"
        | "Character" | "Byte" | "Short" | "Number" | "CharSequence" | "Iterable"
        | "Comparable" | "Runnable" | "Thread" | "Class" | "Exception" | "Throwable" => {
            format!("java/lang/{simple}")
        }
        _ => simple.to_string(),
    }
}

fn is_primitive(s: &str) -> bool {
    matches!(
        s,
        "int" | "long" | "short" | "byte" | "char" | "boolean" | "float" | "double" | "void"
    )
}

/// Parse `Foo`, `a.b.Foo`, `List<Foo>`, `Map<K, V<X>>` into a [`Parsed`] tree.
fn parse_type(s: &str) -> Option<Parsed> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let (name, rest) = match s.find('<') {
        Some(i) => (s[..i].trim().to_string(), Some(&s[i..])),
        None => (s.to_string(), None),
    };
    let args = match rest {
        Some(inner) => parse_args(inner)?,
        None => Vec::new(),
    };
    Some(Parsed { name, args })
}

/// Parse a `<A, B<C>>` argument list (including the surrounding angle brackets),
/// respecting nesting.
fn parse_args(s: &str) -> Option<Vec<Parsed>> {
    let bytes = s.as_bytes();
    if bytes.first() != Some(&b'<') {
        return None;
    }
    let mut depth = 0usize;
    let mut start = 0usize;
    let mut args = Vec::new();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'<' => {
                depth += 1;
                if depth == 1 {
                    start = i + 1;
                }
            }
            b'>' => {
                depth -= 1;
                if depth == 0 {
                    push_arg(&s[start..i], &mut args);
                    break;
                }
            }
            b',' if depth == 1 => {
                push_arg(&s[start..i], &mut args);
                start = i + 1;
            }
            _ => {}
        }
    }
    Some(args)
}

fn push_arg(chunk: &str, out: &mut Vec<Parsed>) {
    let t = chunk.trim();
    if t.is_empty() {
        return;
    }
    // Wildcards `?` / `? extends X` / `? super X` collapse to their bound or Object.
    let resolved = if t == "?" {
        "Object"
    } else if let Some(rest) = t.strip_prefix("? extends ") {
        rest.trim()
    } else if let Some(rest) = t.strip_prefix("? super ") {
        rest.trim()
    } else {
        t
    };
    if let Some(p) = parse_type(resolved) {
        out.push(p);
    }
}
