//! Editor answers for an `application*.yml` / `.properties` buffer.
//!
//! The other direction of the relationship the Java side handles: from Java you ask "what does
//! this key resolve to", and here you ask "what is this key, who reads it, and what may I write".
//!
//! ## What a property file does not tell you
//!
//! Nearly everything. A line reading `timeout: 30` says a name and three characters; it does not
//! say whether `30` is seconds or milliseconds, whether the key is spelled the way anything reads
//! it, or whether the key exists at all. Every answer here is assembled from somewhere else:
//!
//! - **the readers** — the `@Value` field, the `@ConfigurationProperties` binding. They give the
//!   type ("`java.time.Duration`, so `30` means thirty seconds") and the usage count;
//! - **the metadata** — `spring-configuration-metadata.json` out of the dependency jars, which
//!   describes every property Spring and the libraries accept ([`crate::metadata`]).
//!
//! The two never contradict each other in practice because they cover disjoint halves: the
//! metadata knows the framework's keys, the readers know the project's own. Where both answer,
//! the metadata wins — it is a declaration, the other an inference.
//!
//! ## Delivered as
//!
//! - **highlight** — `${…}` inside a value is coloured exactly as it is in a Java `@Value`,
//!   because it is the same thing and reading it as prose is how a typo survives;
//! - **gutter** — the usage count as the glyph; an unmarked line is the useful signal;
//! - **completion + ghost text** — keys and their legal values, never a guess (see below);
//! - **hover** — type, default, prose, and who reads it;
//! - **environment variable** — the override name for a key, computed rather than typed
//!   ([`crate::env`]).
//!
//! ## The bar for a suggestion
//!
//! Completion may offer a list; **ghost text may not guess**. It appears only where the answer
//! is single-valued: a documented default for a key you left empty, or a prefix that exactly one
//! known key can continue. Anywhere else it stays away and lets the popup do its job.
//!
//! That rule is not written here — it is [`bennu_complete::prefix::unique_continuation`], and so
//! are the caret mechanics and the de-duplicated candidate list. What stays in this file is the
//! *vocabulary*: which keys exist, what they mean, and which of two sources describes one better.
//!
//! Keys are matched in Spring's own terms throughout: `app.readTimeout` and `app.read-timeout`
//! are one key ([`canonical_key`]).

use bennu_complete::prelude::{
    line_number, line_prefix, line_start, safe_offset, unique_continuation, within, Proposal,
    Proposals,
};
use bennu_ext::prelude::{ExtGutterMark, ExtHighlight, ExtHover, ExtTarget};
use bennu_proto::prelude::CompletionItem;

use crate::metadata::PropertyMeta;
use crate::model::{simple_name, strip_generics, SpringModel};
use crate::props::{parse_property_file, split_key, PropertyEntry};
use crate::usages::canonical_key;

/// Whether this file is a property source the extension answers for.
pub fn is_property_source(path: &str) -> bool {
    let name = path.replace('\\', "/").rsplit('/').next().unwrap_or_default().to_string();
    crate::props::is_property_file(&name)
}

/// The keys declared in the buffer, each with the places that read it. Parsed from the LIVE
/// text, so a key you just typed is counted without saving.
fn declared_with_usages<'a>(
    model: &'a SpringModel,
    path: &str,
    source: &str,
) -> Vec<(PropertyEntry, Vec<&'a crate::model::PropertyUsage>)> {
    let Some(file) = parse_property_file(path, source) else { return Vec::new() };
    file.entries
        .into_iter()
        .map(|e| {
            let usages = model.usages_of(&canonical_key(&e.key));
            (e, usages)
        })
        .collect()
}

// ── Highlight ────────────────────────────────────────────────────────────────

/// Placeholders and SpEL inside the file's **values**.
///
/// The same pass the Java side runs over a `@Value` string, for the same reason: to the yaml
/// mode `${FILE_UPLOAD_MAX_SIZE:200MB}` is one undifferentiated scalar, so the key, the
/// default and the delimiters all read as plain text — and a misspelled key inside one is
/// invisible. Colouring it here means an expression looks the same wherever it is written,
/// which is the point: it *is* the same thing.
///
/// Only value spans are scanned. A `${` in a comment or in a key is not an expression Spring
/// will resolve, and colouring it would be claiming otherwise.
pub fn highlights(path: &str, source: &str) -> Vec<ExtHighlight> {
    let Some(file) = parse_property_file(path, source) else { return Vec::new() };
    let mut out = Vec::new();
    for e in &file.entries {
        if e.value_end <= e.value_start || e.value_end > source.len() {
            continue;
        }
        crate::highlight::expression_highlights(
            &source[e.value_start..e.value_end],
            e.value_start,
            &mut out,
        );
    }
    out
}

