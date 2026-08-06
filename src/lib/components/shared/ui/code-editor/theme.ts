/**
 * The Arbor CSS-var CodeMirror theme, generalised from merula's `merulaTheme`.
 *
 * Every colour is an Arbor CSS variable, so a theme overlay re-skins the editor for
 * free, and the editing surface sits on `--bg-base` like the rest of the app. The
 * token palette uses the standard `--syntax-*` vars (with sensible fallbacks) and
 * the `cm-tok-<class>` classes emitted by {@link import('./highlight').createHighlightPlugin}
 * — one class per {@link import('./types').TokenClass}.
 */

import { EditorView } from '@codemirror/view';
import { HighlightStyle, syntaxHighlighting } from '@codemirror/language';
import { tags as t } from '@lezer/highlight';
import { namespaceThemeSpec } from './namespace-palette';

/**
 * Lezer highlight style for CodeMirror-built-in / legacy-mode languages (the ones a
 * {@link import('./types').LanguageDescriptor} plugs in via `cmExtension`: XML, YAML,
 * JSON, CSS, JS, Markdown, …). It maps `@lezer/highlight` tags onto the same
 * `--syntax-*` CSS vars the tree-sitter `cm-tok-*` classes use, so both highlighting
 * paths read identically under any theme overlay. Harmless for the tree-sitter path
 * (that emits mark decorations, not Lezer tags), so it can sit in the base extension
 * set unconditionally.
 */
const lezerHighlightStyle = HighlightStyle.define([
  { tag: t.comment, color: 'var(--syntax-comment, #808080)', fontStyle: 'italic' },
  { tag: [t.string, t.special(t.string), t.attributeValue], color: 'var(--syntax-string, #6a8759)' },
  { tag: [t.number, t.bool, t.atom], color: 'var(--syntax-number, #6897bb)' },
  { tag: t.keyword, color: 'var(--syntax-keyword, #cc7832)', fontWeight: '600' },
  { tag: [t.typeName, t.className, t.namespace], color: 'var(--syntax-type, #4d9be6)' },
  { tag: [t.function(t.variableName), t.function(t.propertyName)], color: 'var(--syntax-function, #ffc66d)' },
  { tag: [t.propertyName, t.attributeName], color: 'var(--syntax-field, #9876aa)' },
  // A *declaration* — the name a line brings into existence, as opposed to the names it
  // mentions. Legacy stream modes emit this as `def` (a DTD's `<!ELEMENT name`, a shell
  // function, a `def` in the JS mode); without it a file of definitions reads as flat text.
  { tag: t.definition(t.variableName), color: 'var(--syntax-function, #ffc66d)', fontWeight: '600' },
  { tag: t.variableName, color: 'var(--text-primary)' },
  { tag: t.tagName, color: 'var(--syntax-keyword, #cc7832)' },
  { tag: [t.meta, t.annotation, t.processingInstruction], color: 'var(--syntax-annotation, #bbb529)' },
  { tag: t.constant(t.variableName), color: 'var(--syntax-constant, #9876aa)', fontStyle: 'italic' },
  { tag: [t.operator, t.punctuation, t.separator, t.bracket], color: 'var(--text-secondary)' },
  { tag: t.invalid, color: 'var(--error)' },
  { tag: t.heading, color: 'var(--syntax-keyword, #cc7832)', fontWeight: '600' },
  { tag: [t.link, t.url], color: 'var(--syntax-type, #4d9be6)', textDecoration: 'underline' },
  { tag: t.emphasis, fontStyle: 'italic' },
  { tag: t.strong, fontWeight: '700' },
  { tag: t.quote, color: 'var(--syntax-string, #6a8759)' },
]);

/** The Lezer syntax-highlighting extension (add once to the base editor set). */
export const codeEditorHighlightStyle = syntaxHighlighting(lezerHighlightStyle);

