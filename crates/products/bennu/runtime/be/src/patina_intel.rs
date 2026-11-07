//! `patina_intel` — Patina (`.ptn`, the Voriz mod language) from a `language-intel` extension.
//!
//! ## Nothing about the language lives here
//!
//! The language is in a wasm package (`arbor-extensions-private/plugins/patina`), which exports the
//! `language-intel` interface: this module only builds the request and reshapes the answer into Bennu's wire
//! types. The engine is the shell's, so the call goes over the reverse channel (`__ext_call`), exactly as
//! `shader_mesh` reaches a `mesh-source`. If the package is not installed, the call fails and every entry point
//! answers empty — a `.ptn` is never handed to the Java resolver, which would parse it as Java.
//!
//! ## Every call carries the mod
//!
//! The extension path gives every call a fresh instance, so the backend keeps nothing between two questions.
//! Each request therefore carries the **workspace**: the mod folder (the one with `manifest.ron`, sources under
//! `script/`), the manifest text, and every `.ptn` under it as it is on disk — with the editor's buffer standing in
//! for the file being asked about. A mod is a few dozen small files; the package rebuilds its index from them in
//! tens of milliseconds.
//!
//! ## Build
//!
//! `bennu_patina_build` asks the same package to compile the whole mod with the real compiler (`build`, an **optional** export of
//! `language-intel`) and shows what it says: a report with the checker's and the compiler's diagnostics, the counts, the
//! capabilities and host functions the mod uses, and the schemas. Nothing is produced — a mod is compiled from sources at every
//! load. The same build runs from `bennu_build` when the project root *is* a mod (no `pom.xml`, `Cargo.toml` or Gradle build), so
//! the titlebar button, `Ctrl+F9` and the Problems list work for a mod opened as a project. A package that predates `build`
//! answers "this backend has no build" instead of failing.
//!
//! ## Routing
//!
//! By extension, and by the package's `id`: the `[[provides]]` that serves `.ptn` says `id = "ptn"`. Each entry
//! point returns `Option`, `None` meaning "not mine", like `wgsl_intel`.

use std::path::{Path, PathBuf};

use bennu_core::prelude::BennuState;
use bennu_proto::prelude::{
    BuildDiagnostic, CompletionItem, DeclarationTarget, Diagnostic, HoverInfo, PatinaBuildDiagnostic, PatinaBuildReport, PatinaBuildSchema,
    LspSymbol, RenameEdit, RenameFileEdits, RenamePreview, SourceEdit, UsageHit, UsagesResult,
};
use serde::Deserialize;
use serde_json::{json, Value as Json};

/// The interface and the package id that serve `.ptn`.
const INTERFACE: &str = "language-intel";
const EXTENSION: &str = "ptn";

/// Paths the package makes up for the libraries it carries (`patina-lib://voriz/world.ptn`). Not files: to be opened they are written out,
/// from the package's `library-source`, under `decompiled/patina/` in Bennu's data folder — the same read-only view cache a class decompiled from
/// a jar uses, so the editor already knows not to let anyone type in it and the validators already know not to check it.
const LIBRARY_SCHEME: &str = "patina-lib://";

/// The folder of those views, below the data folder's `decompiled/`.
const LIBRARY_VIEW_DIR: &str = "patina";

/// Whether this file is ours.
pub(crate) fn is_patina(file: &str) -> bool {
    file.rsplit('.').next().is_some_and(|e| e.eq_ignore_ascii_case(EXTENSION))
}

/// Whether `file` is one of those written-out library views (see [`LIBRARY_SCHEME`]). It belongs to no mod: asking the package about it with
/// the folder it sits in as a "mod" would only produce noise, so every entry point answers empty for it.
fn is_library_view(file: &str) -> bool {
    library_views_dir().is_some_and(|d| Path::new(file).starts_with(d))
}

fn library_views_dir() -> Option<PathBuf> {
    Some(arbor_core::prelude::bennu_data_dir().join("decompiled").join(LIBRARY_VIEW_DIR))
}

/// Where the view of a library file lives under `views`: `patina-lib://voriz/world.ptn` is `«views»/voriz/world.ptn`. `None` for a path that
/// is not a library path or that tries to leave the folder (a segment of `..`, an empty one, a drive or a colon).
fn library_view_path(views: &Path, lib: &str) -> Option<PathBuf> {
    let rest = lib.strip_prefix(LIBRARY_SCHEME)?;
    let mut out = views.to_path_buf();
    let mut any = false;
    for seg in rest.split('/') {
        if seg.is_empty() || seg == "." || seg == ".." || seg.contains([':', '\\']) {
            return None;
        }
        out.push(seg);
        any = true;
    }
    any.then_some(out)
}

/// Writes `text` as the view at `path`, only when it is missing or different (a warm view keeps its modification time).
fn write_view(path: &Path, text: &str) -> Option<String> {
    std::fs::create_dir_all(path.parent()?).ok()?;
    if std::fs::read_to_string(path).map(|c| c != text).unwrap_or(true) {
        std::fs::write(path, text).ok()?;
    }
    Some(slash(path))
}

/// The text of a library file, from the package (`library-source`, an answer of `null` when it does not carry that file).
fn library_text(ctx: &BennuState, lib: &str) -> Option<String> {
    call(ctx, "library-source", vec![json!(lib)]).ok().and_then(|v| decode::<Option<String>>("library-source", v)).flatten()
}

/// The read-only view of a library file, written out: its path on disk and its text.
fn library_view(ctx: &BennuState, lib: &str) -> Option<(String, String)> {
    let path = library_view_path(&library_views_dir()?, lib)?;
    let text = library_text(ctx, lib)?;
    Some((write_view(&path, &text)?, text))
}

// ── the request ─────────────────────────────────────────────────────────────────

/// The mod a file belongs to: the nearest folder above it that holds a `manifest.ron` **and** has the file
/// under its `script/`. Without one, the file's own folder.
fn mod_root(file: &Path) -> (PathBuf, Option<String>) {
    let mut dir = file.parent();
    while let Some(d) = dir {
        let manifest = d.join("manifest.ron");
        if manifest.is_file() && file.starts_with(d.join("script")) {
            return (d.to_path_buf(), std::fs::read_to_string(&manifest).ok());
        }
        dir = d.parent();
    }
    (file.parent().map(Path::to_path_buf).unwrap_or_default(), None)
}

