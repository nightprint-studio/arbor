//! `bennu-jdwp` — a client for the Java Debug Wire Protocol.
//!
//! The transport half of a debugger: connect to a JVM, ask it things, be told when it stops.
//! What a *session* is — which breakpoints exist, what the UI shows, what "step over" does to
//! the panel you are looking at — is deliberately not here.
//!
//! ## Talking to a JVM at all
//!
//! The program has to be started for it. One flag:
//!
//! ```text
//! java -agentlib:jdwp=transport=dt_socket,server=y,suspend=n,address=127.0.0.1:0 …
//! ```
//!
//! `server=y` makes the VM listen and print the port it chose (`address=0` picks a free one,
//! and the line it prints is the only way to learn which). `suspend=y` holds the program
//! before `main` so a breakpoint in start-up code can be set in time; `suspend=n` lets it run
//! and attaches to it going.
//!
//! ## The shape of a session
//!
//! ```no_run
//! use bennu_jdwp::prelude::*;
//!
//! # fn main() -> Result<()> {
//! let (client, events) = Client::attach("127.0.0.1:5005")?;
//!
//! // A breakpoint is a location, and a location is a class, a method and a bytecode index —
//! // which is why the line number has to be translated first.
//! let classes = classes_by_signature(&client, &class_signature("com.acme.Order"))?;
//! if let Some(class) = classes.first() {
//!     let methods = methods(&client, class.id)?;
//!     if let Some(location) = location_of_line(&client, class.id, &methods, 118)? {
//!         set_breakpoint(&client, location, SuspendPolicy::All)?;
//!     }
//! }
//! resume_vm(&client)?;
//!
//! // Events arrive on their own channel, from the reader thread.
//! for composite in events {
//!     for event in composite.events {
//!         if let Event::Breakpoint { thread, .. } = event {
//!             for frame in frames(&client, thread)? {
//!                 let _ = frame.location; // → a file and a line, via `Method.LineTable`
//!             }
//!         }
//!     }
//! }
//! # Ok(()) }
//! ```
//!
//! ## Three things this protocol will do to you
//!
//! **Identifier widths are negotiated, not fixed.** `VirtualMachine.IDSizes` is asked first
//! and every field after it depends on the answer. Getting it wrong does not fail loudly — it
//! shifts every subsequent field by four bytes and produces plausible nonsense. Hence
//! [`IdSizes`] threaded through the codec rather than a constant.
//!
//! **Events come in batches.** The VM sends a *composite*: one suspend policy and however many
//! events happened together. Two threads on one breakpoint is one packet, and a client that
//! reads one event per packet leaves the second thread suspended with nobody watching it.
//!
//! **Never call the VM from the thread draining the events.** A reply is delivered by the
//! reader thread; blocking that thread on a reply is a deadlock. Handle events on a worker —
//! the same rule the Arbor shell learned about its own reverse channel.
//!
//! ## What is not here yet
//!
//! Method invocation (`toString()` on a watched object, so an object renders as `Type@id`
//! with its fields underneath rather than as its own description), field *writes*, and
//! hot-swap. Each is a command number and a reply shape; none of them change what is above.

pub mod client;
pub mod codec;
pub mod command;
pub mod error;
pub mod event;
pub mod packet;
pub mod prelude;

#[doc(inline)]
pub use crate::client::Client;
#[doc(inline)]
pub use crate::codec::{IdSizes, Location, Value};
#[doc(inline)]
pub use crate::error::{JdwpError, Result};
#[doc(inline)]
pub use crate::event::{Composite, Event, SuspendPolicy};
