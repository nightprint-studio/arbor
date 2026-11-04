//! `format_style` domain — where the Java formatter's style comes from, and the handlers the settings
//! page talks to.
//!
//! The effective style is built from layers, later winning:
//!
//! 1. the defaults, then the **legacy** layer (the editor's indent settings and the two old formatter
//!    fields, so a profile that never opened the new page formats as it always did);
//! 2. the **global** style, `bennu/format.toml` in the active profile;
//! 3. the project's **detected** style, when `auto_detect` is on (measured on demand, kept in memory);
//! 4. the nearest `.editorconfig` files, when `use_editorconfig` is on;
//! 5. the project's **override**, `[format]` in `<repo>/.arbor/bennu/config.toml` — the sparse set of
//!    keys the user changed for this project only.
//!
//! `use_editorconfig` and `auto_detect` are themselves style keys, read from the layers below the one
//! they gate (global, then project).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use bennu_core::prelude::BennuState;
use bennu_intentions::prelude::{detect_style, editorconfig_style, Detection, FormatStyle};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

type Obj = Map<String, Value>;

const SECTION: &str = "format";
/// Directories never worth reading Java from.
const SKIP_DIRS: [&str; 7] = ["target", "build", "node_modules", ".git", "out", ".gradle", ".idea"];
/// A project this big is measured on a sample: the style does not change with the 601st file.
const MAX_DETECT_FILES: usize = 600;

fn global_path() -> PathBuf {
    arbor_core::prelude::bennu_config_path("format.toml")
}

fn load_global() -> Obj {
    std::fs::read_to_string(global_path())
        .ok()
        .and_then(|t| toml::from_str::<Obj>(&t).ok())
        .unwrap_or_default()
}

fn save_global(style: &Obj) -> Result<(), String> {
    let path = global_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = toml::to_string_pretty(style).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| e.to_string())
}

/// What the profile said before the formatter had a page of its own.
pub(crate) fn legacy_layer(tab_size: Option<u32>, insert_spaces: Option<bool>) -> Obj {
    let cfg = bennu_core::config::load();
    let mut m = Obj::new();
    m.insert("indent_width".into(), Value::from(tab_size.unwrap_or(cfg.indent_width).max(1)));
    m.insert("use_tabs".into(), Value::from(!insert_spaces.unwrap_or(!cfg.indent_with_tabs)));
    m.insert("max_blank_lines".into(), Value::from(cfg.java_max_blank_lines as u64));
    m.insert("indent_case_body".into(), Value::from(cfg.java_indent_case_body));
    m
}

fn project_layer(root: &str) -> Option<Obj> {
    let table = crate::repo_config::read_table(root);
    let section = table.get(SECTION)?;
    let obj = serde_json::to_value(section).ok()?.as_object().cloned()?;
    (!obj.is_empty()).then_some(obj)
}

/// The `.editorconfig` files from `file`'s folder up to `root` (stopping at one that says
/// `root = true`), farthest first, mapped to style keys.
fn editorconfig_layer(root: &str, file: Option<&str>) -> Option<Obj> {
    let root_path = Path::new(root);
    let file_path = file.map(PathBuf::from).unwrap_or_else(|| root_path.join("File.java"));
    let rel = file_path.strip_prefix(root_path).ok()?.to_string_lossy().replace('\\', "/");
    let mut dir = file_path.parent()?.to_path_buf();
    let mut texts: Vec<String> = Vec::new();
    loop {
        if let Ok(text) = std::fs::read_to_string(dir.join(".editorconfig")) {
            let is_root = text.lines().any(|l| l.trim().replace(' ', "").eq_ignore_ascii_case("root=true"));
            texts.push(text);
            if is_root {
                break;
            }
        }
        if dir == root_path || !dir.pop() {
            break;
        }
    }
    if texts.is_empty() {
        return None;
    }
    texts.reverse();
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
    let patch = editorconfig_style(&rel, &refs);
    (!patch.is_empty()).then_some(patch)
}

fn detected_cache() -> &'static Mutex<HashMap<String, Obj>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Obj>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cached_detection(root: &str) -> Option<Obj> {
    detected_cache().lock().unwrap_or_else(|p| p.into_inner()).get(root).cloned()
}

fn collect_java(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        if out.len() >= MAX_DETECT_FILES {
            return;
        }
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            if !SKIP_DIRS.contains(&name.as_str()) && !name.starts_with('.') {
                collect_java(&path, out);
            }
        } else if name.ends_with(".java") {
            out.push(path);
        }
    }
}

