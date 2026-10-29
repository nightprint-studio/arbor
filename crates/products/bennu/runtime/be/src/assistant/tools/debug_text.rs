//! What the debug tools say to the model, and what they refuse to say.
//!
//! Pure functions over the debugger's own types: capped, so a thousand-element collection or a
//! recursive stack costs a handful of lines, and masked, so a variable that looks like a credential
//! never reaches a provider's servers. Kept apart from the tools so the rules can be tested without a
//! JVM.

use bennu_proto::prelude::{DebugPause, DebugValue, StackFrame, StopDetail};

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
    /// The line the breakpoint belongs on: the one asked for, or the next one that holds code.
    pub line: u32,
    pub text: String,
    /// The line that was asked for, when it held no code and `line` is the one after it.
    pub moved_from: Option<u32>,
}

/// How far past a line with no code the next one is looked for.
const MAX_SLIDE_LINES: usize = 8;

/// Line `line` of `source`, or — when it holds no code — the first of the next few that does.
///
/// The debugger would slide a breakpoint on a blank line or a lone brace on its own, but only once the class
/// is loaded, and the model is not told where it went: it believes the value it asked for is the one read. So
/// the slide is decided here, from the text, and said in the answer. The test is textual (a comment, a brace,
/// an annotation, a type or method header, a `case` label) and not the compiler's line table, so the running
/// program stays the authority on where it really stops.
pub(super) fn source_line(source: &str, line: u32) -> Result<SourceLine, String> {
    let rows: Vec<&str> = source.lines().collect();
    let from = (line as usize).saturating_sub(1);
    if line == 0 || from >= rows.len() {
        return Err(format!("the file has {} lines; there is no line {line}", rows.len()));
    }
    let code = code_flags(&rows);
    let Some(at) = (from..rows.len().min(from + 1 + MAX_SLIDE_LINES)).find(|&i| code[i]) else {
        return Err(format!("line {line} holds no code (a comment, blank line, annotation, brace or declaration) and neither do the {MAX_SLIDE_LINES} lines after it, so nothing was set. Read the file and pick a line with a statement."));
    };
    Ok(SourceLine { line: at as u32 + 1, text: clip(rows[at].trim(), MAX_VALUE_CHARS), moved_from: (at != from).then_some(line) })
}

/// For each row: whether the debugger can stop on it.
fn code_flags(rows: &[&str]) -> Vec<bool> {
    let mut in_block = false;
    rows.iter().map(|row| holds_code(&strip_comments(row, &mut in_block))).collect()
}

/// `row` without comments. `in_block` carries a `/* ... */` that spans rows. String and char literals are
/// skipped over, so a `//` inside a URL is not a comment.
fn strip_comments(row: &str, in_block: &mut bool) -> String {
    let chars: Vec<char> = row.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if *in_block {
            if c == '*' && next == Some('/') {
                *in_block = false;
                i += 1;
            }
        } else if c == '/' && next == Some('*') {
            *in_block = true;
            i += 1;
        } else if c == '/' && next == Some('/') {
            break;
        } else if c == '"' || c == '\'' {
            out.push(c);
            i += 1;
            while i < chars.len() && chars[i] != c {
                if chars[i] == '\\' {
                    out.push(chars[i]);
                    i += 1;
                }
                if let Some(&ch) = chars.get(i) {
                    out.push(ch);
                }
                i += 1;
            }
            if i < chars.len() {
                out.push(c);
            }
        } else {
            out.push(c);
        }
        i += 1;
    }
    out
}

/// Words that start a statement, so a line opening with one is code and not a method header.
const STATEMENT_WORDS: [&str; 15] = ["return", "new", "throw", "if", "for", "while", "switch", "catch", "synchronized", "assert", "yield", "else", "case", "do", "try"];
/// Lines that open a block and carry no code of their own.
const BARE_OPENERS: [&str; 6] = ["try", "do", "else", "finally", "static", "default"];

