# bennu-jdwp

A client for the **Java Debug Wire Protocol** — the transport half of a debugger. Connect to a
JVM, ask it things, be told when it stops.

What a *session* is — which breakpoints exist, what the panel shows, what "step over" means to
the file you are looking at — is deliberately not here. This crate ends at the socket.

## Starting a program you can attach to

```text
java -agentlib:jdwp=transport=dt_socket,server=y,suspend=n,address=127.0.0.1:0 …
```

`server=y` makes the VM listen and print the port it picked (`address=…:0` picks a free one,
and that printed line is the only way to learn which). `suspend=y` holds it before `main`, so a
breakpoint in start-up code can be set in time.

## A session, in outline

```rust
use bennu_jdwp::prelude::*;

let (client, events) = Client::attach("127.0.0.1:5005")?;

// A breakpoint is a *location*: a class, a method and a bytecode index. The line number is
// translated through the class's line table — which exists only if it was compiled with `-g`.
let classes = classes_by_signature(&client, &class_signature("com.acme.Order"))?;
let methods = methods(&client, classes[0].id)?;
if let Some(loc) = location_of_line(&client, classes[0].id, &methods, 118)? {
    set_breakpoint(&client, loc, SuspendPolicy::All)?;
}
resume_vm(&client)?;

for composite in events {
    for event in composite.events {
        if let Event::Breakpoint { thread, .. } = event {
            let stack = frames(&client, thread)?;   // innermost first
        }
    }
}
```

## Three things this protocol will do to you

**Identifier widths are negotiated, not fixed.** `VirtualMachine.IDSizes` is the first command
any client sends, and every field after it depends on the answer: an `objectID` is four bytes
on one VM and eight on the next. Getting it wrong does not fail loudly — it shifts every
subsequent field and produces plausible nonsense. Hence `IdSizes` threaded through the codec
instead of a constant.

**Events come in batches.** The VM sends a *composite*: one suspend policy and however many
events happened at once. Two threads hitting one breakpoint is a single packet, and a client
that reads one event per packet leaves the second thread suspended with nobody watching it.

**Never call the VM from the thread draining the events.** Replies are delivered by the reader
thread; blocking it on a reply is a deadlock — the same reverse-channel deadlock the Arbor
shell hit going out-of-process. Handle events on a worker.

## What is here

| Area | Commands |
|---|---|
| VM | `Version`, `IDSizes`, `ClassesBySignature`, `Suspend`, `Resume`, `Dispose` |
| Types | `ReferenceType.Signature` / `.Methods` / `.Fields`, `ClassType.Superclass` |
| Methods | `Method.LineTable`, `Method.VariableTable` |
| Stopping | `EventRequest.Set` (breakpoint · class-prepare · step · exception), `EventRequest.Clear` |
| Threads | `ThreadReference.Name` / `.Frames` / `.Resume` |
| Values | `StackFrame.GetValues` / `.ThisObject`, `ObjectReference.ReferenceType` / `.GetValues`, `ArrayReference.Length` / `.GetValues`, `StringReference.Value` |

Plus `location_of_line`, which is the piece every debugger writes: the first line-table entry
at or after the line you clicked, across every method of the class — because a lambda body and
an inner class compile into methods of their own, and a line with no code has no entry at all.

## Two more things worth knowing

**A step wants a class filter.** An unfiltered *step into* on `list.add(order)` lands in
`ArrayList.add`, then `ensureCapacity`, then `Arrays.copyOf`. `request_step` takes exclusion
patterns (`java.*`, `sun.*`) for exactly this, and without them stepping is unusable rather
than thorough.

**"Caught" means caught by anyone.** An exception request splits caught from uncaught, and the
VM decides caught by whether *any* frame catches it — including the framework three frames up
that will swallow it. Asking for caught throws of `Throwable` under Spring stops thousands of
times before `main`.

## What is not here yet

Method invocation (`toString()` on a watched object — so objects render as `Type@id` with
their fields underneath rather than as their own description), field *writes*, and hot-swap.
Each is a command number and a reply shape; none of them change the shape above.

## Dependencies

None. JDWP is a big-endian binary protocol over a socket, `std::net` and `std::io` are the
whole of what it needs, and a debugger's transport is the last place to take a dependency whose
version someone else controls.