/// Every `.ptn` under `dir`, as `(path, text)`. Hidden folders and `target/` are skipped.
fn ptn_files(dir: &Path, out: &mut Vec<(String, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    for p in paths {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if p.is_dir() {
            if !name.starts_with('.') && name != "target" {
                ptn_files(&p, out);
            }
        } else if is_patina(name) {
            if let Ok(text) = std::fs::read_to_string(&p) {
                out.push((slash(&p), text));
            }
        }
    }
}

fn slash(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// The request the package answers: the workspace, the file, its live text.
fn request(file: &str, source: &str) -> Json {
    let (root, _) = mod_root(Path::new(file));
    request_of(&root, file, source)
}

/// The same for a mod whose folder is already known.
fn request_of(mod_dir: &Path, file: &str, source: &str) -> Json {
    let (root, manifest) = if mod_dir.join("manifest.ron").is_file() {
        (mod_dir.to_path_buf(), std::fs::read_to_string(mod_dir.join("manifest.ron")).ok())
    } else {
        (mod_root(Path::new(file)).0, None)
    };
    let sources = if manifest.is_some() { root.join("script") } else { root.clone() };
    let mut files = Vec::new();
    ptn_files(&sources, &mut files);
    json!({
        "workspace": {
            "dir": slash(&root),
            "manifest": manifest,
            "files": files.into_iter().map(|(path, text)| json!({ "path": path, "text": text })).collect::<Vec<_>>(),
        },
        "path": file.replace('\\', "/"),
        "text": source,
        "lang": "",
    })
}

/// One call into the package. An error is logged and returned; the callers turn it into an empty answer.
fn call(ctx: &BennuState, method: &str, args: Vec<Json>) -> Result<Json, String> {
    ctx.host_call(
        "__ext_call",
        json!({
            "plugin": "bennu_patina",
            "spec": { "interface": INTERFACE, "version": 1, "id": EXTENSION, "method": method, "args": args },
        }),
    )
    .map_err(|e| {
        eprintln!("[patina] {method}: {e}");
        e
    })
}

/// The live buffer when the caller sent one, else the file on disk.
fn text_of(file: &str, source: Option<&str>) -> Option<String> {
    match source {
        Some(s) => Some(s.to_string()),
        None => std::fs::read_to_string(file).ok(),
    }
}

// ── the answer's shapes ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct Span {
    start: usize,
    end: usize,
}

#[derive(Deserialize)]
struct WireDiagnostic {
    message: String,
    severity: String,
    code: String,
    span: Span,
}

#[derive(Deserialize)]
struct WireEdit {
    file: String,
    span: Span,
    #[serde(rename = "new-text")]
    new_text: String,
}

#[derive(Deserialize)]
struct WireCompletion {
    label: String,
    kind: String,
    detail: Option<String>,
    #[serde(rename = "insert-text")]
    insert_text: String,
    replace: Span,
    edits: Vec<WireEdit>,
    doc: Option<String>,
}

#[derive(Deserialize)]
struct WireHover {
    signature: String,
    kind: String,
    container: Option<String>,
    doc: Option<String>,
}

#[derive(Deserialize)]
struct WireLocation {
    file: String,
    span: Span,
}

#[derive(Deserialize)]
struct WireRenameEdit {
    file: String,
    span: Span,
    #[serde(rename = "new-text")]
    new_text: String,
    old: String,
    reason: String,
}

fn decode<T: for<'de> Deserialize<'de>>(method: &str, v: Json) -> Option<T> {
    serde_json::from_value(v).map_err(|e| eprintln!("[patina] {method}: unreadable answer: {e}")).ok()
}

// ── entry points ────────────────────────────────────────────────────────────────

/// The package's diagnostics for the file. `None` for a file that is not Patina.
pub(crate) fn diagnostics(ctx: &BennuState, file: &str, source: Option<&str>) -> Option<Vec<Diagnostic>> {
    if !is_patina(file) {
        return None;
    }
    if is_library_view(file) {
        return Some(Vec::new());
    }
    let Some(text) = text_of(file, source) else { return Some(Vec::new()) };
    let out = call(ctx, "diagnostics", vec![request(file, &text)]).ok().and_then(|v| decode::<Vec<WireDiagnostic>>("diagnostics", v));
    Some(
        out.unwrap_or_default()
            .into_iter()
            .map(|d| Diagnostic { message: d.message, severity: d.severity, code: d.code, start: d.span.start, end: d.span.end })
            .collect(),
    )
}

/// Completion at `offset`. `None` for a file that is not Patina.
pub(crate) fn completion(ctx: &BennuState, file: &str, offset: usize, source: Option<&str>) -> Option<Vec<CompletionItem>> {
    if !is_patina(file) {
        return None;
    }
    if is_library_view(file) {
        return Some(Vec::new());
    }
    let Some(text) = text_of(file, source) else { return Some(Vec::new()) };
    let out = call(ctx, "completion", vec![request(file, &text), json!(offset)])
        .ok()
        .and_then(|v| decode::<Vec<WireCompletion>>("completion", v));
    // The package already ranked them: its order is kept through `sort_text`.
    Some(
        out.unwrap_or_default()
            .into_iter()
            .enumerate()
            .map(|(i, c)| CompletionItem {
                label: c.label,
                kind: c.kind,
                detail: c.detail,
                insert_text: Some(c.insert_text),
                replace_start: Some(c.replace.start),
                replace_end: Some(c.replace.end),
                sort_text: Some(format!("{i:05}")),
                doc: c.doc,
                edits: c.edits.into_iter().map(|e| SourceEdit { file: e.file, start: e.span.start, end: e.span.end, new_text: e.new_text }).collect(),
                ..Default::default()
            })
            .collect(),
    )
}

/// The hover card at `offset`. `None` for a file that is not Patina.
pub(crate) fn hover(ctx: &BennuState, file: &str, source: &str, offset: usize) -> Option<Option<HoverInfo>> {
    if !is_patina(file) {
        return None;
    }
    if is_library_view(file) {
        return Some(None);
    }
    let out = call(ctx, "hover", vec![request(file, source), json!(offset)]).ok().and_then(|v| decode::<Option<WireHover>>("hover", v));
    Some(out.flatten().map(|h| HoverInfo { signature: h.signature, kind: h.kind, container: h.container, doc: h.doc, artifact: None }))
}

/// Where the name at `offset` is declared. `None` for a file that is not Patina; `Some(None)` when there is no
/// declaration to open (nothing there). A library the package carries answers with its read-only view on disk.
pub(crate) fn declaration(ctx: &BennuState, file: &str, source: &str, offset: usize) -> Option<Option<DeclarationTarget>> {
    if !is_patina(file) {
        return None;
    }
    if is_library_view(file) {
        return Some(None);
    }
    let out = call(ctx, "definition", vec![request(file, source), json!(offset)])
        .ok()
        .and_then(|v| decode::<Vec<WireLocation>>("definition", v))
        .unwrap_or_default();
    let here = file.replace('\\', "/");
    let Some(l) = out.into_iter().next() else { return Some(None) };
    // A library the package carries (or a file the catalog makes up) is not on disk: it is written out as a read-only view and the jump goes there.
    if l.file.starts_with(LIBRARY_SCHEME) {
        let Some((view, text)) = library_view(ctx, &l.file) else { return Some(None) };
        return Some(Some(target_in(&text, view, &l.span)));
    }
    // The caret already on the declaration: nothing to go to, and a jump onto itself reads as broken.
    if l.file == here && offset >= l.span.start && offset <= l.span.end {
        return Some(None);
    }
    // Another file of the mod, or of a dependency (a real path, maybe outside the open project): read only as far as the package named it.
    let text = if l.file == here { Some(source.to_string()) } else { std::fs::read_to_string(&l.file).ok() };
    Some(Some(target_in(&text.unwrap_or_default(), l.file, &l.span)))
}

/// The jump target for `span` in `text`, which is the content of `file`.
fn target_in(text: &str, file: String, span: &Span) -> DeclarationTarget {
    let (line, col) = line_col(text, span.start);
    let label = text.get(span.start..span.end).unwrap_or("").to_string();
    DeclarationTarget { file, start: span.start, end: span.end, line: line as u32, col: col as u32, label }
}

#[derive(Deserialize)]
struct WireWorkspaceSymbol {
    name: String,
    kind: String,
    file: String,
    span: Span,
    #[serde(default)]
    container: Option<String>,
    #[serde(default)]
    owner: String,
    #[serde(default)]
    library: bool,
}

/// The mod the search is about: the one `scope` is in (a `.ptn` file of it, or the folder), and a file of it to ask the package through.
fn search_scope(scope: &str) -> Option<(PathBuf, PathBuf)> {
    let path = Path::new(scope);
    let root = mod_root_of(path)?;
    if path.is_file() && is_patina(scope) && !is_library_view(scope) {
        return Some((root, path.to_path_buf()));
    }
    let mut files = Vec::new();
    ptn_files(&root.join("script"), &mut files);
    files.into_iter().next().map(|(p, _)| (root, PathBuf::from(p)))
}

/// Names across the mod of `scope` — its files, the mods it depends on and the libraries the package carries — for "go to class" (Cmd+N) and
/// "go to symbol". `scope` is a project folder or a `.ptn` file; empty when it is in no mod, when the package predates the
/// `workspace-symbols` export (an optional one, like `build`: nothing to show, nothing broken) or when it fails.
pub(crate) fn workspace_symbols(ctx: &BennuState, scope: &str, query: &str) -> Vec<LspSymbol> {
    let Some((root, file)) = search_scope(scope) else { return Vec::new() };
    let file = slash(&file);
    let Ok(text) = std::fs::read_to_string(&file) else { return Vec::new() };
    let out = match call(ctx, "workspace-symbols", vec![request_of(&root, &file, &text), json!(query)]) {
        Ok(v) => decode::<Vec<WireWorkspaceSymbol>>("workspace-symbols", v).unwrap_or_default(),
        Err(_) => return Vec::new(),
    };
    symbols_of(out, |lib| library_view(ctx, lib), |p| std::fs::read_to_string(p).ok())
}

/// The wire symbols as Bennu's, with the line and column of each name. `library` writes out a library file and gives its path and text; `read`
/// reads a file of the mod or of a dependency. A file that cannot be read has no line to land on, so its symbols are dropped rather than sent
/// to line 1.
fn symbols_of(
    wire: Vec<WireWorkspaceSymbol>,
    mut library: impl FnMut(&str) -> Option<(String, String)>,
    mut read: impl FnMut(&str) -> Option<String>,
) -> Vec<LspSymbol> {
    let mut texts: std::collections::HashMap<String, Option<(String, String)>> = std::collections::HashMap::new();
    let mut out = Vec::new();
    for w in wire {
        let slot = texts.entry(w.file.clone()).or_insert_with(|| {
            if w.library { library(&w.file) } else { read(&w.file).map(|t| (w.file.clone(), t)) }
        });
        let Some((file, text)) = slot.as_ref() else { continue };
        let (line, _) = line_col(text, w.span.start);
        let col = text[line_start(text, w.span.start)..w.span.start.min(text.len())].encode_utf16().count() + 1;
        let detail = match (&w.container, w.owner.is_empty()) {
            (Some(c), false) => Some(format!("{}::{c}", w.owner)),
            (Some(c), true) => Some(c.clone()),
            (None, false) => Some(w.owner.clone()),
            (None, true) => None,
        };
        out.push(LspSymbol {
            name: w.name,
            kind: w.kind,
            detail,
            start: w.span.start,
            end: w.span.end,
            name_start: w.span.start,
            name_end: w.span.end,
            line,
            col,
            file: file.clone(),
            deprecated: false,
            children: Vec::new(),
        });
    }
    out
}

/// The uses of the name at `offset`, across the mod. `None` for a file that is not Patina.
pub(crate) fn references(ctx: &BennuState, file: &str, source: &str, offset: usize) -> Option<Option<UsagesResult>> {
    if !is_patina(file) {
        return None;
    }
    if is_library_view(file) {
        return Some(None);
    }
    let out = call(ctx, "references", vec![request(file, source), json!(offset), json!(true)])
        .ok()
        .and_then(|v| decode::<Vec<WireLocation>>("references", v))
        .unwrap_or_default();
    let here = file.replace('\\', "/");
    let mut texts: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let usages: Vec<UsageHit> = out
        .into_iter()
        .filter(|l| !l.file.starts_with(LIBRARY_SCHEME))
        .map(|l| {
            let text = texts.entry(l.file.clone()).or_insert_with(|| if l.file == here { source.to_string() } else { std::fs::read_to_string(&l.file).unwrap_or_default() });
            let (line, col) = line_col(text, l.span.start);
            let preview = text[line_start(text, l.span.start)..].lines().next().unwrap_or("").trim().to_string();
            UsageHit { file: l.file, start: l.span.start, end: l.span.end, line, col, preview, via: None }
        })
        .collect();
    if usages.is_empty() {
        return Some(None);
    }
    let label = name_at(source, offset);
    Some(Some(UsagesResult { target_label: label, usages }))
}

/// The edits of a rename, flat. `None` for a file that is not Patina; empty when the package refuses (a
/// keyword, a library name, nothing there).
pub(crate) fn rename_apply(ctx: &BennuState, file: &str, source: &str, offset: usize, new_name: &str) -> Option<Vec<RenameEdit>> {
    if !is_patina(file) {
        return None;
    }
    if is_library_view(file) {
        return Some(Vec::new());
    }
    let out = call(ctx, "rename", vec![request(file, source), json!(offset), json!(new_name)])
        .ok()
        .and_then(|v| decode::<Vec<WireRenameEdit>>("rename", v))
        .unwrap_or_default();
    Some(
        out.into_iter()
            .map(|e| RenameEdit { file: e.file, start: e.span.start, end: e.span.end, new_text: e.new_text, old: e.old, reason: e.reason, inferred: false })
            .collect(),
    )
}

/// The rename preview: the same edits, grouped by file. `Some(None)` when there is nothing to rename.
pub(crate) fn rename_plan(ctx: &BennuState, file: &str, source: &str, offset: usize, new_name: &str) -> Option<Option<RenamePreview>> {
    let edits = rename_apply(ctx, file, source, offset, new_name)?;
    if edits.is_empty() {
        return Some(None);
    }
    let old_name = edits[0].old.clone();
    let mut files: Vec<RenameFileEdits> = Vec::new();
    for e in edits {
        match files.iter_mut().find(|f| f.file == e.file) {
            Some(f) => f.edits.push(e),
            None => files.push(RenameFileEdits { file: e.file.clone(), edits: vec![e] }),
        }
    }
    for f in &mut files {
        f.edits.sort_by_key(|e| e.start);
    }
    let total_edits = files.iter().map(|f| f.edits.len()).sum();
    Some(Some(RenamePreview {
        target_label: format!("`{old_name}`"),
        old_name,
        new_name: new_name.to_string(),
        files,
        total_edits,
        has_inferred: false,
        blocked: None,
        file_rename: None,
    }))
}

// ── build ───────────────────────────────────────────────────────────────────────

/// Whether `dir` is a Patina mod: a `manifest.ron` and a `script/` folder with at least one `.ptn` in it.
pub(crate) fn is_mod_root(dir: &Path) -> bool {
    dir.join("manifest.ron").is_file() && has_ptn(&dir.join("script"))
}

fn has_ptn(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else { return false };
    entries.filter_map(|e| e.ok().map(|e| e.path())).any(|p| {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if p.is_dir() { !name.starts_with('.') && name != "target" && has_ptn(&p) } else { is_patina(name) }
    })
}

/// The mod a path belongs to: the nearest folder at or above it that is a mod root. `path` can be a file or a folder.
pub(crate) fn mod_root_of(path: &Path) -> Option<PathBuf> {
    let mut dir = if path.is_dir() { Some(path) } else { path.parent() };
    while let Some(d) = dir {
        if is_mod_root(d) {
            return Some(d.to_path_buf());
        }
        dir = d.parent();
    }
    None
}

#[derive(Deserialize)]
struct WireBuildDiagnostic {
    severity: String,
    #[serde(default)]
    origin: String,
    code: String,
    #[serde(default)]
    file: Option<String>,
    #[serde(default)]
    span: Option<Span>,
    message: String,
}

#[derive(Deserialize)]
struct WireBuildSchema {
    name: String,
    kind: String,
}

/// The package's `build-report`. Everything but `ok` and the diagnostics is `default`, so a package that sends fewer fields still
/// reads, and one that sends more is not refused (no `deny_unknown_fields`). The host spells a kebab field both ways
/// (`mod-id` and `mod_id`): only the kebab one is read, and an `alias` would make serde reject the pair as a duplicate.
#[derive(Deserialize)]
struct WireBuildReport {
    ok: bool,
    #[serde(rename = "mod-id", default)]
    mod_id: String,
    diagnostics: Vec<WireBuildDiagnostic>,
    #[serde(default)]
    files: u32,
    #[serde(default)]
    functions: u32,
    #[serde(default)]
    entries: u32,
    #[serde(rename = "entry-list", default)]
    entry_list: Vec<String>,
    #[serde(default)]
    schemas: u32,
    #[serde(rename = "schema-list", default)]
    schema_list: Vec<WireBuildSchema>,
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default)]
    imports: Vec<String>,
    #[serde(default)]
    dependencies: Vec<String>,
    #[serde(default)]
    warnings: Vec<String>,
    #[serde(rename = "elapsed-ms", default)]
    elapsed_ms: u32,
}

