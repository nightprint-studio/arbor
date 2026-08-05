//! What can go wrong talking to a VM.

use std::fmt;

/// A JDWP failure. Three kinds, and the distinction is the useful part: the socket broke, the
/// VM refused, or the bytes were not what the protocol says they should be.
#[derive(Debug)]
pub enum JdwpError {
    /// The socket. Includes the VM exiting under us, which is not exceptional — a debugged
    /// program is allowed to finish.
    Io(std::io::Error),
    /// The VM answered with an error code. `code` is the JDWP error constant; `context` names
    /// the command that got it, because the code alone ("13") is unreadable.
    Vm { code: u16, context: &'static str },
    /// The bytes did not parse — a truncated packet, an unknown tag, a length that runs off
    /// the end. Either a protocol version mismatch or a bug here.
    Protocol(String),
}

impl fmt::Display for JdwpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JdwpError::Io(e) => write!(f, "jdwp: {e}"),
            JdwpError::Vm { code, context } => {
                write!(f, "jdwp: the VM refused {context} — {} ({code})", error_name(*code))
            }
            JdwpError::Protocol(m) => write!(f, "jdwp: {m}"),
        }
    }
}

impl std::error::Error for JdwpError {}

impl From<std::io::Error> for JdwpError {
    fn from(e: std::io::Error) -> Self {
        JdwpError::Io(e)
    }
}

/// The result of every operation in this crate.
pub type Result<T> = std::result::Result<T, JdwpError>;

/// The JDWP error constants worth naming. The full list is a hundred entries; these are the
/// ones a debugger actually provokes, and the rest read as their number — which is still
/// better than the number with no name beside it.
pub fn error_name(code: u16) -> &'static str {
    match code {
        10 => "invalid thread",
        11 => "the thread is not suspended",
        13 => "invalid object",
        20 => "invalid class",
        21 => "the class is not prepared yet",
        23 => "invalid method",
        25 => "invalid field",
        30 => "invalid frame",
        31 => "the VM is dead",
        32 => "the VM is not suspended",
        33 => "the type is not a class",
        35 => "invalid string",
        99 => "not implemented by this VM",
        101 => "the argument is null",
        102 => "the argument is invalid",
        112 => "absent information (compiled without debug info)",
        113 => "invalid event type",
        500 => "invalid argument",
        _ => "error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_reads_as_a_sentence() {
        let e = JdwpError::Vm { code: 112, context: "Method.LineTable" };
        assert_eq!(
            e.to_string(),
            "jdwp: the VM refused Method.LineTable — absent information (compiled without debug info) (112)"
        );
    }
}
