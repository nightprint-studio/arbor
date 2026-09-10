/**
 * The wire item → CodeMirror completion conversion.
 *
 * What is worth pinning here is not the mapping table but the two decisions that were WRONG in
 * the copy this replaced: that `insert_text` and the snippet stops must survive the conversion
 * (they were dropped on the Java path, so the backend could send them and nothing happened), and
 * that `label` is a display string which must never be what lands in the buffer.
 */

import { describe, expect, it } from 'vitest';
import { kindToType, simpleOwner, toCompletion } from './completion-item';
import type { CompletionItem } from '$lib/types/bennu';

const item = (over: Partial<CompletionItem> = {}): CompletionItem => ({
  label: 'size',
  kind: 'method',
  ...over,
});

describe('simpleOwner', () => {
  it('is what the popup has room to show', () => {
    expect(simpleOwner('org/springframework/web/cors/CorsConfiguration')).toBe('CorsConfiguration');
  });

  /// A nested type is `Outer$Inner` in a binary name, and `Outer` is not what declares the member.
  it('takes the innermost name of a nested type', () => {
    expect(simpleOwner('com/x/Outer$Inner')).toBe('Inner');
  });

  it('leaves a bare name alone', () => {
    expect(simpleOwner('Foo')).toBe('Foo');
  });
});

describe('kindToType', () => {
  it('maps both vocabularies onto one icon set', () => {
    // The native engine's spelling and a language server's, for the same thing.
    expect(kindToType('method')).toBe(kindToType('function'));
    expect(kindToType('class')).toBe(kindToType('struct'));
    expect(kindToType('constant')).toBe(kindToType('enum-member'));
  });

  /// The two the native Java engine needs that no server sends. `annotation` earns its own glyph:
  /// after an `@` the whole list is annotations, and the glyph is what says the filter took.
  it('keeps annotation distinct from class', () => {
    expect(kindToType('annotation')).toBe('annotation');
    expect(kindToType('class')).not.toBe('annotation');
  });

  it('falls back rather than inventing a kind', () => {
    expect(kindToType('something-new')).toBe('text');
  });
});

describe('toCompletion', () => {
  it('carries the origin and the deprecated mark across', () => {
    const c = toCompletion(item({ owner: 'java/util/List', deprecated: true }), 10);
    expect(c.origin).toBe('List');
    expect(c.isDeprecated).toBe(true);
  });

  /// A candidate with no owner leaves the row's width to the ones that have one.
  it('leaves the origin unset when the item names no owner', () => {
    expect(toCompletion(item(), 10).origin).toBeUndefined();
  });

  /// The decision the previous Java-side copy got wrong: an item whose inserted text differs from
  /// its label MUST take over the insertion, or accepting `size` writes `size` and not `size()`.
  it('takes over the insertion when the text differs from the label', () => {
    expect(toCompletion(item({ insert_text: 'size()' }), 10).apply).toBeTypeOf('function');
  });

  it('takes over the insertion when there are tab stops', () => {
    const c = toCompletion(item({ insert_text: 'put()', snippet_stops: [{ start: 4, end: 4 }] }), 10);
    expect(c.apply).toBeTypeOf('function');
  });

  /// …and does not, when there is nothing to do beyond inserting the label. A custom `apply` that
  /// only reproduces the default is a second implementation of it.
  it('leaves a plain item to CodeMirror', () => {
    expect(toCompletion(item({ kind: 'field', label: 'name' }), 10).apply).toBeUndefined();
  });

  /// The panel is built for the row the user highlights, not for the four hundred in the list.
  it('offers documentation lazily and only when a builder was given', () => {
    expect(toCompletion(item(), 10).info).toBeUndefined();
    expect(toCompletion(item(), 10, { info: () => null }).info).toBeTypeOf('function');
  });
});

describe('the popup preview', () => {
  /// The popup opens directly under the caret, so a four-line preview drawn there is a four-line
  /// preview with a list on top of it.
  it('collapses a member to the line that carries the information', () => {
    const c = toCompletion(
      item({
        kind: 'generate',
        label: 'getCustomer',
        insert_text: 'public String getCustomer() {\n    return customer;\n}',
      }),
      10,
    );
    expect(c.ghost).toBe('public String getCustomer() { … }');
  });

  /// An ordinary completion inserts what its label says; a preview would be the same word twice.
  it('is absent for a row that inserts its own name', () => {
    expect(toCompletion(item({ insert_text: 'size()' }), 10).ghost).toBeUndefined();
  });
});
