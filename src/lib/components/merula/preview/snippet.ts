/**
 * Compose a tiny `.merula` snippet for an instrument preview.
 *
 * The Preview panel turns its controls (the pressed note or scale degree, the
 * knobs, the articulation, the scale/root, and a free-form chain) into one
 * expression like `n(c4).inst("synth.bass").gain(0.8).room(0.2)`, which the
 * backend evaluates with the real language and plays on the audition bus. Keeping
 * this here (not in the component) means the "how a preview becomes code" rule
 * lives in one testable place — and every new language feature is reachable from
 * the free chain without touching the IPC.
 */

import type { MerulaInstrument } from '$lib/ipc/merula/merula';

/** Per-preview control values (the panel's knobs + selectors). */
export interface PreviewControls {
  gain: number;
  vel: number;
  room: number;
  speed: number;
  pan: number;
  /** Articulation (multisample), '' = the instrument default. */
  art: string;
  /** Scale mode (`minor`, `dorian`, …); '' = chromatic (note names, no scale). */
  scale: string;
  /** Scale root note name (`c`, `cs`, …) when `scale` is set. */
  root: string;
  /** Free-form DSL tail appended verbatim, e.g. `.lpf(800).crush(4)`. */
  chain: string;
}

const NOTE_NAMES = ['c', 'cs', 'd', 'ds', 'e', 'f', 'fs', 'g', 'gs', 'a', 'as', 'b'];

/** MIDI semitone → merula note name (sharps: `cs`/`ds`/`fs`/`gs`/`as`). `C4 = 60`. */
export function midiToName(midi: number): string {
  const pc = ((midi % 12) + 12) % 12;
  return `${NOTE_NAMES[pc]}${Math.floor(midi / 12) - 1}`;
}

/** Compact number for the snippet (≤3 decimals, no trailing zeros). */
function fmt(n: number): string {
  return String(Math.round(n * 1000) / 1000);
}

/** A method call only when `v` differs from the language's default for it. */
function maybe(method: string, v: number, dflt: number): string {
  return Math.abs(v - dflt) < 1e-4 ? '' : `.${method}(${fmt(v)})`;
}

/** Build the snippet for one trigger. For a pitched voice pass `note` (chromatic
 *  MIDI) or `degree` (scale step); a one-shot ignores both and plays native. */
export function buildSnippet(
  inst: MerulaInstrument,
  c: PreviewControls,
  trigger: { note?: number | null; degree?: number | null },
): string {
  const name = JSON.stringify(inst.name); // safe-quoted

  let head: string;
  if (inst.kind === 'sample') {
    // Unpitched one-shot: a sound-bank leaf, native pitch.
    head = `s(${name})`;
  } else if (c.scale && trigger.degree != null) {
    // Scale mode: a numeric degree resolved by `.scale("root:mode")`.
    head = `n(${trigger.degree}).scale(${JSON.stringify(`${c.root}:${c.scale}`)}).inst(${name})`;
  } else {
    // Chromatic: a note name.
    head = `n(${midiToName(trigger.note ?? 60)}).inst(${name})`;
  }

  const art = c.art ? `.art(${JSON.stringify(c.art)})` : '';
  const knobs =
    maybe('gain', c.gain, 1) +
    maybe('vel', c.vel, 0.8) +
    maybe('room', c.room, 0) +
    maybe('speed', c.speed, 1) +
    maybe('pan', c.pan, 0.5);
  const tail = c.chain.trim();

  return `${head}${art}${knobs}${tail}`;
}
