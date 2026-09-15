//! Completion at a caret in a `Cargo.toml`.
//!
//! ## Where the candidates come from
//!
//! Two sources, and neither is a hard-coded list of crate names:
//!
//! - **the schema** ([`crate::schema`]) for everything structural — table headers, the keys of a
//!   table, the values of a closed set. The same table the validator reads, so a key that
//!   completes can never be a key that then flags itself;
//! - **a [`Catalog`]** for the crate names and versions, read off the machine: this workspace's own
//!   members, its `[workspace.dependencies]`, `Cargo.lock`, and the crates already downloaded into
//!   the local registry. No network, ever — a completion popup that waits on crates.io is a
//!   completion popup that appears after you have finished typing.
//!
//! ## The caret decides everything
//!
//! There are seven places a caret can be in a manifest and they want seven different answers:
//!
//! ```toml
//! [dep|                                  # a table header
//! [package]
//! edi|                                   # a key of this table
//! edition.|                              # the dotted suffix of a key
//! edition = "|"                          # the value of a key
//! [dependencies]
//! ser|                                   # a crate name
//! serde = { fea| }                       # a key of a dependency spec
//! serde = { features = ["de|"] }         # an item of an array
//! ```
//!
//! Getting that classification right is most of this module ([`spot_at`]); once the spot is known,
//! the candidates are a lookup.

use std::path::{Path, PathBuf};

use bennu_proto::prelude::CompletionItem;

use crate::deps::declared;
use crate::manifest::Manifest;
use crate::schema::{self, Openness, ValueKind};

/// How many candidates are ever returned. A completion list is read, not scrolled; past a few
/// dozen entries the popup is a wall and the useful answer is to type another character.
const MAX_ITEMS: usize = 200;

/// The crate names and versions this machine knows about, with no network.
///
/// Built by [`Catalog::read`] and worth caching by the caller: the registry directory on a
/// developer machine holds thousands of entries, and listing it on every keystroke would be the
/// one thing that makes completion feel slow.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Catalog {
    /// Crate names that can be offered, deduplicated. Ordered by how likely they are to be what
    /// you want: workspace members and `[workspace.dependencies]` first, then everything the
    /// registry cache has.
    pub crates: Vec<String>,
    /// `name → version`, from `Cargo.lock` and the registry cache. Several versions of one crate
    /// are several entries.
    pub versions: Vec<(String, String)>,
}

impl Catalog {
    /// Read what the machine knows, starting from the workspace root `root`.
    ///
    /// Three sources, cheapest first: the root manifest's `[workspace.dependencies]`, the
    /// workspace's `Cargo.lock`, and the local registry cache. Each is best-effort — a workspace
    /// that has never been built still gets its own members and its declared versions.
    pub fn read(root: &Path) -> Catalog {
        let mut cat = Catalog::default();

        if let Ok(text) = std::fs::read_to_string(root.join("Cargo.toml")) {
            let m = Manifest::parse(&text);
            for d in declared(&m) {
                cat.push_crate(&d.package);
                if !d.req.is_empty() {
                    cat.push_version(&d.package, &d.req);
                }
            }
            // The workspace's own members are names a `path` dependency will want.
            for item in m.items_of("workspace", "members") {
                if let Some(name) = item.text.rsplit('/').next() {
                    if !name.is_empty() && name != "*" {
                        cat.push_crate(name);
                    }
                }
            }
        }
        cat.read_lockfile(root);
        cat.read_registry();
        cat
    }

    /// `name = "…"` / `version = "…"` pairs out of `Cargo.lock`.
    ///
    /// The lockfile is the best source there is: it holds the exact version of everything in the
    /// graph, including what came in transitively, which is precisely the set of names a manifest
    /// is likely to gain a direct dependency on next.
    fn read_lockfile(&mut self, root: &Path) {
        let Ok(text) = std::fs::read_to_string(root.join("Cargo.lock")) else { return };
        let m = Manifest::parse(&text);
        for element in m.array_elements("package") {
            let name = element.iter().find(|e| e.key == "name").and_then(|e| e.str_value());
            let version = element.iter().find(|e| e.key == "version").and_then(|e| e.str_value());
            if let Some(name) = name {
                self.push_crate(name);
                if let Some(v) = version {
                    self.push_version(name, v);
                }
            }
        }
    }

    /// Every `<name>-<version>.crate` in the local registry cache.
    ///
    /// This is the offline answer to "what versions of `serde` are there": whatever has ever been
    /// downloaded on this machine. Incomplete by nature — it knows nothing about a crate never
    /// used here — and that is the honest trade for never touching the network.
    fn read_registry(&mut self) {
        for dir in crate::home::registry_dirs("cache") {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for entry in entries.flatten() {
                let name = entry.file_name();
                let Some(file) = name.to_str() else { continue };
                let Some(stem) = file.strip_suffix(".crate") else { continue };
                if let Some((crate_name, version)) = split_name_version(stem) {
                    self.push_crate(crate_name);
                    self.push_version(crate_name, version);
                }
            }
        }
    }

    fn push_crate(&mut self, name: &str) {
        let name = name.trim();
        if !name.is_empty() && !self.crates.iter().any(|c| c == name) {
            self.crates.push(name.to_string());
        }
    }

