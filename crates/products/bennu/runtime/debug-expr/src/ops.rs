//! What the operators mean, per language. Pure: values in, a value or a reason out.
//!
//! This is where Java and Rust differ, and the difference is the point:
//!
//! |                    | Java                                   | Rust                                 |
//! |--------------------|----------------------------------------|--------------------------------------|
//! | `int` overflow     | wraps (`Integer.MAX_VALUE + 1` < 0)    | an error — Rust would panic          |
//! | integer `/`        | truncates toward zero                  | truncates; `MIN / -1` is an error    |
//! | `/` or `%` by zero | an error (`ArithmeticException`)       | an error                             |
//! | `int` with `double`| promoted to `double`                   | refused — the types do not mix       |
//! | `char`             | promotes to `int` in arithmetic        | compares only with `char`            |
//! | `+` on strings     | joins (`"n=" + n`)                     | refused                              |
//!
//! A comparison involving a NaN is false, except `!=`, as in both languages.

use std::cmp::Ordering;

use crate::ast::{BinOp, UnOp};
use crate::syntax::Lang;
use crate::value::{Prim, Val};

/// A number once `char` has been folded into `int` (Java) and every Rust integer into `long`.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Num {
    Int(i32),
    Long(i64),
    Double(f64),
}

impl Num {
    fn as_i64(self) -> i64 {
        match self {
            Num::Int(v) => i64::from(v),
            Num::Long(v) => v,
            Num::Double(v) => v as i64,
        }
    }

    fn as_f64(self) -> f64 {
        match self {
            Num::Int(v) => f64::from(v),
            Num::Long(v) => v as f64,
            Num::Double(v) => v,
        }
    }
}

fn java_number(p: Prim) -> Option<Num> {
    match p {
        Prim::Int(v) => Some(Num::Int(v)),
        Prim::Long(v) => Some(Num::Long(v)),
        Prim::Double(v) => Some(Num::Double(v)),
        Prim::Char(c) => Some(Num::Int(c as i32)),
        Prim::Bool(_) => None,
    }
}

fn rust_number(p: Prim) -> Option<Num> {
    match p {
        Prim::Int(v) => Some(Num::Long(i64::from(v))),
        Prim::Long(v) => Some(Num::Long(v)),
        Prim::Double(v) => Some(Num::Double(v)),
        Prim::Char(_) | Prim::Bool(_) => None,
    }
}

fn number(lang: Lang, p: Prim) -> Option<Num> {
    match lang {
        Lang::Java => java_number(p),
        Lang::Rust => rust_number(p),
    }
}

/// Apply a binary operator other than `&&` / `||` (which short-circuit, so the evaluator owns them).
pub fn binary<R: PartialEq>(lang: Lang, op: BinOp, l: Val<R>, r: Val<R>) -> Result<Val<R>, String> {
    match op {
        BinOp::Eq | BinOp::Ne => equality(lang, op, &l, &r).map(bool_val),
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => ordering(lang, op, &l, &r).map(bool_val),
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => arithmetic(lang, op, l, r),
        BinOp::And | BinOp::Or => Err("`&&` and `||` short-circuit and are not applied here".to_string()),
    }
}

/// Apply a unary operator.
pub fn unary<R>(lang: Lang, op: UnOp, v: Val<R>) -> Result<Val<R>, String> {
    match (op, v) {
        (UnOp::Not, Val::Prim(Prim::Bool(b))) => Ok(bool_val(!b)),
        (UnOp::Not, other) => Err(format!("`!` needs a boolean, and this is {}", other.describe())),
        (UnOp::Neg, Val::Prim(p)) => negate(lang, p).map(Val::Prim),
        (UnOp::Neg, other) => Err(format!("`-` needs a number, and this is {}", other.describe())),
    }
}

fn bool_val<R>(b: bool) -> Val<R> {
    Val::Prim(Prim::Bool(b))
}

