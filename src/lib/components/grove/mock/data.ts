/**
 * The single fake project behind the Step 0 GroveShell. Everything the shell
 * shows — files, outline, tracks, piano-roll notes, voices, console logs,
 * problems — is sourced from here. When the engine lands, these become live
 * IPC streams; the UI stays the same.
 */

import type {
  GroveProject, Track, Voice, LogLine, Problem, OutlineEntry, GroveFile, Section,
} from './types';

// ── Arrangement timeline ──────────────────────────────────────────────────────
// The song spans 32 cycles: INTRO(4) · BUILD(8) · FULL(16) · OUTRO(4) — the same
// section constants used in song.grove (INTRO/BUILD/FULL/OUTRO).
export const TIMELINE_CYCLES = 32;

export const MOCK_SECTIONS: Section[] = [
  { label: 'INTRO', start: 0,  len: 4 },
  { label: 'BUILD', start: 4,  len: 8 },
  { label: 'FULL',  start: 12, len: 16 },
  { label: 'OUTRO', start: 28, len: 4 },
];

// ── Source files ────────────────────────────────────────────────────────────

const SONG_SOURCE = `import { drumGroove } from "lib/drums.grove"

cps(0.5)                                  // cicles-per-second

let INTRO = 4   let BUILD = 8   let FULL = 16   let OUTRO = 4

// parametric fn · letter+octave notes · @n weight
fn bassline(root) = n($root@3 g1).inst("synth.bass").lpf(800).slow(2)

let motif = n(ef5 g5 ef5 c5)              // reused via $ splice
let perc  = s(oh:2 ~ oh:2 ~ & rim!4 & cym/4).gain(0.4)

tracks(
  // BASS — plays everywhere
  track("bass", bassline(c2)),

  // PAD — chords ' · <> per-cycle alternation · .room
  track("pad", n(<c3'min7 af2'maj7 bf2'7 g2'min7>).slow(2).inst("synth.pad").room(0.6).gain(0.5)),

  // DRUMS — & parallel · (n,k) euclid · active in build+full
  track("drums", arrange(
    cycles(INTRO, ~),
    cycles(BUILD + FULL, par(s(bd ~ sd ~ & hh*8 & cp(3,8)), perc, drumGroove)),
    cycles(OUTRO, ~),
  )),

  // ARP — degrees + .scale · [] group · every(rev) · off echo · full only
  track("arp", arrange(
    cycles(INTRO + BUILD, ~),
    cycles(FULL, n([0 2 4 7]*2).scale("c:minor").inst("synth.pluck").every(4, rev).off(0.125, gain(0.4))),
    cycles(OUTRO, ~),
  )),

  // LEAD — $ interp · _ hold · VSCO voice · sometimes · .log(trace)
  track("lead", arrange(
    cycles(INTRO + BUILD, ~),
    cycles(FULL, n(c5 $motif g4 _).inst("strings.violin").sometimes(degrade).log(trace)),
    cycles(OUTRO, ~),
  )),
)
`;

const DRUMS_SOURCE = `// lib/drums.grove — drum utilities, imported by song.grove.
// A library file: its tracks(…) output is ignored, only fn/let are exported.

let kick  = s(bd ~ bd ~)
let snare = s(~ sd ~ sd)

// A busy hat groove with a euclidean ghost-snare layer.
let drumGroove = s(hh*8 & sd(3,8,2)).gain(0.7)

tracks(
  track("preview", par(kick, snare, drumGroove)),
)
`;

const AMBIENT_SOURCE = `cps(0.35)

// A slow, evolving pad sketch — long-form, "slightly sequential".
fn swell(root) = n($root'maj9).slow(4).inst("synth.pad").room(0.8).gain(rand(0.3, 0.6))

tracks(
  track("drone", n(c2).slow(8).inst("synth.bass").lpf(400).gain(0.4)),
  track("pads", arrange(
    cycles(8, swell(c4)),
    cycles(8, swell(af3)),
    cycles(8, swell(g3)),
  )),
)
`;

const FILES: GroveFile[] = [
  { id: 'f-song',    name: 'song.grove',    path: 'song.grove',       library: false, source: SONG_SOURCE },
  { id: 'f-ambient', name: 'ambient.grove', path: 'sketches/ambient.grove', library: false, source: AMBIENT_SOURCE },
  { id: 'f-drums',   name: 'drums.grove',   path: 'lib/drums.grove',  library: true,  source: DRUMS_SOURCE },
];