/// What a failed call to the package means for the user. The host's error for an export an older package lacks names the
/// interface and the function; it is turned into a sentence instead of being shown as a crash.
fn build_call_error(e: &str) -> String {
    if e.contains("has no function 'build'") || e.contains("no function 'build'") {
        "This Patina backend has no build: the installed package predates the `build` export. Rebuild and reinstall it \
         (arbor-extensions-private/plugins/patina/build.sh --install)."
            .to_string()
    } else if e.contains("exports no interface") {
        "The installed Patina package is not a language-intel backend.".to_string()
    } else {
        format!("The Patina package could not build the mod: {e}")
    }
}

/// The summary line: `3 file(s), 12 function(s), 1 entry, 2 schema(s) in 38 ms`.
fn summary_of(r: &WireBuildReport) -> String {
    format!(
        "{} file(s), {} function(s), {} entr{}, {} schema(s) in {} ms",
        r.files,
        r.functions,
        r.entries,
        if r.entries == 1 { "y" } else { "ies" },
        r.schemas,
        r.elapsed_ms
    )
}

/// The wire report as the contract's, with a line/col for every diagnostic that has a file and a span (read from disk: a build
/// reports the sources as they are saved).
fn to_report(root: &Path, w: WireBuildReport) -> PatinaBuildReport {
    let mut texts: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let summary = summary_of(&w);
    let diagnostics = w
        .diagnostics
        .into_iter()
        .map(|d| {
            let (line, col) = match (&d.file, &d.span) {
                (Some(f), Some(s)) => {
                    let text = texts.entry(f.clone()).or_insert_with(|| std::fs::read_to_string(f).unwrap_or_default());
                    if text.is_empty() { (None, None) } else { let (l, c) = line_col(text, s.start); (Some(l as u32), Some(c as u32)) }
                }
                _ => (None, None),
            };
            PatinaBuildDiagnostic {
                severity: d.severity,
                origin: d.origin,
                code: d.code,
                file: d.file,
                line,
                col,
                start: d.span.as_ref().map(|s| s.start),
                end: d.span.as_ref().map(|s| s.end),
                message: d.message,
            }
        })
        .collect();
    PatinaBuildReport {
        ok: w.ok,
        mod_id: w.mod_id,
        mod_root: slash(root),
        diagnostics,
        files: w.files,
        functions: w.functions,
        entries: w.entries,
        entry_list: w.entry_list,
        schemas: w.schemas,
        schema_list: w.schema_list.into_iter().map(|s| PatinaBuildSchema { name: s.name, kind: s.kind }).collect(),
        capabilities: w.capabilities,
        imports: w.imports,
        dependencies: w.dependencies,
        warnings: w.warnings,
        elapsed_ms: w.elapsed_ms,
        summary,
    }
}

