// =============================================================================
// Plugin import / export — bundle a plugin folder into a zip and back.
//
//   * `export_plugin_template(opts)` — generate a starter plugin from a form
//     payload. Returns the zip bytes; the frontend writes them to the path the
//     user picked via the Tauri save dialog. The bundle always contains:
//
//       <name>/
//         plugin.toml      complete manifest derived from `opts`
//         main.lua         skeleton + the toggled snippet recipes
//         sdk.d.lua        EmmyLua type stubs (LuaLS autocomplete)
//         .luarc.json      LuaLS config — points workspace.library at sdk.d.lua
//
//     The Lua recipe text is NOT inlined in this file: every snippet lives
//     under `src-tauri/templates/plugin/` and is pulled in via `include_str!`,
//     so authors can edit or grep the templates the same way they would any
//     other plugin source.
//
//   * `import_plugin_zip(zip_bytes)` — extract a zip into the user's
//     `plugins/` directory. The archive must be rooted at a single folder
//     containing a `plugin.toml`. Existing folders with the same name are
//     refused (the user must explicitly remove or rename first) so we never
//     silently overwrite a customised plugin.
// =============================================================================

use std::collections::HashSet;
use std::io::{Cursor, Write};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::State;
use zip::write::SimpleFileOptions;
use zip::CompressionMethod;

use crate::error::AppError;
use crate::AppState;

// ── Static template files bundled into the binary ──────────────────────────
//
// `include_str!` paths are relative to *this* source file. Resolved targets:
//   src-tauri/src/commands/plugin_template_commands.rs   ← here
//   src-tauri/templates/plugin/...                       ← templates
//   src-tauri/../plugins/sdk.d.lua                       ← workspace SDK
const SDK_FILE_BYTES: &[u8] = include_bytes!("../../../plugins/sdk.d.lua");
const LUARC_JSON:        &str = include_str!("../../templates/plugin/luarc.json");
const MAIN_HEADER:       &str = include_str!("../../templates/plugin/main_header.lua");
const MAIN_FOOTER:       &str = include_str!("../../templates/plugin/main_footer.lua");
const HOOK_ON_LOAD:      &str = include_str!("../../templates/plugin/hook_on_plugin_load.lua");
const FALLBACK_PRINT:    &str = include_str!("../../templates/plugin/fallback_print.lua");
const RECIPE_COMMAND:        &str = include_str!("../../templates/plugin/recipes/command.lua");
const RECIPE_KEYBINDING:     &str = include_str!("../../templates/plugin/recipes/keybinding.lua");
const RECIPE_SETTINGS_PANEL: &str = include_str!("../../templates/plugin/recipes/settings_panel.lua");
const RECIPE_MODAL:          &str = include_str!("../../templates/plugin/recipes/modal.lua");
const RECIPE_ACTION_TOOLBAR: &str = include_str!("../../templates/plugin/recipes/action_toolbar.lua");
const RECIPE_SIDEBAR:        &str = include_str!("../../templates/plugin/recipes/sidebar.lua");
const RECIPE_NOTIFICATION:   &str = include_str!("../../templates/plugin/recipes/notification.lua");
const RECIPE_JOB_SPAWN:      &str = include_str!("../../templates/plugin/recipes/job_spawn.lua");
const RECIPE_SCHEDULER:      &str = include_str!("../../templates/plugin/recipes/scheduler.lua");
const RECIPE_HTTP_GET:       &str = include_str!("../../templates/plugin/recipes/http_get.lua");

/// Replace the `__SLUG__` placeholder used in every template file. We use a
/// distinctive marker (rather than `{name}` or `$name`) so plugin authors who
/// later look at the rendered template never have to wonder what's a literal
/// and what was substitution.
fn render(template: &str, slug: &str) -> String {
    template.replace("__SLUG__", slug)
}

// ── Export payload -----------------------------------------------------------

