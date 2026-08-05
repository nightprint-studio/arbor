//! The commands a breakpoint debugger needs, typed.
//!
//! Not the whole protocol — JDWP has around a hundred commands and most of them exist for
//! profilers, hot-swap and instrumentation. What is here is the set that answers: *which class
//! is that, which method, which line, stop there, what is on the stack, and what is in these
//! variables.* Everything else can be added one function at a time, and each one is a command
//! number and a reply shape.

use crate::client::Client;
use crate::codec::{Id, IdSizes, Location, Reader, Tag, Value, Writer};
use crate::error::Result;
use crate::event::SuspendPolicy;

// Command sets, named so the call sites read as the specification does.
const VIRTUAL_MACHINE: u8 = 1;
const REFERENCE_TYPE: u8 = 2;
const CLASS_TYPE: u8 = 3;
const METHOD: u8 = 6;
const OBJECT_REFERENCE: u8 = 9;
const STRING_REFERENCE: u8 = 10;
const THREAD_REFERENCE: u8 = 11;
const ARRAY_REFERENCE: u8 = 13;
const EVENT_REQUEST: u8 = 15;
const STACK_FRAME: u8 = 16;

/// `ACC_STATIC` — the modifier bit that tells a class's static fields from its instance ones.
pub const MOD_STATIC: i32 = 0x0008;

/// A loaded type, as the VM refers to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClassRef {
    /// `1` = class, `2` = interface, `3` = array.
    pub type_tag: u8,
    pub id: Id,
    /// Class status bits — `7` (verified · prepared · initialized) is the ready state.
    pub status: i32,
}

/// A method of a type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Method {
    pub id: Id,
    pub name: String,
    /// The JVM descriptor — `(Ljava/lang/String;)I`. Overloads differ only here.
    pub signature: String,
    pub mod_bits: i32,
}

/// One row of a method's line table: the bytecode index a source line starts at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineEntry {
    pub index: u64,
    pub line: i32,
}

/// A frame on a thread's stack. Valid only while the thread stays suspended — resuming it
/// invalidates every frame id it handed out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame {
    pub id: Id,
    pub location: Location,
}

/// One row of a method's local-variable table: a name, and the slot it lives in.
///
/// Present only when the class was compiled with `-g` (or `-g:vars`) — `javac`'s default emits
/// the *line* table but not this one, so a class can be perfectly breakpointable and still have
/// no variable names at all. Maven's compiler plugin turns full debug info on by default, which
/// is why this is usually there on a project built the normal way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Local {
    /// The frame slot to read it from.
    pub slot: i32,
    pub name: String,
    /// The JVM type descriptor — `Ljava/lang/String;`, `I`, `[B`.
    pub signature: String,
    /// The bytecode range it is in scope for: `[start, start + length)`. A variable declared
    /// halfway down a method is *not* readable at the top of it, and asking anyway returns
    /// whatever the slot happens to be reused for.
    pub start: u64,
    pub length: i32,
    /// Whether it is one of the method's arguments (`this` included, on an instance method).
    pub argument: bool,
}

impl Local {
    /// Whether this variable is in scope at bytecode index `at`.
    pub fn in_scope(&self, at: u64) -> bool {
        at >= self.start && at < self.start.saturating_add(self.length.max(0) as u64)
    }
}

/// A field of a type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub id: Id,
    pub name: String,
    /// The JVM type descriptor.
    pub signature: String,
    pub mod_bits: i32,
}

impl Field {
    /// Whether it belongs to the class rather than to an instance of it.
    pub fn is_static(&self) -> bool {
        self.mod_bits & MOD_STATIC != 0
    }
}

// ── the VM itself ───────────────────────────────────────────────────────────────

/// `VirtualMachine.IDSizes` — the widths every other reply depends on. Sent first, by
/// [`Client::attach`], and parseable without knowing them because it is all ints.
pub fn id_sizes(client: &Client) -> Result<IdSizes> {
    let data = client.send(VIRTUAL_MACHINE, 7, Vec::new(), "VirtualMachine.IDSizes")?;
    let mut r = Reader::new(&data, IdSizes::default());
    Ok(IdSizes {
        field: r.i32()? as usize,
        method: r.i32()? as usize,
        object: r.i32()? as usize,
        reference_type: r.i32()? as usize,
        frame: r.i32()? as usize,
    })
}

