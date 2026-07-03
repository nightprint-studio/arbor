//! `spell` domain — `bennu_spellcheck` / `bennu_dict_add` / `bennu_spell_status` /
//! `bennu_download_dictionaries` (the editor spell-checker).
//!
//! Spell-checks the words the user *authored* — declaration-name identifiers (class /
//! interface / enum / method / field / variable / parameter names, split by case) and
//! comment text — against `en_US` + `it_IT` Hunspell, a built-in tech allow-list, and the
//! user's custom dictionaries (global `<data>/custom-dict.txt` + per-project
//! `<root>/.arbor/bennu-dict.txt`). The pure engine (tokenizer / allow-list / dictionary
//! cache / Java walk) lives in `bennu_intel::spell`; this module is the thin wire layer:
//! it resolves `bennu_data_dir()`, drives [`SpellEngine`], and downloads the dictionaries.
//!
//! Dictionaries load from `<data>/dictionaries/{en_US,it_IT}.{aff,dic}`. When none are
//! installed, `bennu_spellcheck` returns an empty list (never an error), so the FE degrades
//! gracefully and can prompt the user to `bennu_download_dictionaries`.
//!
//! The download uses the workspace HTTP client (`arbor_core::prelude::client()`, a
//! pre-configured `reqwest::Client`) — no new network crate — pulling the LibreOffice
//! dictionary raw files and emitting `arbor://bennu/dict-progress` events as each file
//! lands. It runs as an `async` handler on the backend runtime, so it never stalls the
//! dispatcher.

use std::path::PathBuf;

use arbor_core::prelude::bennu_data_dir;
use bennu_core::prelude::BennuState;
use bennu_intel::prelude::{
    global_custom_dict_path, installed_languages, project_custom_dict_path, SpellEngine,
    SpellHit as IntelSpellHit,
};
use bennu_proto::prelude::{SpellHit, SpellStatus};
use serde::Deserialize;
use serde_json::json;

/// Event topic: one dictionary file finished downloading (or the language finished).
const EVT_DICT_PROGRESS: &str = "arbor://bennu/dict-progress";

/// The LibreOffice dictionaries repo raw files, one `(lang, file, url)` per download.
const DICT_SOURCES: &[(&str, &str, &str)] = &[
    (
        "en_US",
        "en_US.aff",
        "https://raw.githubusercontent.com/LibreOffice/dictionaries/master/en/en_US.aff",
    ),
    (
        "en_US",
        "en_US.dic",
        "https://raw.githubusercontent.com/LibreOffice/dictionaries/master/en/en_US.dic",
    ),
    (
        "it_IT",
        "it_IT.aff",
        "https://raw.githubusercontent.com/LibreOffice/dictionaries/master/it_IT/it_IT.aff",
    ),
    (
        "it_IT",
        "it_IT.dic",
        "https://raw.githubusercontent.com/LibreOffice/dictionaries/master/it_IT/it_IT.dic",
    ),
];

// ── bennu_spellcheck ─────────────────────────────────────────────────────────────

/// Args for [`bennu_spellcheck`].
#[derive(Deserialize)]
pub struct SpellcheckArgs {
    /// Absolute path (forward slashes) of the file being checked. Non-Java files return an
    /// empty list for now (the walk is tree-sitter-java).
    pub file: String,
    /// The current (possibly-unsaved) buffer text to spell-check.
    pub source: String,
}

/// Spell-check `source` (a Java buffer): tokenize declaration names + comment words and
/// return one [`SpellHit`] per misspelled sub-word (UTF-8 byte spans into `source`). Empty
/// when no dictionary is installed or the file isn't `.java` — never an error.
#[arbor_rpc::handler]
fn bennu_spellcheck(_ctx: &BennuState, args: SpellcheckArgs) -> Result<Vec<SpellHit>, String> {
    if !is_java(&args.file) {
        return Ok(Vec::new()); // non-Java: not yet supported (note in the summary)
    }
    // Merge the per-project custom dict for the file's project (if any) on top of the
    // global one, so a project-scoped word is honoured. The global set is loaded lazily by
    // the engine; the project words are added here.
    let engine = SpellEngine::new(bennu_data_dir());
    if let Some(root) = project_root_of(&args.file) {
        engine.add_custom_words(read_project_words(&root));
    }
    let hits = engine.spellcheck_source(&args.source);
    Ok(hits.into_iter().map(spell_hit_of).collect())
}

