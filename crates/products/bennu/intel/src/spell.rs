//! Spell-check engine (docs: "editor niceties") — the pure analysis behind
//! `bennu_spellcheck`, spell-checking the words the user *authored*: **declaration-name
//! identifiers** (class / interface / enum / method / field / variable / parameter names)
//! and **comment** text. References, type-uses and string literals are deliberately NOT
//! checked, to keep the noise low.
//!
//! A sub-word is "correct" when it is in EN (`en_US`) OR IT (`it_IT`) Hunspell OR the
//! built-in [`TECH_ALLOWLIST`] of programming abbreviations OR the user's custom
//! dictionary (global + per-project). Everything is case-insensitive.
//!
//! ## Layers
//! - [`tokenize_identifier`] / [`is_allowed`] — the PURE tokenizer + allow-list check
//!   (unit-tested, no I/O).
//! - [`SpellEngine`] — the process-wide, lazily-loaded Hunspell dictionaries (cached in a
//!   `OnceLock<RwLock<..>>`) plus the merged custom set. Missing dict files just make that
//!   language unavailable — never a crash.
//! - [`spellcheck_source`] — parse a `.java` source with tree-sitter, walk it, and return
//!   one [`SpellHit`] per misspelled sub-word (declaration names) / word (comments), each
//!   as a byte span into `source`.
//!
//! The engine loads dictionaries from `<data>/dictionaries/{en_US,it_IT}.{aff,dic}` where
//! `<data>` is supplied by the be layer (`bennu_data_dir()`), so this crate stays free of
//! the path resolver.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

use spellbook::Dictionary;
use tree_sitter::{Node, Parser};

/// Max suggestions computed per misspelled word.
const MAX_SUGGESTIONS: usize = 5;

/// Sub-words shorter than this are skipped (too short to be a meaningful misspelling).
const MIN_WORD_LEN: usize = 3;

/// One misspelled word occurrence: a byte span into the analysed source plus the word and
/// up to [`MAX_SUGGESTIONS`] corrections. The be layer maps this 1:1 onto the wire `SpellHit`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpellHit {
    /// Start byte offset of the sub-word within the source.
    pub start: usize,
    /// End byte offset (exclusive).
    pub end: usize,
    /// The offending sub-word (as it appears in the source).
    pub word: String,
    /// Up to [`MAX_SUGGESTIONS`] suggested corrections (empty when none / unavailable).
    pub suggestions: Vec<String>,
}

// ── built-in tech allow-list ────────────────────────────────────────────────────────

/// Common programming abbreviations that must NOT be flagged (case-insensitive). Curated
/// from the abbreviations that recur in legacy Java / Struts / Spring / JDBC code, so a
/// method named `getDto` or a field `ctx` never trips the checker.
pub static TECH_ALLOWLIST: &[&str] = &[
    // identifiers / accessors
    "idx", "ctx", "req", "res", "resp", "btn", "arg", "args", "param", "params", "cfg",
    "config", "init", "env", "repo", "repos", "svc", "dir", "dirs", "src", "dest", "tmp",
    "temp", "buf", "ptr", "len", "num", "str", "msg", "err", "ok", "fn", "obj", "val",
    "vals", "arr", "kv", "kvs", "fmt", "calc", "ref", "refs", "impl", "util", "utils",
    "id", "ids", "iter", "iterator", "prev", "curr", "cur", "acc", "attr", "attrs", "elem",
    "elems", "func", "expr", "stmt", "cmd", "opt", "opts", "prop", "props", "ctor", "dtor",
    "min", "max", "avg", "sum", "cnt", "count",
    // async / concurrency
    "async", "await", "sync", "mutex", "atomic", "thread", "threads",
    // web / net
    "http", "https", "url", "uri", "urls", "uris", "api", "apis", "json", "xml", "html",
    "css", "js", "dom", "sse", "ws", "cors", "csrf", "http2",
    // persistence
    "sql", "db", "jdbc", "dto", "vo", "bo", "pojo", "dao", "orm", "jpa", "crud", "mybatis",
    "hibernate", "sqlite",
    // platform / runtime
    "jvm", "jre", "jdk", "sdk", "cli", "gui", "ui", "ux", "os", "io", "nio", "vm",
    // security / auth
    "auth", "oauth", "jwt", "uuid", "guid", "regex", "mvc", "dsl", "ast", "cst", "hmac",
    "sha", "md5", "tls", "ssl", "acl", "rbac",
    // struts / spring / entando stack
    "struts", "ognl", "xwork", "taglib", "tld", "jsp", "servlet", "spring", "beans", "bean",
    "entando", "japs", "tiles", "webapp",
    // filler / misc
    "etc", "lorem", "ipsum", "todo", "fixme", "impl", "iface", "enum", "boolean", "bool",
    "char", "int", "long", "byte",
];