/// `VirtualMachine.Version` — the VM's own description, for a status line and for a log that
/// has to say what was attached to.
pub fn version(client: &Client) -> Result<String> {
    let data = client.send(VIRTUAL_MACHINE, 1, Vec::new(), "VirtualMachine.Version")?;
    Reader::new(&data, client.sizes()).string()
}

/// `VirtualMachine.Suspend` — every thread stops.
pub fn suspend_vm(client: &Client) -> Result<()> {
    client.send(VIRTUAL_MACHINE, 8, Vec::new(), "VirtualMachine.Suspend").map(|_| ())
}

/// `VirtualMachine.Resume` — the program runs on.
pub fn resume_vm(client: &Client) -> Result<()> {
    client.send(VIRTUAL_MACHINE, 9, Vec::new(), "VirtualMachine.Resume").map(|_| ())
}

/// `VirtualMachine.Dispose` — detach, leaving the program running and unsuspended. What
/// "stop debugging" should do to a server you did not want to kill.
pub fn dispose(client: &Client) -> Result<()> {
    client.send(VIRTUAL_MACHINE, 6, Vec::new(), "VirtualMachine.Dispose").map(|_| ())
}

/// `VirtualMachine.ClassesBySignature` — the loaded types with this signature.
///
/// **Empty is the normal answer** for a class that has not been loaded yet, and that is the
/// case a debugger has to handle: a breakpoint set before the program reaches the class is
/// not an error, it is a pending breakpoint waiting on a `ClassPrepare` event.
pub fn classes_by_signature(client: &Client, signature: &str) -> Result<Vec<ClassRef>> {
    let mut w = Writer::new(client.sizes());
    w.string(signature);
    let data = client.send(
        VIRTUAL_MACHINE,
        2,
        w.into_bytes(),
        "VirtualMachine.ClassesBySignature",
    )?;
    let mut r = Reader::new(&data, client.sizes());
    let count = r.i32()?.max(0);
    (0..count)
        .map(|_| {
            Ok(ClassRef {
                type_tag: r.u8()?,
                id: r.reference_type_id()?,
                status: r.i32()?,
            })
        })
        .collect()
}

// ── types and methods ───────────────────────────────────────────────────────────

/// `ReferenceType.Signature` — `Lcom/acme/Order;`.
pub fn type_signature(client: &Client, class: Id) -> Result<String> {
    let mut w = Writer::new(client.sizes());
    w.reference_type_id(class);
    let data = client.send(REFERENCE_TYPE, 1, w.into_bytes(), "ReferenceType.Signature")?;
    Reader::new(&data, client.sizes()).string()
}

/// `ReferenceType.Methods` — everything the type declares (not what it inherits).
pub fn methods(client: &Client, class: Id) -> Result<Vec<Method>> {
    let mut w = Writer::new(client.sizes());
    w.reference_type_id(class);
    let data = client.send(REFERENCE_TYPE, 5, w.into_bytes(), "ReferenceType.Methods")?;
    let mut r = Reader::new(&data, client.sizes());
    let count = r.i32()?.max(0);
    (0..count)
        .map(|_| {
            Ok(Method {
                id: r.method_id()?,
                name: r.string()?,
                signature: r.string()?,
                mod_bits: r.i32()?,
            })
        })
        .collect()
}

/// `ReferenceType.Fields` — what the type declares, NOT what it inherits. Walk
/// [`superclass`] for the rest.
pub fn fields(client: &Client, class: Id) -> Result<Vec<Field>> {
    let mut w = Writer::new(client.sizes());
    w.reference_type_id(class);
    let data = client.send(REFERENCE_TYPE, 4, w.into_bytes(), "ReferenceType.Fields")?;
    let mut r = Reader::new(&data, client.sizes());
    let count = r.i32()?.max(0);
    (0..count)
        .map(|_| {
            Ok(Field {
                id: r.field_id()?,
                name: r.string()?,
                signature: r.string()?,
                mod_bits: r.i32()?,
            })
        })
        .collect()
}

