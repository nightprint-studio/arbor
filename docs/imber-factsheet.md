# Imber — fact sheet

Source material for writing about Imber (a joke, a promo, a pitch). Everything below is a feature that exists in the code;
the last section says what has not been tried in real use yet, so a text can stay truthful if it wants to.

## In one paragraph

Imber is the AI assistant built into **Bennu**, the Java (and Rust, and Node) IDE of the **Arbor** suite. It runs on Claude with the
user's own Anthropic API key and works through Bennu's own tools — the code index, usages, rename, build, tests, debugger, profiler —
instead of a shell. It asks before anything that matters, can be reviewed and undone, shows what it costs, and can even
start the program, set breakpoints, read the logs and take a screenshot of the web page it just changed. The name is Latin for *rain*:
Arbor is the forest, the products are the birds that live in its trees, and the assistant is the water the whole forest runs on.

## Who it is for

Developers on enterprise Java projects (Spring, Quarkus, Maven/Gradle multi-module), who want an agent that understands the project
the way their IDE does — and who want to stay in control of what it touches, and of what it costs.

## What it can do

**Understand the project**
- Reads, searches and navigates with the IDE's index (symbols, usages, callers, beans, endpoints, framework catalogs), not just text search.
- Explains a selected passage of code: select it, click the sparkle chip (or speak), ask.
- Draws diagrams with Mermaid (architecture, flows, call chains, ER), validated before they are shown.
- Answers "how do I do X in Bennu" from Bennu's own documentation, never from memory.

**Change the project**
- Exact-text edits, many-file edits as one change, rename symbol across the project, add a parameter, structural search-and-replace.
- Every change is a *proposal* with a diff you approve, or is applied at once depending on the mode — and can be undone (Undo, Local History).
- Works on an **isolated copy** (a git worktree or a flat copy) for risky work, then shows the combined diff before applying it back.
- If two proposals touch the same file, the second becomes "outdated" instead of overwriting the first, and can be re-applied on the current file.

**Run and observe**
- Starts services and recipes (several services in order, with readiness checks), reads their logs, calls local HTTP endpoints, runs saved scenarios with assertions.
- Follows a request across services by correlation id ("service B returned 500 because A sent a null").
- Reads logs the smart way: digests of exceptions, SQL queries, requests and warnings instead of thousands of raw lines; waits for a line to appear.
- A fair build/test queue when several agents work at once, and error attribution ("this error is in a file another conversation edited").

**Debug (Java)**
- Sets breakpoints (with conditions, log messages, and values to capture), launches or attaches, steps, inspects variables, stops on exceptions.
- Gets *notified* when the program stops — even when you trigger the breakpoint by hand later — with the snapshot it asked for.
- Optional, off by default and always approved by you: change the running program (throw an exception, force a return, set a value).
- Its breakpoints show up in the editor with a sparkle, so you always see what it did.

**Profile**
- Records with Java Flight Recorder (or Node's CPU profile, or `samply` for Rust), shows flame graphs and hot methods, and writes the numbers *inside the editor* next to the method ("38% CPU · 120 ms").

**Games (Bevy)**
- Connects to a running Bevy game over BRP, queries entities and resources, and — only if you switch it on — changes them.

**Web front end**
- Takes screenshots of the page it is working on (localhost only) and reads console errors and failed requests.

**Helpers**
- Starts sub-agents with a clean context for side tasks (exploration, a second look), with permissions never wider than its own, a shared cost cap, a visible list and a Stop button.
- Skills: drop a `SKILL.md` in a folder and Imber can use it — but only the skills you switch on.

**Voice**
- Dictation with a local Whisper model: nothing leaves the machine. Hold a key, speak, Imber answers in a large listening window. A guided model picker recommends a model for your PC.
- An optional "Ehi Imber" wake word, **off by default**, with a clear warning (open microphone, uses CPU).

## How it stays safe

- **Three modes**: Manual (every edit is a proposal), Accept edits (applies at once inside what you allowed), Autonomous (can also open the files its own build reports an error in).
- **Four reaches**: this file, this project, all open projects, anywhere.
- **Hard limits in every mode**: secret files (`.env`, keys) never; deletions outside its area ask; no `git push` exists for it.
- **It asks when a decision is yours**: architecture, naming, two reasonable ways to do something — with 2–4 options and a recommendation. A setting lets you make it decide alone ("fa' tu"), with free-text rules of your own.
- **Its key lives in a credential vault**, and goes only to Anthropic.
- **Voice** is local; screenshots and HTTP only touch localhost.

## How it keeps cost down

- Spending limits in euros (day / week / month / per conversation), shown live, shared across concurrent runs.
- Prompt caching tuned so the conversation history is cheap to repeat, with a breakdown of where the tokens go.
- Unchanged files are not sent twice; long build/test/log output becomes a digest you can expand; old tool results are folded away in batches.
- Compaction with a cheaper side model; a confirmation (switchable) before a model change that would rebuild a long conversation's cache.

## Numbers and facts that are safe to quote

- Runs on Claude (Anthropic) with the user's own API key.
- 19 pages of in-app documentation, also exported as Markdown for other AIs.
- Three permission modes, four reaches, around a hundred tools in families that can each be switched off.
- Local speech recognition with Whisper models from about 75 MB to about 3 GB; voice processes show up in the Processes window.
- Cross-platform (Windows, macOS, Linux) by design.
- Built into Bennu; the whole suite is Tauri + Rust + Svelte, no Electron.

## What has not been tried in real life yet (stay honest)

This was built in one very long session and compiled for the first time at the end. Arbor starts; the assistant's features have not yet had real-world use.
Not yet verified: the voice engine on a microphone, the debugger's state-changing actions on a live JVM, the profiler on a real recording,
several chats working at once, the screenshot tool with each browser, and quality/cost figures against other assistants
(no head-to-head comparison has been run). A promo can safely say "built to", "designed to" and "can" — not "beats".

## Tone suggestions for the joke

- The name: *Imber*, rain; "it waters the forest".
- The running gag: it asks before it touches anything, and it will *definitely* tell you what it cost.
- The bird metaphor: the products are the birds; Imber is the weather.
- The line it will not cross: it has no `git push`. Ever.
