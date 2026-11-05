//! The evaluator: an [`Expr`] and a [`DebugTarget`] in, a value or a reason out.
//!
//! The target reads; this decides. Every rule about what an operation *means* — a null halfway
//! down a path, a subscript that is not a whole number, `&&` short-circuiting so
//! `order != null && order.total > 100` is a condition anyone can write — lives here once, so the
//! Java and the Rust debugger cannot drift apart on it.
//!
//! ## Names that are not variables
//!
//! `Status.ACTIVE` is a field access on a name — and `Status` is not a variable. When a name chain
//! does not start at a variable the evaluator asks the target for a **static field** with the
//! longest qualifier it can form (`com.acme.Status` + `ACTIVE`), one level at a time, so
//! `Status.ACTIVE.name` and `com.acme.Status.ACTIVE` both work and `order.status` (where `order` *is*
//! a variable) never goes looking for a class.

use crate::ast::{BinOp, Expr, Lit};
use crate::ops;
use crate::syntax::Syntax;
use crate::target::DebugTarget;
use crate::value::{Prim, Val};

/// Evaluate an expression in the frame the target is looking at.
pub fn eval<T: DebugTarget>(
    expr: &Expr,
    target: &T,
    syntax: Syntax,
) -> Result<Val<T::Ref>, String> {
    Evaluator { target, syntax }.value(expr)
}

/// Evaluate an expression that has to be a **boolean** — a breakpoint condition.
///
/// The error for anything else names what it was, because "stop when `order`" has no meaning anyone
/// would agree on, and a condition that cannot be answered is turned into a stop by the caller.
pub fn eval_condition<T: DebugTarget>(
    expr: &Expr,
    target: &T,
    syntax: Syntax,
) -> Result<bool, String> {
    let evaluator = Evaluator { target, syntax };
    let value = evaluator.value(expr)?;
    match evaluator.settle(value)? {
        Val::Prim(Prim::Bool(b)) => Ok(b),
        other => Err(format!(
            "that is {} — a condition on its own has to be a boolean, so compare it",
            other.describe()
        )),
    }
}

struct Evaluator<'a, T: DebugTarget> {
    target: &'a T,
    syntax: Syntax,
}

fn literal<R>(lit: &Lit) -> Val<R> {
    match lit {
        Lit::Int(v) => Val::Prim(Prim::Int(*v)),
        Lit::Long(v) => Val::Prim(Prim::Long(*v)),
        Lit::Double(v) => Val::Prim(Prim::Double(*v)),
        Lit::Char(c) => Val::Prim(Prim::Char(*c)),
        Lit::Bool(b) => Val::Prim(Prim::Bool(*b)),
        Lit::Str(s) => Val::Str(s.clone()),
        Lit::Null => Val::Null,
    }
}

fn unresolved(expr: &Expr) -> String {
    match expr.qualified_names() {
        Some(names) if names.len() == 1 => {
            format!("no variable or field named `{}` here", names[0])
        }
        Some(names) => format!(
            "`{}` is not a variable, and no loaded class declares it as a static field — the class \
             may not be loaded yet",
            names.join(".")
        ),
        None => format!("`{expr}` cannot be resolved"),
    }
}

