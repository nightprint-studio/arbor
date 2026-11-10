import { describe, expect, it } from 'vitest';
import { tokenizeAngular, type NgClass } from './angular-template-tokens';

/** `[class, text]` for every span, so a test reads like the colouring it expects. */
function read(src: string): Array<[NgClass, string]> {
  return tokenizeAngular(src).map((s) => [s.cls, src.slice(s.from, s.to)]);
}
const has = (src: string, cls: NgClass, text: string) =>
  expect(read(src)).toContainEqual([cls, text]);

describe('control flow', () => {
  const sample = `<!-- Tabella Rettifiche gara -->
@if (rettificheGaraOfferta?.length > 0 && dettProcedura.iterGara != '2' && dettProcedura.iterGara != '4' ) {
<div class="col-6">
  <h3>{{ 'LABEL_DETTAGLIO' |
    translate }}</h3>
</div>
}`;

  it('colours the block keyword and its brace', () => {
    has(sample, 'kw', '@if');
    expect(read(sample).filter(([c, t]) => c === 'punct' && t === '{')).toHaveLength(1);
    expect(read(sample).filter(([c, t]) => c === 'punct' && t === '}')).toHaveLength(1);
  });

  it('colours the whole condition, not up to the first `)`', () => {
    has(sample, 'var', 'rettificheGaraOfferta');
    has(sample, 'op', '?.');
    has(sample, 'prop', 'length');
    has(sample, 'num', '0');
    has(sample, 'op', '&&');
    has(sample, 'op', '!=');
    has(sample, 'str', "'4'");
    has(sample, 'prop', 'iterGara');
  });

  it('reads a pipe whose name wraps onto the next line', () => {
    has(sample, 'str', "'LABEL_DETTAGLIO'");
    has(sample, 'op', '|');
    has(sample, 'pipe', 'translate');
  });

  it('does not read a comment as markup', () => {
    expect(read('<!-- @if (x) { {{ y }} -->')).toEqual([]);
  });

  it('handles else if, else, for, switch, case, default', () => {
    const src = '@if (a) {\n} @else if (b) {\n} @else {\n}';
    expect(read(src).filter(([c]) => c === 'kw').map(([, t]) => t)).toEqual(['@if', '@else if', '@else']);
    const sw = '@switch (kind) {\n @case (1) {x}\n @default {y}\n}';
    expect(read(sw).filter(([c]) => c === 'kw').map(([, t]) => t)).toEqual(['@switch', '@case', '@default']);
  });

  it('colours @for with of / track / let', () => {
    const src = '@for (item of items; track item.id; let i = $index) {\n} @empty {\n}';
    has(src, 'kw', '@for');
    has(src, 'kw', 'of');
    has(src, 'kw', 'track');
    has(src, 'kw', 'let');
    has(src, 'kw', '@empty');
    has(src, 'prop', 'id');
  });

  it('colours @defer triggers and @let', () => {
    has('@defer (on viewport; prefetch on idle) {\n}', 'kw', 'viewport');
    const src = '@let total = price * qty;';
    has(src, 'kw', '@let');
    has(src, 'var', 'total');
    has(src, 'op', '*');
    has(src, 'punct', ';');
  });

  it('leaves text that only looks like a block alone', () => {
    expect(read('write to me@if.com')).toEqual([]);
    expect(read('<p>@media (min-width: 1px) and @if</p>')).toEqual([]);
    expect(read('@if (broken')).toEqual([]);
    expect(read('@if (x) no brace')).toEqual([]);
  });

  it('closing braces are only coloured inside a block', () => {
    expect(read('<p>} stray</p>')).toEqual([]);
  });
});

describe('interpolation', () => {
  it('colours braces, expression and pipe arguments', () => {
    const src = `{{ user?.name | slice:0:10 | uppercase }}`;
    has(src, 'punct', '{{');
    has(src, 'punct', '}}');
    has(src, 'prop', 'name');
    has(src, 'pipe', 'slice');
    has(src, 'pipe', 'uppercase');
    has(src, 'num', '10');
  });

  it('keeps `||` apart from a pipe', () => {
    const src = '{{ a || b | p }}';
    has(src, 'op', '||');
    has(src, 'pipe', 'p');
    expect(read(src).filter(([c]) => c === 'pipe')).toHaveLength(1);
  });

  it('is not fooled by `}}` inside a string', () => {
    has(`{{ '}}' | p }}`, 'pipe', 'p');
  });

  it('paints nothing for an unterminated interpolation', () => {
    expect(read('<p>{{ a | </p><p>rest</p>')).toEqual([]);
  });

  it('works inside a plain attribute value', () => {
    has('<img title="{{ t | translate }}">', 'pipe', 'translate');
  });

  it('colours calls and members', () => {
    const src = '{{ svc.format(item.price, "EUR")!.trim() }}';
    has(src, 'var', 'svc');
    has(src, 'call', 'format');
    has(src, 'call', 'trim');
    has(src, 'str', '"EUR"');
  });
});

describe('binding attributes', () => {
  it('reads property, event and two-way bindings', () => {
    const src = '<input [value]="name ?? \'x\'" (input)="onInput($event); dirty = true" [(ngModel)]="model.text">';
    has(src, 'bind', 'value');
    has(src, 'var', 'name');
    has(src, 'event', 'input');
    has(src, 'call', 'onInput');
    has(src, 'var', '$event');
    has(src, 'lit', 'true');
    has(src, 'bind', 'ngModel');
    has(src, 'prop', 'text');
  });

  it('reads structural directives and refs', () => {
    const src = '<li *ngFor="let h of heroes; trackBy: byId; let i = index" #row>';
    has(src, 'dir', '*ngFor');
    has(src, 'kw', 'let');
    has(src, 'kw', 'of');
    has(src, 'var', 'heroes');
    has(src, 'ref', '#row');
    has('<div *ngIf="ok; else other">', 'var', 'ok');
  });

  it('reads class/style/attr sub-bindings', () => {
    has('<div [class.active]="on" [style.width.px]="w">', 'bind', 'class.active');
    has('<div [style.width.px]="w">', 'bind', 'style.width.px');
  });

  it('ignores ordinary attributes and anchors', () => {
    expect(read('<a href="#top" class="a b" data-x="1">t</a>')).toEqual([]);
    expect(read('<div [broken)="x">')).toEqual([]);
  });

  it('does not run on past an unterminated attribute value', () => {
    expect(read('<a [x]="y\n@if (a) {\n}')).toEqual([]);
  });

  it('skips script and style bodies', () => {
    expect(read('<script>if (a) { b } @if (c) {}</script>')).toEqual([]);
  });
});

describe('plain html', () => {
  it('produces nothing for markup with no Angular syntax', () => {
    const html = '<!doctype html><html><body><h1 id="t">Hi</h1><p class="x">a &amp; b</p></body></html>';
    expect(read(html)).toEqual([]);
  });
});
