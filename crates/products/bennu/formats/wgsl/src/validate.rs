//! Compiling the shader, in the editor.
//!
//! `naga` is not "a WGSL parser we picked" — it is the front end wgpu runs, so what it
//! rejects is what the GPU pipeline would have rejected, at the same place and with the
//! same words. That is the difference between a linter and a compiler, and it is why a
//! squiggle here is worth acting on rather than worth checking.
//!
//! Two passes, and they fail differently on purpose:
//!
//! * **parse** — the grammar. One error, because a parser that has lost its place invents
//!   the rest; the first one is the one to fix.
//! * **validate** — the semantics: types, bindings, entry-point signatures, the uniformity
//!   rules that decide whether a `textureSample` is legal where it stands. This is the pass
//!   that catches what compiles-looking WGSL gets wrong, and the reason to run a real
//!   compiler instead of a highlighter.

use naga::front::wgsl;
use naga::valid::{Capabilities, ValidationFlags, Validator};

/// How bad a problem is. Only two, because `naga` has only two: it either accepted the
/// module or it did not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WgslSeverity {
    Error,
    Warning,
}

/// One problem, located in the source by **UTF-8 byte offsets** — the same coordinate the
/// rest of Bennu's backend speaks, so the editor maps it exactly as it maps every other
/// diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WgslDiagnostic {
    pub start: usize,
    pub end: usize,
    pub severity: WgslSeverity,
    pub message: String,
}

/// A whole-file diagnostic, for a failure that carries no usable span. Better than
/// dropping it: "this shader does not compile and here is why" at line 1 is an answer, and
/// silence is not.
fn whole_file(message: String) -> WgslDiagnostic {
    WgslDiagnostic { start: 0, end: 0, severity: WgslSeverity::Error, message }
}

/// What came of running the compiler over a file.
///
/// Two outcomes and not one, because "no problems" and "not checked" are different
/// answers and an editor that renders both as a clean file is lying about one of them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WgslReport {
    pub diagnostics: Vec<WgslDiagnostic>,
    /// Set when the file was **not** compiled, with the reason. See [`preprocessor_reason`].
    pub skipped: Option<String>,
}

/// Why this file cannot be compiled on its own, if it cannot.
///
/// Bevy's shaders are not WGSL — they are WGSL run through **naga_oil**, whose `#import`,
/// `#ifdef` and `#define_import_path` lines are not in the grammar at all. A file that
/// declares an import is a *fragment of a composed module*: half the identifiers it uses
/// are declared somewhere else entirely, so compiling it alone would produce a wall of
/// "unknown identifier" on a shader that is perfectly correct.
///
/// That wall is worse than no diagnostics, which is why this exists: a file with
/// preprocessor directives gets its outline, its completion and its find-usages from the
/// scanner, and no compiler errors at all. Being told "not checked, because it is composed"
/// is a true statement; a hundred red squiggles on working code is not.
pub fn preprocessor_reason(source: &str) -> Option<String> {
    for line in source.lines() {
        let t = line.trim_start();
        if !t.starts_with('#') {
            continue;
        }
        let directive = t[1..].split(|c: char| !c.is_ascii_alphanumeric() && c != '_').next().unwrap_or("");
        return Some(match directive {
            "import" | "define_import_path" => {
                "composed with naga_oil (`#import`) — checked as part of the shader that imports it"
            }
            "ifdef" | "ifndef" | "if" | "else" | "endif" | "define" => {
                "uses naga_oil preprocessor directives — what compiles depends on the shader defs"
            }
            _ => "uses preprocessor directives that are not part of WGSL",
        }
        .to_string());
    }
    None
}

/// Parse and validate `source`.
///
/// A file that cannot be compiled on its own comes back with no diagnostics and a reason
/// — see [`preprocessor_reason`].
pub fn validate(source: &str) -> WgslReport {
    if let Some(reason) = preprocessor_reason(source) {
        return WgslReport { diagnostics: Vec::new(), skipped: Some(reason) };
    }
    WgslReport { diagnostics: compile(source), skipped: None }
}

