//! `naming` domain — the per-repo naming convention: read it, write it, and scan a buffer with it.
//!
//! The rules themselves are [`bennu_naming`], a leaf that knows nothing about projects. This is the
//! glue: where the config lives (`<repo>/.arbor/bennu/config.toml` `[naming]`, like every other
//! per-repo section — CLAUDE.md rule 11), which project owns a file, and the cache that keeps a
//! project-wide pass from re-reading the same TOML once per file.
//!
//! ## The cache is invalidated by the writer, not by a clock
//!
//! Validation asks for the config on every debounce and once per file on a project-wide pass, so
//! reading the file each time would be thousands of small reads. It is cached per root and dropped
//! the moment [`bennu_set_naming_config`] writes — which means changing a rule takes effect on the
//! next keystroke, with no staleness window to explain. A TTL would have been simpler and would
//! have produced exactly the bug worth avoiding here: "I changed the convention and nothing
//! happened."
//!
//! ## Why the scan merges here rather than inside `bennu-check`
//!
//! A naming rule is not a Java rule — the pack is language-parametric by construction, and
//! `bennu-check` is the Java validator. Merging at the diagnostics funnel
//! ([`crate::intel::bennu_diagnostics`]) keeps it one contributor among several, the same way a
//! framework extension is, and keeps the Java checks free of a dependency on a feature that a
//! project may never switch on.
//!
//! It costs one extra parse of the buffer when the feature IS on. That is the reason the guards in
//! [`bennu_naming::prelude::violations`] are ordered the way they are: a project that has not opted
//! in — the default — never reaches a grammar at all, so the cost is paid only by projects that
//! asked for it.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use bennu_core::prelude::BennuState;
use bennu_naming::prelude::{
    packs, Convention, LanguageRules, NamingConfig, Pack, Target, Violation,
};
use bennu_proto::prelude::{Diagnostic, LspSymbol};
use serde::{Deserialize, Serialize};

/// The TOML section this domain owns.
const SECTION: &str = "naming";

// ── the wire ────────────────────────────────────────────────────────────────────

/// One language pack, as a settings screen needs it: what to call it, what it claims, and what it
/// would fill in if the user asked for the community standard.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackInfo {
    pub id: String,
    pub label: String,
    pub extensions: Vec<String>,
    /// The conventions this language's community uses — offered, never applied on its own.
    pub standard: LanguageRules,
    /// `"grammar"` — parsed here, so every declaration is visible including locals and parameters —
    /// or `"symbols"`, taken from a language server's outline, which holds only types and members.
    pub source: String,
    /// The targets this pack can actually report. A settings screen greys out the rest rather than
    /// offering a rule that would silently never fire.
    pub supported: Vec<Target>,
    /// Whether the open project actually contains a file this pack claims.
    ///
    /// Project Configuration is a screen about **this project**, so a pure-Java tree has no
    /// business being asked how it would like its TypeScript spelled. `true` when no project was
    /// named (the catalog was asked in the abstract), so the field never hides anything by
    /// accident.
    pub present: bool,
}

/// One configurable target (`method`, `local`, …) plus how to name it in a UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetInfo {
    pub id: Target,
    pub label: String,
    /// Whether a rename of this can only ever touch the file it is declared in — what lets the FE
    /// say which fixes are safe to apply in bulk and which need a preview.
    pub file_local: bool,
}

/// Everything the settings screen needs to draw itself, in one round-trip.
///
/// Data-driven on purpose: the FE renders a row per target and a column per pack from this, so a
/// pack or a target added in Rust appears in the UI with no FE change at all.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NamingCatalog {
    pub packs: Vec<PackInfo>,
    pub targets: Vec<TargetInfo>,
    /// Every convention, in the order a dropdown should list them. The value is its own example.
    pub conventions: Vec<String>,
}

// ── args ────────────────────────────────────────────────────────────────────────

/// Args for [`bennu_naming_catalog`].
#[derive(Deserialize)]
pub struct NamingCatalogArgs {
    /// The open project's root. Optional: without it every pack reads as present.
    #[serde(default)]
    pub root: Option<String>,
}

