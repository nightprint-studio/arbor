//! Tool metadata — what turns an RPC handler into something an AI client can call.
//!
//! A handler and a tool are not the same thing, and the difference is the whole reason
//! this module is opt-in. A handler needs a name and a signature; a tool additionally
//! needs a **description written for a reader who cannot see the code**, an **input
//! schema** it can fill in without guessing, and a **safety class** so the host can
//! decide whether to run it, ask first, or refuse.
//!
//! Exposure is therefore `#[handler(mcp(...))]` and never automatic. `bennu-be` alone
//! registers 160-odd handlers; auto-exposing them would publish `bennu_debug_resume` and
//! `set_bennu_config` to a remote model on the same footing as `bennu_read_file`, and
//! bury the useful dozen in a list no model can choose from.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// How much damage a tool can do — the input to the host's allow / ask / deny decision,
/// and the source of the MCP `readOnlyHint` / `destructiveHint` annotations.
///
/// The classification is the handler author's claim, checked by nobody: it is a routing
/// input for policy, not a sandbox. A handler that writes files and declares itself
/// `Read` will be allowed to write files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Safety {
    /// Observes state without changing it. Repeatable, and a failure costs nothing.
    Read,
    /// Changes state in a way the user can undo (write a file, rename a capture).
    Write,
    /// Changes state in a way that is hard or impossible to undo, or runs arbitrary
    /// code (delete, apply a project-wide rewrite, spawn a build or a test run).
    Destructive,
}

impl Safety {
    /// The MCP `readOnlyHint`.
    pub fn read_only(self) -> bool {
        matches!(self, Safety::Read)
    }

    /// The MCP `destructiveHint`.
    pub fn destructive(self) -> bool {
        matches!(self, Safety::Destructive)
    }
}

/// The compile-time half of a tool, emitted by `#[handler(mcp(...))]` into the
/// inventory [`Entry`](crate::Entry).
///
/// `schema` is a function rather than a value because a JSON Schema is a heap structure
/// and an inventory entry is a `static`. It is called once, when the host asks for the
/// tool list.
pub struct ToolMeta {
    /// The tool's name, when it must differ from the method's.
    ///
    /// Needed because handler names are only unique *within* a backend: tyto's
    /// `list_captures` and a future product's would collide in one tool surface, and a
    /// bare `session_state` tells a model nothing about whose session it is. Products
    /// whose handlers are already prefixed (`bennu_read_file`) leave this alone. The
    /// method name travels alongside in [`ToolDescriptor::method`], so an audit line
    /// still names a function.
    pub name: Option<&'static str>,
    /// Short human label. Shown by MCP clients next to the technical name.
    pub title: &'static str,
    /// What the tool does, when to reach for it, and what it does *not* return.
    /// Defaults to the handler's own `///` doc comment.
    pub description: &'static str,
    pub safety: Safety,
    /// Whether calling twice with the same arguments is the same as calling once.
    pub idempotent: bool,
    /// Whether the tool touches something outside this machine (the MCP
    /// `openWorldHint`). Almost everything Arbor exposes is local: `false`.
    pub open_world: bool,
    /// Set when the handler takes a single struct argument: the MCP-facing schema is
    /// that struct's own schema, flattened, and the host re-wraps the arguments under
    /// this parameter name before dispatch.
    ///
    /// The seam keys params by *parameter name*, so `fn f(ctx, args: Foo)` is called
    /// with `{"args": {…}}`. Exposing that shape to a model would mean handing it a
    /// schema whose single property is a meaningless wrapper — so the wrapper is the
    /// host's business, not the model's.
    pub wrap_in: Option<&'static str>,
    /// How to render the result — see [`ToolOutput`].
    pub output: ToolOutput,
    /// Builds the input schema. See [`schema_of`] / [`object_schema`].
    pub schema: fn() -> Value,
}

/// How the host should turn a handler's JSON result into MCP content blocks.
///
/// A handler returns whatever its Rust type serializes to; a model reads a list of
/// typed blocks. Most of the time the honest rendering is "the JSON, as text", but a
/// screenshot is not — handing a model a base64 string labelled `text` wastes the
/// context and the model's vision entirely.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolOutput {
    /// Serialize the result as compact JSON inside a single text block.
    #[default]
    Json,
    /// The result is a bare string: emit it as text, unquoted.
    Text,
    /// The result is `{ "mime_type": "image/png", "data": "<base64>" }` — emit an
    /// image block. A result that doesn't match that shape falls back to JSON, so a
    /// mislabelled handler degrades instead of disappearing.
    Image,
}

/// One argument of a flat-signature handler, for [`object_schema`].
pub struct ToolField {
    pub name: &'static str,
    pub schema: fn() -> Value,
    pub required: bool,
}

/// A tool as it crosses the process boundary: the serializable projection of
/// [`ToolMeta`] that a backend returns from `__tools` and the shell turns into an
/// MCP tool definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDescriptor {
    /// The name the AI client sees and calls.
    pub name: String,
    /// The wire method behind it — the handler's own name. Defaults to the same string;
    /// the host routes on this, so an audit line always maps back to a function.
    pub method: String,
    pub title: String,
    pub description: String,
    pub input_schema: Value,
    pub safety: Safety,
    pub idempotent: bool,
    pub open_world: bool,
    pub wrap_in: Option<String>,
    #[serde(default)]
    pub output: ToolOutput,
}

