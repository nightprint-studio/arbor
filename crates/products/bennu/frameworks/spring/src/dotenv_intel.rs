//! Editor answers for a `.env` file: the variable names Spring Boot reads, and their values.
//!
//! A `.env` is where a Spring property gets overridden from the outside, and the names in it are
//! the least guessable thing in the project: `spring.jpa.show-sql` is `SPRING_JPA_SHOWSQL` (the
//! dash is *removed*, see [`crate::env`]), and `SPRING_JPA_SHOW_SQL` — the obvious spelling —
//! binds nothing and says nothing. So the vocabulary is the one a yaml file completes against,
//! each key written the way the environment spells it.
//!
//! - **completion** — a variable name from the framework's metadata and the project's own
//!   `@ConfigurationProperties` paths, labelled with the key it overrides; after the `=`, the legal
//!   values where the set is closed (a boolean, a documented hint list);
//! - **hover** — the property a name binds to, its type, default and prose.
//!
//! The mapping is one-way (`SHOWSQL` does not say where the dash was), so a name is resolved by
//! computing the variable of every known key and matching — the same function that produced
//! the name, which is what keeps the two directions from disagreeing.
//!
//! A name nobody declares is **not** reported as wrong: a `.env` carries `DB_PASSWORD` and every
//! other variable the application and its container read, and only some of them are Spring's.

use bennu_complete::prelude::{Proposal, Proposals};
use bennu_ext::prelude::ExtHover;
use bennu_proto::prelude::CompletionItem;

use crate::env::env_var_name;
use crate::metadata::PropertyMeta;
use crate::model::SpringModel;
use crate::props_intel::{detail_of, short_type};

/// Whether `path` is a dotenv file: `.env`, `.env.local`, `.env.production`, `prod.env`.
pub fn is_dotenv(path: &str) -> bool {
    let name = path.replace('\\', "/").rsplit('/').next().unwrap_or_default().to_ascii_lowercase();
    name == ".env" || name.starts_with(".env.") || name.ends_with(".env")
}

/// Where the caret is on a line.
#[derive(Debug, PartialEq, Eq)]
enum Caret<'a> {
    /// Typing a variable name; `prefix` is what is there so far.
    Name { prefix: &'a str },
    /// After the `=`: `name` is the variable, `partial` the value typed so far.
    Value { name: &'a str, partial: &'a str },
}

/// Read the caret off the line it is on. `None` in a comment, or where nothing completes.
fn classify(source: &str, offset: usize) -> Option<Caret<'_>> {
    let mut at = offset.min(source.len());
    while at > 0 && !source.is_char_boundary(at) {
        at -= 1;
    }
    let line_start = source[..at].rfind('\n').map_or(0, |i| i + 1);
    let before = source[line_start..at].trim_start();
    if before.starts_with('#') {
        return None;
    }
    // `export FOO=bar` is a `.env` a shell can also source; the name is after the keyword.
    let before = before.strip_prefix("export ").map_or(before, str::trim_start);
    match before.find('=') {
        None => before
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
            .then_some(Caret::Name { prefix: before }),
        Some(eq) => Some(Caret::Value {
            name: before[..eq].trim(),
            partial: before[eq + 1..].trim_start_matches([' ', '"', '\'']),
        }),
    }
}

/// One property Spring may be given from the environment.
struct Known<'a> {
    /// The variable name (`SPRING_JPA_SHOWSQL`).
    name: String,
    /// The property key it overrides (`spring.jpa.show-sql`).
    key: &'a str,
    detail: String,
    meta: Option<&'a PropertyMeta>,
}

/// Every key the framework documents and the project binds, each as the variable that overrides it.
fn known(model: &SpringModel) -> Vec<Known<'_>> {
    let mut out: Vec<Known<'_>> = model
        .metadata
        .all()
        .map(|m| Known { name: env_var_name(&m.name), key: &m.name, detail: detail_of(m), meta: Some(m) })
        .collect();
    out.extend(
        model
            .config_bindings
            .iter()
            // `app.clients.<key>.url`: the middle is whatever the user names it, so there is no
            // single variable to offer.
            .filter(|b| !b.path.contains('<'))
            .map(|b| Known {
                name: env_var_name(&b.path),
                key: &b.path,
                detail: short_type(&b.type_text),
                meta: None,
            }),
    );
    out
}