    fn push_version(&mut self, name: &str, version: &str) {
        let pair = (name.to_string(), version.to_string());
        if !self.versions.contains(&pair) {
            self.versions.push(pair);
        }
    }

    /// The known versions of `name`, newest first.
    ///
    /// Ordered by [`version_key`] and not by string, because a string sort puts `1.9.0` above
    /// `1.10.0` and a pre-release above the release it precedes.
    pub fn versions_of(&self, name: &str) -> Vec<&str> {
        let mut found: Vec<&str> = self
            .versions
            .iter()
            .filter(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
            .collect();
        found.sort_by(|a, b| version_key(b).cmp(&version_key(a)));
        found.dedup();
        found
    }
}

/// A version as a comparable key: major, minor, patch, and whether it is a real release.
///
/// The last component is what makes this more than a split: in semver `1.10.0-beta.1` is *older*
/// than `1.10.0`, and a key built only from the numbers would put the beta on top — offering a
/// pre-release as the newest version of a crate.
pub(crate) fn version_key(v: &str) -> (u64, u64, u64, u8) {
    // Build metadata (`+sha`) is not part of the ordering at all; a pre-release (`-beta`) is.
    let core = v.split('+').next().unwrap_or(v);
    let (core, pre) = core.split_once('-').unwrap_or((core, ""));
    let mut parts = core.split('.').map(|p| p.trim().parse::<u64>().unwrap_or(0));
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        u8::from(pre.is_empty()),
    )
}

/// `serde_json-1.0.108` → `("serde_json", "1.0.108")`.
///
/// Split at the last `-` followed by a digit: a crate name may contain hyphens
/// (`tracing-subscriber-0.3.18`), and splitting at the first one would name the wrong crate.
fn split_name_version(stem: &str) -> Option<(&str, &str)> {
    let bytes = stem.as_bytes();
    let mut at = None;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'-' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit) {
            at = Some(i);
        }
    }
    let i = at?;
    Some((&stem[..i], &stem[i + 1..]))
}

/// What completion knows besides the buffer.
#[derive(Debug, Clone, Default)]
pub struct Context {
    /// The directory holding this manifest — for completing `members` and `path` against real
    /// directories.
    pub dir: Option<PathBuf>,
    /// The crate names and versions available offline.
    pub catalog: Catalog,
}

/// Where the caret is, and what token it is in the middle of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Spot {
    /// Inside a `[header]`.
    Header { prefix: String, from: usize },
    /// Typing a key of `table`.
    Key { table: String, prefix: String, from: usize },
    /// Typing the dotted suffix of the key `base` in `table` (`edition.|`).
    KeySuffix { table: String, base: String, prefix: String, from: usize },
    /// Typing the value of `table.key`.
    Value { table: String, key: String, prefix: String, from: usize, quoted: bool },
    /// Typing an item of the array `table.key`.
    Item { table: String, key: String, prefix: String, from: usize, quoted: bool },
    /// Typing a key inside the inline table `table.key` (`serde = { fea| }`).
    SpecKey { table: String, key: String, prefix: String, from: usize },
    /// Typing the value of `spec` inside the inline table `table.key`.
    SpecValue {
        table: String,
        key: String,
        spec: String,
        prefix: String,
        from: usize,
        quoted: bool,
    },
}

/// Completion candidates for the caret at byte `offset` in `text`.
///
/// Empty when there is nothing to say, which is most positions in most manifests.
pub fn complete(text: &str, offset: usize, ctx: &Context) -> Vec<CompletionItem> {
    let offset = offset.min(text.len());
    let m = Manifest::parse(text);
    let Some(spot) = spot_at(text, offset, &m) else { return Vec::new() };
    let mut out = candidates(&m, &spot, ctx);
    // The replace range is the token the caret is in, both directions: without the forward half,
    // completing in the middle of `edition = "20|21"` would leave a `21` behind.
    let (from, to) = replace_range(text, offset, &spot);
    for item in &mut out {
        item.replace_start = Some(from);
        item.replace_end = Some(to);
    }
    out.truncate(MAX_ITEMS);
    out
}

/// The byte range accepting a candidate replaces.
fn replace_range(text: &str, offset: usize, spot: &Spot) -> (usize, usize) {
    let from = match spot {
        Spot::Header { from, .. }
        | Spot::Key { from, .. }
        | Spot::KeySuffix { from, .. }
        | Spot::Value { from, .. }
        | Spot::Item { from, .. }
        | Spot::SpecKey { from, .. }
        | Spot::SpecValue { from, .. } => *from,
    };
    (from.min(offset), token_end(text, offset))
}

/// How far forward the token under the caret runs.
///
/// Stops at anything that ends a token in this grammar — whitespace, a quote, a bracket, a comma,
/// an `=`. Everything else (including `-`, `_`, `.`, `/`, `:` and `*`) is part of one, because all
/// five appear inside crate names, feature references, paths and version requirements.
fn token_end(text: &str, offset: usize) -> usize {
    let bytes = text.as_bytes();
    let mut i = offset;
    while i < bytes.len() {
        match bytes[i] {
            b' ' | b'\t' | b'\r' | b'\n' | b'"' | b'\'' | b'[' | b']' | b'{' | b'}' | b','
            | b'=' | b'#' => break,
            _ => i += 1,
        }
    }
    i
}