/// The lines the Build panel prints for a report: what was built, every problem with `file:line:col`, then what the mod uses.
pub(crate) fn report_lines(r: &PatinaBuildReport) -> Vec<String> {
    let mut out = vec![format!("Patina build of `{}` ({})", r.mod_id, r.mod_root)];
    for d in &r.dependencies {
        out.push(format!("dependency {d}"));
    }
    for d in &r.diagnostics {
        let place = match (&d.file, d.line, d.col) {
            (Some(f), Some(l), Some(c)) => format!("{f}:{l}:{c}: "),
            (Some(f), _, _) => format!("{f}: "),
            _ => String::new(),
        };
        out.push(format!("{place}{}[{}]: {}", d.severity, d.code, d.message));
    }
    for w in &r.warnings {
        out.push(format!("note: {w}"));
    }
    if r.ok {
        if !r.capabilities.is_empty() {
            out.push(format!("capabilities used: {}", r.capabilities.join(", ")));
        }
        if !r.imports.is_empty() {
            out.push(format!("host functions imported: {}", r.imports.len()));
            out.extend(r.imports.iter().map(|i| format!("  {i}")));
        }
        for e in &r.entry_list {
            out.push(format!("entry {e}"));
        }
        for s in &r.schema_list {
            out.push(format!("schema {} ({})", s.name, s.kind));
        }
    }
    out.push(format!("{}: {}", if r.ok { "Patina build succeeded" } else { "Patina build failed" }, r.summary));
    out
}

