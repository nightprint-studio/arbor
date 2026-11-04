import { describe, expect, it } from 'vitest';
import { urlMarks, variablesIn } from './url-marks';

const classAt = (marks: ReturnType<typeof urlMarks>, text: string, needle: string) => {
  const at = text.indexOf(needle);
  return marks.filter((m) => m.from <= at && m.to >= at + needle.length).map((m) => m.className);
};

describe('urlMarks', () => {
  const text = 'https://stg.example.it/api/bandi?stato=APERTO&page=0';
  const marks = urlMarks(text, new Set());

  it('colours the scheme, the host, the path and the query apart', () => {
    expect(classAt(marks, text, 'https')).toContain('cm-tok-keyword');
    expect(classAt(marks, text, 'stg.example.it')).toContain('cm-tok-type');
    expect(classAt(marks, text, '/api/bandi')).toContain('cm-tok-ident');
    expect(classAt(marks, text, 'stato')).toContain('cm-tok-field');
    expect(classAt(marks, text, 'APERTO')).toContain('cm-tok-string');
    expect(classAt(marks, text, 'page')).toContain('cm-tok-field');
  });

  it('marks a variable as resolved or not', () => {
    const t = '{{baseUrl}}/api?id={{id}}';
    const m = urlMarks(t, new Set(['baseUrl']));
    expect(classAt(m, t, '{{baseUrl}}').join(' ')).toContain('cm-tok-constant');
    expect(classAt(m, t, '{{baseUrl}}').join(' ')).not.toContain('invalid');
    expect(classAt(m, t, '{{id}}').join(' ')).toContain('cm-tok-invalid');
  });

  it('does not colour a leading variable as a host', () => {
    expect(urlMarks('{{baseUrl}}/x', new Set()).some((m) => m.className === 'cm-tok-ident' && m.from === 0 && m.to === 11)).toBe(false);
  });

  it('has nothing to say about an empty field', () => {
    expect(urlMarks('', new Set())).toEqual([]);
  });

  it('lists variable names once', () => {
    expect(variablesIn('{{a}}/{{b}}?x={{a}}')).toEqual(['a', 'b']);
  });
});