// ── where is the caret ─────────────────────────────────────────────────────────

/// Classify the caret. `None` where nothing completes (a comment, past a closed value).
pub fn spot_at(text: &str, offset: usize, m: &Manifest) -> Option<Spot> {
    let line_start = text[..offset].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let before = &text[line_start..offset];

    // A comment: nothing completes in prose.
    if let Some(hash) = before.find('#') {
        // …unless the `#` is inside a string, which `strip_comment` handles for the parser but
        // this one line has to answer for itself.
        if !in_string(&before[..hash]) {
            return None;
        }
    }

    let trimmed = before.trim_start();
    let indent = before.len() - trimmed.len();

    // A `[header]` still being typed: no `]` yet on this line.
    if let Some(rest) = trimmed.strip_prefix('[') {
        if !rest.contains(']') {
            let rest = rest.strip_prefix('[').unwrap_or(rest);
            let from = line_start + indent + (before.len() - indent - rest.len() - indent.min(0));
            // The prefix is everything after the brackets; recompute `from` from the prefix length
            // so a `[[` opener lands correctly.
            let from = from.max(offset - rest.len());
            return Some(Spot::Header { prefix: rest.to_string(), from });
        }
    }

    // Inside an assignment — the spans know where it starts, so a multi-line array works.
    if let Some(e) = m.entry_at(offset) {
        if offset <= e.key_end {
            let typed = &text[e.key_start..offset];
            return Some(match typed.rsplit_once('.') {
                Some((base, suffix)) => Spot::KeySuffix {
                    table: e.table.clone(),
                    base: base.to_string(),
                    prefix: suffix.to_string(),
                    from: offset - suffix.len(),
                },
                None => Spot::Key {
                    table: e.table.clone(),
                    prefix: typed.to_string(),
                    from: e.key_start,
                },
            });
        }
        if offset < e.value_start {
            // Between the `=` and the value — offer the value with nothing typed.
            return Some(Spot::Value {
                table: e.table.clone(),
                key: e.base_key().to_string(),
                prefix: String::new(),
                from: offset,
                quoted: false,
            });
        }
        let raw = &text[e.value_start..e.value_end.min(text.len())];
        let rel = offset - e.value_start;
        return Some(value_spot(&e.table, e.base_key(), raw, rel, e.value_start));
    }

    // Not an assignment yet: a bare word at the start of a line is a key being typed.
    let word = trailing_word(trimmed);
    // Anything before the word on this line means we are not at a key position (a stray value, a
    // continuation of something unparsed) — say nothing rather than guess.
    if trimmed.len() != word.len() {
        return None;
    }
    let table = m.table_at(offset).to_string();
    // `edition.|` — a dotted key with no `=` yet, which is exactly when the suffix is being
    // typed and is the moment `workspace` is worth offering.
    Some(match word.split_once('.') {
        Some((base, suffix)) => Spot::KeySuffix {
            table,
            base: base.to_string(),
            prefix: suffix.to_string(),
            from: offset - suffix.len(),
        },
        None => Spot::Key { table, prefix: word.to_string(), from: offset - word.len() },
    })
}

/// Whether an odd number of quotes has been opened in `s`.
fn in_string(s: &str) -> bool {
    let mut quote: Option<u8> = None;
    for &b in s.as_bytes() {
        match b {
            b'"' | b'\'' => match quote {
                Some(q) if q == b => quote = None,
                Some(_) => {}
                None => quote = Some(b),
            },
            _ => {}
        }
    }
    quote.is_some()
}

/// The bare word `s` ends with (`ser` from `  ser`), empty when it ends in a separator.
fn trailing_word(s: &str) -> &str {
    let bytes = s.as_bytes();
    let mut i = bytes.len();
    while i > 0 {
        match bytes[i - 1] {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' => i -= 1,
            _ => break,
        }
    }
    &s[i..]
}