/// The build itself, with the output streamed as `arbor://bennu/build-output`. `Err` only when no report could be had at all.
fn run_build(ctx: &BennuState, root: &Path) -> Result<PatinaBuildReport, String> {
    let manifest = std::fs::read_to_string(root.join("manifest.ron")).map_err(|e| format!("cannot read {}: {e}", root.join("manifest.ron").display()))?;
    let mut files = Vec::new();
    ptn_files(&root.join("script"), &mut files);
    let workspace = json!({
        "dir": slash(root),
        "manifest": manifest,
        "files": files.into_iter().map(|(path, text)| json!({ "path": path, "text": text })).collect::<Vec<_>>(),
    });
    let answer = call(ctx, "build", vec![workspace, json!("")]).map_err(|e| build_call_error(&e))?;
    let wire: WireBuildReport =
        serde_json::from_value(answer).map_err(|e| format!("The Patina package answered the build in a shape this Arbor does not read: {e}"))?;
    let report = to_report(root, wire);
    let sink = ctx.event_sink();
    for line in report_lines(&report) {
        sink.emit(crate::build::EVT_BUILD_OUTPUT, json!({ "text": line }));
    }
    Ok(report)
}

/// Args for [`bennu_patina_build`].
#[derive(Deserialize)]
pub struct PatinaBuildArgs {
    /// A `.ptn` file of the mod, or the mod folder: the nearest folder above it with a `manifest.ron` and `.ptn` sources under `script/` is built.
    pub path: String,
}

/// Compile a Patina mod with the real compiler and report everything it says (see the module doc). Queued like every build of the
/// project, so it does not run beside a Maven or Cargo build of the same tree.
#[arbor_rpc::handler]
fn bennu_patina_build(ctx: &BennuState, args: PatinaBuildArgs) -> Result<PatinaBuildReport, String> {
    let root = mod_root_of(Path::new(&args.path)).ok_or_else(|| {
        format!("{} is not inside a Patina mod: a mod is a folder with a manifest.ron and .ptn sources under script/", args.path)
    })?;
    let root_path = slash(&root);
    let sink = ctx.event_sink();
    sink.progress("Patina build", None, None);
    let served = crate::build_queue::coalesced(
        sink,
        &root_path,
        crate::build_queue::Kind::Build,
        "patina".to_string(),
        "patina build",
        |r: &PatinaBuildReport| r.ok,
        || run_build(ctx, &root),
    )?;
    Ok(served.value)
}