/// Args for [`bennu_get_naming_config`].
#[derive(Deserialize)]
pub struct GetNamingConfigArgs {
    /// Absolute path to the project root (the dir whose `.arbor/bennu/config.toml` holds it).
    pub root: String,
}

/// Args for [`bennu_set_naming_config`].
#[derive(Deserialize)]
pub struct SetNamingConfigArgs {
    /// Absolute path to the project root.
    pub root: String,
    /// The whole `[naming]` section to persist.
    pub config: NamingConfig,
}

// ── handlers ────────────────────────────────────────────────────────────────────

/// Read `[naming]`. A project that never configured it yields the default — everything off.
#[arbor_rpc::handler]
fn bennu_get_naming_config(
    _ctx: &BennuState,
    args: GetNamingConfigArgs,
) -> Result<NamingConfig, String> {
    Ok(config_for_root(&args.root))
}

/// Persist `[naming]`, leaving every other section of the file intact, and drop the cached copy so
/// the next validation uses it.
#[arbor_rpc::handler]
fn bennu_set_naming_config(_ctx: &BennuState, args: SetNamingConfigArgs) -> Result<(), String> {
    crate::repo_config::save(&args.root, SECTION, &args.config)?;
    invalidate(&args.root);
    Ok(())
}

/// The packs, targets and conventions a settings screen renders from.
///
/// `root` is optional and only decides each pack's `present` flag — the catalog itself is static.
#[arbor_rpc::handler]
fn bennu_naming_catalog(
    _ctx: &BennuState,
    args: NamingCatalogArgs,
) -> Result<NamingCatalog, String> {
    Ok(catalog(args.root.as_deref()))
}

/// One pack, described for a settings screen. `present` says whether this project has any file it
/// claims (see [`languages_present`]).
fn pack_info(pack: &'static Pack, present: bool) -> PackInfo {
    PackInfo {
        id: pack.id.to_string(),
        label: pack.label.to_string(),
        extensions: pack.extensions.iter().map(|e| e.to_string()).collect(),
        standard: LanguageRules::from_pairs(pack.standard.iter().copied()),
        source: if pack.is_symbol_backed() { "symbols" } else { "grammar" }.to_string(),
        supported: Target::ALL.into_iter().filter(|t| pack.supports(*t)).collect(),
        present,
    }
}

/// The whole catalog. A function rather than the handler's body so the test asserts on exactly
/// what the FE receives, instead of on a second copy that could drift from it.
fn catalog(root: Option<&str>) -> NamingCatalog {
    let found = root.map(languages_present);
    NamingCatalog {
        packs: packs()
            .iter()
            // No project named → every pack is "present": the catalog was asked in the abstract,
            // and answering `false` there would hide all of them.
            .map(|pack| {
                pack_info(pack, found.as_ref().is_none_or(|ids| ids.contains(pack.id)))
            })
            .collect(),
        targets: Target::ALL
            .into_iter()
            .map(|target| TargetInfo {
                id: target,
                label: target.label().to_string(),
                file_local: target.is_file_local(),
            })
            .collect(),
        conventions: Convention::ALL.iter().map(|c| c.as_str().to_string()).collect(),
    }
}

// ── the scan, for the diagnostics funnel and for Alt+Enter ──────────────────────

/// The naming violations in `source`, as wire diagnostics. Empty unless the owning project opted
/// in — which is the common case, and costs one map lookup.
pub(crate) fn diagnostics_for(file: &str, source: &str) -> Vec<Diagnostic> {
    violations_for(file, source).iter().map(Violation::to_diagnostic).collect()
}

/// Every violation in `source`, whichever route this file's pack uses.
///
/// The one place that resolves the project, picks the route and fetches an outline if one is
/// needed — so a caller (a diagnostic, an intention, the bulk fix) never has to know which of the
/// two kinds of pack it is dealing with.
pub(crate) fn violations_for(file: &str, source: &str) -> Vec<Violation> {
    let Some((rel, config)) = owning_project(file) else { return Vec::new() };
    match outline_for(file, &rel, source, &config) {
        Some(symbols) => {
            bennu_naming::prelude::violations_from_symbols(&rel, &symbols, source, &config)
        }
        None => bennu_naming::prelude::violations(&rel, source, &config),
    }
}