fn negate(lang: Lang, p: Prim) -> Result<Prim, String> {
    let Some(n) = number(lang, p) else {
        return Err(format!("`-` needs a number, and this is {}", p.describe()));
    };
    match (lang, n) {
        (_, Num::Double(v)) => Ok(Prim::Double(-v)),
        (Lang::Java, Num::Int(v)) => Ok(Prim::Int(v.wrapping_neg())),
        (Lang::Java, Num::Long(v)) => Ok(Prim::Long(v.wrapping_neg())),
        (Lang::Rust, n) => n
            .as_i64()
            .checked_neg()
            .map(Prim::Long)
            .ok_or_else(|| "`-` overflows — Rust checks arithmetic and would panic".to_string()),
    }
}

// ── arithmetic ──────────────────────────────────────────────────────────────────

fn arithmetic<R: PartialEq>(lang: Lang, op: BinOp, l: Val<R>, r: Val<R>) -> Result<Val<R>, String> {
    if let (Val::Prim(a), Val::Prim(b)) = (&l, &r) {
        if let (Some(x), Some(y)) = (number(lang, *a), number(lang, *b)) {
            return match lang {
                Lang::Java => java_arithmetic(op, x, y),
                Lang::Rust => rust_arithmetic(op, x, y),
            }
            .map(Val::Prim);
        }
    }
    if op == BinOp::Add && (matches!(l, Val::Str(_)) || matches!(r, Val::Str(_))) {
        return match lang {
            Lang::Java => Ok(Val::Str(format!("{}{}", java_text(&l)?, java_text(&r)?))),
            Lang::Rust => Err(
                "Rust has no `+` between strings in a watch — compare them, or expand the value"
                    .to_string(),
            ),
        };
    }
    Err(format!(
        "`{}` works on numbers, and this has {} and {}",
        op.word(),
        l.describe(),
        r.describe()
    ))
}

/// Java's numeric promotion: `double` wins, then `long`, else `int`.
fn java_arithmetic(op: BinOp, a: Num, b: Num) -> Result<Prim, String> {
    if matches!(a, Num::Double(_)) || matches!(b, Num::Double(_)) {
        let (x, y) = (a.as_f64(), b.as_f64());
        return Ok(Prim::Double(match op {
            BinOp::Add => x + y,
            BinOp::Sub => x - y,
            BinOp::Mul => x * y,
            BinOp::Div => x / y,
            _ => x % y,
        }));
    }
    if matches!(op, BinOp::Div | BinOp::Rem) && b.as_i64() == 0 {
        return Err(zero_divisor(op));
    }
    if matches!(a, Num::Long(_)) || matches!(b, Num::Long(_)) {
        let (x, y) = (a.as_i64(), b.as_i64());
        return Ok(Prim::Long(match op {
            BinOp::Add => x.wrapping_add(y),
            BinOp::Sub => x.wrapping_sub(y),
            BinOp::Mul => x.wrapping_mul(y),
            BinOp::Div => x.wrapping_div(y),
            _ => x.wrapping_rem(y),
        }));
    }
    let (x, y) = (a.as_i64() as i32, b.as_i64() as i32);
    Ok(Prim::Int(match op {
        BinOp::Add => x.wrapping_add(y),
        BinOp::Sub => x.wrapping_sub(y),
        BinOp::Mul => x.wrapping_mul(y),
        BinOp::Div => x.wrapping_div(y),
        _ => x.wrapping_rem(y),
    }))
}

