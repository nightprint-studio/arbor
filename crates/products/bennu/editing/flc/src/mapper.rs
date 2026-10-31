//! Field-by-field mapping between two classes — the next `target.setX(source.getX());` the author is about to type.
//!
//! In a converter (`Dto toDto(Entity e)`) most of the body is one statement per field, and which field comes next is
//! not a guess: the target's properties that no line has assigned yet, matched by name to what the source offers. This
//! module is the pure half — reading the lines above the caret for the style and the variables, matching names and
//! types, rendering the line — and it knows no index: the host hands it the properties as plain data.
//!
//! What it will not do is invent a conversion: a property is matched only to one whose type is the same (or the same
//! type boxed), by a name that is equal, equal after case/underscore/plural, or `id` against `customerId`.

use std::collections::HashSet;

/// Below this a match is not offered.
pub const MIN_CONFIDENCE: f32 = 0.7;

/// How the lines above assign the target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    /// `target.setX(source.getX());`
    Setter,
    /// `.x(source.getX())` in a builder chain.
    Builder,
}

/// What the lines above the caret say about the conversion in progress.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Context {
    pub style: Style,
    /// The variable being filled (setter style), when a line names it.
    pub target_var: Option<String>,
    /// The target's class as written at its construction, when that line is above.
    pub target_type: Option<String>,
    /// The variable the values are read from, when a mapping line names it.
    pub source_var: Option<String>,
    /// The properties the lines above already assign, as written (`Name`, `name`).
    pub assigned: Vec<String>,
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

fn leading_ident(s: &str) -> &str {
    let end = s.char_indices().find(|(_, c)| !is_ident(*c)).map_or(s.len(), |(i, _)| i);
    &s[..end]
}

/// `inner` as `var.accessor()` → `var`.
fn source_of(inner: &str) -> Option<String> {
    let inner = inner.trim();
    let var = leading_ident(inner);
    let rest = inner[var.len()..].strip_prefix('.')?;
    let accessor = leading_ident(rest);
    (!var.is_empty() && !accessor.is_empty() && &rest[accessor.len()..] == "()").then(|| var.to_string())
}

/// The text between the parentheses opened right after `s`'s start, when it ends with the matching close.
fn argument(s: &str) -> Option<&str> {
    let s = s.trim_end().trim_end_matches(';').trim_end();
    let inner = s.strip_prefix('(')?.strip_suffix(')')?;
    Some(inner)
}

/// `target.setX(source.getX());` → (target, `X`, source).
fn parse_setter(line: &str) -> Option<(String, String, String)> {
    let var = leading_ident(line);
    let rest = line[var.len()..].strip_prefix(".set")?;
    let prop = leading_ident(rest);
    if var.is_empty() || !prop.starts_with(|c: char| c.is_uppercase()) {
        return None;
    }
    let source = source_of(argument(&rest[prop.len()..])?)?;
    Some((var.to_string(), prop.to_string(), source))
}

/// `.x(source.getX())` → (`x`, source).
fn parse_builder(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix('.')?;
    let name = leading_ident(rest);
    if name.is_empty() || name == "build" {
        return None;
    }
    let source = source_of(argument(&rest[name.len()..])?)?;
    Some((name.to_string(), source))
}

/// The class named right after `new` or right before `.builder()`, and the variable declared on that line.
fn parse_construction(line: &str) -> Option<(Style, Option<String>, String)> {
    let line = line.trim_end().trim_end_matches(';');
    let (head, style, ty) = if let Some((head, tail)) = line.split_once(" new ") {
        (head, Style::Setter, leading_ident(tail.trim_start()).to_string())
    } else if let Some(at) = line.find(".builder()") {
        let before = &line[..at];
        let ty: String = before.chars().rev().take_while(|c| is_ident(*c)).collect::<String>().chars().rev().collect();
        (&before[..before.len() - ty.len()], Style::Builder, ty)
    } else {
        return None;
    };
    if ty.is_empty() || !ty.starts_with(|c: char| c.is_uppercase()) {
        return None;
    }
    // `Type name =`, `var name =`, `final Type name =`; `return` and a bare builder declare nothing
    let var = head.trim_end().strip_suffix('=').map(|l| l.trim_end()).and_then(|l| {
        let name: String = l.chars().rev().take_while(|c| is_ident(*c)).collect::<String>().chars().rev().collect();
        (!name.is_empty() && l.len() > name.len()).then_some(name)
    });
    Some((style, var, ty))
}