/// The violation the caret at `offset` is inside, if any — what an intention offers to fix.
pub(crate) fn violation_at(file: &str, source: &str, offset: usize) -> Option<Violation> {
    violations_for(file, source).into_iter().find(|v| offset >= v.start && offset <= v.end)
}

/// The language server's outline for `file`, but **only** when this file's rules would actually be
/// read from one.
///
/// The guard is the point. `needs_symbols` answers from the config and the path alone — no server
/// is contacted for a Java file, for a project that has not opted in, for a file whose pack has
/// every target off, or for generated code. Without it, every keystroke on every file would put a
/// `documentSymbol` round-trip in front of the diagnostics the user is waiting for.
///
/// `None` means "use the grammar route". An empty outline (no server installed, or one still
/// warming up) is `Some(vec![])`, which yields no violations rather than falling through to a
/// grammar that does not exist for this language.
fn outline_for(
    file: &str,
    rel: &str,
    source: &str,
    config: &NamingConfig,
) -> Option<Vec<LspSymbol>> {
    bennu_naming::prelude::needs_symbols(rel, source, config)
        .then(|| crate::lsp_route::document_symbols(file, source))
}

/// The file's project-relative path and its project's rules, or `None` when there is no scan to do.
///
/// A file no open project owns is never scanned: the convention is a property of a project, and
/// there is nowhere for a scratch buffer's rules to have come from. The `enabled` check is here
/// too, ahead of the config clone, because this runs once per file on a project-wide pass.
fn owning_project(file: &str) -> Option<(String, NamingConfig)> {
    let root = crate::index_service::IndexService::global().root_for_file(file)?;
    let config = config_for_root(&root);
    config.enabled.then(|| (relative_to(&root, file), config))
}

/// `file` as a project-relative, forward-slashed path — what the `ignore` globs are written
/// against. Falls back to the absolute path when it is somehow not under `root`.
fn relative_to(root: &str, file: &str) -> String {
    let file = file.replace('\\', "/");
    let root = root.replace('\\', "/");
    let root = root.trim_end_matches('/');
    match file.strip_prefix(root).and_then(|rest| rest.strip_prefix('/')) {
        Some(rel) => rel.to_string(),
        None => file,
    }
}

// ── which languages this project actually contains ──────────────────────────────

/// How many directory entries the presence walk will look at before giving up.
///
/// It exists so a pathological tree cannot make opening a settings modal feel broken. Being wrong
/// in the "gave up early" direction costs a pack row the user can still reveal by hand; being wrong
/// in the "walked a million files" direction costs them the modal.
const PRESENCE_BUDGET: usize = 40_000;

/// The ids of the packs that claim at least one file under `root`.
///
/// **Stops as soon as every pack has been seen.** On a project that has some of each — the common
/// case for anything with a web front-end — that is a few hundred entries, not a full tree walk.
/// On a pure-Java tree it walks until the budget runs out, which is the price of proving a
/// negative; it happens once, when the modal opens, off the UI thread.
fn languages_present(root: &str) -> std::collections::BTreeSet<&'static str> {
    let mut found = std::collections::BTreeSet::new();
    let total = packs().len();
    let mut budget = PRESENCE_BUDGET;
    scan_for_packs(std::path::Path::new(root), &mut found, &mut budget, total);
    found
}

fn scan_for_packs(
    dir: &std::path::Path,
    found: &mut std::collections::BTreeSet<&'static str>,
    budget: &mut usize,
    total: usize,
) {
    if found.len() == total || *budget == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        if found.len() == total || *budget == 0 {
            return;
        }
        *budget -= 1;
        let path = entry.path();
        // `is_dir()` stats the entry; `file_type()` is already in hand from the directory read on
        // every platform that matters, and this loop runs tens of thousands of times.
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if crate::find::SKIP_DIRS.contains(&name) {
                continue;
            }
            scan_for_packs(&path, found, budget, total);
        } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if let Some(pack) = bennu_naming::prelude::pack_for_path(name) {
                found.insert(pack.id);
            }
        }
    }
}

// ── the config cache ────────────────────────────────────────────────────────────

fn cache() -> &'static Mutex<HashMap<String, NamingConfig>> {
    static CACHE: OnceLock<Mutex<HashMap<String, NamingConfig>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The project's `[naming]`, read once and remembered until it is written.
