/**
 * Shape of every piece of mocked data behind the GroveShell (Step 0). These
 * mirror what the real backend (`arbor-grove-*` crates + IPC) will eventually
 * stream, so swapping mock → live later is a data-source change, not a UI
 * rewrite. Nothing here imports anything Arbor-specific — grove stays
 * self-contained and extractable.
 */

export type LogLevel = 'trace' | 'debug' | 'info' | 'warn' | 'error';

/** A `.grove` source file inside a grove project (folder + `grove.toml`). */
export interface GroveFile {
  id:       string;
  /** File name with extension, e.g. `song.grove`. */
  name:     string;
  /** Project-relative path, e.g. `lib/drums.grove`. */
  path:     string;
  /** Whether this file is a library (imported-only, its `tracks(…)` ignored). */
  library:  boolean;
  /** Raw source text — what the editor renders. */
  source:   string;
}

/** A grove project = folder with `grove.toml` + its `.grove` files. */
export interface GroveProject {
  id:       string;
  name:     string;
  /** The "for whom" blurb from `grove.toml`. */
  audience: string;
  path:     string;
  files:    GroveFile[];
}

/** An outline entry for the active file (tracks / fn / let / import). */
export interface OutlineEntry {
  id:    string;
  kind:  'track' | 'fn' | 'let' | 'import';
  label: string;
  /** 1-based line in the source the symbol is declared on. */
  line:  number;
}

/** A registry voice — default synth preset or a VSCO 2 sampler instrument. */
export interface Voice {
  id:        string;
  /** Dotted registry name, e.g. `synth.bass` or `strings.violin`. */
  name:      string;
  kind:      'synth' | 'sampler';
  /** VSCO sampler voices need their WAV bank downloaded before first note. */
  installed: boolean;
}

/** One note block on a track's read-only piano roll. */
export interface RollNote {
  /** Start position in cycles (absolute timeline). */
  start: number;
  /** Length in cycles. */
  len:   number;
  /** Pitch row, 0 = bottom of the visible range … rows-1 = top. */
  row:   number;
  /** Whether this hap is "active now" (highlighted by the transport). */
  active?: boolean;
}

/** A region on the arrangement timeline (Logic-style block: a stretch of
 *  cycles where the track plays). Maps to `arrange(cycles(n, …))` sections. */
export interface Region {
  /** Start position in cycles (absolute timeline). */
  start: number;
  /** Length in cycles. */
  len:   number;
  /** Short label drawn on the block, e.g. the section or pattern name. */
  label: string;
  /** Visual content density 0..1 — how busy the inner mini-pattern looks. */
  density: number;
}

/** A track/channel — one mixer strip, one viz lane. */
export interface Track {
  id:       string;
  name:     string;
  /** Accent colour index (0..N) — used for the lane + strip tint. */
  colorIdx: number;
  /** Voice/instrument label shown in the lane header. */
  voice:    string;
  muted:    boolean;
  soloed:   boolean;
  gain:     number;   // 0..1
  pan:      number;   // 0..1 (0.5 = center)
  room:     number;   // 0..1 reverb send
  /** Live output meter level 0..1 (peak). */
  meter:    number;
  /** Number of pitch rows the roll spans. */
  rollRows: number;
  notes:    RollNote[];
  /** Arrangement regions across the absolute timeline. */
  regions:  Region[];
}

/** A labelled span of the arrangement (intro / build / full / outro). */
export interface Section {
  label: string;
  /** Start cycle. */
  start: number;
  /** Length in cycles. */
  len:   number;
}

/** A console log line (already gated to the threshold, like the real engine). */
export interface LogLine {
  id:     number;
  level:  LogLevel;
  /** Cycle position the log was emitted at (engine clock). */
  cycle:  number;
  /** Source — `eval`, `engine`, `audio`, or a track/fn name. */
  source: string;
  text:   string;
}

/** A diagnostic for the Problems panel (parser / eval error or warning). */
export interface Problem {
  id:       string;
  severity: 'error' | 'warning';
  file:     string;
  line:     number;
  col:      number;
  message:  string;
}
