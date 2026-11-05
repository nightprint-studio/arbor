//! The syntax tree of an expression.

use std::fmt;

use crate::path::Step;
use crate::syntax::Syntax;

/// A literal, as written.
#[derive(Debug, Clone, PartialEq)]
pub enum Lit {
    Int(i32),
    Long(i64),
    Double(f64),
    /// A code unit (Java) or scalar (Rust).
    Char(u32),
    Str(String),
    Bool(bool),
    Null,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

impl BinOp {
    pub fn word(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Rem => "%",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::And => "&&",
            BinOp::Or => "||",
        }
    }

    /// `+ - * / %` — the ones that produce a number (or, in Java, a joined string).
    pub fn is_arithmetic(self) -> bool {
        matches!(self, BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem)
    }

    /// The six comparisons.
    pub fn is_comparison(self) -> bool {
        matches!(self, BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge)
    }

    /// Whether it asks which side is **bigger** — the ones only numbers can answer.
    pub fn is_ordering(self) -> bool {
        matches!(self, BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge)
    }
}

/// An expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Lit(Lit),
    /// A variable — or, when it is not one, the first segment of a qualified type name.
    Name(String),
    Field(Box<Expr>, String),
    Index(Box<Expr>, Box<Expr>),
    /// `*p` — Rust only.
    Deref(Box<Expr>),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
    /// Parsed so that it can be refused **by name** with a real explanation, rather than reported as
    /// a stray `(`. Evaluating one is [`Syntax::calls`]'s decision.
    Call { callee: Box<Expr>, args: Vec<Expr> },
}

impl Expr {
    /// The expression as a watch *path* — names, `.field`, constant subscripts and `*` — or `None`
    /// when it is anything wider. What lets a caller route: a path is walked by reading the stopped
    /// program, anything else is evaluated.
    pub fn as_path(&self) -> Option<Vec<Step>> {
        match self {
            Expr::Name(name) => Some(vec![Step::Field(name.clone())]),
            Expr::Field(base, name) => {
                let mut steps = base.as_path()?;
                steps.push(Step::Field(name.clone()));
                Some(steps)
            }
            Expr::Index(base, at) => {
                let at = at.constant_index()?;
                let mut steps = base.as_path()?;
                steps.push(Step::Index(at));
                Some(steps)
            }
            Expr::Deref(base) => {
                let mut steps = base.as_path()?;
                steps.push(Step::Deref);
                Some(steps)
            }
            _ => None,
        }
    }

    /// A subscript written as a number: `2`, `-1`.
    fn constant_index(&self) -> Option<i32> {
        match self {
            Expr::Lit(Lit::Int(k)) => Some(*k),
            Expr::Lit(Lit::Long(k)) => i32::try_from(*k).ok(),
            Expr::Unary(UnOp::Neg, inner) => match **inner {
                Expr::Lit(Lit::Int(k)) => k.checked_neg(),
                _ => None,
            },
            _ => None,
        }
    }

    /// Whether anything in it is read from the program. An expression that is not is a constant —
    /// which a condition refuses, because no stopped program could change its answer.
    pub fn reads_program(&self) -> bool {
        match self {
            Expr::Lit(_) => false,
            Expr::Name(_) => true,
            Expr::Field(base, _) | Expr::Deref(base) | Expr::Unary(_, base) => base.reads_program(),
            Expr::Index(a, b) | Expr::Binary(_, a, b) => a.reads_program() || b.reads_program(),
            Expr::Ternary(a, b, c) => a.reads_program() || b.reads_program() || c.reads_program(),
            Expr::Call { callee, args } => {
                callee.reads_program() || args.iter().any(Expr::reads_program)
            }
        }
    }

    /// The reason this expression cannot be evaluated at all, when it contains something refused by
    /// name — currently a call. Checked before evaluation so a condition is refused as it is typed,
    /// not at the tenth thousandth hit.
    pub fn refusal(&self, syntax: Syntax) -> Option<String> {
        if syntax.calls {
            return None;
        }
        match self {
            Expr::Call { callee, .. } => Some(syntax.call_refusal(&callee.to_string())),
            Expr::Lit(_) | Expr::Name(_) => None,
            Expr::Field(base, _) | Expr::Deref(base) | Expr::Unary(_, base) => base.refusal(syntax),
            Expr::Index(a, b) | Expr::Binary(_, a, b) => a.refusal(syntax).or_else(|| b.refusal(syntax)),
            Expr::Ternary(a, b, c) => {
                a.refusal(syntax).or_else(|| b.refusal(syntax)).or_else(|| c.refusal(syntax))
            }
        }
    }