impl<T: DebugTarget> Evaluator<'_, T> {
    fn value(&self, expr: &Expr) -> Result<Val<T::Ref>, String> {
        match self.resolve(expr)? {
            Some(value) => Ok(value),
            None => Err(unresolved(expr)),
        }
    }

    /// `None` only for a `Name` / `.field` chain that is neither a variable nor a known static.
    fn resolve(&self, expr: &Expr) -> Result<Option<Val<T::Ref>>, String> {
        match expr {
            Expr::Lit(lit) => Ok(Some(literal(lit))),
            Expr::Name(name) => self.target.variable(name),
            Expr::Field(base, name) => match self.resolve(base)? {
                Some(owner) => self.member(owner, base, name).map(Some),
                None => self.static_member(expr),
            },
            Expr::Index(base, at) => {
                let owner = self.value(base)?;
                let at = self.whole_number(at)?;
                self.element(owner, base, at).map(Some)
            }
            Expr::Deref(base) => {
                let pointer = self.value(base)?;
                self.pointee(pointer, base).map(Some)
            }
            Expr::Unary(op, inner) => {
                let value = self.value(inner)?;
                let value = self.settle(value)?;
                ops::unary(self.syntax.lang, *op, value).map(Some)
            }
            Expr::Binary(BinOp::And | BinOp::Or, ..) => self.logical(expr).map(Some),
            Expr::Binary(op, a, b) => {
                let (left, right) = (self.value(a)?, self.value(b)?);
                // `x == null` compares the reference itself: settling it would read a string or
                // unbox a box for nothing, and a null side needs neither.
                let against_null = matches!(left, Val::Null) || matches!(right, Val::Null);
                let (left, right) = if op.is_comparison() && against_null {
                    (left, right)
                } else {
                    (self.settle(left)?, self.settle(right)?)
                };
                ops::binary(self.syntax.lang, *op, left, right).map(Some)
            }
            Expr::Ternary(cond, then, otherwise) => {
                let chosen = if self.truth(cond, "`?:`")? { then } else { otherwise };
                self.value(chosen).map(Some)
            }
            Expr::Call { callee, args } => self.call(callee, args).map(Some),
        }
    }

    fn member(&self, owner: Val<T::Ref>, base: &Expr, name: &str) -> Result<Val<T::Ref>, String> {
        match owner {
            Val::Ref(r) => self.target.field(&r, name, &base.to_string()),
            Val::Null => Err(format!("`{base}` is null, so there is no `.{name}` to read")),
            Val::Str(_) => Err(format!("`{base}` is a string — it has no field `{name}`")),
            Val::Prim(_) => Err(format!("`{base}` is a primitive — there is nothing inside it")),
        }
    }

    fn element(&self, owner: Val<T::Ref>, base: &Expr, at: i64) -> Result<Val<T::Ref>, String> {
        match owner {
            Val::Ref(r) => self.target.index(&r, at, &base.to_string()),
            Val::Null => Err(format!("`{base}` is null, so there is no `[{at}]` to read")),
            other => Err(format!("`{base}` is {} — it has no elements", other.describe())),
        }
    }

    fn pointee(&self, pointer: Val<T::Ref>, base: &Expr) -> Result<Val<T::Ref>, String> {
        match pointer {
            Val::Ref(r) => self.target.deref(&r, &base.to_string()),
            Val::Null => Err(format!("`{base}` is null, so there is nothing to read through `*`")),
            other => Err(format!("`*` needs a reference, and `{base}` is {}", other.describe())),
        }
    }

    /// `Status.ACTIVE` where `Status` was not a variable: the last name is a static of the type the
    /// ones before it spell.
    fn static_member(&self, expr: &Expr) -> Result<Option<Val<T::Ref>>, String> {
        let Some(mut names) = expr.qualified_names() else { return Ok(None) };
        let Some(name) = names.pop() else { return Ok(None) };
        self.target.static_field(&names, &name)
    }

    /// An index or subscript: a whole number, however the language spells its width.
    fn whole_number(&self, expr: &Expr) -> Result<i64, String> {
        let value = self.value(expr)?;
        match self.settle(value)? {
            Val::Prim(Prim::Int(v)) => Ok(i64::from(v)),
            Val::Prim(Prim::Long(v)) => Ok(v),
            Val::Prim(Prim::Char(c)) if self.syntax.lang == crate::syntax::Lang::Java => {
                Ok(i64::from(c))
            }
            other => Err(format!(
                "a subscript has to be a whole number, and `{expr}` is {}",
                other.describe()
            )),
        }
    }

    /// `&&` and `||`: the right side is not evaluated when the left decides.
    fn logical(&self, expr: &Expr) -> Result<Val<T::Ref>, String> {
        let Expr::Binary(op, a, b) = expr else { return self.value(expr) };
        let word = format!("`{}`", op.word());
        let left = self.truth(a, &word)?;
        let decided = match op {
            BinOp::And => !left,
            _ => left,
        };
        if decided {
            return Ok(Val::Prim(Prim::Bool(left)));
        }
        Ok(Val::Prim(Prim::Bool(self.truth(b, &word)?)))
    }

    /// An operand that has to be a boolean — a primitive one, or a `Boolean` box.
    fn truth(&self, expr: &Expr, operator: &str) -> Result<bool, String> {
        let value = self.value(expr)?;
        match self.settle(value)? {
            Val::Prim(Prim::Bool(b)) => Ok(b),
            other => Err(format!(
                "{operator} needs a boolean, and `{expr}` is {}",
                other.describe()
            )),
        }
    }

    /// A reference as what it stands for: a string becomes its text, a box its primitive. Anything
    /// else stays a reference and compares by identity.
    fn settle(&self, value: Val<T::Ref>) -> Result<Val<T::Ref>, String> {
        let Val::Ref(r) = value else { return Ok(value) };
        if let Some(text) = self.target.text(&r)? {
            return Ok(Val::Str(text));
        }
        if let Some(prim) = self.target.unbox(&r)? {
            return Ok(Val::Prim(prim));
        }
        Ok(Val::Ref(r))
    }

    fn call(&self, callee: &Expr, args: &[Expr]) -> Result<Val<T::Ref>, String> {
        if !self.syntax.calls {
            return Err(self.syntax.call_refusal(&callee.to_string()));
        }
        let (receiver, name) = match callee {
            Expr::Field(base, name) => (Some(self.value(base)?), name.as_str()),
            Expr::Name(name) => (None, name.as_str()),
            other => return Err(format!("`{other}(…)` is not something that can be called")),
        };
        let values = args.iter().map(|a| self.value(a)).collect::<Result<Vec<_>, _>>()?;
        self.target.call(receiver.as_ref(), name, &values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::condition::parse_condition;
    use crate::parser::parse;
    use crate::syntax::{JAVA, RUST};
    use std::collections::HashMap;

    /// A stopped program in a hash map: variables, objects with fields, arrays, statics, strings
    /// and boxes. Handles are plain numbers.
    #[derive(Default)]
    struct Mock {
        vars: HashMap<&'static str, Val<u32>>,
        objects: HashMap<u32, HashMap<&'static str, Val<u32>>>,
        arrays: HashMap<u32, Vec<Val<u32>>>,
        statics: HashMap<&'static str, Val<u32>>,
        strings: HashMap<u32, &'static str>,
        boxes: HashMap<u32, Prim>,
        pointers: HashMap<u32, Val<u32>>,
    }

    impl DebugTarget for Mock {
        type Ref = u32;

        fn variable(&self, name: &str) -> Result<Option<Val<u32>>, String> {
            Ok(self.vars.get(name).cloned())
        }
        fn field(&self, owner: &u32, name: &str, of: &str) -> Result<Val<u32>, String> {
            self.objects
                .get(owner)
                .and_then(|o| o.get(name))
                .cloned()
                .ok_or_else(|| format!("no field named `{name}` on `{of}`"))
        }
        fn index(&self, owner: &u32, at: i64, of: &str) -> Result<Val<u32>, String> {
            let array = self.arrays.get(owner).ok_or_else(|| format!("`{of}` is not an array"))?;
            usize::try_from(at)
                .ok()
                .and_then(|i| array.get(i))
                .cloned()
                .ok_or_else(|| format!("index {at} is outside an array of {}", array.len()))
        }
        fn deref(&self, pointer: &u32, of: &str) -> Result<Val<u32>, String> {
            self.pointers.get(pointer).cloned().ok_or_else(|| format!("`{of}` points at nothing"))
        }
        fn static_field(&self, qualifier: &[String], name: &str) -> Result<Option<Val<u32>>, String> {
            Ok(self.statics.get(format!("{}.{name}", qualifier.join(".")).as_str()).cloned())
        }
        fn text(&self, of: &u32) -> Result<Option<String>, String> {
            Ok(self.strings.get(of).map(|s| s.to_string()))
        }
        fn unbox(&self, of: &u32) -> Result<Option<Prim>, String> {
            Ok(self.boxes.get(of).copied())
        }
    }

    fn int(v: i32) -> Val<u32> {
        Val::Prim(Prim::Int(v))
    }

    /// `order` (1) has a `total`, a `customer` (2) with a `name` (string 3), and a `status` (4).
    fn program() -> Mock {
        let mut m = Mock::default();
        m.vars.insert("i", int(5));
        m.vars.insert("n", int(6));
        m.vars.insert("zero", int(0));
        m.vars.insert("done", Val::Prim(Prim::Bool(false)));
        m.vars.insert("order", Val::Ref(1));
        m.vars.insert("nothing", Val::Null);
        m.vars.insert("items", Val::Ref(10));
        m.vars.insert("count", Val::Ref(20));
        m.vars.insert("label", Val::Ref(3));
        m.objects.insert(
            1,
            HashMap::from([("total", int(250)), ("customer", Val::Ref(2)), ("status", Val::Ref(4))]),
        );
        m.objects.insert(2, HashMap::from([("name", Val::Ref(3))]));
        m.objects.insert(4, HashMap::from([("name", Val::Ref(5))]));
        m.strings.insert(3, "acme");
        m.strings.insert(5, "ACTIVE");
        m.arrays.insert(10, vec![int(7), int(8), int(9)]);
        m.boxes.insert(20, Prim::Int(10));
        m.statics.insert("Status.ACTIVE", Val::Ref(4));
        m.statics.insert("com.acme.Status.ACTIVE", Val::Ref(4));
        m.statics.insert("Limits.MAX", int(100));
        m.pointers.insert(30, int(42));
        m.vars.insert("p", Val::Ref(30));
        m
    }

    fn holds(text: &str) -> Result<bool, String> {
        holds_in(text, &program())
    }

    fn holds_in(text: &str, m: &Mock) -> Result<bool, String> {
        let cond = parse_condition(text, JAVA)?.expect("not empty");
        eval_condition(&cond, m, JAVA)
    }

    fn value(text: &str) -> Result<Val<u32>, String> {
        eval(&parse(text, JAVA).unwrap(), &program(), JAVA)
    }

    #[test]
    fn a_path_is_read_and_compared_with_a_literal() {
        assert_eq!(holds("order.total > 100"), Ok(true));
        assert_eq!(holds("order.customer.name == \"acme\""), Ok(true), "a string compares by its text");
        assert_eq!(holds("order.customer.name == \"other\""), Ok(false));
        assert_eq!(holds("items[1] == 8"), Ok(true));
        assert_eq!(holds("!done"), Ok(true));
    }

    #[test]
    fn arithmetic_on_both_sides() {
        assert_eq!(holds("i + 1 == n"), Ok(true));
        assert_eq!(holds("i * 2 - 1 == 9"), Ok(true));
        assert_eq!(holds("order.total % 100 == 50"), Ok(true));
        assert_eq!(holds("i / 2 == 2"), Ok(true), "integer division truncates");
        assert_eq!(holds("(i + 1) * 2 == 12"), Ok(true));
    }

    #[test]
    fn a_subscript_may_be_an_expression() {
        assert_eq!(holds("items[i - 4] == 8"), Ok(true));
        assert!(holds("items[i] == 8").unwrap_err().contains("outside an array of 3"));
        assert!(holds("items[1.5] == 8").unwrap_err().contains("whole number"));
        assert!(holds("items[\"a\"] == 8").unwrap_err().contains("whole number"));
    }

    /// The reason `&&` has to short-circuit: without it the right side walks through the null.
    #[test]
    fn and_and_or_short_circuit_so_a_guard_protects_what_follows() {
        assert_eq!(holds("nothing != null && nothing.total > 100"), Ok(false));
        assert_eq!(holds("nothing == null || nothing.total > 100"), Ok(true));
        assert!(holds("nothing.total > 100").unwrap_err().contains("is null"));
        assert!(holds("order != null && order.total > 100").unwrap());
    }

    #[test]
    fn a_null_halfway_down_the_path_names_which_one() {
        let mut m = program();
        m.objects.get_mut(&1).unwrap().insert("customer", Val::Null);
        let err = holds_in("order.customer.name == \"acme\"", &m).unwrap_err();
        assert!(err.contains("`order.customer` is null"), "{err}");
        assert!(err.contains(".name"), "{err}");
    }

    #[test]
    fn dividing_by_zero_is_a_clean_error() {
        assert_eq!(holds("i / zero == 1").unwrap_err(), "division by zero");
        assert_eq!(holds("i % zero == 1").unwrap_err(), "remainder by zero");
    }

    #[test]
    fn a_missing_name_says_so() {
        assert!(holds("nope > 1").unwrap_err().contains("no variable or field named `nope`"));
        assert!(holds("order.nope > 1").unwrap_err().contains("no field named `nope`"));
    }

    #[test]
    fn a_static_or_an_enum_constant_is_read_by_its_type() {
        assert_eq!(holds("order.total < Limits.MAX"), Ok(false));
        assert_eq!(holds("order.total > Limits.MAX"), Ok(true));
        assert_eq!(holds("order.status == Status.ACTIVE"), Ok(true), "an enum compares by identity");
        assert_eq!(holds("Status.ACTIVE.name == \"ACTIVE\""), Ok(true), "and a member of it is read");
        assert_eq!(holds("order.status == com.acme.Status.ACTIVE"), Ok(true), "fully qualified too");
        let err = holds("order.status == Status.GONE").unwrap_err();
        assert!(err.contains("`Status.GONE`") && err.contains("not be loaded"), "{err}");
    }

    #[test]
    fn a_name_that_is_a_variable_never_goes_looking_for_a_class() {
        // `order.status` has no static anywhere; if `order` were looked up as a type the error
        // would be about classes, not about the field.
        let err = holds("order.nope == 1").unwrap_err();
        assert!(!err.contains("loaded"), "{err}");
    }

    #[test]
    fn a_box_unboxes_where_a_number_is_needed() {
        assert_eq!(holds("count > 3"), Ok(true));
        assert_eq!(holds("count + 1 == 11"), Ok(true));
    }

    #[test]
    fn a_string_variable_compares_and_joins_by_its_text() {
        assert_eq!(holds("label == \"acme\""), Ok(true));
        assert_eq!(holds("label + \"!\" == \"acme!\""), Ok(true));
        assert_eq!(holds("\"n=\" + n == \"n=6\""), Ok(true));
    }

    #[test]
    fn the_ternary_evaluates_only_the_branch_it_takes() {
        assert_eq!(holds("(done ? nothing.total : order.total) == 250"), Ok(true));
        assert_eq!(holds("(i > 1 ? 10 : 20) == 10"), Ok(true));
        assert!(holds("(i ? 1 : 2) == 1").unwrap_err().contains("needs a boolean"));
    }

    #[test]
    fn a_condition_that_is_not_a_boolean_says_what_it_was() {
        let err = holds("order.total + 1 == 3 || order.total").unwrap_err();
        assert!(err.contains("needs a boolean"), "{err}");
        let cond = parse("order.total", JAVA).unwrap();
        let err = eval_condition(&cond, &program(), JAVA).unwrap_err();
        assert!(err.contains("has to be a boolean"), "{err}");
    }

    #[test]
    fn a_call_parses_and_is_refused_when_it_is_evaluated() {
        let expr = parse("order.items.size() > 0", JAVA).unwrap();
        let err = eval(&expr, &program(), JAVA).unwrap_err();
        assert!(err.contains("calls a method"), "{err}");
        assert!(err.contains("order.items.size"), "{err}");
    }

    #[test]
    fn a_call_reaches_the_target_only_when_the_syntax_allows_it() {
        struct Echo;
        impl DebugTarget for Echo {
            type Ref = u32;
            fn variable(&self, _: &str) -> Result<Option<Val<u32>>, String> {
                Ok(Some(Val::Prim(Prim::Int(1))))
            }
            fn field(&self, _: &u32, _: &str, _: &str) -> Result<Val<u32>, String> {
                Err("no".into())
            }
            fn index(&self, _: &u32, _: i64, _: &str) -> Result<Val<u32>, String> {
                Err("no".into())
            }
            fn deref(&self, _: &u32, _: &str) -> Result<Val<u32>, String> {
                Err("no".into())
            }
            fn call(&self, _: Option<&Val<u32>>, name: &str, args: &[Val<u32>]) -> Result<Val<u32>, String> {
                Ok(Val::Str(format!("{name}/{}", args.len())))
            }
        }
        let open = Syntax { calls: true, ..JAVA };
        let expr = parse("f(a, b)", open).unwrap();
        assert_eq!(eval(&expr, &Echo, open), Ok(Val::Str("f/2".into())));
        assert!(expr.refusal(open).is_none());
        assert!(eval(&expr, &Echo, JAVA).is_err());
    }

    #[test]
    fn the_value_of_a_plain_path_is_the_reference_itself() {
        assert_eq!(value("order"), Ok(Val::Ref(1)));
        assert_eq!(value("order.customer.name"), Ok(Val::Ref(3)));
        assert_eq!(value("i + 1"), Ok(int(6)));
    }

    // ── Rust ────────────────────────────────────────────────────────────────────

    fn rust_program() -> Mock {
        let mut m = Mock::default();
        m.vars.insert("n", Val::Prim(Prim::Long(i64::MAX)));
        m.vars.insert("small", Val::Prim(Prim::Long(3)));
        m.vars.insert("ratio", Val::Prim(Prim::Double(0.5)));
        m.vars.insert("p", Val::Ref(30));
        m.pointers.insert(30, Val::Prim(Prim::Long(42)));
        m
    }

    fn rust_holds(text: &str) -> Result<bool, String> {
        let cond = parse_condition(text, RUST)?.expect("not empty");
        eval_condition(&cond, &rust_program(), RUST)
    }

    #[test]
    fn rust_follows_a_reference_with_a_star_and_compares_what_it_finds() {
        assert_eq!(rust_holds("*p == 42"), Ok(true));
        assert_eq!(rust_holds("*p + 1 > 42"), Ok(true));
    }

    #[test]
    fn rust_overflow_is_an_error_not_a_wrap() {
        assert!(rust_holds("n + 1 > 0").unwrap_err().contains("overflows"));
        assert_eq!(rust_holds("small * 2 == 6"), Ok(true));
    }

    #[test]
    fn rust_refuses_to_mix_types_and_to_add_strings() {
        assert!(rust_holds("small + ratio > 1").unwrap_err().contains("mix"));
        assert_eq!(rust_holds("ratio * 2.0 == 1.0"), Ok(true));
    }
}