/// Form payload sent by `PluginExportTemplateModal`. Every field has a
/// safe default so the frontend can omit anything the user hasn't filled in.
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct ExportPluginTemplateOpts {
    // Identity
    pub name:        String,
    pub version:     String,
    pub description: String,
    pub author:      String,
    pub license:     Option<String>,
    pub repository:  Option<String>,
    pub keywords:    Vec<String>,

    // Permissions
    pub fs:                String,           // "none" | "read" | "write"
    pub fs_scope:          Vec<String>,      // empty = repo-sandboxed
    pub git:               String,           // "none" | "read" | "write" | "history_rewrite"
    pub terminal:          String,           // "none" | "commands" | "any"
    pub terminal_scope:    Vec<String>,
    pub network:           Vec<String>,
    pub env_read:          bool,
    pub issues:            String,           // "none" | "read" | "write"
    pub toolchain:         String,           // "none" | "read" | "write"
    pub service_export:    bool,
    pub service_call:      bool,
    pub settings_read_others: bool,

    // Hooks (the manifest-declared lifecycle subset users care about most)
    pub hook_on_plugin_load:  bool,
    pub hook_on_repo_open:    bool,
    pub hook_on_repo_close:   bool,
    pub hook_on_tab_switch:   bool,
    pub hook_on_commit:       bool,
    pub hook_on_push:         bool,
    pub hook_on_pull:         bool,
    pub hook_on_fetch:        bool,
    pub hook_on_checkout:     bool,
    pub hook_on_branch_create: bool,
    pub hook_on_branch_delete: bool,
    pub hook_on_mr_opened:    bool,
    pub hook_on_mr_merged:    bool,

    // Optional scheduler skeleton
    pub include_scheduler: bool,

    // Lua "recipe" snippets to inject into main.lua
    pub snippet_command:        bool,
    pub snippet_keybinding:     bool,
    pub snippet_settings_panel: bool,
    pub snippet_modal:          bool,
    pub snippet_action_toolbar: bool,
    pub snippet_sidebar:        bool,
    pub snippet_notification:   bool,
    pub snippet_job_spawn:      bool,
    pub snippet_scheduler:      bool,
    pub snippet_http_get:       bool,
}

// ── Helpers -----------------------------------------------------------------

/// True when the slug only contains characters safe for a folder + plugin name.
fn slug_ok(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Normalise a user-typed plugin name into a filesystem-safe slug. Falls back
/// to "my-plugin" when the input is empty after sanitisation.
fn sanitize_slug(raw: &str) -> String {
    let cleaned: String = raw
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c }
            else if c.is_whitespace() { '-' }
            else { '_' }
        })
        .collect();
    if cleaned.is_empty() { "my-plugin".to_string() } else { cleaned }
}

fn esc_toml_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

// ── plugin.toml generation --------------------------------------------------

