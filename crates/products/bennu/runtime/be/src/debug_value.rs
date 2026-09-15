//! Reading a stopped program: variables, what is inside them, and watches.
//!
//! Everything here runs against a **suspended** VM and is asked for on demand — a frame's
//! variables when that frame is selected, an object's fields when it is expanded. Fetching the
//! whole object graph at every stop would be both slow and mostly wasted: a stack of forty
//! frames has one you are looking at.
//!
//! ## What a value is allowed to cost
//!
//! JDWP hands out *handles*, not values. `Order` comes back as an object id, and turning it
//! into the word "Order" is another round trip (`ObjectReference.ReferenceType`), and its
//! fields are another. Each row of the variables tree therefore costs one or two calls against
//! a suspended VM — microseconds each, and bounded by what is on screen because expansion is
//! lazy.
//!
//! What is deliberately **not** done is calling `toString()`. Invoking a method on the debugged
//! VM runs arbitrary application code inside a paused program: it can block on a lock the
//! suspended thread holds, it can mutate state, and it can throw. IntelliJ does it and accepts
//! the consequences; here an object renders as `Order@1f3c` with its real fields underneath,
//! which is always true and never has a side effect.
//!
//! ## Watches are paths, not Java
//!
//! A watch expression is `name`, `name.field.field`, or `name[3]` — resolved here by reading fields
//! and array slots. The grammar itself is [`crate::debug_path`], shared with the Rust watch: the
//! walk is what differs between JDWP and DAP, the shape of what the user typed is not. See that
//! module for why a watch is deliberately not an expression language.

// Named, not glob: the crate's prelude exports its own one-parameter `Result<T>`, which would
// shadow `std`'s in a module whose functions all return `Result<T, String>`.
use bennu_jdwp::prelude::{
    array_length, array_values, class_name, frame_this, frame_values, object_type, object_values,
    string_value, superclass, Field, Frame, Id, Local, Tag, Value,
};
use bennu_proto::prelude::DebugValue;

use crate::debug::{simple_name, Session, MAX_ELEMENTS};
use crate::debug_path::{self, Step};

/// Past this many characters a string is shown cut. A log line's worth is plenty to recognise
/// a value by; a 4 MB JSON payload in a variables row helps nobody.
const MAX_STRING: usize = 200;

/// How far up the inheritance chain an object's fields are collected. Deep enough for any real
/// hierarchy, and a hard stop rather than trusting `superclass` to terminate.
const MAX_SUPERS: usize = 20;

// ── a frame's variables ─────────────────────────────────────────────────────────

/// Everything in scope at frame `index` of the stopped thread: the receiver, then the
/// arguments, then the locals — the order they are declared in and the order they are read in.
///
/// A class compiled without `-g:vars` has no variable table at all, and the honest answer is
/// `this` alone rather than an empty panel that reads as "this method has no variables".
pub(crate) fn variables(session: &Session, index: usize) -> Result<Vec<DebugValue>, String> {
    let Some((thread, frame)) = session.frame_at(index) else {
        return Err("the program is not stopped there".to_string());
    };
    let table = session.variables_of(frame.location.class, frame.location.method);
    // Only the ones alive at this bytecode index. A variable declared halfway down a method
    // occupies its slot from that point on, and reading it earlier returns whatever the
    // compiler last reused the slot for — a plausible number that is not the answer.
    let live: Vec<&Local> = table.iter().filter(|l| l.in_scope(frame.location.index)).collect();

    let mut out = Vec::with_capacity(live.len() + 1);

    if !live.iter().any(|l| l.name == "this") {
        // No table, or a `static` method. Ask directly: `this` is the one variable that is
        // there whether or not the class carries debug information.
        if let Ok(value) = frame_this(&session.client, thread, frame.id) {
            if !value.is_null() {
                out.push(describe(session, "this", "this", None, value));
            }
        }
    }

    if !live.is_empty() {
        let slots: Vec<(i32, Tag)> =
            live.iter().map(|l| (l.slot, tag_of(&l.signature))).collect();
        let values = frame_values(&session.client, thread, frame.id, &slots)
            .map_err(|e| e.to_string())?;
        // Arguments first, then the rest — the two groups a reader wants told apart, and the
        // only grouping the variable table can honestly support.
        for wanted_arguments in [true, false] {
            for (local, value) in live.iter().zip(values.iter()) {
                if local.argument != wanted_arguments {
                    continue;
                }
                let kind = if local.name == "this" {
                    "this"
                } else if local.argument {
                    "argument"
                } else {
                    "local"
                };
                out.push(describe(
                    session,
                    &local.name,
                    kind,
                    Some(local.signature.as_str()),
                    *value,
                ));
            }
        }
    }

    Ok(out)
}