/// Whether a line, comments already removed, can hold a breakpoint.
fn holds_code(stripped: &str) -> bool {
    let text = stripped.trim();
    // Only closers and separators: `}`, `};`, `});`.
    if text.chars().all(|c| matches!(c, '{' | '}' | '(' | ')' | ';' | ',' | ' ' | '\t')) {
        return false;
    }
    if text.starts_with('@') {
        return false;
    }
    // `} else {` has no code; `} catch (E e) {` does (it stores the exception).
    let text = text.trim_start_matches('}').trim_start();
    let squeezed: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let head = squeezed.trim_end_matches('{').trim();
    if BARE_OPENERS.contains(&head) || head == "default:" || head == "else" {
        return false;
    }
    // `case X:` / `default:` labels of an old-style switch.
    if (text.starts_with("case ") || text.starts_with("default")) && squeezed.ends_with(':') {
        return false;
    }
    !is_declaration_header(&squeezed)
}

/// A type or method header that opens its body on the same line: `public class A {`, `int total(int a) {`.
fn is_declaration_header(text: &str) -> bool {
    if !text.ends_with('{') {
        return false;
    }
    let before_paren = text.split(['(', '{']).next().unwrap_or("");
    let words: Vec<&str> = before_paren.split_whitespace().collect();
    if words.iter().any(|w| matches!(*w, "class" | "interface" | "enum" | "record" | "@interface")) {
        return true;
    }
    // A method or constructor: modifiers/type and a name before the parenthesis, nothing that makes it a
    // statement (an assignment, a call on a receiver, a keyword).
    text.contains('(')
        && words.len() >= 2
        && !before_paren.contains(['=', '.'])
        && !before_paren.contains("->")
        && !words.iter().any(|w| STATEMENT_WORDS.contains(w))
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
    let mut out = format!("The program is STOPPED (suspended): {why}{thread}.");
    out.push_str(&detail_text(&p.detail));
    out
}

