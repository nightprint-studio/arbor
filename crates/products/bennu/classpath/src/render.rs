//! A decoded bytecode signature, written back as the Java a person reads.
//!
//! ## Why this is a module and not three functions in a consumer
//!
//! A `.class` carries a member's type as a JVMS §4.7.9.1 `Signature` string —
//! `<X:Ljava/lang/Throwable;>(Ljava/util/function/Supplier<+TX;>;)TT;^TX;` — and
//! [`Member::raw_signature`](crate::members::Member::raw_signature) keeps it verbatim, on purpose:
//! it is the only lossless form, and a consumer that needs the generics can re-decode it.
//!
//! What every consumer then needs is the *same* sentence back in Java, and there were two of them
//! deriving it separately. The decompiled source view rendered
//! `<X extends Throwable> T orElseThrow(Supplier<? extends X> arg0) throws X`; the hover card
//! rendered nothing at all and printed the raw string, so pointing at `orElseThrow` produced a line
//! of JVM descriptor. One of them was right and the other was a bug precisely because the rendering
//! lived inside the one that was right.
//!
//! ## Simple names, and why
//!
//! Every type is written by its simple name (`Supplier`, not `java.util.function.Supplier`). A
//! signature is read at a glance — in a stub, in a tooltip, in a one-line strip — and the package is
//! either already known or available on the line below it. Fully-qualified names in that position
//! push the part being read off the edge.

use crate::sig::{MethodSig, TypeArg, TypeParam, TypeSig};

/// Whether `raw` is a **bytecode** signature (or descriptor) rather than a signature already written
/// in Java.
///
/// The distinction matters because `raw_signature` carries both: a member decoded from a `.class`
/// holds the JVM string, and one built from project source holds text a person wrote. A method's
/// JVM form always opens with the parameter list `(` or with its type parameters `<`, and no Java
/// declaration ever starts with either — it starts with a modifier, a type or a name. A field's form
/// is a single type descriptor, which starts with one of the base-type letters, `L`, `T` or `[`; the
/// letters are ambiguous with a one-character Java type name, so a field is only recognised by the
/// unambiguous prefixes.
pub fn is_bytecode_method(raw: &str) -> bool {
    raw.starts_with('(') || raw.starts_with('<')
}

/// Whether `raw` is a bytecode **field** descriptor/signature. See [`is_bytecode_method`].
pub fn is_bytecode_field(raw: &str) -> bool {
    // `Ljava/lang/String;` and `TT;` are only descriptors when they end in `;` — which a written
    // Java type never does. `[` opens an array descriptor and nothing else. A lone base-type letter
    // is the third form, and it is unambiguous here for a reason that is not about the letter: a
    // field written in Java is a type *and a name*, so one token is never a declaration.
    const BASE: &str = "BCDFIJSZ";
    raw.starts_with('[')
        || (raw.ends_with(';') && raw.starts_with(['L', 'T']))
        || (raw.len() == 1 && BASE.contains(raw))
}

/// The method "core" — `<TypeParams> Ret name(Params) throws X`, no modifiers and no body — decoded
/// from a bytecode signature.
///
/// `None` when `raw` does not decode, which includes every signature that is already Java: a caller
/// falls back to what it had. `ctor_simple` is `Some(simpleClassName)` for an `<init>`, which renders
/// as `Simple(...)` with no return type.
///
/// `param_names`, when it has an entry, names that parameter — the caller supplies them because a
/// class file does not carry them (`-parameters` is off by default) and only the caller knows
/// whether it has real ones. An entry that is `None` (or a short list) renders the type alone rather
/// than inventing `arg0`, which would state something the code does not say.
pub fn method_core(
    raw: &str,
    name: &str,
    ctor_simple: Option<&str>,
    param_names: &[Option<String>],
) -> Option<String> {
    if !is_bytecode_method(raw) {
        return None;
    }
    let ms = crate::sig::parse_method(raw).ok()?;
    Some(method_core_of(&ms, name, ctor_simple, param_names))
}

/// [`method_core`] over an already-decoded signature.
pub fn method_core_of(
    ms: &MethodSig,
    name: &str,
    ctor_simple: Option<&str>,
    param_names: &[Option<String>],
) -> String {
    let type_params = if ms.type_params.is_empty() {
        String::new()
    } else {
        let ps: Vec<String> = ms.type_params.iter().map(type_param).collect();
        format!("<{}> ", ps.join(", "))
    };
    let params: Vec<String> = ms
        .params
        .iter()
        .enumerate()
        .map(|(i, p)| match param_names.get(i).and_then(Option::as_deref) {
            Some(n) => format!("{} {n}", type_sig(p)),
            None => type_sig(p),
        })
        .collect();
    let throws = if ms.throws.is_empty() {
        String::new()
    } else {
        let ts: Vec<String> = ms.throws.iter().map(type_sig).collect();
        format!(" throws {}", ts.join(", "))
    };
    let head = match ctor_simple {
        Some(simple) => format!("{type_params}{simple}"),
        None => format!("{type_params}{} {name}", type_sig(&ms.result)),
    };
    format!("{head}({}){throws}", params.join(", "))
}

