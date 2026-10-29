//! What the debug tools say to the model, and what they refuse to say.
//!
//! Pure functions over the debugger's own types: capped, so a thousand-element collection or a
//! recursive stack costs a handful of lines, and masked, so a variable that looks like a credential
//! never reaches a provider's servers. Kept apart from the tools so the rules can be tested without a
//! JVM.

use bennu_proto::prelude::{DebugPause, DebugValue, StackFrame};

use crate::debug_path::{self, Step};

/// Frames listed from a stop. A recursive failure has hundreds; the first screen is what matters.
pub(super) const MAX_STACK_FRAMES: usize = 15;
/// Variables, fields or elements listed in one answer.
pub(super) const MAX_ROWS: usize = 40;
/// Characters of one value. A long string is cut and says so.
pub(super) const MAX_VALUE_CHARS: usize = 160;

/// Name fragments (compared lower-case, with `_` and `-` removed) that mark a variable as a secret.
///
/// Deliberately broad: `tokenCount` is masked too, and the model can ask the user for it. Showing a
/// bearer token to a provider cannot be taken back, hiding a counter can.
const SECRET_FRAGMENTS: [&str; 11] = [
    "password", "passwd", "secret", "token", "apikey", "credential", "privatekey", "passphrase", "authorization", "accesskey", "sessionid",
];

/// What stands in for a masked value.
pub(super) const MASKED: &str = "«masked: the name looks like a secret»";

/// Whether a variable, field or path segment is named like a credential.
pub(super) fn is_secret_name(name: &str) -> bool {
    let flat: String = name.chars().filter(|c| *c != '_' && *c != '-').flat_map(char::to_lowercase).collect();
    SECRET_FRAGMENTS.iter().any(|f| flat.contains(f))
}

/// The first segment of a parsed path that is named like a secret.
pub(super) fn secret_segment(steps: &[Step]) -> Option<&str> {
    steps.iter().find_map(|s| match s {
        Step::Field(name) if is_secret_name(name) => Some(name.as_str()),
        _ => None,
    })
}

/// `text` cut to `max` characters, with an ellipsis when it was.
pub(super) fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let kept: String = text.chars().take(max).collect();
    format!("{kept}…")
}

/// Whether `expr` assigns: a `=` that is not part of `==`, `!=`, `<=` or `>=`.
fn assigns(expr: &str) -> bool {
    let chars: Vec<char> = expr.chars().collect();
    chars.iter().enumerate().any(|(i, c)| {
        *c == '='
            && chars.get(i + 1) != Some(&'=')
            && !(i > 0 && matches!(chars[i - 1], '=' | '!' | '<' | '>'))
    })
}

/// The rule for what `debug_evaluate` runs, as code.
///
/// **Only a path is evaluated** — `order`, `order.customer.name`, `items[2]` — and a path is read
/// out of the stopped program's fields and array slots: no method is called, no constructor runs,
/// nothing is assigned, so evaluating one cannot change the program or run its code. Everything else
/// is refused by name before it reaches the debugger. That is stricter than "ask the user first":
/// there is no approval that makes a call run, because there is nothing that would run it.
pub(super) fn parse_expression(expression: &str) -> Result<Vec<Step>, String> {
    let e = expression.trim();
    if e.is_empty() {
        return Err("`expression` is required".into());
    }
    if e.chars().count() > 200 {
        return Err("the expression is too long: it is a path such as `order.customer.name`".into());
    }
    if assigns(e) || e.contains("++") || e.contains("--") {
        return Err("assignments and increments are never evaluated: this tool reads values and cannot change them".into());
    }
    if e.contains('(') || e.starts_with("new ") || e.contains(" new ") {
        return Err("method calls and constructors are never evaluated, because they would run code in the program being debugged. Read the fields they would use instead: `order.items[0].price`".into());
    }
    let steps = debug_path::parse(e, debug_path::JAVA).map_err(|why| format!("{why}. Only a path is evaluated: a name, `.field` and `[0]`"))?;
    if let Some(name) = secret_segment(&steps) {
        return Err(format!("`{name}` looks like a secret, so its value is not shown to the assistant"));
    }
    Ok(steps)
}

