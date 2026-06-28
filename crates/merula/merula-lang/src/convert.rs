//! Value coercions and the `f64 → Time` conversion.
//!
//! Argument checking happens here so the call sites read declaratively and every
//! mismatch becomes a located [`LangError`]. (Coercing a value to a
//! [`Transform`](crate::value::Transform) needs the evaluator and lives in
//! `eval/transforms.rs` instead.)

use merula_pattern::prelude::{ControlMap, Param, Pattern, SourceSpan, Time};

use crate::error::{LangError, LangErrorKind, Result};
use crate::value::Value;

fn type_err(span: SourceSpan, expected: &str, got: &Value) -> LangError {
    LangError::at(
        span,
        LangErrorKind::Type {
            expected: expected.to_string(),
            got: got.type_name().to_string(),
        },
    )
}

/// Require a number.
pub fn as_number(v: &Value, span: SourceSpan) -> Result<f64> {
    match v {
        Value::Number(n) => Ok(*n),
        other => Err(type_err(span, "number", other)),
    }
}

/// Require an integer-valued number.
pub fn as_int(v: &Value, span: SourceSpan) -> Result<i64> {
    let n = as_number(v, span)?;
    if n.fract() != 0.0 {
        return Err(LangError::at(
            span,
            LangErrorKind::Type {
                expected: "integer".to_string(),
                got: format!("{n}"),
            },
        ));
    }
    Ok(n as i64)
}

/// Require a string.
pub fn as_str(v: &Value, span: SourceSpan) -> Result<String> {
    match v {
        Value::Str(s) => Ok(s.clone()),
        other => Err(type_err(span, "string", other)),
    }
}

/// Require a musical pattern. An `arrange(...)` result coerces to its flattened
/// pattern (its section layout is only meaningful when fed straight to `track`).
pub fn as_pattern(v: Value, span: SourceSpan) -> Result<Pattern<ControlMap>> {
    match v {
        Value::Pattern(p) => Ok(p),
        Value::Arrangement(p, _, _) => Ok(p),
        other => Err(type_err(span, "pattern", &other)),
    }
}

/// Coerce to a patternised control [`Param`]: a constant number or a numeric
/// signal (`rand`). A musical pattern is **not** a numeric parameter.
pub fn as_param(v: &Value, span: SourceSpan) -> Result<Param> {
    match v {
        Value::Number(n) => Ok(Param::Const(*n)),
        Value::NumSignal(p) => Ok(Param::Pat(p.clone())),
        other => Err(type_err(span, "number or numeric signal", other)),
    }
}

/// Convert a host number to exact cycle [`Time`].
///
/// Musical times are dyadic (`0.5`, `0.25`, `0.125`) and a few simple tuplets
/// (thirds, fifths); we recognise those exactly and fall back to a fine fixed
/// denominator otherwise. `f64` literals like `0.1` were never exact in the
/// source anyway, so the fallback's rounding is acceptable.
pub fn f64_to_time(x: f64) -> Time {
    const DENS: &[i64] = &[
        1, 2, 3, 4, 5, 6, 7, 8, 9, 12, 16, 24, 32, 48, 64, 128, 256, 512, 1024,
    ];
    for &d in DENS {
        let scaled = x * d as f64;
        if (scaled - scaled.round()).abs() < 1e-9 {
            return Time::new(scaled.round() as i64, d);
        }
    }
    Time::new((x * 1_000_000.0).round() as i64, 1_000_000)
}