// ── what is inside a value ──────────────────────────────────────────────────────

/// The contents of an object handle: an array's elements, or an object's fields — its own and
/// every superclass's, because a field you did not declare is still a field you are looking at.
///
/// Statics are left out. A class's constants are the same at every breakpoint and would bury
/// the four fields that change.
pub(crate) fn expand(session: &Session, object: Id) -> Result<Vec<DebugValue>, String> {
    if object == 0 {
        return Ok(Vec::new());
    }
    let class = object_type(&session.client, object).map_err(|e| e.to_string())?;
    // type_tag 3 = array.
    if class.type_tag == 3 {
        return array_rows(session, object, &session.signature_of(class.id));
    }
    field_rows(session, object, class.id)
}

fn array_rows(session: &Session, array: Id, signature: &str) -> Result<Vec<DebugValue>, String> {
    let length = array_length(&session.client, array).map_err(|e| e.to_string())?;
    let take = length.min(MAX_ELEMENTS).max(0);
    if take == 0 {
        return Ok(Vec::new());
    }
    let element = signature.strip_prefix('[').unwrap_or(signature).to_string();
    let values = array_values(&session.client, array, 0, take).map_err(|e| e.to_string())?;
    let mut rows: Vec<DebugValue> = values
        .into_iter()
        .enumerate()
        .map(|(i, v)| describe(session, &format!("[{i}]"), "element", Some(element.as_str()), v))
        .collect();
    if length > take {
        // Said, not silently dropped: a list that stops at 100 without saying so reads as an
        // array of 100.
        rows.push(DebugValue {
            name: format!("… {} more", length - take),
            kind: "element".to_string(),
            type_name: String::new(),
            value: String::new(),
            object: None,
        });
    }
    Ok(rows)
}

fn field_rows(session: &Session, object: Id, class: Id) -> Result<Vec<DebugValue>, String> {
    let mut rows = Vec::new();
    let mut current = class;
    for _ in 0..MAX_SUPERS {
        if current == 0 {
            break;
        }
        let declared: Vec<Field> =
            session.fields_of(current).into_iter().filter(|f| !f.is_static()).collect();
        if !declared.is_empty() {
            let ids: Vec<Id> = declared.iter().map(|f| f.id).collect();
            let values = object_values(&session.client, object, &ids).unwrap_or_default();
            for (field, value) in declared.iter().zip(values) {
                rows.push(describe(
                    session,
                    &field.name,
                    "field",
                    Some(field.signature.as_str()),
                    value,
                ));
            }
        }
        // `java.lang.Object` declares nothing, so the walk ends on its own; the counter is the
        // guard against a VM that answers something unexpected.
        current = superclass(&session.client, current).unwrap_or(0);
    }
    Ok(rows)
}

// ── watches ─────────────────────────────────────────────────────────────────────

/// Evaluate a watch path against a frame. The result is named by the whole expression, so the
/// panel row reads as what was asked rather than as its last segment.
pub(crate) fn watch(
    session: &Session,
    frame: usize,
    expression: &str,
) -> Result<DebugValue, String> {
    let Some((thread, at)) = session.frame_at(frame) else {
        return Err("the program is not stopped there".to_string());
    };
    let steps = debug_path::parse(expression, debug_path::JAVA)?;
    let value = walk_path(session, thread, &at, &steps)?;
    Ok(describe(session, expression, "watch", None, value))
}

/// Resolve an already-parsed path against a **named** frame of a thread.
///
/// Separate from [`watch`] because a breakpoint condition is evaluated in a frame the session has
/// not announced a pause for — the VM stopped, the condition is being tested, and if it does not
/// hold the program is let go without anyone ever being told it stopped. Going through
/// `frame_at` would mean publishing a pause first and taking it back, which is a lie the panel
/// would briefly render.
pub(crate) fn walk_path(
    session: &Session,
    thread: Id,
    at: &Frame,
    steps: &[Step],
) -> Result<Value, String> {
    let Some(Step::Field(first)) = steps.first() else {
        return Err("a path starts with a variable name".to_string());
    };
    let mut value = root_value(session, thread, at, first)?;
    for step in &steps[1..] {
        value = follow(session, value, step)?;
    }
    Ok(value)
}