/// Classify a caret inside a value. `rel` is the caret's offset within `raw`; `base` is the
/// absolute offset `raw` starts at.
fn value_spot(table: &str, key: &str, raw: &str, rel: usize, base: usize) -> Spot {
    let rel = rel.min(raw.len());
    let head = &raw[..rel];
    let bytes = head.as_bytes();

    // Walk to the caret, tracking the bracket stack, the open quote, and — inside an inline table
    // — which spec key we are past the `=` of.
    let mut stack: Vec<u8> = Vec::new();
    let mut quote: Option<(u8, usize)> = None;
    let mut token_start = 0usize;
    let mut spec_key: Option<String> = None;
    let mut after_eq = false;
    let mut pending = String::new();

    for (i, &b) in bytes.iter().enumerate() {
        if let Some((q, _)) = quote {
            if b == q {
                quote = None;
                token_start = i + 1;
            }
            continue;
        }
        match b {
            b'"' | b'\'' => {
                quote = Some((b, i));
                token_start = i + 1;
            }
            b'[' | b'{' => {
                stack.push(b);
                token_start = i + 1;
                pending.clear();
                if b == b'{' {
                    after_eq = false;
                    spec_key = None;
                }
            }
            b']' | b'}' => {
                stack.pop();
                token_start = i + 1;
                pending.clear();
                after_eq = false;
                spec_key = None;
            }
            b'=' => {
                after_eq = true;
                spec_key = Some(pending.trim().to_string());
                pending.clear();
                token_start = i + 1;
            }
            b',' => {
                after_eq = false;
                spec_key = None;
                pending.clear();
                token_start = i + 1;
            }
            b' ' | b'\t' | b'\r' | b'\n' => {
                if pending.trim().is_empty() {
                    pending.clear();
                    token_start = i + 1;
                }
            }
            _ => pending.push(b as char),
        }
    }

    let quoted = quote.is_some();
    let from = base + if quoted { quote.map(|(_, at)| at + 1).unwrap_or(token_start) } else { token_start };
    let prefix = raw[from - base..rel].to_string();
    let inline = stack.contains(&b'{');
    let in_array = stack.last() == Some(&b'[');

    match (inline, in_array, after_eq) {
        // `serde = { features = ["de|"] }` — an array inside a spec. The spec key is what it is
        // an array *of*, which is the answer the candidates need.
        (true, true, _) => Spot::SpecValue {
            table: table.to_string(),
            key: key.to_string(),
            spec: spec_key.unwrap_or_default(),
            prefix,
            from,
            quoted,
        },
        // `serde = { version = "|" }`
        (true, false, true) => Spot::SpecValue {
            table: table.to_string(),
            key: key.to_string(),
            spec: spec_key.unwrap_or_default(),
            prefix,
            from,
            quoted,
        },
        // `serde = { fea| }`
        (true, false, false) => {
            Spot::SpecKey { table: table.to_string(), key: key.to_string(), prefix, from }
        }
        // `members = ["cra|"]`
        (false, true, _) => {
            Spot::Item { table: table.to_string(), key: key.to_string(), prefix, from, quoted }
        }
        // `edition = "20|"`
        (false, false, _) => {
            Spot::Value { table: table.to_string(), key: key.to_string(), prefix, from, quoted }
        }
    }
}

// ── what to offer ──────────────────────────────────────────────────────────────

fn candidates(m: &Manifest, spot: &Spot, ctx: &Context) -> Vec<CompletionItem> {
    match spot {
        Spot::Header { prefix, .. } => header_items(m, prefix),
        Spot::Key { table, prefix, .. } => key_items(m, table, prefix, ctx),
        Spot::KeySuffix { table, base, prefix, .. } => suffix_items(table, base, prefix),
        Spot::Value { table, key, prefix, quoted, .. } => {
            value_items(table, key, prefix, *quoted, ctx)
        }
        Spot::Item { table, key, prefix, quoted, .. } => {
            item_items(m, table, key, prefix, *quoted, ctx)
        }
        Spot::SpecKey { table, key, prefix, .. } => spec_key_items(m, table, key, prefix),
        Spot::SpecValue { table, key, spec, prefix, quoted, .. } => {
            spec_value_items(table, key, spec, prefix, *quoted, ctx)
        }
    }
}

fn header_items(m: &Manifest, prefix: &str) -> Vec<CompletionItem> {
    schema::HEADER_SUGGESTIONS
        .iter()
        .filter(|(name, _)| matches_prefix(name.trim_start_matches('['), prefix))
        // A table the manifest already has is not a table to add — except an array-of-tables,
        // where a second `[[bin]]` is exactly what you want.
        .filter(|(name, _)| name.starts_with('[') || !m.has_table(name.trim_matches('\'')))
        .map(|(name, doc)| item(name, "table", doc))
        .collect()
}

fn key_items(m: &Manifest, table: &str, prefix: &str, ctx: &Context) -> Vec<CompletionItem> {
    let Some(def) = schema::table_def(table) else { return Vec::new() };
    match def.open {
        Openness::Dependencies => crate_items(m, table, prefix, ctx),
        // `[features]` keys are the user's own names, and `[lints.*]` keys are a tool's — neither
        // is ours to suggest.
        Openness::Free => Vec::new(),
        Openness::Closed => def
            .keys
            .iter()
            .filter(|k| matches_prefix(k.name, prefix))
            // Already set — offering it again is offering a duplicate-key error.
            .filter(|k| m.get_base(table, k.name).is_none())
            .map(|k| item(k.name, "property", k.doc))
            .collect(),
    }
}

/// Crate names for a dependency table: the workspace's own first, then everything the machine has.
fn crate_items(m: &Manifest, table: &str, prefix: &str, ctx: &Context) -> Vec<CompletionItem> {
    let already: Vec<String> = m.entries_in(table).map(|e| e.base_key().to_string()).collect();
    ctx.catalog
        .crates
        .iter()
        .filter(|name| matches_prefix(name, prefix))
        .filter(|name| !already.iter().any(|a| a == *name))
        .map(|name| {
            let newest = ctx.catalog.versions_of(name).first().map(|v| v.to_string());
            let detail = match &newest {
                Some(v) => format!("{v} — from this machine's registry"),
                None => "a crate in this workspace".to_string(),
            };
            let mut it = item(name, "module", &detail);
            // Insert the whole assignment: a crate name on its own is not a dependency, and
            // typing ` = "1"` after every completion is the part nobody wants to do.
            if let Some(v) = newest {
                it.insert_text = Some(format!("{name} = \"{v}\""));
            }
            it
        })
        .collect()
}