export const MOCK_PROJECT: GroveProject = {
  id:       'p-roman-tactics',
  name:     'Roman Tactics',
  audience: 'In-game soundtrack — strategy campaign',
  path:     'C:/games/roman-tactics/music',
  files:    FILES,
};

/** Recent projects for the titlebar fast-swap dropdown (only the first exists). */
export const RECENT_PROJECTS = [
  { id: 'p-roman-tactics', name: 'Roman Tactics', audience: 'Strategy campaign OST' },
  { id: 'p-neon-drift',    name: 'Neon Drift',    audience: 'Racing menu loops' },
  { id: 'p-study-beats',   name: 'Study Beats',   audience: 'Lo-fi focus playlist' },
];

// ── Outline (for song.grove) ──────────────────────────────────────────────────

export const MOCK_OUTLINE: OutlineEntry[] = [
  { id: 'o-import',   kind: 'import', label: 'drumGroove',     line: 1 },
  { id: 'o-intro',    kind: 'let',    label: 'INTRO',          line: 5 },
  { id: 'o-build',    kind: 'let',    label: 'BUILD',          line: 5 },
  { id: 'o-full',     kind: 'let',    label: 'FULL',           line: 5 },
  { id: 'o-outro',    kind: 'let',    label: 'OUTRO',          line: 5 },
  { id: 'o-bassline', kind: 'fn',     label: 'bassline(root)', line: 8 },
  { id: 'o-motif',    kind: 'let',    label: 'motif',          line: 10 },
  { id: 'o-perc',     kind: 'let',    label: 'perc',           line: 11 },
  { id: 'o-t-bass',   kind: 'track',  label: 'bass',           line: 15 },
  { id: 'o-t-pad',    kind: 'track',  label: 'pad',            line: 18 },
  { id: 'o-t-drums',  kind: 'track',  label: 'drums',          line: 21 },
  { id: 'o-t-arp',    kind: 'track',  label: 'arp',            line: 28 },
  { id: 'o-t-lead',   kind: 'track',  label: 'lead',           line: 35 },
];

// ── Sound bank (registry voices) ──────────────────────────────────────────────

export const MOCK_VOICES: Voice[] = [
  { id: 'v-bass',    name: 'synth.bass',     kind: 'synth',   installed: true },
  { id: 'v-pad',     name: 'synth.pad',      kind: 'synth',   installed: true },
  { id: 'v-pluck',   name: 'synth.pluck',    kind: 'synth',   installed: true },
  { id: 'v-lead',    name: 'synth.lead',     kind: 'synth',   installed: true },
  { id: 'v-violin',  name: 'strings.violin', kind: 'sampler', installed: true },
  { id: 'v-cello',   name: 'strings.cello',  kind: 'sampler', installed: false },
  { id: 'v-horn',    name: 'brass.horn',     kind: 'sampler', installed: false },
  { id: 'v-flute',   name: 'winds.flute',    kind: 'sampler', installed: false },
];

// ── Tracks (+ piano-roll notes + meters) ──────────────────────────────────────

/** Tiny helper: a row of evenly-spaced notes across `count` slots. */
function steps(rows: number, row: number, count: number, active?: number) {
  const notes = [];
  for (let i = 0; i < count; i++) {
    notes.push({ start: i / count, len: (1 / count) * 0.7, row, active: i === active });
  }
  return notes;
}

