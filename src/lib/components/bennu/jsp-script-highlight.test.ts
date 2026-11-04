import { describe, it, expect } from 'vitest';

import { jspScriptSpans, maskScriptBodies } from './jsp-script-highlight';

/** The class of the span that colours exactly `needle` (first occurrence), innermost wins. */
function classOf(src: string, needle: string): string | undefined {
  const from = src.indexOf(needle);
  expect(from, `"${needle}" not in source`).toBeGreaterThanOrEqual(0);
  const covering = jspScriptSpans(src, () => undefined)
    .filter((s) => s.from <= from && s.to >= from + needle.length);
  return covering.at(-1)?.cls;
}

const BODY = `
  <c:if test="\${noteVisibili}">
    $(this).attr('title', '<wp:i18n key="LABEL_NOTA" />'); // <b>x</b>
    <c:if test="\${skin == 'highcontrast' || skin == 'text'}">$(this).text('(a)');</c:if>
  </c:if>
`;

describe('maskScriptBodies', () => {
  it('masks closers inside a script body and keeps the length', () => {
    const page = `<div></div><script>${BODY}</script><p></p>`;
    const masked = maskScriptBodies(page);
    expect(masked.length).toBe(page.length);
    expect(masked).toContain('< c:if>');
    expect(masked).toContain('</script>');
    expect(masked.startsWith('<div></div>')).toBe(true);
    expect(masked.endsWith('<p></p>')).toBe(true);
  });

  it('leaves a page without a script untouched, and skips a script named in a comment', () => {
    expect(maskScriptBodies('<a></a>')).toBe('<a></a>');
    const commented = '<%-- <script> --%><div></div>';
    expect(maskScriptBodies(commented)).toBe(commented);
  });
});

describe('jspScriptSpans', () => {
  it('colours JS as JS and JSP tags / EL as JSP', () => {
    expect(classOf(BODY, 'c:if')).toBe('keyword');
    expect(classOf(BODY, 'wp:i18n')).toBe('keyword');
    expect(classOf(BODY, 'noteVisibili')).toBe('ident');
    expect(classOf(BODY, 'test')).toBe('field');
    expect(classOf(BODY, 'highcontrast')).toBe('string');
    expect(classOf(BODY, 'title')).toBe('string');
  });

  it('keeps the JavaScript comment a comment', () => {
    expect(classOf(BODY, '// <b>x</b>')).toBe('comment');
  });

  it('returns spans in ascending order', () => {
    const spans = jspScriptSpans(BODY, () => undefined);
    for (let i = 1; i < spans.length; i++) expect(spans[i].from).toBeGreaterThanOrEqual(spans[i - 1].from);
  });

  it('uses the declared library hue for a tag', () => {
    const spans = jspScriptSpans('<c:if test="x">1</c:if>', (p) => (p === 'c' ? 3 : undefined));
    expect(spans.some((s) => s.cls === 'ns-3')).toBe(true);
  });
});