    /// The maximal sub-expressions that are watch paths, in reading order — what a condition
    /// "read", for the stop it let through (`total = 250, limit = 100`). Duplicates are kept; the
    /// caller drops them.
    pub fn paths(&self) -> Vec<&Expr> {
        let mut out = Vec::new();
        self.collect_paths(&mut out);
        out
    }

    fn collect_paths<'a>(&'a self, out: &mut Vec<&'a Expr>) {
        if self.as_path().is_some() {
            out.push(self);
            return;
        }
        match self {
            Expr::Lit(_) | Expr::Name(_) => {}
            Expr::Field(base, _) | Expr::Deref(base) | Expr::Unary(_, base) => {
                base.collect_paths(out)
            }
            Expr::Index(a, b) | Expr::Binary(_, a, b) => {
                a.collect_paths(out);
                b.collect_paths(out);
            }
            Expr::Ternary(a, b, c) => {
                a.collect_paths(out);
                b.collect_paths(out);
                c.collect_paths(out);
            }
            Expr::Call { callee, args } => {
                callee.collect_paths(out);
                for arg in args {
                    arg.collect_paths(out);
                }
            }
        }
    }

    /// The names of a `Name` / `.field` chain (`Status.ACTIVE` → `["Status", "ACTIVE"]`), or `None`
    /// when the expression is anything else. What a qualified type name looks like before anyone
    /// knows it is one.
    pub fn qualified_names(&self) -> Option<Vec<String>> {
        match self {
            Expr::Name(name) => Some(vec![name.clone()]),
            Expr::Field(base, name) => {
                let mut names = base.qualified_names()?;
                names.push(name.clone());
                Some(names)
            }
            _ => None,
        }
    }

    /// Needs parentheses to be the base of a `.field`, `[i]` or the operand of a unary.
    fn is_compound(&self) -> bool {
        matches!(self, Expr::Binary(..) | Expr::Ternary(..) | Expr::Unary(..) | Expr::Deref(..))
    }
}

impl fmt::Display for Lit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Lit::Int(v) => write!(f, "{v}"),
            Lit::Long(v) => write!(f, "{v}"),
            Lit::Double(v) => write!(f, "{v:?}"),
            Lit::Char(c) => match char::from_u32(*c) {
                Some(ch) => write!(f, "{ch:?}"),
                None => write!(f, "'\\u{c:04x}'"),
            },
            Lit::Str(s) => write!(f, "{s:?}"),
            Lit::Bool(b) => write!(f, "{b}"),
            Lit::Null => f.write_str("null"),
        }
    }
}

/// The expression as source, with parentheses wherever the tree needs them — what a message quotes
/// ("`order.customer` is null").
impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fn wrapped(f: &mut fmt::Formatter<'_>, e: &Expr, wrap: bool) -> fmt::Result {
            if wrap {
                write!(f, "({e})")
            } else {
                write!(f, "{e}")
            }
        }
        match self {
            Expr::Lit(l) => write!(f, "{l}"),
            Expr::Name(n) => f.write_str(n),
            Expr::Field(base, name) => {
                wrapped(f, base, base.is_compound())?;
                write!(f, ".{name}")
            }
            Expr::Index(base, at) => {
                wrapped(f, base, base.is_compound())?;
                write!(f, "[{at}]")
            }
            Expr::Deref(inner) => {
                f.write_str("*")?;
                wrapped(f, inner, matches!(**inner, Expr::Binary(..) | Expr::Ternary(..)))
            }
            Expr::Unary(op, inner) => {
                f.write_str(if *op == UnOp::Neg { "-" } else { "!" })?;
                wrapped(f, inner, matches!(**inner, Expr::Binary(..) | Expr::Ternary(..)))
            }
            Expr::Binary(op, a, b) => {
                wrapped(f, a, matches!(**a, Expr::Binary(..) | Expr::Ternary(..)))?;
                write!(f, " {} ", op.word())?;
                wrapped(f, b, matches!(**b, Expr::Binary(..) | Expr::Ternary(..)))
            }
            Expr::Ternary(c, a, b) => {
                wrapped(f, c, matches!(**c, Expr::Ternary(..)))?;
                f.write_str(" ? ")?;
                write!(f, "{a}")?;
                f.write_str(" : ")?;
                write!(f, "{b}")
            }
            Expr::Call { callee, args } => {
                wrapped(f, callee, callee.is_compound())?;
                f.write_str("(")?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{arg}")?;
                }
                f.write_str(")")
            }
        }
    }
}
