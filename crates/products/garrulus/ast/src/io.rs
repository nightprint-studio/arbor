//! The format seam: [`Reader`] parses a source format into a [`Document`],
//! [`Writer`] renders one back out.
//!
//! Two traits, and every format is an implementation of one or both. Markdown is
//! `MarkdownReader` / `MarkdownWriter` in `garrulus-parse`; HTML export, PDF
//! export, the static-site export and a plain-text export are `Writer`s; an
//! org-mode, AsciiDoc, Notion or Joplin import is a `Reader`. Everything between
//! the two — the index, backlinks, search, refactors, transclusion — is written
//! once against [`Document`] and is paid for once.
//!
//! Errors are two flat enums with `Display` messages, deliberately. Those strings
//! cross the RPC seam to the frontend as text (`docs/backend-architecture.md`:
//! error strings *are* the contract), so they must read as something a user can
//! act on, and they must be identical whether the handler ran in-process or in
//! the child.

use crate::document::Document;

/// Parse a source format into the model.
pub trait Reader {
    /// Read `source` into a [`Document`].
    ///
    /// Implementations should be **error-tolerant**: a note with a malformed
    /// table is still a note, and refusing to open it would strand the user's
    /// text behind a parse error. Reserve [`ReadError`] for input that cannot
    /// yield a document at all.
    fn read(&self, source: &str) -> Result<Document, ReadError>;
}

/// Render the model into a target format.
pub trait Writer {
    /// Render `doc` into this writer's format.
    ///
    /// A writer whose format can express frontmatter **must** honour
    /// [`crate::frontmatter::Frontmatter::source`] and echo the raw text when it
    /// is present — that is the byte-stability invariant, and losing it turns
    /// every note in the vault into a diff.
    fn write(&self, doc: &Document) -> Result<String, WriteError>;
}

/// Why a source could not be turned into a [`Document`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReadError {
    /// The frontmatter block is present but unusable — an unterminated fence, or
    /// a body that is not a mapping.
    #[error("frontmatter non valido a byte {offset}: {message}")]
    Frontmatter {
        /// Byte offset where the problem was noticed.
        offset: usize,
        /// What is wrong, in user-facing terms.
        message: String,
    },
    /// The grammar could not be loaded or applied — a build/link problem, not a
    /// problem with the note. Kept apart from [`ReadError::Syntax`] because the
    /// user can do nothing about it and the UI should say so.
    #[error("il parser non è disponibile: {0}")]
    Grammar(String),
    /// The source could not be parsed into a document.
    #[error("impossibile interpretare il documento: {0}")]
    Syntax(String),
    /// Anything a specific reader needs to report that the variants above do not
    /// cover. A catch-all rather than a growing enum that every consumer has to
    /// re-match on.
    #[error("{0}")]
    Other(String),
}

impl ReadError {
    /// A [`ReadError::Frontmatter`] from its parts.
    pub fn frontmatter(offset: usize, message: impl Into<String>) -> Self {
        ReadError::Frontmatter { offset, message: message.into() }
    }

    /// A [`ReadError::Syntax`].
    pub fn syntax(message: impl Into<String>) -> Self {
        ReadError::Syntax(message.into())
    }

    /// A [`ReadError::Grammar`].
    pub fn grammar(message: impl Into<String>) -> Self {
        ReadError::Grammar(message.into())
    }
}

/// Why a [`Document`] could not be rendered.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WriteError {
    /// The document uses a construct this format cannot express. The variant
    /// exists because it is a *design* answer, not a failure: a plain-text writer
    /// legitimately cannot render a table, and saying so beats guessing.
    #[error("questo formato non può rappresentare: {0}")]
    Unsupported(String),
    /// The frontmatter could not be serialised back out.
    #[error("impossibile serializzare il frontmatter: {0}")]
    Frontmatter(String),
    /// Anything else a specific writer needs to report.
    #[error("{0}")]
    Other(String),
}

impl WriteError {
    /// A [`WriteError::Unsupported`].
    pub fn unsupported(what: impl Into<String>) -> Self {
        WriteError::Unsupported(what.into())
    }

    /// A [`WriteError::Frontmatter`].
    pub fn frontmatter(message: impl Into<String>) -> Self {
        WriteError::Frontmatter(message.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The seam has to be object-safe: `GarrulusState` holds readers and writers
    /// chosen at runtime (which import format, which export format), so both
    /// traits must survive `Box<dyn …>`. Cheaper to assert here than to discover
    /// it in the BE.
    #[test]
    fn both_traits_are_object_safe() {
        struct Nothing;
        impl Reader for Nothing {
            fn read(&self, _source: &str) -> Result<Document, ReadError> {
                Ok(Document::empty())
            }
        }
        impl Writer for Nothing {
            fn write(&self, _doc: &Document) -> Result<String, WriteError> {
                Ok(String::new())
            }
        }

        let reader: Box<dyn Reader> = Box::new(Nothing);
        let writer: Box<dyn Writer> = Box::new(Nothing);
        assert!(reader.read("").is_ok());
        assert_eq!(writer.write(&Document::empty()), Ok(String::new()));
    }

    #[test]
    fn error_messages_carry_the_detail_they_were_given() {
        let err = ReadError::frontmatter(4, "fence non chiusa");
        assert!(err.to_string().contains("fence non chiusa"));
        assert!(err.to_string().contains('4'));
        assert_eq!(WriteError::unsupported("tabelle").to_string(), "questo formato non può rappresentare: tabelle");
    }
}
