//! AST → `.nemus` source: a deterministic pretty-printer.
//!
//! The inverse direction of the evaluator. It walks the [`ast`](crate::ast) and
//! prints **canonical** `.nemus` text that re-parses to the same tree — a
//! *semantic* round-trip, not byte-exact: comments and incidental whitespace
//! live only in the source, never in the AST, so they are not recovered (this is
//! expected, `design/nemus/editing-model.md`). On already-canonical input the
//! printer is idempotent.
//!
//! It is the enabler for the future editor's surgical edits and for
//! *materialisation* — [`crate::materialize`] builds an AST out of evaluated
//! haps and prints it through here.
//!
//! ## Canonical style
//!
//! - One top-level item per line; the file ends with a newline.
//! - `tracks(...)` / `arrange(...)` — the output / arrangement combinators —
//!   print one argument per line, indented two spaces, with a trailing comma.
//!   Every other call is inline; method chains stay on one line.
//! - A mini-notation island that would overrun [`MAX_WIDTH`] wraps: `<...>` /
//!   `[...]` break to one element per line, `a & b` to one lane per line. Short
//!   islands stay inline. Whitespace is `extras` in the grammar, so the wrapped
//!   form re-parses to the same tree.
//! - Host arithmetic is **tight** (`i*0.1`, `a-1`); structural tokens are
//!   **spaced** (`a & b`, `let x = …`, `i => …`); `,` takes a trailing space.
//! - **Minimal-but-correct** parentheses, driven by operator precedence.
//! - Numbers print in shortest round-trip form: `4`, `0.5`, `0.125` (no `.0`,
//!   no trailing zeros, never exponent notation).

use crate::ast::{
    BinOp, Expr, ExprKind, Ident, Import, Island, IslandKind, Item, Leaf, Mini, MiniArg, MiniKind,
    Postfix, Program, UnOp,
};

/// Calls printed one-argument-per-line. A deliberate formatting choice: these
/// are the arrangement / output combinators, which read best as a vertical list
/// of sections rather than a long inline argument run.
const MULTILINE_CALLS: &[&str] = &["tracks", "arrange"];

/// One indentation step.
const INDENT: &str = "  ";

/// Soft right margin. A mini-notation node whose inline form would push the
/// current line past this is broken across indented lines instead (one `<...>`
/// element / one `&` lane per line) — so a long imported take reads as a column
/// of bars rather than one unscannable line. Leaves are never split.
const MAX_WIDTH: usize = 88;

// Precedence ladder, loosest → tightest (mirrors `design/nemus/grammar.md §4`).
const P_LAMBDA: u8 = 0;
const P_RANGE: u8 = 1;
const P_ADD: u8 = 2;
const P_MUL: u8 = 3;
const P_UNARY: u8 = 4;
const P_POSTFIX: u8 = 5;
const P_ATOM: u8 = 6;

/// Print a whole program to canonical `.nemus` source (trailing newline).
pub fn emit(program: &Program) -> String {
    let mut out = String::new();
    for item in &program.items {
        write_item(&mut out, item);
        out.push('\n');
    }
    out
}

/// Print a single expression — the unit the future editor materialises and
/// splices back into source. No trailing newline.
pub fn emit_expr(expr: &Expr) -> String {
    let mut out = String::new();
    write_expr(&mut out, expr, P_LAMBDA, false, 0);
    out
}

// ── Items ─────────────────────────────────────────────────────────────────────

fn write_item(out: &mut String, item: &Item) {
    match item {
        Item::Import(im) => write_import(out, im),
        Item::Let(b) => {
            out.push_str("let ");
            out.push_str(&b.name.name);
            out.push_str(" = ");
            write_expr(out, &b.value, P_LAMBDA, false, 0);
        }
        Item::Fn(f) => {
            out.push_str("fn ");
            out.push_str(&f.name.name);
            out.push('(');
            write_idents(out, &f.params);
            out.push_str(") = ");
            write_expr(out, &f.body, P_LAMBDA, false, 0);
        }
        Item::Expr(e) => write_expr(out, e, P_LAMBDA, false, 0),
    }
}

fn write_import(out: &mut String, im: &Import) {
    out.push_str("import { ");
    write_idents(out, &im.names);
    out.push_str(" } from ");
    write_quoted(out, &im.path);
}

fn write_idents(out: &mut String, idents: &[Ident]) {
    for (i, id) in idents.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&id.name);
    }
}