impl ToolDescriptor {
    /// Re-apply the wrapper this tool's handler expects. The model sends flat
    /// arguments; the handler may want them under `args`.
    pub fn wrap_arguments(&self, arguments: Value) -> Value {
        match &self.wrap_in {
            Some(key) => json!({ key.as_str(): arguments }),
            None => arguments,
        }
    }
}

/// The JSON Schema for `T`, shaped for a tool's `inputSchema`.
///
/// Subschemas are inlined rather than referenced: `$defs` + `$ref` is legal JSON Schema
/// and a coin flip on whether a given MCP client resolves it, and an unresolved `$ref`
/// is a tool the model cannot call.
///
/// Three root keys are dropped. `$schema` is a meta-schema URI nobody reads. `title` is
/// the Rust type's name. `description` is the args struct's own doc comment — written
/// for a Rust reader, frequently rustdoc-flavoured (`Args for [`bennu_read_file`].`),
/// and always redundant next to the tool's real description. Per-*field* descriptions
/// are untouched: those are the ones a model actually needs.
pub fn schema_of<T: schemars::JsonSchema>() -> Value {
    let settings = schemars::generate::SchemaSettings::draft2020_12()
        .with(|s| s.inline_subschemas = true);
    let mut value = settings.into_generator().into_root_schema_for::<T>().to_value();
    if let Some(obj) = value.as_object_mut() {
        obj.remove("$schema");
        obj.remove("title");
        obj.remove("description");
    }
    value
}

/// Compose an object schema out of a flat handler's arguments.
///
/// Used for `fn f(ctx, id: String, limit: Option<u32>)`, where there is no struct to
/// derive from. Optional arguments contribute their **inner** type and stay out of
/// `required`, so a model sees `{"type":"string"}` rather than `{"type":["string","null"]}`
/// — the union is technically truer and empirically worse to prompt against.
pub fn object_schema(fields: &[ToolField]) -> Value {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();
    for field in fields {
        properties.insert(field.name.to_string(), (field.schema)());
        if field.required {
            required.push(Value::String(field.name.to_string()));
        }
    }
    json!({
        "type": "object",
        "properties": Value::Object(properties),
        "required": Value::Array(required),
        // Refuse invented arguments outright: a model that hallucinates a `dry_run`
        // flag should get a validation error, not silent non-application of it.
        "additionalProperties": false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(schemars::JsonSchema)]
    #[allow(dead_code)]
    struct Nested {
        /// How deep to go.
        depth: u32,
    }

    /// Args for the thing. Written for a Rust reader.
    #[derive(schemars::JsonSchema)]
    #[allow(dead_code)]
    struct Args {
        /// Absolute path to the project root.
        root: String,
        nested: Nested,
        limit: Option<u32>,
    }

    #[test]
    fn schema_of_inlines_and_strips_the_meta_keys() {
        let schema = schema_of::<Args>();
        let obj = schema.as_object().unwrap();
        assert!(!obj.contains_key("$schema"));
        assert!(!obj.contains_key("title"));
        // The struct's own doc is dropped; the tool's description is authoritative.
        assert!(!obj.contains_key("description"));
        assert!(!obj.contains_key("$defs"), "subschemas must be inlined: {schema:#}");
        // The nested struct is spelled out in place, not referenced.
        let nested = &obj["properties"]["nested"];
        assert_eq!(nested["properties"]["depth"]["type"], "integer");
        // Field docs survive as descriptions — the reason schemars is here at all.
        assert_eq!(obj["properties"]["root"]["description"], "Absolute path to the project root.");
    }

    #[test]
    fn object_schema_marks_only_required_fields() {
        let schema = object_schema(&[
            ToolField { name: "id", schema: schema_of::<String>, required: true },
            ToolField { name: "limit", schema: schema_of::<u32>, required: false },
        ]);
        assert_eq!(schema["properties"]["id"]["type"], "string");
        assert_eq!(schema["properties"]["limit"]["type"], "integer");
        assert_eq!(schema["required"], json!(["id"]));
        assert_eq!(schema["additionalProperties"], json!(false));
    }

    #[test]
    fn wrap_arguments_restores_the_handler_convention() {
        let d = ToolDescriptor {
            name: "x".into(),
            method: "x".into(),
            title: "X".into(),
            description: String::new(),
            input_schema: Value::Null,
            safety: Safety::Read,
            idempotent: true,
            open_world: false,
            wrap_in: Some("args".into()),
            output: ToolOutput::Json,
        };
        assert_eq!(d.wrap_arguments(json!({ "root": "/p" })), json!({ "args": { "root": "/p" } }));

        let flat = ToolDescriptor { wrap_in: None, ..d };
        assert_eq!(flat.wrap_arguments(json!({ "id": 1 })), json!({ "id": 1 }));
    }
}
