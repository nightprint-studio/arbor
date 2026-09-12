//! Rendering a template, and what a template declares about itself.
//!
//! ## Directives
//!
//! A template tells Bennu a few things the output alone cannot: what it is for, where its result
//! goes, what file it creates. It says them in lines of their own, normally a Jinja comment:
//!
//! ```jinja
//! {# bennu.description: A Spring Data repository for the entity at the caret -#}
//! {# bennu.output: file -#}
//! {# bennu.file: {{ class.name }}Repository.java -#}
//! ```
//!
//! Read **before** rendering, because some of them decide what the template is rendered with. Only
//! a line that *starts* with `bennu.` counts — past the comment opener and whitespace — so prose that
//! mentions a directive in a template's own documentation is not taken for one.

use std::collections::BTreeMap;

use minijinja::Environment;
use serde::Serialize;
use serde_json::Value;

use crate::facts::{StyleFacts, TemplateFacts};
use crate::imports::Imports;
use crate::line_map::{self, mark_lines, unmark_lines, LineTrace};
use crate::names::{camel, java_string, pascal, snake};
use crate::naming::naming_value;
use crate::style::style_value;

/// The filters Bennu adds to Jinja's own, with what each does — what an editor offers after `|`.
///
/// [`render`] registers exactly these; a test renders every one, so this list cannot name a filter a
/// template would then find missing.
pub const CUSTOM_FILTERS: &[(&str, &str)] = &[
    ("snake", "`customerName` → `customer_name`"),
    ("camel", "`customer_name` → `customerName`"),
    ("pascal", "`customer_name` → `CustomerName`"),
    ("java_string", "The text as a Java string literal, quoted and escaped"),
    (
        "arguments",
        "`valid | arguments` → each `value_java`, comma-separated; `arguments(field.name, case.value_java)` changes that field's",
    ),
    ("imported", "`\"java.time.LocalDate\" | imported` → `LocalDate`, and the file gets the import"),
];

/// What a render wrote, and the imports it asked for on the way — see [`crate::imports`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Output {
    pub text: String,
    /// Fully qualified, sorted, each once: `java.time.LocalDate`, `lombok.val`.
    pub imports: Vec<String>,
}

/// Render `template` with `context`.
///
/// Blocks are trimmed the way hand-written templates expect — a line holding only `{% … %}` leaves
/// nothing behind — and the [`CUSTOM_FILTERS`] are added. Inside [`line_map::trace_lines`] it renders a second
/// time, to say which line of the template wrote each line of the text.
pub fn render(template: &str, context: &impl Serialize) -> Result<String, String> {
    render_in(template, context, &Imports::default())
}

fn render_in(template: &str, context: &impl Serialize, imports: &Imports) -> Result<String, String> {
    let env = environment(imports);
    let text = env.render_str(template, context).map_err(|error| match error.line() {
        Some(line) => format!("template line {line}: {error}"),
        None => format!("template: {error}"),
    })?;
    if line_map::tracing() {
        trace(&env, template, context, &text);
    }
    Ok(text)
}

fn environment(imports: &Imports) -> Environment<'static> {
    let mut env = Environment::new();
    env.set_trim_blocks(true);
    env.set_lstrip_blocks(true);
    env.set_keep_trailing_newline(true);
    env.add_filter("snake", |value: String| snake(&value));
    env.add_filter("camel", |value: String| camel(&value));
    env.add_filter("pascal", |value: String| pascal(&value));
    env.add_filter("java_string", |value: String| java_string(&value));
    env.add_filter("arguments", arguments);
    let imports = imports.clone();
    env.add_filter("imported", move |name: String| imports.imported(&name));
    env
}

/// `valid | arguments`: the `value_java` of each item, comma-separated — what goes between a constructor's
/// parentheses. `arguments(field.name, case.value_java)` writes that one field's argument as the value instead,
/// which is the whole of a test that breaks one constraint at a time.
fn arguments(
    items: minijinja::Value,
    field: Option<String>,
    value: Option<minijinja::Value>,
) -> Result<String, minijinja::Error> {
    let mut out = Vec::new();
    for item in items.try_iter()? {
        let written = item.get_attr("value_java").unwrap_or_default();
        if written.is_undefined() || written.is_none() {
            continue;
        }
        let named = item.get_attr("field").ok();
        let named = named.as_ref().and_then(minijinja::Value::as_str);
        out.push(match (&field, &value) {
            (Some(field), Some(value)) if named == Some(field.as_str()) => value.to_string(),
            _ => written.to_string(),
        });
    }
    Ok(out.join(", "))
}

/// The second render a trace asks for, kept only when — its marks removed — it wrote the same text.
fn trace(env: &Environment<'_>, template: &str, context: &impl Serialize, text: &str) {
    let Some(marked) = mark_lines(template) else { return };
    let Ok(output) = env.render_str(&marked, context) else { return };
    let (output, lines) = unmark_lines(&output);
    if output == text {
        line_map::record(LineTrace { template: template.to_string(), output, lines });
    }
}