/// Whether `word` (case-insensitive) is in the built-in tech allow-list.
pub fn is_tech_allowed(word: &str) -> bool {
    let lower = word.to_ascii_lowercase();
    TECH_ALLOWLIST.iter().any(|&w| w == lower)
}

// ── tokenizer (re-exported) ─────────────────────────────────────────────────────────

/// Splitting an identifier into its words lives in [`bennu_naming`], which needs exactly the same
/// split to decide whether a name is camelCase and to render the one that would be. Two copies
/// would have drifted on the first acronym anybody disagreed about — and drift there means the
/// spell-checker and the naming check underlining different halves of the same identifier.
///
/// Re-exported rather than reached through the other crate at every call site: these are `spell`'s
/// vocabulary too, and `bennu_intel::prelude` has always published them.
pub use bennu_naming::prelude::{tokenize_identifier, SubWord};

/// Whether a sub-word is *skippable* without ever consulting a dictionary: too short,
/// all-caps acronym, or contains a digit. (The allow-list / dict membership is checked
/// separately by the engine.)
pub fn is_trivially_skippable(word: &str) -> bool {
    if word.chars().count() < MIN_WORD_LEN {
        return true;
    }
    if word.chars().any(|c| c.is_numeric()) {
        return true;
    }
    // ALL-CAPS acronym (every cased char is uppercase, and there is at least one letter).
    let mut has_letter = false;
    for c in word.chars() {
        if c.is_alphabetic() {
            has_letter = true;
            if c.is_lowercase() {
                return false;
            }
        }
    }
    has_letter
}

// ── the process-wide engine ──────────────────────────────────────────────────────────

/// The loaded dictionaries + the merged custom set, cached process-wide.
struct Loaded {
    /// `en_US`, if its `.aff`/`.dic` were present + parsed.
    en: Option<Dictionary>,
    /// `it_IT`, if its `.aff`/`.dic` were present + parsed.
    it: Option<Dictionary>,
    /// The merged custom set (global + per-project), lowercased.
    custom: HashSet<String>,
    /// The data dir the dictionaries were loaded from (to reload from the same place).
    data_dir: PathBuf,
}

static ENGINE: OnceLock<RwLock<Option<Loaded>>> = OnceLock::new();

fn cell() -> &'static RwLock<Option<Loaded>> {
    ENGINE.get_or_init(|| RwLock::new(None))
}

/// The spell engine: a thin handle over the process-wide dictionary cache. Cheap to
/// construct — the heavy load happens once, lazily, on first use (or on [`reload`]).
pub struct SpellEngine {
    data_dir: PathBuf,
}