/// `ClassType.Superclass` — `0` when the type is `java.lang.Object` (or an interface). The
/// terminator of a walk up the inheritance chain, and the reason a variables panel can show
/// the fields a subclass did not declare.
pub fn superclass(client: &Client, class: Id) -> Result<Id> {
    let mut w = Writer::new(client.sizes());
    w.reference_type_id(class);
    let data = client.send(CLASS_TYPE, 1, w.into_bytes(), "ClassType.Superclass")?;
    Reader::new(&data, client.sizes()).reference_type_id()
}

/// `Method.LineTable` — which bytecode index each source line begins at.
///
/// This is the whole of how a breakpoint on "line 118" becomes a location the VM understands.
/// A class compiled without debug information has no table and the VM answers *absent
/// information* — which is worth surfacing verbatim, because the fix is a build flag and no
/// amount of retrying will help.
pub fn line_table(client: &Client, class: Id, method: Id) -> Result<Vec<LineEntry>> {
    let mut w = Writer::new(client.sizes());
    w.reference_type_id(class).method_id(method);
    let data = client.send(METHOD, 1, w.into_bytes(), "Method.LineTable")?;
    let mut r = Reader::new(&data, client.sizes());
    let _start = r.i64()?;
    let _end = r.i64()?;
    let count = r.i32()?.max(0);
    (0..count)
        .map(|_| Ok(LineEntry { index: r.i64()? as u64, line: r.i32()? }))
        .collect()
}

/// `Method.VariableTable` — the method's local variables, with the slots to read them from.
///
/// This is what turns a frame into *named* variables; without it a debugger has slot numbers and
/// nothing to call them. The VM answers `ABSENT_INFORMATION` when the class was compiled without
/// `-g:vars`, which is worth surfacing verbatim rather than showing an empty panel: an empty
/// panel reads as "this method has no variables", and the fix is a build flag.
///
/// `argument` is derived from the reply's `arg_count`, which JDWP gives as *the number of slots
/// the arguments occupy* — so a `long` argument counts twice, and slot 0 of an instance method
/// is `this`. Comparing the slot against it is how the two groups are told apart.
pub fn variable_table(client: &Client, class: Id, method: Id) -> Result<Vec<Local>> {
    let mut w = Writer::new(client.sizes());
    w.reference_type_id(class).method_id(method);
    let data = client.send(METHOD, 2, w.into_bytes(), "Method.VariableTable")?;
    decode_variable_table(&data, client.sizes())
}

/// The reply body of [`variable_table`], decoded. Split out from the round trip so the
/// argument/local split has a test that does not need a JVM.
fn decode_variable_table(data: &[u8], sizes: IdSizes) -> Result<Vec<Local>> {
    let mut r = Reader::new(data, sizes);
    let arg_count = r.i32()?;
    let count = r.i32()?.max(0);
    (0..count)
        .map(|_| {
            let start = r.i64()? as u64;
            let name = r.string()?;
            let signature = r.string()?;
            let length = r.i32()?;
            let slot = r.i32()?;
            Ok(Local { slot, name, signature, start, length, argument: slot < arg_count })
        })
        .collect()
}

/// The location a source line maps to, given the methods of its class.
///
/// A line can appear in several methods' tables (a lambda body, an inner class compiled into
/// the same file) and more than once within one; the **first** entry at or after the requested
/// line wins.
///
/// *At or after*, not *at*: a comment, a blank line or a declaration compiles to no bytecode at
/// all, and someone who clicked the gutter beside one meant the statement under it. The caller
/// is expected to translate the answer back into a line and say so when it moved — a breakpoint
/// that silently binds somewhere else is how you spend an afternoon watching a line that never
/// executes. `None` means there was nothing at or after it in the whole class.
pub fn location_of_line(
    client: &Client,
    class: Id,
    methods: &[Method],
    line: i32,
) -> Result<Option<Location>> {
    let mut best: Option<(i32, Location)> = None;
    for method in methods {
        // A method with no table is not an error here — abstract and native ones have none.
        let Ok(table) = line_table(client, class, method.id) else { continue };
        for entry in table {
            if entry.line < line {
                continue;
            }
            let candidate = Location {
                type_tag: 1,
                class,
                method: method.id,
                index: entry.index,
            };
            match best {
                Some((best_line, _)) if best_line <= entry.line => {}
                _ => best = Some((entry.line, candidate)),
            }
        }
    }
    Ok(best.map(|(_, location)| location))
}