fn config_for_root(root: &str) -> NamingConfig {
    let key = root.replace('\\', "/");
    if let Ok(guard) = cache().lock() {
        if let Some(hit) = guard.get(&key) {
            return hit.clone();
        }
    }
    let loaded: NamingConfig = crate::repo_config::load(root, SECTION);
    if let Ok(mut guard) = cache().lock() {
        guard.insert(key, loaded.clone());
    }
    loaded
}

/// Drop the cached copy for `root`.
fn invalidate(root: &str) {
    if let Ok(mut guard) = cache().lock() {
        guard.remove(&root.replace('\\', "/"));
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_paths_are_forward_slashed_and_rooted() {
        assert_eq!(
            relative_to("C:/proj", "C:/proj/src/main/java/Order.java"),
            "src/main/java/Order.java"
        );
        assert_eq!(relative_to("C:/proj/", "C:\\proj\\src\\Order.java"), "src/Order.java");
        // A file outside the root keeps its own path rather than producing a bogus relative one.
        assert_eq!(relative_to("C:/proj", "D:/other/Order.java"), "D:/other/Order.java");
    }

    #[test]
    fn the_catalog_describes_every_pack_and_target() {
        let catalog = catalog(None);
        assert!(catalog.packs.iter().any(|p| p.id == "java"));
        assert_eq!(catalog.targets.len(), Target::ALL.len());
        assert_eq!(catalog.conventions[0], "any", "the off switch must lead the dropdown");
        for pack in &catalog.packs {
            // The standard a pack offers must itself be a valid, non-empty set of rules…
            assert!(!pack.standard.is_off(), "{} offers an empty standard", pack.id);
            // …and it must never propose a rule for something the pack cannot report.
            for (target, _) in &pack.standard.0 {
                assert!(
                    pack.supported.contains(target),
                    "{} offers a standard for {target}, which it never reports",
                    pack.id
                );
            }
        }
    }

    #[test]
    fn presence_finds_the_languages_a_tree_contains_and_no_others() {
        let root = std::env::temp_dir().join("bennu-naming-presence-test");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src/main/java/com/acme")).expect("mkdir");
        std::fs::create_dir_all(root.join("node_modules/pkg")).expect("mkdir");
        std::fs::write(root.join("src/main/java/com/acme/Order.java"), "class Order {}")
            .expect("write");
        // A dependency's TypeScript is not this project's TypeScript — the walk skips the same
        // directories every other project-wide pass does.
        std::fs::write(root.join("node_modules/pkg/index.ts"), "export {}").expect("write");

        let found = languages_present(root.to_str().expect("utf-8"));
        assert!(found.contains("java"));
        assert!(!found.contains("typescript"), "node_modules must not count as project source");

        // Add a real one and it appears.
        std::fs::write(root.join("src/app.ts"), "export {}").expect("write");
        let found = languages_present(root.to_str().expect("utf-8"));
        assert!(found.contains("typescript"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_catalog_says_which_packs_cannot_see_locals() {
        let catalog = catalog(None);
        let java = catalog.packs.iter().find(|p| p.id == "java").expect("java");
        assert_eq!(java.source, "grammar");
        assert!(java.supported.contains(&Target::Local));

        // A server-backed pack reads an outline, which has no locals or parameters in it — the FE
        // greys those rows out on the strength of this.
        let ts = catalog.packs.iter().find(|p| p.id == "typescript").expect("typescript");
        assert_eq!(ts.source, "symbols");
        assert!(!ts.supported.contains(&Target::Local));
        assert!(!ts.supported.contains(&Target::Parameter));
        assert!(ts.supported.contains(&Target::Method));
    }

    #[test]
    fn the_cache_hands_back_what_was_put_in_and_forgets_on_invalidate() {
        let root = "C:/naming-cache-test";
        let seeded = NamingConfig { enabled: true, ..Default::default() };
        cache().lock().unwrap().insert(root.to_string(), seeded.clone());
        assert_eq!(config_for_root(root), seeded);
        invalidate(root);
        // With nothing on disk at that path, the reload is the default — i.e. the cache is gone.
        assert!(!config_for_root(root).enabled);
    }
}