fn build_manifest(opts: &ExportPluginTemplateOpts, slug: &str) -> String {
    let mut out = String::new();
    let name        = if opts.name.is_empty()        { slug.to_string() }                     else { opts.name.clone() };
    let version     = if opts.version.is_empty()     { "0.1.0".to_string() }                  else { opts.version.clone() };
    let description = if opts.description.is_empty() { "A new Arbor plugin.".to_string() }    else { opts.description.clone() };
    let author      = if opts.author.is_empty()      { "Anonymous".to_string() }              else { opts.author.clone() };

    out.push_str(&format!("name        = \"{}\"\n", esc_toml_string(&name)));
    out.push_str(&format!("version     = \"{}\"\n", esc_toml_string(&version)));
    out.push_str(&format!("description = \"{}\"\n", esc_toml_string(&description)));
    out.push_str(&format!("author      = \"{}\"\n", esc_toml_string(&author)));
    if let Some(l) = opts.license.as_ref().filter(|s| !s.is_empty()) {
        out.push_str(&format!("license     = \"{}\"\n", esc_toml_string(l)));
    }
    if let Some(r) = opts.repository.as_ref().filter(|s| !s.is_empty()) {
        out.push_str(&format!("repository  = \"{}\"\n", esc_toml_string(r)));
    }
    if !opts.keywords.is_empty() {
        let kws = opts.keywords.iter()
            .map(|k| format!("\"{}\"", esc_toml_string(k)))
            .collect::<Vec<_>>().join(", ");
        out.push_str(&format!("keywords    = [{kws}]\n"));
    }
    out.push_str("arbor_api   = 1\n");
    out.push('\n');

    // ── [permissions] ────────────────────────────────────────────────────────
    out.push_str("[permissions]\n");
    let level_or = |s: &str, fallback: &str| -> String {
        if s.is_empty() { fallback.to_string() } else { s.to_string() }
    };
    let fs       = level_or(&opts.fs,        "none");
    let git      = level_or(&opts.git,       "none");
    let terminal = level_or(&opts.terminal,  "none");
    let issues   = level_or(&opts.issues,    "none");
    let toolch   = level_or(&opts.toolchain, "none");

    if fs != "none"   { out.push_str(&format!("fs        = \"{fs}\"\n")); }
    if !opts.fs_scope.is_empty() {
        let arr = opts.fs_scope.iter()
            .map(|s| format!("\"{}\"", esc_toml_string(s)))
            .collect::<Vec<_>>().join(", ");
        out.push_str(&format!("fs_scope  = [{arr}]\n"));
    }
    if git != "none"      { out.push_str(&format!("git       = \"{git}\"\n")); }
    if terminal != "none" {
        out.push_str(&format!("terminal  = \"{terminal}\"\n"));
        if terminal == "commands" && !opts.terminal_scope.is_empty() {
            let arr = opts.terminal_scope.iter()
                .map(|s| format!("\"{}\"", esc_toml_string(s)))
                .collect::<Vec<_>>().join(", ");
            out.push_str(&format!("terminal_scope = [{arr}]\n"));
        }
    }
    if !opts.network.is_empty() {
        let arr = opts.network.iter()
            .map(|s| format!("\"{}\"", esc_toml_string(s)))
            .collect::<Vec<_>>().join(", ");
        out.push_str(&format!("network   = [{arr}]\n"));
    }
    if opts.env_read              { out.push_str("env_read  = true\n"); }
    if issues != "none"           { out.push_str(&format!("issues    = \"{issues}\"\n")); }
    if toolch != "none"           { out.push_str(&format!("toolchain = \"{toolch}\"\n")); }
    if opts.service_export        { out.push_str("service_export        = true\n"); }
    if opts.service_call          { out.push_str("service_call          = true\n"); }
    if opts.settings_read_others  { out.push_str("settings_read_others  = true\n"); }

    // ── [hooks] ───────────────────────────────────────────────────────────
    let any_hook = opts.hook_on_plugin_load || opts.hook_on_repo_open || opts.hook_on_repo_close
                || opts.hook_on_tab_switch  || opts.hook_on_commit     || opts.hook_on_push
                || opts.hook_on_pull        || opts.hook_on_fetch      || opts.hook_on_checkout
                || opts.hook_on_branch_create || opts.hook_on_branch_delete
                || opts.hook_on_mr_opened || opts.hook_on_mr_merged;
    if any_hook {
        out.push('\n');
        out.push_str("[hooks]\n");
        let mut h = |flag: bool, key: &str| {
            if flag { out.push_str(&format!("{key:18} = true\n")); }
        };
        h(opts.hook_on_plugin_load,   "on_plugin_load");
        h(opts.hook_on_repo_open,     "on_repo_open");
        h(opts.hook_on_repo_close,    "on_repo_close");
        h(opts.hook_on_tab_switch,    "on_tab_switch");
        h(opts.hook_on_commit,        "on_commit");
        h(opts.hook_on_push,          "on_push");
        h(opts.hook_on_pull,          "on_pull");
        h(opts.hook_on_fetch,         "on_fetch");
        h(opts.hook_on_checkout,      "on_checkout");
        h(opts.hook_on_branch_create, "on_branch_create");
        h(opts.hook_on_branch_delete, "on_branch_delete");
        h(opts.hook_on_mr_opened,     "on_mr_opened");
        h(opts.hook_on_mr_merged,     "on_mr_merged");
    }

    if opts.include_scheduler {
        out.push('\n');
        out.push_str("[scheduler]\n");
        out.push_str("enabled = true\n");
    }

    out
}