/// Placeholder parameter names — `arg0`, `arg1`, … — for a caller that must write *a* name.
///
/// A code generator needs one (an override stub with a nameless parameter does not compile); a
/// tooltip must not use one, because `arg0` is the decompiler's invention and showing it claims the
/// parameter is called that. The two uses look identical, which is why the placeholder is a
/// deliberate call rather than a default.
pub fn placeholder_names(count: usize) -> Vec<Option<String>> {
    (0..count).map(|i| Some(format!("arg{i}"))).collect()
}

/// A field's declared type, decoded from its bytecode descriptor/signature. `None` when `raw` is not
/// one (see [`is_bytecode_field`]).
pub fn field_type(raw: &str) -> Option<String> {
    if !is_bytecode_field(raw) {
        return None;
    }
    crate::sig::parse_field(raw).ok().map(|t| type_sig(&t))
}

/// A decoded type, written as readable simple-name Java: `Supplier<? extends X>`, `List<String>`,
/// `T`, `int[]`.
pub fn type_sig(t: &TypeSig) -> String {
    match t {
        TypeSig::Base(c) => match c {
            'I' => "int",
            'J' => "long",
            'S' => "short",
            'B' => "byte",
            'C' => "char",
            'Z' => "boolean",
            'F' => "float",
            'D' => "double",
            _ => "Object",
        }
        .to_string(),
        TypeSig::Void => "void".to_string(),
        TypeSig::TypeVar(n) => n.clone(),
        TypeSig::Array(inner) => format!("{}[]", type_sig(inner)),
        TypeSig::Class(ct) => {
            let mut s = ct.name.rsplit('.').next().unwrap_or(&ct.name).to_string();
            if !ct.args.is_empty() {
                let a: Vec<String> = ct.args.iter().map(type_arg).collect();
                s.push_str(&format!("<{}>", a.join(", ")));
            }
            for (iname, iargs) in &ct.inners {
                s.push('.');
                s.push_str(iname);
                if !iargs.is_empty() {
                    let a: Vec<String> = iargs.iter().map(type_arg).collect();
                    s.push_str(&format!("<{}>", a.join(", ")));
                }
            }
            s
        }
    }
}

/// One `<…>` type argument — `?`, `? extends X`, `? super X`, or an exact type.
pub fn type_arg(a: &TypeArg) -> String {
    match a {
        TypeArg::Unbounded => "?".to_string(),
        TypeArg::Extends(t) => format!("? extends {}", type_sig(t)),
        TypeArg::Super(t) => format!("? super {}", type_sig(t)),
        TypeArg::Exact(t) => type_sig(t),
    }
}

/// A method/class type parameter — `X extends Throwable`, `T` — with a vacuous `extends Object`
/// bound suppressed, since every type parameter has it and writing it says nothing.
pub fn type_param(tp: &TypeParam) -> String {
    let mut bounds: Vec<String> = Vec::new();
    if let Some(cb) = &tp.class_bound {
        let s = type_sig(cb);
        if s != "Object" {
            bounds.push(s);
        }
    }
    for ib in &tp.interface_bounds {
        bounds.push(type_sig(ib));
    }
    if bounds.is_empty() {
        tp.name.clone()
    } else {
        format!("{} extends {}", tp.name, bounds.join(" & "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The signature that started this: pointing at `Optional.orElseThrow` used to print the
    /// descriptor verbatim.
    #[test]
    fn the_optional_signature_reads_as_java() {
        let raw = "<X:Ljava/lang/Throwable;>(Ljava/util/function/Supplier<+TX;>;)TT;^TX;";
        assert_eq!(
            method_core(raw, "orElseThrow", None, &[]).unwrap(),
            "<X extends Throwable> T orElseThrow(Supplier<? extends X>) throws X"
        );
    }

    #[test]
    fn placeholders_are_written_only_when_asked_for() {
        let raw = "(Ljava/lang/String;I)V";
        assert_eq!(method_core(raw, "put", None, &[]).unwrap(), "void put(String, int)");
        assert_eq!(
            method_core(raw, "put", None, &placeholder_names(2)).unwrap(),
            "void put(String arg0, int arg1)"
        );
    }

    #[test]
    fn a_constructor_renders_as_its_class_name() {
        let raw = "(Ljava/lang/String;)V";
        assert_eq!(method_core(raw, "<init>", Some("Order"), &[]).unwrap(), "Order(String)");
    }

    /// The whole point of the sniff: a signature that is already Java must come back untouched, so
    /// a project member keeps the names its source carries.
    #[test]
    fn a_written_java_signature_is_not_a_bytecode_one() {
        assert!(!is_bytecode_method("String orElseThrow(Supplier<X> supplier)"));
        assert_eq!(method_core("int add(int a, int b)", "add", None, &[]), None);
        assert!(!is_bytecode_field("List<String> names"));
        assert!(!is_bytecode_field("T"));
    }

    #[test]
    fn a_field_descriptor_reads_as_its_type() {
        assert_eq!(field_type("Ljava/util/List<Ljava/lang/String;>;").unwrap(), "List<String>");
        assert_eq!(field_type("[I").unwrap(), "int[]");
        assert_eq!(field_type("TT;").unwrap(), "T");
        assert_eq!(field_type("I").unwrap(), "int");
    }
}
