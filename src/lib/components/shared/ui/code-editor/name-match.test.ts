/**
 * The browser half of the name-matching rule.
 *
 * These cases are the same ones `bennu-complete`'s `name_match` pins in Rust, on purpose: the two
 * filter the same list — the engine when it builds it, this side on every keystroke afterwards —
 * and a rule that has drifted shows up as a popup that quietly loses candidates as you type.
 */

import { describe, expect, it } from 'vitest';
import { matchTier, matchesName } from './name-match';

describe('matchTier', () => {
  it('ranks an exact prefix above one in the wrong case above the humps', () => {
    expect(matchTier('toLowerCase', 'toLo')).toBe(0);
    expect(matchTier('toLowerCase', 'toLO')).toBe(1);
    expect(matchTier('toLowerCase', 'tolc')).toBe(2);
  });

  /// The two the old `startsWith` filter had no answer for.
  it('reaches the humps of a member name', () => {
    expect(matchesName('addAllowedHeader', 'aah')).toBe(true);
    expect(matchesName('addAllElements', 'aAE')).toBe(true);
  });

  /// A constant's words are separated by `_`, and reaching for one is the same gesture.
  it('treats an underscore as a word boundary', () => {
    expect(matchesName('MAX_VALUE', 'MAXV')).toBe(true);
    expect(matchesName('SOME_LONG_NAME', 'SLN')).toBe(true);
  });

  it('refuses a name that does not answer', () => {
    expect(matchTier('toLowerCase', 'toX')).toBeNull();
    expect(matchTier('size', 'sizeOfEverything')).toBeNull();
  });

  /// `receiver.` — everything is admissible.
  it('admits everything on an empty prefix', () => {
    expect(matchTier('anything', '')).toBe(0);
  });

  /// The default's whole point: a capital means a type in Java.
  it('separates a type from a variable by the first letter', () => {
    expect(matchTier('order', 'Order')).toBeNull();
    expect(matchTier('Order', 'order')).toBeNull();
    expect(matchTier('SpringApplication', 'spring', 'ignore')).toBe(1);
  });

  /// Strict means strict, humps included — and that is what the setting is FOR, rather than
  /// switching the humps off.
  it('honours case on every letter under "all"', () => {
    expect(matchTier('addAllElements', 'aAE', 'all')).toBe(2);
    expect(matchTier('addAllElements', 'aae', 'all')).toBeNull();
    expect(matchTier('toLowerCase', 'toLo', 'all')).toBe(0);
    expect(matchTier('toLowerCase', 'tolo', 'all')).toBeNull();
  });
});
