//! One walk of the document, four lists out of it.
//!
//! The outline, the tasks, the links and the tags are the four things every panel
//! in the product is built on, and they are all answers to "what is in this
//! note". Computing them in one traversal rather than four passes is not a
//! micro-optimisation — it is what makes them *consistent*: a task under a
//! heading knows which heading, because the same walk saw both.
//!
//! ## Why this is a walk and not a regex
//!
//! The entire reason Garrulus parses markdown rather than scanning it is visible
//! here in one line: [`Block::Code`] is skipped. A `#tag` inside a fenced block is
//! text, a `[[` inside inline code is not a link, and a note about markdown
//! syntax does not silently acquire twenty phantom tags. Every note app built on
//! regexes gets this wrong, and it is the bug the user notices first.

use garrulus_ast::prelude::{Block, Document, Inline, ListItem, TaskState};

use crate::note::{Heading, Link, Tag, Task};

/// Everything a single traversal yields.
#[derive(Debug, Clone, Default)]
pub struct Extracted {
    /// Wikilinks, markdown links and embedded images, in document order.
    pub links: Vec<Link>,
    /// Inline `#tags`, in document order. Frontmatter tags are added by the
    /// caller, which is the only place that knows about frontmatter.
    pub tags: Vec<Tag>,
    /// Checkbox items, each carrying the heading it sits under.
    pub tasks: Vec<Task>,
    /// The outline.
    pub headings: Vec<Heading>,
    /// The first level-1 heading, which is the title of a note whose frontmatter
    /// does not name one.
    pub first_h1: Option<String>,
}

/// Walk a parsed document once.
pub fn extract(document: &Document) -> Extracted {
    let mut out = Extracted::default();
    let mut heading: Option<String> = None;
    walk_blocks(&document.blocks, &mut out, &mut heading);
    out
}

/// The plain text of a run of inlines — what a heading is called, what a task
/// says, what a table cell holds.
///
/// Emphasis and links contribute their content, not their syntax: the title of
/// `# Il **crash** all'avvio` is `Il crash all'avvio`, because that is what the
/// user would type into the quick switcher.
pub fn inline_text(inlines: &[Inline]) -> String {
    let mut out = String::new();
    push_inline_text(inlines, &mut out);
    out.trim().to_string()
}

fn push_inline_text(inlines: &[Inline], out: &mut String) {
    for inline in inlines {
        match inline {
            Inline::Text(text) => out.push_str(text),
            Inline::Code(text) => out.push_str(text),
            Inline::Emph(children)
            | Inline::Strong(children)
            | Inline::Strike(children)
            | Inline::Highlight(children) => push_inline_text(children, out),
            Inline::Link { label, .. } => push_inline_text(label, out),
            // A wikilink reads as its alias when it has one — that is the whole
            // point of `[[note|come lo chiamo qui]]`.
            Inline::WikiLink { target, alias, .. } => {
                out.push_str(alias.as_deref().unwrap_or(target))
            }
            Inline::Image { alt, .. } => out.push_str(alt),
            Inline::Tag { name, .. } => {
                out.push('#');
                out.push_str(name);
            }
            Inline::Break => out.push(' '),
        }
    }
}

fn walk_blocks(blocks: &[Block], out: &mut Extracted, heading: &mut Option<String>) {
    for block in blocks {
        match block {
            Block::Heading { level, inlines, span } => {
                let text = inline_text(inlines);
                if *level == 1 && out.first_h1.is_none() && !text.is_empty() {
                    out.first_h1 = Some(text.clone());
                }
                *heading = Some(text.clone());
                out.headings.push(Heading { level: *level, text, span: span.clone() });
                walk_inlines(inlines, out);
            }
            Block::Paragraph { inlines, .. } => walk_inlines(inlines, out),
            Block::List { items, .. } => walk_items(items, out, heading),
            // Deliberately not walked: see the module note.
            Block::Code { .. } | Block::Rule { .. } | Block::Html { .. } => {}
            Block::Quote { blocks, .. } => walk_blocks(blocks, out, heading),
            Block::Callout { blocks, .. } => walk_blocks(blocks, out, heading),
            Block::Table { head, rows, .. } => {
                for cell in head {
                    walk_inlines(cell, out);
                }
                for row in rows {
                    for cell in row {
                        walk_inlines(cell, out);
                    }
                }
            }
        }
    }
}

fn walk_items(items: &[ListItem], out: &mut Extracted, heading: &mut Option<String>) {
    for item in items {
        if let Some(state) = &item.task {
            out.tasks.push(Task {
                done: matches!(state, TaskState::Done),
                text: first_line_of(&item.blocks),
                heading: heading.clone(),
                span: item.span.clone(),
            });
        }
        walk_blocks(&item.blocks, out, heading);
    }
}

/// A task's text is the first paragraph of the item — the rest is the note that
/// hangs off it, and putting all of it in the Tasks panel would make the panel
/// unreadable.
fn first_line_of(blocks: &[Block]) -> String {
    for block in blocks {
        match block {
            Block::Paragraph { inlines, .. } => return inline_text(inlines),
            Block::Heading { inlines, .. } => return inline_text(inlines),
            _ => continue,
        }
    }
    String::new()
}

fn walk_inlines(inlines: &[Inline], out: &mut Extracted) {
    for inline in inlines {
        match inline {
            Inline::Emph(children)
            | Inline::Strong(children)
            | Inline::Strike(children)
            | Inline::Highlight(children) => walk_inlines(children, out),
            Inline::WikiLink { target, heading, alias, embed, span } => out.links.push(Link {
                target: target.clone(),
                heading: heading.clone(),
                alias: alias.clone(),
                embed: *embed,
                external: false,
                span: span.clone(),
            }),
            Inline::Link { href, label, span } => {
                out.links.push(Link {
                    target: href.clone(),
                    heading: None,
                    alias: None,
                    embed: false,
                    external: is_external(href),
                    span: span.clone(),
                });
                walk_inlines(label, out);
            }
            // An image is a link to an attachment, and it has to be one: the
            // orphan-attachment check is "which files nothing points at", and an
            // image that did not count as a pointer would be collected as
            // garbage while still on screen.
            Inline::Image { src, alt, span } => out.links.push(Link {
                target: src.clone(),
                heading: None,
                alias: Some(alt.clone()).filter(|a| !a.is_empty()),
                embed: true,
                external: is_external(src),
                span: span.clone(),
            }),
            Inline::Tag { name, span } => {
                out.tags.push(Tag { name: name.clone(), span: Some(span.clone()) })
            }
            Inline::Text(_) | Inline::Code(_) | Inline::Break => {}
        }
    }
}

/// Does this href point outside the vault?
///
/// Anything with a scheme does, including `arbor://` — a deep link into another
/// product is not a note in this vault, and counting it as one would fill the
/// unresolved-links panel with links that are perfectly fine.
pub fn is_external(href: &str) -> bool {
    match href.find("://") {
        Some(at) => href[..at].chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-'),
        None => href.starts_with("mailto:") || href.starts_with("tel:"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scheme_makes_a_link_external() {
        assert!(is_external("https://example.org"));
        assert!(is_external("arbor://corvus/commit/abc"));
        assert!(is_external("mailto:qualcuno@example.org"));
        assert!(!is_external("./altra-nota.md"));
        assert!(!is_external("attachments/schermata.png"));
        assert!(!is_external("nota con : due punti.md"));
    }
}