// ── Gutter / navigate ────────────────────────────────────────────────────────

/// A gutter mark on every key something reads, with the count as its glyph.
pub fn gutter(model: &SpringModel, path: &str, source: &str) -> Vec<ExtGutterMark> {
    declared_with_usages(model, path, source)
        .into_iter()
        .filter(|(_, u)| !u.is_empty())
        .map(|(entry, usages)| ExtGutterMark {
            line: entry.line,
            kind: "usage".to_string(),
            tooltip: match usages.len() {
                1 => format!("1 usage — {} ({})", usages[0].label, usages[0].kind),
                n => format!("{n} usages of `{}`", entry.key),
            },
            targets: usages.iter().map(|u| target_of(u)).collect(),
        })
        .collect()
}

fn target_of(u: &crate::model::PropertyUsage) -> ExtTarget {
    ExtTarget {
        file: u.file.clone(),
        offset: u.offset,
        label: u.label.clone(),
        detail: u.kind.clone(),
    }
}

/// Go-to from a key in a property file → the places that read it.
pub fn navigate(model: &SpringModel, path: &str, source: &str, offset: usize) -> Vec<ExtTarget> {
    declared_with_usages(model, path, source)
        .into_iter()
        .find(|(e, _)| within(offset, e.key_start, e.key_end))
        .map(|(_, usages)| usages.iter().map(|u| target_of(u)).collect())
        .unwrap_or_default()
}

// ── Hover ────────────────────────────────────────────────────────────────────

/// Hover on a key: what it is, what type it takes, what it defaults to, and who reads it.
pub fn hover(model: &SpringModel, path: &str, source: &str, offset: usize) -> Option<ExtHover> {
    let (entry, usages) = declared_with_usages(model, path, source)
        .into_iter()
        .find(|(e, _)| within(offset, e.key_start, e.key_end))?;

    let meta = model.metadata.lookup(&entry.key);
    let mut lines: Vec<String> = Vec::new();

    if let Some(t) = resolved_type(&entry.key, meta, &usages) {
        lines.push(format!("Type  {t}"));
    }
    if let Some(m) = meta {
        if !m.default_value.is_empty() {
            lines.push(format!("Default  {}", m.default_value));
        }
        if let Some(reason) = &m.deprecation {
            let detail = match (reason.is_empty(), m.replacement.is_empty()) {
                (true, true) => "Deprecated.".to_string(),
                (true, false) => format!("Deprecated — use `{}` instead.", m.replacement),
                (false, _) => format!("Deprecated — {reason}"),
            };
            lines.push(detail);
        }
        if !m.description.is_empty() {
            lines.push(m.description.clone());
        }
        if m.name != entry.key {
            // The lookup succeeded through a map ancestor: say which declaration made this
            // line legal, or "documented" reads as a claim about a key nobody declared.
            lines.push(format!("Nested under `{}`.", m.name));
        }
        if !m.source_type.is_empty() {
            lines.push(format!("Declared by {}", simple_name(&m.source_type)));
        }
    }

    lines.push(match usages.len() {
        // Said plainly, because it is the interesting answer: nothing in this project reads
        // this line. It may still be read from outside — a starter, an env override — so it is
        // stated as a fact about the project rather than as a verdict on the key.
        0 if meta.is_some() => "Nothing in this project reads this key directly.".to_string(),
        0 => "Nothing in this project reads this key.".to_string(),
        1 => format!("Read by {} ({})", usages[0].label, usages[0].kind),
        n => {
            let names: Vec<&str> = usages.iter().take(4).map(|u| u.label.as_str()).collect();
            let more = if n > 4 { format!(", +{} more", n - 4) } else { String::new() };
            format!("Read by {n}: {}{more}", names.join(", "))
        }
    });

    Some(ExtHover {
        title: entry.key.clone(),
        signature: if entry.value.is_empty() { "(empty)".to_string() } else { entry.value },
        doc: lines.join("\n"),
    })
}