/// What the stop adds to where it is: new or leftover, the condition and what it read, what the last method
/// returned, the other stopped threads.
pub(super) fn detail_text(d: &StopDetail) -> String {
    let mut out = String::new();
    match d.origin.as_str() {
        "fresh" => out.push_str("\nA NEW stop: a breakpoint or exception reached just now."),
        "step" => out.push_str("\nA step finished (not a new breakpoint)."),
        "residual" => out.push_str("\nA LEFTOVER stop: it was hit while another stop was still being looked at, and waited its turn."),
        _ => {}
    }
    if d.suspends == "thread" {
        out.push_str("\nOnly this thread is suspended; the others are running.");
    }
    if !d.condition.is_empty() {
        let read = if d.condition_values.is_empty() { String::new() } else { format!(" — it read: {}", d.condition_values) };
        out.push_str(&format!("\nThe breakpoint's condition `{}` let it through{read}.", d.condition));
    }
    if let Some(r) = &d.returned {
        if r.method.is_empty() {
            out.push_str(&format!("\nReturn value: {}.", r.value));
        } else {
            let ty = if r.type_name.is_empty() { String::new() } else { format!(": {}", r.type_name) };
            out.push_str(&format!("\nThe method the step left, `{}`, returned{ty} = {}.", r.method, clip(&r.value, MAX_VALUE_CHARS)));
        }
    }
    if !d.also_stopped.is_empty() {
        let others: Vec<String> = d.also_stopped.iter().map(|t| format!("`{}` ({}, {})", t.thread_name, t.reason, t.location)).collect();
        out.push_str(&format!("\nAlso stopped, waiting: {}. debug_select_thread switches to one; debug_resume moves on to the next.", others.join("; ")));
    }
    out
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
    use bennu_proto::prelude::{ReturnedValue, StoppedThread};

    #[test]
    fn a_stop_says_if_it_is_new_what_let_it_through_and_who_else_waits() {
        let d = StopDetail {
            origin: "residual".into(),
            suspends: "thread".into(),
            condition: "total > 100".into(),
            condition_values: "total = 250".into(),
            returned: Some(ReturnedValue { method: "com.acme.A.price".into(), type_name: "int".into(), value: "7".into() }),
            also_stopped: vec![StoppedThread { thread: "2".into(), thread_name: "worker-2".into(), reason: "breakpoint".into(), location: "A.java:9".into() }],
        };
        let text = detail_text(&d);
        assert!(text.contains("LEFTOVER"), "{text}");
        assert!(text.contains("Only this thread is suspended"), "{text}");
        assert!(text.contains("`total > 100` let it through — it read: total = 250"), "{text}");
        assert!(text.contains("`com.acme.A.price`, returned: int = 7"), "{text}");
        assert!(text.contains("`worker-2` (breakpoint, A.java:9)"), "{text}");
        assert!(detail_text(&StopDetail { origin: "fresh".into(), ..StopDetail::default() }).contains("NEW"));
        assert!(detail_text(&StopDetail { origin: "step".into(), ..StopDetail::default() }).contains("not a new breakpoint"));
        assert_eq!(detail_text(&StopDetail::default()), "", "an engine that says nothing adds nothing");
        let unread = StopDetail { returned: Some(ReturnedValue { method: String::new(), type_name: String::new(), value: "no method returned during the step".into() }), ..StopDetail::default() };
        assert!(detail_text(&unread).contains("Return value: no method returned"));
    }

    fn frame(index: u32, class: &str, method: &str, line: Option<u32>, file: Option<&str>, project: bool) -> StackFrame {
        StackFrame { index, class: class.into(), method: method.into(), name: String::new(), line, file: file.map(Into::into), project }
    }

    fn pause(reason: &str, exception: Option<&str>, frames: Vec<StackFrame>) -> DebugPause {
        DebugPause { session_id: "s".into(), thread: "1".into(), thread_name: "main".into(), reason: reason.into(), exception: exception.map(Into::into), frames, detail: Default::default() }
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
    fn a_line_with_code_is_kept_and_one_without_slides_to_the_next() {
        let src = "class A {\n    // note\n    int f() {\n        return 1;\n    }\n}\n";
        let code = source_line(src, 4).unwrap();
        assert_eq!((code.line, code.text.as_str(), code.moved_from), (4, "return 1;", None));
        let comment = source_line(src, 2).unwrap();
        assert_eq!((comment.line, comment.moved_from), (4, Some(2)), "a comment and a method header slide to the statement");
        assert!(source_line(src, 5).unwrap_err().contains("holds no code"), "a lone brace with nothing after it");
        assert!(source_line(src, 99).unwrap_err().contains("6 lines"));
    }

    #[test]
    fn what_holds_no_code_is_told_from_what_does() {
        for none in ["", "{", "}", "});", "// c", "/* c */", "@Override", "public class A {", "static {", "} else {", "} finally {", "case 1:", "default:", "public int total(int a) throws IOException {", "private Order(String id) {"] {
            assert!(!holds_code(&strip_comments(none, &mut false)), "{none:?}");
        }
        for code in ["return 1;", "int a = 0;", "if (a > 0) {", "} else if (b) {", "} catch (Exception e) {", "for (int i = 0; i < n; i++) {", "list.forEach(x -> {", "Foo f = new Foo() {", "synchronized (lock) {", "String u = \"http://x\"; // note", "int[] a = {"] {
            assert!(holds_code(&strip_comments(code, &mut false)), "{code:?}");
        }
    }

    #[test]
    fn a_block_comment_spanning_rows_holds_no_code() {
        let src = "class A {\n  /*\n   * x = 1;\n   */\n  int a = 1;\n}\n";
        assert_eq!(source_line(src, 3).unwrap().line, 5);
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
