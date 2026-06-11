//! Front-end entry point: source text → [`Program`] AST.
//!
//! Drives the committed Tree-sitter parser (`src/parser.c`, compiled by
//! `build.rs`) and walks the resulting CST into the typed [`ast`](crate::ast).
//! The walker is mechanical: every named grammar node maps onto exactly one AST
//! variant (`src/node-types.json` is the authoritative shape), and spans come
//! straight from the nodes' native byte ranges. Syntax errors (TS `ERROR` /
//! missing nodes) surface as a located [`LangError`].

use arbor_grove_pattern::prelude::SourceSpan;
use tree_sitter::{Node, Parser};

use crate::ast::{
    BinOp, Expr, ExprKind, FnDef, Ident, Import, Island, IslandKind, Item, Leaf, LetBind, Mini,
    MiniKind, Postfix, Program, UnOp,
};
use crate::error::{LangError, LangErrorKind, Result};

extern "C" {
    fn tree_sitter_grove() -> *const ();
}

/// Parse `.grove` source into an AST.
pub fn parse(source: &str) -> Result<Program> {
    let language = unsafe { tree_sitter::Language::from_raw(tree_sitter_grove().cast()) };
    let mut parser = Parser::new();
    parser
        .set_language(&language)
        .map_err(|e| LangError::unlocated(LangErrorKind::Parse(format!("load grammar: {e}"))))?;

    let tree = parser.parse(source.as_bytes(), None).ok_or_else(|| {
        LangError::unlocated(LangErrorKind::Parse("the parser produced no tree".to_string()))
    })?;
    let root = tree.root_node();

    if let Some(err) = first_error(root) {
        let msg = if err.is_missing() {
            format!("missing `{}`", err.kind())
        } else {
            "syntax error".to_string()
        };
        return Err(LangError::at(span(err), LangErrorKind::Parse(msg)));
    }

    walk_program(root, source)
}

// ── Diagnostics ────────────────────────────────────────────────────────────────

/// First `ERROR` / missing node in pre-order, for a located parse error.
fn first_error(node: Node) -> Option<Node> {
    if node.is_error() || node.is_missing() {
        return Some(node);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(e) = first_error(child) {
            return Some(e);
        }
    }
    None
}

// ── Items ──────────────────────────────────────────────────────────────────────

fn walk_program(root: Node, src: &str) -> Result<Program> {
    let mut items = Vec::new();
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        if child.kind() == "comment" {
            continue;
        }
        items.push(walk_item(child, src)?);
    }
    Ok(Program { items })
}

fn walk_item(node: Node, src: &str) -> Result<Item> {
    match node.kind() {
        "import_statement" => Ok(Item::Import(walk_import(node, src)?)),
        "let_binding" => Ok(Item::Let(walk_let(node, src)?)),
        "fn_definition" => Ok(Item::Fn(walk_fn(node, src)?)),
        _ => Ok(Item::Expr(walk_expr(node, src)?)),
    }
}

fn walk_import(node: Node, src: &str) -> Result<Import> {
    let mut names = Vec::new();
    let mut cursor = node.walk();
    for n in node.children_by_field_name("name", &mut cursor) {
        names.push(ident(n, src));
    }
    let path = unquote(node_text(field(node, "path")?, src));
    Ok(Import {
        names,
        path,
        span: span(node),
    })
}

fn walk_let(node: Node, src: &str) -> Result<LetBind> {
    Ok(LetBind {
        name: ident(field(node, "name")?, src),
        value: walk_expr(field(node, "value")?, src)?,
        span: span(node),
    })
}

fn walk_fn(node: Node, src: &str) -> Result<FnDef> {
    let name = ident(field(node, "name")?, src);
    let mut params = Vec::new();
    if let Some(p) = node.child_by_field_name("params") {
        let mut cursor = p.walk();
        for id in p.named_children(&mut cursor) {
            if id.kind() == "identifier" {
                params.push(ident(id, src));
            }
        }
    }
    Ok(FnDef {
        name,
        params,
        body: walk_expr(field(node, "body")?, src)?,
        span: span(node),
    })
}

// ── Expressions ────────────────────────────────────────────────────────────────