impl SpellEngine {
    /// A handle bound to `data_dir` (the be layer passes `bennu_data_dir()`).
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self { data_dir: data_dir.into() }
    }

    /// Ensure the dictionaries are loaded (lazy, once). Reloads if the cached load was for
    /// a different data dir. Never errors: a missing dict just leaves that language `None`.
    fn ensure_loaded(&self) {
        {
            let g = cell().read().unwrap_or_else(|p| p.into_inner());
            if g.as_ref().map(|l| l.data_dir == self.data_dir).unwrap_or(false) {
                return;
            }
        }
        let loaded = load_from(&self.data_dir);
        *cell().write().unwrap_or_else(|p| p.into_inner()) = Some(loaded);
    }

    /// Force a reload of the Hunspell dictionaries + custom set from disk (called after a
    /// successful dictionary download, so the next check reflects the new files).
    pub fn reload(&self) {
        let loaded = load_from(&self.data_dir);
        *cell().write().unwrap_or_else(|p| p.into_inner()) = Some(loaded);
    }

    /// Merge additional custom words (lowercased) into the in-memory set, so a just-added
    /// word is honoured by subsequent checks without a full reload. No-op when unloaded.
    pub fn add_custom_words<I: IntoIterator<Item = String>>(&self, words: I) {
        self.ensure_loaded();
        let mut g = cell().write().unwrap_or_else(|p| p.into_inner());
        if let Some(loaded) = g.as_mut() {
            for w in words {
                loaded.custom.insert(w.to_ascii_lowercase());
            }
        }
    }

    /// Whether at least one Hunspell dictionary is loaded (so a spellcheck can find hits).
    pub fn any_dictionary(&self) -> bool {
        self.ensure_loaded();
        let g = cell().read().unwrap_or_else(|p| p.into_inner());
        g.as_ref().map(|l| l.en.is_some() || l.it.is_some()).unwrap_or(false)
    }

    /// Whether `word` is considered correct: allow-list OR custom OR EN OR IT (all
    /// case-insensitive). The dictionaries are Hunspell, which are already case-aware; we
    /// also try the lowercased form so a leading-cap sub-word (`Order`) checks against a
    /// lowercase stem.
    fn is_correct(&self, word: &str) -> bool {
        if is_tech_allowed(word) {
            return true;
        }
        let lower = word.to_ascii_lowercase();
        let g = cell().read().unwrap_or_else(|p| p.into_inner());
        let Some(loaded) = g.as_ref() else { return true }; // unloaded → never flag
        if loaded.custom.contains(&lower) {
            return true;
        }
        for dict in [loaded.en.as_ref(), loaded.it.as_ref()].into_iter().flatten() {
            if dict.check(word) || dict.check(&lower) {
                return true;
            }
        }
        false
    }

    /// Up to [`MAX_SUGGESTIONS`] corrections for `word` (from EN first, then IT). Empty
    /// when no dictionary can suggest.
    fn suggest(&self, word: &str) -> Vec<String> {
        let g = cell().read().unwrap_or_else(|p| p.into_inner());
        let Some(loaded) = g.as_ref() else { return Vec::new() };
        let mut out = Vec::new();
        for dict in [loaded.en.as_ref(), loaded.it.as_ref()].into_iter().flatten() {
            let mut sug = Vec::new();
            dict.suggest(word, &mut sug);
            for s in sug {
                if out.len() >= MAX_SUGGESTIONS {
                    break;
                }
                if !out.contains(&s) {
                    out.push(s);
                }
            }
            if out.len() >= MAX_SUGGESTIONS {
                break;
            }
        }
        out
    }

    /// Spell-check a `.java` source: parse it with tree-sitter, walk declaration-name
    /// identifiers + comments, and return one [`SpellHit`] per misspelled word (byte spans
    /// into `source`). Returns an empty vec when no dictionary is installed (never errors).
    pub fn spellcheck_source(&self, source: &str) -> Vec<SpellHit> {
        self.ensure_loaded();
        if !self.any_dictionary() {
            return Vec::new();
        }
        let mut parser = Parser::new();
        if parser.set_language(&tree_sitter_java::LANGUAGE.into()).is_err() {
            return Vec::new();
        }
        let Some(tree) = parser.parse(source, None) else { return Vec::new() };
        let bytes = source.as_bytes();
        let mut out = Vec::new();

        let mut stack = vec![tree.root_node()];
        while let Some(n) = stack.pop() {
            let mut cur = n.walk();
            for c in n.named_children(&mut cur) {
                stack.push(c);
            }
            match n.kind() {
                "line_comment" | "block_comment" => self.check_comment(&n, bytes, &mut out),
                "identifier" | "type_identifier" => {
                    if is_declaration_name(&n) {
                        self.check_identifier(&n, bytes, &mut out);
                    }
                }
                _ => {}
            }
        }
        out
    }

    /// Tokenize a declaration-name identifier and push a hit per misspelled sub-word (byte
    /// span within the whole source).
    fn check_identifier(&self, node: &Node, bytes: &[u8], out: &mut Vec<SpellHit>) {
        let Ok(text) = node.utf8_text(bytes) else { return };
        let base = node.start_byte();
        for sw in tokenize_identifier(text) {
            if is_trivially_skippable(&sw.text) || self.is_correct(&sw.text) {
                continue;
            }
            out.push(SpellHit {
                start: base + sw.start,
                end: base + sw.end,
                suggestions: self.suggest(&sw.text),
                word: sw.text,
            });
        }
    }

    /// Split a comment's text into letter-only words and push a hit per misspelled word.
    /// Byte spans are computed by scanning the comment text (so offsets land inside the
    /// original source).
    fn check_comment(&self, node: &Node, bytes: &[u8], out: &mut Vec<SpellHit>) {
        let Ok(text) = node.utf8_text(bytes) else { return };
        let base = node.start_byte();
        for word in comment_words(text) {
            if is_trivially_skippable(&word.text) || self.is_correct(&word.text) {
                continue;
            }
            out.push(SpellHit {
                start: base + word.start,
                end: base + word.end,
                suggestions: self.suggest(&word.text),
                word: word.text,
            });
        }
    }
}

