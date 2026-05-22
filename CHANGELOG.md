# Changelog

All notable changes to Arbor are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- MR / PR sidebar no longer shows a raw 404 when the remote has pull/merge requests
  disabled — the sidebar, Command Palette entries, and `arbor://mr/open/<n>` deep link
  all gracefully report the feature as unavailable.
- Plugin Logs panel now shows plugin failures that used to be terminal-only: runtime
  errors from hook handlers and service callbacks, `arbor.ui.tree.set` payload
  validation errors, and malformed `plugin.toml` manifests (the broken folder also
  shows up as a "Failed to load" entry in the Plugin Manager).

## [0.1.0] — 2026-05-21

Initial public release.
