//! What a `debug_launch` call asks for, worked out from its arguments and the project's run
//! configurations. Pure: no process is started here, so the rules are tested without a JVM.

use std::collections::HashMap;

use bennu_proto::prelude::{RunConfig, RunConfigSet};
use serde_json::Value;

/// A launch, resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Plan {
    pub main_class: String,
    pub module: Option<String>,
    pub args: Vec<String>,
    pub vm_args: Vec<String>,
    pub working_dir: Option<String>,
    pub env: HashMap<String, String>,
    pub classpath_scope: Option<String>,
    /// Hold the VM before `main` until breakpoints are installed. On by default for the assistant: it sets
    /// its breakpoints first and a short program would otherwise be over before the first one binds.
    pub suspend: bool,
    /// How the launch is named in the console and the thread.
    pub label: String,
}

/// Split a command-line string into words: spaces separate, quotes group, a quote inside the other
/// kind is literal. Not a shell — no expansion, no escapes beyond the quotes.
pub(super) fn split_words(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut started = false;
    for c in line.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => current.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                started = true;
            }
            (None, c) if c.is_whitespace() => {
                if started || !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            (None, c) => current.push(c),
        }
    }
    if started || !current.is_empty() {
        words.push(current);
    }
    words
}

fn strings(input: &Value, key: &str) -> Option<Vec<String>> {
    input.get(key).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
}

fn text<'a>(input: &'a Value, key: &str) -> Option<&'a str> {
    input.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty())
}

fn find_config<'a>(set: &'a RunConfigSet, wanted: &str) -> Result<&'a RunConfig, String> {
    set.configs.iter().find(|c| c.id == wanted || c.name.eq_ignore_ascii_case(wanted)).ok_or_else(|| {
        let names: Vec<&str> = set.configs.iter().filter(|c| matches!(c.kind.as_str(), "" | "application" | "springboot")).map(|c| c.name.as_str()).collect();
        if names.is_empty() {
            format!("there is no run configuration `{wanted}`, and the project has no application ones: pass `main_class`")
        } else {
            format!("there is no run configuration `{wanted}`. The project's: {}", names.join(", "))
        }
    })
}