// ── main.lua skeleton + recipe snippets -------------------------------------

fn build_main_lua(opts: &ExportPluginTemplateOpts, slug: &str) -> String {
    let mut out = String::new();
    out.push_str(&render(MAIN_HEADER, slug));

    // Lifecycle hook(s)
    if opts.hook_on_plugin_load {
        out.push_str(&render(HOOK_ON_LOAD, slug));
    } else {
        out.push_str(&render(FALLBACK_PRINT, slug));
    }

    // Other hook stubs — generated from a plain table so adding a hook is
    // one edit, not three.
    let hook_stubs: &[(bool, &str, &str)] = &[
        (opts.hook_on_repo_open,     "on_repo_open",     "  -- Repository tab activated. ctx: { tab_id, path, name }"),
        (opts.hook_on_repo_close,    "on_repo_close",    "  -- Repository tab closed."),
        (opts.hook_on_tab_switch,    "on_tab_switch",    "  -- User switched tabs."),
        (opts.hook_on_commit,        "on_commit",        "  -- A commit was created on this repo."),
        (opts.hook_on_push,          "on_push",          "  -- A push completed."),
        (opts.hook_on_pull,          "on_pull",          "  -- A pull completed."),
        (opts.hook_on_fetch,         "on_fetch",         "  -- A fetch completed."),
        (opts.hook_on_checkout,      "on_checkout",      "  -- A branch / commit checkout happened."),
        (opts.hook_on_branch_create, "on_branch_create", "  -- A branch was created."),
        (opts.hook_on_branch_delete, "on_branch_delete", "  -- A branch was deleted."),
        (opts.hook_on_mr_opened,     "on_mr_opened",     "  -- A merge / pull request was opened."),
        (opts.hook_on_mr_merged,     "on_mr_merged",     "  -- A merge / pull request was merged."),
    ];
    for (flag, name, body) in hook_stubs.iter() {
        if !*flag { continue; }
        out.push_str(&format!("arbor.events.on(\"{name}\", function(_ctx)\n"));
        out.push_str(body);
        out.push_str("\nend)\n\n");
    }

    // Recipes — only emit a section header if at least one is enabled.
    let recipes: &[(bool, &str)] = &[
        (opts.snippet_command,        RECIPE_COMMAND),
        (opts.snippet_keybinding,     RECIPE_KEYBINDING),
        (opts.snippet_settings_panel, RECIPE_SETTINGS_PANEL),
        (opts.snippet_modal,          RECIPE_MODAL),
        (opts.snippet_action_toolbar, RECIPE_ACTION_TOOLBAR),
        (opts.snippet_sidebar,        RECIPE_SIDEBAR),
        (opts.snippet_notification,   RECIPE_NOTIFICATION),
        (opts.snippet_job_spawn,      RECIPE_JOB_SPAWN),
        (opts.snippet_scheduler,      RECIPE_SCHEDULER),
        (opts.snippet_http_get,       RECIPE_HTTP_GET),
    ];
    if recipes.iter().any(|(b, _)| *b) {
        out.push_str("-- ═══════════════════════════════════════════════════════════════════════\n");
        out.push_str("-- Recipes — toggle these on/off when re-exporting from the Plugin Manager.\n");
        out.push_str("-- ═══════════════════════════════════════════════════════════════════════\n");
        for (on, body) in recipes.iter() {
            if !*on { continue; }
            out.push_str(&render(body, slug));
        }
    }

    out.push_str(MAIN_FOOTER);
    out
}