// ── stopping ────────────────────────────────────────────────────────────────────

/// `EventRequest.Set` for a breakpoint at `location`. Returns the request id, which is what
/// the event carries back and what [`clear_event`] needs.
pub fn set_breakpoint(
    client: &Client,
    location: Location,
    policy: SuspendPolicy,
) -> Result<i32> {
    let mut w = Writer::new(client.sizes());
    w.u8(crate::event::kind::BREAKPOINT).u8(policy.to_byte()).i32(1);
    w.u8(7).location(location); // modifier 7 = LocationOnly
    let data = client.send(EVENT_REQUEST, 1, w.into_bytes(), "EventRequest.Set (breakpoint)")?;
    Reader::new(&data, client.sizes()).i32()
}

/// `EventRequest.Set` for a class being loaded, matched by name pattern (`com.acme.*`).
///
/// The other half of a pending breakpoint: you cannot set one on a class the VM has not loaded,
/// so you ask to be told when it loads and set it then.
pub fn request_class_prepare(client: &Client, pattern: &str) -> Result<i32> {
    let mut w = Writer::new(client.sizes());
    w.u8(crate::event::kind::CLASS_PREPARE).u8(SuspendPolicy::EventThread.to_byte()).i32(1);
    w.u8(5).string(pattern); // modifier 5 = ClassMatch
    let data =
        client.send(EVENT_REQUEST, 1, w.into_bytes(), "EventRequest.Set (class prepare)")?;
    Reader::new(&data, client.sizes()).i32()
}

/// `EventRequest.Set` for a **throw**, optionally narrowed to one exception type.
///
/// `class` of `None` means every throwable, which is the only way to catch the ones you did not
/// think of. `caught` / `uncaught` select which halves you want, and they are not the same
/// question: an uncaught throw is a crash worth stopping on always, while a caught one is
/// ordinary control flow in any framework that uses exceptions for flow — Spring throws and
/// catches thousands before `main` reaches your code, and asking for caught throws of
/// `Throwable` is how a debugger becomes unusable rather than thorough.
///
/// Note the VM decides "caught" by whether *any* frame catches it, which includes the framework
/// that will swallow it three frames up.
pub fn request_exception(
    client: &Client,
    class: Option<Id>,
    caught: bool,
    uncaught: bool,
    policy: SuspendPolicy,
) -> Result<i32> {
    let mut w = Writer::new(client.sizes());
    w.u8(crate::event::kind::EXCEPTION).u8(policy.to_byte()).i32(1);
    w.u8(8) // modifier 8 = ExceptionOnly
        .reference_type_id(class.unwrap_or(0))
        .u8(u8::from(caught))
        .u8(u8::from(uncaught));
    let data = client.send(EVENT_REQUEST, 1, w.into_bytes(), "EventRequest.Set (exception)")?;
    Reader::new(&data, client.sizes()).i32()
}

/// How far a step goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepDepth {
    Into,
    Over,
    Out,
}