export const codeEditorTheme = EditorView.theme(
  {
    '&': {
      height: '100%',
      backgroundColor: 'var(--bg-base)',
      color: 'var(--text-primary)',
      fontFamily: 'var(--font-code)',
      fontSize: 'var(--font-size-sm)',
    },
    '&.cm-focused': { outline: 'none' },
    // `position: relative` makes the scroller the containing block for the ruler guide
    // (an absolutely-positioned child that must scroll with the content on both axes).
    '.cm-scroller': { fontFamily: 'var(--font-code)', lineHeight: '1.55', overflow: 'auto', position: 'relative' },
    // Vertical margin guide (see `editorRuler`): a faint line at a column. `top:0` +
    // an explicit `height` (set to the full content height by the plugin) makes it span
    // the whole document and scroll with it — a `bottom:0` here would instead size it to
    // the *visible* box, so it wouldn't follow a vertical scroll.
    '.cm-ruler': {
      position: 'absolute', top: '0', width: '0',
      borderLeft: '1px solid var(--border-subtle)',
      pointerEvents: 'none',
    },
    '.cm-content': { padding: '6px 0', caretColor: 'var(--text-primary)' },
    '.cm-line': { padding: '0 12px' },
    '.cm-gutters': {
      backgroundColor: 'var(--bg-base)',
      color: 'var(--text-disabled)',
      border: 'none',
      fontFamily: 'var(--font-code)',
    },
    '.cm-lineNumbers .cm-gutterElement': { padding: '0 8px 0 14px', minWidth: '34px' },
    '.cm-activeLineGutter': { backgroundColor: 'transparent', color: 'var(--text-secondary)' },
    // Fold gutter (collapse arrows) + the inline placeholder for a folded block.
    '.cm-foldGutter .cm-gutterElement': { padding: '0 2px', color: 'var(--text-disabled)', cursor: 'pointer' },
    '.cm-foldGutter .cm-gutterElement:hover': { color: 'var(--text-primary)' },
    // Chevron markers (markerDOM in folding.ts). Muted, brighten on hover; the
    // collapsed marker is a touch stronger so a folded block reads at a glance.
    '.cm-foldMarker': { fontSize: 'var(--font-size-2xs)', lineHeight: '1', color: 'var(--text-disabled)' },
    '.cm-foldGutter .cm-gutterElement:hover .cm-foldMarker': { color: 'var(--text-primary)' },
    '.cm-foldMarker-closed': { color: 'var(--text-muted)' },
    '.cm-foldPlaceholder': {
      backgroundColor: 'var(--bg-hover)', color: 'var(--text-muted)',
      border: '1px solid var(--border-subtle)', borderRadius: 'var(--radius-sm)',
      margin: '0 2px', padding: '0 4px', cursor: 'pointer',
    },
    '.cm-foldPlaceholder:hover': { color: 'var(--text-primary)', borderColor: 'var(--border-focus, var(--accent))' },
    // Current line: a touch warmer than plain hover and tinted with the accent so the
    // caret's row is easy to re-find at a glance. Kept subtle — it must not fight the
    // syntax colours.
    '.cm-activeLine': {
      backgroundColor: 'color-mix(in srgb, var(--accent) 9%, var(--bg-hover) 55%)',
    },
    // Caret: a 2px bright accent bar (the default 1px sliver is hard to spot against
    // the code). Both the steady + the drop cursor track it.
    '.cm-cursor, .cm-dropCursor': {
      borderLeftColor: 'var(--accent)',
      borderLeftWidth: '2px',
    },
    '&.cm-focused .cm-cursor': { borderLeftColor: 'var(--accent)' },
    // Selection: a clearly-visible accent wash (the faint `--accent-subtle` was hard
    // to see against the code). Focused selection is a touch stronger.
    '.cm-selectionBackground, .cm-content ::selection': {
      backgroundColor: 'color-mix(in srgb, var(--accent) 28%, transparent) !important',
    },
    '&.cm-focused .cm-selectionBackground': {
      backgroundColor: 'color-mix(in srgb, var(--accent) 34%, transparent) !important',
    },
    '.cm-matchingBracket': {
      outline: '1px solid var(--accent-strong, var(--accent))', borderRadius: '2px',
    },
    // Ctrl/Cmd-hover go-to affordance: the token a click would navigate is underlined and
    // the pointer becomes a hand (the mouse is over this span while Ctrl is held).
    '.cm-goto-link': {
      textDecoration: 'underline',
      textUnderlineOffset: '2px',
      textDecorationColor: 'var(--accent)',
      color: 'var(--accent)',
      cursor: 'pointer',
    },
    '.cm-tooltip': {
      backgroundColor: 'var(--bg-elevated)', color: 'var(--text-primary)',
      border: '1px solid var(--border-subtle)', borderRadius: 'var(--radius-md)',
    },
    // A diagnostic message is a sentence, but nothing guarantees it: a backend that
    // quotes the offending text can produce one the size of the file. Unbounded, it
    // covers the window and reads like a crash rather than a message about a line.
    // Bounded and scrollable, the worst case is a small box you can ignore.
    '.cm-tooltip.cm-tooltip-lint': {
      padding: '2px 6px', maxWidth: '520px', maxHeight: '340px', overflowY: 'auto',
    },
    '.cm-diagnostic': { whiteSpace: 'pre-wrap', overflowWrap: 'anywhere' },
    // The lint list (the panel) has the same exposure, one row per diagnostic.
    '.cm-panel.cm-panel-lint ul': { maxHeight: '180px' },
    '.cm-panel.cm-panel-lint li': { overflowWrap: 'anywhere' },

    // ── Refused paste (`pasteIntoLiteral`) ────────────────────────────────────
    // Shown at the caret when a language will not perform a paste. It states a
    // limit, so it is informative rather than alarming — warning, not error.
    '.cm-paste-hint': {
      padding: '6px 10px', maxWidth: '380px',
      fontFamily: 'var(--font-ui-sans)', fontSize: 'var(--font-size-xs)',
      lineHeight: '1.45', color: 'var(--text-primary)',
      borderLeft: '2px solid var(--warning)',
    },

    // ── Hover card (a language `intel.hover` source) ──────────────────────────
    // One card shape for every product: a monospaced title (a signature, a column
    // name), a muted meta line, and an optional wrapped body. Bennu renders symbol
    // signatures into it and Picus renders column facts; keeping the class names
    // product-neutral is what stops the second one from forking the CSS.
    '.cm-tooltip .cm-hover-card': {
      padding: '8px 11px', maxWidth: '520px', maxHeight: '340px', overflowY: 'auto',
    },
    // The head is the answer: a small kind tag, then the signature. They sit on one line
    // so the eye lands on the name, not on a label above it.
    '.cm-hover-card .cm-hc-head': {
      display: 'flex', alignItems: 'baseline', gap: '7px',
    },
    '.cm-hover-card .cm-hc-kind': {
      flexShrink: '0',
      fontFamily: 'var(--font-ui-sans)', fontSize: 'var(--font-size-3xs)', fontWeight: '700',
      letterSpacing: '0.05em', textTransform: 'uppercase',
      color: 'var(--accent)', backgroundColor: 'var(--accent-subtle)',
      borderRadius: 'var(--radius-sm)', padding: '1px 5px',
    },
    '.cm-hover-card .cm-hc-title': {
      fontFamily: 'var(--font-code)', fontSize: 'var(--font-size-sm)', color: 'var(--text-primary)',
      whiteSpace: 'pre-wrap', wordBreak: 'break-word',
    },
    '.cm-hover-card .cm-hc-meta': {
      fontFamily: 'var(--font-code)', fontSize: 'var(--font-size-2xs)', color: 'var(--text-muted)',
      marginTop: '3px', wordBreak: 'break-all',
    },
    '.cm-hover-card .cm-hc-doc': {
      fontFamily: 'var(--font-ui-sans)', fontSize: 'var(--font-size-xs)', color: 'var(--text-secondary)',
      lineHeight: '1.5',
      marginTop: '7px', paddingTop: '7px', borderTop: '1px solid var(--border-subtle)',
      whiteSpace: 'pre-wrap',
    },
    // `@param` / `@return` / `@throws` as a definition list: the subject in the left
    // column, its text in the right, so a six-parameter method stays readable.
    '.cm-hover-card .cm-hc-tags': {
      display: 'grid', gridTemplateColumns: 'max-content minmax(0, 1fr)', gap: '2px 10px',
      margin: '6px 0 0', paddingTop: '6px', borderTop: '1px solid var(--border-subtle)',
      fontSize: 'var(--font-size-2xs)',
    },
    '.cm-hover-card .cm-hc-tags dt': {
      fontFamily: 'var(--font-code)', color: 'var(--text-muted)', whiteSpace: 'nowrap',
    },
    '.cm-hover-card .cm-hc-tags dd': {
      margin: '0', fontFamily: 'var(--font-ui-sans)', color: 'var(--text-secondary)',
    },
    '.cm-hover-card .cm-hc-tags dt.cm-hc-deprecated': { color: 'var(--warning)', fontWeight: '700' },

    // ── Search panel (Ctrl+F) — themed to match Arbor's inputs/buttons ──
    '.cm-panels': { backgroundColor: 'var(--bg-elevated)', color: 'var(--text-primary)' },
    '.cm-panels.cm-panels-top': { borderBottom: '1px solid var(--border-subtle)' },
    '.cm-panel.cm-search': {
      padding: '6px 8px', fontFamily: 'var(--font-ui-sans)', fontSize: 'var(--font-size-sm)',
      display: 'flex', alignItems: 'center', flexWrap: 'wrap', gap: '6px',
    },
    '.cm-panel.cm-search label': { display: 'inline-flex', alignItems: 'center', gap: '3px', fontSize: 'var(--font-size-xs)', color: 'var(--text-muted)' },
    '.cm-panel.cm-search input, .cm-panel.cm-search input[type=text]': {
      backgroundColor: 'var(--bg-input)', color: 'var(--text-primary)',
      border: '1px solid var(--border-subtle)', borderRadius: 'var(--radius-sm)',
      padding: '3px 6px', fontFamily: 'var(--font-code)', fontSize: 'var(--font-size-sm)', outline: 'none',
    },
    '.cm-panel.cm-search input:focus': { borderColor: 'var(--border-focus, var(--accent))' },
    '.cm-panel.cm-search button, .cm-panel.cm-search .cm-button': {
      backgroundColor: 'var(--bg-input)', color: 'var(--text-secondary)',
      border: '1px solid var(--border-subtle)', borderRadius: 'var(--radius-sm)',
      backgroundImage: 'none', padding: '3px 8px', cursor: 'pointer', fontSize: 'var(--font-size-xs)',
    },
    '.cm-panel.cm-search button:hover, .cm-panel.cm-search .cm-button:hover': {
      backgroundColor: 'var(--bg-hover)', color: 'var(--text-primary)',
    },
    '.cm-panel.cm-search .cm-button:active': { backgroundImage: 'none' },
    '.cm-panel.cm-search [name=close]': { color: 'var(--text-muted)', fontSize: '16px' },
    '.cm-panel.cm-search [name=close]:hover': { color: 'var(--text-primary)' },
    '.cm-searchMatch': { backgroundColor: 'color-mix(in srgb, var(--warning) 28%, transparent)', borderRadius: '2px' },
    '.cm-searchMatch.cm-searchMatch-selected': { backgroundColor: 'color-mix(in srgb, var(--accent) 45%, transparent)' },
    '.cm-selectionMatch': { backgroundColor: 'color-mix(in srgb, var(--accent) 18%, transparent)' },

    // ── Token palette ──
    //
    // Conventional, hierarchical colouring using the standard Arbor `--syntax-*`
    // vars (with fallbacks tuned to an IntelliJ-Darcula-ish palette), so a theme
    // overlay re-skins the highlight app-wide while the defaults read cleanly on
    // a real .java file: keywords burnt-orange, types blue, method calls gold,
    // strings green, annotations olive, fields/`this` violet.
    '.cm-tok-comment':     { color: 'var(--syntax-comment, #808080)', fontStyle: 'italic' },
    '.cm-tok-string':      { color: 'var(--syntax-string, #6a8759)' },
    '.cm-tok-number':      { color: 'var(--syntax-number, #6897bb)' },
    '.cm-tok-constant':    { color: 'var(--syntax-constant, #9876aa)', fontStyle: 'italic' },
    '.cm-tok-keyword':     { color: 'var(--syntax-keyword, #cc7832)', fontWeight: '600' },
    '.cm-tok-type':        { color: 'var(--syntax-type, #4d9be6)' },
    '.cm-tok-function':    { color: 'var(--syntax-function, #ffc66d)' },
    // A method *declaration* name — same gold as a call but bolder so a
    // definition stands out from its call sites (IntelliJ underlines these; we
    // weight them instead to avoid fighting the fold gutter / active line).
    '.cm-tok-declaration': { color: 'var(--syntax-function, #ffc66d)', fontWeight: '600' },
    '.cm-tok-annotation':  { color: 'var(--syntax-annotation, #bbb529)' },
    // A field/property reference — violet, distinct from a plain local (which
    // stays text-primary) so instance state is legible at a glance.
    '.cm-tok-field':       { color: 'var(--syntax-field, #9876aa)' },
    // `this` / `super` — the keyword orange, italic, so self-reference reads as
    // language scaffolding rather than an identifier.
    '.cm-tok-self':        { color: 'var(--syntax-keyword, #cc7832)', fontStyle: 'italic' },
    '.cm-tok-label':       { color: 'var(--syntax-label, var(--text-secondary))' },
    '.cm-tok-ident':       { color: 'var(--text-primary)' },
    '.cm-tok-operator':    { color: 'var(--syntax-operator, var(--text-secondary))' },
    '.cm-tok-punctuation': { color: 'var(--text-muted)' },

    // ── Namespace palette ──
    //
    // `.cm-tok-ns-0…N` — a colour per namespace FAMILY rather than per token kind (a
    // JSP taglib prefix, an XML namespace). Categorical, not semantic: see
    // `namespace-palette.ts` for what the hues do and do not mean.
    ...namespaceThemeSpec,

    // ── Autocomplete + hover docs ──
    '.cm-tooltip-autocomplete': {
      backgroundColor: 'var(--bg-elevated)',
      border: '1px solid var(--border-subtle)', borderRadius: 'var(--radius-md)',
    },
    '.cm-tooltip-autocomplete > ul > li': {
      fontFamily: 'var(--font-code)', fontSize: 'var(--font-size-sm)',
      padding: '2px 6px', color: 'var(--text-primary)',
    },
    '.cm-tooltip-autocomplete > ul > li[aria-selected]': {
      backgroundColor: 'var(--accent-subtle)', color: 'var(--text-primary)',
    },
    '.cm-completionLabel': { color: 'var(--text-primary)' },
    '.cm-completionDetail': { color: 'var(--text-muted)', fontStyle: 'normal', marginLeft: '0.6em' },
    '.cm-completionMatchedText': { color: 'var(--accent)', textDecoration: 'none', fontWeight: '700' },
  },
  { dark: true },
);