fn walk_expr(node: Node, src: &str) -> Result<Expr> {
    // Parentheses carry no AST node — unwrap to the inner expression (the
    // emitter re-derives minimal parens from precedence).
    if node.kind() == "parenthesized" {
        return walk_expr(first_named(node)?, src);
    }

    let span = span(node);
    let kind = match node.kind() {
        "number" => ExprKind::Number(num_f64(node, src)?),
        "string" => ExprKind::Str(unquote(node_text(node, src))),
        "note_literal" => ExprKind::Note(node_text(node, src).to_string()),
        "identifier" => ExprKind::Var(node_text(node, src).to_string()),
        "call_expression" => ExprKind::Call {
            name: ident(field(node, "function")?, src),
            args: walk_args(field(node, "arguments")?, src)?,
        },
        "method_call" => ExprKind::Method {
            recv: Box::new(walk_expr(field(node, "receiver")?, src)?),
            name: ident(field(node, "method")?, src),
            args: walk_args(field(node, "arguments")?, src)?,
        },
        "unary_expression" => ExprKind::Unary {
            op: UnOp::Neg,
            rhs: Box::new(walk_expr(field(node, "operand")?, src)?),
        },
        "binary_expression" => ExprKind::Binary {
            op: bin_op(field(node, "operator")?)?,
            lhs: Box::new(walk_expr(field(node, "left")?, src)?),
            rhs: Box::new(walk_expr(field(node, "right")?, src)?),
        },
        "range_expression" => ExprKind::Range {
            lo: Box::new(walk_expr(field(node, "lo")?, src)?),
            hi: Box::new(walk_expr(field(node, "hi")?, src)?),
            inclusive: field(node, "operator")?.kind() == "range_inclusive_op",
        },
        "lambda" => walk_lambda(node, src)?,
        "island" => ExprKind::Island(walk_island(node, src)?),
        other => return Err(parse_err(node, &format!("unexpected expression `{other}`"))),
    };
    Ok(Expr { kind, span })
}

fn walk_lambda(node: Node, src: &str) -> Result<ExprKind> {
    let mut params = Vec::new();
    let mut cursor = node.walk();
    // The `params` field gathers the parens too (`(`, `parameters`, `)`); pick
    // out the identifiers — either a lone one or the `parameters` node's.
    for p in node.children_by_field_name("params", &mut cursor) {
        match p.kind() {
            "identifier" => params.push(ident(p, src)),
            "parameters" => {
                let mut inner = p.walk();
                for id in p.named_children(&mut inner) {
                    if id.kind() == "identifier" {
                        params.push(ident(id, src));
                    }
                }
            }
            _ => {}
        }
    }
    Ok(ExprKind::Lambda {
        params,
        body: Box::new(walk_expr(field(node, "body")?, src)?),
    })
}

fn walk_args(node: Node, src: &str) -> Result<Vec<Expr>> {
    let mut args = Vec::new();
    let mut cursor = node.walk();
    for c in node.named_children(&mut cursor) {
        if c.kind() == "comment" {
            continue;
        }
        args.push(walk_expr(c, src)?);
    }
    Ok(args)
}

fn bin_op(node: Node) -> Result<BinOp> {
    Ok(match node.kind() {
        "+" => BinOp::Add,
        "-" => BinOp::Sub,
        "*" => BinOp::Mul,
        "/" => BinOp::Div,
        _ => return Err(parse_err(node, "unknown operator")),
    })
}

// ── Islands (mini-notation) ─────────────────────────────────────────────────────

fn walk_island(node: Node, src: &str) -> Result<Island> {
    Ok(Island {
        kind: island_kind(field(node, "open")?, src),
        root: walk_mini(field(node, "body")?, src)?,
        span: span(node),
    })
}

/// The island kind is read off the `island_start` token (`s(` / `sound(` /
/// `n(` / `note(`).
fn island_kind(open: Node, src: &str) -> IslandKind {
    match node_text(open, src).trim_end_matches('(').trim() {
        "n" | "note" => IslandKind::Note,
        _ => IslandKind::Sound,
    }
}