// ── Expressions ───────────────────────────────────────────────────────────────

/// Binding power of an expression, used to decide minimal parentheses.
fn prec(kind: &ExprKind) -> u8 {
    match kind {
        ExprKind::Lambda { .. } => P_LAMBDA,
        ExprKind::Range { .. } => P_RANGE,
        ExprKind::Binary {
            op: BinOp::Add | BinOp::Sub,
            ..
        } => P_ADD,
        ExprKind::Binary {
            op: BinOp::Mul | BinOp::Div,
            ..
        } => P_MUL,
        ExprKind::Unary { .. } => P_UNARY,
        ExprKind::Method { .. } => P_POSTFIX,
        ExprKind::Number(_)
        | ExprKind::Str(_)
        | ExprKind::Note(_)
        | ExprKind::Var(_)
        | ExprKind::Call { .. }
        | ExprKind::Island(_) => P_ATOM,
    }
}

/// Emit `e` as a child under a parent of binding power `parent`. `right` marks
/// the right operand of a left-associative binary, which needs parentheses even
/// at equal precedence (`a-(b-c)`, `a/(b*c)`).
fn write_expr(out: &mut String, e: &Expr, parent: u8, right: bool, indent: usize) {
    let p = prec(&e.kind);
    let needs_parens = p < parent || (p == parent && right);
    if needs_parens {
        out.push('(');
    }
    write_expr_inner(out, e, indent);
    if needs_parens {
        out.push(')');
    }
}

fn write_expr_inner(out: &mut String, e: &Expr, indent: usize) {
    match &e.kind {
        ExprKind::Number(n) => out.push_str(&fmt_number(*n)),
        ExprKind::Str(s) => write_quoted(out, s),
        ExprKind::Note(name) => out.push_str(name),
        ExprKind::Var(name) => out.push_str(name),
        ExprKind::Call { name, args } => write_call(out, &name.name, args, indent),
        ExprKind::Method { recv, name, args } => {
            write_expr(out, recv, P_POSTFIX, false, indent);
            out.push('.');
            out.push_str(&name.name);
            out.push('(');
            write_args_inline(out, args, indent);
            out.push(')');
        }
        ExprKind::Unary { op, rhs } => {
            match op {
                UnOp::Neg => out.push('-'),
            }
            write_expr(out, rhs, P_UNARY, false, indent);
        }
        ExprKind::Binary { op, lhs, rhs } => {
            let p = prec(&e.kind);
            write_expr(out, lhs, p, false, indent);
            out.push_str(bin_op_str(*op));
            write_expr(out, rhs, p, true, indent);
        }
        ExprKind::Range { lo, hi, inclusive } => {
            // Operands are `addExpr`-level: a tighter child needs no parens, a
            // looser one (a nested range / lambda) does.
            write_expr(out, lo, P_ADD, false, indent);
            out.push_str(if *inclusive { "..=" } else { ".." });
            write_expr(out, hi, P_ADD, false, indent);
        }
        ExprKind::Lambda { params, body } => {
            if params.len() == 1 {
                out.push_str(&params[0].name);
            } else {
                out.push('(');
                write_idents(out, params);
                out.push(')');
            }
            out.push_str(" => ");
            write_expr(out, body, P_LAMBDA, false, indent);
        }
        ExprKind::Island(isl) => write_island(out, isl, indent),
    }
}

fn bin_op_str(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
    }
}

/// `name(args)` — multi-line for the arrangement combinators, inline otherwise.
fn write_call(out: &mut String, name: &str, args: &[Expr], indent: usize) {
    out.push_str(name);
    if args.is_empty() {
        out.push_str("()");
        return;
    }
    if MULTILINE_CALLS.contains(&name) {
        out.push_str("(\n");
        for arg in args {
            push_indent(out, indent + 1);
            write_expr(out, arg, P_LAMBDA, false, indent + 1);
            out.push_str(",\n");
        }
        push_indent(out, indent);
        out.push(')');
    } else {
        out.push('(');
        write_args_inline(out, args, indent);
        out.push(')');
    }
}

fn write_args_inline(out: &mut String, args: &[Expr], indent: usize) {
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        write_expr(out, arg, P_LAMBDA, false, indent);
    }
}

// ── Islands (mini-notation) ───────────────────────────────────────────────────

