//! The server-backed packs: declarations taken from a language server's document outline.
//!
//! Bennu already asks every server it routes to for `textDocument/documentSymbol` — that is the
//! Structure panel. An outline entry carries a `kind` and the byte range of the **name**, which is
//! exactly what a naming rule needs, so a whole family of languages is covered without a single new
//! grammar.
//!
//! ## What an outline cannot tell you, and what that costs
//!
//! An outline lists types and their members. It does **not** contain locals or parameters, so those
//! two targets never fire here and [`crate::pack::Pack::supports`] says so — a settings screen greys
//! them out rather than offering a rule that would silently do nothing.
//!
//! It also needs the server installed and warm. No server, no outline, no check — which is honest:
//! the alternative would be a second, worse answer for a language something else already reads
//! properly.
//!
//! ## Kinds nobody maps, on purpose
//!
//! `variable` and, for TypeScript and JavaScript, `constant` are deliberately **not** mapped. A
//! module-level binding in those languages is spelled `camelCase` or `UPPER_SNAKE_CASE` depending
//! on whether the author considers it a constant — a distinction the declaration does not carry and
//! a server does not report. A rule there would be right half the time, which for a check that runs
//! on every file is the same as being wrong. Rust has no such ambiguity: a `const` is a `const`, so
//! its `constant` kind maps.
//!
//! An unmapped kind is skipped, but its **children are still visited** — an `impl` block is not a
//! declaration whose name anyone chose, and the methods inside it very much are.

use bennu_proto::prelude::LspSymbol;

use crate::convention::Convention::{Camel, LowerSnake, Pascal, UpperSnake};
use crate::pack::{DeclSource, Declared, Pack};
use crate::target::Target::{Constant, EnumConstant, Field, Method, Package, Type};

pub const TYPESCRIPT: Pack = Pack {
    id: "typescript",
    label: "TypeScript",
    extensions: &["ts", "tsx", "mts", "cts"],
    standard: &[(Type, Pascal), (Method, Camel), (Field, Camel), (EnumConstant, Pascal)],
    source: DeclSource::Symbols(&[
        ("class", Type),
        ("interface", Type),
        ("enum", Type),
        ("struct", Type),
        ("method", Method),
        ("function", Method),
        ("property", Field),
        ("field", Field),
        ("enum-member", EnumConstant),
    ]),
};

pub const JAVASCRIPT: Pack = Pack {
    id: "javascript",
    label: "JavaScript",
    extensions: &["js", "jsx", "mjs", "cjs"],
    standard: &[(Type, Pascal), (Method, Camel), (Field, Camel)],
    source: DeclSource::Symbols(&[
        ("class", Type),
        ("method", Method),
        ("function", Method),
        ("property", Field),
        ("field", Field),
    ]),
};

pub const RUST: Pack = Pack {
    id: "rust",
    label: "Rust",
    extensions: &["rs"],
    standard: &[
        (Type, Pascal),
        (Method, LowerSnake),
        (Field, LowerSnake),
        (Constant, UpperSnake),
        (EnumConstant, Pascal),
        (Package, LowerSnake),
    ],
    // rust-analyzer renames a few kinds into Rust's own vocabulary before they reach here
    // (`interface` → `trait`, `type-parameter` → `type alias`), so those are the spellings to match.
    source: DeclSource::Symbols(&[
        ("struct", Type),
        ("enum", Type),
        ("trait", Type),
        ("type alias", Type),
        ("class", Type),
        ("function", Method),
        ("method", Method),
        ("field", Field),
        ("constant", Constant),
        ("enum-member", EnumConstant),
        ("module", Package),
    ]),
};

/// Flatten an outline into the declarations `pack` recognises.
///
/// The name is taken from the source by the symbol's **name range** rather than from its `name`
/// field: a server is free to put a signature or a qualifier in there (`fn foo(a: u32)`,
/// `impl Display for Foo`), and renaming what the range covers is the only reading that stays true
/// to what a rename would actually replace. The `name` field is the fallback when the range is not
/// usable.
pub fn declarations_from(pack: &Pack, symbols: &[LspSymbol], source: &str) -> Vec<Declared> {
    let mut out = Vec::new();
    collect(pack, symbols, source, &mut out);
    out
}

