//! Span-aware error reporting.
//!
//! Every failure carries an optional [`SourceSpan`] so the editor can underline
//! the exact characters at fault (`design/grove/architecture.md` — diagnostics
//! flow back to the front end with spans). The kinds cover the whole pipeline:
//! parse errors from the Tree-sitter front end, name/arity/type resolution in
//! the evaluator, mini-notation context rules, the totality (no-recursion)
//! guarantee, and import resolution.

use std::fmt;

use arbor_grove_pattern::prelude::{PatternError, SourceSpan};

/// A grove language error, optionally located in the source.
#[derive(Clone, Debug, PartialEq)]
pub struct LangError {
    pub kind: LangErrorKind,
    pub span: Option<SourceSpan>,
}

/// What went wrong.
#[derive(Clone, Debug, PartialEq)]
pub enum LangErrorKind {
    /// The Tree-sitter front end could not parse the source (syntax error).
    Parse(String),
    /// A name (variable / function / builtin) is not in scope.
    UnknownName(String),
    /// A value was used as a function/transform but isn't callable.
    NotCallable(String),
    /// Wrong number of arguments to a call.
    Arity {
        name: String,
        expected: usize,
        got: usize,
    },
    /// A value had the wrong type for where it was used.
    Type { expected: String, got: String },
    /// A mini-notation context rule was broken (`:n` outside `s`, `'chord`
    /// outside `n`, a bare degree without `.scale(...)`, …).
    Context(String),
    /// A `fn` (directly or mutually) calls itself — forbidden, grove is total.
    Recursion(Vec<String>),
    /// An `import` graph contains a cycle.
    ImportCycle(Vec<String>),
    /// A scale/note spec failed to parse (wraps the pattern crate's error).
    Pitch(PatternError),
    /// Anything else, with a message.
    Other(String),
}

impl LangError {
    /// Build an error with a span.
    pub fn at(span: SourceSpan, kind: LangErrorKind) -> Self {
        LangError {
            kind,
            span: Some(span),
        }
    }

    /// Build an error without a known location.
    pub fn unlocated(kind: LangErrorKind) -> Self {
        LangError { kind, span: None }
    }

    /// Attach a span if one isn't already set (fills in location while unwinding).
    pub fn or_span(mut self, span: SourceSpan) -> Self {
        if self.span.is_none() {
            self.span = Some(span);
        }
        self
    }
}

impl fmt::Display for LangError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.kind)?;
        if let Some(s) = self.span {
            write!(f, " (bytes {}..{})", s.start, s.end)?;
        }
        Ok(())
    }
}

impl fmt::Display for LangErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LangErrorKind::Parse(m) => write!(f, "parse error: {m}"),
            LangErrorKind::UnknownName(n) => write!(f, "unknown name `{n}`"),
            LangErrorKind::NotCallable(n) => write!(f, "`{n}` is not callable"),
            LangErrorKind::Arity {
                name,
                expected,
                got,
            } => write!(f, "`{name}` expects {expected} argument(s), got {got}"),
            LangErrorKind::Type { expected, got } => {
                write!(f, "type mismatch: expected {expected}, got {got}")
            }
            LangErrorKind::Context(m) => write!(f, "mini-notation: {m}"),
            LangErrorKind::Recursion(chain) => {
                write!(f, "recursive function call: {}", chain.join(" → "))
            }
            LangErrorKind::ImportCycle(chain) => {
                write!(f, "import cycle: {}", chain.join(" → "))
            }
            LangErrorKind::Pitch(e) => write!(f, "{e}"),
            LangErrorKind::Other(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for LangError {}

impl From<PatternError> for LangError {
    fn from(e: PatternError) -> Self {
        LangError::unlocated(LangErrorKind::Pitch(e))
    }
}

/// Crate-local result alias.
pub type Result<T> = std::result::Result<T, LangError>;