/// The type of a key, from the declaration if there is one and from its readers otherwise.
///
/// Readers are an inference, so they are only trusted when they **agree**: two fields binding
/// the same key to different types is a real situation (and usually a bug), and picking one to
/// display would hide it. It is reported instead.
fn resolved_type(
    key: &str,
    meta: Option<&PropertyMeta>,
    usages: &[&crate::model::PropertyUsage],
) -> Option<String> {
    if let Some(m) = meta {
        return Some(short_type(&map_value_type(m, key)));
    }
    let mut declared: Vec<&str> =
        usages.iter().map(|u| u.type_text.as_str()).filter(|t| !t.is_empty()).collect();
    declared.sort_unstable();
    declared.dedup();
    match declared.len() {
        0 => None,
        1 => Some(short_type(declared[0])),
        _ => Some(format!(
            "{} — readers disagree",
            declared.iter().map(|t| short_type(t)).collect::<Vec<_>>().join(" / ")
        )),
    }
}

/// For a key that resolved through a map ancestor, the map's **value** type is the type of
/// this line — `logging.level` is a `Map<String,String>`, but `logging.level.root` is a String.
fn map_value_type(meta: &PropertyMeta, key: &str) -> String {
    if canonical_key(&meta.name) == canonical_key(key) {
        return meta.type_text.clone();
    }
    match (meta.type_text.find('<'), meta.type_text.rfind('>')) {
        (Some(open), Some(close)) if close > open => meta.type_text[open + 1..close]
            .rsplit(',')
            .next()
            .unwrap_or(&meta.type_text)
            .trim()
            .to_string(),
        _ => meta.type_text.clone(),
    }
}

/// `java.util.List<java.lang.String>` → `List<String>`. Generic arguments included: for a
/// property the shape of the collection is exactly what you need to know to write the value.
fn short_type(fqcn: &str) -> String {
    let bare = strip_generics(fqcn);
    let head = simple_name(&bare).replace('$', ".");
    match (fqcn.find('<'), fqcn.rfind('>')) {
        (Some(open), Some(close)) if close > open => {
            let args: Vec<String> =
                split_type_args(&fqcn[open + 1..close]).iter().map(|a| short_type(a)).collect();
            format!("{head}<{}>", args.join(", "))
        }
        _ => head,
    }
}