/// `EventRequest.Set` for a single step on `thread`. Line-granularity — the depth decides
/// whether a call is entered, passed over, or returned from.
///
/// `exclude` is a list of class-name patterns the step will not stop inside (`java.*`,
/// `sun.*`). It is not a nicety: an unfiltered *step into* on `list.add(order)` lands in
/// `java.util.ArrayList.add`, then `ensureCapacity`, then `Arrays.copyOf` — a stepping session
/// spent somewhere the user has no source for and did not ask about. The filter makes the step
/// pass through those frames and stop at the first one that is not excluded.
///
/// `policy` should match whatever the session's breakpoints use. Mixing them is the subtle
/// mistake: after an `All` suspend every thread is stopped and only `resume_vm` restarts them,
/// while an `EventThread` suspend is undone by `resume_thread` — a session that uses both has
/// to remember which, and gets it wrong exactly once, leaving the program half-frozen.
///
/// A step request is **one-shot in practice**: clear it when the `Step` event arrives, or the
/// next resume steps again.
pub fn request_step(
    client: &Client,
    thread: Id,
    depth: StepDepth,
    exclude: &[&str],
    policy: SuspendPolicy,
) -> Result<i32> {
    let mut w = Writer::new(client.sizes());
    w.u8(crate::event::kind::SINGLE_STEP)
        .u8(policy.to_byte())
        .i32(1 + exclude.len() as i32);
    w.u8(10) // modifier 10 = Step
        .object_id(thread)
        .i32(1) // size: 1 = line
        .i32(match depth {
            StepDepth::Into => 0,
            StepDepth::Over => 1,
            StepDepth::Out => 2,
        });
    for pattern in exclude {
        w.u8(6).string(pattern); // modifier 6 = ClassExclude
    }
    let data = client.send(EVENT_REQUEST, 1, w.into_bytes(), "EventRequest.Set (step)")?;
    Reader::new(&data, client.sizes()).i32()
}

/// `EventRequest.Clear` — stop asking for this event. `kind` must be the one it was set with.
pub fn clear_event(client: &Client, kind: u8, request: i32) -> Result<()> {
    let mut w = Writer::new(client.sizes());
    w.u8(kind).i32(request);
    client.send(EVENT_REQUEST, 2, w.into_bytes(), "EventRequest.Clear").map(|_| ())
}

// ── threads and stacks ──────────────────────────────────────────────────────────

/// `ThreadReference.Name`.
pub fn thread_name(client: &Client, thread: Id) -> Result<String> {
    let mut w = Writer::new(client.sizes());
    w.object_id(thread);
    let data = client.send(THREAD_REFERENCE, 1, w.into_bytes(), "ThreadReference.Name")?;
    Reader::new(&data, client.sizes()).string()
}

/// `ThreadReference.Resume` — this thread only. What continues after a step.
pub fn resume_thread(client: &Client, thread: Id) -> Result<()> {
    let mut w = Writer::new(client.sizes());
    w.object_id(thread);
    client.send(THREAD_REFERENCE, 3, w.into_bytes(), "ThreadReference.Resume").map(|_| ())
}

/// `ThreadReference.Frames` — the whole stack, innermost first. The thread must be suspended.
pub fn frames(client: &Client, thread: Id) -> Result<Vec<Frame>> {
    let mut w = Writer::new(client.sizes());
    w.object_id(thread).i32(0).i32(-1); // from the top, all of them
    let data = client.send(THREAD_REFERENCE, 6, w.into_bytes(), "ThreadReference.Frames")?;
    let mut r = Reader::new(&data, client.sizes());
    let count = r.i32()?.max(0);
    (0..count).map(|_| Ok(Frame { id: r.frame_id()?, location: r.location()? })).collect()
}

/// `StackFrame.GetValues` — the locals at `slots`, each with the type tag its declaration says
/// it has.
///
/// The slot numbers and their tags come from the class's local-variable table
/// (`Method.VariableTable`), which is only present when the class was compiled with `-g`. That
/// is the same build flag the line table needs, so a class that can hold a breakpoint can
/// usually show its variables too.
pub fn frame_values(
    client: &Client,
    thread: Id,
    frame: Id,
    slots: &[(i32, Tag)],
) -> Result<Vec<Value>> {
    let mut w = Writer::new(client.sizes());
    w.object_id(thread).frame_id(frame).i32(slots.len() as i32);
    for (slot, tag) in slots {
        w.i32(*slot).u8(tag.to_byte());
    }
    let data = client.send(STACK_FRAME, 1, w.into_bytes(), "StackFrame.GetValues")?;
    let mut r = Reader::new(&data, client.sizes());
    let count = r.i32()?.max(0);
    (0..count).map(|_| r.value()).collect()
}

