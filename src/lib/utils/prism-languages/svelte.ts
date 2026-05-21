/**
 * Svelte — custom Prism grammar + per-line smart dispatcher.
 *
 * No official prismjs component exists for Svelte, so we build one here.
 *
 * Strategy
 * ─────────
 * • Extend TypeScript as the base so lines inside <script> blocks get full
 *   TS colouring without any extra logic.
 * • Layer HTML/template tokens on top (inserted before 'string') so they win
 *   over generic TS patterns wherever markup is present.
 * • Add a second pass of patterns specifically for multi-line tags: in a diff
 *   viewer every line is highlighted in isolation, so `<button\n  on:click\n>`
 *   arrives as three separate strings that must each be identifiable.
 * • Export `highlightLine()` which dispatches each diff line to the right
 *   grammar (Svelte template, CSS, or plain TypeScript).
 *
 * Tokens covered
 * ──────────────
 *  HTML  │ <tag …>  </tag>  <Component />  <!-- comment -->
 *  Multi │ <tag (no >)  · directive-attr lines · attr=val lines · > / />
 *  Svelte│ {#if …}  {:else if …}  {/if}  {@html …}  {expression}
 *  TS    │ $: reactive label · everything else via TypeScript grammar
 *  CSS   │ property: value;  @media …  .selector {  (auto-detected)
 */

import Prism from 'prismjs';
import 'prismjs/components/prism-typescript';
import 'prismjs/components/prism-css';
import 'prismjs/components/prism-scss';

// ─── Grammar registration ──────────────────────────────────────────────────────