// ── Small answers ────────────────────────────────────────────────────────────────────────────────

/// ` with exit code 1`, ` (it was killed)` or nothing, for "the program has ended…".
pub(super) fn exit_text(exit: Option<Option<i32>>) -> String {
    match exit {
        Some(Some(code)) => format!(" with exit code {code}"),
        Some(None) => " (it was killed, so it has no exit code)".to_string(),
        None => String::new(),
    }
}

/// A wait's length in seconds: what was asked for, between 1 and `max`, or `default`.
pub(super) fn wait_seconds(asked: Option<u64>, default: u64, max: u64) -> u64 {
    asked.unwrap_or(default).clamp(1, max)
}

/// A frame number checked against the stack that is there.
pub(super) fn check_frame(frame: usize, count: usize) -> Result<(), String> {
    if frame < count {
        return Ok(());
    }
    Err(format!("there is no frame {frame}: the stack has {count} (0 is the innermost)"))
}

/// A source line, shown back to the model so it can see what it put a breakpoint on.
#[derive(Debug)]
pub(super) struct SourceLine {
    pub text: String,
    /// Set when the line holds no code, so the breakpoint will bind to the next line that does.
    pub warning: Option<String>,
}

/// Line `line` of `source`, trimmed and cut, with a warning when it cannot hold a breakpoint.
pub(super) fn source_line(source: &str, line: u32) -> Result<SourceLine, String> {
    let total = source.lines().count();
    let Some(raw) = source.lines().nth((line as usize).saturating_sub(1)) else {
        return Err(format!("the file has {total} lines; there is no line {line}"));
    };
    let trimmed = raw.trim();
    let no_code = trimmed.is_empty()
        || trimmed.starts_with("//")
        || trimmed.starts_with("/*")
        || trimmed.starts_with('*')
        || trimmed.starts_with('@')
        || matches!(trimmed, "{" | "}" | "};");
    let warning = no_code.then(|| format!("line {line} holds no code (a comment, blank line, annotation or lone brace): the debugger will bind to the next line that has code. Prefer a line with a statement."));
    Ok(SourceLine { text: clip(trimmed, MAX_VALUE_CHARS), warning })
}

// ── Stack ────────────────────────────────────────────────────────────────────────────────────────

/// `#0 com.acme.Order.total(Order.java:42)`, with `[library]` on frames that are not the project's.
pub(super) fn frame_line(f: &StackFrame) -> String {
    let file = f.file.as_deref().map(|p| p.rsplit(['/', '\\']).next().unwrap_or(p)).unwrap_or("?");
    let at = match f.line {
        Some(line) => format!("{file}:{line}"),
        None => file.to_string(),
    };
    let name = if f.name.is_empty() { format!("{}.{}", f.class, f.method) } else { f.name.clone() };
    let origin = if f.project { "" } else { " [library]" };
    format!("#{} {name}({at}){origin}", f.index)
}

/// One line on why and where the program stopped.
pub(super) fn stop_summary(p: &DebugPause) -> String {
    let why = match (&*p.reason, &p.exception) {
        ("exception", Some(e)) => format!("an exception was thrown: {e}"),
        ("step", _) => "a step finished".to_string(),
        (other, _) => format!("a {other} was hit"),
    };
    let thread = if p.thread_name.is_empty() { String::new() } else { format!(" on thread `{}`", p.thread_name) };
    format!("The program is STOPPED (suspended): {why}{thread}.")
}

/// The stop and its frames, at most `max_frames`, with the project's source file of the first frame
/// that has one so the model can open it.
pub(super) fn stack_text(p: &DebugPause, max_frames: usize) -> String {
    let mut out = stop_summary(p);
    out.push('\n');
    for f in p.frames.iter().take(max_frames) {
        out.push_str(&frame_line(f));
        out.push('\n');
    }
    if p.frames.len() > max_frames {
        out.push_str(&format!("… {} more frames (the outer ones)\n", p.frames.len() - max_frames));
    }
    if let Some(file) = p.frames.iter().find(|f| f.project).and_then(|f| f.file.as_deref()) {
        out.push_str(&format!("Source of the first project frame: {file}\n"));
    }
    out.trim_end().to_string()
}

