//! The commands that **change** the debugged program, and the pure rules around them.
//!
//! Everything in [`crate::command`] looks; what is here acts: create an object in the VM, throw it in a
//! thread, return early from a method, pop frames, write a variable or a field. A VM may not support
//! some of these (it says so in its capabilities, [`capabilities`]), and a host should ask before trying
//! so the refusal reads as a sentence rather than as `NOT_IMPLEMENTED (99)`.
//!
//! The text → value rules ([`parse_literal`], [`typed_value`]) are pure and tested without a VM: what
//! may be written is deliberately small (primitives, `String`, `null`) because anything bigger would need
//! a constructor call, and that is a decision the caller makes explicitly (see [`new_instance`]).

use crate::client::Client;
use crate::codec::{Id, Reader, Tag, Value, Writer};
use crate::error::{JdwpError, Result};

const VIRTUAL_MACHINE: u8 = 1;
const CLASS_TYPE: u8 = 3;
const OBJECT_REFERENCE: u8 = 9;
const THREAD_REFERENCE: u8 = 11;
const STACK_FRAME: u8 = 16;

/// `INVOKE_SINGLE_THREADED`: only the invoking thread runs while the constructor does.
const INVOKE_SINGLE_THREADED: i32 = 1;

// ── capabilities ────────────────────────────────────────────────────────────────

/// What the VM says it can do, of the part that matters for changing a program.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Capabilities {
    pub can_pop_frames: bool,
    pub can_force_early_return: bool,
    pub can_get_instance_info: bool,
}

/// Decode the reply of `VirtualMachine.CapabilitiesNew`: up to 32 booleans, in the order the
/// specification lists them. An older VM sends fewer; the rest read as "no".
pub fn decode_capabilities(data: &[u8]) -> Result<Capabilities> {
    let mut r = Reader::new(data, Default::default());
    let mut flags = Vec::new();
    while r.remaining() > 0 && flags.len() < 32 {
        flags.push(r.bool()?);
    }
    let at = |i: usize| flags.get(i).copied().unwrap_or(false);
    Ok(Capabilities { can_pop_frames: at(10), can_force_early_return: at(20), can_get_instance_info: at(15) })
}

/// `VirtualMachine.CapabilitiesNew`. A VM that does not implement it is treated as having none.
pub fn capabilities(client: &Client) -> Result<Capabilities> {
    match client.send(VIRTUAL_MACHINE, 17, Vec::new(), "VirtualMachine.CapabilitiesNew") {
        Ok(data) => decode_capabilities(&data),
        Err(JdwpError::Vm { code: 99, .. }) => Ok(Capabilities::default()),
        Err(e) => Err(e),
    }
}

// ── commands ────────────────────────────────────────────────────────────────────

/// `VirtualMachine.CreateString` — a `java.lang.String` in the VM, to pass as an argument.
pub fn create_string(client: &Client, text: &str) -> Result<Id> {
    let mut w = Writer::new(client.sizes());
    w.string(text);
    let data = client.send(VIRTUAL_MACHINE, 11, w.into_bytes(), "VirtualMachine.CreateString")?;
    Reader::new(&data, client.sizes()).object_id()
}

/// What `ClassType.NewInstance` answered: the new object, and the exception its constructor threw (0 = none).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NewInstance {
    pub object: Id,
    pub exception: Id,
}

/// `ClassType.NewInstance` — **runs the constructor** in `thread`, which must be suspended by an event (a
/// breakpoint stop is). Whatever the constructor does, happens.
pub fn new_instance(client: &Client, class: Id, thread: Id, constructor: Id, args: &[Value]) -> Result<NewInstance> {
    let mut w = Writer::new(client.sizes());
    w.reference_type_id(class).object_id(thread).method_id(constructor).i32(args.len() as i32);
    for a in args {
        w.value(a);
    }
    w.i32(INVOKE_SINGLE_THREADED);
    let data = client.send(CLASS_TYPE, 4, w.into_bytes(), "ClassType.NewInstance")?;
    let mut r = Reader::new(&data, client.sizes());
    let object = r.value()?;
    let exception = r.value()?;
    let id = |v: Value| if let Value::Object { id, .. } = v { id } else { 0 };
    Ok(NewInstance { object: id(object), exception: id(exception) })
}

