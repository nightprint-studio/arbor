# Changelog — merula

All notable changes to **merula** (the live-coding music DSL, engine and audio
stack shipped with Arbor) are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- **Offline render loads the sample banks.** `render_offline*` only ever
  installed the built-in `synth.*` presets, so every bounce — single render,
  stems, *Export all* — played sampled instruments on the fallback synth while
  live playback played the real thing. The shell now hands the render the same
  registry the audio thread uses (`render_offline_with_registry`; the old entry
  points stay valid as built-ins-only shortcuts). Previously exported files must
  be re-rendered.
- **An unresolved instrument name is reported, not swallowed.** A bounce now
  warns once per unresolved name (naming the instrument and the output file)
  instead of quietly substituting the fallback synth.

### Changed

- **Alpha concluded.** The merula alpha is now considered closed — the DSL,
  pattern engine, audio stack, instruments and the import/transcribe pipeline
  are feature-complete for the alpha scope. Further work moves on to the next
  development phase.