/// Render `template` with `context`, the facts every kind reads — `project`, `style` and `naming` — beside its
/// own fields, and `parameters` laid over all of it.
///
/// What a preview renders with when the class at hand does not take the branch being written:
/// `{"mode": "members"}`, `{"class": {"name": "Order"}}`. Objects merge key by key at any depth, so a
/// renamed class keeps its fields; anything else — a list, a number — replaces what was there, and a
/// key the context does not have is added.
///
/// With no parameters the context is rendered as it is, not through JSON: a JSON object does not keep
/// its fields in order, and a template iterating one would change output for nobody's reason.
pub fn render_with(
    template: &str,
    context: &impl Serialize,
    facts: &TemplateFacts,
    parameters: Option<&Value>,
) -> Result<String, String> {
    render_code(template, context, facts, parameters).map(|output| output.text)
}

/// [`render_with`], and the imports the output asked for — what a kind whose output is Java renders with.
pub fn render_code(
    template: &str,
    context: &impl Serialize,
    facts: &TemplateFacts,
    parameters: Option<&Value>,
) -> Result<Output, String> {
    let imports = Imports::default();
    let text = match parameters {
        None | Some(Value::Null) => render_in(template, &with_facts(context, facts, &imports), &imports)?,
        Some(Value::Object(over)) if over.is_empty() => render_in(template, &with_facts(context, facts, &imports), &imports)?,
        Some(over @ Value::Object(_)) => {
            let mut data = serde_json::to_value(context).map_err(|e| format!("template context: {e}"))?;
            // Facts first, so a parameter can stand in for them: `{"style": {"lombok_val": true}}`.
            if let Value::Object(map) = &mut data {
                map.insert("project".to_string(), serde_json::to_value(&facts.project).unwrap_or_default());
                map.insert("style".to_string(), serde_json::to_value(&facts.style).unwrap_or_default());
                map.insert("naming".to_string(), serde_json::to_value(&facts.naming).unwrap_or_default());
            }
            lay_over(&mut data, over);
            // `style` back as the object whose methods write declarations — from the laid-over facts, so a
            // parameter saying `"local_var": true` changes what `style.local(…)` writes as well as the fact.
            let style = data
                .get("style")
                .and_then(|style| serde_json::from_value::<StyleFacts>(style.clone()).ok())
                .unwrap_or_else(|| facts.style.clone());
            let naming = data
                .get("naming")
                .and_then(|naming| serde_json::from_value::<BTreeMap<String, String>>(naming.clone()).ok())
                .map_or_else(|| facts.naming.clone(), |over| facts.naming.laid_over(&over));
            let mut fields = fields_of(&minijinja::Value::from_serialize(&data));
            fields.insert("style".to_string(), style_value(style, imports.clone()));
            fields.insert("naming".to_string(), naming_value(naming));
            render_in(template, &fields, &imports)?
        }
        Some(_) => return Err("Parameters are a JSON object: { \"name\": value }".to_string()),
    };
    Ok(Output { text, imports: imports.list() })
}

/// The context's own fields with `project`, `style` and `naming` beside them — as minijinja values, not through
/// JSON, which would put a struct's nested fields in alphabetical order. `style` and `naming` are the objects of
/// [`crate::style`] and [`crate::naming`].
fn with_facts(context: &impl Serialize, facts: &TemplateFacts, imports: &Imports) -> BTreeMap<String, minijinja::Value> {
    let mut out = fields_of(&minijinja::Value::from_serialize(context));
    out.insert("project".to_string(), minijinja::Value::from_serialize(&facts.project));
    out.insert("style".to_string(), style_value(facts.style.clone(), imports.clone()));
    out.insert("naming".to_string(), naming_value(facts.naming.clone()));
    out
}

/// A map value's fields, each kept as the value it is.
fn fields_of(value: &minijinja::Value) -> BTreeMap<String, minijinja::Value> {
    let mut out = BTreeMap::new();
    if let Ok(keys) = value.try_iter() {
        for key in keys {
            let field = value.get_item(&key).unwrap_or_default();
            out.insert(key.to_string(), field);
        }
    }
    out
}

