//! `style` as a template reads it: the facts of Settings › Java Style, and declarations written the way they
//! say.
//!
//! The facts alone leave every template to spell the same chain — Lombok's `val` where it is chosen and there,
//! `var` from Java 10, `final` when that is the habit, the type otherwise — and to get its order right each
//! time. [`StyleFacts::local`] is that chain written once, and a template calls it as `style.local(type)`.

use std::sync::Arc;

use minijinja::value::{from_args, Enumerator, Object, ObjectRepr, Value};
use minijinja::{Error, ErrorKind, State};

use crate::facts::StyleFacts;
use crate::imports::Imports;

/// The methods of `style`, with what each writes — what an editor offers after `style.`.
///
/// A test calls every one, so this list cannot name a method a template would then find missing.
pub const STYLE_METHODS: &[(&str, &str)] = &[
    (
        "local",
        "local(type) — what goes before a local's name: `val`, `var`, `final Order` or `Order`. For a local with an initializer",
    ),
    ("param", "param(type) — a parameter's type, `final` when parameters are"),
    ("braces", "braces(body) — a one-line body in its braces, with a space inside them or without"),
];

impl StyleFacts {
    /// What goes before a local's name, for a local with an initializer — which `val` and `var` both need.
    ///
    /// `val` first, being final already; then `var`; then the type — `final` in front of the last two when
    /// locals are final. Both facts are gated on the project where they are read, so neither is written where
    /// it would not compile.
    pub fn local(&self, type_name: &str) -> String {
        match (self.lombok_val, self.local_var) {
            (true, _) => "val".to_string(),
            (false, true) => format!("{}var", self.final_keyword()),
            (false, false) => format!("{}{}", self.final_keyword(), type_name.trim()),
        }
    }

    /// A parameter's type, `final` when parameters are.
    pub fn param(&self, type_name: &str) -> String {
        format!("{}{}", self.final_keyword(), type_name.trim())
    }

    /// A one-line body in its braces — `{ return x; }` or `{return x;}` — and `{}` for no body at all.
    pub fn braces(&self, body: &str) -> String {
        match (body.trim(), self.space_in_braces) {
            ("", _) => "{}".to_string(),
            (body, true) => format!("{{ {body} }}"),
            (body, false) => format!("{{{body}}}"),
        }
    }

    fn final_keyword(&self) -> &'static str {
        if self.final_params {
            "final "
        } else {
            ""
        }
    }
}

/// The facts as a template reads them: `style.lombok_val` and the rest, and the methods above — which record in
/// `imports` what they write needs: Lombok's `val` is a type like any other.
pub(crate) fn style_value(facts: StyleFacts, imports: Imports) -> Value {
    let fields = Value::from_serialize(&facts);
    Value::from_object(StyleObject { facts, fields, imports })
}

#[derive(Debug)]
struct StyleObject {
    facts: StyleFacts,
    /// The same facts as a map, so a field is read by its serialized name and a new one needs no line here.
    fields: Value,
    imports: Imports,
}

impl Object for StyleObject {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Map
    }

    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        self.fields.get_item(key).ok().filter(|value| !value.is_undefined())
    }

    fn enumerate(self: &Arc<Self>) -> Enumerator {
        match self.fields.try_iter() {
            Ok(keys) => Enumerator::Values(keys.collect()),
            Err(_) => Enumerator::Empty,
        }
    }

    fn call_method(self: &Arc<Self>, _state: &State<'_, '_>, method: &str, args: &[Value]) -> Result<Value, Error> {
        let write: fn(&StyleFacts, &str) -> String = match method {
            "local" => StyleFacts::local,
            "param" => StyleFacts::param,
            "braces" => StyleFacts::braces,
            _ => return Err(Error::from(ErrorKind::UnknownMethod)),
        };
        let (text,): (String,) = from_args(args)?;
        if method == "local" && self.facts.lombok_val {
            self.imports.require("lombok.val");
        }
        Ok(Value::from(write(&self.facts, &text)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::render_with;
    use crate::facts::TemplateFacts;
    use serde_json::json;

    fn style(final_params: bool, lombok_val: bool, local_var: bool) -> StyleFacts {
        StyleFacts { final_params, lombok_val, local_var, ..StyleFacts::default() }
    }

    #[test]
    fn a_local_is_val_then_var_then_its_type_with_final_where_it_can_go() {
        assert_eq!(style(false, false, false).local("Order"), "Order");
        assert_eq!(style(true, false, false).local("Order"), "final Order");
        assert_eq!(style(false, false, true).local("Order"), "var");
        assert_eq!(style(true, false, true).local("Order"), "final var");
        assert_eq!(style(true, true, true).local("Order"), "val", "val is final already, and wins");
        assert_eq!(style(true, false, false).param("int"), "final int");
    }

    #[test]
    fn a_one_line_body_follows_the_spacing() {
        let mut facts = StyleFacts::default();
        assert_eq!(facts.braces("return x;"), "{return x;}");
        facts.space_in_braces = true;
        assert_eq!(facts.braces(" return x; "), "{ return x; }");
        assert_eq!(facts.braces("  "), "{}");
    }

    #[test]
    fn every_method_an_editor_offers_is_one_a_template_can_call() {
        let facts = TemplateFacts::default();
        for (name, _) in STYLE_METHODS {
            let out = render_with(&format!("{{{{ style.{name}(\"Order\") }}}}"), &json!({}), &facts, None);
            assert!(out.is_ok(), "{name}: {out:?}");
        }
    }

    #[test]
    fn the_facts_read_as_before_and_a_parameter_changes_what_the_methods_write() {
        let mut facts = TemplateFacts::default();
        facts.style.final_params = true;
        let template = "{{ style.final_params }} {{ style.local(\"Order\") }}";
        assert_eq!(render_with(template, &json!({}), &facts, None).unwrap(), "true final Order");
        let over = json!({ "style": { "local_var": true } });
        assert_eq!(render_with(template, &json!({}), &facts, Some(&over)).unwrap(), "true final var");
    }
}