/// The compiler proper, for a file that really is plain WGSL.
fn compile(source: &str) -> Vec<WgslDiagnostic> {
    let module = match wgsl::parse_str(source) {
        Ok(m) => m,
        Err(e) => {
            // `labels` carries the spans; the first is where the parser gave up, and the
            // rest are the context it was tracking ("expected `;`, found …" plus "opened
            // here"). All of them are worth showing — the second one is frequently where
            // the mistake actually is.
            let mut out: Vec<WgslDiagnostic> = e
                .labels()
                .filter_map(|(span, note)| {
                    let range = span.to_range()?;
                    Some(WgslDiagnostic {
                        start: range.start,
                        end: range.end,
                        severity: WgslSeverity::Error,
                        message: if note.is_empty() {
                            e.message().to_string()
                        } else {
                            format!("{}: {note}", e.message())
                        },
                    })
                })
                .collect();
            if out.is_empty() {
                out.push(whole_file(e.message().to_string()));
            }
            return out;
        }
    };

    // Every capability, because the editor does not know which adapter this shader will
    // run on and refusing `f16` here would be reporting a problem the user's machine does
    // not have. The device rejects what it cannot do; this pass is about the shader.
    let mut validator = Validator::new(ValidationFlags::all(), Capabilities::all());
    match validator.validate(&module) {
        Ok(_) => Vec::new(),
        Err(err) => {
            let text = err.emit_to_string(source);
            let mut out: Vec<WgslDiagnostic> = err
                .spans()
                .filter_map(|(span, note)| {
                    let range = span.to_range()?;
                    Some(WgslDiagnostic {
                        start: range.start,
                        end: range.end,
                        severity: WgslSeverity::Error,
                        message: note.clone(),
                    })
                })
                .collect();
            if out.is_empty() {
                // A validation failure with no span at all — a module-level rule, e.g. two
                // entry points sharing a name. The rendered form still says what it is.
                out.push(whole_file(first_line(&text)));
            }
            out
        }
    }
}

/// The first non-empty line of a rendered error — the sentence, without the source
/// excerpt naga draws underneath it. The editor has the source; it needs the sentence.
fn first_line(text: &str) -> String {
    text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("invalid shader").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_valid_shader_reports_nothing() {
        let src = r#"
@vertex
fn vs_main(@location(0) pos: vec3<f32>) -> @builtin(position) vec4<f32> {
    return vec4<f32>(pos, 1.0);
}
"#;
        assert_eq!(validate(src), WgslReport::default());
    }

    #[test]
    fn a_syntax_error_is_located() {
        // Missing the closing paren on the return type.
        let src = "fn f() -> vec4<f32 { return vec4<f32>(0.0); }";
        let diags = validate(src).diagnostics;
        assert!(!diags.is_empty(), "a malformed shader must report something");
        assert!(diags.iter().all(|d| d.severity == WgslSeverity::Error));
        // Located inside the file rather than dumped at the top.
        assert!(diags.iter().any(|d| d.end > 0 && d.end <= src.len()));
    }

    #[test]
    fn a_bevy_shader_is_not_compiled_and_says_why() {
        let src = r#"
#import bevy_pbr::mesh_view_bindings::view

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    return view.clip_from_world[0];
}
"#;
        let report = validate(src);
        assert!(
            report.diagnostics.is_empty(),
            "a composed shader must not be flooded with errors about identifiers that live \
             in the module it imports: {:?}",
            report.diagnostics
        );
        assert!(report.skipped.is_some(), "and it must say it was not checked");
    }

    #[test]
    fn conditional_compilation_is_not_compiled_either() {
        let src = "#ifdef SIXTEEN_BYTE_ALIGNMENT
fn f() {}
#endif
";
        assert!(validate(src).skipped.is_some());
    }

    #[test]
    fn a_type_error_survives_the_grammar() {
        // Parses fine; the types do not agree — which is the class of mistake a
        // highlighter cannot see and a compiler can.
        let src = "fn f() -> f32 { return vec2<f32>(1.0, 2.0); }";
        let diags = validate(src).diagnostics;
        assert!(!diags.is_empty(), "a well-formed shader with wrong types must be reported");
    }

    #[test]
    fn an_unknown_identifier_is_reported_where_it_is_written() {
        let src = "fn f() -> f32 { return nope; }";
        let diags = validate(src).diagnostics;
        assert!(!diags.is_empty());
        let at = src.find("nope").unwrap();
        assert!(
            diags.iter().any(|d| d.start <= at && at < d.end.max(at + 1)),
            "the span should cover the name that is unknown, got {diags:?}"
        );
    }
}