fn write_island(out: &mut String, isl: &Island, indent: usize) {
    out.push_str(match isl.kind {
        IslandKind::Sound => "s",
        IslandKind::Note => "n",
    });
    out.push('(');
    write_parallel_w(out, &isl.root, indent);
    out.push(')');
}

// ── Width-aware wrapping ────────────────────────────────────────────────────────
//
// Each `_w` writer first renders its node *inline* (reusing the plain writers
// below) and, if it still fits before the [`MAX_WIDTH`] margin at the current
// column, emits that verbatim. Only when it would overflow does it break — at the
// one structural seam that reads well: `<...>`/`[...]` put one element per line,
// `a & b` put one lane per line. The plain writers stay the single source of
// truth for the inline form (so wrapped and inline output can't drift apart).

/// Current column: bytes since the last newline (island tokens are ASCII, so
/// byte length tracks visible width).
fn cur_col(out: &str) -> usize {
    match out.rfind('\n') {
        Some(i) => out.len() - i - 1,
        None => out.len(),
    }
}

/// True if `m`'s inline form fits before the margin at the current column.
fn fits_inline(out: &str, m: &Mini, render: fn(&mut String, &Mini)) -> Option<String> {
    let mut inline = String::new();
    render(&mut inline, m);
    if !inline.contains('\n') && cur_col(out) + inline.len() <= MAX_WIDTH {
        Some(inline)
    } else {
        None
    }
}

fn write_parallel_w(out: &mut String, m: &Mini, indent: usize) {
    if let Some(inline) = fits_inline(out, m, write_parallel) {
        out.push_str(&inline);
        return;
    }
    match &m.kind {
        MiniKind::Parallel(lanes) => {
            for (i, lane) in lanes.iter().enumerate() {
                if i > 0 {
                    out.push('\n');
                    push_indent(out, indent);
                    out.push_str("& ");
                }
                write_sequence_w(out, lane, indent + 1);
            }
        }
        _ => write_sequence_w(out, m, indent),
    }
}

fn write_sequence_w(out: &mut String, m: &Mini, indent: usize) {
    if let Some(inline) = fits_inline(out, m, write_sequence) {
        out.push_str(&inline);
        return;
    }
    match &m.kind {
        MiniKind::Sequence(items) => {
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push('\n');
                    push_indent(out, indent);
                }
                write_term_w(out, item, indent);
            }
        }
        _ => write_term_w(out, m, indent),
    }
}

fn write_term_w(out: &mut String, m: &Mini, indent: usize) {
    if let Some(inline) = fits_inline(out, m, write_term) {
        out.push_str(&inline);
        return;
    }
    match &m.kind {
        MiniKind::Term { atom, postfixes } => {
            write_atom_w(out, atom, indent);
            for pf in postfixes {
                write_postfix(out, pf);
            }
        }
        _ => write_atom_w(out, m, indent),
    }
}

fn write_atom_w(out: &mut String, m: &Mini, indent: usize) {
    if let Some(inline) = fits_inline(out, m, write_atom) {
        out.push_str(&inline);
        return;
    }
    match &m.kind {
        MiniKind::Group(inner) => {
            out.push('[');
            write_parallel_w(out, inner, indent + 1);
            out.push(']');
        }
        MiniKind::Alt(inner) => {
            out.push_str("<\n");
            push_indent(out, indent + 1);
            write_parallel_w(out, inner, indent + 1);
            out.push('\n');
            push_indent(out, indent);
            out.push('>');
        }
        MiniKind::Poly { body, steps } => {
            out.push('{');
            write_parallel_w(out, body, indent + 1);
            out.push('}');
            if let Some(n) = steps {
                out.push('%');
                out.push_str(&n.to_string());
            }
        }
        // Defensive (a composite where an atom was expected): bracket then wrap.
        MiniKind::Parallel(_) => {
            out.push('[');
            write_parallel_w(out, m, indent + 1);
            out.push(']');
        }
        MiniKind::Sequence(_) => {
            out.push('[');
            write_sequence_w(out, m, indent + 1);
            out.push(']');
        }
        // Leaves and the like never overflow on their own.
        _ => write_atom(out, m),
    }
}

/// `a & b & c` — the loosest island layer. Brackets/angles are *not* added here:
/// they come from explicit [`MiniKind::Group`] / [`MiniKind::Alt`] nodes (see
/// [`write_atom`]).
fn write_parallel(out: &mut String, m: &Mini) {
    match &m.kind {
        MiniKind::Parallel(lanes) => {
            for (i, lane) in lanes.iter().enumerate() {
                if i > 0 {
                    out.push_str(" & ");
                }
                write_sequence(out, lane);
            }
        }
        _ => write_sequence(out, m),
    }
}

