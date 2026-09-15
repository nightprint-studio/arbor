/**
 * The completion trigger's two decisions, pinned on the shapes that broke: a member prefix already
 * typed after the dot (keywords were appended there), and a method reference's `::` (which did not
 * open the popup at all).
 */

import { describe, expect, it } from 'vitest';
import {
  completionSite, offersFallbackWords, shouldAskForCompletion, wordBefore,
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