/// `bennu_build` on a root that is a mod: the report as the parsed diagnostics the Build panel already shows.
pub(crate) fn as_build_result(report: &PatinaBuildReport) -> Vec<BuildDiagnostic> {
    report
        .diagnostics
        .iter()
        .map(|d| BuildDiagnostic { file: d.file.clone(), line: d.line, col: d.col, severity: d.severity.clone(), message: format!("[{}] {}", d.code, d.message) })
        .collect()
}

/// The build of a project root that is a mod, for `bennu_build`: its streamed output is already out, so only the outcome is returned.
pub(crate) fn build_mod_root(ctx: &BennuState, root: &Path) -> Result<(bool, Vec<BuildDiagnostic>), String> {
    ctx.event_sink().progress("Patina build", None, None);
    let report = run_build(ctx, root)?;
    Ok((report.ok, as_build_result(&report)))
}

// ── text helpers ────────────────────────────────────────────────────────────────

/// Byte offset of the start of the line containing `at`.
fn line_start(text: &str, at: usize) -> usize {
    text[..at.min(text.len())].rfind('\n').map(|i| i + 1).unwrap_or(0)
}

/// 1-based line and column (in characters) of `at`.
fn line_col(text: &str, at: usize) -> (usize, usize) {
    let at = at.min(text.len());
    let line = text[..at].matches('\n').count() + 1;
    let col = text[line_start(text, at)..at].chars().count() + 1;
    (line, col)
}