/// Space-separated terms.
fn write_sequence(out: &mut String, m: &Mini) {
    match &m.kind {
        MiniKind::Sequence(items) => {
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                write_term(out, item);
            }
        }
        _ => write_term(out, m),
    }
}

/// An atom followed by its postfix chain (`bd*2`, `cp:2(3,8)`).
fn write_term(out: &mut String, m: &Mini) {
    match &m.kind {
        MiniKind::Term { atom, postfixes } => {
            write_atom(out, atom);
            for pf in postfixes {
                write_postfix(out, pf);
            }
        }
        _ => write_atom(out, m),
    }
}

fn write_atom(out: &mut String, m: &Mini) {
    match &m.kind {
        MiniKind::Leaf(l) => write_leaf(out, l),
        MiniKind::Rest => out.push('~'),
        MiniKind::Extend => out.push('_'),
        MiniKind::Splice(id) => {
            out.push('$');
            out.push_str(&id.name);
        }
        MiniKind::Group(inner) => {
            out.push('[');
            write_parallel(out, inner);
            out.push(']');
        }
        MiniKind::Alt(inner) => {
            out.push('<');
            write_parallel(out, inner);
            out.push('>');
        }
        MiniKind::Poly { body, steps } => {
            out.push('{');
            write_parallel(out, body);
            out.push('}');
            if let Some(n) = steps {
                out.push('%');
                out.push_str(&n.to_string());
            }
        }
        // Defensive: a composite where the grammar expects an atom (a hand-built
        // or materialised tree that skipped a Group node). Bracket it so the
        // text still re-parses to the same structure.
        MiniKind::Parallel(_) => {
            out.push('[');
            write_parallel(out, m);
            out.push(']');
        }
        MiniKind::Sequence(_) => {
            out.push('[');
            write_sequence(out, m);
            out.push(']');
        }
        MiniKind::Term { .. } => write_term(out, m),
    }
}

fn write_leaf(out: &mut String, l: &Leaf) {
    match l {
        Leaf::Sound(s) | Leaf::NoteName(s) => out.push_str(s),
        Leaf::Degree(d) => out.push_str(&d.to_string()),
    }
}

fn write_postfix(out: &mut String, pf: &Postfix) {
    match pf {
        Postfix::Fast(n) => {
            out.push('*');
            write_mini_arg(out, n);
        }
        Postfix::Slow(n) => {
            out.push('/');
            write_mini_arg(out, n);
        }
        Postfix::Replicate(n) => {
            out.push('!');
            out.push_str(&n.to_string());
        }
        Postfix::Weight(n) => {
            out.push('@');
            out.push_str(&n.to_string());
        }
        Postfix::Euclid {
            pulses,
            steps,
            rotation,
        } => {
            out.push('(');
            write_mini_arg(out, pulses);
            out.push(',');
            write_mini_arg(out, steps);
            if let Some(r) = rotation {
                out.push(',');
                write_mini_arg(out, r);
            }
            out.push(')');
        }
        Postfix::Variant(n) => {
            out.push(':');
            out.push_str(&n.to_string());
        }
        Postfix::Chord(name) => {
            out.push('\'');
            out.push_str(name);
        }
    }
}

/// A postfix factor: a literal number, or a patternised sub-pattern printed as
/// its own mini-notation atom (`<2 3>`, `[2 3]`, `{2 3}`).
fn write_mini_arg(out: &mut String, arg: &MiniArg) {
    match arg {
        MiniArg::Const(n) => out.push_str(&fmt_number(*n)),
        MiniArg::Pat(m) => write_atom(out, m),
    }
}

// ── Leaf helpers ──────────────────────────────────────────────────────────────

/// Shortest round-trip number. Rust's `{}` for `f64` is already shortest and
/// never uses exponent notation, so it matches the grammar's `NUMBER` directly:
/// integers print with no `.0`, fractions with no trailing zeros.
fn fmt_number(n: f64) -> String {
    format!("{n}")
}

fn write_quoted(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            _ => out.push(c),
        }
    }
    out.push('"');
}

fn push_indent(out: &mut String, levels: usize) {
    for _ in 0..levels {
        out.push_str(INDENT);
    }
}
