//! `config_class` domain — a `@ConfigurationProperties` class written from a configuration file.
//!
//! Reading the file and typing its keys are `bennu-templates`' ([`bennu_templates::config_tree`],
//! [`bennu_templates::config_class`]). This is what only the backend knows: which files beside the open
//! one are its profiles, the project's Java level and whether it has Lombok — which decide the template a
//! project starts from — and where in the project a class like this belongs.

use std::path::{Path, PathBuf};

use bennu_core::prelude::BennuState;
use bennu_templates::prelude::{
    class_name_for, common_group, key_at, key_nodes, keys_in, narrow, read_config, relative_to, render_code, render_with,
    ConfigClassContext, ConfigValue, Directives, KeyNode, Template,
};
use serde::{Deserialize, Serialize};

use crate::templates_render::{source_of, RenderArgs, Rendered};

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// The open file, then its profile files — `application-dev.yml` beside `application.yml`, a
/// `.properties` of the same name too — so the values of the one open are the samples.
fn sources(file: &str, profiles: bool) -> Vec<PathBuf> {
    let path = Path::new(file);
    let mut out = vec![path.to_path_buf()];
    let (Some(dir), true) = (path.parent(), profiles) else { return out };
    let base = base_name(file_name(file));
    let Ok(entries) = std::fs::read_dir(dir) else { return out };
    let mut siblings: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|p| p.as_path() != path)
        .filter(|p| {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                bennu_spring::prelude::is_property_file(n) && base_name(n) == base
            })
        })
        .collect();
    siblings.sort();
    out.extend(siblings);
    out
}

/// `application-dev.yml` → `application`.
fn base_name(name: &str) -> &str {
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    stem.split_once('-').map_or(stem, |(base, _)| base)
}

/// Every source read into one tree, and the names of the files that were.
fn read_all(file: &str, source: &str, profiles: bool) -> (ConfigValue, Vec<String>) {
    let mut tree = ConfigValue::Map(Vec::new());
    let mut names = Vec::new();
    for (index, path) in sources(file, profiles).into_iter().enumerate() {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
        let text = match index {
            0 => source.to_string(),
            _ => match std::fs::read_to_string(&path) {
                Ok(text) => text,
                Err(_) => continue,
            },
        };
        tree.merge(read_config(&name, &text));
        names.push(name);
    }
    (tree, names)
}

/// Each prefix of `key` that is a group of keys, outermost first — `app`, `app.mail` for `app.mail.host`.
/// Without a key, the groups at the top of the file.
fn prefixes_of(key: Option<&str>, tree: &ConfigValue) -> Vec<String> {
    let is_group = |value: Option<&ConfigValue>| matches!(value, Some(ConfigValue::Map(entries)) if !entries.is_empty());
    match key {
        Some(key) => {
            let segments: Vec<&str> = key.split('.').collect();
            (1..=segments.len())
                .map(|n| segments[..n].join("."))
                .filter(|prefix| is_group(tree.at(prefix)))
                .collect()
        }
        None => match tree {
            ConfigValue::Map(entries) => {
                entries.iter().filter(|(_, v)| is_group(Some(v))).map(|(k, _)| k.clone()).collect()
            }
            _ => Vec::new(),
        },
    }
}

/// Beside the module's Spring Boot application class, in a `config` package — or the top of the module's
/// Java sources while that class has not been found by a scan yet.
fn suggested_directory(root: &str, file: &str) -> String {
    let file = file.replace('\\', "/");
    let java_root = match file.find("/src/main/resources/") {
        Some(at) => format!("{}/src/main/java", &file[..at]),
        None => format!("{}/src/main/java", root.trim_end_matches('/')),
    };
    match crate::main_classes::cached_boot_application(root, &java_root) {
        Some(app) => format!("{}/config", app.rsplit_once('/').map_or(app.as_str(), |(dir, _)| dir)),
        None => java_root,
    }
}

/// Whether the application already registers every `@ConfigurationProperties` class it finds.
fn scans_properties(root: &str, file: &str) -> bool {
    let file = file.replace('\\', "/");
    let under = file.find("/src/main/resources/").map_or(root.to_string(), |at| file[..at].to_string());
    crate::main_classes::cached_boot_application(root, &under)
        .and_then(|app| std::fs::read_to_string(app).ok())
        .is_some_and(|text| text.contains("ConfigurationPropertiesScan"))
}

#[derive(Deserialize)]
pub struct ConfigClassOriginArgs {
    pub root: String,
    pub file: String,
    /// The buffer, possibly unsaved.
    pub source: String,
    #[serde(default)]
    pub offset: Option<usize>,
    /// The end of a selection starting at `offset`: then the keys it names decide, not the caret's group.
    #[serde(default)]
    pub selection_end: Option<usize>,
}

#[derive(Serialize)]
pub struct ConfigClassOrigin {
    /// The key at the caret.
    pub key: Option<String>,
    /// The keys a class can be written for: every group of keys the caret is in, outermost first — or
    /// the groups at the top of the file.
    pub prefixes: Vec<String>,
    /// The innermost group the caret is in — or, for a selection, the group its keys share.
    pub prefix: Option<String>,
    /// The keys a selection named, relative to `prefix`; `None` without a selection.
    pub selected: Option<Vec<String>>,
    pub class_name: String,
    /// Where the class would go, and the package that folder is.
    pub directory: String,
    pub package: String,
    /// The profile files beside the open one.
    pub profiles: Vec<String>,
}