// ── Values ───────────────────────────────────────────────────────────────────────────────────────

/// `name: Type = value`, masked when the name looks like a secret, with a note when there is more
/// inside it.
pub(super) fn row_text(row: &DebugValue) -> String {
    let secret = is_secret_name(&row.name);
    let value = if secret { MASKED.to_string() } else { clip(&row.value, MAX_VALUE_CHARS) };
    let ty = if row.type_name.is_empty() { String::new() } else { format!(": {}", row.type_name) };
    let more = if row.object.is_some() && !secret { "  (has fields: use debug_variables with `path`)" } else { "" };
    format!("{}{ty} = {value}{more}", row.name)
}

/// Rows as lines, at most [`MAX_ROWS`], saying how many were left out.
pub(super) fn rows_text(rows: &[DebugValue]) -> String {
    let mut out: Vec<String> = rows.iter().take(MAX_ROWS).map(row_text).collect();
    if rows.len() > MAX_ROWS {
        out.push(format!("… {} more not shown", rows.len() - MAX_ROWS));
    }
    if out.is_empty() {
        return "(nothing in scope)".to_string();
    }
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(index: u32, class: &str, method: &str, line: Option<u32>, file: Option<&str>, project: bool) -> StackFrame {
        StackFrame { index, class: class.into(), method: method.into(), name: String::new(), line, file: file.map(Into::into), project }
    }

    fn pause(reason: &str, exception: Option<&str>, frames: Vec<StackFrame>) -> DebugPause {
        DebugPause { session_id: "s".into(), thread: "1".into(), thread_name: "main".into(), reason: reason.into(), exception: exception.map(Into::into), frames }
    }

    fn row(name: &str, ty: &str, value: &str, object: bool) -> DebugValue {
        DebugValue { name: name.into(), kind: "local".into(), type_name: ty.into(), value: value.into(), object: object.then(|| "7".to_string()) }
    }

    #[test]
    fn names_that_look_like_credentials_are_secret_whatever_the_style() {
        for n in ["password", "dbPassword", "DB_PASSWD", "apiKey", "api_key", "api-key", "authToken", "clientSecret", "Authorization", "privateKey", "sessionId"] {
            assert!(is_secret_name(n), "{n}");
        }
        for n in ["order", "total", "key", "name", "customerId", "items"] {
            assert!(!is_secret_name(n), "{n}");
        }
    }

    #[test]
    fn a_secret_variable_is_masked_in_the_listing_and_never_offered_for_expansion() {
        let line = row_text(&row("password", "String", "\"hunter2\"", false));
        assert!(!line.contains("hunter2") && line.contains("masked"), "{line}");
        let obj = row_text(&row("credentials", "Creds", "Creds@1", true));
        assert!(!obj.contains("Creds@1") && !obj.contains("has fields"), "{obj}");
        let plain = row_text(&row("order", "Order", "Order@1f", true));
        assert!(plain.contains("Order@1f") && plain.contains("has fields"));
    }

    #[test]
    fn only_a_path_is_evaluated() {
        assert!(parse_expression("order.customer.name").is_ok());
        assert!(parse_expression("items[2].price").is_ok());
        for bad in ["order.total()", "list.size()", "x = 5", "i++", "--i", "new Foo()", "a + b", "order.setTotal(1)", "System.exit(0)"] {
            assert!(parse_expression(bad).is_err(), "{bad} must be refused");
        }
    }

    #[test]
    fn a_refusal_says_why_in_words_the_model_can_act_on() {
        assert!(parse_expression("x = 5").unwrap_err().contains("never evaluated"));
        assert!(parse_expression("order.total()").unwrap_err().contains("would run code"));
        assert!(parse_expression("i++").unwrap_err().contains("increments"));
        assert!(parse_expression("a b").unwrap_err().contains("path"));
        assert!(parse_expression("  ").unwrap_err().contains("required"));
        assert!(parse_expression(&"a".repeat(300)).unwrap_err().contains("too long"));
    }

    #[test]
    fn comparison_operators_are_not_assignments() {
        assert!(!assigns("a == b") && !assigns("a != b") && !assigns("a <= b") && !assigns("a >= b"));
        assert!(assigns("a = b") && assigns("a=b"));
    }

    #[test]
    fn a_path_through_a_secret_is_refused() {
        assert!(parse_expression("config.password").unwrap_err().contains("secret"));
        assert!(parse_expression("user.apiKey.length").unwrap_err().contains("secret"));
    }

    #[test]
    fn a_stack_is_capped_and_says_how_much_was_left_out() {
        let frames: Vec<StackFrame> = (0..40).map(|i| frame(i, "com.acme.A", "go", Some(10 + i), Some("C:/p/A.java"), true)).collect();
        let text = stack_text(&pause("breakpoint", None, frames), MAX_STACK_FRAMES);
        assert_eq!(text.lines().filter(|l| l.starts_with('#')).count(), MAX_STACK_FRAMES);
        assert!(text.contains("25 more frames"));
        assert!(text.contains("STOPPED") && text.contains("breakpoint was hit") && text.contains("main"));
        assert!(text.contains("Source of the first project frame: C:/p/A.java"));
    }

    #[test]
    fn the_stop_reason_is_in_plain_words() {
        assert!(stop_summary(&pause("exception", Some("java.lang.NullPointerException"), vec![])).contains("NullPointerException"));
        assert!(stop_summary(&pause("step", None, vec![])).contains("step finished"));
    }

    #[test]
    fn a_frame_names_its_file_and_marks_library_code() {
        assert_eq!(frame_line(&frame(0, "com.acme.Order", "total", Some(42), Some("C:\\p\\src\\Order.java"), true)), "#0 com.acme.Order.total(Order.java:42)");
        assert_eq!(frame_line(&frame(3, "java.util.ArrayList", "add", None, None, false)), "#3 java.util.ArrayList.add(?) [library]");
    }

    #[test]
    fn how_a_program_ended_reads_naturally() {
        assert_eq!(exit_text(Some(Some(1))), " with exit code 1");
        assert!(exit_text(Some(None)).contains("killed"));
        assert_eq!(exit_text(None), "");
    }

    #[test]
    fn a_wait_and_a_frame_are_bounded() {
        assert_eq!(wait_seconds(None, 60, 300), 60);
        assert_eq!(wait_seconds(Some(0), 60, 300), 1);
        assert_eq!(wait_seconds(Some(10_000), 60, 300), 300);
        assert!(check_frame(2, 3).is_ok());
        assert!(check_frame(3, 3).unwrap_err().contains("stack has 3"));
    }

    #[test]
    fn a_breakpoint_line_is_echoed_and_a_line_without_code_is_flagged() {
        let src = "class A {\n    // note\n    int f() {\n        return 1;\n    }\n}\n";
        let code = source_line(src, 4).unwrap();
        assert_eq!(code.text, "return 1;");
        assert!(code.warning.is_none());
        assert!(source_line(src, 2).unwrap().warning.unwrap().contains("no code"));
        assert!(source_line(src, 5).unwrap().warning.is_some(), "a lone brace");
        assert!(source_line(src, 99).unwrap_err().contains("6 lines"));
    }

    #[test]
    fn listings_are_capped_and_long_values_cut() {
        let rows: Vec<DebugValue> = (0..60).map(|i| row(&format!("v{i}"), "int", &i.to_string(), false)).collect();
        let text = rows_text(&rows);
        assert_eq!(text.lines().count(), MAX_ROWS + 1);
        assert!(text.contains("20 more not shown"));
        assert!(row_text(&row("s", "String", &"x".repeat(500), false)).chars().count() < 200);
        assert_eq!(rows_text(&[]), "(nothing in scope)");
        assert_eq!(clip("short", 10), "short");
        assert_eq!(clip("abcdef", 3), "abc…");
    }
}