/// One span of `text` as a [`SubWord`]. Prose, not identifiers: the identifier split lives in
/// `bennu-naming` (see the re-export above) and comments do not want it — `getUserName` written in
/// a sentence is one word to a dictionary.
fn sub_word(text: &str, start: usize, end: usize) -> SubWord {
    SubWord { text: text[start..end].to_string(), start, end }
}

/// Split a comment into letter-run words with byte spans (relative to the comment start).
/// A "word" is a maximal run of alphabetic chars; comment markers (`//`, `/*`, `*`) and
/// punctuation split words and are dropped.
fn comment_words(text: &str) -> Vec<SubWord> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for (off, c) in text.char_indices() {
        if c.is_alphabetic() {
            if start.is_none() {
                start = Some(off);
            }
        } else if let Some(s) = start.take() {
            out.push(sub_word(text, s, off));
        }
    }
    if let Some(s) = start.take() {
        out.push(sub_word(text, s, text.len()));
    }
    // A comment word may itself be camelCase-ish (rare in prose); keep it whole — comments
    // are natural language, so we check the run as-is.
    out
}

/// Whether `node` is the NAME of a declaration (class / interface / enum / method / field /
/// local variable / parameter). Mirrors the recognition style in `refs.rs`'s
/// `is_declaration_name` / `decl_name_key`, extended to the authored-name kinds we
/// spell-check (fields, locals, params).
fn is_declaration_name(node: &Node) -> bool {
    let Some(parent) = node.parent() else { return false };
    let is_name_field = || {
        parent.child_by_field_name("name").map(|nm| nm.id() == node.id()).unwrap_or(false)
    };
    match parent.kind() {
        // type declarations: the `name` field.
        "class_declaration" | "interface_declaration" | "enum_declaration"
        | "annotation_type_declaration" | "record_declaration" => is_name_field(),
        // method / constructor: the `name` field.
        "method_declaration" | "constructor_declaration" => is_name_field(),
        // field + local declarations declare via a `variable_declarator` whose `name` is
        // the identifier (both `field_declaration` and `local_variable_declaration` wrap
        // one or more declarators).
        "variable_declarator" => is_name_field(),
        // method / constructor / lambda / catch parameters.
        "formal_parameter" | "spread_parameter" | "catch_formal_parameter"
        | "inferred_parameter" => is_name_field(),
        // enum constant names.
        "enum_constant" => is_name_field(),
        _ => false,
    }
}

// ── dictionary loading (I/O) ─────────────────────────────────────────────────────────

/// Load the Hunspell dictionaries + custom set from `data_dir`. Never panics: a missing /
/// malformed dict file leaves that language `None`; a missing custom file is an empty set.
fn load_from(data_dir: &Path) -> Loaded {
    let dict_dir = data_dir.join("dictionaries");
    let en = load_lang(&dict_dir, "en_US");
    let it = load_lang(&dict_dir, "it_IT");
    let custom = load_custom(data_dir, None);
    Loaded { en, it, custom, data_dir: data_dir.to_path_buf() }
}

/// Load one language's `<lang>.aff` + `<lang>.dic` from `dict_dir`. `None` when either file
/// is missing or fails to parse.
fn load_lang(dict_dir: &Path, lang: &str) -> Option<Dictionary> {
    let aff = std::fs::read_to_string(dict_dir.join(format!("{lang}.aff"))).ok()?;
    let dic = std::fs::read_to_string(dict_dir.join(format!("{lang}.dic"))).ok()?;
    match Dictionary::new(&aff, &dic) {
        Ok(d) => Some(d),
        Err(e) => {
            eprintln!("bennu-intel: spell dict {lang} parse failed: {e}");
            None
        }
    }
}

/// The global custom-dict path: `<data>/custom-dict.txt`.
pub fn global_custom_dict_path(data_dir: &Path) -> PathBuf {
    data_dir.join("custom-dict.txt")
}