/// Split `A,B<C,D>` at depth-0 commas only.
fn split_type_args(args: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (i, c) in args.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                out.push(args[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(args[start..].trim().to_string());
    out
}

// ── Where the caret is ───────────────────────────────────────────────────────

/// What the caret is in the middle of typing.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Caret {
    /// A key, with the parent path it hangs under and the part typed so far.
    Key { parent: String, partial: String },
    /// A value, with the key it belongs to and the part typed so far.
    Value { key: String, partial: String },
}

impl Caret {
    /// The full dotted key the caret is completing (key position only).
    fn full_key(&self) -> String {
        match self {
            Caret::Key { parent, partial } if parent.is_empty() => partial.clone(),
            Caret::Key { parent, partial } => format!("{parent}.{partial}"),
            Caret::Value { key, .. } => key.clone(),
        }
    }
}

fn classify(path: &str, source: &str, offset: usize) -> Option<Caret> {
    let offset = safe_offset(source, offset)?;
    let line_start = line_start(source, offset);
    let before = line_prefix(source, offset);
    let trimmed = before.trim_start();
    if trimmed.starts_with('#') || trimmed.starts_with('!') || trimmed.starts_with('-') {
        return None;
    }
    // The caret's own indentation, not the line's: on `  a:  |b` the parent search must start
    // from where the key was written, and the line may continue past the caret.
    let indent = before.len() - trimmed.len();

    let is_yaml = !path.to_ascii_lowercase().ends_with(".properties");
    let sep = if is_yaml {
        split_key(trimmed)
    } else {
        trimmed.find(|c: char| c == '=' || c == ':')
    };

    match sep {
        // Past the separator → completing a value.
        Some(i) => {
            let key = trimmed[..i].trim();
            if key.is_empty() {
                return None;
            }
            let full = if is_yaml {
                let parent = yaml_parents(source, line_start, indent);
                if parent.is_empty() { key.to_string() } else { format!("{parent}.{key}") }
            } else {
                key.to_string()
            };
            Some(Caret::Value { key: full, partial: trimmed[i + 1..].trim_start().to_string() })
        }
        None => {
            let parent = if is_yaml { yaml_parents(source, line_start, indent) } else { String::new() };
            Some(Caret::Key { parent, partial: trimmed.to_string() })
        }
    }
}

/// The dotted path of the mapping keys open above `line_start` at an indent below `indent`.
///
/// Only keys with **no value of their own** count as parents — `app: x` followed by a deeper
/// line is malformed yaml, and treating it as a parent would invent a key that cannot exist.
fn yaml_parents(source: &str, line_start: usize, indent: usize) -> String {
    let mut segments: Vec<String> = Vec::new();
    let mut need = indent;
    for raw in source[..line_start].split('\n').rev() {
        if need == 0 {
            break;
        }
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        let trimmed = line.trim_start();
        if trimmed.is_empty()
            || trimmed.starts_with('#')
            || trimmed.starts_with("---")
            || trimmed.starts_with('-')
        {
            continue;
        }
        let ind = line.len() - trimmed.len();
        if ind >= need {
            continue;
        }
        let Some(colon) = split_key(trimmed) else { continue };
        if !trimmed[colon + 1..].trim().is_empty() {
            continue;
        }
        let key = trimmed[..colon].trim();
        if key.is_empty() {
            continue;
        }
        segments.push(key.to_string());
        need = ind;
    }
    segments.reverse();
    segments.join(".")
}

// ── Completion ───────────────────────────────────────────────────────────────

/// Keys (and legal values) at the caret.
///
/// Two sources, and the second is the one that makes this worth having on a legacy project:
/// alongside everything Spring documents, the project's **own** `@ConfigurationProperties`
/// paths complete too — those are keys nobody wrote documentation for and everybody misspells.
///
/// Documented first, project's own second, and the ordering is load-bearing: when the
/// annotation processor is on the classpath a key is in *both* vocabularies, and
/// [`Proposals`] keeps whichever described it first. Documented wins because it is a
/// declaration and the other an inference.
pub fn completions(
    model: &SpringModel,
    path: &str,
    source: &str,
    offset: usize,
) -> Vec<CompletionItem> {
    let Some(caret) = classify(path, source, offset) else { return Vec::new() };
    let mut out = Proposals::default();
    match &caret {
        Caret::Value { key, partial } => value_completions(model, key, partial, &mut out),
        Caret::Key { parent, .. } => {
            let full = canonical_key(&caret.full_key());
            // Under a yaml parent only the tail is typed — the ancestors are already on the
            // page above the caret, and offering them again would write them twice.
            let strip = if parent.is_empty() { 0 } else { canonical_key(parent).len() + 1 };
            let tail = |key: &str| key.get(strip..).filter(|s| !s.is_empty()).map(str::to_string);

            for meta in model.metadata.starting_with(&full) {
                let Some(label) = tail(&canonical_key(&meta.name)) else { continue };
                if !out.offer(Proposal::new(label, "property").detail(detail_of(meta))) && out.is_full()
                {
                    break;
                }
            }
            for b in &model.config_bindings {
                let canonical = canonical_key(&b.path);
                if !canonical.starts_with(&full) {
                    continue;
                }
                let Some(label) = tail(&canonical) else { continue };
                let detail =
                    format!("{}  · {}", short_type(&b.type_text), simple_name(&b.owner_fqcn));
                if !out.offer(Proposal::new(label, "property").detail(detail)) && out.is_full() {
                    break;
                }
            }
        }
    }
    out.into_items()
}

/// The legal values of `key`, filtered by what has been typed.
///
/// Only where the set is genuinely closed: a documented hint list, or a boolean. Everything
/// else — a URL, a duration, a class name — has no candidates, and offering the current
/// default as the only entry would dress a guess up as a choice.
fn value_completions(model: &SpringModel, key: &str, partial: &str, out: &mut Proposals) {
    let hints = model.metadata.values_for(key);
    let lower = partial.to_ascii_lowercase();
    if !hints.is_empty() {
        out.extend(
            hints
                .iter()
                .filter(|h| h.value.to_ascii_lowercase().starts_with(&lower))
                .map(|h| Proposal::new(h.value.clone(), "value").detail(h.description.clone())),
        );
        return;
    }
    let is_bool = model
        .metadata
        .lookup(key)
        .map(|m| m.type_text.ends_with("Boolean") || m.type_text == "boolean")
        .unwrap_or(false);
    if !is_bool {
        return;
    }
    out.extend(
        ["true", "false"]
            .into_iter()
            .filter(|v| v.starts_with(&lower))
            .map(|v| Proposal::new(v, "value")),
    );
}

fn detail_of(meta: &PropertyMeta) -> String {
    let ty = short_type(&meta.type_text);
    match (meta.default_value.is_empty(), meta.deprecation.is_some()) {
        (_, true) => format!("{ty}  · deprecated"),
        (false, _) => format!("{ty}  = {}", meta.default_value),
        _ => ty,
    }
}

// ── Ghost text ───────────────────────────────────────────────────────────────

/// The continuation that certainly follows the caret, or `None`.
///
/// "Certainly" is the whole contract — this is drawn inline, ahead of the caret, where it
/// reads like text that is already there. Two cases qualify:
///
/// 1. **an empty value whose key has a documented default** — Boot will use that number if the
///    line stays empty, so writing it changes nothing but makes it visible;
/// 2. **a key prefix exactly one known key can continue** — not a ranking, a unique match.
///
/// Anything else returns `None` and leaves the popup to present the alternatives honestly.
pub fn inline_hint(model: &SpringModel, path: &str, source: &str, offset: usize) -> Option<String> {
    match classify(path, source, offset)? {
        Caret::Value { key, partial } if partial.is_empty() => {
            let meta = model.metadata.lookup(&key)?;
            // Only for the key as declared: the default of `logging.level` is not the default
            // of `logging.level.com.acme`.
            if canonical_key(&meta.name) != canonical_key(&key) || meta.default_value.is_empty() {
                return None;
            }
            Some(meta.default_value.clone())
        }
        Caret::Value { .. } => None,
        caret @ Caret::Key { .. } => {
            let full = canonical_key(&caret.full_key());
            unique_continuation(
                &full,
                model.metadata.starting_with(&full).map(|m| canonical_key(&m.name)),
            )
        }
    }
}

// ── Environment override ─────────────────────────────────────────────────────

/// The environment variable that overrides the key on the line at `offset`.
///
/// Matched by **line**, not by the key span: this answers a right-click, and a right-click
/// lands wherever the pointer was. Requiring the caret to be inside the key would make the
/// menu item mysteriously do nothing half the time.
pub fn env_var_at(path: &str, source: &str, offset: usize) -> Option<crate::env::EnvVar> {
    let file = parse_property_file(path, source)?;
    let line = line_number(source, offset);
    let entry = file.entries.into_iter().find(|e| e.line == line)?;
    Some(crate::env::env_var(&entry.key, &entry.value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::beans::JavaUnit;
    use crate::model::SpringModel;
    use crate::scan::scan_java;

    const YAML_PATH: &str = "/p/src/main/resources/application.yml";
    const PROPS_PATH: &str = "/p/src/main/resources/application.properties";

    fn model(java: &str) -> SpringModel {
        let text = format!("import org.springframework.beans.factory.annotation.*;\n{java}");
        let u = JavaUnit { facts: scan_java("/p/S.java", &text).unwrap(), text };
        let units = std::slice::from_ref(&u);
        SpringModel {
            property_usages: crate::usages::property_usages(units, &[], &[]),
            ..SpringModel::default()
        }
    }

    /// A model whose metadata is the curated table — the state a project is in before its
    /// jars are read, and therefore the one worth testing against.
    fn with_metadata(java: &str) -> SpringModel {
        SpringModel { metadata: crate::metadata::builtin_index(), ..model(java) }
    }

    // ── Highlight ────────────────────────────────────────────────────────────

    #[test]
    fn a_placeholder_in_a_yaml_value_is_coloured_like_one_in_a_value_annotation() {
        let yaml = "app:\n  size: ${FILE_UPLOAD_MAX_SIZE:200MB}\n";
        let hs = highlights(YAML_PATH, yaml);
        let span = |kind: &str| {
            hs.iter().find(|h| h.kind == kind).map(|h| &yaml[h.start..h.end]).unwrap_or("")
        };
        assert_eq!(span("spring.placeholder"), "${FILE_UPLOAD_MAX_SIZE:200MB}");
        assert_eq!(span("spring.placeholder.key"), "FILE_UPLOAD_MAX_SIZE");
        assert_eq!(span("spring.placeholder.default"), "200MB");
    }

    #[test]
    fn a_quoted_value_is_scanned_inside_its_quotes() {
        let yaml = "app:\n  url: \"${DB_URL}\"\n";
        let hs = highlights(YAML_PATH, yaml);
        assert_eq!(
            hs.iter().find(|h| h.kind == "spring.placeholder").map(|h| &yaml[h.start..h.end]),
            Some("${DB_URL}"),
        );
    }

    /// Only values. A `${` written in a comment is not something Spring resolves, and
    /// colouring it would say it is.
    #[test]
    fn comments_and_keys_are_left_alone() {
        assert!(highlights(YAML_PATH, "# see ${OTHER}\napp:\n  a: 1\n").is_empty());
        assert!(highlights(YAML_PATH, "app:\n  plain: 30\n").is_empty());
    }

    #[test]
    fn properties_files_are_highlighted_too() {
        let text = "app.size=${MAX_SIZE:200MB}\n";
        let hs = highlights(PROPS_PATH, text);
        assert_eq!(
            hs.iter().find(|h| h.kind == "spring.placeholder.key").map(|h| &text[h.start..h.end]),
            Some("MAX_SIZE"),
        );
    }

    // ── Gutter / navigate ────────────────────────────────────────────────────

    #[test]
    fn a_read_key_gets_a_gutter_mark_whose_glyph_is_the_count() {
        let m = model("class S { @Value(\"${app.timeout}\") int a; @Value(\"${app.timeout}\") int b; }");
        let yaml = "app:\n  timeout: 30\n  unused: x\n";
        let marks = gutter(&m, YAML_PATH, yaml);
        assert_eq!(marks.len(), 1, "only the key something reads is marked");
        assert_eq!(marks[0].line, 2);
        assert_eq!(marks[0].targets.len(), 2);
        assert!(marks[0].tooltip.starts_with("2 usages"));
    }

    #[test]
    fn the_relaxed_spelling_still_matches() {
        let m = model("class S { @Value(\"${app.readTimeout}\") int a; }");
        let yaml = "app:\n  read-timeout: 30\n";
        assert_eq!(gutter(&m, YAML_PATH, yaml).len(), 1, "one key, two spellings");
    }

    #[test]
    fn go_to_from_a_key_lands_on_the_reader() {
        let m = model("class S { @Value(\"${app.timeout}\") int a; }");
        let yaml = "app:\n  timeout: 30\n";
        let at = yaml.find("timeout").unwrap() + 1;
        let t = navigate(&m, YAML_PATH, yaml, at);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].label, "S.a");
        assert_eq!(t[0].file, "/p/S.java");
    }

    // ── Hover ────────────────────────────────────────────────────────────────

    #[test]
    fn hover_says_who_reads_it_and_admits_when_nobody_does() {
        let m = model("class S { @Value(\"${app.timeout}\") int a; }");
        let yaml = "app:\n  timeout: 30\n  unused: x\n";
        let h = hover(&m, YAML_PATH, yaml, yaml.find("timeout").unwrap() + 1).unwrap();
        assert_eq!(h.title, "app.timeout");
        assert_eq!(h.signature, "30");
        assert!(h.doc.contains("S.a"));

        let dead = hover(&m, YAML_PATH, yaml, yaml.find("unused").unwrap() + 1).unwrap();
        assert!(dead.doc.contains("Nothing in this project reads"));
    }

    /// The answer the file itself cannot give: `30` is a Duration, so it means thirty seconds.
    #[test]
    fn hover_takes_the_type_from_the_field_the_key_is_injected_into() {
        let m = model("class S { @Value(\"${app.timeout}\") java.time.Duration t; }");
        let h = hover(&m, YAML_PATH, "app:\n  timeout: 30\n", 12).unwrap();
        assert!(h.doc.contains("Type  Duration"), "got: {}", h.doc);
    }

    #[test]
    fn two_readers_that_disagree_about_the_type_are_reported_rather_than_resolved() {
        let m = model(
            "class S { @Value(\"${app.n}\") int a; @Value(\"${app.n}\") String b; }",
        );
        let h = hover(&m, YAML_PATH, "app:\n  n: 1\n", 9).unwrap();
        assert!(h.doc.contains("readers disagree"), "got: {}", h.doc);
    }

    #[test]
    fn a_documented_key_hovers_with_its_type_default_and_prose() {
        let m = with_metadata("class S {}");
        let yaml = "server:\n  port: 9090\n";
        let h = hover(&m, YAML_PATH, yaml, yaml.find("port").unwrap() + 1).unwrap();
        assert_eq!(h.title, "server.port");
        assert!(h.doc.contains("Type  Integer"));
        assert!(h.doc.contains("Default  8080"));
        assert!(h.doc.contains("HTTP port"));
    }

    /// `logging.level.root` is a String even though `logging.level` is a Map — hovering the
    /// map's own type there would be actively misleading.
    #[test]
    fn a_key_under_a_map_shows_the_maps_value_type() {
        let m = with_metadata("class S {}");
        let yaml = "logging:\n  level:\n    root: DEBUG\n";
        let h = hover(&m, YAML_PATH, yaml, yaml.find("root").unwrap() + 1).unwrap();
        assert!(h.doc.contains("Type  String"), "got: {}", h.doc);
        assert!(h.doc.contains("Nested under `logging.level`"));
    }

    #[test]
    fn a_caret_off_any_key_answers_nothing() {
        let m = model("class S {}");
        let yaml = "app:\n  timeout: 30\n";
        assert!(hover(&m, YAML_PATH, yaml, yaml.find("30").unwrap()).is_none());
        assert!(navigate(&m, YAML_PATH, yaml, yaml.find("30").unwrap()).is_empty());
    }

    // ── Caret classification ─────────────────────────────────────────────────

    #[test]
    fn nesting_is_read_from_the_indentation_above_the_caret() {
        let yaml = "spring:\n  datasource:\n    ur";
        assert_eq!(
            classify(YAML_PATH, yaml, yaml.len()),
            Some(Caret::Key {
                parent: "spring.datasource".to_string(),
                partial: "ur".to_string(),
            }),
        );
    }

    #[test]
    fn a_sibling_that_already_has_a_value_is_not_mistaken_for_a_parent() {
        let yaml = "spring:\n  profiles: dev\n  datasource:\n    ur";
        let Some(Caret::Key { parent, .. }) = classify(YAML_PATH, yaml, yaml.len()) else {
            panic!("expected a key caret")
        };
        assert_eq!(parent, "spring.datasource");
    }

    #[test]
    fn past_the_colon_the_caret_is_completing_a_value() {
        let yaml = "spring:\n  jpa:\n    hibernate:\n      ddl-auto: cre";
        assert_eq!(
            classify(YAML_PATH, yaml, yaml.len()),
            Some(Caret::Value {
                key: "spring.jpa.hibernate.ddl-auto".to_string(),
                partial: "cre".to_string(),
            }),
        );
    }

    #[test]
    fn a_properties_file_has_no_parents_only_whole_keys() {
        let text = "spring.datasource.ur";
        assert_eq!(
            classify(PROPS_PATH, text, text.len()),
            Some(Caret::Key { parent: String::new(), partial: "spring.datasource.ur".to_string() }),
        );
        let text = "server.port=80";
        assert_eq!(
            classify(PROPS_PATH, text, text.len()),
            Some(Caret::Value { key: "server.port".to_string(), partial: "80".to_string() }),
        );
    }

    #[test]
    fn comments_and_sequence_items_are_not_completion_sites() {
        assert!(classify(YAML_PATH, "# serv", 6).is_none());
        assert!(classify(YAML_PATH, "list:\n  - ite", 13).is_none());
    }

    // ── Completion ───────────────────────────────────────────────────────────

    #[test]
    fn keys_complete_relative_to_the_nesting_they_are_written_under() {
        let m = with_metadata("class S {}");
        let yaml = "spring:\n  datasource:\n    u";
        let labels: Vec<String> =
            completions(&m, YAML_PATH, yaml, yaml.len()).into_iter().map(|c| c.label).collect();
        // Relative — the parent keys are already on the page above the caret.
        assert!(labels.contains(&"url".to_string()), "got: {labels:?}");
        assert!(!labels.iter().any(|l| l.starts_with("spring.")));
    }

    #[test]
    fn a_flat_key_completes_whole_in_a_properties_file() {
        let m = with_metadata("class S {}");
        let text = "spring.datasource.u";
        let labels: Vec<String> =
            completions(&m, PROPS_PATH, text, text.len()).into_iter().map(|c| c.label).collect();
        assert!(labels.contains(&"spring.datasource.url".to_string()), "got: {labels:?}");
    }

    #[test]
    fn a_completion_carries_the_type_and_the_default() {
        let m = with_metadata("class S {}");
        let text = "server.por";
        let item = completions(&m, PROPS_PATH, text, text.len())
            .into_iter()
            .find(|c| c.label == "server.port")
            .unwrap();
        assert_eq!(item.detail.as_deref(), Some("Integer  = 8080"));
        assert_eq!(item.kind, "property");
    }

    /// The half that matters on a legacy project: keys nobody documented still complete.
    #[test]
    fn the_projects_own_bound_properties_complete_alongside_springs() {
        let m = SpringModel {
            config_bindings: vec![crate::model::ConfigBinding {
                owner_fqcn: "com.acme.AppProps".to_string(),
                field: "retryCount".to_string(),
                path: "acme.app.retry-count".to_string(),
                type_text: "java.lang.Integer".to_string(),
                root_prefix: "acme.app".to_string(),
                file: "/p/AppProps.java".to_string(),
                offset: 0,
            }],
            ..with_metadata("class S {}")
        };
        let text = "acme.app.re";
        let item = completions(&m, PROPS_PATH, text, text.len())
            .into_iter()
            .find(|c| c.label == "acme.app.retry-count")
            .expect("the project's own key");
        assert!(item.detail.unwrap().contains("AppProps"));
    }

    #[test]
    fn values_complete_only_where_the_set_is_closed() {
        let m = with_metadata("class S {}");
        let yaml = "spring:\n  jpa:\n    hibernate:\n      ddl-auto: ";
        let labels: Vec<String> =
            completions(&m, YAML_PATH, yaml, yaml.len()).into_iter().map(|c| c.label).collect();
        assert_eq!(labels, ["none", "validate", "update", "create", "create-drop"]);

        // A boolean is a closed set too.
        let yaml = "spring:\n  jpa:\n    show-sql: ";
        let labels: Vec<String> =
            completions(&m, YAML_PATH, yaml, yaml.len()).into_iter().map(|c| c.label).collect();
        assert_eq!(labels, ["true", "false"]);

        // A URL is not.
        let yaml = "spring:\n  datasource:\n    url: ";
        assert!(completions(&m, YAML_PATH, yaml, yaml.len()).is_empty());
    }

    #[test]
    fn a_typed_value_prefix_filters_the_hints() {
        let m = with_metadata("class S {}");
        let text = "spring.jpa.hibernate.ddl-auto=cre";
        let labels: Vec<String> =
            completions(&m, PROPS_PATH, text, text.len()).into_iter().map(|c| c.label).collect();
        assert_eq!(labels, ["create", "create-drop"]);
    }

    // ── Ghost text ───────────────────────────────────────────────────────────

    #[test]
    fn an_empty_value_ghosts_the_documented_default() {
        let m = with_metadata("class S {}");
        let yaml = "server:\n  port: ";
        assert_eq!(inline_hint(&m, YAML_PATH, yaml, yaml.len()).as_deref(), Some("8080"));
    }

    #[test]
    fn a_prefix_only_one_key_can_continue_is_ghosted() {
        let m = with_metadata("class S {}");
        let text = "server.servlet.context-p";
        assert_eq!(inline_hint(&m, PROPS_PATH, text, text.len()).as_deref(), Some("ath"));
    }

    /// The discipline that separates this from a guess: several candidates, no ghost.
    #[test]
    fn an_ambiguous_prefix_is_left_to_the_popup() {
        let m = with_metadata("class S {}");
        let text = "spring.datasource.";
        assert!(inline_hint(&m, PROPS_PATH, text, text.len()).is_none());
        // A key with no documented default gets nothing either.
        let text = "spring.datasource.url=";
        assert!(inline_hint(&m, PROPS_PATH, text, text.len()).is_none());
        // Nor does a nested key borrow its map's default.
        let text = "logging.level.root=";
        assert!(inline_hint(&m, PROPS_PATH, text, text.len()).is_none());
    }

    // ── Environment override ─────────────────────────────────────────────────

    #[test]
    fn the_env_override_is_computed_from_the_line_the_pointer_is_on() {
        let yaml = "spring:\n  jpa:\n    show-sql: true\n";
        let at = yaml.find("true").unwrap();
        let v = env_var_at(YAML_PATH, yaml, at).unwrap();
        assert_eq!(v.key, "spring.jpa.show-sql");
        assert_eq!(v.name, "SPRING_JPA_SHOWSQL");
        assert_eq!(v.value, "true");

        // Anywhere on the line, including the key and the indentation.
        assert_eq!(env_var_at(YAML_PATH, yaml, yaml.find("show-sql").unwrap()).unwrap().name, v.name);
        // A line that declares no leaf key has no override to offer.
        assert!(env_var_at(YAML_PATH, yaml, 2).is_none());
    }

    #[test]
    fn only_spring_property_files_are_answered_for() {
        assert!(is_property_source("/p/application.yml"));
        assert!(is_property_source(r"C:\p\application-dev.properties"));
        assert!(!is_property_source("/p/messages.properties"));
    }
}