// ── Public commands ---------------------------------------------------------

/// Build the in-memory zip bundle. Shared by the two export entry points so
/// the layout and contents stay in lock-step regardless of which one fires.
fn build_template_zip(opts: &ExportPluginTemplateOpts, slug: &str) -> Result<Vec<u8>, AppError> {
    let manifest = build_manifest(opts, slug);
    let main_lua = build_main_lua(opts, slug);

    let mut buf = Cursor::new(Vec::<u8>::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let opt_dir  = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        let opt_file = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

        // Top-level folder so unzipping creates `<slug>/` next to siblings.
        zip.add_directory(format!("{slug}/"), opt_dir)
            .map_err(|e| AppError::Other(format!("zip dir: {e}")))?;

        let mut write_text = |path: String, body: &str| -> Result<(), AppError> {
            zip.start_file(path, opt_file)
                .map_err(|e| AppError::Other(format!("zip start: {e}")))?;
            zip.write_all(body.as_bytes())?;
            Ok(())
        };
        write_text(format!("{slug}/plugin.toml"),  &manifest)?;
        write_text(format!("{slug}/main.lua"),     &main_lua)?;
        write_text(format!("{slug}/.luarc.json"),  LUARC_JSON)?;

        zip.start_file(format!("{slug}/sdk.d.lua"), opt_file)
            .map_err(|e| AppError::Other(format!("zip start: {e}")))?;
        zip.write_all(SDK_FILE_BYTES)?;

        zip.finish().map_err(|e| AppError::Other(format!("zip finish: {e}")))?;
    }
    Ok(buf.into_inner())
}

/// Generate the plugin template and write it to `target_path`. The frontend
/// passes the path it got back from Arbor's `FilePickerModal` (mode='save').
/// Returns the absolute path written to so the UI can show it in the toast.
#[tauri::command]
pub fn export_plugin_template_to_path(
    _state: State<'_, AppState>,
    opts: ExportPluginTemplateOpts,
    target_path: String,
) -> Result<String, AppError> {
    let raw_slug = if opts.name.is_empty() { "my-plugin".to_string() } else { opts.name.clone() };
    let slug     = sanitize_slug(&raw_slug);

    let bytes = build_template_zip(&opts, &slug)?;

    // Resolve the final write target: the picker can return either a file
    // path (user typed `foo.zip`) or a directory (user just clicked OK on a
    // folder). When it's a directory, drop the bundle inside as `<slug>.zip`.
    let mut path = PathBuf::from(&target_path);
    if path.is_dir() {
        path.push(format!("{slug}.zip"));
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, &bytes)?;
    Ok(path.to_string_lossy().to_string())
}

/// Result returned to the UI after a successful import.
#[derive(Debug, Serialize)]
pub struct ImportPluginResult {
    pub plugin_name: String,
    pub plugin_dir:  String,
    pub files:       usize,
}