fn collect(pack: &Pack, symbols: &[LspSymbol], source: &str, out: &mut Vec<Declared>) {
    for symbol in symbols {
        if let Some(target) = pack.target_for_kind(&symbol.kind) {
            if let Some(name) = name_of(symbol, source) {
                out.push(Declared {
                    target,
                    name,
                    start: symbol.name_start,
                    end: symbol.name_end,
                });
            }
        }
        // Even when this symbol is not itself checkable — an `impl` block, a namespace — what is
        // nested inside it is.
        collect(pack, &symbol.children, source, out);
    }
}

/// The identifier text of `symbol`, from its name range where that is usable.
fn name_of(symbol: &LspSymbol, source: &str) -> Option<String> {
    let (start, end) = (symbol.name_start, symbol.name_end);
    if start < end
        && end <= source.len()
        && source.is_char_boundary(start)
        && source.is_char_boundary(end)
    {
        let sliced = &source[start..end];
        // A server that points the "name range" at a whole signature would otherwise have us
        // rename the signature. An identifier has no spaces or brackets in any language here.
        if !sliced.is_empty() && sliced.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$')
        {
            return Some(sliced.to_string());
        }
    }
    (!symbol.name.is_empty()).then(|| symbol.name.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn symbol(kind: &str, name: &str, source: &str, children: Vec<LspSymbol>) -> LspSymbol {
        let start = source.find(name).unwrap_or(0);
        LspSymbol {
            name: name.to_string(),
            kind: kind.to_string(),
            detail: None,
            start,
            end: start + name.len(),
            name_start: start,
            name_end: start + name.len(),
            line: 1,
            col: 1,
            file: "a.ts".to_string(),
            deprecated: false,
            children,
        }
    }

    #[test]
    fn maps_the_kinds_it_knows_and_skips_the_rest() {
        let source = "class order_service { do_work() {} } let loose_binding = 1;";
        let outline = vec![
            symbol("class", "order_service", source, vec![symbol("method", "do_work", source, vec![])]),
            symbol("variable", "loose_binding", source, vec![]),
        ];
        let found = declarations_from(&TYPESCRIPT, &outline, source);
        let names: Vec<&str> = found.iter().map(|d| d.name.as_str()).collect();
        // `variable` is deliberately unmapped — see the module doc.
        assert_eq!(names, ["order_service", "do_work"]);
        assert_eq!(found[0].target, Type);
        assert_eq!(found[1].target, Method);
    }

    #[test]
    fn children_of_an_unmapped_symbol_are_still_collected() {
        let source = "impl Foo { fn do_work() {} }";
        let outline =
            vec![symbol("impl", "impl Foo", source, vec![symbol("function", "do_work", source, vec![])])];
        let found = declarations_from(&RUST, &outline, source);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].name, "do_work");
    }

    #[test]
    fn the_name_comes_from_the_range_not_the_label() {
        let source = "fn do_work(a: u32) {}";
        let mut sym = symbol("function", "do_work", source, vec![]);
        // A server that labels the entry with the whole signature must not have us rename that.
        sym.name = "do_work(a: u32)".to_string();
        let found = declarations_from(&RUST, &[sym], source);
        assert_eq!(found[0].name, "do_work");
    }

    #[test]
    fn a_name_range_that_is_not_an_identifier_falls_back_to_the_label() {
        let source = "impl Display for Foo {}";
        let mut sym = symbol("struct", "Foo", source, vec![]);
        sym.name_start = 0;
        sym.name_end = source.len(); // the whole line — not an identifier
        let found = declarations_from(&RUST, &[sym], source);
        assert_eq!(found[0].name, "Foo");
    }

    #[test]
    fn rust_maps_its_own_vocabulary() {
        let source = "trait Reader {} mod some_mod {} const MAX: u8 = 1;";
        let outline = vec![
            symbol("trait", "Reader", source, vec![]),
            symbol("module", "some_mod", source, vec![]),
            symbol("constant", "MAX", source, vec![]),
        ];
        let found = declarations_from(&RUST, &outline, source);
        let targets: Vec<_> = found.iter().map(|d| d.target).collect();
        assert_eq!(targets, [Type, Package, Constant]);
    }
}