/// Map the intel [`IntelSpellHit`] onto the wire [`SpellHit`] (field-for-field).
fn spell_hit_of(h: IntelSpellHit) -> SpellHit {
    SpellHit { start: h.start, end: h.end, word: h.word, suggestions: h.suggestions }
}

// ── bennu_dict_add ───────────────────────────────────────────────────────────────

/// Args for [`bennu_dict_add`].
#[derive(Deserialize)]
pub struct DictAddArgs {
    /// The word to add to the custom dictionary.
    pub word: String,
    /// `"project"` (per-project `<root>/.arbor/bennu-dict.txt`) or `"global"`
    /// (`<data>/custom-dict.txt`).
    pub scope: String,
    /// The project root (used for `scope == "project"`; ignored for `"global"`).
    pub root: String,
}

/// Append `word` to the matching custom dictionary (dedup, creating parent dirs on demand),
/// then refresh the in-memory custom set so subsequent `bennu_spellcheck` calls honour it.
#[arbor_rpc::handler]
fn bennu_dict_add(_ctx: &BennuState, args: DictAddArgs) -> Result<(), String> {
    let word = args.word.trim();
    if word.is_empty() {
        return Err("word is empty".to_string());
    }
    let path = match args.scope.as_str() {
        "global" => global_custom_dict_path(&bennu_data_dir()),
        "project" => {
            if args.root.trim().is_empty() {
                return Err("scope 'project' requires a root".to_string());
            }
            project_custom_dict_path(&PathBuf::from(&args.root))
        }
        other => return Err(format!("unknown scope '{other}' (expected 'project' | 'global')")),
    };
    append_word_deduped(&path, word)?;

    // Reflect it immediately in the loaded engine (no full reload needed for one word).
    SpellEngine::new(bennu_data_dir()).add_custom_words(std::iter::once(word.to_string()));
    Ok(())
}

/// Append `word` to `path` (one word per line, UTF-8), creating parent dirs and the file on
/// demand. A no-op when `word` is already present (case-insensitive dedup).
fn append_word_deduped(path: &std::path::Path, word: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create dict dir: {e}"))?;
    }
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    let lower = word.to_ascii_lowercase();
    if existing.lines().any(|l| l.trim().to_ascii_lowercase() == lower) {
        return Ok(()); // already present
    }
    let mut content = existing;
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(word);
    content.push('\n');
    std::fs::write(path, content).map_err(|e| format!("write dict: {e}"))
}

// ── bennu_spell_status ───────────────────────────────────────────────────────────

/// Args for [`bennu_spell_status`] — none (an empty struct so the wire shape is a bare
/// object).
#[derive(Deserialize)]
pub struct SpellStatusArgs {}

/// Report whether any dictionary is installed + which languages are on disk. A cheap
/// file-existence check (does not load the dictionaries).
#[arbor_rpc::handler]
fn bennu_spell_status(_ctx: &BennuState, _args: SpellStatusArgs) -> Result<SpellStatus, String> {
    Ok(current_status())
}

/// The current dictionary status from disk.
fn current_status() -> SpellStatus {
    let languages = installed_languages(&bennu_data_dir());
    SpellStatus { installed: !languages.is_empty(), languages }
}

// ── bennu_download_dictionaries ──────────────────────────────────────────────────

/// Args for [`bennu_download_dictionaries`] — none.
#[derive(Deserialize)]
pub struct DownloadDictionariesArgs {}