/// Rust's: checked, and strictly typed.
fn rust_arithmetic(op: BinOp, a: Num, b: Num) -> Result<Prim, String> {
    match (a, b) {
        (Num::Double(x), Num::Double(y)) => Ok(Prim::Double(match op {
            BinOp::Add => x + y,
            BinOp::Sub => x - y,
            BinOp::Mul => x * y,
            BinOp::Div => x / y,
            _ => x % y,
        })),
        (Num::Double(_), _) | (_, Num::Double(_)) => Err(mixed_numbers()),
        _ => {
            let (x, y) = (a.as_i64(), b.as_i64());
            if matches!(op, BinOp::Div | BinOp::Rem) && y == 0 {
                return Err(zero_divisor(op));
            }
            let result = match op {
                BinOp::Add => x.checked_add(y),
                BinOp::Sub => x.checked_sub(y),
                BinOp::Mul => x.checked_mul(y),
                BinOp::Div => x.checked_div(y),
                _ => x.checked_rem(y),
            };
            result.map(Prim::Long).ok_or_else(|| {
                format!("`{x} {} {y}` overflows — Rust checks arithmetic and would panic", op.word())
            })
        }
    }
}

fn zero_divisor(op: BinOp) -> String {
    if op == BinOp::Div { "division by zero".to_string() } else { "remainder by zero".to_string() }
}

fn mixed_numbers() -> String {
    "Rust does not mix integers and floats — write `1.0` for a float or `1` for an integer".to_string()
}

/// A value as `"…" + x` joins it. An object is refused: its text is `toString()`, a call.
fn java_text<R>(v: &Val<R>) -> Result<String, String> {
    match v {
        Val::Str(s) => Ok(s.clone()),
        Val::Null => Ok("null".to_string()),
        Val::Prim(p) => Ok(java_prim_text(*p)),
        Val::Ref(_) => Err(
            "an object cannot be joined to a string — that is `toString()`, a method call, which \
             would run application code inside the paused program. Join one of its fields"
                .to_string(),
        ),
    }
}

fn java_prim_text(p: Prim) -> String {
    match p {
        Prim::Bool(b) => b.to_string(),
        Prim::Char(c) => char::from_u32(c).map(String::from).unwrap_or_default(),
        Prim::Int(v) => v.to_string(),
        Prim::Long(v) => v.to_string(),
        Prim::Double(v) => java_double_text(v),
    }
}

/// `Double.toString` for the cases that matter: a whole double keeps its `.0`.
fn java_double_text(v: f64) -> String {
    if v.is_nan() {
        "NaN".to_string()
    } else if v.is_infinite() {
        (if v > 0.0 { "Infinity" } else { "-Infinity" }).to_string()
    } else if v == v.trunc() && v.abs() < 1e7 {
        format!("{v:.1}")
    } else {
        v.to_string()
    }
}

// ── comparison ──────────────────────────────────────────────────────────────────

fn equality<R: PartialEq>(lang: Lang, op: BinOp, l: &Val<R>, r: &Val<R>) -> Result<bool, String> {
    let equal = match (l, r) {
        (Val::Null, Val::Null) => true,
        // A primitive is never null, and asking is a mistake worth naming — Java would throw an NPE
        // unboxing, and `false` here would read as an answer.
        (Val::Null, Val::Prim(p)) | (Val::Prim(p), Val::Null) => {
            return Err(format!("{} cannot be null, so it is not comparable with null", p.describe()))
        }
        (Val::Null, _) | (_, Val::Null) => false,
        (Val::Str(a), Val::Str(b)) => a == b,
        // Rust compares VALUES, which a debugger would have to walk field by field; answering with
        // identity (what Java's `==` means) would be a confident wrong answer.
        (Val::Ref(_), Val::Ref(_)) if lang == Lang::Rust => {
            return Err(
                "Rust compares values, not references — compare the fields you mean, one by one"
                    .to_string(),
            )
        }
        // Identity, which is what `==` means between two objects in Java.
        (Val::Ref(a), Val::Ref(b)) => a == b,
        (Val::Prim(Prim::Bool(a)), Val::Prim(Prim::Bool(b))) => a == b,
        (Val::Prim(a), Val::Prim(b)) => {
            return Ok(holds(op, compare_numbers(lang, *a, *b)?));
        }
        _ => return Err(format!("{} and {} are not comparable", l.describe(), r.describe())),
    };
    Ok(if op == BinOp::Eq { equal } else { !equal })
}