/// The first segment: a local of the frame, else a field of its receiver — which is how
/// watching a bare field name works inside an instance method.
fn root_value(session: &Session, thread: Id, at: &Frame, name: &str) -> Result<Value, String> {
    let table = session.variables_of(at.location.class, at.location.method);
    if let Some(local) =
        table.iter().find(|l| l.name == name && l.in_scope(at.location.index))
    {
        let slots = [(local.slot, tag_of(&local.signature))];
        let values = frame_values(&session.client, thread, at.id, &slots)
            .map_err(|e| e.to_string())?;
        return values.into_iter().next().ok_or_else(|| "no value".to_string());
    }

    let this = frame_this(&session.client, thread, at.id).map_err(|e| e.to_string())?;
    if this.is_null() {
        return Err(format!("no variable named `{name}` here"));
    }
    follow(session, this, &Step::Field(name.to_string()))
        .map_err(|_| format!("no variable or field named `{name}` here"))
}

/// One hop: a field of an object, or an element of an array.
fn follow(session: &Session, value: Value, step: &Step) -> Result<Value, String> {
    let Value::Object { id, .. } = value else {
        return Err("that is a primitive — there is nothing inside it".to_string());
    };
    if id == 0 {
        return Err("null".to_string());
    }
    let class = object_type(&session.client, id).map_err(|e| e.to_string())?;

    match step {
        // The Java syntax has no `*`, so the parser never produces this — the arm exists because
        // one grammar serves both languages and an unreachable case is better said than assumed.
        Step::Deref => Err("there is no `*` in a Java watch".to_string()),
        Step::Index(at) => {
            if class.type_tag != 3 {
                return Err("that is not an array".to_string());
            }
            let length = array_length(&session.client, id).map_err(|e| e.to_string())?;
            if *at < 0 || *at >= length {
                return Err(format!("index {at} is outside an array of {length}"));
            }
            array_values(&session.client, id, *at, 1)
                .map_err(|e| e.to_string())?
                .into_iter()
                .next()
                .ok_or_else(|| "no value".to_string())
        }
        Step::Field(name) => {
            let mut current = class.id;
            for _ in 0..MAX_SUPERS {
                if current == 0 {
                    break;
                }
                if let Some(field) = session.fields_of(current).into_iter().find(|f| &f.name == name)
                {
                    return object_values(&session.client, id, &[field.id])
                        .map_err(|e| e.to_string())?
                        .into_iter()
                        .next()
                        .ok_or_else(|| "no value".to_string());
                }
                current = superclass(&session.client, current).unwrap_or(0);
            }
            Err(format!("no field named `{name}` on {}", simple_name(&session.class_name_of(class.id))))
        }
    }
}

// ── rendering ───────────────────────────────────────────────────────────────────

/// One value as a row: what it is called, what type it was declared as, what it reads as, and
/// whether there is more inside.
fn describe(
    session: &Session,
    name: &str,
    kind: &str,
    signature: Option<&str>,
    value: Value,
) -> DebugValue {
    let (text, object) = render(session, value);
    // The DECLARED type when we know it (`List` even though the object is an `ArrayList`,
    // which is what the code says), and the runtime one otherwise.
    let type_name = match signature {
        Some(s) => type_display(s),
        None => match value {
            Value::Object { id, .. } if id != 0 => {
                session.type_name_of(id).unwrap_or_default()
            }
            _ => String::new(),
        },
    };
    DebugValue {
        name: name.to_string(),
        kind: kind.to_string(),
        type_name,
        value: text,
        object: object.map(|id| id.to_string()),
    }
}

