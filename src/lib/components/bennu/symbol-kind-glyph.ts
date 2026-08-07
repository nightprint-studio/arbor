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

import { Box, Braces, Code2, FileCode2, SquareFunction, Variable } from 'lucide-svelte';
import type { IconComponent } from '$lib/types/icon';

/** What a row draws for a kind: a lucide component and the colour to tint it. */
export interface KindGlyph {
  icon: IconComponent;
  color: string;
}

const GLYPHS: Record<string, KindGlyph> = {
  // ── Bennu's own Java scanner ──
  class:     { icon: Box,            color: 'var(--info)' },
  interface: { icon: Box,            color: 'var(--info)' },
  enum:      { icon: Box,            color: 'var(--info)' },
  record:    { icon: Box,            color: 'var(--info)' },
  method:    { icon: SquareFunction, color: 'var(--color-tag, #c792ea)' },
  field:     { icon: Variable,       color: 'var(--success)' },
  group:     { icon: Braces,         color: 'var(--text-muted)' },
  element:   { icon: Code2,          color: 'var(--color-tag, #c792ea)' },
  // ── from a language server ──
  struct:    { icon: Box,            color: 'var(--info)' },
  object:    { icon: Box,            color: 'var(--info)' },
  // A Rust `trait` is an interface, and reads as one.
  trait:     { icon: Box,            color: 'var(--info)' },
  // An `impl` block is not a type — it is a container of members, which is what a module is too.
  impl:      { icon: Braces,         color: 'var(--color-tag, #c792ea)' },
  'type alias': { icon: Code2,       color: 'var(--text-secondary)' },
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