/// The identifier around `offset`, for a results label.
fn name_at(text: &str, offset: usize) -> String {
    let offset = offset.min(text.len());
    let is_ident = |c: char| c.is_alphanumeric() || c == '_';
    let start = text[..offset].char_indices().rev().take_while(|&(_, c)| is_ident(c)).last().map(|(i, _)| i).unwrap_or(offset);
    let end = text[offset..].char_indices().find(|&(_, c)| !is_ident(c)).map(|(i, _)| offset + i).unwrap_or(text.len());
    text[start..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bennu-patina-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn only_ptn_files_are_ours() {
        assert!(is_patina("/m/script/a.ptn"));
        assert!(is_patina("C:/m/script/A.PTN"));
        assert!(!is_patina("/m/script/a.rs"));
        assert!(!is_patina("/m/manifest.ron"));
    }

    #[test]
    fn the_mod_root_is_the_folder_with_the_manifest_above_script() {
        let d = scratch("root");
        std::fs::create_dir_all(d.join("script/brains")).unwrap();
        std::fs::write(d.join("manifest.ron"), "(id: \"marmo\")").unwrap();
        let (root, manifest) = mod_root(&d.join("script/brains/shy.ptn"));
        assert_eq!(root, d);
        assert_eq!(manifest.as_deref(), Some("(id: \"marmo\")"));
    }

    #[test]
    fn a_manifest_that_does_not_own_the_file_is_not_its_mod() {
        // A `.ptn` beside the manifest, not under `script/`: not part of the mod's sources.
        let d = scratch("beside");
        std::fs::write(d.join("manifest.ron"), "(id: \"marmo\")").unwrap();
        let (root, manifest) = mod_root(&d.join("notes.ptn"));
        assert_eq!(root, d);
        assert!(manifest.is_none());
    }

    #[test]
    fn the_request_carries_every_ptn_of_the_mod_and_the_live_text() {
        let d = scratch("request");
        std::fs::create_dir_all(d.join("script/brains")).unwrap();
        std::fs::create_dir_all(d.join("script/.hidden")).unwrap();
        std::fs::write(d.join("manifest.ron"), "(id: \"marmo\")").unwrap();
        std::fs::write(d.join("script/a.ptn"), "fn a() {}").unwrap();
        std::fs::write(d.join("script/brains/b.ptn"), "fn b() {}").unwrap();
        std::fs::write(d.join("script/.hidden/c.ptn"), "fn c() {}").unwrap();
        std::fs::write(d.join("script/notes.txt"), "x").unwrap();
        let file = slash(&d.join("script/a.ptn"));
        let r = request(&file, "fn a() { unsaved }");
        let files: Vec<&str> = r["workspace"]["files"].as_array().unwrap().iter().map(|f| f["path"].as_str().unwrap()).collect();
        assert_eq!(files.len(), 2, "{files:?}");
        assert!(files.iter().any(|p| p.ends_with("script/brains/b.ptn")));
        assert_eq!(r["text"], "fn a() { unsaved }");
        assert_eq!(r["workspace"]["manifest"], "(id: \"marmo\")");
    }

    // ── build ──────────────────────────────────────────────────────────────────────

    /// What the real package answers (copied from `a_guest_builds_a_mod_and_a_missing_export_says_so` in `arbor-plugin-wasm`): the host
    /// spells every kebab field both ways, and `span` is a record or null.
    const REAL_ANSWER: &str = r#"{"capabilities":["world.write"],"dependencies":["marmo @ /r/marmo"],"diagnostics":[{"code":"compile.unsupported","file":"__FILE__","message":"method `fx::sin` of the base library (in m::main::bad)","origin":"compiler","severity":"error","span":{"end":76,"start":69}},{"code":"build.dependency_cycle","file":null,"message":"cycle","origin":"dependency","severity":"warning","span":null}],"elapsed-ms":212,"elapsed_ms":212,"entries":1,"entry-list":["hook voriz::events::StepOn"],"entry_list":["hook voriz::events::StepOn"],"files":1,"functions":2,"imports":["voriz::world::World::set_block"],"mod-id":"m","mod_id":"m","ok":false,"schema-list":[{"name":"m::main::Total","kind":"state"}],"schema_list":[{"name":"m::main::Total","kind":"state"}],"schemas":1,"warnings":["a note"]}"#;

    fn wire(file: &str) -> WireBuildReport {
        serde_json::from_str(&REAL_ANSWER.replace("__FILE__", file)).expect("the real answer reads")
    }

    #[test]
    fn the_real_answer_of_the_package_reads_and_keeps_every_field() {
        let d = scratch("wire");
        std::fs::create_dir_all(d.join("script")).unwrap();
        let file = slash(&d.join("script/main.ptn"));
        // 69..76 is `x.sin()` on the third line, at column 23.
        std::fs::write(&file, "use marmo::Gem\nfn value(g: Gem) -> int { g.n }\nfn bad(x: fx) -> fx { x.sin() }\n").unwrap();
        let r = to_report(&d, wire(&file));
        assert!(!r.ok);
        assert_eq!((r.mod_id.as_str(), r.files, r.functions, r.entries, r.schemas, r.elapsed_ms), ("m", 1, 2, 1, 1, 212));
        assert_eq!(r.entry_list, vec!["hook voriz::events::StepOn"]);
        assert_eq!(r.schema_list[0].name, "m::main::Total");
        assert_eq!((r.capabilities.len(), r.imports.len(), r.dependencies.len(), r.warnings.len()), (1, 1, 1, 1));
        assert_eq!(r.summary, "1 file(s), 2 function(s), 1 entry, 1 schema(s) in 212 ms");
        let first = &r.diagnostics[0];
        assert_eq!((first.origin.as_str(), first.code.as_str()), ("compiler", "compile.unsupported"));
        assert_eq!((first.start, first.end), (Some(69), Some(76)));
        assert_eq!((first.line, first.col), (Some(3), Some(23)), "line/col come from the file as saved");
    }

    #[test]
    fn a_diagnostic_without_a_file_or_span_has_no_place() {
        let r = to_report(Path::new("/m"), wire("/does/not/exist.ptn"));
        let cycle = &r.diagnostics[1];
        assert_eq!((cycle.file.clone(), cycle.line, cycle.col, cycle.start), (None, None, None, None));
        // The first one has a file the disk does not have: still reported, with the span but no line.
        assert_eq!((r.diagnostics[0].line, r.diagnostics[0].start), (None, Some(69)));
    }

    #[test]
    fn a_package_that_sends_fewer_or_more_fields_still_reads() {
        let fewer: WireBuildReport = serde_json::from_str(r#"{"ok":true,"diagnostics":[]}"#).unwrap();
        assert!(fewer.ok && fewer.files == 0 && fewer.entry_list.is_empty());
        let more: WireBuildReport = serde_json::from_str(r#"{"ok":true,"diagnostics":[],"gas-per-hook":[1,2],"future":{"x":1}}"#).unwrap();
        assert!(more.ok);
    }

    #[test]
    fn an_older_package_without_build_says_so_in_a_sentence() {
        let e = build_call_error("'arbor:extensions/language-intel@1.0.0' has no function 'build'");
        assert!(e.starts_with("This Patina backend has no build"), "{e}");
        assert!(build_call_error("this extension exports no interface 'arbor:extensions/language-intel@1.0.0'").contains("not a language-intel backend"));
        assert!(build_call_error("'build' trapped: boom").contains("could not build the mod: 'build' trapped: boom"));
    }

    #[test]
    fn a_mod_is_a_manifest_with_ptn_sources_under_script() {
        let d = scratch("modroot");
        std::fs::create_dir_all(d.join("script/brains")).unwrap();
        assert!(!is_mod_root(&d), "no manifest");
        std::fs::write(d.join("manifest.ron"), "(id: \"marmo\")").unwrap();
        assert!(!is_mod_root(&d), "a manifest but no sources");
        std::fs::write(d.join("script/brains/shy.ptn"), "fn f() {}").unwrap();
        assert!(is_mod_root(&d));
        // From a file inside, from the folder itself, and from a sub-folder.
        assert_eq!(mod_root_of(&d.join("script/brains/shy.ptn")), Some(d.clone()));
        assert_eq!(mod_root_of(&d), Some(d.clone()));
        assert_eq!(mod_root_of(&d.join("script/brains")), Some(d.clone()));
        // Sources in a hidden folder do not make a mod.
        let h = scratch("hidden");
        std::fs::create_dir_all(h.join("script/.old")).unwrap();
        std::fs::write(h.join("manifest.ron"), "(id: \"x\")").unwrap();
        std::fs::write(h.join("script/.old/a.ptn"), "fn f() {}").unwrap();
        assert!(!is_mod_root(&h));
    }

    #[test]
    fn the_report_lines_are_what_the_build_panel_prints() {
        let d = scratch("lines");
        std::fs::create_dir_all(d.join("script")).unwrap();
        let file = slash(&d.join("script/main.ptn"));
        std::fs::write(&file, "use marmo::Gem\nfn value(g: Gem) -> int { g.n }\nfn bad(x: fx) -> fx { x.sin() }\n").unwrap();
        let lines = report_lines(&to_report(&d, wire(&file)));
        assert!(lines[0].starts_with("Patina build of `m` ("), "{lines:?}");
        assert!(lines.contains(&"dependency marmo @ /r/marmo".to_string()));
        assert!(lines.iter().any(|l| l.contains(":3:") && l.contains("error[compile.unsupported]: method `fx::sin`")), "{lines:?}");
        assert!(lines.contains(&"warning[build.dependency_cycle]: cycle".to_string()));
        assert!(lines.contains(&"note: a note".to_string()));
        assert!(lines.last().unwrap().starts_with("Patina build failed: 1 file(s)"), "{lines:?}");
        // A failed build does not list what the mod uses: nothing was compiled.
        assert!(!lines.iter().any(|l| l.starts_with("capabilities used")));
    }

    #[test]
    fn the_diagnostics_become_the_panels_build_diagnostics() {
        let d = scratch("asdiag");
        let file = slash(&d.join("main.ptn"));
        std::fs::write(&file, "x\n").unwrap();
        let mut w = wire(&file);
        w.diagnostics[0].span = Some(Span { start: 0, end: 1 });
        let diags = as_build_result(&to_report(&d, w));
        assert_eq!(diags[0].file.as_deref(), Some(file.as_str()));
        assert_eq!((diags[0].line, diags[0].col, diags[0].severity.as_str()), (Some(1), Some(1), "error"));
        assert!(diags[0].message.starts_with("[compile.unsupported] "));
        assert_eq!(diags[1].file, None);
    }

    #[test]
    fn a_path_outside_any_mod_is_refused_with_a_sentence_and_nothing_is_called() {
        let d = scratch("nomod");
        assert_eq!(mod_root_of(&d.join("a.ptn")), None);
    }

    #[test]
    fn line_col_and_name_at() {
        let t = "fn f() {\n    let àb = 1\n}";
        let at = t.find("àb").unwrap();
        assert_eq!(line_col(t, at), (2, 9));
        assert_eq!(name_at(t, at), "àb");
    }

    // ── go to a declaration, and the name search (Cmd+N) ──────────────────────────

    #[test]
    fn a_library_path_becomes_a_view_inside_the_views_folder_and_nothing_else_does() {
        let views = Path::new("/data/decompiled/patina");
        assert_eq!(library_view_path(views, "patina-lib://voriz/world.ptn"), Some(views.join("voriz").join("world.ptn")));
        assert_eq!(library_view_path(views, "patina-lib://ui/barra.ptn"), Some(views.join("ui").join("barra.ptn")));
        for bad in ["/etc/passwd", "patina-lib://", "patina-lib://../x.ptn", "patina-lib://voriz/../../x", "patina-lib://voriz//x", "patina-lib://c:/x", "patina-lib://voriz\\x"] {
            assert_eq!(library_view_path(views, bad), None, "{bad}");
        }
    }

    #[test]
    fn a_view_is_written_once_and_rewritten_only_when_it_changes() {
        let d = scratch("view");
        let p = d.join("voriz/world.ptn");
        assert_eq!(write_view(&p, "pub struct World").as_deref(), Some(slash(&p).as_str()));
        let first = std::fs::metadata(&p).unwrap().modified().unwrap();
        write_view(&p, "pub struct World").unwrap();
        assert_eq!(std::fs::metadata(&p).unwrap().modified().unwrap(), first, "same text, same file");
        write_view(&p, "pub struct World2").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "pub struct World2");
    }

    #[test]
    fn a_library_view_belongs_to_no_mod_so_every_question_about_it_is_empty() {
        let view = slash(&library_views_dir().unwrap().join("voriz/world.ptn"));
        assert!(is_library_view(&view));
        assert!(!is_library_view("/mods/marmo/script/a.ptn"));
        assert!(!is_library_view(&slash(&arbor_core::prelude::bennu_data_dir().join("decompiled/java/util/List.java"))));
    }

    #[test]
    fn the_target_of_a_jump_has_the_line_the_column_and_the_name() {
        let text = "pub struct A {}\nfn àb() {}\n";
        let at = text.find("àb").unwrap();
        let t = target_in(text, "/m/script/a.ptn".into(), &Span { start: at, end: at + "àb".len() });
        assert_eq!((t.line, t.col, t.label.as_str(), t.file.as_str()), (2, 4, "àb", "/m/script/a.ptn"));
    }

    fn sym(name: &str, kind: &str, file: &str, start: usize, container: Option<&str>, owner: &str, library: bool) -> WireWorkspaceSymbol {
        WireWorkspaceSymbol { name: name.into(), kind: kind.into(), file: file.into(), span: Span { start, end: start + name.len() }, container: container.map(str::to_owned), owner: owner.into(), library }
    }

    #[test]
    fn name_search_hits_carry_line_column_kind_and_where_they_come_from() {
        let mod_file = "pub struct Tally {}\n\npub enum Kind { Rock, Ore }\n";
        let hits = vec![
            sym("Kind", "enum", "/m/script/a.ptn", mod_file.find("Kind").unwrap(), None, "m", false),
            sym("Ore", "enum-member", "/m/script/a.ptn", mod_file.find("Ore").unwrap(), Some("Kind"), "m", false),
            sym("Player", "struct", "patina-lib://voriz/players.ptn", 11, None, "voriz", true),
            sym("Gone", "struct", "/dep/script/gone.ptn", 0, None, "dep", false),
        ];
        let out = symbols_of(
            hits,
            |lib| (lib == "patina-lib://voriz/players.ptn").then(|| ("/data/decompiled/patina/voriz/players.ptn".to_string(), "// players\npub struct Player".to_string())),
            |p| (p == "/m/script/a.ptn").then(|| mod_file.to_string()),
        );
        let names: Vec<&str> = out.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["Kind", "Ore", "Player"], "a file that cannot be read has no line to land on, so its hits are dropped");
        assert_eq!((out[0].kind.as_str(), out[0].line, out[0].col, out[0].file.as_str()), ("enum", 3, 10, "/m/script/a.ptn"));
        assert_eq!(out[0].detail.as_deref(), Some("m"));
        assert_eq!((out[1].kind.as_str(), out[1].detail.as_deref()), ("enum-member", Some("m::Kind")));
        // A library hit opens the read-only view that was written out, not the made-up path.
        assert_eq!((out[2].file.as_str(), out[2].line), ("/data/decompiled/patina/voriz/players.ptn", 2));
    }

    #[test]
    fn the_search_scope_is_the_mod_of_a_folder_or_of_a_file() {
        let d = scratch("scope");
        std::fs::create_dir_all(d.join("script/brains")).unwrap();
        std::fs::write(d.join("manifest.ron"), "(id: \"marmo\")").unwrap();
        std::fs::write(d.join("script/brains/shy.ptn"), "fn f() {}").unwrap();
        std::fs::write(d.join("script/a.ptn"), "fn g() {}").unwrap();
        let (root, file) = search_scope(&slash(&d)).expect("the folder is a mod");
        assert_eq!(root, d);
        assert!(file.starts_with(d.join("script")) && is_patina(&slash(&file)));
        let (root, file) = search_scope(&slash(&d.join("script/brains/shy.ptn"))).unwrap();
        assert_eq!((root, file), (d.clone(), d.join("script/brains/shy.ptn")));
        assert!(search_scope("/nowhere/at/all").is_none());
        assert!(search_scope(&slash(&scratch("nomod"))).is_none());
    }
}