/// A value as text, plus the handle to expand when there is something inside it.
fn render(session: &Session, value: Value) -> (String, Option<Id>) {
    match value {
        Value::Void => ("void".to_string(), None),
        Value::Boolean(b) => (b.to_string(), None),
        Value::Byte(v) => (v.to_string(), None),
        Value::Short(v) => (v.to_string(), None),
        Value::Int(v) => (v.to_string(), None),
        Value::Long(v) => (v.to_string(), None),
        Value::Float(v) => (v.to_string(), None),
        Value::Double(v) => (v.to_string(), None),
        // A char is a UTF-16 code unit: one half of a surrogate pair is not a character, and
        // showing its number is more honest than showing a replacement glyph.
        Value::Char(c) => (
            match char::from_u32(u32::from(c)) {
                Some(ch) => format!("'{ch}'"),
                None => format!("\\u{c:04x}"),
            },
            None,
        ),
        Value::Object { id: 0, .. } => ("null".to_string(), None),
        Value::Object { tag: Tag::String, id } => {
            let text = string_value(&session.client, id).unwrap_or_default();
            (quote(&text), None)
        }
        Value::Object { tag: Tag::Array, id } => {
            let length = array_length(&session.client, id).unwrap_or(0);
            // The length goes where the FIRST pair of brackets is, so a two-dimensional array
            // reads `String[3][]` — the row you can expand is the outer one.
            let name = session.type_name_of(id).unwrap_or_default();
            let text = match name.find("[]") {
                Some(i) => format!("{}[{length}]{}", &name[..i], &name[i + 2..]),
                None => format!("{name}[{length}]"),
            };
            (text, Some(id))
        }
        Value::Object { id, .. } => {
            let name = session.type_name_of(id).unwrap_or_else(|| "Object".to_string());
            // The handle in the label, because two rows showing `Order@1f3c` are the same
            // object and two showing different numbers are not — which is the question a
            // variables panel is asked most often after "what is in it".
            (format!("{name}@{id:x}"), Some(id))
        }
    }
}

/// A string as a literal, cut if it is long.
fn quote(text: &str) -> String {
    let escaped: String = text
        .chars()
        .take(MAX_STRING)
        .map(|c| match c {
            '\n' => "\\n".to_string(),
            '\r' => "\\r".to_string(),
            '\t' => "\\t".to_string(),
            '"' => "\\\"".to_string(),
            c => c.to_string(),
        })
        .collect();
    if text.chars().count() > MAX_STRING {
        format!("\"{escaped}…\" ({} chars)", text.chars().count())
    } else {
        format!("\"{escaped}\"")
    }
}

/// The tag a JVM type descriptor implies — what `StackFrame.GetValues` has to be told in
/// advance for each slot it reads.
fn tag_of(signature: &str) -> Tag {
    match signature.as_bytes().first() {
        Some(b'[') => Tag::Array,
        Some(b'Z') => Tag::Boolean,
        Some(b'B') => Tag::Byte,
        Some(b'C') => Tag::Char,
        Some(b'S') => Tag::Short,
        Some(b'I') => Tag::Int,
        Some(b'J') => Tag::Long,
        Some(b'F') => Tag::Float,
        Some(b'D') => Tag::Double,
        // `L…;`, and anything unrecognised. An object is the safe guess: its payload is an
        // identifier of a width the codec knows, so a wrong guess here costs a wrong label
        // rather than a misparsed reply.
        _ => Tag::Object,
    }
}

/// A JVM type descriptor as a reader sees it: `I` → `int`, `[Ljava/lang/String;` → `String[]`.
pub(crate) fn type_display(signature: &str) -> String {
    if let Some(inner) = signature.strip_prefix('[') {
        return format!("{}[]", type_display(inner));
    }
    match signature {
        "Z" => "boolean".to_string(),
        "B" => "byte".to_string(),
        "C" => "char".to_string(),
        "S" => "short".to_string(),
        "I" => "int".to_string(),
        "J" => "long".to_string(),
        "F" => "float".to_string(),
        "D" => "double".to_string(),
        "V" => "void".to_string(),
        other => simple_name(&class_name(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_descriptor_reads_as_the_type_the_code_declares() {
        assert_eq!(type_display("I"), "int");
        assert_eq!(type_display("Ljava/lang/String;"), "String");
        assert_eq!(type_display("[I"), "int[]");
        assert_eq!(type_display("[[Ljava/util/List;"), "List[][]");
        assert_eq!(type_display("Lcom/acme/Order$Line;"), "Order$Line");
    }

    /// The tag has to be right *before* the value is read — it decides how many bytes follow.
    #[test]
    fn a_descriptor_implies_the_tag_its_slot_is_read_with() {
        assert_eq!(tag_of("I"), Tag::Int);
        assert_eq!(tag_of("J"), Tag::Long);
        assert_eq!(tag_of("Ljava/lang/String;"), Tag::Object);
        assert_eq!(tag_of("[B"), Tag::Array);
        assert_eq!(tag_of(""), Tag::Object);
    }




    #[test]
    fn a_long_string_is_cut_and_says_so() {
        let short = quote("hello");
        assert_eq!(short, "\"hello\"");
        assert_eq!(quote("a\nb"), "\"a\\nb\"");

        let long = "x".repeat(MAX_STRING + 50);
        let cut = quote(&long);
        assert!(cut.contains('…'));
        assert!(cut.contains(&format!("({} chars)", MAX_STRING + 50)));
    }
}
