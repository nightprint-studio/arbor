/**
 * Prism grammar for XML Schema (XSD).
 *
 * XSD is plain XML, so we extend the existing `markup` grammar (already
 * registered globally via prism-shared.ts) rather than rolling a new one,
 * and layer three XSD-specific tweaks on top:
 *
 *  1. QName references inside attribute values (`type="xs:string"`,
 *     `base="pg:ContoCorrenteType"`) get a `builtin` token so type
 *     references pop visually instead of melting into the string colour.
 *     In an XSD a colon-prefixed token inside an attribute value is
 *     essentially always a type/element reference — generalising past
 *     `xs:`/`xsd:` covers the very common case of custom target-namespace
 *     prefixes.
 *
 *  2. **Multi-line tag fragments.** Diff/blame/conflict-resolver views call
 *     `highlight()` one line at a time (see diff-formatter.ts). Prism's
 *     stock `tag` pattern requires the entire `<…>` to be present in the
 *     match, so a tag spanning multiple lines (super common in XSD: the
 *     root `<xsd:schema xmlns:xsd="…" xmlns:pg="…" …>` and any element
 *     with a handful of attributes wrapped) leaves each fragment line
 *     uncoloured. We add three fragment patterns covering the three shapes
 *     a continuation line can take:
 *
 *       (a) opening fragment: `<xsd:element name="…" type="…"`  (no `>`)
 *       (b) attribute-only:   `   maxOccurs="1" minOccurs="0">`  (with/without `>`)
 *       (c) closing-only:     `/>`  or  `>`  alone on its line
 *
 *     All three are restricted to a single physical line (`[ \t]` instead
 *     of `\s`, plus `^…$/m`) so they cannot accidentally span lines in the
 *     whole-file paths (RepoBrowser, markdown code blocks) where the stock
 *     `tag` pattern already handles multi-line tags correctly.
 *
 *  3. `<xsd:documentation>` / `<xsd:annotation>` body text rendered through
 *     a whole-file highlighter is matched as a single block and the prose
 *     inside coloured as a comment. This pattern can't match in per-line
 *     views (each call only sees one line) but is a cheap improvement on
 *     whole-file paths.
 *
 * Prism nests `attr-value` inside the `tag` token, not at top level — the
 * path is `markup.tag.inside['attr-value'].inside`. `Prism.languages.extend`
 * deep-clones, so mutating the cloned `xsd` grammar does not leak into the
 * shared markup grammar.
 */

import Prism from 'prismjs';

import 'prismjs/components/prism-markup';

Prism.languages.xsd = Prism.languages.extend('markup', {});

// ── 1. QName "builtin" inside attribute values ─────────────────────────────
const tag = (Prism.languages.xsd as any)?.tag;
const attrValue = tag?.inside?.['attr-value'];
if (attrValue) {
  attrValue.inside = attrValue.inside || {};
  attrValue.inside['builtin'] = /[A-Za-z_][\w.-]*:[A-Za-z_][\w.-]*/;
}

// ── 2. Multi-line tag fragment patterns ────────────────────────────────────
// Shared inside-rules. `attr-value` reuses the same `builtin` highlight we
// gave the stock `tag.inside['attr-value']` so type-ref QNames stay coloured
// on continuation lines too.
const attrInside = {
  'attr-value': {
    pattern: /=[ \t]*(?:"[^"]*"|'[^']*')/,
    inside: {
      'punctuation': /^=|["']/,
      'builtin': /[A-Za-z_][\w.-]*:[A-Za-z_][\w.-]*/,
    },
  },
  'attr-name': {
    pattern: /[\w:-]+(?=[ \t]*=)/,
    inside: { 'namespace': /^[^\s>\/:]+:/ },
  },
  'punctuation': /\/?>/,
};

Prism.languages.insertBefore('xsd', 'tag', {
  // (a) Opening fragment — line starts with `<tag` and does NOT close on the
  //     same line. The trailing `(?<!>)` lookbehind ensures we don't shadow
  //     the stock `tag` pattern when a complete tag fits on one line.
  //     Example:  `<xsd:element name="qualifica" type="xsd:string"`
  'tag-open-fragment': {
    pattern: /<\/?[a-zA-Z_][\w:-]*(?:[ \t]+[\w:-]+(?:[ \t]*=[ \t]*(?:"[^"]*"|'[^']*'))?)*[ \t]*$(?<!>)/m,
    inside: {
      'tag': {
        pattern: /^<\/?[a-zA-Z_][\w:-]*/,
        inside: {
          'punctuation': /^<\/?/,
          'namespace': /^[^\s>\/:]+:/,
        },
      },
      ...attrInside,
    },
    alias: 'tag',
  },

  // (b) Attribute-only continuation line — no `<`, possibly closing with
  //     `>` or `/>`. Anchored with `^…$/m` to guarantee single-line scope.
  //     Example:  `             maxOccurs="1" minOccurs="0">`
  'tag-attr-line': {
    pattern: /^[ \t]+[\w:-]+[ \t]*=[ \t]*(?:"[^"]*"|'[^']*')(?:[ \t]+[\w:-]+[ \t]*=[ \t]*(?:"[^"]*"|'[^']*'))*[ \t]*\/?>?[ \t]*$/m,
    inside: attrInside,
    alias: 'tag',
  },

  // (c) Closing-only line: `>` or `/>` (often after a wrapped attr list).
  'tag-close-fragment': {
    pattern: /^[ \t]*\/?>[ \t]*$/m,
    alias: 'punctuation',
  },
});

// ── 3. Documentation / annotation block (whole-file paths only) ────────────
// Multi-line; cannot match in per-line views but is essentially free on
// whole-file paths and gives `<xsd:documentation>` body prose a comment
// colour instead of leaving it as raw text.
Prism.languages.insertBefore('xsd', 'tag-open-fragment', {
  'doc-block': {
    pattern: /<(xs|xsd):(?:documentation|annotation)\b[^>]*?>[\s\S]*?<\/\1:(?:documentation|annotation)>/,
    greedy: true,
    inside: {
      'tag': tag,
      'comment': /[\s\S]+/,
    },
  },
});
