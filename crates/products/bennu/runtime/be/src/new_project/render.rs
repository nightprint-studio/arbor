//! A deliberately tiny template language for the data files under `templates/`.
//!
//! - `{{name}}` — the variable's value; an undefined name is an **error**, so a typo in a template
//!   fails a unit test instead of producing a project with a hole in it.
//! - `{{#if name}} … {{/if}}` / `{{#unless name}} … {{/unless}}` — a block shown while the
//!   variable is non-empty and not `"false"`. Blocks nest.
//! - A block tag alone on its line takes the whole line with it, so conditional sections do not
//!   leave blank lines behind.
//!
//! Nothing else: no loops, no helpers, no escaping. The few places that need repetition (a list of
//! dependencies) build that text in Rust and pass it in as one variable.

use std::collections::BTreeMap;

use super::model::GenFile;

#[derive(Default, Debug)]
pub struct Vars(BTreeMap<String, String>);

impl Vars {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn set(&mut self, key: &str, value: impl Into<String>) -> &mut Self {
        self.0.insert(key.into(), value.into());
        self
    }
    /// A flag is the empty string (off) or `"true"` (on).
    pub fn flag(&mut self, key: &str, on: bool) -> &mut Self {
        self.set(key, if on { "true" } else { "" })
    }
    fn truthy(&self, key: &str) -> Result<bool, String> {
        let v = self.0.get(key).ok_or_else(|| format!("template uses undefined variable `{key}`"))?;
        Ok(!v.is_empty() && v != "false")
    }
}

enum Tok<'a> {
    Text(&'a str),
    Var(&'a str),
    Open { negate: bool, key: &'a str },
    Close,
}

fn tokenize(tpl: &str) -> Result<Vec<Tok<'_>>, String> {
    let mut toks = Vec::new();
    let mut cursor = 0;
    while let Some(rel) = tpl[cursor..].find("{{") {
        let start = cursor + rel;
        let end = tpl[start..].find("}}").map(|e| start + e + 2).ok_or("unclosed `{{` in template")?;
        let inner = tpl[start + 2..end - 2].trim();
        let tok = if let Some(k) = inner.strip_prefix("#if ") {
            Tok::Open { negate: false, key: k.trim() }
        } else if let Some(k) = inner.strip_prefix("#unless ") {
            Tok::Open { negate: true, key: k.trim() }
        } else if inner == "/if" || inner == "/unless" {
            Tok::Close
        } else if inner.is_empty() || inner.starts_with('#') || inner.starts_with('/') {
            return Err(format!("unknown template tag `{{{{{inner}}}}}`"));
        } else {
            Tok::Var(inner)
        };

        let block = !matches!(tok, Tok::Var(_));
        let (mut text_end, mut next) = (start, end);
        if block {
            // Standalone: only blanks between the line start and the tag, and the tag to the
            // end of its line.
            let line_start = tpl[..start].rfind('\n').map_or(0, |i| i + 1);
            let after = &tpl[end..];
            let line_rest = after.find('\n').map_or(after.len(), |i| i);
            if tpl[line_start..start].trim().is_empty() && after[..line_rest].trim().is_empty() {
                text_end = line_start;
                next = (end + line_rest + 1).min(tpl.len());
            }
        }
        if text_end > cursor {
            toks.push(Tok::Text(&tpl[cursor..text_end]));
        }
        toks.push(tok);
        cursor = next;
    }
    if cursor < tpl.len() {
        toks.push(Tok::Text(&tpl[cursor..]));
    }
    Ok(toks)
}

/// Render `tpl` with `vars`.
pub fn render(tpl: &str, vars: &Vars) -> Result<String, String> {
    let mut out = String::with_capacity(tpl.len());
    // One entry per open block: whether its body is shown.
    let mut stack: Vec<bool> = Vec::new();
    for tok in tokenize(tpl)? {
        let live = stack.iter().all(|b| *b);
        match tok {
            Tok::Text(t) if live => out.push_str(t),
            Tok::Var(k) if live => {
                out.push_str(vars.0.get(k).ok_or_else(|| format!("template uses undefined variable `{k}`"))?)
            }
            Tok::Open { negate, key } => stack.push(if live { vars.truthy(key)? != negate } else { false }),
            Tok::Close => {
                stack.pop().ok_or("`{{/if}}` without a matching `{{#if}}`")?;
            }
            _ => {}
        }
    }
    if !stack.is_empty() {
        return Err("`{{#if}}` without a matching `{{/if}}`".into());
    }
    Ok(out)
}

/// A generator's files as data: `(path template, content template)`. A path that renders empty is
/// skipped — that is how a conditional file is written, `{{#if x}}path{{/if}}`.
pub type TemplateSet = &'static [(&'static str, &'static str)];

pub fn render_set(set: TemplateSet, vars: &Vars) -> Result<Vec<GenFile>, String> {
    let mut out = Vec::with_capacity(set.len());
    for (path_tpl, content_tpl) in set {
        let path = render(path_tpl, vars)?;
        if path.trim().is_empty() {
            continue;
        }
        out.push(GenFile::new(path.trim(), render(content_tpl, vars)?));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_set_skips_empty_paths() {
        const SET: TemplateSet = &[("a.txt", "{{name}}"), ("{{#if db}}b.txt{{/if}}", "x"), ("{{#if web}}c.txt{{/if}}", "y")];
        let files = render_set(SET, &vars()).unwrap();
        let paths: Vec<_> = files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["a.txt", "c.txt"]);
        assert_eq!(files[0].content, "demo");
    }

    fn vars() -> Vars {
        let mut v = Vars::new();
        v.set("name", "demo").flag("web", true).flag("db", false);
        v
    }

    #[test]
    fn substitutes() {
        assert_eq!(render("hi {{name}}!", &vars()).unwrap(), "hi demo!");
        assert_eq!(render("{{ name }}", &vars()).unwrap(), "demo");
    }

    #[test]
    fn undefined_variable_is_an_error() {
        assert!(render("{{nope}}", &vars()).unwrap_err().contains("nope"));
    }

    #[test]
    fn conditionals_and_nesting() {
        let t = "a{{#if web}}b{{#if db}}c{{/if}}d{{/if}}e{{#unless db}}f{{/unless}}";
        assert_eq!(render(t, &vars()).unwrap(), "abdef");
    }

    #[test]
    fn a_hidden_branch_does_not_demand_its_variables() {
        // `db` is off; the undefined variable inside must not be evaluated.
        assert_eq!(render("{{#if db}}{{missing}}{{/if}}ok", &vars()).unwrap(), "ok");
    }

    #[test]
    fn standalone_block_lines_vanish() {
        let t = "one\n{{#if web}}\ntwo\n{{/if}}\n{{#if db}}\nthree\n{{/if}}\nfour\n";
        assert_eq!(render(t, &vars()).unwrap(), "one\ntwo\nfour\n");
    }

    #[test]
    fn unbalanced_blocks_are_errors() {
        assert!(render("{{#if web}}x", &vars()).is_err());
        assert!(render("x{{/if}}", &vars()).is_err());
        assert!(render("{{oops", &vars()).is_err());
    }
}
