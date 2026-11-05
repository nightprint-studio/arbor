//! What an expression may contain in one language, and how its numbers behave.

/// Which language's rules apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Java,
    Rust,
}

/// The per-language switches the lexer, parser and evaluator consult.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Syntax {
    pub lang: Lang,
    /// Whether a prefix `*` dereferences. Rust only.
    pub deref: bool,
    /// The shapes a *watch path* takes here, for the one line the user reads when they typed
    /// something else. Prose rather than a grammar, because the reader is looking at a text box.
    pub shapes: &'static str,
    /// Whether a call expression may be evaluated. `false` at level 1: a call parses, then is
    /// refused by name. The switch exists so that invoking methods is a target capability, not a
    /// grammar change.
    pub calls: bool,
}

pub const JAVA: Syntax = Syntax {
    lang: Lang::Java,
    deref: false,
    shapes: "a name, `.field` and `[0]`",
    calls: false,
};

pub const RUST: Syntax = Syntax {
    lang: Lang::Rust,
    deref: true,
    shapes: "a name, `.field`, `.0`, `[0]`, and a leading `*` to follow a reference",
    calls: false,
};

impl Syntax {
    /// Why `callee(…)` is refused — the reason depends on the language, because it is not the same
    /// reason: a JVM can run anything but should not inside a paused program; a native binary only
    /// contains the functions that were compiled into it.
    pub fn call_refusal(&self, callee: &str) -> String {
        match self.lang {
            Lang::Java => format!(
                "`{callee}(…)` calls a method, which cannot be evaluated — it would run application \
                 code inside the paused program. Read a field instead"
            ),
            Lang::Rust => format!(
                "`{callee}(…)` calls a function, which cannot be evaluated here — the debugger can \
                 only call what is in the binary, and running it would execute code inside the \
                 paused program. Read the fields it would use instead"
            ),
        }
    }

    /// The language's name, for a message.
    pub fn language(&self) -> &'static str {
        match self.lang {
            Lang::Java => "Java",
            Lang::Rust => "Rust",
        }
    }
}
