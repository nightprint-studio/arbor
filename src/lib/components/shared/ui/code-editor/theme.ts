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

export const codeEditorTheme = EditorView.theme(
  {
    '&': {
      height: '100%',
      backgroundColor: 'var(--bg-base)',
      color: 'var(--text-primary)',
      fontFamily: 'var(--font-code)',
      fontSize: '12.5px',
    },
    '&.cm-focused': { outline: 'none' },
    '.cm-scroller': { fontFamily: 'var(--font-code)', lineHeight: '1.55', overflow: 'auto' },
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
    '.cm-foldMarker': { fontSize: '10px', lineHeight: '1', color: 'var(--text-disabled)' },
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
    '.cm-selectionBackground, .cm-content ::selection': {
      backgroundColor: 'var(--accent-subtle) !important',
    },
    '&.cm-focused .cm-selectionBackground': { backgroundColor: 'var(--accent-subtle) !important' },
    '.cm-matchingBracket': {
      outline: '1px solid var(--accent-strong, var(--accent))', borderRadius: '2px',
    },
    '.cm-tooltip': {
      backgroundColor: 'var(--bg-elevated)', color: 'var(--text-primary)',
      border: '1px solid var(--border-subtle)', borderRadius: 'var(--radius-md)',
    },
    '.cm-tooltip.cm-tooltip-lint': { padding: '2px 6px' },

    // ── Search panel (Ctrl+F) — themed to match Arbor's inputs/buttons ──
    '.cm-panels': { backgroundColor: 'var(--bg-elevated)', color: 'var(--text-primary)' },
    '.cm-panels.cm-panels-top': { borderBottom: '1px solid var(--border-subtle)' },
    '.cm-panel.cm-search': {
      padding: '6px 8px', fontFamily: 'var(--font-ui-sans)', fontSize: '12px',
      display: 'flex', alignItems: 'center', flexWrap: 'wrap', gap: '6px',
    },
    '.cm-panel.cm-search label': { display: 'inline-flex', alignItems: 'center', gap: '3px', fontSize: '11px', color: 'var(--text-muted)' },
    '.cm-panel.cm-search input, .cm-panel.cm-search input[type=text]': {
      backgroundColor: 'var(--bg-input)', color: 'var(--text-primary)',
      border: '1px solid var(--border-subtle)', borderRadius: 'var(--radius-sm)',
      padding: '3px 6px', fontFamily: 'var(--font-code)', fontSize: '12px', outline: 'none',
    },
    '.cm-panel.cm-search input:focus': { borderColor: 'var(--border-focus, var(--accent))' },
    '.cm-panel.cm-search button, .cm-panel.cm-search .cm-button': {
      backgroundColor: 'var(--bg-input)', color: 'var(--text-secondary)',
      border: '1px solid var(--border-subtle)', borderRadius: 'var(--radius-sm)',
      backgroundImage: 'none', padding: '3px 8px', cursor: 'pointer', fontSize: '11px',
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

    // ── Autocomplete + hover docs ──
    '.cm-tooltip-autocomplete': {
      backgroundColor: 'var(--bg-elevated)',
      border: '1px solid var(--border-subtle)', borderRadius: 'var(--radius-md)',
    },
    '.cm-tooltip-autocomplete > ul > li': {
      fontFamily: 'var(--font-code)', fontSize: '12px',
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