fn walk_mini(node: Node, src: &str) -> Result<Mini> {
    let span = span(node);
    let kind = match node.kind() {
        "parallel" => MiniKind::Parallel(walk_mini_children(node, src)?),
        "sequence" => MiniKind::Sequence(walk_mini_children(node, src)?),
        "term" => {
            let atom = Box::new(walk_mini(field(node, "atom")?, src)?);
            let mut postfixes = Vec::new();
            let mut cursor = node.walk();
            for p in node.children_by_field_name("postfix", &mut cursor) {
                postfixes.push(walk_postfix(p, src)?);
            }
            MiniKind::Term { atom, postfixes }
        }
        "group" => MiniKind::Group(Box::new(walk_mini(first_named(node)?, src)?)),
        "alternation" => MiniKind::Alt(Box::new(walk_mini(first_named(node)?, src)?)),
        "rest" => MiniKind::Rest,
        "extend" => MiniKind::Extend,
        "splice" => MiniKind::Splice(ident(field(node, "name")?, src)),
        "sound_name" => MiniKind::Leaf(Leaf::Sound(node_text(node, src).to_string())),
        "note_name" => MiniKind::Leaf(Leaf::NoteName(node_text(node, src).to_string())),
        "integer" => MiniKind::Leaf(Leaf::Degree(int_i32(node, src)?)),
        other => return Err(parse_err(node, &format!("unexpected mini-notation node `{other}`"))),
    };
    Ok(Mini { kind, span })
}

fn walk_mini_children(node: Node, src: &str) -> Result<Vec<Mini>> {
    let mut out = Vec::new();
    let mut cursor = node.walk();
    for c in node.named_children(&mut cursor) {
        if c.kind() == "comment" {
            continue;
        }
        out.push(walk_mini(c, src)?);
    }
    Ok(out)
}

fn walk_postfix(node: Node, src: &str) -> Result<Postfix> {
    Ok(match node.kind() {
        "fast" => Postfix::Fast(num_f64(field(node, "n")?, src)?),
        "slow" => Postfix::Slow(num_f64(field(node, "n")?, src)?),
        "replicate" => Postfix::Replicate(int_u32(field(node, "n")?, src)?),
        "weight" => Postfix::Weight(int_u32(field(node, "n")?, src)?),
        "variant" => Postfix::Variant(int_u32(field(node, "n")?, src)?),
        "euclid" => Postfix::Euclid {
            pulses: int_u32(field(node, "pulses")?, src)?,
            steps: int_u32(field(node, "steps")?, src)?,
            rotation: match node.child_by_field_name("rotation") {
                Some(r) => Some(int_i32(r, src)?),
                None => None,
            },
        },
        "chord" => Postfix::Chord(node_text(field(node, "name")?, src).to_string()),
        other => return Err(parse_err(node, &format!("unexpected postfix `{other}`"))),
    })
}

// ── Node helpers ────────────────────────────────────────────────────────────────

fn span(node: Node) -> SourceSpan {
    SourceSpan::new(node.start_byte() as u32, node.end_byte() as u32)
}

fn node_text<'a>(node: Node, src: &'a str) -> &'a str {
    &src[node.start_byte()..node.end_byte()]
}

fn ident(node: Node, src: &str) -> Ident {
    Ident {
        name: node_text(node, src).to_string(),
        span: span(node),
    }
}

/// A required child by field name.
fn field<'a>(node: Node<'a>, name: &str) -> Result<Node<'a>> {
    node.child_by_field_name(name)
        .ok_or_else(|| LangError::at(span(node), LangErrorKind::Parse(format!("missing `{name}`"))))
}

/// The first named, non-comment child (for `[...]` / `<...>` / `(...)`).
fn first_named(node: Node) -> Result<Node> {
    let mut cursor = node.walk();
    for c in node.named_children(&mut cursor) {
        if c.kind() != "comment" {
            return Ok(c);
        }
    }
    Err(parse_err(node, "expected a child node"))
}

fn num_f64(node: Node, src: &str) -> Result<f64> {
    node_text(node, src)
        .parse::<f64>()
        .map_err(|_| parse_err(node, "invalid number"))
}

fn int_u32(node: Node, src: &str) -> Result<u32> {
    node_text(node, src)
        .parse::<u32>()
        .map_err(|_| parse_err(node, "invalid integer"))
}

fn int_i32(node: Node, src: &str) -> Result<i32> {
    node_text(node, src)
        .parse::<i32>()
        .map_err(|_| parse_err(node, "invalid integer"))
}

fn parse_err(node: Node, msg: &str) -> LangError {
    LangError::at(span(node), LangErrorKind::Parse(msg.to_string()))
}

/// Strip the surrounding quotes of a `string` token and unescape `\"` / `\\`.
fn unquote(raw: &str) -> String {
    let inner = raw
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(raw);
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(c);
        }
    }
    out
}
