//! A Tauri command written at the caret of a Rust file, with the registration line and a typed TypeScript
//! wrapper around `invoke` (arguments in camelCase, the way Tauri hands them over).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::context::GenContext;
use crate::names::{camel, generic_args, is_identifier, snake};
use crate::output::{parse_options, MemberTarget, Members, Output};
use crate::registry::{Generator, GeneratorMeta, Target};

pub struct TauriCommand;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CommandParam {
    pub name: String,
    pub ty: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TauriOptions {
    pub name: String,
    pub params: Vec<CommandParam>,
    /// The success type; `()` for none.
    pub returns: String,
    /// `Result<T, String>`.
    pub fallible: bool,
    pub async_fn: bool,
    /// Managed state type taken as `tauri::State<'_, T>`; empty for none.
    pub state: String,
    pub app_handle: bool,
    pub window: bool,
    pub public: bool,
}

impl Default for TauriOptions {
    fn default() -> Self {
        TauriOptions {
            name: String::new(),
            params: Vec::new(),
            returns: "()".into(),
            fallible: true,
            async_fn: false,
            state: String::new(),
            app_handle: false,
            window: false,
            public: true,
        }
    }
}

impl Generator for TauriCommand {
    fn meta(&self) -> GeneratorMeta {
        GeneratorMeta {
            id: "tauri-command",
            title: "Generate Tauri command and TypeScript wrapper",
            description: "A #[tauri::command] function at the caret, its generate_handler! entry and a typed invoke() wrapper.",
            icon: "Plug",
            group: "Rust",
            language: "rust",
            targets: &[Target::Rust],
            related: &["tauri"],
        }
    }

    fn defaults(&self, _ctx: &GenContext) -> Value {
        serde_json::to_value(TauriOptions { name: "my_command".into(), params: vec![CommandParam { name: "name".into(), ty: "String".into() }], returns: "String".into(), ..TauriOptions::default() })
            .unwrap_or(Value::Null)
    }

