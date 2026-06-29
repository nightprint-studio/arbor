//! `format` domain — reformat a `.merula` document to canonical style.
//!
//! Parses the source to the AST and prints it back through the canonical
//! pretty-printer (`merula-lang`'s [`emit`]), the same printer the materialiser
//! uses. The round-trip is *semantic*, not byte-exact: comments and incidental
//! whitespace live only in the source and are not recovered. A syntax error is
//! surfaced verbatim so the editor can tell the user formatting was skipped — it
//! then leaves the buffer untouched rather than dropping content.
//!
//! Pure (no state, no I/O); ported verbatim from the shell's
//! `src-tauri/src/merula/format.rs`, with the `AppError` mapped to the wire
//! `String`.

use merula::prelude::{emit, parse};

use crate::state::MerulaState;

/// Reformat `.merula` source to canonical style. Returns the formatted text, or a
/// language error (a syntax error the buffer currently has) which the caller
/// shows without modifying the document.
#[arbor_rpc::handler]
fn merula_format(_ctx: &MerulaState, source: String) -> Result<String, String> {
    let program = parse(&source).map_err(|e| e.to_string())?;
    Ok(emit(&program))
}

#[cfg(test)]
mod tests {
    use merula::prelude::{emit, parse};

    /// Formatting is idempotent: reformatting already-canonical source is a no-op.
    /// Parse → emit → parse → emit must reach a fixed point on the first pass.
    #[test]
    fn format_is_idempotent() {
        let src =
            "cps(0.5)\n\ntracks(\n  track(\"lead\", n(c4 e4 g4 c5).inst(\"synth.lead\")),\n)\n";
        let once = emit(&parse(src).expect("parses"));
        let twice = emit(&parse(&once).expect("reparses"));
        assert_eq!(once, twice, "canonical formatting must be a fixed point");
    }

    /// A syntax error is surfaced (the buffer is left untouched by the caller),
    /// never silently swallowed into empty output.
    #[test]
    fn syntax_error_surfaces() {
        // Unbalanced parens — not a valid program.
        assert!(parse("tracks(track(\"x\",").is_err());
    }
}