/// Download `en_US` + `it_IT` Hunspell (`.aff` + `.dic`) into `<data>/dictionaries/` using
/// the workspace HTTP client, emitting `arbor://bennu/dict-progress` as each file lands,
/// then reload the engine cache and return the fresh [`SpellStatus`]. Runs on the backend
/// runtime (async handler) so it never stalls the dispatcher.
#[arbor_rpc::handler]
async fn bennu_download_dictionaries(
    ctx: &BennuState,
    _args: DownloadDictionariesArgs,
) -> Result<SpellStatus, String> {
    let dict_dir = bennu_data_dir().join("dictionaries");
    if let Err(e) = tokio::fs::create_dir_all(&dict_dir).await {
        return Err(format!("create dictionaries dir: {e}"));
    }

    let client = arbor_core::prelude::client();
    let sink = ctx.event_sink();
    let mut any_ok = false;

    for (lang, file, url) in DICT_SOURCES {
        match download_file(&client, url).await {
            Ok(bytes) => {
                let path = dict_dir.join(file);
                if let Err(e) = tokio::fs::write(&path, &bytes).await {
                    eprintln!("bennu-be: write {file}: {e}");
                    sink.emit(EVT_DICT_PROGRESS, json!({ "lang": lang, "file": file, "done": false }));
                    continue;
                }
                any_ok = true;
                sink.emit(EVT_DICT_PROGRESS, json!({ "lang": lang, "file": file, "done": true }));
            }
            Err(e) => {
                eprintln!("bennu-be: download {url}: {e}");
                sink.emit(EVT_DICT_PROGRESS, json!({ "lang": lang, "file": file, "done": false }));
            }
        }
    }

    // Reload the engine so the next spellcheck reflects the new files. Parsing the dicts is
    // CPU-heavy → do it off the async runtime on the blocking pool.
    if any_ok {
        let _ = tokio::task::spawn_blocking(|| {
            SpellEngine::new(bennu_data_dir()).reload();
        })
        .await;
    }

    Ok(current_status())
}

/// GET `url` and return the body bytes. Error string on any transport / status failure — a
/// download failure is per-file non-fatal to the overall command (logged + a `done:false`
/// progress event), so this returns the error to the caller loop rather than aborting.
async fn download_file(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, String> {
    let resp = client.get(url).send().await.map_err(|e| format!("request: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("status {}", resp.status()));
    }
    resp.bytes().await.map(|b| b.to_vec()).map_err(|e| format!("body: {e}"))
}

// ── helpers ──────────────────────────────────────────────────────────────────────

/// Whether `file` is a `.java` source (the only kind the tree-sitter walk handles).
fn is_java(file: &str) -> bool {
    std::path::Path::new(file).extension().and_then(|e| e.to_str()) == Some("java")
}

/// The project root owning `file`: the nearest ancestor directory containing an `.arbor`
/// folder or a `pom.xml` (best-effort; `None` when none is found). Used only to locate the
/// per-project custom dict.
fn project_root_of(file: &str) -> Option<PathBuf> {
    let mut dir = PathBuf::from(file);
    dir.pop();
    loop {
        if dir.join(".arbor").is_dir() || dir.join("pom.xml").is_file() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Read the per-project custom words (lowercasing is handled by the engine merge).
fn read_project_words(root: &std::path::Path) -> Vec<String> {
    let path = project_custom_dict_path(root);
    let Ok(text) = std::fs::read_to_string(path) else { return Vec::new() };
    text.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).map(|l| l.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_java_only_matches_java() {
        assert!(is_java("C:/p/src/Foo.java"));
        assert!(!is_java("C:/p/src/Foo.xml"));
        assert!(!is_java("C:/p/README"));
    }

    #[test]
    fn append_word_dedups_case_insensitively() {
        let dir = std::env::temp_dir().join(format!("bennu-dict-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("d.txt");
        let _ = std::fs::remove_file(&path);
        append_word_deduped(&path, "Widget").unwrap();
        append_word_deduped(&path, "widget").unwrap(); // dup (case-insensitive)
        append_word_deduped(&path, "gadget").unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
        assert_eq!(lines, vec!["Widget", "gadget"]);
        let _ = std::fs::remove_file(&path);
    }
}
