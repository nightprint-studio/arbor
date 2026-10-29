import { describe, expect, it } from 'vitest';
import { inlineCodeKind, renderMarkdown } from './markdown';

describe('renderMarkdown — what an assistant answer needs', () => {
  it('renders a pipe table with its header and alignment', () => {
    const html = renderMarkdown('| Name | Cost |\n|:--|--:|\n| Opus | 4 |\n| Haiku | 0.1 |');
    expect(html).toContain('<table class="md-table">');
    expect(html).toContain('<th>Name</th>');
    expect(html).toContain('<th style="text-align:right">Cost</th>');
    expect(html).toContain('<td style="text-align:right">0.1</td>');
    expect(html.match(/<tr>/g)).toHaveLength(3);
  });

  it('keeps inline markdown inside table cells and ends the table at the first line without a pipe', () => {
    const html = renderMarkdown('| a | b |\n|---|---|\n| **x** | `y` |\n\nafter');
    expect(html).toContain('<td><strong>x</strong></td>');
    expect(html).toContain('<td><code class="md-inline-code md-ic-ident">y</code></td>');
    expect(html.indexOf('</table>')).toBeLessThan(html.indexOf('after'));
  });

  it('does not take a line with a pipe for a table without the rule line', () => {
    expect(renderMarkdown('a | b')).not.toContain('<table');
  });

  it('renders headings up to six levels, the deeper ones styled as the third', () => {
    const html = renderMarkdown('#### Deep');
    expect(html).toContain('<h4 class="md-h3">Deep</h4>');
  });

  it('keeps an indented list item in the list instead of dropping it to plain text', () => {
    const html = renderMarkdown('- one\n  - nested\n- two');
    expect(html.match(/<li>/g)).toHaveLength(3);
    expect(html.match(/<ul/g)).toHaveLength(1);
  });

  it('renders strikethrough', () => {
    expect(renderMarkdown('~~old~~')).toContain('<s>old</s>');
  });

  it('never lets markup in a table cell through unescaped', () => {
    const html = renderMarkdown('| a |\n|---|\n| <img src=x onerror=alert(1)> |');
    expect(html).not.toContain('onerror');
  });
});

describe('inlineCodeKind — what an inline code span names', () => {
  const kind = (s: string) => inlineCodeKind(s);

  it('recognises annotations, with or without arguments', () => {
    expect(kind('@RestController')).toBe('annotation');
    expect(kind('@RequestMapping(BASE_PATH + "mail")')).toBe('annotation');
    expect(kind('@Protection(profile = @Profile(capability = X))')).toBe('annotation');
  });

  it('recognises calls and methods', () => {
    expect(kind('send_registrazione(receiver, username)')).toBe('call');
    expect(kind('mail.send()')).toBe('call');
  });

  it('recognises types, constants and files', () => {
    expect(kind('MailService')).toBe('type');
    expect(kind('List<String>')).toBe('type');
    expect(kind('BASE_PATH')).toBe('const');
    expect(kind('ProfiloEnum.SOLA_LETTURA')).toBe('const');
    expect(kind('MailController.java')).toBe('path');
    expect(kind('src/main/App.rs')).toBe('path');
  });

  it('recognises keywords, literals and plain identifiers', () => {
    expect(kind('boolean')).toBe('keyword');
    expect(kind('true')).toBe('keyword');
    expect(kind('42')).toBe('number');
    expect(kind('receiver')).toBe('ident');
  });

  it('declines anything it cannot tell, rather than guessing a colour', () => {
    expect(kind('a + b * c')).toBe('other');
    expect(kind('')).toBe('other');
    expect(kind('some words here')).toBe('other');
  });

  it('puts the kind on the rendered span and the language on the code block', () => {
    expect(renderMarkdown('use `@Entity` here')).toContain('md-inline-code md-ic-annotation');
    expect(renderMarkdown('```java\nint x;\n```')).toContain('data-lang="java"');
  });
});