/// After a dot in a key: the inheritance marker, or a dependency's spec keys.
fn suffix_items(table: &str, base: &str, prefix: &str) -> Vec<CompletionItem> {
    if schema::is_dependency_table(table) {
        return schema::DEP_KEYS
            .iter()
            .filter(|k| matches_prefix(k.name, prefix))
            .map(|k| item(k.name, "property", k.doc))
            .collect();
    }
    let Some(def) = schema::table_def(table) else { return Vec::new() };
    let Some(key) = def.key(base) else { return Vec::new() };
    if !key.inheritable || !matches_prefix("workspace", prefix) {
        return Vec::new();
    }
    vec![item("workspace", "property", "Inherit this from `[workspace.package]`.")]
}

fn value_items(
    table: &str,
    key: &str,
    prefix: &str,
    quoted: bool,
    ctx: &Context,
) -> Vec<CompletionItem> {
    // A dependency's value is its version requirement.
    if schema::is_dependency_table(table) {
        return version_items(key, prefix, quoted, ctx);
    }
    let Some(def) = schema::table_def(table) else { return Vec::new() };
    let Some(k) = def.key(key) else { return Vec::new() };
    match k.kind {
        ValueKind::Enum(allowed) => allowed
            .iter()
            .filter(|v| matches_prefix(v, prefix))
            .map(|v| quoted_item(v, quoted, "value", "an allowed value"))
            .collect(),
        ValueKind::Bool => ["true", "false"]
            .iter()
            .filter(|v| matches_prefix(v, prefix))
            .map(|v| item(v, "value", "a boolean"))
            .collect(),
        _ => Vec::new(),
    }
}

/// The versions of `name` this machine has, newest first.
fn version_items(name: &str, prefix: &str, quoted: bool, ctx: &Context) -> Vec<CompletionItem> {
    let versions = ctx.catalog.versions_of(name);
    if versions.is_empty() {
        return Vec::new();
    }
    versions
        .iter()
        .filter(|v| matches_prefix(v, prefix))
        .enumerate()
        .map(|(i, v)| {
            let mut it = quoted_item(v, quoted, "value", "a version on this machine");
            // Newest first, and CodeMirror sorts by this string — so pad the index.
            it.sort_text = Some(format!("{i:04}"));
            it
        })
        .collect()
}

/// An item of an array value.
fn item_items(
    m: &Manifest,
    table: &str,
    key: &str,
    prefix: &str,
    quoted: bool,
    ctx: &Context,
) -> Vec<CompletionItem> {
    match (table, key) {
        // A feature's value: other features, `dep:<optional>`, `<dep>/<feature>`.
        ("features", _) => feature_items(m, prefix, quoted),
        ("workspace", "members") | ("workspace", "exclude") | ("workspace", "default-members") => {
            member_items(ctx, prefix, quoted)
        }
        _ => {
            let Some(def) = schema::table_def(table) else { return Vec::new() };
            match def.key(key).map(|k| k.kind) {
                Some(ValueKind::StrArray) if key == "crate-type" => schema::crate_types()
                    .iter()
                    .filter(|v| matches_prefix(v, prefix))
                    .map(|v| quoted_item(v, quoted, "value", "a crate type"))
                    .collect(),
                Some(ValueKind::StrArray) if key == "required-features" => m
                    .entries_in("features")
                    .filter(|e| matches_prefix(&e.key, prefix))
                    .map(|e| quoted_item(&e.key, quoted, "value", "a feature of this crate"))
                    .collect(),
                _ => Vec::new(),
            }
        }
    }
}

/// What a `[features]` value may refer to. The three shapes, offered as three groups.
fn feature_items(m: &Manifest, prefix: &str, quoted: bool) -> Vec<CompletionItem> {
    let deps = declared(m);
    let mut out = Vec::new();
    for e in m.entries_in("features") {
        if matches_prefix(&e.key, prefix) {
            out.push(quoted_item(&e.key, quoted, "value", "another feature of this crate"));
        }
    }
    for d in &deps {
        if d.optional && matches_prefix(&d.name, prefix) {
            out.push(quoted_item(&d.name, quoted, "value", "an optional dependency's own feature"));
        }
        let dep_ref = format!("dep:{}", d.name);
        if d.optional && matches_prefix(&dep_ref, prefix) {
            out.push(quoted_item(&dep_ref, quoted, "value", "the dependency, with no feature of its own"));
        }
        let slash = format!("{}/", d.name);
        if matches_prefix(&slash, prefix) {
            out.push(quoted_item(&slash, quoted, "value", "enable a feature ON this dependency"));
        }
    }
    out
}

