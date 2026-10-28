//! `add parameter` domain — `bennu_add_parameter_plan` (the preview the dialog renders) and
//! `bennu_add_parameter_for_call` (the quick fix on a call that passes one argument too many).
//!
//! Both plan through the owning project's semantic engine, exactly as rename does, and answer in
//! rename's own wire shapes — a [`RenamePreview`] inside [`AddParameterPreview`], flat
//! [`RenameEdit`]s for the quick fix — so the editor previews and applies them with the machinery a
//! rename already has. The pure half of the work (what a parameter should be called, which names in a
//! written type might need an import) lives here and is tested here; what needs the resolver and the
//! files on disk is [`IndexService::plan_add_parameter`] and
//! [`IndexService::add_parameter_for_call`].

use bennu_core::prelude::BennuState;
use bennu_intel::prelude::{default_value_for, Anchor, NewParameter};
use bennu_proto::prelude::{AddParameterPreview, RenameEdit};
use serde::Deserialize;

use crate::index_service::IndexService;
use crate::rename::{preview_of, wire_edit};

/// Args for [`bennu_add_parameter_plan`].
#[derive(Deserialize)]
pub struct AddParameterArgs {
    /// Absolute path (forward slashes) of the file the caret is in.
    pub file: String,
    /// The current (possibly-unsaved) buffer text.
    pub source: String,
    /// Byte offset of the caret — on the method's declaration, or on a call of it.
    pub offset: usize,
    /// The new parameter's type, as written.
    pub type_text: String,
    /// The new parameter's name.
    pub name: String,
    /// What the existing calls pass for it. Absent: the type's default (`0`, `false`, `null`).
    #[serde(default)]
    pub default_value: Option<String>,
    /// The index of the parameter it goes before. Absent: last.
    #[serde(default)]
    pub before: Option<usize>,
}

/// Args for [`bennu_add_parameter_for_call`].
#[derive(Deserialize)]
pub struct AddParameterForCallArgs {
    /// The file the CALL is in — the buffer being edited.
    pub file: String,
    /// Its live text.
    pub source: String,
    /// The diagnostic's span: from the called method's NAME.
    pub start: usize,
    pub end: usize,
}

/// Plan adding a parameter at the caret. `None` while the index is still building or when the caret
/// is on no method this project declares; a preview with `blocked` set when it is and the change is
/// not safe.
#[arbor_rpc::handler]
fn bennu_add_parameter_plan(
    _ctx: &BennuState,
    args: AddParameterArgs,
) -> Result<Option<AddParameterPreview>, String> {
    let default_value = args
        .default_value
        .clone()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| default_value_for(&args.type_text));
    let param = NewParameter {
        type_text: args.type_text,
        name: args.name,
        default_value,
        before: args.before,
    };
    let plan = IndexService::global().plan_add_parameter(
        &args.file,
        &args.source,
        args.offset,
        &param,
        Anchor::Caret,
        &[],
    );
    // The dialog asks while the user is still typing — a half-written type is not an error, it is
    // a preview that says so.
    Ok(plan.map(|p| AddParameterPreview { parameters: p.parameters, preview: preview_of(p.plan) }))
}

/// The edits that add the parameter a call is already passing — the *Add parameter* quick fix.
///
/// An error rather than an empty list when the plan is refused: the refusal names the reason (a
/// method reference somewhere, a library override), and a quick fix that silently does nothing teaches
/// you to stop pressing the key.
#[arbor_rpc::handler]
fn bennu_add_parameter_for_call(
    _ctx: &BennuState,
    args: AddParameterForCallArgs,
) -> Result<Vec<RenameEdit>, String> {
    let plan = IndexService::global().add_parameter_for_call(&args.file, &args.source, args.start)?;
    Ok(plan.plan.files.into_iter().flat_map(|f| f.edits).map(wire_edit).collect())
}

/// Whether `offset` is on the name of a method declaration — where *Add parameter…* is offered.
pub(crate) fn on_method_declaration_name(source: &str, offset: usize) -> bool {
    let Some(tree) = bennu_java::prelude::parse_java(source) else { return false };
    let at = offset.min(source.len());
    let Some(node) = tree.root_node().descendant_for_byte_range(at, at) else { return false };
    node.parent().is_some_and(|p| {
        p.kind() == "method_declaration" && p.child_by_field_name("name").is_some_and(|n| n.id() == node.id())
    })
}

