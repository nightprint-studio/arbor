//! Durations as the tools print them.

use serde_json::Value;

/// A duration in milliseconds from whatever `jfr print --json` wrote: an ISO-8601 string
/// (`PT0.0123S`, `PT1M2.5S`), a `"12.3 ms"` style string, or a bare number (nanoseconds).
pub fn duration_ms(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(0.0) / 1_000_000.0,
        Value::String(s) => parse_duration_str(s),
        _ => 0.0,
    }
}

pub(crate) fn parse_duration_str(s: &str) -> f64 {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("PT").or_else(|| s.strip_prefix("pt")) {
        let mut ms = 0.0;
        let mut num = String::new();
        for c in rest.chars() {
            match c {
                '0'..='9' | '.' | '-' => num.push(c),
                'H' | 'h' => ms += take(&mut num) * 3_600_000.0,
                'M' | 'm' => ms += take(&mut num) * 60_000.0,
                'S' | 's' => ms += take(&mut num) * 1_000.0,
                _ => {}
            }
        }
        return ms;
    }
    let split = s.find(|c: char| c.is_alphabetic()).unwrap_or(s.len());
    let value: f64 = s[..split].trim().parse().unwrap_or(0.0);
    match s[split..].trim() {
        "ns" => value / 1_000_000.0,
        "us" | "µs" => value / 1_000.0,
        "s" => value * 1_000.0,
        "m" | "min" => value * 60_000.0,
        _ => value, // "ms" or bare
    }
}

fn take(num: &mut String) -> f64 {
    let v = num.parse().unwrap_or(0.0);
    num.clear();
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_iso_and_unit_forms() {
        assert!((duration_ms(&json!("PT0.0123S")) - 12.3).abs() < 1e-9);
        assert!((duration_ms(&json!("PT1M2.5S")) - 62_500.0).abs() < 1e-9);
        assert!((duration_ms(&json!("12.5 ms")) - 12.5).abs() < 1e-9);
        assert!((duration_ms(&json!("500 us")) - 0.5).abs() < 1e-9);
        assert!((duration_ms(&json!(2_000_000)) - 2.0).abs() < 1e-9);
        assert_eq!(duration_ms(&json!(null)), 0.0);
    }
}
