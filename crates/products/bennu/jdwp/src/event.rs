//! What the VM says on its own initiative.
//!
//! Events do not arrive one per packet: the VM sends a **composite** — one suspend policy and
//! a list of events that happened together. Two breakpoints on the same line of two threads
//! come as one packet, and a client that assumed one event per packet would lose the second.
//!
//! Only the events a breakpoint debugger asks for are decoded; anything else is kept as
//! [`Event::Other`] with its kind, because dropping an event silently is how a client ends up
//! waiting forever for a VM that already answered.

use crate::codec::{Id, IdSizes, Location, Reader, Tag};
use crate::error::Result;

/// What the VM does to the program when an event fires. Requested per event; echoed on the
/// composite so a client knows what state it is now in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuspendPolicy {
    /// Nothing stops. A logging breakpoint.
    None,
    /// Only the thread that hit it. What "step over" needs.
    EventThread,
    /// Every thread. What a breakpoint in a server usually wants — the request handler you are
    /// looking at is rarely alone.
    All,
}

impl SuspendPolicy {
    pub fn to_byte(self) -> u8 {
        match self {
            SuspendPolicy::None => 0,
            SuspendPolicy::EventThread => 1,
            SuspendPolicy::All => 2,
        }
    }

    pub fn from_byte(b: u8) -> SuspendPolicy {
        match b {
            0 => SuspendPolicy::None,
            1 => SuspendPolicy::EventThread,
            _ => SuspendPolicy::All,
        }
    }
}

/// The event kinds this crate names. The numbers are JDWP's own.
pub mod kind {
    pub const SINGLE_STEP: u8 = 1;
    pub const BREAKPOINT: u8 = 2;
    pub const EXCEPTION: u8 = 4;
    pub const THREAD_START: u8 = 6;
    pub const THREAD_DEATH: u8 = 7;
    pub const CLASS_PREPARE: u8 = 8;
    pub const VM_START: u8 = 90;
    pub const VM_DEATH: u8 = 99;
}

/// One thing that happened in the VM.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// The VM is up and suspended at its start, if that is what was asked for.
    VmStart { request: i32, thread: Id },
    /// The program ended.
    VmDeath { request: i32 },
    /// A breakpoint was reached. `thread` is suspended if the policy said so.
    Breakpoint { request: i32, thread: Id, location: Location },
    /// A step finished.
    Step { request: i32, thread: Id, location: Location },
    /// A throwable was thrown. `catch_location` is where it will be caught — `class` is `0`
    /// when nothing catches it, which is exactly the "uncaught" a debugger wants to stop on.
    Exception {
        request: i32,
        thread: Id,
        location: Location,
        exception: (Tag, Id),
        catch_location: Location,
    },
    /// A class was loaded. What a breakpoint set before the class existed is waiting for.
    ClassPrepare { request: i32, thread: Id, signature: String },
    ThreadStart { request: i32, thread: Id },
    ThreadDeath { request: i32, thread: Id },
    /// A kind this crate does not decode. Carried rather than dropped: the request id is
    /// enough to know whose it was, and the suspend policy still applies to the VM whether or
    /// not we understood the event.
    Other { request: i32, kind: u8 },
}

impl Event {
    /// The event-request id this event answers — the number [`crate::command`]'s
    /// `event_request_set` returned.
    pub fn request(&self) -> i32 {
        match self {
            Event::VmStart { request, .. }
            | Event::VmDeath { request }
            | Event::Breakpoint { request, .. }
            | Event::Step { request, .. }
            | Event::Exception { request, .. }
            | Event::ClassPrepare { request, .. }
            | Event::ThreadStart { request, .. }
            | Event::ThreadDeath { request, .. }
            | Event::Other { request, .. } => *request,
        }
    }

    /// The thread it happened on, when it happened on one.
    pub fn thread(&self) -> Option<Id> {
        match self {
            Event::VmStart { thread, .. }
            | Event::Breakpoint { thread, .. }
            | Event::Step { thread, .. }
            | Event::Exception { thread, .. }
            | Event::ClassPrepare { thread, .. }
            | Event::ThreadStart { thread, .. }
            | Event::ThreadDeath { thread, .. } => Some(*thread),
            _ => None,
        }
    }

    /// Where it happened, when that means anything.
    pub fn location(&self) -> Option<Location> {
        match self {
            Event::Breakpoint { location, .. }
            | Event::Step { location, .. }
            | Event::Exception { location, .. } => Some(*location),
            _ => None,
        }
    }
}

/// A composite: everything that happened at once, and what the VM did about it.
#[derive(Debug, Clone, PartialEq)]
pub struct Composite {
    pub policy: SuspendPolicy,
    pub events: Vec<Event>,
}