/// `StackFrame.ThisObject` — the receiver of the frame's method, or null in a `static` one.
///
/// The one variable that is there whether or not the class carries a variable table, which
/// makes it the fallback when [`variable_table`] answers *absent information*: a panel showing
/// `this` and its fields is far from nothing.
pub fn frame_this(client: &Client, thread: Id, frame: Id) -> Result<Value> {
    let mut w = Writer::new(client.sizes());
    w.object_id(thread).frame_id(frame);
    let data = client.send(STACK_FRAME, 3, w.into_bytes(), "StackFrame.ThisObject")?;
    Reader::new(&data, client.sizes()).value()
}

/// `ObjectReference.GetValues` — the named fields of one object, in the order asked.
pub fn object_values(client: &Client, object: Id, fields: &[Id]) -> Result<Vec<Value>> {
    let mut w = Writer::new(client.sizes());
    w.object_id(object).i32(fields.len() as i32);
    for f in fields {
        w.field_id(*f);
    }
    let data = client.send(OBJECT_REFERENCE, 2, w.into_bytes(), "ObjectReference.GetValues")?;
    let mut r = Reader::new(&data, client.sizes());
    let count = r.i32()?.max(0);
    (0..count).map(|_| r.value()).collect()
}

/// `ArrayReference.Length`.
pub fn array_length(client: &Client, array: Id) -> Result<i32> {
    let mut w = Writer::new(client.sizes());
    w.object_id(array);
    let data = client.send(ARRAY_REFERENCE, 1, w.into_bytes(), "ArrayReference.Length")?;
    Reader::new(&data, client.sizes()).i32()
}

/// `ArrayReference.GetValues` — `length` elements from `first`.
///
/// The reply is an *array region*, and its one trap is that the element encoding depends on the
/// region's tag: a primitive array's values arrive **untagged** (the region's single tag covers
/// them all), while an object array's arrive tagged individually — because each element may be
/// a different subtype, or null. Reading one shape for the other silently misparses the whole
/// region.
pub fn array_values(client: &Client, array: Id, first: i32, length: i32) -> Result<Vec<Value>> {
    let mut w = Writer::new(client.sizes());
    w.object_id(array).i32(first).i32(length);
    let data = client.send(ARRAY_REFERENCE, 2, w.into_bytes(), "ArrayReference.GetValues")?;
    decode_array_region(&data, client.sizes())
}

/// The reply body of [`array_values`], decoded — the tagged/untagged distinction above, tested.
fn decode_array_region(data: &[u8], sizes: IdSizes) -> Result<Vec<Value>> {
    let mut r = Reader::new(data, sizes);
    let tag = Tag::from_byte(r.u8()?)?;
    let count = r.i32()?.max(0);
    (0..count)
        .map(|_| if tag.is_object() { r.value() } else { r.value_of(tag) })
        .collect()
}

/// `StringReference.Value` — the text behind a string handle. Every readable string in a
/// variables panel costs one of these.
pub fn string_value(client: &Client, string: Id) -> Result<String> {
    let mut w = Writer::new(client.sizes());
    w.object_id(string);
    let data = client.send(STRING_REFERENCE, 1, w.into_bytes(), "StringReference.Value")?;
    Reader::new(&data, client.sizes()).string()
}

/// `ObjectReference.ReferenceType` — what an object handle actually is, so a variables panel
/// can say `ArrayList` rather than `Object@4711`.
pub fn object_type(client: &Client, object: Id) -> Result<ClassRef> {
    let mut w = Writer::new(client.sizes());
    w.object_id(object);
    let data =
        client.send(OBJECT_REFERENCE, 1, w.into_bytes(), "ObjectReference.ReferenceType")?;
    let mut r = Reader::new(&data, client.sizes());
    Ok(ClassRef { type_tag: r.u8()?, id: r.reference_type_id()?, status: 0 })
}

// ── names ───────────────────────────────────────────────────────────────────────

/// A dotted class name as the JVM signature the protocol speaks — `com.acme.Order` →
/// `Lcom/acme/Order;`. Nested classes keep their `$`, which is how the VM knows them.
pub fn class_signature(fqcn: &str) -> String {
    format!("L{};", fqcn.replace('.', "/"))
}

