//! The positions beyond a declaration, an assignment, a `return` and a condition where the expression being written
//! has a type the surrounding code already fixes — read off the neighbours rather than off a declaration.
//!
//! * **A comparison** — `status == |`, `status != |`: the other operand's type. An enum on the left means the
//!   constants of that enum are what goes on the right.
//! * **A `case` label** — `case |` in a `switch` over `status`: the selector's type.
//! * **A ternary branch** — `c ? Status.ACTIVE : |`: the other branch, when it types; otherwise the walk goes on
//!   and the expected type of the whole ternary answers.
//! * **An array initializer** — `Foo[] xs = { a, | }`: the element of the declared array type.
//! * **A few arguments whose slot is knowable without overload resolution** — `a.equals(|)` wants whatever `a` is,
//!   and `List.of(|)`, `Set.of(|)`, `Arrays.asList(|)` and `Stream.of(|)` want the element of the collection the
//!   call is being assigned to.
//!
//! Every other argument slot stays unanswered (see [`crate::expected`]): which parameter is meant depends on which
//! overload binds, and a wrong answer there would rank confidently against the truth. The caller still stops the
//! walk at an argument list, so the declaration around a call is not mistaken for the slot inside it.

use super::*;

/// Collection factories whose arguments are elements of the collection the call produces.
const ELEMENT_FACTORIES: &[(&str, &str)] =
    &[("List", "of"), ("Set", "of"), ("Arrays", "asList"), ("Stream", "of"), ("EnumSet", "of")];

impl Ctx<'_> {
    fn type_of_operand(&self, operand: &Node) -> Option<TypeRef> {
        let enclosing = enclosing_type_fqn(operand, self.bytes, self.symbols);
        self.infer_expr(operand, enclosing.as_deref())
    }

    /// The operator of a binary expression, as written.
    pub(super) fn binary_operator(&self, binary: &Node) -> Option<String> {
        let (left, right) = (binary.child_by_field_name("left")?, binary.child_by_field_name("right")?);
        let text = std::str::from_utf8(self.bytes.get(left.end_byte()..right.start_byte())?).ok()?;
        Some(text.trim().to_string())
    }

    /// `a == |` and `| == a`: the type of the other side. Only the two equality operators — `<` and `+` constrain
    /// nothing a ranking could use.
    pub(super) fn expected_from_comparison(&self, binary: &Node, child: &Node) -> Option<TypeRef> {
        let (left, right) = (binary.child_by_field_name("left")?, binary.child_by_field_name("right")?);
        let op = self.binary_operator(binary)?;
        if op != "==" && op != "!=" {
            return None;
        }
        let other = if child.id() == right.id() {
            left
        } else if child.id() == left.id() {
            right
        } else {
            return None;
        };
        self.type_of_operand(&other)
    }

    /// `case |`: the type of what the enclosing `switch` is over.
    pub(super) fn expected_from_case(&self, label: &Node) -> Option<TypeRef> {
        let mut up = crate::parents::parent_of(label);
        while let Some(n) = up {
            if matches!(n.kind(), "switch_expression" | "switch_statement") {
                let selector = n.child_by_field_name("condition")?;
                let inner = if selector.kind() == "parenthesized_expression" { selector.named_child(0)? } else { selector };
                return self.type_of_operand(&inner);
            }
            up = crate::parents::parent_of(n);
        }
        None
    }

    /// A ternary branch: the type of the other branch, when it has one.
    pub(super) fn expected_from_ternary(&self, ternary: &Node, child: &Node) -> Option<TypeRef> {
        let (yes, no) = (ternary.child_by_field_name("consequence")?, ternary.child_by_field_name("alternative")?);
        if ternary.child_by_field_name("condition").is_some_and(|c| c.id() == child.id()) {
            return Some(TypeRef::simple("boolean"));
        }
        let other = if child.id() == yes.id() {
            no
        } else if child.id() == no.id() {
            yes
        } else {
            return None;
        };
        self.type_of_operand(&other)
    }

    /// `Foo[] xs = { | }` and `new Foo[] { | }`: the element of the array.
    pub(super) fn expected_from_array_initializer(&self, init: &Node) -> Option<TypeRef> {
        let parent = crate::parents::parent_of(init)?;
        let array = match parent.kind() {
            "variable_declarator" => {
                let text = node_text(&crate::parents::parent_of(parent)?.child_by_field_name("type")?, self.bytes)?;
                self.resolve_type_text(&text)?
            }
            "array_creation_expression" => {
                let mut element = self.resolve_type_text(&node_text(&parent.child_by_field_name("type")?, self.bytes)?)?;
                let dims = node_text(&parent.child_by_field_name("dimensions")?, self.bytes)?.matches('[').count();
                element.dims = element.dims.saturating_add(dims as u8);
                element
            }
            _ => return None,
        };
        (array.dims > 0).then(|| TypeRef { dims: array.dims - 1, ..array })
    }

    /// An argument whose slot needs no overload resolution: `a.equals(|)`, and the elements of a collection factory.
    /// `None` for every other call.
    pub(super) fn expected_from_argument(&self, args: &Node) -> Option<TypeRef> {
        let call = crate::parents::parent_of(args)?;
        if call.kind() != "method_invocation" {
            return None;
        }
        let name = node_text(&call.child_by_field_name("name")?, self.bytes)?;
        let object = call.child_by_field_name("object");
        if name == "equals" && args.named_child_count() == 1 {
            return self.type_of_operand(&object?);
        }
        let object_text = node_text(&object?, self.bytes)?;
        if !ELEMENT_FACTORIES.iter().any(|(o, m)| *o == object_text && *m == name) {
            return None;
        }
        let source = std::str::from_utf8(self.bytes).ok()?;
        let whole = super::expected_at(&self.root, source, self.symbols, call.start_byte(), self.resolver)?;
        match whole.type_args.as_slice() {
            [element] => Some(element.clone()),
            _ => None,
        }
    }
}
