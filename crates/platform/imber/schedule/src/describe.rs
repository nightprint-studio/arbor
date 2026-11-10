//! A schedule in a few words, for the approval card and for a tool's answer.

use arbor_scheduler::prelude::{format_local, Job, JobAction};
use serde_json::{json, Value};

/// The `when` of a schedule tool call in a few words.
pub fn describe_when(when: Option<&Value>) -> String {
    let Some(w) = when else { return "(no time given)".into() };
    let s = |k: &str| w.get(k).and_then(Value::as_str).unwrap_or("?");
    match s("kind") {
        "once" => format!("once, {}", s("at")),
        "daily" => format!("every day at {}", s("time")),
        "weekly" => format!("every {} at {}", w.get("days").and_then(Value::as_array).map(|d| d.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", ")).unwrap_or_default(), s("time")),
        "every" => format!("every {} min", w.get("minutes").and_then(Value::as_u64).unwrap_or(0)),
        "cron" => format!("cron {}", s("expr")),
        other => other.to_string(),
    }
}

/// The first `max` characters of `text`, one line, with `…` when it was cut.
pub fn brief(text: &str, max: usize) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= max {
        return line;
    }
    let mut out: String = line.chars().take(max).collect();
    out.push('…');
    out
}

/// The one-line view of a job a tool answers with.
pub fn job_json<A: JobAction>(j: &Job<A>) -> Value {
    json!({
        "id": j.id, "name": j.name, "enabled": j.enabled, "action": j.action.word(), "mode": j.mode,
        "next_run": j.next_run_ms.map(format_local), "runs": j.runs,
        "last_run": j.last_run.as_ref().map(|l| json!({ "at": format_local(l.at_ms), "ok": l.ok, "detail": l.detail })),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_when_reads_as_words() {
        assert_eq!(describe_when(Some(&json!({"kind": "weekly", "days": ["mon", "fri"], "time": "09:00"}))), "every mon, fri at 09:00");
        assert_eq!(describe_when(Some(&json!({"kind": "every", "minutes": 30}))), "every 30 min");
        assert_eq!(describe_when(Some(&json!({"kind": "daily", "time": "08:15"}))), "every day at 08:15");
        assert_eq!(describe_when(Some(&json!({"kind": "once", "at": "2026-11-05T09:00"}))), "once, 2026-11-05T09:00");
        assert_eq!(describe_when(None), "(no time given)");
    }

    #[test]
    fn a_brief_is_one_line_and_bounded() {
        assert_eq!(brief("a  b\n c", 10), "a b c");
        assert_eq!(brief("abcdef", 3), "abc…");
    }
}