export const MOCK_TRACKS: Track[] = [
  {
    id: 't-bass', name: 'bass', colorIdx: 0, voice: 'synth.bass',
    muted: false, soloed: false, gain: 0.85, pan: 0.5, room: 0.1, meter: 0.62,
    rollRows: 6,
    notes: [
      { start: 0.0, len: 0.45, row: 1, active: true },
      { start: 0.5, len: 0.45, row: 0 },
    ],
    regions: [{ start: 0, len: 32, label: 'bassline(c2)', density: 0.4 }],
  },
  {
    id: 't-pad', name: 'pad', colorIdx: 1, voice: 'synth.pad',
    muted: false, soloed: false, gain: 0.5, pan: 0.5, room: 0.6, meter: 0.34,
    rollRows: 8,
    notes: [
      { start: 0, len: 1, row: 2, active: true },
      { start: 0, len: 1, row: 4, active: true },
      { start: 0, len: 1, row: 6, active: true },
    ],
    regions: [{ start: 0, len: 32, label: 'min7 / maj7 pads', density: 0.25 }],
  },
  {
    id: 't-drums', name: 'drums', colorIdx: 2, voice: '(samples)',
    muted: false, soloed: false, gain: 0.9, pan: 0.5, room: 0.05, meter: 0.78,
    rollRows: 4,
    notes: [
      ...steps(4, 0, 4, 0),                       // kick
      { start: 0.25, len: 0.12, row: 1 },         // snare
      { start: 0.75, len: 0.12, row: 1 },
      ...steps(4, 3, 8),                           // hats
    ],
    regions: [{ start: 4, len: 24, label: 'bd sd & hh*8 & cp(3,8)', density: 0.85 }],
  },
  {
    id: 't-arp', name: 'arp', colorIdx: 3, voice: 'synth.pluck',
    muted: true, soloed: false, gain: 0.6, pan: 0.6, room: 0.3, meter: 0.0,
    rollRows: 8,
    notes: [
      { start: 0.0,  len: 0.12, row: 0 },
      { start: 0.25, len: 0.12, row: 2 },
      { start: 0.5,  len: 0.12, row: 4 },
      { start: 0.75, len: 0.12, row: 7 },
    ],
    regions: [{ start: 12, len: 16, label: '[0 2 4 7]*2 .scale', density: 0.6 }],
  },
  {
    id: 't-lead', name: 'lead', colorIdx: 4, voice: 'strings.violin',
    muted: false, soloed: true, gain: 0.7, pan: 0.45, room: 0.4, meter: 0.51,
    rollRows: 10,
    notes: [
      { start: 0.0,  len: 0.2, row: 8, active: true },
      { start: 0.25, len: 0.2, row: 6 },
      { start: 0.5,  len: 0.4, row: 9 },
    ],
    regions: [{ start: 12, len: 16, label: 'c5 $motif g4 _', density: 0.45 }],
  },
];

// ── Console logs (pre-gated, like the real engine) ────────────────────────────

export const MOCK_LOGS: LogLine[] = [
  { id: 1, level: 'info',  cycle: 0,  source: 'engine', text: 'transport started · cps=0.5 · sr=48000 buffer=512' },
  { id: 2, level: 'info',  cycle: 0,  source: 'eval',   text: 'song.grove → 5 tracks · 0 errors' },
  { id: 3, level: 'debug', cycle: 0,  source: 'audio',  text: 'voice registry: 4 synth, 1 sampler resolved' },
  { id: 4, level: 'warn',  cycle: 2,  source: 'audio',  text: 'strings.cello not installed — falling back to synth' },
  { id: 5, level: 'debug', cycle: 4,  source: 'engine', text: 'section change → BUILD (cycle 4)' },
  { id: 6, level: 'trace', cycle: 12, source: 'lead',   text: 'hap c5 @ 0.00 (strings.violin, gain 0.70)' },
  { id: 7, level: 'trace', cycle: 12, source: 'lead',   text: 'hap ef5 @ 0.25 (strings.violin, gain 0.70)' },
  { id: 8, level: 'info',  cycle: 12, source: 'engine', text: 'section change → FULL (cycle 12)' },
  { id: 9, level: 'debug', cycle: 12, source: 'engine', text: 're-eval swap applied at cycle boundary 12' },
  { id: 10, level: 'trace', cycle: 13, source: 'arp',   text: 'degrade dropped 2/8 events (seed=13)' },
];

// ── Problems (diagnostics) ────────────────────────────────────────────────────

export const MOCK_PROBLEMS: Problem[] = [
  { id: 'pb-1', severity: 'warning', file: 'song.grove',    line: 24, col: 41,
    message: 'cp(3,8) euclid: 3 hits over 8 steps — verify intended density' },
  { id: 'pb-2', severity: 'warning', file: 'sketches/ambient.grove', line: 4, col: 38,
    message: "voice 'synth.pad' room=0.8 may clip the reverb bus send" },
];
