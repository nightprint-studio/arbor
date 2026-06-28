//! Tiny constructors for `merula-lang` AST nodes.
//!
//! Kept apart from [`crate::emit`] so that file stays about *structure policy*
//! (how a [`Song`](crate::model::Song) maps to phrases, sections and lets) while
//! this one is just the boilerplate of building well-formed nodes. Every node is
//! given a zero span (`0..0`): the import emitter never points back at source, and
//! uniform spans make structural equality (used to dedup repeated phrases) compare
//! shape only.

use merula_lang::prelude::{
    Expr, ExprKind, Ident, Island, IslandKind, Leaf, Mini, MiniKind, Program, SourceSpan,
};

/// The zero span shared by every constructed node.
pub(crate) fn sp() -> SourceSpan {
    SourceSpan::new(0, 0)
}

fn ex(kind: ExprKind) -> Expr {
    Expr { kind, span: sp() }
}

pub(crate) fn num(n: f64) -> Expr {
    ex(ExprKind::Number(n))
}

pub(crate) fn string(s: &str) -> Expr {
    ex(ExprKind::Str(s.to_string()))
}

pub(crate) fn var(name: &str) -> Expr {
    ex(ExprKind::Var(name.to_string()))
}

pub(crate) fn ident(name: &str) -> Ident {
    Ident {
        name: name.to_string(),
        span: sp(),
    }
}

pub(crate) fn call(name: &str, args: Vec<Expr>) -> Expr {
    ex(ExprKind::Call {
        name: ident(name),
        args,
    })
}

pub(crate) fn method(recv: Expr, name: &str, args: Vec<Expr>) -> Expr {
    ex(ExprKind::Method {
        recv: Box::new(recv),
        name: ident(name),
        args,
    })
}

pub(crate) fn island(kind: IslandKind, root: Mini) -> Expr {
    ex(ExprKind::Island(Island {
        kind,
        root,
        span: sp(),
    }))
}

pub(crate) fn let_item(name: &str, value: Expr) -> merula_lang::prelude::Item {
    merula_lang::prelude::Item::Let(merula_lang::prelude::LetBind {
        name: ident(name),
        value,
        span: sp(),
    })
}

pub(crate) fn program(items: Vec<merula_lang::prelude::Item>) -> Program {
    Program { items }
}

pub(crate) fn mini(kind: MiniKind) -> Mini {
    Mini { kind, span: sp() }
}

pub(crate) fn leaf(l: Leaf) -> Mini {
    mini(MiniKind::Leaf(l))
}