/// Measure the project's Java and remember the result for `auto_detect`.
fn detect_project(root: &str) -> Detection {
    let mut files = Vec::new();
    collect_java(Path::new(root), &mut files);
    let texts: Vec<String> =
        files.iter().filter_map(|p| std::fs::read(p).ok()).map(|b| String::from_utf8_lossy(&b).into_owned()).collect();
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
    let detection = detect_style(&refs);
    detected_cache().lock().unwrap_or_else(|p| p.into_inner()).insert(root.to_string(), detection.patch.clone());
    detection
}

/// The style in force for `file` (or for the project at `root`), with the editor's own indent
/// settings as the lowest layer.
pub(crate) fn resolve(root: Option<&str>, file: Option<&str>, legacy: &Obj) -> FormatStyle {
    let global = load_global();
    let project = root.and_then(project_layer);

    // The two switches that gate layers.
    let empty = Obj::new();
    let gate = FormatStyle::layered(&[legacy, &global, project.as_ref().unwrap_or(&empty)]);
    let mut layers: Vec<Obj> = vec![legacy.clone(), global];
    if let Some(root) = root {
        if gate.auto_detect {
            if let Some(d) = cached_detection(root) {
                layers.push(d);
            }
        }
        if gate.use_editorconfig {
            if let Some(e) = editorconfig_layer(root, file) {
                layers.push(e);
            }
        }
    }
    if let Some(p) = project {
        layers.push(p);
    }
    let refs: Vec<&Obj> = layers.iter().collect();
    FormatStyle::layered(&refs)
}

// ── Handlers ────────────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct SettingsArgs {
    #[serde(default)]
    pub root: Option<String>,
}

#[derive(Serialize)]
pub struct SettingsOut {
    defaults: Obj,
    global: Obj,
    project: Option<Obj>,
    effective: Obj,
    editorconfig: Option<Obj>,
    detected: Option<Obj>,
}

/// Every layer of the style, for the settings page.
#[arbor_rpc::handler]
fn bennu_format_settings(_ctx: &BennuState, args: SettingsArgs) -> Result<SettingsOut, String> {
    let legacy = legacy_layer(None, None);
    let root = args.root.as_deref().filter(|r| !r.is_empty());
    Ok(SettingsOut {
        defaults: FormatStyle::default().to_map(),
        global: FormatStyle::layered(&[&legacy, &load_global()]).to_map(),
        project: root.and_then(project_layer),
        effective: resolve(root, None, &legacy).to_map(),
        editorconfig: root.and_then(|r| editorconfig_layer(r, None)),
        detected: root.and_then(cached_detection),
    })
}

#[derive(Deserialize)]
pub struct SetGlobalArgs {
    pub style: Obj,
}

/// Replace the global style. Saved in full, so a value that happens to equal the default still
/// outranks the legacy layer beneath it.
#[arbor_rpc::handler]
fn bennu_format_set_global(_ctx: &BennuState, args: SetGlobalArgs) -> Result<(), String> {
    save_global(&FormatStyle::layered(&[&args.style]).to_map())
}

#[derive(Deserialize)]
pub struct SetProjectArgs {
    pub root: String,
    #[serde(default)]
    pub overrides: Option<Obj>,
}

/// Replace (or, with `null`, clear) the project's override. Only keys the formatter knows are kept.
#[arbor_rpc::handler]
fn bennu_format_set_project(_ctx: &BennuState, args: SetProjectArgs) -> Result<(), String> {
    let known = FormatStyle::default().to_map();
    let kept: Obj = args
        .overrides
        .unwrap_or_default()
        .into_iter()
        .filter(|(k, v)| known.get(k).is_some_and(|d| std::mem::discriminant(d) == std::mem::discriminant(v)))
        .collect();
    crate::repo_config::save(&args.root, SECTION, &kept)
}

#[derive(Deserialize)]
pub struct PreviewArgs {
    pub style: Obj,
    pub source: String,
}

/// `source` formatted with `style` — pure, for the live preview.
#[arbor_rpc::handler]
fn bennu_format_preview(_ctx: &BennuState, args: PreviewArgs) -> Result<String, String> {
    Ok(bennu_intentions::prelude::format_source(&args.source, &FormatStyle::layered(&[&args.style])))
}

#[derive(Deserialize)]
pub struct DetectArgs {
    pub root: String,
}

/// Measure the project's Java for the style it follows.
#[arbor_rpc::handler]
fn bennu_format_detect(_ctx: &BennuState, args: DetectArgs) -> Result<Detection, String> {
    Ok(detect_project(&args.root))
}
