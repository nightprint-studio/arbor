/**
 * The completion trigger's two decisions, pinned on the shapes that broke: a member prefix already
 * typed after the dot (keywords were appended there), and a method reference's `::` (which did not
 * open the popup at all).
 */

import { describe, expect, it } from 'vitest';
import {
  completionSite, insideJavaString, offersFallbackWords, shouldAskForCompletion, wordBefore,
} from './java-completion-trigger';

describe('wordBefore', () => {
  it('is the identifier at the end, snake_case and `$` included', () => {
    expect(wordBefore('identity_resolver.resolve_identity().ma')).toBe('ma');
    expect(wordBefore('x = $tmp_1')).toBe('$tmp_1');
    expect(wordBefore('map(')).toBe('');
  });
});

describe('completionSite', () => {
  it('reads a member position with a prefix already typed after the dot', () => {
    expect(completionSite('identity_resolver.resolve_identity().ma')).toBe('member');
    expect(completionSite('identity_resolver.resolve_identity().')).toBe('member');
  });

  it('reads the member half of a method reference as a member position', () => {
    expect(completionSite('map(ResolvedIdentity::')).toBe('member');
    expect(completionSite('map(ResolvedIdentity::ide')).toBe('member');
    expect(completionSite('map(ResolvedIdentity :: ide')).toBe('member');
  });

  it('reads an annotation and a bare word', () => {
    expect(completionSite('    @Req')).toBe('annotation');
    expect(completionSite('map(Re')).toBe('bare');
    expect(completionSite('cond ? a : b')).toBe('bare');
  });
});

describe('offersFallbackWords', () => {
  it('adds no keywords or buffer words once a member name is being typed', () => {
    expect(offersFallbackWords('identity_resolver.resolve_identity().ma')).toBe(false);
    expect(offersFallbackWords('map(ResolvedIdentity::re')).toBe(false);
  });

  it('still adds them to a bare word', () => {
    expect(offersFallbackWords('    ret')).toBe(true);
    expect(offersFallbackWords('map(Re')).toBe(true);
  });
});

describe('shouldAskForCompletion', () => {
  it('asks right after `::`, as it does after `.` and `@`', () => {
    expect(shouldAskForCompletion('map(ResolvedIdentity::', false)).toBe(true);
    expect(shouldAskForCompletion('resolve_identity().', false)).toBe(true);
    expect(shouldAskForCompletion('    @', false)).toBe(true);
  });

  it('asks while a word is typed, and always when asked explicitly', () => {
    expect(shouldAskForCompletion('map(Re', false)).toBe(true);
    expect(shouldAskForCompletion('map(', true)).toBe(true);
  });

  it('does not open unasked on a bare opening paren, a single colon or whitespace', () => {
    expect(shouldAskForCompletion('map(', false)).toBe(false);
    expect(shouldAskForCompletion('cond ? a :', false)).toBe(false);
    expect(shouldAskForCompletion('    ', false)).toBe(false);
  });
});

describe('inside an annotation', () => {
  it('asks at the opening quote of a string argument and after `${`, not at every parenthesis', () => {
    expect(shouldAskForCompletion('  @GetMapping("', false)).toBe(true);
    expect(shouldAskForCompletion('  @PostMapping(produces = "', false)).toBe(true);
    expect(shouldAskForCompletion('  @Value("${', false)).toBe(true);
    expect(shouldAskForCompletion('  @GetMapping(', false)).toBe(false);
    expect(shouldAskForCompletion('  call("', false)).toBe(false);
  });

  it('adds no words from the buffer inside the string of an annotation, and keeps them elsewhere', () => {
    expect(offersFallbackWords('  @GetMapping("/or')).toBe(false);
    expect(offersFallbackWords('  @Value("${spring.da')).toBe(false);
    expect(offersFallbackWords('  @GetMapping("/a") void f() { fo')).toBe(true);
    expect(offersFallbackWords('  String s = "ab')).toBe(true);
  });
});

describe('insideJavaString', () => {
  it('is true inside a plain literal and false once it is closed', () => {
    expect(insideJavaString('String q = "select o from')).toBe(true);
    expect(insideJavaString('String q = "select"; int a')).toBe(false);
    expect(insideJavaString('x = "a\\"b')).toBe(true); // an escaped quote does not close it
  });

  it('follows a text block across lines — the shape that never reached the JPA extension', () => {
    expect(insideJavaString('@Query("""\n  select o\n  from Order o where o.')).toBe(true);
    expect(insideJavaString('@Query("""\n  select o\n  """) Object m(); int a')).toBe(false);
  });

  it('ignores quotes in comments and character literals', () => {
    expect(insideJavaString('// he said "hi\nint a')).toBe(false);
    expect(insideJavaString('/* " */ int a')).toBe(false);
    expect(insideJavaString("char q = '\"'; int a")).toBe(false);
  });

  it('ends an unterminated plain literal at its line, as the compiler would', () => {
    expect(insideJavaString('String a = "oops\nint b')).toBe(false);
  });
});

describe('shouldAskForCompletion — bind parameters', () => {
  it('asks after a colon only inside a string', () => {
    expect(shouldAskForCompletion('where o.total > :', false, () => true)).toBe(true);
    expect(shouldAskForCompletion('x = cond ? a :', false, () => false)).toBe(false);
    expect(shouldAskForCompletion('x = cond ? a :', false)).toBe(false);
  });
});
