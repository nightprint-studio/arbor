//! Rendering a resolved member back into the text a person reads.
//!
//! A [`Member`](bennu_java::prelude::Member) carries binary names and a `raw_signature` string;
//! everything a user is shown — an override stub's parameter list, a parameter hint's signature, an
//! inlay hint's name — is that model turned back into Java. These are the pieces that do it.
//!
//! They live apart from any one consumer because the *names* matter and are fragile: a parameter is
//! called `volume` because the rendered signature said so, and a member decoded from a class file
//! usually says nothing at all. Two consumers deriving that separately would sooner or later show
//! the same method under two different parameter names.
//!
//! And they are careful about the difference between *not knowing* a name and knowing it to be
//! `arg0`. [`parameters`] fills the gap with a placeholder, which is what a code generator needs;
//! [`named_parameters`] leaves it empty, which is what anything that shows the name to a reader
//! needs. Blurring the two put `get_genere(arg0: …)` on the screen above a method whose parameter is
//! called `codice`.

use bennu_java::prelude::{Member, TypeRef};

/// The `(written type, parameter name)` pairs of a method, in declaration order.
///
/// Names come from the member's rendered signature when it has them — an override of
/// `speak(int volume)` reading `volume` rather than `arg0` is most of the difference between
/// generated code you keep and generated code you rename. A member decoded from a class file
/// usually carries no names, and falls back to `arg0`, `arg1` — the same synthesis the decompiled
/// source view uses.
pub fn parameters(m: &Member) -> Vec<(String, String)> {
    named_parameters(m)
        .into_iter()
        .enumerate()
        .map(|(i, (ty, name))| (ty, name.unwrap_or_else(|| format!("arg{i}"))))
        .collect()
}

/// The `(written type, real parameter name)` pairs — with `None` where the name is **not known**.
///
/// The distinction [`parameters`] erases, and it matters wherever the name is shown as a fact about
/// the code rather than used as a placeholder. A class file carries no parameter names unless it was
/// compiled with `-parameters`, so for most library methods there is genuinely no name to report:
///
/// * an **override stub** needs *a* name to write, and `arg0` is the honest placeholder — the same
///   one the decompiled-source view uses, and one the user immediately renames;
/// * an **inlay hint** or a **signature strip** is a claim about what the parameter is called, and
///   `get_genere(arg0: …)` states something false. The right answer there is to say nothing.
///
/// Which is why this exists: the two uses look identical and are opposites.
pub fn named_parameters(m: &Member) -> Vec<(String, Option<String>)> {
    let names = signature_param_names(&m.raw_signature, m.params.len());
    m.params
        .iter()
        .enumerate()
        .map(|(i, p)| (render_type(p), names.get(i).cloned().flatten()))
        .collect()
}

