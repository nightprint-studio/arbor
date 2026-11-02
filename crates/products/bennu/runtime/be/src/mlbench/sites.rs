//! Where a type-driven completion would be asked something, read from the parse tree of code that is already written:
//! the right side of a `=`, the value of a `return`, the argument of a call, the member after a dot. The code is the label.

use bennu_java::prelude::parse_java;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub(super) enum Position {
    AfterAssign,
    AfterReturn,
    CallArgument,
    MemberAfterDot,
}

impl Position {
    pub(super) fn label(self) -> &'static str {
        match self {
            Position::AfterAssign => "after =",
            Position::AfterReturn => "after return",
            Position::CallArgument => "call argument",
            Position::MemberAfterDot => "member after dot",
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct Site {
    pub position: Position,
    /// Byte range of the thing that was written there: the expression, the argument, or the member's name.
    pub start: usize,
    pub end: usize,
    /// The declared type text when the position states one: a variable's type, the method's return type.
    pub declared: Option<String>,
    /// For an argument: the callee's name and the argument's index; for a member: the name of the previous call of the chain.
    pub callee: Option<String>,
    pub arg_index: usize,
    /// For an assignment: the name written on the left. For a member: the receiver expression.
    pub target: Option<String>,
}

// `tree_sitter::Node` is not a dependency of this crate, so the helpers over nodes are macros: the type is inferred.
macro_rules! text {
    ($src:expr, $n:expr) => {
        &$src[$n.start_byte()..$n.end_byte()]
    };
}

/// The declared return type of the method enclosing `$n`, `None` in a lambda or outside a method.
macro_rules! enclosing_return_type {
    ($n:expr, $src:expr) => {{
        let mut cur = $n.parent();
        let mut found: Option<String> = None;
        while let Some(p) = cur {
            match p.kind() {
                "lambda_expression" | "class_body" => break,
                "method_declaration" => {
                    found = p.child_by_field_name("type").map(|t| text!($src, t).to_string()).filter(|t| t != "void");
                    break;
                }
                _ => cur = p.parent(),
            }
        }
        found
    }};
}

/// Every site in `src`, in source order.
pub(super) fn sites(src: &str) -> Vec<Site> {
    let Some(tree) = parse_java(src) else { return Vec::new() };
    let mut out = Vec::new();
    let mut stack = vec![tree.root_node()];
    while let Some(n) = stack.pop() {
        let mut c = n.walk();
        let children: Vec<_> = n.children(&mut c).collect();
        stack.extend(children.iter().rev().copied());
        match n.kind() {
            "local_variable_declaration" | "field_declaration" => {
                let declared = n.child_by_field_name("type").map(|t| text!(src, t).to_string());
                let mut c2 = n.walk();
                for d in n.children(&mut c2).filter(|d| d.kind() == "variable_declarator") {
                    let Some(v) = d.child_by_field_name("value") else { continue };
                    if matches!(v.kind(), "array_initializer") {
                        continue;
                    }
                    let name = d.child_by_field_name("name").map(|x| text!(src, x).to_string());
                    let declared = declared.clone().filter(|t| t != "var");
                    out.push(Site { position: Position::AfterAssign, start: v.start_byte(), end: v.end_byte(), declared, callee: None, arg_index: 0, target: name });
                }
            }
            "assignment_expression" => {
                if let (Some(l), Some(r)) = (n.child_by_field_name("left"), n.child_by_field_name("right")) {
                    let plain = src[l.end_byte()..r.start_byte()].trim() == "=";
                    if plain {
                        out.push(Site { position: Position::AfterAssign, start: r.start_byte(), end: r.end_byte(), declared: None, callee: None, arg_index: 0, target: Some(text!(src, l).to_string()) });
                    }
                }
            }
            "return_statement" => {
                let mut c2 = n.walk();
                let value = n.children(&mut c2).find(|x| x.is_named() && x.kind() != "line_comment" && x.kind() != "block_comment");
                if let Some(v) = value {
                    out.push(Site { position: Position::AfterReturn, start: v.start_byte(), end: v.end_byte(), declared: enclosing_return_type!(n, src), callee: None, arg_index: 0, target: None });
                }
            }
            "method_invocation" | "object_creation_expression" => {
                let callee = if n.kind() == "method_invocation" { n.child_by_field_name("name") } else { n.child_by_field_name("type") }.map(|x| text!(src, x).to_string());
                if let Some(args) = n.child_by_field_name("arguments") {
                    let mut c2 = args.walk();
                    let list: Vec<_> = args.children(&mut c2).filter(|a| a.is_named() && !a.kind().ends_with("comment")).collect();
                    for (i, a) in list.iter().enumerate() {
                        out.push(Site { position: Position::CallArgument, start: a.start_byte(), end: a.end_byte(), declared: None, callee: callee.clone(), arg_index: i, target: None });
                    }
                }
                if n.kind() == "method_invocation" {
                    if let (Some(recv), Some(name)) = (n.child_by_field_name("object"), n.child_by_field_name("name")) {
                        let previous = (recv.kind() == "method_invocation").then(|| recv.child_by_field_name("name").map(|x| text!(src, x).to_string())).flatten();
                        out.push(Site { position: Position::MemberAfterDot, start: name.start_byte(), end: name.end_byte(), declared: None, callee: previous, arg_index: 0, target: Some(text!(src, recv).to_string()) });
                    }
                }
            }
            "field_access" => {
                if let (Some(recv), Some(f)) = (n.child_by_field_name("object"), n.child_by_field_name("field")) {
                    let previous = (recv.kind() == "method_invocation").then(|| recv.child_by_field_name("name").map(|x| text!(src, x).to_string())).flatten();
                    out.push(Site { position: Position::MemberAfterDot, start: f.start_byte(), end: f.end_byte(), declared: None, callee: previous, arg_index: 0, target: Some(text!(src, recv).to_string()) });
                }
            }
            _ => {}
        }
    }
    out.sort_by_key(|s| (s.start, s.end));
    out
}

/// What the right-hand side is, in the six families the sample is described by.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub(super) enum Shape {
    Variable,
    StaticCall,
    New,
    OwnCall,
    InstanceCall,
    Literal,
    Lambda,
    Other,
}

impl Shape {
    pub(super) fn label(self) -> &'static str {
        match self {
            Shape::Variable => "variable / field",
            Shape::StaticCall => "`Type.method(...)`",
            Shape::New => "`new Type(...)`",
            Shape::OwnCall => "call of the own class",
            Shape::InstanceCall => "`value.method(...)`",
            Shape::Literal => "literal",
            Shape::Lambda => "lambda / method ref",
            Shape::Other => "other",
        }
    }
}