/// The per-project custom-dict path: `<root>/.arbor/bennu-dict.txt`.
pub fn project_custom_dict_path(root: &Path) -> PathBuf {
    root.join(".arbor").join("bennu-dict.txt")
}

/// Load + merge the custom words: the global file, plus the per-project file when `root`
/// is given. Lowercased. Missing files → nothing added.
fn load_custom(data_dir: &Path, root: Option<&Path>) -> HashSet<String> {
    let mut set = HashSet::new();
    read_words_into(&global_custom_dict_path(data_dir), &mut set);
    if let Some(root) = root {
        read_words_into(&project_custom_dict_path(root), &mut set);
    }
    set
}

/// Append each non-empty, lowercased line of `path` (one word per line) into `set`.
fn read_words_into(path: &Path, set: &mut HashSet<String>) {
    let Ok(text) = std::fs::read_to_string(path) else { return };
    for line in text.lines() {
        let w = line.trim();
        if !w.is_empty() {
            set.insert(w.to_ascii_lowercase());
        }
    }
}

/// The languages present on disk under `<data>/dictionaries` (any of `en_US` / `it_IT`
/// with both `.aff` + `.dic`). Cheap file-existence check — does not load the dictionaries.
pub fn installed_languages(data_dir: &Path) -> Vec<String> {
    let dict_dir = data_dir.join("dictionaries");
    let mut out = Vec::new();
    for lang in ["en_US", "it_IT"] {
        if dict_dir.join(format!("{lang}.aff")).is_file()
            && dict_dir.join(format!("{lang}.dic")).is_file()
        {
            out.push(lang.to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(ws: &[SubWord]) -> Vec<&str> {
        ws.iter().map(|w| w.text.as_str()).collect()
    }

    #[test]
    fn splits_camel_case() {
        assert_eq!(texts(&tokenize_identifier("getUserName")), vec!["get", "User", "Name"]);
    }

    #[test]
    fn splits_snake_and_kebab() {
        assert_eq!(texts(&tokenize_identifier("user_name")), vec!["user", "name"]);
        assert_eq!(texts(&tokenize_identifier("user-name")), vec!["user", "name"]);
    }

    #[test]
    fn splits_acronym_run() {
        assert_eq!(texts(&tokenize_identifier("parseXMLFile")), vec!["parse", "XML", "File"]);
        assert_eq!(texts(&tokenize_identifier("HTTPServer")), vec!["HTTP", "Server"]);
    }

    #[test]
    fn splits_digit_runs() {
        assert_eq!(texts(&tokenize_identifier("md5Hash")), vec!["md", "5", "Hash"]);
        assert_eq!(texts(&tokenize_identifier("utf8Encoder")), vec!["utf", "8", "Encoder"]);
    }

    #[test]
    fn subword_spans_are_correct() {
        let ws = tokenize_identifier("getUser");
        assert_eq!(ws[0], SubWord { text: "get".into(), start: 0, end: 3 });
        assert_eq!(ws[1], SubWord { text: "User".into(), start: 3, end: 7 });
    }

    #[test]
    fn trivially_skippable_rules() {
        assert!(is_trivially_skippable("io")); // len < 3
        assert!(is_trivially_skippable("XML")); // all-caps acronym
        assert!(is_trivially_skippable("md5")); // has digit
        assert!(!is_trivially_skippable("User")); // normal cased word
        assert!(!is_trivially_skippable("name"));
    }

    #[test]
    fn allowlist_skips_common_abbrevs() {
        assert!(is_tech_allowed("ctx"));
        assert!(is_tech_allowed("DTO")); // case-insensitive
        assert!(is_tech_allowed("repo"));
        assert!(!is_tech_allowed("frobnicate"));
    }

    #[test]
    fn comment_words_split_on_punctuation() {
        let ws = comment_words("// TODO: refactor the widget");
        assert_eq!(texts(&ws), vec!["TODO", "refactor", "the", "widget"]);
    }

    #[test]
    fn correct_vs_misspelled_with_inmemory_dict() {
        // A tiny in-memory Hunspell dict (no affix rules): en_US-style header + two stems.
        let aff = "SET UTF-8\n";
        let dic = "2\nhello\nworld\n";
        let dict = Dictionary::new(aff, dic).expect("tiny dict");
        assert!(dict.check("hello"));
        assert!(!dict.check("helllo"));
    }
}