/// Reads the statements above the caret line. `before` is the text up to the start of the caret's line.
///
/// `None` unless the line right above (blank lines aside) is a mapping line or the construction of the target.
pub fn read_context(before: &str) -> Option<Context> {
    let mut ctx: Option<Context> = None;
    for raw in before.lines().rev() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let mapped = parse_setter(line).map(|(t, p, s)| (Style::Setter, Some(t), p, s)).or_else(|| parse_builder(line).map(|(p, s)| (Style::Builder, None, p, s)));
        match (mapped, ctx.as_mut()) {
            (Some((style, target, prop, source)), None) => {
                ctx = Some(Context { style, target_var: target, target_type: None, source_var: Some(source), assigned: vec![prop] });
            }
            (Some((style, target, prop, _)), Some(c)) if c.style == style && (style == Style::Builder || c.target_var == target) => {
                c.assigned.push(prop);
            }
            (Some(_), Some(_)) => return None,
            (None, _) => {
                if let Some((style, var, ty)) = parse_construction(line) {
                    match ctx.as_mut() {
                        Some(c) if c.style == style => {
                            c.target_type = Some(ty);
                            c.target_var = c.target_var.take().or(var);
                        }
                        Some(_) => return None,
                        None => {
                            ctx = Some(Context { style, target_var: var, target_type: Some(ty), source_var: None, assigned: Vec::new() })
                        }
                    }
                } else if ctx.is_none() {
                    return None;
                }
                // a line that is neither ends the run of mapping lines
                break;
            }
        }
    }
    ctx
}

/// A property of a class: the name its accessor or setter stands for, the method that reads or writes it, its type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prop {
    /// `Name` for `getName`/`setName`; the method's own name for a builder method or a record accessor.
    pub name: String,
    /// The method to call.
    pub method: String,
    /// The type, fully qualified with its arguments (`java.util.List<java.lang.String>`).
    pub ty: String,
}

/// Lower case, no `_`/`-`, plural folded to singular: the form two spellings of one name share.
pub fn stem(name: &str) -> String {
    let n: String = name.chars().filter(|c| *c != '_' && *c != '-').flat_map(char::to_lowercase).collect();
    if let Some(base) = n.strip_suffix("ies").filter(|b| !b.is_empty()) {
        format!("{base}y")
    } else if n.len() > 3 && n.ends_with('s') && !n.ends_with("ss") {
        n[..n.len() - 1].to_string()
    } else {
        n
    }
}