/// Extract a plugin zip into the user's plugins directory.
///
/// The archive must be rooted at exactly one folder containing a
/// `plugin.toml`. Subdirectories underneath that folder are extracted as-is.
/// The folder name (= installed plugin name on disk) is taken from the zip
/// itself so the user sees the same identifier the author shipped.
#[tauri::command]
pub fn import_plugin_zip(
    _state: State<'_, AppState>,
    zip_bytes: Vec<u8>,
) -> Result<ImportPluginResult, AppError> {
    if zip_bytes.is_empty() {
        return Err(AppError::Other("empty plugin archive".into()));
    }

    let reader = Cursor::new(zip_bytes);
    let mut archive = zip::ZipArchive::new(reader)
        .map_err(|e| AppError::Other(format!("invalid zip: {e}")))?;

    // ── Pass 1: discover the single root folder + verify plugin.toml exists.
    let mut root_candidates: HashSet<String> = HashSet::new();
    let mut has_manifest = false;
    let mut manifest_root: Option<String> = None;

    for i in 0..archive.len() {
        let f = archive.by_index(i)
            .map_err(|e| AppError::Other(format!("zip read: {e}")))?;
        let name = f.name().replace('\\', "/");
        if name.contains("..") || name.starts_with('/') {
            return Err(AppError::Other(format!("unsafe path in archive: {name}")));
        }
        let first = name.split('/').next().unwrap_or("");
        if first.is_empty() { continue; }
        root_candidates.insert(first.to_string());

        // Match files of the form `<root>/plugin.toml` (no nesting).
        if !f.is_dir() {
            let parts: Vec<&str> = name.split('/').filter(|p| !p.is_empty()).collect();
            if parts.len() == 2 && parts[1] == "plugin.toml" {
                has_manifest = true;
                manifest_root = Some(parts[0].to_string());
            }
        }
    }
    if !has_manifest {
        return Err(AppError::Other(
            "archive does not contain `<plugin>/plugin.toml` at the top level".into()
        ));
    }
    if root_candidates.len() != 1 {
        return Err(AppError::Other(
            "archive must be rooted at a single top-level folder".into()
        ));
    }
    let root = manifest_root.expect("manifest_root set above");
    if !slug_ok(&root) {
        return Err(AppError::Other(format!(
            "plugin folder name '{root}' contains unsafe characters — \
             only ASCII letters, digits, '-' and '_' are allowed"
        )));
    }

    // ── Refuse to overwrite an existing plugin folder.
    let plugins_dir = crate::plugin::runtime::plugin_dir();
    std::fs::create_dir_all(&plugins_dir)?;
    let target_root = plugins_dir.join(&root);
    if target_root.exists() {
        return Err(AppError::Other(format!(
            "plugin '{root}' already installed — remove or rename the existing \
             folder before importing again"
        )));
    }

    // ── Pass 2: extract.
    let mut written = 0usize;
    for i in 0..archive.len() {
        let mut f = archive.by_index(i)
            .map_err(|e| AppError::Other(format!("zip read: {e}")))?;
        let raw_name = f.name().replace('\\', "/");
        if raw_name.contains("..") || raw_name.starts_with('/') {
            return Err(AppError::Other(format!("unsafe path in archive: {raw_name}")));
        }

        // Build the extraction path inside `plugins/`. The archive already
        // contains the root folder so we reuse the relative name verbatim.
        let mut out_path = PathBuf::from(&plugins_dir);
        for part in raw_name.split('/').filter(|p| !p.is_empty()) {
            out_path.push(part);
        }

        if f.is_dir() {
            std::fs::create_dir_all(&out_path)?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out_file = std::fs::File::create(&out_path)?;
        std::io::copy(&mut f, &mut out_file)?;
        written += 1;
    }

    // Read the manifest's `name` field for the user-facing label (falls back
    // to the directory name if the TOML can't be parsed).
    let plugin_name = match std::fs::read_to_string(target_root.join("plugin.toml")) {
        Ok(s) => match s.parse::<toml::Value>() {
            Ok(v) => v.get("name")
                .and_then(|n| n.as_str())
                .map(String::from)
                .unwrap_or_else(|| root.clone()),
            Err(_) => root.clone(),
        },
        Err(_) => root.clone(),
    };

    Ok(ImportPluginResult {
        plugin_name,
        plugin_dir: target_root.to_string_lossy().to_string(),
        files:      written,
    })
}

/// Read a previously-exported zip on disk and forward to `import_plugin_zip`.
/// Saves the frontend the trouble of pulling the bytes through the Tauri FS
/// plugin when the user picks a path with the open dialog.
#[tauri::command]
pub fn import_plugin_zip_from_path(
    state: State<'_, AppState>,
    path: String,
) -> Result<ImportPluginResult, AppError> {
    let bytes = std::fs::read(&path)?;
    import_plugin_zip(state, bytes)
}