/// `ThreadReference.Stop` — raise `throwable` in the thread, as if the line it is on had thrown it. Delivered
/// when the thread runs again.
pub fn stop_thread(client: &Client, thread: Id, throwable: Id) -> Result<()> {
    let mut w = Writer::new(client.sizes());
    w.object_id(thread).object_id(throwable);
    client.send(THREAD_REFERENCE, 10, w.into_bytes(), "ThreadReference.Stop").map(|_| ())
}

/// `ThreadReference.ForceEarlyReturn` — the method on top of the stack returns `value` now, without running
/// the rest of it (`finally` blocks included).
pub fn force_early_return(client: &Client, thread: Id, value: &Value) -> Result<()> {
    let mut w = Writer::new(client.sizes());
    w.object_id(thread).value(value);
    client.send(THREAD_REFERENCE, 14, w.into_bytes(), "ThreadReference.ForceEarlyReturn").map(|_| ())
}

/// `StackFrame.PopFrames` — discard `frame` and every frame above it; the thread is left at the call in the
/// caller, which runs again when the program resumes.
pub fn pop_frames(client: &Client, thread: Id, frame: Id) -> Result<()> {
    let mut w = Writer::new(client.sizes());
    w.object_id(thread).frame_id(frame);
    client.send(STACK_FRAME, 4, w.into_bytes(), "StackFrame.PopFrames").map(|_| ())
}

/// `StackFrame.SetValues` for one local variable.
pub fn set_frame_value(client: &Client, thread: Id, frame: Id, slot: i32, value: &Value) -> Result<()> {
    let mut w = Writer::new(client.sizes());
    w.object_id(thread).frame_id(frame).i32(1).i32(slot).value(value);
    client.send(STACK_FRAME, 2, w.into_bytes(), "StackFrame.SetValues").map(|_| ())
}

/// `ObjectReference.SetValues` for one field (the value travels untagged: the field says what it is).
pub fn set_field_value(client: &Client, object: Id, field: Id, value: &Value) -> Result<()> {
    let mut w = Writer::new(client.sizes());
    w.object_id(object).i32(1).field_id(field).untagged_value(value);
    client.send(OBJECT_REFERENCE, 3, w.into_bytes(), "ObjectReference.SetValues").map(|_| ())
}

// ── what may be written ─────────────────────────────────────────────────────────

/// A value as the agent typed it.
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Void,
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Char(char),
    Text(String),
}

/// Read `null`, `true`/`false`, an integer, a decimal, `'c'` or `"text"`. Anything else is refused:
/// no expressions, no names, nothing that would have to be evaluated.
pub fn parse_literal(text: &str) -> std::result::Result<Literal, String> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(Literal::Void);
    }
    match t {
        "null" => return Ok(Literal::Null),
        "true" => return Ok(Literal::Bool(true)),
        "false" => return Ok(Literal::Bool(false)),
        _ => {}
    }
    if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
        return Ok(Literal::Text(unescape(&t[1..t.len() - 1])?));
    }
    if t.len() >= 3 && t.starts_with('\'') && t.ends_with('\'') {
        let inner = unescape(&t[1..t.len() - 1])?;
        let mut chars = inner.chars();
        return match (chars.next(), chars.next()) {
            (Some(c), None) => Ok(Literal::Char(c)),
            _ => Err(format!("{t} is not a single character")),
        };
    }
    if let Ok(i) = t.parse::<i64>() {
        return Ok(Literal::Int(i));
    }
    if let Ok(f) = t.parse::<f64>() {
        if f.is_finite() {
            return Ok(Literal::Float(f));
        }
    }
    Err(format!("`{t}` is not a literal: use a number, true/false, 'c', \"text\" or null"))
}

fn unescape(raw: &str) -> std::result::Result<String, String> {
    let mut out = String::new();
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('"') => out.push('"'),
            Some('\'') => out.push('\''),
            Some('\\') => out.push('\\'),
            other => return Err(format!("unsupported escape \\{}", other.map(String::from).unwrap_or_default())),
        }
    }
    Ok(out)
}