/// Variable names (and legal values) at the caret.
pub fn completions(model: &SpringModel, source: &str, offset: usize) -> Vec<CompletionItem> {
    let Some(caret) = classify(source, offset) else { return Vec::new() };
    let mut out = Proposals::default();
    match caret {
        Caret::Name { prefix } => {
            let want = prefix.to_ascii_uppercase();
            for k in known(model).into_iter().filter(|k| k.name.starts_with(&want)) {
                // The detail leads with the key, so a name the environment spells oddly still
                // says which property it is.
                let detail = format!("{}  · {}", k.key, k.detail);
                if !out.offer(Proposal::new(k.name, "property").detail(detail)) && out.is_full() {
                    break;
                }
            }
        }
        Caret::Value { name, partial } => {
            let upper = name.to_ascii_uppercase();
            if let Some(meta) = known(model).into_iter().find(|k| k.name == upper).and_then(|k| k.meta) {
                value_completions(model, meta, partial, &mut out);
            }
        }
    }
    out.into_items()
}

/// The legal values of a property, filtered by what has been typed — only where the set is closed,
/// for the reason [`crate::props_intel`] gives: a default offered as the only choice is a guess
/// dressed as one.
fn value_completions(model: &SpringModel, meta: &PropertyMeta, partial: &str, out: &mut Proposals) {
    let lower = partial.to_ascii_lowercase();
    let hints = model.metadata.values_for(&meta.name);
    if !hints.is_empty() {
        out.extend(
            hints
                .iter()
                .filter(|h| h.value.to_ascii_lowercase().starts_with(&lower))
                .map(|h| Proposal::new(h.value.clone(), "value").detail(h.description.clone())),
        );
        return;
    }
    if meta.type_text.ends_with("Boolean") || meta.type_text == "boolean" {
        out.extend(
            ["true", "false"]
                .into_iter()
                .filter(|v| v.starts_with(&lower))
                .map(|v| Proposal::new(v, "value")),
        );
    }
}

/// The property a variable name binds to, when the caret is on one.
pub fn hover(model: &SpringModel, source: &str, offset: usize) -> Option<ExtHover> {
    let mut at = offset.min(source.len());
    while at > 0 && !source.is_char_boundary(at) {
        at -= 1;
    }
    let line_start = source[..at].rfind('\n').map_or(0, |i| i + 1);
    let line_end = source[at..].find('\n').map_or(source.len(), |i| at + i);
    let line = &source[line_start..line_end];
    let trimmed = line.trim_start();
    if trimmed.starts_with('#') {
        return None;
    }
    let body = trimmed.strip_prefix("export ").map_or(trimmed, str::trim_start);
    let eq = body.find('=')?;
    let name = body[..eq].trim();
    let value = body[eq + 1..].trim();
    // Only over the name: a hover on the value would explain the key to somebody reading the value.
    let name_start = line_start + (line.len() - body.len());
    if at < name_start || at > name_start + name.len() {
        return None;
    }

    let upper = name.to_ascii_uppercase();
    let found = known(model).into_iter().find(|k| k.name == upper)?;
    let mut lines = vec![format!("Overrides  {}", found.key)];
    if let Some(m) = found.meta {
        lines.push(format!("Type  {}", short_type(&m.type_text)));
        if !m.default_value.is_empty() {
            lines.push(format!("Default  {}", m.default_value));
        }
        if let Some(reason) = &m.deprecation {
            lines.push(match (reason.is_empty(), m.replacement.is_empty()) {
                (true, true) => "Deprecated.".to_string(),
                (true, false) => format!("Deprecated — use `{}` instead.", m.replacement),
                (false, _) => format!("Deprecated — {reason}"),
            });
        }
        if !m.description.is_empty() {
            lines.push(m.description.clone());
        }
    } else {
        lines.push(format!("Type  {}", found.detail));
    }
    Some(ExtHover {
        title: name.to_string(),
        signature: if value.is_empty() { "(empty)".to_string() } else { value.to_string() },
        doc: lines.join("\n"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dotenv_files_are_recognised_by_name() {
        assert!(is_dotenv("/p/.env"));
        assert!(is_dotenv(r"C:\p\.env.local"));
        assert!(is_dotenv("/p/prod.env"));
        assert!(!is_dotenv("/p/environment.yml"));
        assert!(!is_dotenv("/p/application.properties"));
    }

    #[test]
    fn the_caret_is_a_name_before_the_equals_and_a_value_after() {
        assert_eq!(classify("SPRING_J", 8), Some(Caret::Name { prefix: "SPRING_J" }));
        assert_eq!(
            classify("SPRING_JPA_SHOWSQL=tr", 21),
            Some(Caret::Value { name: "SPRING_JPA_SHOWSQL", partial: "tr" })
        );
        // The keyword of a sourceable file is not part of the name.
        assert_eq!(classify("export SERVER_P", 15), Some(Caret::Name { prefix: "SERVER_P" }));
    }

    #[test]
    fn a_comment_completes_nothing() {
        assert_eq!(classify("# SPRING_", 9), None);
        assert_eq!(classify("A=1\n  # SPRING_", 15), None);
    }

    #[test]
    fn only_the_current_line_is_read() {
        assert_eq!(classify("A=1\nSERVER_", 11), Some(Caret::Name { prefix: "SERVER_" }));
    }
}
