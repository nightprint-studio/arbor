//! What the program looked like the moment it stopped, taken by Bennu itself — so the AI assistant gets it even
//! when it is not awake to ask: where, the top of the stack, the top frame's locals and the values the breakpoint
//! asked to capture. Read once, while the thread is still suspended; capped and masked later (see
//! `bennu_assistant::stopnote`).

use bennu_assistant::prelude::StopSnapshot;
use bennu_proto::prelude::DebugPause;

use crate::debug::Session;

/// Most locals read from the top frame (the note shows fewer; the rest is one `debug_variables` away).
const READ_LOCALS: usize = 40;

/// `Order.java:42 in OrderService.total`, from the first project frame or else the innermost one.
pub(crate) fn location_of(pause: &DebugPause) -> String {
    let Some(frame) = pause.frames.iter().find(|f| f.project).or_else(|| pause.frames.first()) else {
        return "an unknown place".to_string();
    };
    let file = frame.file.as_deref().map(|p| p.rsplit(['/', '\\']).next().unwrap_or(p)).unwrap_or("?");
    let at = frame.line.map_or_else(|| file.to_string(), |l| format!("{file}:{l}"));
    let class = if frame.class.is_empty() { String::new() } else { crate::debug::simple_name(&frame.class) };
    let place = if frame.name.is_empty() { format!("{class}.{}", frame.method) } else { frame.name.clone() };
    format!("{at} in {place}")
}

/// A stack frame as one short line.
fn frame_line(f: &bennu_proto::prelude::StackFrame) -> String {
    let file = f.file.as_deref().map(|p| p.rsplit(['/', '\\']).next().unwrap_or(p)).unwrap_or("?");
    let at = f.line.map_or_else(|| file.to_string(), |l| format!("{file}:{l}"));
    let name = if f.name.is_empty() { format!("{}.{}", f.class, f.method) } else { f.name.clone() };
    format!("#{} {name}({at}){}", f.index, if f.project { "" } else { " [library]" })
}

/// Why it stopped, in the words of the note.
fn reason_of(pause: &DebugPause) -> String {
    match (pause.reason.as_str(), &pause.exception) {
        ("exception", Some(e)) => format!("exception {e}"),
        (other, _) => other.to_string(),
    }
}

/// Take the snapshot. The session must already hold `pause` as its stop (frame 0 is readable).
pub(crate) fn take(session: &Session, thread: bennu_jdwp::prelude::Id, pause: &DebugPause, captures: &[String]) -> StopSnapshot {
    let top = session.frame_at(0).map(|(_, f)| f);
    let captured = captures
        .iter()
        .take(bennu_assistant::prelude::MAX_CAPTURES)
        .map(|expr| {
            let value = match &top {
                Some(frame) => crate::debug_value::render_path(session, thread, frame, expr).unwrap_or_else(|why| format!("(unreadable: {why})")),
                None => "(unreadable: no frame)".to_string(),
            };
            (expr.clone(), value)
        })
        .collect();
    let locals = crate::debug_value::variables(session, 0)
        .unwrap_or_default()
        .into_iter()
        .take(READ_LOCALS)
        .map(|v| (v.name, v.value))
        .collect();
    StopSnapshot {
        location: location_of(pause),
        reason: reason_of(pause),
        frames: pause.frames.iter().map(frame_line).collect(),
        captured,
        locals,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_proto::prelude::StackFrame;

    fn frame(index: u32, class: &str, method: &str, line: u32, file: Option<&str>, project: bool) -> StackFrame {
        StackFrame { index, class: class.into(), method: method.into(), name: String::new(), line: Some(line), file: file.map(String::from), project }
    }

    fn pause(frames: Vec<StackFrame>) -> DebugPause {
        DebugPause { session_id: "s".into(), thread: "1".into(), thread_name: "main".into(), reason: "breakpoint".into(), exception: None, frames, detail: Default::default() }
    }

    #[test]
    fn the_location_is_the_first_project_frame_not_the_library_one_on_top() {
        let p = pause(vec![
            frame(0, "java.util.ArrayList", "add", 10, None, false),
            frame(1, "com.acme.OrderService", "total", 42, Some("C:\\p\\src\\Order.java"), true),
        ]);
        assert_eq!(location_of(&p), "Order.java:42 in OrderService.total");
        assert_eq!(location_of(&pause(Vec::new())), "an unknown place");
    }

    #[test]
    fn an_exception_stop_names_the_exception() {
        let mut p = pause(vec![frame(0, "a.B", "m", 1, Some("B.java"), true)]);
        p.reason = "exception".into();
        p.exception = Some("IllegalStateException".into());
        assert_eq!(reason_of(&p), "exception IllegalStateException");
        p.exception = None;
        assert_eq!(reason_of(&p), "exception");
    }

    #[test]
    fn library_frames_are_marked_in_the_stack_lines() {
        assert_eq!(frame_line(&frame(0, "java.util.ArrayList", "add", 10, None, false)), "#0 java.util.ArrayList.add(?:10) [library]");
        assert_eq!(frame_line(&frame(1, "a.B", "m", 3, Some("/p/B.java"), true)), "#1 a.B.m(B.java:3)");
    }
}