fn valid_class_name(name: &str) -> bool {
    !name.is_empty() && name.split('.').all(|part| !part.is_empty() && part.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$'))
}

/// Resolve a `debug_launch` call. Explicit arguments win over the configuration they name.
pub(super) fn plan(input: &Value, configs: &RunConfigSet) -> Result<Plan, String> {
    let config = match text(input, "config") {
        Some(name) => Some(find_config(configs, name)?),
        None => None,
    };
    if let Some(c) = config {
        if !matches!(c.kind.as_str(), "" | "application" | "springboot") {
            return Err(format!("run configuration `{}` is a `{}` one: only applications can be debugged here", c.name, c.kind));
        }
    }
    let main_class = text(input, "main_class")
        .map(str::to_string)
        .or_else(|| config.map(|c| c.main_class.trim().to_string()).filter(|m| !m.is_empty()))
        .ok_or("pass `main_class` (fully qualified), or a `config` that names one")?;
    if !valid_class_name(&main_class) {
        return Err(format!("`{main_class}` is not a class name: use the fully qualified one, e.g. com.acme.App"));
    }

    let mut vm_args = strings(input, "vm_args").or_else(|| config.map(|c| split_words(&c.vm_args))).unwrap_or_default();
    if let Some(bad) = vm_args.iter().find(|a| a.contains("jdwp")) {
        return Err(format!("`{bad}`: the debugger's agent is added for you, do not pass one"));
    }
    if let Some(c) = config.filter(|c| c.kind == "springboot" && !c.profiles.trim().is_empty()) {
        if !vm_args.iter().any(|a| a.starts_with("-Dspring.profiles.active")) {
            vm_args.push(format!("-Dspring.profiles.active={}", c.profiles.trim()));
        }
    }
    let args = strings(input, "args").or_else(|| config.map(|c| split_words(&c.program_args))).unwrap_or_default();
    let module = text(input, "module").map(str::to_string).or_else(|| config.map(|c| c.module.trim().to_string()).filter(|m| !m.is_empty()));
    let working_dir = config.map(|c| c.working_dir.trim().to_string()).filter(|d| !d.is_empty());
    let env = config.map(|c| c.env.iter().map(|e| (e.key.clone(), e.value.clone())).collect()).unwrap_or_default();
    let classpath_scope = config.map(|c| c.classpath_scope.trim().to_string()).filter(|s| !s.is_empty());
    let suspend = input.get("suspend").and_then(Value::as_bool).unwrap_or(true);
    let simple = main_class.rsplit('.').next().unwrap_or(&main_class).to_string();
    let label = format!("AI · {}", config.map(|c| c.name.clone()).filter(|n| !n.is_empty()).unwrap_or(simple));
    Ok(Plan { main_class, module, args, vm_args, working_dir, env, classpath_scope, suspend, label })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_proto::prelude::EnvVar;
    use serde_json::json;

    fn config(name: &str, kind: &str, main: &str) -> RunConfig {
        RunConfig { id: format!("id-{name}"), name: name.into(), kind: kind.into(), main_class: main.into(), ..RunConfig::default() }
    }

    fn set(configs: Vec<RunConfig>) -> RunConfigSet {
        RunConfigSet { configs, active_id: None }
    }

    #[test]
    fn words_are_split_like_a_command_line() {
        assert_eq!(split_words("-Xmx512m -Dname=\"a b\" 'x y' plain"), vec!["-Xmx512m", "-Dname=a b", "x y", "plain"]);
        assert_eq!(split_words("  spaced   out  "), vec!["spaced", "out"]);
        assert_eq!(split_words("\"\""), vec![""], "an empty quoted argument is an argument");
        assert!(split_words("   ").is_empty());
    }

    #[test]
    fn a_main_class_alone_is_enough_and_the_vm_is_held_at_start() {
        let p = plan(&json!({ "main_class": "com.acme.App" }), &set(vec![])).unwrap();
        assert_eq!(p.main_class, "com.acme.App");
        assert!(p.suspend, "held at start by default so the breakpoints bind in time");
        assert_eq!(p.label, "AI · App");
        assert!(!plan(&json!({ "main_class": "com.acme.App", "suspend": false }), &set(vec![])).unwrap().suspend);
    }

    #[test]
    fn a_configuration_supplies_what_the_call_leaves_out() {
        let mut c = config("Server", "springboot", "com.acme.Server");
        c.vm_args = "-Xmx256m".into();
        c.program_args = "--port=9000 \"two words\"".into();
        c.profiles = "dev,local".into();
        c.module = "services/api".into();
        c.env = vec![EnvVar { key: "MODE".into(), value: "test".into() }];
        let p = plan(&json!({ "config": "server" }), &set(vec![c])).unwrap();
        assert_eq!(p.main_class, "com.acme.Server");
        assert_eq!(p.args, vec!["--port=9000", "two words"]);
        assert_eq!(p.vm_args, vec!["-Xmx256m", "-Dspring.profiles.active=dev,local"]);
        assert_eq!(p.module.as_deref(), Some("services/api"));
        assert_eq!(p.env.get("MODE").map(String::as_str), Some("test"));
        assert_eq!(p.label, "AI · Server");
    }

    #[test]
    fn what_the_call_says_wins_over_the_configuration() {
        let mut c = config("Server", "application", "com.acme.Server");
        c.program_args = "--old".into();
        let p = plan(&json!({ "config": "Server", "main_class": "com.acme.Other", "args": ["--new"] }), &set(vec![c])).unwrap();
        assert_eq!((p.main_class.as_str(), p.args), ("com.acme.Other", vec!["--new".to_string()]));
    }

    #[test]
    fn a_launch_that_cannot_be_debugged_is_refused_with_the_way_out() {
        assert!(plan(&json!({}), &set(vec![])).unwrap_err().contains("main_class"));
        assert!(plan(&json!({ "main_class": "not a class" }), &set(vec![])).unwrap_err().contains("not a class name"));
        let err = plan(&json!({ "config": "nope" }), &set(vec![config("Server", "application", "a.B")])).unwrap_err();
        assert!(err.contains("no run configuration") && err.contains("Server"), "{err}");
        assert!(plan(&json!({ "config": "Tests" }), &set(vec![config("Tests", "junit", "")])).unwrap_err().contains("only applications"));
        let agent = plan(&json!({ "main_class": "a.B", "vm_args": ["-agentlib:jdwp=transport=dt_socket"] }), &set(vec![])).unwrap_err();
        assert!(agent.contains("added for you"), "{agent}");
    }
}