/// Directories under the manifest's own that hold a `Cargo.toml`, plus the `dir/*` glob.
fn member_items(ctx: &Context, prefix: &str, quoted: bool) -> Vec<CompletionItem> {
    let Some(dir) = ctx.dir.as_deref() else { return Vec::new() };
    // The prefix may already be a partial path (`crates/`), so completion continues from the
    // directory it names rather than always from the root.
    let (base_rel, tail) = match prefix.rfind('/') {
        Some(i) => (&prefix[..i + 1], &prefix[i + 1..]),
        None => ("", prefix),
    };
    let base = dir.join(base_rel);
    let Ok(entries) = std::fs::read_dir(&base) else { return Vec::new() };
    let mut out = Vec::new();
    let mut any_child_crate = false;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_string) else { continue };
        if name.starts_with('.') || name == "target" {
            continue;
        }
        if !matches_prefix(&name, tail) {
            continue;
        }
        let full = format!("{base_rel}{name}");
        if path.join("Cargo.toml").is_file() {
            out.push(quoted_item(&full, quoted, "folder", "a crate here"));
        } else {
            // Not a crate itself, but it may hold them — worth offering as a step on the way.
            any_child_crate = true;
            out.push(quoted_item(&format!("{full}/"), quoted, "folder", "a directory"));
        }
    }
    if any_child_crate && matches_prefix(&format!("{base_rel}*"), prefix) {
        out.push(quoted_item(
            &format!("{base_rel}*"),
            quoted,
            "value",
            "every crate directly under here",
        ));
    }
    out
}

/// The keys of a dependency spec, minus the ones already written.
fn spec_key_items(m: &Manifest, table: &str, key: &str, prefix: &str) -> Vec<CompletionItem> {
    if !schema::is_dependency_table(table) {
        return Vec::new();
    }
    let present: Vec<String> = m
        .get_base(table, key)
        .map(|e| e.inline_keys().into_iter().map(|k| k.key).collect())
        .unwrap_or_default();
    schema::DEP_KEYS
        .iter()
        .filter(|k| matches_prefix(k.name, prefix))
        .filter(|k| !present.iter().any(|p| p == k.name))
        .map(|k| item(k.name, "property", k.doc))
        .collect()
}

/// The value of one key inside a dependency spec.
fn spec_value_items(
    table: &str,
    key: &str,
    spec: &str,
    prefix: &str,
    quoted: bool,
    ctx: &Context,
) -> Vec<CompletionItem> {
    if !schema::is_dependency_table(table) {
        return Vec::new();
    }
    match spec {
        "version" => version_items(key, prefix, quoted, ctx),
        "optional" | "workspace" | "default-features" | "public" => ["true", "false"]
            .iter()
            .filter(|v| matches_prefix(v, prefix))
            .map(|v| item(v, "value", "a boolean"))
            .collect(),
        "package" => ctx
            .catalog
            .crates
            .iter()
            .filter(|name| matches_prefix(name, prefix))
            .map(|name| quoted_item(name, quoted, "module", "the real crate name"))
            .collect(),
        // A dependency's own features live in ITS manifest, which we have not read. Offering a
        // guess here would be inventing feature names.
        _ => Vec::new(),
    }
}

// ── helpers ────────────────────────────────────────────────────────────────────

/// Case-insensitive prefix match. Empty prefix matches everything.
fn matches_prefix(candidate: &str, prefix: &str) -> bool {
    prefix.is_empty() || candidate.to_ascii_lowercase().starts_with(&prefix.to_ascii_lowercase())
}

fn item(label: &str, kind: &str, detail: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: kind.to_string(),
        detail: (!detail.is_empty()).then(|| detail.to_string()),
        ..CompletionItem::default()
    }
}