/// Decode a composite event packet's payload (`Event.Composite`, command set 64, command 100).
///
/// An event whose kind is not decoded still consumes its bytes — which is why the unknown case
/// stops reading the rest of the packet: without knowing an event's shape there is no way to
/// find where the next one starts, and reading on would produce nonsense. The events decoded
/// so far are returned, which is the recoverable half.
pub fn parse_composite(data: &[u8], sizes: IdSizes) -> Result<Composite> {
    let mut r = Reader::new(data, sizes);
    let policy = SuspendPolicy::from_byte(r.u8()?);
    let count = r.i32()?.max(0);
    let mut events = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let kind = r.u8()?;
        let request = r.i32()?;
        let event = match kind {
            kind::VM_START => Event::VmStart { request, thread: r.object_id()? },
            kind::VM_DEATH => Event::VmDeath { request },
            kind::BREAKPOINT => Event::Breakpoint {
                request,
                thread: r.object_id()?,
                location: r.location()?,
            },
            kind::SINGLE_STEP => {
                Event::Step { request, thread: r.object_id()?, location: r.location()? }
            }
            kind::EXCEPTION => {
                let thread = r.object_id()?;
                let location = r.location()?;
                let tag = Tag::from_byte(r.u8()?)?;
                let id = r.object_id()?;
                Event::Exception {
                    request,
                    thread,
                    location,
                    exception: (tag, id),
                    catch_location: r.location()?,
                }
            }
            kind::CLASS_PREPARE => {
                let thread = r.object_id()?;
                let _ref_type_tag = r.u8()?;
                let _type_id = r.reference_type_id()?;
                let signature = r.string()?;
                let _status = r.i32()?;
                Event::ClassPrepare { request, thread, signature }
            }
            kind::THREAD_START => Event::ThreadStart { request, thread: r.object_id()? },
            kind::THREAD_DEATH => Event::ThreadDeath { request, thread: r.object_id()? },
            other => {
                // Its payload is an unknown length, so nothing after it can be located.
                events.push(Event::Other { request, kind: other });
                break;
            }
        };
        events.push(event);
    }
    Ok(Composite { policy, events })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::Writer;

    fn sizes() -> IdSizes {
        IdSizes { field: 8, method: 8, object: 8, reference_type: 8, frame: 8 }
    }

    fn location() -> Location {
        Location { type_tag: 1, class: 0x0A, method: 0x0B, index: 12 }
    }

    #[test]
    fn one_packet_can_carry_two_events() {
        // Two threads on the same breakpoint: one composite, two events. A client that read
        // only the first would leave a thread suspended with nobody watching it.
        let mut w = Writer::new(sizes());
        w.u8(SuspendPolicy::All.to_byte()).i32(2);
        w.u8(kind::BREAKPOINT).i32(7).object_id(100).location(location());
        w.u8(kind::BREAKPOINT).i32(7).object_id(101).location(location());
        let c = parse_composite(&w.into_bytes(), sizes()).unwrap();
        assert_eq!(c.policy, SuspendPolicy::All);
        assert_eq!(c.events.len(), 2);
        assert_eq!(c.events[0].thread(), Some(100));
        assert_eq!(c.events[1].thread(), Some(101));
        assert_eq!(c.events[0].request(), 7);
        assert_eq!(c.events[0].location(), Some(location()));
    }

    #[test]
    fn an_uncaught_exception_is_the_one_with_no_catch_location() {
        let mut w = Writer::new(sizes());
        w.u8(SuspendPolicy::EventThread.to_byte()).i32(1);
        w.u8(kind::EXCEPTION).i32(3).object_id(9).location(location());
        w.u8(b'L').object_id(555);
        w.location(Location { type_tag: 0, class: 0, method: 0, index: 0 });
        let c = parse_composite(&w.into_bytes(), sizes()).unwrap();
        match &c.events[0] {
            Event::Exception { exception, catch_location, .. } => {
                assert_eq!(exception.1, 555);
                assert_eq!(catch_location.class, 0, "class 0 means nothing catches it");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_class_prepare_carries_the_signature_a_pending_breakpoint_waits_for() {
        let mut w = Writer::new(sizes());
        w.u8(SuspendPolicy::None.to_byte()).i32(1);
        w.u8(kind::CLASS_PREPARE).i32(1).object_id(2).u8(1).reference_type_id(33);
        w.string("Lcom/acme/Order;").i32(7);
        let c = parse_composite(&w.into_bytes(), sizes()).unwrap();
        assert_eq!(
            c.events[0],
            Event::ClassPrepare { request: 1, thread: 2, signature: "Lcom/acme/Order;".into() }
        );
    }

    #[test]
    fn an_unknown_kind_is_kept_rather_than_dropped() {
        let mut w = Writer::new(sizes());
        w.u8(SuspendPolicy::None.to_byte()).i32(1);
        w.u8(41).i32(5); // METHOD_ENTRY — not decoded here
        let c = parse_composite(&w.into_bytes(), sizes()).unwrap();
        assert_eq!(c.events, vec![Event::Other { request: 5, kind: 41 }]);
    }

    #[test]
    fn vm_death_ends_the_session_and_names_no_thread() {
        let mut w = Writer::new(sizes());
        w.u8(SuspendPolicy::All.to_byte()).i32(1);
        w.u8(kind::VM_DEATH).i32(0);
        let c = parse_composite(&w.into_bytes(), sizes()).unwrap();
        assert_eq!(c.events[0], Event::VmDeath { request: 0 });
        assert_eq!(c.events[0].thread(), None);
    }
}