fn ordering<R>(lang: Lang, op: BinOp, l: &Val<R>, r: &Val<R>) -> Result<bool, String> {
    if let (Val::Prim(a), Val::Prim(b)) = (l, r) {
        if !matches!(a, Prim::Bool(_)) && !matches!(b, Prim::Bool(_)) {
            return Ok(holds(op, compare_numbers(lang, *a, *b)?));
        }
    }
    Err(format!(
        "`{}` compares numbers, and this compares {} with {}",
        op.word(),
        l.describe(),
        r.describe()
    ))
}

/// How two primitives order, or `None` when a NaN is involved.
fn compare_numbers(lang: Lang, a: Prim, b: Prim) -> Result<Option<Ordering>, String> {
    let not_comparable = || format!("{} and {} are not comparable", a.describe(), b.describe());
    if lang == Lang::Rust {
        if let (Prim::Char(x), Prim::Char(y)) = (a, b) {
            return Ok(Some(x.cmp(&y)));
        }
    }
    let (Some(x), Some(y)) = (number(lang, a), number(lang, b)) else {
        return Err(not_comparable());
    };
    match (lang, x, y) {
        (Lang::Rust, Num::Double(_), Num::Long(_) | Num::Int(_))
        | (Lang::Rust, Num::Long(_) | Num::Int(_), Num::Double(_)) => Err(mixed_numbers()),
        (_, Num::Double(_), _) | (_, _, Num::Double(_)) => Ok(x.as_f64().partial_cmp(&y.as_f64())),
        _ => Ok(Some(x.as_i64().cmp(&y.as_i64()))),
    }
}

