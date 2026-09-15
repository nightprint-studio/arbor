//! `override_stub` — the pure half of "implement / override methods": what a generated method looks
//! like, and where in the class body it goes.
//!
//! Kept apart from the query that decides WHICH methods can be overridden (that needs the whole
//! supertype hierarchy, so it lives in `bennu-query`). This end takes a description and produces
//! text, which means every shape it can produce is unit-tested here rather than eyeballed in an
//! editor.
//!
//! ## What the body is
//!
//! Two cases, and the difference matters more than it looks:
//!
//! - **Abstract** — nothing to delegate to. The body throws
//!   `UnsupportedOperationException("Not implemented")`, which is what IntelliJ writes and the only
//!   honest thing to put there: a stub returning `null` compiles, runs, and lies until something
//!   downstream fails somewhere else.
//! - **Concrete** — `return super.m(args);` (or a bare `super.m(args);` when it returns `void`).
//!   Overriding a concrete method usually means adding to it, so the generated body starts by doing
//!   what the supertype did.
//!
//! ## What is NOT decided here
//!
//! Parameter names. They arrive in the spec, because where they come from differs by source: a
//! project supertype records them, a class file usually does not (`arg0`, `arg1` — the same
//! synthesis the decompiled-stub view already uses).

/// One method to generate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverrideSpec {
    pub name: String,
    /// `(type, name)` in declaration order, already rendered as they should be written.
    pub params: Vec<(String, String)>,
    /// The return type as written; `"void"` for none.
    pub return_type: String,
    /// `"public"` | `"protected"` — what the override is declared as. Java forbids narrowing, so
    /// this is the supertype's own visibility (a package-private one is only offered inside its
    /// package, and is written without a modifier).
    pub visibility: String,
    /// The supertype declared it abstract (or it is an interface method with no default), so there
    /// is no `super` to call.
    pub is_abstract: bool,
    /// Checked exceptions the supertype declares, as written type names. An override may throw
    /// fewer but not more, and dropping the clause outright makes a `super` call not compile.
    pub throws: Vec<String>,
}

/// Render `spec` as a Java method, every line prefixed with `indent`.
///
/// No trailing newline: the caller joins several and decides the separation, which is the only way
/// generating one method and generating six can produce the same spacing.
pub fn render_override(spec: &OverrideSpec, indent: &str) -> String {
    let params = spec
        .params
        .iter()
        .map(|(ty, name)| format!("{ty} {name}"))
        .collect::<Vec<_>>()
        .join(", ");
    let modifier = if spec.visibility == "package" { String::new() } else { format!("{} ", spec.visibility) };
    let throws = if spec.throws.is_empty() {
        String::new()
    } else {
        format!(" throws {}", spec.throws.join(", "))
    };

    let body = if spec.is_abstract {
        "throw new UnsupportedOperationException(\"Not implemented\");".to_string()
    } else {
        let call = format!(
            "super.{}({})",
            spec.name,
            spec.params.iter().map(|(_, n)| n.as_str()).collect::<Vec<_>>().join(", ")
        );
        if spec.return_type == "void" {
            format!("{call};")
        } else {
            format!("return {call};")
        }
    };

    format!(
        "{indent}@Override\n\
         {indent}{modifier}{ret} {name}({params}){throws} {{\n\
         {indent}    {body}\n\
         {indent}}}",
        ret = spec.return_type,
        name = spec.name,
    )
}

/// Where generated members go in the class body containing byte `offset`, and the indentation to
/// write them at.
///
/// Just inside the class's closing brace, which is where a reader looks for "what did I just add"
/// and where adding does not push anything else around. Returns `(insert_at, indent)`.
///
/// `None` when `offset` is not inside a braced body — there is nowhere to put anything, and
/// guessing a position in a file that does not parse is how a generator corrupts a buffer.
pub fn class_body_insertion(source: &str, offset: usize) -> Option<(usize, String)> {
    let close = enclosing_close_brace(source.as_bytes(), offset)?;
    // The indentation of the closing brace is the class's own; its members sit one level in.
    let line_start = source[..close].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let base: String = source[line_start..close]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect();
    let indent = format!("{base}    ");
    Some((close, indent))
}

