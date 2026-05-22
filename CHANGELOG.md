# Changelog

All notable changes to Arbor are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `Ctrl+Shift+Enter` in the commit message field commits and pushes in one step.
- The Commit split-button menu now shows shortcut hints and tinted icons next
  to each option, matching the graph context menu.
- Appearance setting to switch the title-bar window controls between the
  Mac-inspired coloured trio (default) and a flat Windows-/IntelliJ-style set;
  the same switch also restyles the close button in modal and panel headers.

### Fixed

- MR / PR sidebar no longer shows a raw 404 when the remote has pull/merge requests
  disabled — the sidebar, Command Palette entries, and `arbor://mr/open/<n>` deep link
  all gracefully report the feature as unavailable.
- Plugin Logs panel now shows plugin failures that used to be terminal-only: runtime
  errors from hook handlers and service callbacks, `arbor.ui.tree.set` payload
  validation errors, and malformed `plugin.toml` manifests (the broken folder also
  shows up as a "Failed to load" entry in the Plugin Manager).
- Restored Unicode glyphs (em-dashes, arrows, box-drawing characters, bullets) in
  the Docs panel pages and exported Markdown / HTML docs, which had been corrupted
  by a previous round-trip through Windows-1252.
- Docs Markdown export now preserves inline `<code>` and other formatting inside
  table cells and headings; previously e.g. a table row referencing `<h1>` would
  render as a bare HTML tag (and be stripped by GitHub's renderer) instead of as
  inline code.

## [0.1.0] — 2026-05-21

Initial public release.