/// Pull parameter names out of a rendered signature (`int add(int a, int b)`), when it has them.
///
/// Returns one entry per parameter — `None` where the text carried no name. A bytecode signature
/// renders types alone, and a signature whose parameter count disagrees with the member's is not
/// describing this member, so both yield nothing rather than a wrong name.
pub fn signature_param_names(raw: &str, arity: usize) -> Vec<Option<String>> {
    let empty = vec![None; arity];
    let (Some(open), Some(close)) = (raw.find('('), raw.rfind(')')) else {
        return empty;
    };
    if close < open {
        return empty;
    }
    let inner = raw[open + 1..close].trim();
    if inner.is_empty() {
        return empty;
    }
    let parts = split_top_level(inner);
    if parts.len() != arity {
        return empty;
    }
    parts
        .into_iter()
        .map(|p| {
            let p = p.trim();
            // `Map<String, Integer> byName` — the name is the last whitespace-separated word, and
            // only when there IS one (a bare type is a type, not a name).
            let (_, name) = p.rsplit_once(char::is_whitespace)?;
            let name = name.trim_start_matches(['.', '@']).trim();
            (!name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$'))
                .then(|| name.to_string())
        })
        .collect()
}

/// Split a parameter list on commas that are not inside generics — `Map<String, Integer> m, int n`
/// is two parameters, not three.
pub fn split_top_level(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (i, c) in s.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => depth -= 1,
            ',' if depth == 0 => {
                out.push(&s[start..i]);
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    out.push(&s[start..]);
    out
}

/// A `TypeRef` as it should be written: simple name, generics kept.
pub fn render_type(t: &TypeRef) -> String {
    let simple = simple_of(&t.binary_name);
    if t.type_args.is_empty() {
        simple.to_string()
    } else {
        let args: Vec<String> = t.type_args.iter().map(render_type).collect();
        format!("{simple}<{}>", args.join(", "))
    }
}

/// The readable name of a binary name — `com/x/Outer$Inner` → `Inner`.
pub fn simple_of(binary: &str) -> &str {
    binary.rsplit(['/', '$']).next().unwrap_or(binary)
}

/// `speak(int volume) : String` — a member on one line, the shape a picker row wants.
pub fn render_signature(name: &str, params: &[(String, String)], ret: &str) -> String {
    let ps: Vec<String> = params.iter().map(|(t, n)| render_param(t, n)).collect();
    format!("{name}({}) : {ret}", ps.join(", "))
}

/// One rendered parameter — `int volume`, or just `int` when the name is not known.
///
/// The empty name is how [`named_parameters`] says "this member carries no name for it", and the
/// rendering has to mean the same thing: `int ` with a trailing space would read as a name nobody
/// can see. Shared with whatever locates the parameters inside a rendered signature, so the two
/// cannot disagree about where one ends.
pub fn render_param(ty: &str, name: &str) -> String {
    if name.is_empty() {
        ty.to_string()
    } else {
        format!("{ty} {name}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::{Member, TypeRef};

    #[test]
    fn names_come_from_the_rendered_signature() {
        let names = signature_param_names("int add(int a, int b)", 2);
        assert_eq!(names, vec![Some("a".to_string()), Some("b".to_string())]);
    }

    #[test]
    fn a_generic_parameter_is_one_parameter() {
        let names = signature_param_names("void put(Map<String, Integer> byName, int n)", 2);
        assert_eq!(names, vec![Some("byName".to_string()), Some("n".to_string())]);
    }

    #[test]
    fn a_bytecode_signature_carries_no_names() {
        assert_eq!(signature_param_names("add(int, int)", 2), vec![None, None]);
    }

    /// A signature whose arity disagrees is describing some other method — take nothing from it.
    #[test]
    fn a_mismatched_arity_yields_no_names() {
        assert_eq!(signature_param_names("int add(int a, int b)", 3), vec![None, None, None]);
    }

    #[test]
    fn a_type_renders_simple_with_its_generics() {
        let t = TypeRef {
            binary_name: "java/util/Map".into(),
            type_args: vec![
                TypeRef::simple("java/lang/String"),
                TypeRef::simple("com/acme/Order"),
            ],
        };
        assert_eq!(render_type(&t), "Map<String, Order>");
    }

    #[test]
    fn a_nested_type_renders_as_its_inner_name() {
        assert_eq!(simple_of("com/acme/Outer$Inner"), "Inner");
    }

    // ── the placeholder must not escape into a claim ──────────────────────────

    fn bytecode_method() -> Member {
        // A class file carries types and no names — the shape most library methods arrive in.
        Member::method(
            "get_genere",
            TypeRef::simple("java/lang/Integer"),
            vec![TypeRef::simple("java/lang/String")],
        )
        .sig("get_genere(String)")
    }

    fn source_method() -> Member {
        Member::method(
            "get_genere",
            TypeRef::simple("java/lang/Integer"),
            vec![TypeRef::simple("java/lang/String")],
        )
        .sig("Integer get_genere(String codice)")
    }

    /// A generated override needs *a* name to write, so the placeholder is right there.
    #[test]
    fn the_generator_gets_a_placeholder_when_there_is_no_name() {
        assert_eq!(parameters(&bytecode_method())[0].1, "arg0");
    }

    /// Anything that SHOWS the name must be able to tell "unknown" from "called arg0" — the two look
    /// identical in a hint and are opposites.
    #[test]
    fn a_name_that_is_not_known_is_reported_as_unknown() {
        assert_eq!(named_parameters(&bytecode_method())[0].1, None);
    }

    #[test]
    fn a_real_name_survives_both_ways() {
        assert_eq!(parameters(&source_method())[0].1, "codice");
        assert_eq!(named_parameters(&source_method())[0].1, Some("codice".to_string()));
    }

    /// `get_genere(String)`, not `get_genere(String )`.
    #[test]
    fn a_nameless_parameter_renders_as_its_type_alone() {
        assert_eq!(render_param("String", ""), "String");
        let params = vec![("String".to_string(), String::new())];
        assert_eq!(
            render_signature("get_genere", &params, "Integer"),
            "get_genere(String) : Integer"
        );
    }
}