/// A literal made ready for a slot of a given declared type.
#[derive(Debug, Clone, PartialEq)]
pub enum Typed {
    Ready(Value),
    /// A `String` has to be created in the VM first ([`create_string`]).
    String(String),
}

/// The return type of a method descriptor: `(I)Ljava/lang/String;` → `Ljava/lang/String;`.
pub fn return_descriptor(method_signature: &str) -> &str {
    method_signature.rsplit_once(')').map_or("", |(_, ret)| ret)
}

/// Fit `literal` to the declared type `descriptor` (`I`, `Z`, `Ljava/lang/String;`, …).
///
/// Only primitives, `String` and `null` are accepted: a value of any other class would have to be built, which
/// is not something a write of a literal can mean. Numbers are range-checked, not wrapped.
pub fn typed_value(descriptor: &str, literal: &Literal) -> std::result::Result<Typed, String> {
    let ranged = |min: i64, max: i64, name: &str| -> std::result::Result<i64, String> {
        match literal {
            Literal::Int(i) if (min..=max).contains(i) => Ok(*i),
            Literal::Int(i) => Err(format!("{i} does not fit in a {name}")),
            _ => Err(format!("a {name} takes a whole number")),
        }
    };
    let ready = |v: Value| Ok(Typed::Ready(v));
    match descriptor {
        "V" => match literal {
            Literal::Void => ready(Value::Void),
            _ => Err("the method returns nothing (void): give no value".into()),
        },
        "Z" => match literal {
            Literal::Bool(b) => ready(Value::Boolean(*b)),
            _ => Err("a boolean takes true or false".into()),
        },
        "B" => ranged(i8::MIN.into(), i8::MAX.into(), "byte").and_then(|i| ready(Value::Byte(i as i8))),
        "S" => ranged(i16::MIN.into(), i16::MAX.into(), "short").and_then(|i| ready(Value::Short(i as i16))),
        "I" => ranged(i32::MIN.into(), i32::MAX.into(), "int").and_then(|i| ready(Value::Int(i as i32))),
        "J" => ranged(i64::MIN, i64::MAX, "long").and_then(|i| ready(Value::Long(i))),
        "F" => match literal {
            Literal::Int(i) => ready(Value::Float(*i as f32)),
            Literal::Float(f) => ready(Value::Float(*f as f32)),
            _ => Err("a float takes a number".into()),
        },
        "D" => match literal {
            Literal::Int(i) => ready(Value::Double(*i as f64)),
            Literal::Float(f) => ready(Value::Double(*f)),
            _ => Err("a double takes a number".into()),
        },
        "C" => match literal {
            Literal::Char(c) if (*c as u32) <= 0xFFFF => ready(Value::Char(*c as u32 as u16)),
            Literal::Char(_) => Err("that character is outside the 16-bit range of a char".into()),
            _ => Err("a char takes 'c'".into()),
        },
        "Ljava/lang/String;" => match literal {
            Literal::Text(t) => Ok(Typed::String(t.clone())),
            Literal::Null => ready(Value::Object { tag: Tag::String, id: 0 }),
            _ => Err("a String takes \"text\" or null".into()),
        },
        d if d.starts_with('L') || d.starts_with('[') => match literal {
            Literal::Null => ready(Value::Object { tag: if d.starts_with('[') { Tag::Array } else { Tag::Object }, id: 0 }),
            _ => Err("only primitives, String and null can be written: an object of another type would have to be built".into()),
        },
        other => Err(format!("unknown type `{other}`")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_are_read_by_position_and_a_short_reply_means_no() {
        let mut flags = vec![0u8; 32];
        flags[10] = 1; // canPopFrames
        flags[20] = 1; // canForceEarlyReturn
        let caps = decode_capabilities(&flags).unwrap();
        assert!(caps.can_pop_frames && caps.can_force_early_return && !caps.can_get_instance_info);
        let short = decode_capabilities(&[1, 1, 1]).unwrap();
        assert!(!short.can_pop_frames && !short.can_force_early_return, "an old VM sends only the first seven");
        assert_eq!(decode_capabilities(&[]).unwrap(), Capabilities::default());
    }

    #[test]
    fn literals_are_read_without_evaluating_anything() {
        assert_eq!(parse_literal(" 42 ").unwrap(), Literal::Int(42));
        assert_eq!(parse_literal("-7").unwrap(), Literal::Int(-7));
        assert_eq!(parse_literal("2.5").unwrap(), Literal::Float(2.5));
        assert_eq!(parse_literal("true").unwrap(), Literal::Bool(true));
        assert_eq!(parse_literal("null").unwrap(), Literal::Null);
        assert_eq!(parse_literal("'x'").unwrap(), Literal::Char('x'));
        assert_eq!(parse_literal("\"a\\nb\"").unwrap(), Literal::Text("a\nb".into()));
        assert_eq!(parse_literal("").unwrap(), Literal::Void);
        for bad in ["order.total", "1 + 2", "new Foo()", "'ab'", "\"x\\q\"", "NaN"] {
            assert!(parse_literal(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_number_that_does_not_fit_is_refused_not_wrapped() {
        assert_eq!(typed_value("B", &Literal::Int(127)).unwrap(), Typed::Ready(Value::Byte(127)));
        assert!(typed_value("B", &Literal::Int(128)).unwrap_err().contains("does not fit in a byte"));
        assert!(typed_value("I", &Literal::Int(i64::from(i32::MAX) + 1)).is_err());
        assert_eq!(typed_value("J", &Literal::Int(i64::MAX)).unwrap(), Typed::Ready(Value::Long(i64::MAX)));
        assert!(typed_value("I", &Literal::Float(1.5)).is_err());
    }

    #[test]
    fn each_declared_type_takes_the_literal_it_means() {
        assert_eq!(typed_value("Z", &Literal::Bool(false)).unwrap(), Typed::Ready(Value::Boolean(false)));
        assert!(typed_value("Z", &Literal::Int(1)).is_err());
        assert_eq!(typed_value("D", &Literal::Int(3)).unwrap(), Typed::Ready(Value::Double(3.0)));
        assert_eq!(typed_value("C", &Literal::Char('€')).unwrap(), Typed::Ready(Value::Char(0x20AC)));
        assert!(typed_value("C", &Literal::Char('😀')).is_err());
        assert_eq!(typed_value("V", &Literal::Void).unwrap(), Typed::Ready(Value::Void));
        assert!(typed_value("V", &Literal::Int(1)).is_err());
    }

    #[test]
    fn strings_are_created_in_the_vm_and_other_objects_only_null() {
        assert_eq!(typed_value("Ljava/lang/String;", &Literal::Text("hi".into())).unwrap(), Typed::String("hi".into()));
        assert_eq!(
            typed_value("Lcom/acme/Order;", &Literal::Null).unwrap(),
            Typed::Ready(Value::Object { tag: Tag::Object, id: 0 })
        );
        assert!(typed_value("Lcom/acme/Order;", &Literal::Int(1)).unwrap_err().contains("would have to be built"));
        assert!(typed_value("[I", &Literal::Text("x".into())).is_err());
    }

    #[test]
    fn the_return_type_is_what_follows_the_parenthesis() {
        assert_eq!(return_descriptor("(ILjava/lang/String;)J"), "J");
        assert_eq!(return_descriptor("()V"), "V");
        assert_eq!(return_descriptor("()[Ljava/lang/String;"), "[Ljava/lang/String;");
        assert_eq!(return_descriptor("nonsense"), "");
    }

    #[test]
    fn the_command_payloads_have_the_shape_the_specification_gives() {
        // ForceEarlyReturn: thread id, then a tagged value.
        let sizes = crate::codec::IdSizes::default();
        let mut w = Writer::new(sizes);
        w.object_id(5).value(&Value::Int(9));
        let b = w.into_bytes();
        assert_eq!(b.len(), 8 + 1 + 4);
        assert_eq!(b[8], b'I');
        // ObjectReference.SetValues: object, count, field id, UNtagged value.
        let mut w = Writer::new(sizes);
        w.object_id(5).i32(1).field_id(7).untagged_value(&Value::Short(3));
        assert_eq!(w.into_bytes().len(), 8 + 4 + 8 + 2);
    }
}