    fn generate(&self, options: &Value, _ctx: &GenContext) -> Result<Output, String> {
        let o: TauriOptions = parse_options(options)?;
        let raw = o.name.trim();
        if raw.is_empty() || !raw.chars().all(|c| c.is_alphanumeric() || c == '_') || !is_identifier(&snake(raw)) {
            return Err(format!("`{}` is not a valid function name", o.name));
        }
        let name = snake(raw);
        for p in &o.params {
            if !is_identifier(&p.name) || p.ty.trim().is_empty() {
                return Err("Every parameter needs a name and a type".into());
            }
        }
        let text = rust_command(&o, &name);
        let mut out = Output::default();
        out.members = Some(Members { target: MemberTarget::Caret, text, imports: Vec::new() });
        out = out
            .extra("Registration", "rust", format!(".invoke_handler(tauri::generate_handler![{name}])"))
            .extra("TypeScript", "typescript", typescript_wrapper(&o, &name));
        if o.async_fn && !o.fallible && o.params.iter().any(|p| p.ty.trim_start().starts_with('&')) {
            out = out.note("An async command that takes a borrowed argument (`&str`) must return `Result` — turn on `Result`, or take an owned `String`.");
        }
        if o.fallible {
            out = out.note("Errors cross the boundary as `String`; a custom error type must implement `serde::Serialize`.");
        }
        Ok(out)
    }
}

fn rust_command(o: &TauriOptions, name: &str) -> String {
    let mut params: Vec<String> = Vec::new();
    if o.app_handle {
        params.push("app: tauri::AppHandle".to_string());
    }
    if o.window {
        params.push("window: tauri::Window".to_string());
    }
    if !o.state.trim().is_empty() {
        params.push(format!("state: tauri::State<'_, {}>", o.state.trim()));
    }
    for p in &o.params {
        params.push(format!("{}: {}", p.name, p.ty.trim()));
    }
    let returns = o.returns.trim();
    let ret = match (o.fallible, returns.is_empty() || returns == "()") {
        (true, true) => " -> Result<(), String>".to_string(),
        (true, false) => format!(" -> Result<{returns}, String>"),
        (false, true) => String::new(),
        (false, false) => format!(" -> {returns}"),
    };
    let body = match (o.fallible, returns.is_empty() || returns == "()") {
        (true, true) => "    Ok(())",
        (true, false) => "    todo!()",
        (false, true) => "    // TODO",
        (false, false) => "    todo!()",
    };
    format!(
        "#[tauri::command]\n{}{}fn {name}({}){ret} {{\n{body}\n}}",
        if o.public { "pub " } else { "" },
        if o.async_fn { "async " } else { "" },
        params.join(", "),
    )
}

/// The TypeScript spelling of a Rust type used on the command boundary.
pub fn ts_type(rust: &str) -> String {
    let t = rust.trim().trim_start_matches('&').trim_start_matches("mut ").trim();
    let base = t.split('<').next().unwrap_or(t).trim();
    let simple = base.rsplit("::").next().unwrap_or(base);
    let args = generic_args(t);
    match simple {
        "String" | "str" | "PathBuf" | "Path" | "char" => "string".into(),
        "bool" => "boolean".into(),
        "i8" | "i16" | "i32" | "i64" | "isize" | "u8" | "u16" | "u32" | "u64" | "usize" | "f32" | "f64" => "number".into(),
        "()" | "" => "void".into(),
        "Vec" | "VecDeque" | "HashSet" | "BTreeSet" => format!("{}[]", wrap_union(&ts_type(args.first().map(String::as_str).unwrap_or("unknown")))),
        "Option" => format!("{} | null", ts_type(args.first().map(String::as_str).unwrap_or("unknown"))),
        "HashMap" | "BTreeMap" => format!("Record<string, {}>", ts_type(args.get(1).map(String::as_str).unwrap_or("unknown"))),
        "Result" => ts_type(args.first().map(String::as_str).unwrap_or("void")),
        "Value" => "unknown".into(),
        _ if t.starts_with('(') => "unknown".into(),
        other => other.to_string(),
    }
}

fn wrap_union(ty: &str) -> String {
    if ty.contains(' ') { format!("({ty})") } else { ty.to_string() }
}

fn typescript_wrapper(o: &TauriOptions, name: &str) -> String {
    let function = camel(name);
    let params: Vec<(String, String)> = o.params.iter().map(|p| (camel(&p.name), ts_type(&p.ty))).collect();
    let signature: Vec<String> = params.iter().map(|(n, t)| format!("{n}: {t}")).collect();
    let returns = if o.returns.trim().is_empty() { "void".to_string() } else { ts_type(&o.returns) };
    let args = if params.is_empty() { String::new() } else { format!(", {{ {} }}", params.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(", ")) };
    format!(
        "import {{ invoke }} from '@tauri-apps/api/core';\n\nexport function {function}({}): Promise<{returns}> {{\n  return invoke<{returns}>('{name}'{args});\n}}\n",
        signature.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::sample_context;

    fn gen(options: serde_json::Value) -> Output {
        TauriCommand.generate(&options, &sample_context()).expect("generates")
    }

    #[test]
    fn a_fallible_async_command_with_state() {
        let out = gen(serde_json::json!({
            "name": "load_user", "async_fn": true, "state": "AppState", "returns": "User",
            "params": [{ "name": "user_id", "ty": "u64" }]
        }));
        let text = &out.members.as_ref().unwrap().text;
        assert_eq!(
            text,
            "#[tauri::command]\npub async fn load_user(state: tauri::State<'_, AppState>, user_id: u64) -> Result<User, String> {\n    todo!()\n}"
        );
        assert_eq!(out.members.as_ref().unwrap().target, MemberTarget::Caret);
        assert_eq!(out.extras[0].text, ".invoke_handler(tauri::generate_handler![load_user])");
    }

    #[test]
    fn the_wrapper_uses_camel_case_arguments_and_mapped_types() {
        let out = gen(serde_json::json!({
            "name": "load_user", "returns": "Vec<User>",
            "params": [{ "name": "user_id", "ty": "u64" }, { "name": "tags", "ty": "Option<Vec<String>>" }]
        }));
        let ts = &out.extras[1].text;
        assert!(ts.contains("export function loadUser(userId: number, tags: string[] | null): Promise<User[]> {"), "{ts}");
        assert!(ts.contains("return invoke<User[]>('load_user', { userId, tags });"), "{ts}");
    }

    #[test]
    fn a_command_without_a_result_or_return_is_a_plain_function() {
        let out = gen(serde_json::json!({ "name": "ping", "fallible": false, "returns": "()", "public": false }));
        assert_eq!(out.members.as_ref().unwrap().text, "#[tauri::command]\nfn ping() {\n    // TODO\n}");
        assert!(out.extras[1].text.contains("return invoke<void>('ping');"));
    }

    #[test]
    fn rust_types_map_to_typescript() {
        assert_eq!(ts_type("&str"), "string");
        assert_eq!(ts_type("HashMap<String, Vec<i32>>"), "Record<string, number[]>");
        assert_eq!(ts_type("Result<bool, String>"), "boolean");
        assert_eq!(ts_type("std::path::PathBuf"), "string");
        assert_eq!(ts_type("Option<Vec<Option<String>>>"), "(string | null)[] | null");
        assert_eq!(ts_type("serde_json::Value"), "unknown");
    }

    #[test]
    fn async_with_a_borrowed_argument_and_no_result_is_flagged() {
        let out = gen(serde_json::json!({ "name": "greet", "async_fn": true, "fallible": false, "params": [{ "name": "who", "ty": "&str" }] }));
        assert!(out.notes.iter().any(|n| n.contains("borrowed")));
    }

    #[test]
    fn bad_names_are_refused() {
        assert!(TauriCommand.generate(&serde_json::json!({ "name": "not valid!" }), &sample_context()).is_err());
        assert!(TauriCommand.generate(&serde_json::json!({ "name": "ok", "params": [{ "name": "", "ty": "u8" }] }), &sample_context()).is_err());
    }
}
