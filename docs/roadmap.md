# Roadmap

What's planned next for Arbor. Items are loosely ordered; the architectural block is a single contiguous effort, the others are independent.

This page is the forward-looking companion to [status.md](status.md), which describes the current state.

## Next

### Bevy Remote Protocol client

Inspector for live Bevy applications via the Bevy Remote Protocol. Entity tree, schema-driven component and resource editor with graphical widgets (color picker, `Vec` drag, entity-reference picker), performance analysis. Sidebar tree on the right, detail and console at the bottom.

### GitHub and GitLab issue trackers

First-party native issue tracking alongside the existing Linear and Jira integrations. Reuses the OAuth credentials and REST/GraphQL clients that already power the PR/MR, CI, and security surfaces.

### Releases

A first-party view of GitHub and GitLab releases: tag, asset list, and notes, with the option to cut a new release from any tag — notes pre-filled from the commit range since the previous one. Sits next to the existing PR/MR and CI panels, on the same provider clients.

### Markdown viewer and editor for project documentation

Wiki-style rendering and editing of repository documentation. Tables, callouts, checklists, footnotes, wikilinks `[[…]]`. The viewer ships first; the editor follows.

### Per-repo Pull / Fetch in the workspace manager

Fills the gap created by the bulk *Fetch all* / *Pull all* / *Tag all* actions: trigger the same operation against a single workspace member, without leaving the workspace manager.

### Customisable status bar

The footer ships a fixed set of chips today — current branch, ahead/behind, encoding, indexer status, jobs, notifications. Make each one toggleable and reorderable, and let the whole bar be hidden outright for users who want the absolute minimum chrome. Same drag-to-reorder, click-to-hide UX as the Activity Bar customise modal.

### System notifications

For events that deserve a desktop ping when Arbor isn't the foreground window — pipeline finished, push complete, MR/PR merged, fetch picked up new upstream commits — fall back from the in-app toast to the native OS notification stack (Windows toast, macOS Notification Center, libnotify on Linux). Per-event opt-in, off by default so the user picks what's worth being interrupted for.

### Visual interactive rebase

Drag-and-drop reordering with squash / edit / drop / fixup actions and a timeline preview. Today rebase delegates to the `git` CLI.

### Configurable GitFlow

The current GitFlow surface is opinionated — fixed branch prefixes, fixed base branches, fixed finish behaviour (tag, merge-back, branch deletion). The next iteration exposes these as per-repo settings: custom prefixes for feature/release/hotfix/support, configurable base and target branches, opt-in steps for finish actions, and hook points so plugins can extend the flow without forking it.

### Extension API in the Run / Compile plugin

The bundled *Run / Compile* plugin ships with a fixed set of language toolchains. Open it up so other plugins can register their own: how to detect a project (file patterns, manifest), how to invoke build and run, what to stream into the bottom panel. New language support then ships as a standalone plugin instead of as a patch to the bundled one.

### Optional AI plugin for commit messages, PR descriptions, branch names

Bundled but off by default. Calls a model of the user's choice — local via Ollama, or any of the usual API providers (Claude, OpenAI, …) — to draft the parts of git work that are mostly typing: commit messages from the staged diff, MR/PR descriptions from the commit range, branch names from a one-sentence intent, release notes from the commits since the last tag. Endpoint, model and prompt templates live in the plugin's settings; nothing leaves the machine unless the user explicitly wires it to a remote provider.

## Architectural block: backend split into separate crates

A multi-session cleanup pass: today most of the Rust backend lives in a single `arbor` crate. The goal is to peel off self-contained domains into dedicated crates so each one has a sharp dependency surface, can be built and tested in isolation, and could in principle be reused outside the app. The split is already underway — `cloud-storage` and the OAuth helpers were extracted first — and continues with `git_provider` (GitHub/GitLab clients), `pipeline`, the Lua `plugin` host, `jobs`, and the `git` wrapper layer.

Pure refactor: no user-visible behaviour changes, but it unlocks the subprocess-runtime block below and makes future contribution easier.

## Architectural block: plugin runtime as a subprocess

A single contiguous effort spread over multiple sessions. Each phase ships independently, but the real payoff comes from completing all three.

1. **Plugin subprocess runtime.** Move heavy plugins out of the in-process Lua sandbox into isolated subprocesses that talk to the host over IPC. Existing Lua plugins keep working; new APIs may be added and existing ones may shift slightly. The subprocess path is for plugins that need real Rust crates, native dependencies, or genuine isolation from the host.
2. **Migrate the Studio plugins.** JSON / TOML / YAML / RON / `.properties` studios become subprocess plugins. The CodeMirror + schema footprint leaves the host process.
3. **Migrate `cloud-storage`.** The opendal + yup-oauth2 footprint leaves the host process.

## Lower priority

### Source Export operation catalog

The unimplemented operations listed in [status.md](status.md#known-gaps) shipped progressively, removing the *"not implemented"* set entirely. Today they fail fast at run time.

## Maybe

> Both halves are awkward. Every language prints stack traces its own way — Rust `src/foo.rs:12:34`, Python `File "foo.py", line 12`, JS `at fn (path:line:col)`, Java `at Foo.bar(File.java:42)`, Go `file.go:123 +0xdead`, and the list goes on — and every editor has its own URL scheme for jumping to a location (`vscode://file/…`, `idea://open?file=…`, `subl://`, `zed://`…). Doing this well means a parser per stack format and a launcher per IDE the user has configured under Settings → IDE; any miss silently drops the link, and people fall back to copy-pasting paths into a terminal — worse than leaving the text plain.

### Clickable stack traces in the Job Output panel

Detect file paths inside JobOutput log lines — relative paths resolved against the job's working directory, line and column captured when present — and turn them into links. Clicking jumps to the user's preferred editor for that language, reusing the IDE picker that already lives in settings.

## A note on the Experimental plugins

`commit-validator`, `gitignore-suggester`, and `repo-bookmarks` (marked **Experimental** in their entries on the [arbor-extensions](https://github.com/nightprint-studio/arbor-extensions) registry) are unlikely to receive proactive hardening. They're tools the maintainer doesn't reach for day-to-day. They'll get attention based on user-reported issues, and they'll be promoted to **Functional** or **Stable** when feedback says they're ready.