macro_rules! shape_of_node {
    ($v:expr, $src:expr) => {{
        let (v, src) = (&$v, &$src);
    match v.kind() {
        "identifier" | "this" => Shape::Variable,
        "field_access" => {
            let o = v.child_by_field_name("object").map(|o| o.kind());
            if o == Some("identifier") || o == Some("this") { Shape::Variable } else { Shape::Other }
        }
        "object_creation_expression" => Shape::New,
        "lambda_expression" | "method_reference" => Shape::Lambda,
        "method_invocation" => match v.child_by_field_name("object") {
            None => Shape::OwnCall,
            Some(o) if o.kind() == "identifier" && text!(src, o).starts_with(|c: char| c.is_ascii_uppercase()) => Shape::StaticCall,
            Some(o) if o.kind() == "this" => Shape::OwnCall,
            Some(_) => Shape::InstanceCall,
        },
        "decimal_integer_literal" | "hex_integer_literal" | "decimal_floating_point_literal" | "string_literal" | "character_literal" | "true" | "false" | "null_literal" | "binary_integer_literal" | "octal_integer_literal" | "text_block" => Shape::Literal,
        "parenthesized_expression" => Shape::Other,
        _ => Shape::Other,
    }
    }};
}

/// The shape of the expression `expr` (source text of one expression).
pub(super) fn shape_of(expr: &str) -> Shape {
    let wrapped = format!("class C {{ Object o = {expr}; }}");
    let Some(tree) = parse_java(&wrapped) else { return Shape::Other };
    let mut stack = vec![tree.root_node()];
    while let Some(n) = stack.pop() {
        if n.kind() == "variable_declarator" {
            let Some(v) = n.child_by_field_name("value") else { return Shape::Other };
            return shape_of_node!(v, wrapped);
        }
        let mut c = n.walk();
        stack.extend(n.children(&mut c).collect::<Vec<_>>());
    }
    Shape::Other
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_four_positions_are_found_with_what_they_state() {
        let src = "class A { String f(int n) { String s = make(n); s = other.get(n, 1); return s.trim(); } }";
        let found = sites(src);
        let at = |p: Position| found.iter().filter(|s| s.position == p).collect::<Vec<_>>();
        let assigns = at(Position::AfterAssign);
        assert_eq!(assigns.len(), 2);
        assert_eq!(assigns[0].declared.as_deref(), Some("String"));
        assert_eq!(&src[assigns[0].start..assigns[0].end], "make(n)");
        assert_eq!(assigns[1].target.as_deref(), Some("s"));
        let ret = at(Position::AfterReturn);
        assert_eq!(ret[0].declared.as_deref(), Some("String"));
        let args = at(Position::CallArgument);
        assert!(args.iter().any(|a| a.callee.as_deref() == Some("get") && a.arg_index == 1));
        let members = at(Position::MemberAfterDot);
        assert!(members.iter().any(|m| &src[m.start..m.end] == "trim" && m.target.as_deref() == Some("s")));
    }

    #[test]
    fn a_return_inside_a_lambda_states_no_type() {
        let src = "class A { Object f() { run(() -> { return 1; }); return null; } }";
        let rets: Vec<_> = sites(src).into_iter().filter(|s| s.position == Position::AfterReturn).collect();
        assert_eq!(rets.len(), 2);
        assert!(rets[0].declared.is_none());
        assert_eq!(rets[1].declared.as_deref(), Some("Object"));
    }

    #[test]
    fn the_shape_of_the_right_side() {
        assert_eq!(shape_of("name"), Shape::Variable);
        assert_eq!(shape_of("Objects.requireNonNull(x)"), Shape::StaticCall);
        assert_eq!(shape_of("new ArrayList<>()"), Shape::New);
        assert_eq!(shape_of("compute(x)"), Shape::OwnCall);
        assert_eq!(shape_of("list.size()"), Shape::InstanceCall);
        assert_eq!(shape_of("\"text\""), Shape::Literal);
        assert_eq!(shape_of("x -> x + 1"), Shape::Lambda);
        assert_eq!(shape_of("a + b"), Shape::Other);
    }

    #[test]
    fn a_chain_remembers_the_call_before_the_dot() {
        let src = "class A { void f() { x.trim().length(); } }";
        let m: Vec<_> = sites(src).into_iter().filter(|s| s.position == Position::MemberAfterDot).collect();
        assert!(m.iter().any(|s| &src[s.start..s.end] == "length" && s.callee.as_deref() == Some("trim")));
    }
}