/// Whether `op` holds for an ordering; an unordered pair (NaN) satisfies only `!=`.
fn holds(op: BinOp, ordering: Option<Ordering>) -> bool {
    let Some(ordering) = ordering else { return op == BinOp::Ne };
    match op {
        BinOp::Eq => ordering == Ordering::Equal,
        BinOp::Ne => ordering != Ordering::Equal,
        BinOp::Lt => ordering == Ordering::Less,
        BinOp::Le => ordering != Ordering::Greater,
        BinOp::Gt => ordering == Ordering::Greater,
        BinOp::Ge => ordering != Ordering::Less,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type V = Val<u32>;

    fn int(v: i32) -> V {
        Val::Prim(Prim::Int(v))
    }
    fn long(v: i64) -> V {
        Val::Prim(Prim::Long(v))
    }
    fn dbl(v: f64) -> V {
        Val::Prim(Prim::Double(v))
    }
    fn text(s: &str) -> V {
        Val::Str(s.to_string())
    }
    fn object(id: u32) -> V {
        Val::Ref(id)
    }

    fn java(op: BinOp, l: V, r: V) -> Result<V, String> {
        binary(Lang::Java, op, l, r)
    }
    fn rust(op: BinOp, l: V, r: V) -> Result<V, String> {
        binary(Lang::Rust, op, l, r)
    }
    fn truth(v: Result<V, String>) -> bool {
        match v.expect("an answer") {
            Val::Prim(Prim::Bool(b)) => b,
            other => panic!("not a boolean: {other:?}"),
        }
    }

    // ── Java ────────────────────────────────────────────────────────────────────

    #[test]
    fn java_int_arithmetic_wraps_around() {
        assert_eq!(java(BinOp::Add, int(i32::MAX), int(1)), Ok(int(i32::MIN)));
        assert_eq!(java(BinOp::Mul, int(65_536), int(65_536)), Ok(int(0)));
        assert_eq!(java(BinOp::Sub, int(i32::MIN), int(1)), Ok(int(i32::MAX)));
        // …and a long does not, until it has to.
        assert_eq!(java(BinOp::Add, long(i32::MAX as i64), int(1)), Ok(long(2_147_483_648)));
        assert_eq!(java(BinOp::Add, long(i64::MAX), long(1)), Ok(long(i64::MIN)));
    }

    #[test]
    fn java_integer_division_truncates_toward_zero() {
        assert_eq!(java(BinOp::Div, int(7), int(2)), Ok(int(3)));
        assert_eq!(java(BinOp::Div, int(-7), int(2)), Ok(int(-3)));
        assert_eq!(java(BinOp::Rem, int(-7), int(2)), Ok(int(-1)));
        assert_eq!(java(BinOp::Div, int(i32::MIN), int(-1)), Ok(int(i32::MIN)));
        assert_eq!(java(BinOp::Div, int(7), dbl(2.0)), Ok(dbl(3.5)), "a double promotes the int");
    }

    #[test]
    fn dividing_by_zero_is_an_error_not_a_crash_or_an_infinity() {
        assert_eq!(java(BinOp::Div, int(1), int(0)), Err("division by zero".into()));
        assert_eq!(java(BinOp::Rem, long(1), long(0)), Err("remainder by zero".into()));
        assert_eq!(rust(BinOp::Div, long(1), long(0)), Err("division by zero".into()));
        // A floating division by zero is Infinity in both languages, not an error.
        assert_eq!(java(BinOp::Div, dbl(1.0), dbl(0.0)), Ok(dbl(f64::INFINITY)));
    }

    #[test]
    fn a_java_char_promotes_to_int() {
        let c = Val::Prim(Prim::Char('a' as u32));
        assert_eq!(java(BinOp::Add, c.clone(), int(1)), Ok(int(98)));
        assert!(truth(java(BinOp::Eq, c.clone(), int(97))));
        assert!(truth(java(BinOp::Eq, c, Val::Prim(Prim::Char('a' as u32)))));
    }

    #[test]
    fn java_plus_joins_strings_and_says_how_to_not_call_to_string() {
        assert_eq!(java(BinOp::Add, text("n="), int(5)), Ok(text("n=5")));
        assert_eq!(java(BinOp::Add, int(5), text("!")), Ok(text("5!")));
        assert_eq!(java(BinOp::Add, text("x"), Val::Null), Ok(text("xnull")));
        assert_eq!(java(BinOp::Add, text("d="), dbl(2.0)), Ok(text("d=2.0")));
        assert_eq!(java(BinOp::Add, text("b="), Val::Prim(Prim::Bool(true))), Ok(text("b=true")));
        let err = java(BinOp::Add, text("o="), object(1)).unwrap_err();
        assert!(err.contains("toString"), "{err}");
        assert!(java(BinOp::Sub, text("a"), int(1)).unwrap_err().contains("works on numbers"));
    }

    // ── Rust ────────────────────────────────────────────────────────────────────

    #[test]
    fn rust_arithmetic_is_checked() {
        assert_eq!(rust(BinOp::Add, long(2), long(3)), Ok(long(5)));
        let err = rust(BinOp::Add, long(i64::MAX), long(1)).unwrap_err();
        assert!(err.contains("overflows"), "{err}");
        assert!(rust(BinOp::Mul, long(i64::MAX), long(2)).is_err());
        assert!(rust(BinOp::Div, long(i64::MIN), long(-1)).unwrap_err().contains("overflows"));
        assert_eq!(rust(BinOp::Div, long(-7), long(2)), Ok(long(-3)));
    }

    #[test]
    fn rust_does_not_mix_integers_and_floats_or_add_strings() {
        assert!(rust(BinOp::Add, long(1), dbl(1.5)).unwrap_err().contains("mix"));
        assert!(rust(BinOp::Lt, long(1), dbl(1.5)).unwrap_err().contains("mix"));
        assert_eq!(rust(BinOp::Add, dbl(1.0), dbl(1.5)), Ok(dbl(2.5)));
        assert!(rust(BinOp::Add, text("a"), text("b")).unwrap_err().contains("no `+`"));
    }

    #[test]
    fn rust_does_not_answer_with_identity_where_it_means_equality() {
        assert!(rust(BinOp::Eq, object(1), object(1)).unwrap_err().contains("values, not references"));
        assert!(truth(java(BinOp::Eq, object(1), object(1))));
    }

    #[test]
    fn a_rust_char_compares_only_with_a_char() {
        let a = Val::Prim(Prim::Char('a' as u32));
        let b = Val::Prim(Prim::Char('b' as u32));
        assert!(truth(rust(BinOp::Lt, a.clone(), b)));
        assert!(rust(BinOp::Eq, a, long(97)).is_err());
    }

    // ── comparison, both ────────────────────────────────────────────────────────

    #[test]
    fn numbers_compare_as_numbers_whichever_width_they_came_from() {
        assert!(truth(java(BinOp::Gt, int(5), int(3))));
        assert!(truth(java(BinOp::Ge, int(3), dbl(3.0))));
        assert!(truth(java(BinOp::Lt, dbl(0.25), int(1))));
        assert!(truth(java(BinOp::Eq, long(120), int(120))));
    }

    #[test]
    fn a_nan_is_unequal_to_everything_including_itself() {
        let nan = dbl(f64::NAN);
        assert!(!truth(java(BinOp::Eq, nan.clone(), nan.clone())));
        assert!(truth(java(BinOp::Ne, nan.clone(), nan.clone())));
        assert!(!truth(java(BinOp::Lt, nan, dbl(1.0))));
    }

    #[test]
    fn null_compares_with_anything_that_could_be_null_and_never_orders() {
        assert!(truth(java(BinOp::Eq, Val::Null, Val::Null)));
        assert!(truth(java(BinOp::Ne, object(7), Val::Null)));
        assert!(!truth(java(BinOp::Eq, text("x"), Val::Null)));
        assert!(java(BinOp::Lt, object(7), Val::Null).is_err());
        assert!(java(BinOp::Eq, int(1), Val::Null).unwrap_err().contains("cannot be null"));
    }

    #[test]
    fn a_string_compares_by_its_text_and_an_object_by_identity() {
        assert!(truth(java(BinOp::Eq, text("a"), text("a"))));
        assert!(truth(java(BinOp::Eq, object(3), object(3))));
        assert!(truth(java(BinOp::Ne, object(3), object(4))));
        // Ordering two objects is the mistake worth naming rather than answering.
        assert!(java(BinOp::Lt, text("a"), text("b")).unwrap_err().contains("compares numbers"));
    }

    #[test]
    fn comparing_things_of_different_kinds_says_which_two() {
        let err = java(BinOp::Eq, Val::Prim(Prim::Bool(true)), int(1)).unwrap_err();
        assert!(err.contains("not comparable"), "{err}");
    }

    // ── unary ───────────────────────────────────────────────────────────────────

    #[test]
    fn negation_and_not() {
        assert_eq!(unary(Lang::Java, UnOp::Neg, int(5)), Ok(int(-5)));
        assert_eq!(unary(Lang::Java, UnOp::Neg, int(i32::MIN)), Ok(int(i32::MIN)));
        assert_eq!(unary(Lang::Rust, UnOp::Neg, long(5)), Ok(long(-5)));
        assert!(unary::<u32>(Lang::Rust, UnOp::Neg, long(i64::MIN)).is_err());
        assert!(truth(unary(Lang::Java, UnOp::Not, Val::Prim(Prim::Bool(false)))));
        assert!(unary(Lang::Java, UnOp::Not, int(1)).unwrap_err().contains("needs a boolean"));
        assert!(unary(Lang::Java, UnOp::Neg, text("x")).unwrap_err().contains("needs a number"));
    }
}
