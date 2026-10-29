---
title: "Imber: Debugging with Imber"
summary: "Breakpoints, launching and attaching, stops and snapshots, and the off-by-default tools that change a running program."
keywords: ["debugger", "breakpoint", "logpoint", "stop", "snapshot", "launch", "attach", "mutate", "idle"]
related: ["Imber: Running and observing", "Imber: Permissions and modes", "Imber: Settings reference"]
source: src/lib/components/bennu/docs/imber/Debugging.svelte
---
<!-- Generated from the in-app docs - do not edit. Edit the Svelte pages and run `node scripts/build-help-index.mjs`. -->

# Imber: Debugging with Imber

With the Debugger family on, Imber can debug a Java program instead of guessing: it sets breakpoints and logpoints, runs the program under the debugger or attaches to one, waits for a stop, reads the stack and values and steps, on the very session the Debug panel shows.

The debugger itself is described on the **Debugging** and **Frames, values and watches** pages; this page covers what is specific to Imber.

## What it can do

| Step | Tools | Asks? |
| --- | --- | --- |
| Set, list, remove breakpoints and logpoints | `debug_set_breakpoint`, `debug_list_breakpoints`, `debug_remove_breakpoint` | No |
| Start a session | `debug_launch` (builds and runs a main class), `debug_attach` (to a JVM on 127.0.0.1) | Every time |
| Wait and look | `debug_wait_stop`, `debug_stack`, `debug_variables`, `debug_evaluate`, `debug_output` | No |
| Move | `debug_step` (several steps at once), `debug_resume`, `debug_run_to` (like Run to Cursor), `debug_break_on_exception` | No |
| Let go | `debug_detach` | No |

## Its breakpoints are visible and yours

Breakpoints Imber sets are saved in the project's breakpoints, drawn with a sparkle in the gutter and badged *Imber* in the Breakpoints list (`Ctrl+Shift+F8`). You can disable or delete them like any other. It never changes or removes one you set, and holds at most 20. A logpoint can say "only when `total > 100`" through a condition and a pass count, and can ask for values to be captured at the stop.

## Reading values cannot change anything

Values are read as *paths* (`order.customer.name`, `items[2].price`) through fields and array slots. Method calls, constructors, assignments and `++` are refused before they reach the program, with or without approval. Variables named like secrets (password, token, secret, apiKey, credential…) are masked. Results are capped: frames, variables and long values are cut and say so.

## The session belongs to the conversation

The session outlives one message: when a run ends the program stays alive (suspended, if it was stopped at a breakpoint, so you can inspect it) and the next message goes on from there. There is one debug session per conversation. It ends when Imber detaches (a program it launched is stopped, one it attached to is resumed and left running), when the program exits, after the idle time in Settings › Imber › Debug session idle time (30 minutes by default, counted from the last debug tool), when the project closes, or when you press *Stop* beside the debugger controls, where the line "Imber … ends in N min if unused" shows how long is left. Breakpoints it set without starting a session stay for you to use.

## When the program stops while Imber is not waiting

Bennu itself notices the stop and takes a snapshot while the program is still suspended: where it stopped, the top of the stack, the top frame's locals and the values the breakpoint asked to capture (capped and with secrets masked). What happens next follows the conversation:

- **A run is active**: the snapshot reaches Imber as a system note with its next tool result.
- **No run is active**: the note is queued for your next message, and the thread shows a card ("Stopped at Order.java:42 in OrderService.total") with *Send to Imber now*.
- **Start a run by itself**: off by default (Settings › Imber › Start a run when the program stops, or per conversation from the composer's menu › Decisions). At most three an hour per conversation. It spends money and counts toward your limits.

## Changing the program: a separate switch

*Let it change the program it debugs* (off by default) offers four tools: `debug_throw` creates an exception inside the program (its constructor runs there) and throws it from the line the stopped thread is on, `debug_force_return` makes the method on top of the stack return a literal value at once (finally blocks included), `debug_drop_frame` discards a frame so the call runs again, `debug_set_value` writes a number, boolean, character, String or null to a local, a field or a path such as `order.total` (final and static fields are refused). All need a stop; the last two need a VM that supports them. The program changes while it runs and **there is no undo**: side effects already produced stay. Imber asks once per conversation (Autonomous does not skip the question) and every change is listed in the thread and in the Debug panel as an *Imber action*.

The debugger tools target Java programs: a main class to launch, or a JVM listening on a local debug port.