/// Drops a role suffix so `CustomerDto` and `CustomerEntity` both read `customer`.
fn core_of(type_name: &str) -> String {
    let simple = type_name.rsplit('.').next().unwrap_or(type_name);
    let lower = simple.to_ascii_lowercase();
    for suffix in ["dto", "entity", "model", "vo", "bean", "request", "response", "view", "resource"] {
        if let Some(c) = lower.strip_suffix(suffix).filter(|c| !c.is_empty()) {
            return c.to_string();
        }
    }
    lower
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum NameMatch {
    Id,
    Normalised,
    Same,
}

fn name_match(target: &str, source: &str, target_type: &str, source_type: &str) -> Option<NameMatch> {
    if target.eq_ignore_ascii_case(source) {
        return Some(NameMatch::Same);
    }
    if stem(target) == stem(source) {
        return Some(NameMatch::Normalised);
    }
    // `id` against `customerId`, when `customer` is one of the two classes
    let (t, s) = (stem(target), stem(source));
    let roles = [core_of(target_type), core_of(source_type)];
    let by_class = |long: &str, short: &str| short == "id" && long.strip_suffix("id").is_some_and(|c| roles.iter().any(|r| stem(r) == c || r == c));
    (by_class(&t, &s) || by_class(&s, &t)).then_some(NameMatch::Id)
}

fn without_args(ty: &str) -> &str {
    ty.split('<').next().unwrap_or(ty)
}

fn boxed(ty: &str) -> &str {
    match ty {
        "int" => "java.lang.Integer",
        "long" => "java.lang.Long",
        "boolean" => "java.lang.Boolean",
        "double" => "java.lang.Double",
        "float" => "java.lang.Float",
        "short" => "java.lang.Short",
        "byte" => "java.lang.Byte",
        "char" => "java.lang.Character",
        other => other,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TypeFit {
    Exact,
    /// The same type boxed, or one side raw.
    Loose,
}

fn type_fit(source: &str, target: &str) -> Option<TypeFit> {
    if source == target {
        return Some(TypeFit::Exact);
    }
    if boxed(source) == boxed(target) {
        return Some(TypeFit::Loose);
    }
    let (s, t) = (without_args(source), without_args(target));
    (s == t && (!source.contains('<') || !target.contains('<'))).then_some(TypeFit::Loose)
}

/// The next property to map: the first of `target` (in the order given) that no line assigned and that exactly one
/// property of `source` answers to well enough.
#[derive(Debug, Clone, PartialEq)]
pub struct Pick {
    pub target: usize,
    pub source: usize,
    pub confidence: f32,
}

pub fn next_mapping(target: &[Prop], source: &[Prop], assigned: &[String], target_type: &str, source_type: &str) -> Option<Pick> {
    let done: HashSet<String> = assigned.iter().map(|a| stem(a)).collect();
    for (ti, t) in target.iter().enumerate() {
        if done.contains(&stem(&t.name)) {
            continue;
        }
        // (score, confidence, source index): a name that is the same beats one that is merely alike
        let mut found: Vec<(u8, f32, usize)> = Vec::new();
        for (si, s) in source.iter().enumerate() {
            let (Some(name), Some(ty)) = (name_match(&t.name, &s.name, target_type, source_type), type_fit(&s.ty, &t.ty)) else { continue };
            let confidence = if name == NameMatch::Same && ty == TypeFit::Exact { 0.9 } else { 0.75 };
            found.push((name as u8 * 2 + u8::from(ty == TypeFit::Exact), confidence, si));
        }
        let Some(best) = found.iter().map(|f| f.0).max() else { continue };
        let top: Vec<_> = found.iter().filter(|f| f.0 == best).collect();
        // two answers to one name: not a choice to make for the author
        if top.len() == 1 && top[0].1 >= MIN_CONFIDENCE {
            return Some(Pick { target: ti, source: top[0].2, confidence: top[0].1 });
        }
    }
    None
}

/// The statement for `pick`, in the style the lines above use.
pub fn render(style: Style, target_var: &str, source_var: &str, target: &Prop, source: &Prop) -> String {
    match style {
        Style::Setter => format!("{target_var}.{}({source_var}.{}());", target.method, source.method),
        Style::Builder => format!(".{}({source_var}.{}())", target.method, source.method),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prop(name: &str, method: &str, ty: &str) -> Prop {
        Prop { name: name.into(), method: method.into(), ty: ty.into() }
    }

    const STR: &str = "java.lang.String";

    #[test]
    fn setter_lines_give_the_style_the_variables_and_what_is_done() {
        let before = "    Dto toDto(Entity e) {\n        Dto d = new Dto();\n        d.setId(e.getId());\n        d.setName(e.getName());\n";
        let c = read_context(before).expect("a conversion");
        assert_eq!(c.style, Style::Setter);
        assert_eq!((c.target_var.as_deref(), c.source_var.as_deref(), c.target_type.as_deref()), (Some("d"), Some("e"), Some("Dto")));
        assert_eq!(c.assigned, ["Name", "Id"]);
    }

    #[test]
    fn right_after_the_construction_nothing_is_assigned_yet() {
        let c = read_context("    Dto toDto(Entity e) {\n        final Dto d = new Dto();\n").unwrap();
        assert_eq!((c.style, c.target_var.as_deref(), c.target_type.as_deref(), c.source_var), (Style::Setter, Some("d"), Some("Dto"), None));
        assert!(c.assigned.is_empty());
    }

    #[test]
    fn builder_chains_are_read_the_same_way() {
        let c = read_context("        return Dto.builder()\n            .id(e.getId())\n            .name(e.getName())\n").unwrap();
        assert_eq!((c.style, c.target_type.as_deref(), c.source_var.as_deref()), (Style::Builder, Some("Dto"), Some("e")));
        assert_eq!(c.assigned, ["name", "id"]);
        let c = read_context("        Dto d = Dto.builder()\n").unwrap();
        assert_eq!((c.style, c.target_var.as_deref(), c.target_type.as_deref()), (Style::Builder, Some("d"), Some("Dto")));
    }

    #[test]
    fn a_line_that_is_not_a_conversion_gives_nothing() {
        assert!(read_context("        int total = compute();\n").is_none());
        assert!(read_context("        d.setId(42);\n").is_none(), "a literal is not read from a source");
        assert!(read_context("").is_none());
        // mixed styles: not one conversion
        assert!(read_context("        d.setId(e.getId());\n        .name(e.getName())\n").is_none());
    }

    #[test]
    fn the_next_unassigned_property_is_matched_by_name_and_type() {
        let target = [prop("Id", "setId", "long"), prop("Name", "setName", STR), prop("Age", "setAge", "int")];
        let source = [prop("Id", "getId", "long"), prop("Name", "getName", STR), prop("Age", "getAge", "java.lang.Integer")];
        let pick = next_mapping(&target, &source, &["Id".into()], "Dto", "Entity").unwrap();
        assert_eq!((pick.target, pick.source, pick.confidence), (1, 1, 0.9));
        let pick = next_mapping(&target, &source, &["Id".into(), "Name".into()], "Dto", "Entity").unwrap();
        assert_eq!((pick.target, pick.source, pick.confidence), (2, 2, 0.75), "boxed is looser");
        assert!(next_mapping(&target, &source, &["Id".into(), "Name".into(), "Age".into()], "Dto", "Entity").is_none());
    }

    #[test]
    fn a_property_nothing_answers_is_skipped_not_forced() {
        let target = [prop("Extra", "setExtra", STR), prop("Name", "setName", STR)];
        let source = [prop("Name", "getName", STR)];
        assert_eq!(next_mapping(&target, &source, &[], "Dto", "Entity").unwrap().target, 1);
        // the type does not fit: no conversion is made up
        let source = [prop("Name", "getName", "long")];
        assert!(next_mapping(&target, &source, &[], "Dto", "Entity").is_none());
    }

    #[test]
    fn case_underscores_and_plurals_are_tolerated() {
        let target = [prop("Items", "setItems", "java.util.List<java.lang.String>"), prop("OrderDate", "setOrderDate", "java.util.Date")];
        let source = [prop("item", "getItem", "java.util.List<java.lang.String>"), prop("order_date", "getOrder_date", "java.util.Date")];
        let pick = next_mapping(&target, &source, &[], "Dto", "Entity").unwrap();
        assert_eq!((pick.target, pick.source, pick.confidence), (0, 0, 0.75));
        let pick = next_mapping(&target, &source, &["Items".into()], "Dto", "Entity").unwrap();
        assert_eq!((pick.target, pick.source), (1, 1));
    }

    #[test]
    fn id_answers_to_the_class_prefixed_id_and_only_to_that() {
        let target = [prop("CustomerId", "setCustomerId", "long")];
        let source = [prop("Id", "getId", "long"), prop("Version", "getVersion", "long")];
        assert!(next_mapping(&target, &source, &[], "OrderDto", "Order").is_none(), "an order's id is not a customer's");
        let pick = next_mapping(&target, &source, &[], "OrderDto", "Customer").unwrap();
        assert_eq!((pick.source, pick.confidence), (0, 0.75));
    }

    #[test]
    fn two_equally_good_answers_are_not_a_choice() {
        let target = [prop("Name", "setName", STR)];
        let source = [prop("name", "getName", STR), prop("NAME", "getNAME", STR)];
        assert!(next_mapping(&target, &source, &[], "Dto", "Entity").is_none());
    }

    #[test]
    fn the_statement_follows_the_style() {
        let (t, s) = (prop("Name", "setName", STR), prop("Name", "getName", STR));
        assert_eq!(render(Style::Setter, "d", "e", &t, &s), "d.setName(e.getName());");
        let (t, s) = (prop("name", "name", STR), prop("Name", "getName", STR));
        assert_eq!(render(Style::Builder, "", "e", &t, &s), ".name(e.getName())");
    }
}
