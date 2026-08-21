/**
 * The **glyph** per symbol kind — a shape, for a list whose rows are of *different* kinds.
 *
 * One table because a symbol's shape must not depend on which panel you are looking at: the Structure
 * outline, the File Structure popup and the call/type hierarchy all name the same things, and a
 * `struct` that is a box in one and a bracket in another teaches that they are different kinds.
 *
 * The sibling device is `SymbolKindIcon.svelte`, the lettered **ring**, and the two answer different
 * questions. A shape answers *type versus function versus field*, which is what a mixed list needs. A
 * letter answers *which kind of type*, which is what a list where every row is a type needs — there a
 * shape carries no information at all. Neither is a fallback for the other.
 *
 * The Java scanner's kinds and a language server's are mapped onto the **same** icons on purpose. A
 * Rust `struct` and a Java `class` are the same thing to a reader scanning a list, and giving them
 * different shapes would suggest they are not. What differs is the vocabulary in the label, which is
 * whatever the engine called it.
 *
 * An unknown kind gets the neutral glyph rather than nothing — a server is free to invent kinds, and a
 * row with no icon reads as a broken row rather than as an unfamiliar one.
 */

import { Braces, Code2, FileCode2, SquareFunction, Variable } from 'lucide-svelte';
import SymbolKindIconRaw from './SymbolKindIcon.svelte';
import type { IconComponent } from '$lib/types/icon';

/**
 * What a row draws for a kind: a component, the colour to tint it, and any props the component
 * needs beyond a size.
 *
 * `props` exists for one entry and it is the important one — see {@link RING} below.
 */
export interface KindGlyph {
  icon: IconComponent;
  color: string;
  /** Extra props for `icon`. Spread by the consumer after `size`. */
  props?: Record<string, unknown>;
}

/** The Svelte-5 component as an icon slot — the same cast every icon map in the app uses. */
const SymbolKindIcon = SymbolKindIconRaw as unknown as IconComponent;

/**
 * A **type**, drawn as the lettered ring — `C` class, `S` struct, `T` trait, `E` enum.
 *
 * These used to be a cube, and a cube is a placeholder: it is what an icon set offers when it has
 * nothing to say, it says the same nothing for a class as for a struct as for a trait, and three
 * identical cubes down an outline are three rows a reader has to read the *text* of to tell apart
 * — which is the job the icon was there to do.
 *
 * The ring is not a new device: `SymbolKindIcon.svelte` already draws it for the Go-to *Types*
 * tab, where every row is a type. Reaching for it here too is a narrowing of that component's
 * stated scope, and worth it: the reason a mixed list wanted a *shape* was that a shape separates
 * a type from a function, and a ring does that at least as well while also saying **which** type.
 * A function is still a function glyph and a field still a field one, so the coarse distinction
 * survives intact.
 *
 * The colour comes from the ring's own table (it hue-codes the role), so these entries carry a
 * colour only for the consumers that tint a container around the icon.
 */
const RING = (color: string, kind: string): KindGlyph => ({
  icon: SymbolKindIcon,
  color,
  props: { kind },
});

const GLYPHS: Record<string, KindGlyph> = {
  // ── Bennu's own Java scanner ──
  class:     RING('var(--info)', 'class'),
  interface: RING('var(--success)', 'interface'),
  enum:      RING('var(--warning)', 'enum'),
  record:    RING('var(--color-tag, #c792ea)', 'record'),
  annotation: RING('var(--accent)', 'annotation'),
  method:    { icon: SquareFunction, color: 'var(--color-tag, #c792ea)' },
  field:     { icon: Variable,       color: 'var(--success)' },
  group:     { icon: Braces,         color: 'var(--text-muted)' },
  element:   { icon: Code2,          color: 'var(--color-tag, #c792ea)' },
  // ── from a language server ──
  struct:    RING('var(--info)', 'struct'),
  object:    RING('var(--info)', 'object'),
  // A Rust `trait` is an interface, and reads as one — but it is called a trait, and the ring is
  // exactly the device that can say both at once: the interface green, and a `T`.
  trait:     RING('var(--success)', 'trait'),
  // An `impl` block is not a type — it is a container of members, which is what a module is too.
  impl:      { icon: Braces,         color: 'var(--color-tag, #c792ea)' },
  'type alias': RING('var(--text-secondary)', 'type alias'),
  namespace: { icon: Braces,         color: 'var(--text-muted)' },
  module:    { icon: Braces,         color: 'var(--text-muted)' },
  function:  { icon: SquareFunction, color: 'var(--color-tag, #c792ea)' },
  constructor: { icon: SquareFunction, color: 'var(--color-tag, #c792ea)' },
  property:  { icon: Variable,       color: 'var(--success)' },
  variable:  { icon: Variable,       color: 'var(--success)' },
  constant:  { icon: Variable,       color: 'var(--warning)' },
  enummember: { icon: Variable,      color: 'var(--warning)' },
  'enum member': { icon: Variable,   color: 'var(--warning)' },
  typeparameter: { icon: Code2,      color: 'var(--text-muted)' },
  'type parameter': { icon: Code2,   color: 'var(--text-muted)' },
};

const UNKNOWN: KindGlyph = { icon: FileCode2, color: 'var(--text-muted)' };

/** The glyph + colour for a kind name, case-insensitively. */
export function kindGlyph(kind: string): KindGlyph {
  return GLYPHS[kind.toLowerCase()] ?? UNKNOWN;
}