fn lay_over(base: &mut Value, over: &Value) {
    match (base, over) {
        (Value::Object(base), Value::Object(over)) => {
            for (key, value) in over {
                match base.get_mut(key) {
                    Some(slot) => lay_over(slot, value),
                    None => {
                        base.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (base, over) => *base = over.clone(),
    }
}

/// The JSON Schema of a context type, with every subschema inlined so a reader walks it without
/// resolving a `$ref`.
pub fn schema_of<T: schemars::JsonSchema>() -> serde_json::Value {
    schemars::generate::SchemaSettings::draft2020_12()
        .with(|s| s.inline_subschemas = true)
        .into_generator()
        .into_root_schema_for::<T>()
        .to_value()
}

/// The `bennu.<key>: <value>` lines of a template, in order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Directives {
    entries: Vec<(String, String)>,
}

impl Directives {
    pub fn read(template: &str) -> Self {
        let mut entries = Vec::new();
        for line in template.lines() {
            let content = line.trim_start();
            let content = content
                .strip_prefix("{#-")
                .or_else(|| content.strip_prefix("{#"))
                .unwrap_or(content)
                .trim_start();
            let Some(rest) = content.strip_prefix("bennu.") else { continue };
            let key: String =
                rest.chars().take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-')).collect();
            if key.is_empty() {
                continue;
            }
            let Some(value) = rest[key.len()..].trim_start().strip_prefix(':') else { continue };
            let mut value = value.trim();
            for closer in ["-#}", "#}"] {
                if let Some(stripped) = value.strip_suffix(closer) {
                    value = stripped.trim_end();
                    break;
                }
            }
            entries.push((key, value.to_string()));
        }
        Self { entries }
    }

    /// The first value given for `key`.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str()).filter(|v| !v.is_empty())
    }

    /// Every value given for `key`, split on commas, each once, in order.
    pub fn list(&self, key: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for (_, value) in self.entries.iter().filter(|(k, _)| k == key) {
            for item in value.split(',').map(str::trim).filter(|item| !item.is_empty()) {
                if !out.iter().any(|seen| seen == item) {
                    out.push(item.to_string());
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_filter_an_editor_offers_is_one_a_template_can_use() {
        for (name, _) in CUSTOM_FILTERS {
            let out = render(&format!("{{{{ \"customerName\" | {name} }}}}"), &json!({}));
            assert!(out.is_ok(), "{name}: {out:?}");
        }
        assert_eq!(render("{{ n | snake }}", &json!({ "n": "customerName" })).unwrap(), "customer_name");
    }

    #[test]
    fn arguments_write_a_constructor_call_and_change_the_one_asked_for() {
        let context = json!({ "valid": [{ "field": "name", "value_java": "\"Ada\"" }, { "field": "age", "value_java": "36" }] });
        assert_eq!(render("{{ valid | arguments }}", &context).unwrap(), "\"Ada\", 36");
        assert_eq!(render("{{ valid | arguments(\"age\", \"-1\") }}", &context).unwrap(), "\"Ada\", -1");
    }

    #[test]
    fn a_mistake_in_a_template_says_which_line() {
        let error = render("line one\n{{ x | nosuchfilter }}\n", &json!({ "x": 1 })).unwrap_err();
        assert!(error.contains("line 2"), "{error}");
    }

    #[test]
    fn parameters_merge_into_nested_objects_and_keep_what_they_do_not_name() {
        let context = json!({ "class": { "name": "Customer", "fields": ["id", "email"] }, "mode": "file" });
        let template = "{{ class.name }} {{ class.fields | length }} {{ mode }} {{ extra }}";
        let parameters = json!({ "class": { "name": "Order" }, "mode": "members", "extra": 7 });
        assert_eq!(render_with(template, &context, &TemplateFacts::default(), Some(&parameters)).unwrap(), "Order 2 members 7");
    }

    #[test]
    fn a_list_parameter_replaces_the_list_rather_than_merging_into_it() {
        let context = json!({ "fields": ["id", "email"] });
        let parameters = json!({ "fields": ["name"] });
        assert_eq!(render_with("{{ fields | join(',') }}", &context, &TemplateFacts::default(), Some(&parameters)).unwrap(), "name");
    }

    #[test]
    fn no_parameters_render_as_before_and_a_non_object_is_refused() {
        let context = json!({ "n": 1 });
        assert_eq!(render_with("{{ n }}", &context, &TemplateFacts::default(), None).unwrap(), "1");
        assert_eq!(render_with("{{ n }}", &context, &TemplateFacts::default(), Some(&json!({}))).unwrap(), "1");
        assert!(render_with("{{ n }}", &context, &TemplateFacts::default(), Some(&json!([1]))).unwrap_err().contains("JSON object"));
    }

    #[test]
    fn every_template_reads_the_project_and_the_style_beside_its_own_fields() {
        let mut facts = TemplateFacts::default();
        facts.project.java = 17;
        facts.style.lombok_val = true;
        let context = json!({ "name": "Order" });
        let template = "{{ name }} {{ project.java }} {{ style.lombok_val }}";
        assert_eq!(render_with(template, &context, &facts, None).unwrap(), "Order 17 true");
        let over = json!({ "style": { "lombok_val": false } });
        assert_eq!(render_with(template, &context, &facts, Some(&over)).unwrap(), "Order 17 false", "a parameter stands in");
    }

    #[test]
    fn directives_are_read_from_their_own_lines_only() {
        let template = "{# bennu.description: A repository -#}\n\
                        {#- bennu.file: {{ class.name }}Repository.java -#}\n\
                        {#\n  bennu.constants: com.example.Messages, com.example.Codes\n#}\n\
                        {# A line mentioning `bennu.output: file` is prose, not a directive #}\n\
                        {# bennu.constants: com.example.Messages #}\n";
        let directives = Directives::read(template);
        assert_eq!(directives.get("description"), Some("A repository"));
        assert_eq!(directives.get("file"), Some("{{ class.name }}Repository.java"));
        assert_eq!(directives.get("output"), None);
        assert_eq!(directives.list("constants"), ["com.example.Messages", "com.example.Codes"]);
    }
}