/// The reverse — `Lcom/acme/Order;` → `com.acme.Order`. Anything that is not an object
/// signature (a primitive, an array) comes back as it went in.
pub fn class_name(signature: &str) -> String {
    match signature.strip_prefix('L').and_then(|s| s.strip_suffix(';')) {
        Some(inner) => inner.replace('/', "."),
        None => signature.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_class_name_becomes_the_signature_the_vm_speaks() {
        assert_eq!(class_signature("com.acme.Order"), "Lcom/acme/Order;");
        assert_eq!(class_signature("com.acme.Order$Line"), "Lcom/acme/Order$Line;");
        assert_eq!(class_name("Lcom/acme/Order;"), "com.acme.Order");
        assert_eq!(class_name("Lcom/acme/Order$Line;"), "com.acme.Order$Line");
    }

    #[test]
    fn a_signature_that_is_not_a_class_is_left_alone() {
        assert_eq!(class_name("I"), "I");
        assert_eq!(class_name("[Ljava/lang/String;"), "[Ljava/lang/String;");
    }

    // ── reply decoding ──────────────────────────────────────────────────────────

    use crate::codec::Writer;

    fn sizes() -> IdSizes {
        IdSizes::default()
    }

    /// `arg_count` is a count of SLOTS, so `this` and the declared parameters fall below it and
    /// the method's own locals above — which is the only thing separating "Arguments" from
    /// "Locals" in a variables panel.
    #[test]
    fn the_argument_boundary_is_a_slot_number_not_a_position() {
        let mut w = Writer::new(sizes());
        w.i32(3).i32(4); // 3 argument slots (this + a long, say), 4 rows
        for (slot, name) in [(0, "this"), (1, "amount"), (3, "total"), (4, "i")] {
            w.i64(0).string(name).string("I").i32(200).i32(slot);
        }
        let table = decode_variable_table(&w.into_bytes(), sizes()).unwrap();
        let args: Vec<&str> =
            table.iter().filter(|l| l.argument).map(|l| l.name.as_str()).collect();
        assert_eq!(args, vec!["this", "amount"]);
        let locals: Vec<&str> =
            table.iter().filter(|l| !l.argument).map(|l| l.name.as_str()).collect();
        assert_eq!(locals, vec!["total", "i"], "slot 3 is past the argument slots");
    }

    /// A variable declared halfway down a method is not readable at the top of it — the slot is
    /// there, but it holds whatever the compiler last reused it for.
    #[test]
    fn a_local_is_only_in_scope_inside_its_bytecode_range() {
        let l = Local {
            slot: 2,
            name: "total".into(),
            signature: "I".into(),
            start: 10,
            length: 5,
            argument: false,
        };
        assert!(!l.in_scope(9));
        assert!(l.in_scope(10));
        assert!(l.in_scope(14));
        assert!(!l.in_scope(15), "the range is half-open");
    }

    /// The array-region trap: a primitive region's values are untagged, an object region's are
    /// tagged one by one. Reading one shape for the other misparses the whole region rather
    /// than failing, which is why both are pinned here.
    #[test]
    fn a_primitive_region_is_untagged_and_an_object_region_is_not() {
        let mut w = Writer::new(sizes());
        w.u8(b'I').i32(3).i32(10).i32(20).i32(30);
        assert_eq!(
            decode_array_region(&w.into_bytes(), sizes()).unwrap(),
            vec![Value::Int(10), Value::Int(20), Value::Int(30)]
        );

        let mut w = Writer::new(sizes());
        w.u8(b'L').i32(2);
        w.u8(b's').object_id(7); // a String element…
        w.u8(b'L').object_id(0); // …and a null one
        let region = decode_array_region(&w.into_bytes(), sizes()).unwrap();
        assert_eq!(region[0], Value::Object { tag: Tag::String, id: 7 });
        assert!(region[1].is_null());
    }

    #[test]
    fn a_static_field_is_told_apart_by_its_modifier_bits() {
        let instance =
            Field { id: 1, name: "total".into(), signature: "I".into(), mod_bits: 0x0002 };
        let shared =
            Field { id: 2, name: "COUNT".into(), signature: "I".into(), mod_bits: 0x000A };
        assert!(!instance.is_static());
        assert!(shared.is_static());
    }
}