/// The last argument of the call whose name is at `at` — the one a *wrong number of arguments* error
/// says is one too many.
pub(crate) fn last_argument_of_call(root: tree_sitter::Node<'_>, at: usize) -> Option<tree_sitter::Node<'_>> {
    let mut cur = root.descendant_for_byte_range(at, at);
    while let Some(n) = cur {
        if n.kind() == "method_invocation" {
            let args = n.child_by_field_name("arguments")?;
            let mut cursor = args.walk();
            let last = args
                .named_children(&mut cursor)
                .filter(|a| !matches!(a.kind(), "line_comment" | "block_comment"))
                .last();
            return last;
        }
        cur = n.parent();
    }
    None
}

/// The names written in `type_text` that could need an import — the capitalised, unqualified
/// identifiers: `Map<String, List<Order>>` → `Map`, `String`, `List`, `Order`. A qualified name
/// (`java.util.List`) is already unambiguous, and a lowercase one is a primitive or a package.
pub(crate) fn written_type_names(type_text: &str) -> Vec<String> {
    fn flush(word: &mut String, qualified: bool, out: &mut Vec<String>) {
        if !qualified && word.chars().next().is_some_and(|c| c.is_uppercase()) && !out.contains(word) {
            out.push(word.clone());
        }
        word.clear();
    }
    let mut out: Vec<String> = Vec::new();
    let mut word = String::new();
    // Everything after a dot belongs to a name that is already qualified. A qualified name's FIRST
    // segment is a package unless it is capitalised (`Map.Entry`) — the outer type, which does need
    // its import, and is not after a dot.
    let mut after_dot = false;
    for ch in type_text.chars() {
        if ch.is_alphanumeric() || ch == '_' || ch == '$' {
            word.push(ch);
            continue;
        }
        flush(&mut word, after_dot, &mut out);
        after_dot = ch == '.';
    }
    flush(&mut word, after_dot, &mut out);
    out
}

const KEYWORDS: &[&str] = &[
    "abstract", "assert", "boolean", "break", "byte", "case", "catch", "char", "class", "const", "continue",
    "default", "do", "double", "else", "enum", "extends", "final", "finally", "float", "for", "goto", "if",
    "implements", "import", "instanceof", "int", "interface", "long", "native", "new", "package", "private",
    "protected", "public", "return", "short", "static", "strictfp", "super", "switch", "synchronized", "this",
    "throw", "throws", "transient", "try", "void", "volatile", "while", "true", "false", "null",
];

/// What a parameter holding `argument` (a call's argument, as written) of type `type_text` should be
/// called: the variable itself, the property a getter reads, or the type's own name.
pub(crate) fn parameter_name_for(argument: &str, type_text: &str) -> String {
    let arg = argument.trim();
    let name = plain_identifier(arg)
        .or_else(|| accessor_property(arg))
        .unwrap_or_else(|| name_from_type(type_text));
    if KEYWORDS.contains(&name.as_str()) {
        format!("{name}Value")
    } else {
        name
    }
}

fn plain_identifier(arg: &str) -> Option<String> {
    let mut chars = arg.chars();
    let first = chars.next()?;
    let ok = (first.is_alphabetic() || first == '_' || first == '$')
        && chars.all(|c| c.is_alphanumeric() || c == '_' || c == '$');
    (ok && !KEYWORDS.contains(&arg)).then(|| arg.to_string())
}

/// `order.getCustomerName()` → `customerName`; `order.total()` → `total`.
fn accessor_property(arg: &str) -> Option<String> {
    let call = arg.strip_suffix("()")?;
    let (receiver, last) = match call.rsplit_once('.') {
        Some((receiver, last)) => (Some(receiver), last),
        None => (None, call),
    };
    plain_identifier(last)?;
    if let Some(rest) = last.strip_prefix("get") {
        if rest.chars().next().is_some_and(|c| c.is_uppercase()) {
            return Some(lower_first(rest));
        }
    }
    // A record-style accessor on a variable (`order.total()`); not a static factory (`List.of()`),
    // whose receiver is a type and whose name says nothing about the value.
    let on_a_variable = receiver.is_some_and(|r| r.chars().next().is_some_and(|c| c.is_lowercase()));
    on_a_variable.then(|| last.to_string())
}

fn name_from_type(type_text: &str) -> String {
    let base = type_text.split('<').next().unwrap_or(type_text).trim();
    let array = base.ends_with("[]");
    let simple = base.trim_end_matches("[]").rsplit('.').next().unwrap_or(base);
    let name = match simple {
        "int" | "long" | "short" | "byte" | "Integer" | "Long" | "Short" | "Byte" => "value".to_string(),
        "double" | "float" | "Double" | "Float" => "amount".to_string(),
        "boolean" | "Boolean" => "flag".to_string(),
        "char" | "Character" => "ch".to_string(),
        "String" => "text".to_string(),
        other => lower_first(other),
    };
    if array { format!("{name}s") } else { name }
}

fn lower_first(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(c) => c.to_lowercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_variable_keeps_its_name() {
        assert_eq!(parameter_name_for("customer", "Customer"), "customer");
        assert_eq!(parameter_name_for("  total ", "int"), "total");
    }

    #[test]
    fn a_getter_names_the_property_it_reads() {
        assert_eq!(parameter_name_for("order.getCustomerName()", "String"), "customerName");
        assert_eq!(parameter_name_for("getId()", "long"), "id");
        assert_eq!(parameter_name_for("order.total()", "BigDecimal"), "total");
    }

    #[test]
    fn anything_else_is_named_after_its_type() {
        assert_eq!(parameter_name_for("5", "int"), "value");
        assert_eq!(parameter_name_for("\"x\"", "String"), "text");
        assert_eq!(parameter_name_for("true", "boolean"), "flag");
        assert_eq!(parameter_name_for("new Order()", "Order"), "order");
        assert_eq!(parameter_name_for("List.of()", "List<String>"), "list");
        assert_eq!(parameter_name_for("a + b", "int[]"), "values");
    }

    #[test]
    fn a_keyword_is_never_a_name() {
        assert_eq!(parameter_name_for("x.getClass()", "Class<?>"), "classValue");
        assert_eq!(parameter_name_for("null", "Object"), "object");
    }

    #[test]
    fn the_written_names_that_could_need_an_import() {
        assert_eq!(written_type_names("Map<String, List<Order>>"), ["Map", "String", "List", "Order"]);
        assert_eq!(written_type_names("int"), Vec::<String>::new());
        assert_eq!(written_type_names("java.util.List<Order>"), ["Order"]);
        assert_eq!(written_type_names("Map.Entry<K, V>"), ["Map", "K", "V"]);
        assert_eq!(written_type_names("Order[]"), ["Order"]);
    }
}