/// What a configuration class can be written for, at the caret of a configuration file.
#[arbor_rpc::handler]
fn bennu_config_class_origin(_ctx: &BennuState, args: ConfigClassOriginArgs) -> Result<ConfigClassOrigin, String> {
    let name = file_name(&args.file);
    let (tree, files) = read_all(&args.file, &args.source, true);
    let selection = match (args.offset, args.selection_end) {
        (Some(start), Some(end)) if end > start => Some(keys_in(name, &args.source, start, end)).filter(|k| !k.is_empty()),
        _ => None,
    };
    let (key, selected) = match selection {
        Some(keys) => {
            let group = common_group(&tree, &keys)
                .ok_or("The selection holds keys of more than one group, and a class binds one prefix — select within one")?;
            let selected = relative_to(&group, &keys);
            (Some(group), Some(selected))
        }
        None => (args.offset.and_then(|offset| key_at(name, &args.source, offset)), None),
    };
    let prefixes = prefixes_of(key.as_deref(), &tree);
    let prefix = key.as_ref().and_then(|_| prefixes.last().cloned());
    let directory = suggested_directory(&args.root, &args.file);
    Ok(ConfigClassOrigin {
        class_name: prefix.as_deref().map(class_name_for).unwrap_or_default(),
        package: bennu_java::prelude::infer_package(Path::new(&directory)).unwrap_or_default(),
        profiles: files.into_iter().skip(1).collect(),
        key,
        prefixes,
        prefix,
        selected,
        directory,
    })
}

#[derive(Deserialize)]
pub struct ConfigClassKeysArgs {
    pub file: String,
    /// The buffer, possibly unsaved.
    pub source: String,
    pub prefix: String,
    #[serde(default)]
    pub profiles: Option<bool>,
}

/// The keys under a prefix, parents before children — for choosing which of them the class gets.
#[arbor_rpc::handler]
fn bennu_config_class_keys(_ctx: &BennuState, args: ConfigClassKeysArgs) -> Result<Vec<KeyNode>, String> {
    let (tree, _) = read_all(&args.file, &args.source, args.profiles.unwrap_or(true));
    Ok(tree.at(&args.prefix).map(key_nodes).unwrap_or_default())
}

/// A `config-class` template rendered — what `bennu_render_template` answers for that kind.
pub(crate) fn render(args: &RenderArgs, template: &Template) -> Result<Rendered, String> {
    let (file, source) = source_of(args)?;
    let (tree, files) = read_all(&file, &source, args.profiles.unwrap_or(true));
    let prefix = match args.prefix.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        Some(prefix) => prefix.to_string(),
        None => args
            .offset
            .and_then(|offset| key_at(file_name(&file), &source, offset))
            .and_then(|key| prefixes_of(Some(&key), &tree).pop())
            .ok_or("There is no group of keys at the caret — give the prefix to write a class for")?,
    };
    let value = tree.at(&prefix).ok_or_else(|| format!("No configuration file here has keys under `{prefix}`"))?;
    let directory = args.directory.clone().unwrap_or_else(|| suggested_directory(&args.root, &file));
    let class_name = args
        .name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map_or_else(|| class_name_for(&prefix), str::to_string);
    let package = bennu_java::prelude::infer_package(Path::new(&directory)).unwrap_or_default();
    let facts = crate::templates_facts::facts_at(&args.root, &format!("{}/{class_name}.java", directory.trim_end_matches(['/', '\\'])));
    let chosen = match &args.keys {
        Some(keys) if keys.is_empty() => return Err("Choose at least one key for the class".to_string()),
        Some(keys) => narrow(value, keys),
        None => value.clone(),
    };
    let context = ConfigClassContext::new(&prefix, &class_name, &package, &chosen, files.clone(), &args.maps.clone().unwrap_or_default())?;

    let parameters = args.parameters.as_ref();
    let output = render_code(&template.text, &context, &facts, parameters)?;
    let target = match Directives::read(&template.text).get("file") {
        Some(pattern) => render_with(pattern, &context, &facts, parameters)?.trim().to_string(),
        None => format!("{class_name}.java"),
    };
    let path = format!("{}/{}", directory.trim_end_matches(['/', '\\']), target.trim_start_matches('/'));
    let text = crate::templates_imports::with_imports(&path, output.text, &output.imports);

    let mut notes = context.notes.clone();
    if files.len() > 1 {
        notes.push(format!("Keys read from {}.", files.join(", ")));
    }
    let java = facts.project.java;
    if (1..16).contains(&java) && text.contains("public record ") {
        notes.push(format!("A record needs Java 16, and this project is on Java {java} — choose a class template."));
    }
    if !scans_properties(&args.root, &file) {
        notes.push(
            "Spring binds it once it is registered: `@ConfigurationPropertiesScan` on the application, or `@EnableConfigurationProperties` naming the class."
                .to_string(),
        );
    }
    Ok(Rendered::file(&template.name, text, path, notes))
}
