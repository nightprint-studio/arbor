# Roadmap

What's planned next for Arbor. Items are loosely ordered; the architectural block is a single contiguous effort, the others are independent.

This page is the forward-looking companion to [status.md](status.md), which describes the current state.

## Next

### Bevy Remote Protocol client

Inspector for live Bevy applications via the Bevy Remote Protocol. Entity tree, schema-driven component and resource editor with graphical widgets (color picker, `Vec` drag, entity-reference picker), performance analysis. Sidebar tree on the right, detail and console at the bottom.

### GitHub and GitLab issue trackers

First-party native issue tracking alongside the existing Linear and Jira integrations. Reuses the OAuth credentials and REST/GraphQL clients that already power the PR/MR, CI, and security surfaces.

### Markdown viewer and editor for project documentation

Wiki-style rendering and editing of repository documentation. Tables, callouts, checklists, footnotes, wikilinks `[[…]]`. The viewer ships first; the editor follows.

### Per-repo Pull / Fetch in the workspace manager

Fills the gap created by the bulk *Fetch all* / *Pull all* / *Tag all* actions: trigger the same operation against a single workspace member, without leaving the workspace manager.

### Visual interactive rebase

Drag-and-drop reordering with squash / edit / drop / fixup actions and a timeline preview. Today rebase delegates to the `git` CLI.

## Architectural block: plugin runtime as a subprocess

A single contiguous effort spread over multiple sessions. Each phase ships independently, but the real payoff comes from completing all three.

1. **Plugin subprocess runtime.** Move heavy plugins out of the in-process Lua sandbox into isolated subprocesses that talk to the host over IPC. Existing Lua plugins keep working; new APIs may be added and existing ones may shift slightly. The subprocess path is for plugins that need real Rust crates, native dependencies, or genuine isolation from the host.
2. **Migrate the Studio plugins.** JSON / TOML / YAML / RON / `.properties` studios become subprocess plugins. The CodeMirror + schema footprint leaves the host process.
3. **Migrate `cloud-storage`.** The opendal + yup-oauth2 footprint leaves the host process.

## Lower priority

### Source Export operation catalog

The unimplemented operations listed in [status.md](status.md#known-gaps) shipped progressively, removing the *"not implemented"* set entirely. Today they fail fast at run time.

## A note on the Experimental plugins

`commit-validator`, `gitignore-suggester`, and `repo-bookmarks` (marked **Experimental** in their entries on the [arbor-extensions](https://github.com/nightprint-studio/arbor-extensions) registry) are unlikely to receive proactive hardening. They're tools the maintainer doesn't reach for day-to-day. They'll get attention based on user-reported issues, and they'll be promoted to **Functional** or **Stable** when feedback says they're ready.