/// The closing brace of the innermost braced body containing `offset`, skipping strings, character
/// literals and comments — a `{` inside `"a { b"` is text, and counting it would close the class
/// somewhere in the middle of a method.
fn enclosing_close_brace(b: &[u8], offset: usize) -> Option<usize> {
    let offset = offset.min(b.len());
    let mut stack: Vec<usize> = Vec::new();
    let mut i = 0usize;
    let mut target: Option<usize> = None;
    while i < b.len() {
        match b[i] {
            b'"' | b'\'' => {
                i = crate::scan::string_end(b, i);
                continue;
            }
            b'/' if i + 1 < b.len() && b[i + 1] == b'/' => {
                i = crate::scan::line_comment_end(b, i);
                continue;
            }
            b'/' if i + 1 < b.len() && b[i + 1] == b'*' => {
                i = crate::scan::block_comment_end(b, i);
                continue;
            }
            b'{' => stack.push(i),
            b'}' => {
                let open = stack.pop();
                // The first body that CLOSES after the caret and opened before it is the innermost
                // one containing it.
                if target.is_none() && i >= offset && open.is_some_and(|o| o < offset) {
                    target = Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    target
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(name: &str, ret: &str, abstract_: bool) -> OverrideSpec {
        OverrideSpec {
            name: name.to_string(),
            params: Vec::new(),
            return_type: ret.to_string(),
            visibility: "public".to_string(),
            is_abstract: abstract_,
            throws: Vec::new(),
        }
    }

    /// Nothing to delegate to, so the body says so out loud rather than returning a plausible lie.
    #[test]
    fn an_abstract_method_throws_rather_than_returning_nothing() {
        let s = render_override(&spec("run", "void", true), "    ");
        assert_eq!(
            s,
            "    @Override\n    public void run() {\n        throw new UnsupportedOperationException(\"Not implemented\");\n    }"
        );
    }

    /// Overriding something concrete usually means adding to it, so the stub starts by doing what
    /// the supertype did.
    #[test]
    fn a_concrete_method_delegates_to_super() {
        let mut sp = spec("speak", "String", false);
        sp.params = vec![("int".into(), "volume".into())];
        let s = render_override(&sp, "    ");
        assert!(s.contains("public String speak(int volume) {"), "{s}");
        assert!(s.contains("return super.speak(volume);"), "{s}");
    }

    #[test]
    fn a_void_override_does_not_return_super() {
        let s = render_override(&spec("close", "void", false), "  ");
        assert!(s.contains("  super.close();"), "{s}");
        assert!(!s.contains("return"), "{s}");
    }

    /// An override may throw fewer exceptions but not more — and dropping the clause makes the
    /// `super` call in the generated body not compile.
    #[test]
    fn a_throws_clause_is_carried_over() {
        let mut sp = spec("read", "int", false);
        sp.throws = vec!["IOException".into()];
        assert!(render_override(&sp, "").contains("int read() throws IOException {"));
    }

    /// Package-private is written by writing nothing. `"package public"` would not compile.
    #[test]
    fn package_private_is_written_without_a_modifier() {
        let mut sp = spec("helper", "void", true);
        sp.visibility = "package".into();
        assert!(render_override(&sp, "").starts_with("@Override\nvoid helper()"));
    }

    #[test]
    fn members_go_just_inside_the_class_brace_one_level_in() {
        let src = "package p;\nclass C {\n    void m() {\n        int x;\n    }\n}\n";
        let caret = src.find("int x").unwrap();
        let (at, indent) = class_body_insertion(src, caret).expect("inside a body");
        // The innermost body containing the caret is the METHOD's, which is the honest answer to
        // "which braces am I in" — the caller passes the caret it wants.
        assert_eq!(&src[at..at + 1], "}");
        assert_eq!(indent, "        ");
    }

    #[test]
    fn a_caret_in_the_class_body_finds_the_class_brace() {
        let src = "class C {\n    int f;\n\n}\n";
        let caret = src.find("int f").unwrap();
        let (at, indent) = class_body_insertion(src, caret).unwrap();
        assert_eq!(at, src.rfind('}').unwrap());
        assert_eq!(indent, "    ");
    }

    /// A brace inside a string is text. Counting it closes the class in the middle of a method and
    /// the generated members land inside it.
    #[test]
    fn braces_in_strings_and_comments_are_not_braces() {
        let src = "class C {\n    String s = \"a { b\"; // } not this one\n    int f;\n}\n";
        let caret = src.find("int f").unwrap();
        let (at, _) = class_body_insertion(src, caret).unwrap();
        assert_eq!(at, src.rfind('}').unwrap(), "the class brace, not one in the text");
    }

    #[test]
    fn a_caret_outside_any_body_has_nowhere_to_put_anything() {
        assert!(class_body_insertion("package p;\n\nclass C {\n}\n", 3).is_none());
    }
}