/// A value candidate that supplies its own quotes when the caret is not already inside a string.
fn quoted_item(value: &str, already_quoted: bool, kind: &str, detail: &str) -> CompletionItem {
    let mut it = item(value, kind, detail);
    if !already_quoted {
        it.insert_text = Some(format!("\"{value}\""));
    }
    it
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A catalog with no filesystem behind it, so the tests are about the logic.
    fn catalog() -> Catalog {
        Catalog {
            crates: vec!["serde".into(), "serde_json".into(), "tokio".into(), "anyhow".into()],
            versions: vec![
                ("serde".into(), "1.0.9".into()),
                ("serde".into(), "1.0.10".into()),
                ("tokio".into(), "1.35.0".into()),
            ],
        }
    }

    fn ctx() -> Context {
        Context { dir: None, catalog: catalog() }
    }

    /// Complete at the `|` in `text`, which is removed first.
    fn at(text: &str) -> Vec<CompletionItem> {
        let offset = text.find('|').expect("mark the caret with |");
        let real = text.replace('|', "");
        complete(&real, offset, &ctx())
    }

    fn labels(text: &str) -> Vec<String> {
        at(text).into_iter().map(|i| i.label).collect()
    }

    fn spot(text: &str) -> Spot {
        let offset = text.find('|').expect("mark the caret with |");
        let real = text.replace('|', "");
        let m = Manifest::parse(&real);
        spot_at(&real, offset, &m).expect("a spot")
    }

    #[test]
    fn a_header_being_typed_offers_tables() {
        // Prefix matching, not fuzzy: `dep` is `dependencies` and not `dev-dependencies`, which
        // is predictable in a way a fuzzy match is not.
        assert_eq!(labels("[dep|"), vec!["dependencies"]);
        let got = labels("[d|");
        assert!(got.contains(&"dependencies".to_string()), "{got:?}");
        assert!(got.contains(&"dev-dependencies".to_string()), "{got:?}");
        assert!(!got.contains(&"package".to_string()), "the prefix filters: {got:?}");
    }

    /// A table the manifest already has is not one to add — but a second `[[bin]]` is.
    #[test]
    fn a_table_already_present_is_not_offered_again() {
        let got = labels("[dependencies]\nserde = \"1\"\n[dep|");
        assert!(!got.contains(&"dependencies".to_string()), "{got:?}");
        let got = labels("[[bin]]\nname = \"a\"\n[[b|");
        assert!(got.contains(&"[bin]".to_string()), "{got:?}");
    }

    #[test]
    fn a_key_of_a_closed_table_comes_from_the_schema() {
        let got = labels("[package]\nname = \"x\"\nedi|");
        assert_eq!(got, vec!["edition"]);
        // Already-set keys are not offered — that would be offering a duplicate-key error.
        let got = labels("[package]\nname = \"x\"\nn|");
        assert!(!got.contains(&"name".to_string()), "{got:?}");
    }

    #[test]
    fn a_key_in_a_dependency_table_offers_crate_names_with_a_version() {
        let items = at("[package]\nname = \"x\"\n[dependencies]\nser|");
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, vec!["serde", "serde_json"]);
        // The whole assignment is inserted: a bare crate name is not a dependency.
        assert_eq!(items[0].insert_text.as_deref(), Some("serde = \"1.0.10\""));
        // Newest version, not the first one seen.
        assert!(items[0].detail.as_deref().is_some_and(|d| d.starts_with("1.0.10")));
    }

    #[test]
    fn a_crate_already_declared_is_not_offered_again() {
        let got = labels("[package]\nname = \"x\"\n[dependencies]\nserde = \"1\"\nser|");
        assert_eq!(got, vec!["serde_json"]);
    }

    #[test]
    fn after_a_dot_the_inheritance_marker_is_offered_only_where_it_is_legal() {
        assert_eq!(labels("[package]\nedition.|"), vec!["workspace"]);
        // `name` cannot be inherited, so nothing is offered rather than something wrong.
        assert!(labels("[package]\nname.|").is_empty());
        // In a dependency table a dot introduces a spec key.
        let got = labels("[package]\nname = \"x\"\n[dependencies]\nserde.ver|");
        assert_eq!(got, vec!["version"]);
    }

    #[test]
    fn an_enum_value_is_offered_from_the_same_list_the_validator_checks() {
        let got = labels("[package]\nname = \"x\"\nedition = \"20|\"");
        assert_eq!(got, vec!["2015", "2018", "2021", "2024"]);
        // The caret is already inside quotes, so the insertion must not add another pair.
        let items = at("[package]\nname = \"x\"\nedition = \"20|\"");
        assert!(items[0].insert_text.is_none());
    }

    #[test]
    fn an_enum_value_typed_outside_quotes_brings_its_own() {
        let items = at("[package]\nname = \"x\"\nedition = |");
        assert!(!items.is_empty());
        assert_eq!(items.iter().find(|i| i.label == "2021").unwrap().insert_text.as_deref(), Some("\"2021\""));
    }

    #[test]
    fn a_dependency_version_offers_what_this_machine_has_newest_first() {
        let items = at("[package]\nname = \"x\"\n[dependencies]\nserde = \"|\"");
        let got: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(got, vec!["1.0.10", "1.0.9"], "1.0.10 is newer than 1.0.9");
        // …and the order survives whatever the client sorts by.
        assert_eq!(items[0].sort_text.as_deref(), Some("0000"));
    }

    #[test]
    fn a_spec_key_is_offered_inside_an_inline_table() {
        let got = labels("[package]\nname = \"x\"\n[dependencies]\nserde = { fea| }");
        assert_eq!(got, vec!["features"]);
        // One already written is not offered again.
        let got = labels("[package]\nname = \"x\"\n[dependencies]\nserde = { version = \"1\", ver| }");
        assert!(got.is_empty(), "{got:?}");
    }

    #[test]
    fn a_spec_value_knows_which_key_it_belongs_to() {
        let got = labels("[package]\nname = \"x\"\n[dependencies]\nserde = { version = \"|\" }");
        assert_eq!(got, vec!["1.0.10", "1.0.9"]);
        let got = labels("[package]\nname = \"x\"\n[dependencies]\nserde = { optional = | }");
        assert_eq!(got, vec!["true", "false"]);
        // A dependency's own features are in ITS manifest, which we have not read — so nothing,
        // rather than invented names.
        assert!(labels("[package]\nname = \"x\"\n[dependencies]\nserde = { features = [\"|\"] }").is_empty());
    }

    #[test]
    fn a_feature_value_offers_features_optional_dependencies_and_both_reference_forms() {
        let text = "\
[package]
name = \"x\"

[dependencies]
serde = { version = \"1\", optional = true }
anyhow = \"1\"

[features]
std = []
pretty = [\"|\"]
";
        let offset = text.find('|').expect("caret");
        let got: Vec<String> = complete(&text.replace('|', ""), offset, &ctx())
            .into_iter()
            .map(|i| i.label)
            .collect();
        assert!(got.contains(&"std".to_string()), "another feature: {got:?}");
        assert!(got.contains(&"serde".to_string()), "an optional dependency: {got:?}");
        assert!(got.contains(&"dep:serde".to_string()), "the dep: form: {got:?}");
        assert!(got.contains(&"serde/".to_string()), "the slash form: {got:?}");
        // A non-optional dependency has no implicit feature, so it is offered ONLY with a slash.
        assert!(got.contains(&"anyhow/".to_string()), "{got:?}");
        assert!(!got.contains(&"anyhow".to_string()), "{got:?}");
    }

    #[test]
    fn a_multi_line_array_completes_because_the_spans_know_where_it_started() {
        let text = "[package]\nname = \"x\"\n[features]\nstd = []\npretty = [\n  \"s|\",\n]\n";
        let offset = text.find('|').expect("caret");
        let got: Vec<String> = complete(&text.replace('|', ""), offset, &ctx())
            .into_iter()
            .map(|i| i.label)
            .collect();
        assert_eq!(got, vec!["std"]);
    }

    #[test]
    fn nothing_completes_inside_a_comment() {
        let offset = "[package]\n# ser|".find('|').unwrap();
        assert!(complete(&"[package]\n# ser|".replace('|', ""), offset, &ctx()).is_empty());
        // …but a `#` inside a string is not a comment.
        assert!(matches!(spot("[package]\nname = \"a#b|\""), Spot::Value { .. }));
    }

    #[test]
    fn the_replace_range_covers_the_whole_token_in_both_directions() {
        let text = "[package]\nname = \"x\"\nedition = \"20|21\"";
        let offset = text.find('|').unwrap();
        let real = text.replace('|', "");
        let items = complete(&real, offset, &ctx());
        let it = items.first().expect("at least one edition");
        // The range is the quoted content `2021`, so accepting replaces it whole rather than
        // leaving a `21` behind.
        assert_eq!(&real[it.replace_start.unwrap()..it.replace_end.unwrap()], "2021");
    }

    #[test]
    fn a_free_table_offers_nothing_rather_than_guessing() {
        assert!(labels("[package]\nname = \"x\"\n[features]\nmy|").is_empty());
        assert!(labels("[package]\nname = \"x\"\n[package.metadata]\nany|").is_empty());
    }

    #[test]
    fn the_spot_is_classified_correctly_in_every_position() {
        assert!(matches!(spot("[dep|"), Spot::Header { .. }));
        assert!(matches!(spot("[package]\nedi|"), Spot::Key { .. }));
        assert!(matches!(spot("[package]\nedition.|"), Spot::KeySuffix { .. }));
        assert!(matches!(spot("[package]\nedition = \"2|\""), Spot::Value { .. }));
        assert!(matches!(spot("[package]\ninclude = [\"a|\"]"), Spot::Item { .. }));
        assert!(matches!(spot("[dependencies]\nserde = { v|"), Spot::SpecKey { .. }));
        assert!(matches!(spot("[dependencies]\nserde = { version = \"1|\""), Spot::SpecValue { .. }));
        assert!(matches!(
            spot("[dependencies]\nserde = { features = [\"d|\"] }"),
            Spot::SpecValue { .. }
        ));
    }

    #[test]
    fn a_version_sorts_numerically_not_lexically() {
        let cat = Catalog {
            crates: vec!["x".into()],
            versions: vec![
                ("x".into(), "1.9.0".into()),
                ("x".into(), "1.10.0".into()),
                ("x".into(), "1.10.0-beta.1".into()),
            ],
        };
        // 1.10.0 above 1.9.0 — a plain string sort gets this backwards, which is why
        // `version_key` exists.
        assert_eq!(cat.versions_of("x").first(), Some(&"1.10.0"));
        assert_eq!(cat.versions_of("nope"), Vec::<&str>::new());
    }

    #[test]
    fn a_registry_filename_splits_at_the_version_not_the_first_hyphen() {
        assert_eq!(split_name_version("serde-1.0.0"), Some(("serde", "1.0.0")));
        assert_eq!(
            split_name_version("tracing-subscriber-0.3.18"),
            Some(("tracing-subscriber", "0.3.18"))
        );
        // A name ending in a digit-led segment that is not a version has no answer to give.
        assert_eq!(split_name_version("noversion"), None);
    }

    #[test]
    fn members_complete_against_real_directories() {
        let dir = std::env::temp_dir().join(format!(
            "bennu-cargo-complete-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("crates/one")).unwrap();
        std::fs::write(dir.join("crates/one/Cargo.toml"), "[package]\nname=\"one\"\n").unwrap();
        std::fs::create_dir_all(dir.join("target")).unwrap();

        let ctx = Context { dir: Some(dir.clone()), catalog: Catalog::default() };
        let text = "[workspace]\nmembers = [\"|\"]\n";
        let offset = text.find('|').unwrap();
        let got: Vec<String> = complete(&text.replace('|', ""), offset, &ctx)
            .into_iter()
            .map(|i| i.label)
            .collect();
        // `crates` is not itself a crate, so it is offered as a directory plus the glob that
        // covers what is under it. `target` is never offered.
        assert!(got.contains(&"crates/".to_string()), "{got:?}");
        assert!(got.contains(&"*".to_string()), "{got:?}");
        assert!(!got.iter().any(|g| g.starts_with("target")), "{got:?}");

        // One level down, the crate itself is offered.
        let text = "[workspace]\nmembers = [\"crates/|\"]\n";
        let offset = text.find('|').unwrap();
        let got: Vec<String> = complete(&text.replace('|', ""), offset, &ctx)
            .into_iter()
            .map(|i| i.label)
            .collect();
        assert!(got.contains(&"crates/one".to_string()), "{got:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