function register(): void {
  if (Prism.languages.svelte) return;

  // Base: TypeScript — script-block lines are highlighted automatically.
  Prism.languages.svelte = Prism.languages.extend('typescript', {});

  // Insert HTML/template tokens before TypeScript's 'string' so they take
  // precedence when the line contains markup.
  Prism.languages.insertBefore('svelte', 'string', {

    // ── HTML / Svelte comment ────────────────────────────────────────────────
    'html-comment': {
      pattern: /<!--(?:[^-]|-(?!->))*(?:-->|$)/,
      alias: 'comment',
      greedy: true,
    },

    // ── Complete HTML / Component tag (greedy) ───────────────────────────────
    //   Matches self-closing and tags up to the first unquoted >.
    //   PascalCase tag names are treated as Svelte components (class-name).
    'html-tag': {
      pattern: /<\/?(?:[A-Z][a-zA-Z0-9.]*|[a-z][a-zA-Z0-9]*(?::[a-zA-Z0-9]+)?)\b[^>]*\/?>/,
      greedy: true,
      inside: {
        // Tag name portion: <div  </div  <MyComp
        'tag-name': {
          pattern: /^<\/?[a-zA-Z][a-zA-Z0-9.:]*\b/,
          inside: {
            punctuation: /^<\/?/,
            'class-name': /^[A-Z][a-zA-Z0-9.]*/,
            tag: { pattern: /^[a-z][a-zA-Z0-9.:]*/, alias: 'keyword' },
          },
        },
        // Svelte directives — must precede generic attr-name to win the match.
        //   on:click={h}  bind:value  class:active={b}  use:tooltip  …
        'svelte-directive': {
          pattern: /\b(?:on|bind|class|use|transition|in|out|animate|let|style):[a-zA-Z_][a-zA-Z0-9_.-]*(?:\s*=\s*(?:"[^"]*"|'[^']*'|\{(?:[^{}]|\{[^{}]*\})*\}))?/,
          inside: {
            'directive-prefix': { pattern: /^[a-z]+(?=:)/, alias: 'keyword' },
            punctuation: /[:=]/,
            // Expression inside the directive value: {handler}
            'svelte-expr': {
              pattern: /\{(?:[^{}]|\{[^{}]*\})*\}/,
              inside: {
                punctuation: /[{}]/,
                expression: { pattern: /[^{}]+/, inside: Prism.languages.typescript },
              },
            },
            'attr-value': /(?:"[^"]*"|'[^']*')/,
          },
        },
        // Regular attribute values: ="string"  ={expr}
        'attr-value': {
          pattern: /=\s*(?:"[^"]*"|'[^']*'|\{(?:[^{}]|\{[^{}]*\})*\}|[^\s>]+)/,
          inside: {
            punctuation: [{ pattern: /^=/, alias: 'attr-equals' }, /["']/],
            'svelte-expr': {
              pattern: /\{(?:[^{}]|\{[^{}]*\})*\}/,
              inside: {
                punctuation: /[{}]/,
                expression: { pattern: /[^{}]+/, inside: Prism.languages.typescript },
              },
            },
          },
        },
        'attr-name': /[\w:-]+/,
        punctuation: /\/?>/,
      },
    },

    // ── Svelte block tags ────────────────────────────────────────────────────
    //   {#if cond}  {:else if cond}  {:else}  {/if}  {@html …}  etc.
    'svelte-block': {
      pattern: /\{(?:[#/][a-z]+|:[a-z]+(?:\s+if)?|@[a-z]+)(?:\s(?:[^{}]|\{[^{}]*\})*)?\}/i,
      greedy: true,
      inside: {
        punctuation: /[{}]/,
        keyword: /[#/:@][a-z]+(?:\s+if)?/i,
        expression: {
          pattern: /\s[\s\S]+(?=\})/,
          inside: Prism.languages.typescript,
        },
      },
    },

    // ── Inline expression: {value}  {fn()}  (1-level brace nesting) ─────────
    'svelte-expression': {
      pattern: /\{(?:[^{}]|\{[^{}]*\})*\}/,
      inside: {
        punctuation: /^[{]|[}]$/,
        expression: { pattern: /[\s\S]+/, inside: Prism.languages.typescript },
      },
    },
  });

  // ── Multi-line tag support ─────────────────────────────────────────────────
  //
  // When a tag is split across lines the diff viewer passes each line
  // individually to highlight(). These patterns catch the fragments:
  //
  //   <Button              ← tag opener with no >
  //     on:click={handler} ← directive attribute line
  //     class="foo"        ← regular attribute line
  //     disabled           ← boolean attribute line
  //   />                   ← closing bracket line
  //
  // Patterns are anchored to the start/end of the string (each call receives
  // exactly one line with the trailing \n already stripped).
  Prism.languages.insertBefore('svelte', 'keyword', {

    // <TagName  or  </TagName  — opening fragment, no closing >
    'tag-open-fragment': {
      pattern: /^\s*<\/?(?:[A-Z][a-zA-Z0-9.]*|[a-z][a-zA-Z0-9:.-]*)\b/,
      inside: {
        punctuation: /^<\/?/,
        'class-name': /[A-Z][a-zA-Z0-9.]*/,
        tag: { pattern: /[a-z][a-zA-Z0-9.:]*$/, alias: 'keyword' },
      },
    },

    // Directive attribute on its own line: `  on:click={handler}`
    //   End-of-string anchor ($) prevents matching inside TS expressions.
    'directive-attr-line': {
      pattern: /^\s+(?:on|bind|class|use|transition|in|out|animate|let|style):[a-zA-Z_][a-zA-Z0-9_.-]*(?:\s*=\s*(?:"[^"]*"|'[^']*'|\{(?:[^{}]|\{[^{}]*\})*\}))?(?=\s*$)/,
      inside: {
        'directive-prefix': { pattern: /[a-z]+(?=:)/, alias: 'keyword' },
        punctuation: /[:={}]/,
        'attr-value': /(?:"[^"]*"|'[^']*')/,
        expression: { pattern: /[^{}=:\s"'][^{}]*/, inside: Prism.languages.typescript },
      },
    },

    // Regular attribute on its own line: `  class="bar"` or `  id={myId}`
    'attr-value-line': {
      pattern: /^\s+[\w:-]+=(?:"[^"]*"|'[^']*'|\{(?:[^{}]|\{[^{}]*\})*\})(?=\s*$)/,
      inside: {
        'attr-name': /^[\w:-]+/,
        punctuation: /[={}]/,
        'attr-value': /(?:"[^"]*"|'[^']*')/,
        expression: { pattern: /[^{}]+/, inside: Prism.languages.typescript },
      },
    },

    // Boolean attribute on its own line: `  disabled`  `  required`  `  autofocus`
    //   Restricted to all-lowercase so it doesn't swallow TS identifiers.
    'bool-attr-line': {
      pattern: /^\s+[a-z][a-z-]*[a-z](?=\s*$)/,
      alias: 'attr-name',
    },

    // Closing bracket alone: `>` or `/>`
    'tag-close-line': {
      pattern: /^\s*\/?>\s*$/,
      alias: 'punctuation',
    },

    // Svelte $: reactive label
    'svelte-reactive': {
      pattern: /^\s*\$:/,
      alias: 'keyword',
    },
  });
}

register();

// ─── svelte-css grammar ───────────────────────────────────────────────────────
//
// Prism's built-in CSS/SCSS grammar only tokenises functions (var, rgba…),
// strings, punctuation and at-rules. It leaves property names, numbers with
// units, hex colours and keyword values as plain (unstyled) text.
//
// This grammar extends SCSS and adds the missing tokens so that a line like
//   `  background-color: rgba(0,0,0,0.5);`
// becomes:
//   attr-name   punctuation  function  number …   punctuation
//   background-color  :      rgba      (0,0,0,0.5)  ;
//
// Token → theme alias mapping (see app.css .token.* rules):
//   property name  → attr-name  (yellow  #ffc66d)
//   number / unit  → number     (purple  #9876aa)
//   hex colour     → number     (purple)
//   keyword value  → keyword    (orange  #cc7832)

function registerSvelteCSS(): void {
  if (Prism.languages['svelte-css']) return;

  Prism.languages['svelte-css'] = Prism.languages.extend('scss', {});

  // Insert before 'function' so our tokens win when there is ambiguity.
  Prism.languages.insertBefore('svelte-css', 'function', {

    // ── CSS property name ────────────────────────────────────────────────────
    //   Matches the identifier (possibly hyphenated or a custom --prop) that
    //   appears at the start of the line, before the colon separator.
    //   Examples: `color`, `background-color`, `--my-token`, `font-size`
    'css-prop': {
      pattern: /^\s*-?-?[\w-]+(?=\s*:)/,
      alias: 'attr-name',
    },

    // ── Numbers with optional CSS unit ───────────────────────────────────────
    //   Covers: 16px  1.5em  100%  0.5s  360deg  0  1  1.5  -4px  …
    //   Lookahead ensures the trailing delimiter is not consumed.
    'css-number': {
      pattern: /-?\b\d+(?:\.\d+)?(?:px|r?em|ch|ex|vw|vh|vmin|vmax|svw|svh|cqw|cqh|fr|pt|pc|cm|mm|in|s|ms|deg|rad|turn|grad|dpi|dpcm|dppx|%)\b|-?\b\d+(?:\.\d+)?(?=\s*[,;)\s]|$)/i,
      alias: 'number',
    },

    // ── Hex colour literals ─────────────────────────────────────────────────
    //   #rgb  #rrggbb  #rgba  #rrggbbaa
    'css-hex': {
      pattern: /#(?:[0-9a-fA-F]{3,4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})\b/,
      alias: 'number',
    },

    // ── CSS keyword values ───────────────────────────────────────────────────
    //   Only highlights common values; avoids clashing with property names
    //   because this token is tried AFTER css-prop (which consumes line start).
    'css-value-kw': {
      pattern: /\b(?:none|auto|inherit|initial|unset|revert(?:-layer)?|normal|bold|bolder|lighter|italic|oblique|flex|grid|block|inline(?:-block|-flex|-grid|-table)?|table(?:-cell|-row|-column)?|flow(?:-root)?|contents|absolute|relative|fixed|sticky|static|hidden|visible|scroll|clip|collapse|auto|nowrap|wrap(?:-reverse)?|row(?:-reverse)?|column(?:-reverse)?|dense|space-(?:between|around|evenly)|flex-(?:start|end)|center|stretch|baseline|start|end|self-start|self-end|solid|dashed|dotted|double|groove|ridge|inset|outset|pointer|default|crosshair|move|not-allowed|grab(?:bing)?|zoom-(?:in|out)|text|transparent|currentColor|ease(?:-in(?:-out)?|-out)?|linear|step-(?:start|end)|infinite|alternate(?:-reverse)?|both|forwards|backwards|running|paused|left|right|top|bottom|middle|justify|uppercase|lowercase|capitalize|underline|overline|line-through|no-repeat|repeat(?:-[xy])?|cover|contain|fill|stroke|pre(?:-wrap|-line)?|break-(?:all|word)|keep-all|flat|preserve-3d|no-drop|vertical|horizontal|ltr|rtl|capitalize|round|butt|square|miter|bevel|nonzero|evenodd|pan-(?:x|y)|manipulation|pinch-zoom|all|revert)\b/,
      alias: 'keyword',
    },
  });
}

registerSvelteCSS();

// ─── CSS line detection ────────────────────────────────────────────────────────
//
// Because each diff line is highlighted in isolation we can't know whether a
// line is inside a <style> block. These heuristics cover the common cases
// without false-positives on TypeScript.

// TypeScript statement starters — lines beginning with any of these are TS.
const TS_LEAD =
  /^(?:import |export |const |let |var |function |class |type |interface |return |if |else |for |while |do\b|switch |case |break|continue|async |await |throw |try\b|catch|new |delete |typeof |void |\/\/|\/\*|\* )/;

// Known single-word CSS property names (hyphenated ones are detected by the
// `prop.includes('-')` check and don't need to be listed here).
const KNOWN_CSS_PROPS = new Set([
  'color', 'background', 'border', 'margin', 'padding', 'width', 'height',
  'display', 'position', 'top', 'right', 'bottom', 'left', 'flex', 'grid',
  'gap', 'opacity', 'overflow', 'cursor', 'font', 'visibility', 'transform',
  'transition', 'animation', 'content', 'outline', 'float', 'clear',
  'direction', 'resize', 'appearance', 'order', 'filter', 'isolation',
  'inset', 'rotate', 'scale', 'translate', 'perspective',
]);

function isCssLine(code: string): boolean {
  const t = code.trim();
  if (!t || TS_LEAD.test(t)) return false;

  // @-rules: @media, @keyframes, @apply, @tailwind, @layer …
  if (/^@[\w-]/.test(t)) return true;

  // Selector line ending with {: `.foo {`  `&:hover {`  `#id {`
  if (/^[.#&*:[[\w>~+][^{]*\{\s*$/.test(t)) return true;

  // property: value[;]
  const m = t.match(/^([\w-]+)\s*:\s*([\s\S]+?)(?:\s*;)?\s*$/);
  if (!m) return false;

  const prop = m[1];
  const value = m[2].trim();

  // Hyphenated property or CSS custom property (--var-name)
  if (prop.includes('-') || prop.startsWith('--')) return true;

  if (!KNOWN_CSS_PROPS.has(prop)) return false;

  // Guard: TypeScript type annotation `name: PascalCaseType` without semicolon
  if (/^[A-Z]/.test(value) && !t.endsWith(';')) return false;

  return true;
}

// ─── Exported line highlighter ────────────────────────────────────────────────

export function highlightLine(code: string): string {
  if (!code.trim()) return code;

  try {
    // ── HTML comment, tags, Svelte template syntax ─────────────────────────
    if (
      /^\s*</.test(code)       ||  // tag/comment open (full or fragment)
      /\{[#/:@]/.test(code)    ||  // svelte block  {#if …}  {:else}  {/if}  {@html …}
      /^\s*\/?>\s*$/.test(code)    // lone `>` or `/>` closing a multi-line tag
    ) {
      return Prism.highlight(code, Prism.languages.svelte, 'svelte');
    }

    // ── Attribute continuation lines (multi-line HTML tags) ────────────────
    if (/^\s+(?:on|bind|class|use|transition|in|out|animate|let|style):/.test(code)) {
      return Prism.highlight(code, Prism.languages.svelte, 'svelte');
    }
    if (/^\s+[\w:-]+=(?:"[^"]*"|'[^']*'|\{)/.test(code)) {
      return Prism.highlight(code, Prism.languages.svelte, 'svelte');
    }

    // ── CSS inside <style> blocks ──────────────────────────────────────────
    if (isCssLine(code)) {
      return Prism.highlight(code, Prism.languages['svelte-css'], 'svelte-css');
    }

    // ── Default: TypeScript (covers <script> block content, $: labels …) ──
    return Prism.highlight(code, Prism.languages.typescript, 'typescript');
  } catch {
    return escapeHtml(code);
  }
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}
