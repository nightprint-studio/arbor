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
  // A name the LANGUAGE brings, as opposed to one the file does. Legacy stream modes emit this
  // as `builtin` (`textureSample` in a shader, `echo` in a shell, `count` in SQL) and until now
  // nothing styled it, so every one of them rendered as ordinary text — which is the same as
  // saying the language has no standard library. Between a call's orange and plain text on
  // purpose: it IS a call, and it is not one you wrote.
  { tag: t.standard(t.variableName), color: 'var(--syntax-builtin, #8888c6)' },
  { tag: t.tagName, color: 'var(--syntax-keyword, #cc7832)' },
  // `this` / `super`. A keyword in weight, italic in shape — it names something rather than
  // doing something, and the same two rules apply to `cm-tok-self` on the tree-sitter path.
  { tag: t.self, color: 'var(--syntax-keyword, #cc7832)', fontStyle: 'italic' },
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
      // `--cm-font-size` is set on the mount host by the `fontSize` prop (a user setting);
      // an editor that never sets one keeps the app's code size.
      fontSize: 'var(--cm-font-size, var(--font-size-sm))',
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
    // Whitespace glyphs (the `showWhitespace` preference). CodeMirror's own dot is a hardcoded
    // grey that reads as dirt on a dark theme; the tab arrow it draws is an SVG data URI and
    // stays as it is — a neutral grey works for a line, not for a field of dots.
    '.cm-highlightSpace': {
      backgroundImage: 'radial-gradient(circle at 50% 55%, var(--text-disabled) 20%, transparent 5%)',
    },
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

    // ── Tooltip hosts that must shrink instead of clip ────────────────────────
    // CodeMirror measures a tooltip, and when there is less room above (or below) the
    // caret than the card is tall it sets an explicit `height` on the HOST element —
    // `.cm-tooltip-hover`, the div it wraps every hover section in. That host had no
    // `overflow`, so the forced height did not shorten the card, it **cropped** it: a long
    // Javadoc ended mid-sentence with no scrollbar anywhere, and the card's own
    // `overflow-y: auto` never engaged because the card itself still believed it had all
    // the room it asked for.
    //
    // A flex column with `min-height: 0` on the sections is the whole fix: the imposed
    // height reaches the section, the section gives it to its scrollable body, and the
    // worst case becomes a short card you scroll rather than a sentence that stops.
    '.cm-tooltip.cm-tooltip-hover': {
      display: 'flex', flexDirection: 'column', overflow: 'hidden',
    },
    '.cm-tooltip.cm-tooltip-hover > .cm-tooltip-section': { minHeight: '0' },

    // ── Hover card (a language `intel.hover` source) ──────────────────────────
    // One card shape for every product: a monospaced title (a signature, a column
    // name), a muted meta line, and an optional wrapped body. Bennu renders symbol
    // signatures into it and Picus renders column facts; keeping the class names
    // product-neutral is what stops the second one from forking the CSS.
    //
    // The headline never scrolls and the body does — a card whose signature scrolls away
    // is one you have to scroll back up to identify.
    '.cm-tooltip .cm-hover-card': {
      display: 'flex', flexDirection: 'column', overflow: 'hidden',
      minHeight: '0', maxWidth: '640px', maxHeight: '440px',
      fontFamily: 'var(--font-ui-sans)',
    },
    '.cm-hover-card .cm-hc-headline': { flex: '0 0 auto', padding: '8px 11px 7px' },
    '.cm-hover-card .cm-hc-body': {
      flex: '1 1 auto', minHeight: '0', overflowY: 'auto', overscrollBehavior: 'contain',
      padding: '8px 11px 9px', borderTop: '1px solid var(--border-subtle)', outline: 'none',
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
    // Fainter than the package above it: it answers a rarer question, and a coordinate is long
    // enough that at the package's weight it would be the loudest thing on the card.
    '.cm-hover-card .cm-hc-artifact': {
      fontFamily: 'var(--font-code)', fontSize: 'var(--font-size-2xs)', color: 'var(--text-disabled)',
      marginTop: '1px', wordBreak: 'break-all',
    },
    '.cm-hc-doc': {
      fontFamily: 'var(--font-ui-sans)', fontSize: 'var(--font-size-xs)', color: 'var(--text-secondary)',
      lineHeight: '1.5', wordBreak: 'break-word',
    },

    // ── Rendered Javadoc ──────────────────────────────────────────────────────
    // A doc comment is HTML and it is now rendered as such (`hover-card.ts`), so it needs a small
    // typographic scale of its own. Scoped to `.cm-hc-doc` and NOT to the card around it: the same
    // renderer draws the hover tooltip and the completion popup's documentation panel, and a
    // stylesheet that only reached one of them would leave the other as raw markup. Deliberately flat: one heading size for
    // every `<h1>`–`<h6>`, one code style. A tooltip that reproduces a web page's hierarchy
    // is a tooltip nobody skims.
    // Spacing above rather than below, so the paragraph that opens a body — which is bare text,
    // not a `<p>`, because nothing precedes it to separate it from — does not sit flush against
    // the one after it.
    '.cm-hc-doc p': { margin: '6px 0 0' },
    '.cm-hc-doc p:first-child': { marginTop: '0' },
    '.cm-hc-doc h4': {
      margin: '9px 0 4px', fontSize: 'var(--font-size-3xs)', fontWeight: '700',
      letterSpacing: '0.05em', textTransform: 'uppercase', color: 'var(--text-muted)',
    },
    '.cm-hc-doc h4:first-child': { marginTop: '0' },
    '.cm-hc-doc ul, .cm-hc-doc ol': {
      margin: '4px 0 6px', paddingLeft: '17px',
    },
    '.cm-hc-doc li': { margin: '2px 0' },
    '.cm-hc-doc code': {
      fontFamily: 'var(--font-code)', fontSize: '0.92em',
      backgroundColor: 'var(--bg-base)', border: '1px solid var(--border-subtle)',
      borderRadius: 'var(--radius-sm)', padding: '0 3px',
    },
    // A `{@link}` names something you could go and read: coloured like the accent so it
    // reads as a reference, not as a literal you are meant to type.
    '.cm-hc-doc code.cm-hc-ref': {
      color: 'var(--accent)', backgroundColor: 'transparent', border: 'none', padding: '0',
    },
    // A code sample scrolls sideways rather than wrapping: a wrapped line of Java is a line
    // that no longer says what it did.
    '.cm-hc-doc pre': {
      margin: '6px 0', padding: '6px 8px',
      backgroundColor: 'var(--bg-base)', border: '1px solid var(--border-subtle)',
      borderRadius: 'var(--radius-sm)',
      overflowX: 'auto', whiteSpace: 'pre',
      fontFamily: 'var(--font-code)', fontSize: 'var(--font-size-2xs)', lineHeight: '1.45',
      color: 'var(--text-primary)',
    },
    '.cm-hc-doc pre code': {
      backgroundColor: 'transparent', border: 'none', padding: '0', fontSize: 'inherit',
    },
    '.cm-hc-doc blockquote': {
      margin: '6px 0', paddingLeft: '8px',
      borderLeft: '2px solid var(--border-subtle)', color: 'var(--text-muted)',
    },
    '.cm-hc-doc table': {
      borderCollapse: 'collapse', margin: '6px 0', fontSize: 'var(--font-size-2xs)',
    },
    '.cm-hc-doc td, .cm-hc-doc th': {
      border: '1px solid var(--border-subtle)', padding: '2px 6px', textAlign: 'left',
    },
    '.cm-hc-doc hr': {
      border: 'none', borderTop: '1px solid var(--border-subtle)', margin: '7px 0',
    },
    // Text, not a link — clicking one inside a Tauri webview would navigate the app itself.
    '.cm-hc-doc .cm-hc-anchor': {
      color: 'var(--accent)', textDecoration: 'underline', textUnderlineOffset: '2px',
    },
    '.cm-hc-doc dl': { margin: '4px 0' },
    '.cm-hc-doc dd': { margin: '0 0 4px 12px' },

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
      lineHeight: '1.45',
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
    // ── Structure inside a comment ──
    //
    // Mixed *from* the comment colour towards the accent each part echoes, instead of taking
    // that accent whole: a `@title:` is a comment first and metadata second, and painting it
    // full keyword-orange turns a twenty-line doc block into the loudest thing on screen.
    // Starting at the comment grey and stepping towards the accent keeps the hierarchy and
    // still says which part is which. The italic stays for the same reason.
    '.cm-tok-comment-directive': {
      color: 'color-mix(in srgb, var(--syntax-comment, #808080) 38%, var(--syntax-keyword, #cc7832))',
      fontStyle: 'italic',
      fontWeight: '600',
    },
    '.cm-tok-comment-code': {
      color: 'color-mix(in srgb, var(--syntax-comment, #808080) 38%, var(--syntax-function, #ffc66d))',
      fontStyle: 'italic',
    },
    // Emphasis does not change subject, it raises the voice: no hue of its own, just
    // brighter than the prose around it.
    '.cm-tok-comment-strong': {
      color: 'color-mix(in srgb, var(--syntax-comment, #808080) 45%, var(--text-primary))',
      fontStyle: 'italic',
      fontWeight: '600',
    },
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

    // ── Semantic-token classes ──
    //
    // Emitted only by a language server (see `semantic-tokens.ts`), which knows things a
    // grammar cannot: that `Foo` is a trait rather than a struct, that `println!` is a macro,
    // that a binding is `mut`. Each of these is a distinction a stream highlighter has no way
    // to draw, which is why they earn their own colours rather than folding into the ones above.
    //
    // ⚠️ The rule that makes the layering actually work. The base highlighter does NOT leave
    // identifiers unstyled — `t.variableName` above paints them `--text-primary` — so where both
    // layers cover the same text there are two competing `color` rules on two **nested** spans,
    // and the inner one wins regardless of selector specificity. Which of the two is inner
    // follows decoration precedence, not anything the semantic layer decides. Without this,
    // the base layer's white could win and a Rust file came out as flat text with coloured
    // keywords — which is exactly how the bug looked.
    //
    // Keyed on `.cm-sem` (the marker every semantic span carries) rather than on `cm-tok-*` in
    // general: a tree-sitter language's *injections* nest coloured spans inside a token on
    // purpose (a JSP `<script>` body), and flattening those would be a real regression.
    '.cm-sem span': { color: 'inherit' },
    //
    // A macro invocation is neither a call nor a keyword; the annotation olive puts it in the
    // same visual family as a Java annotation, which is the closest thing it is.
    '.cm-tok-macro':       { color: 'var(--syntax-annotation, #bbb529)' },
    // A lifetime is type-adjacent scaffolding — the type blue, italic and slightly muted so
    // `'a` does not shout as loudly as the type it qualifies.
    '.cm-tok-lifetime':    { color: 'var(--syntax-type, #4d9be6)', fontStyle: 'italic', opacity: '0.85' },
    // A parameter, distinct from both a local (text-primary) and a field (violet): inside a
    // long function the difference between "this came in" and "this is ours" is what you scan for.
    '.cm-tok-parameter':   { color: 'var(--syntax-parameter, #a9b7c6)', fontStyle: 'italic' },
    // `\n` in a string, `{:?}` in a `format!` — code hiding inside a literal, and much easier
    // to read picked out from it.
    '.cm-tok-escape':      { color: 'var(--syntax-escape, #cc7832)', fontWeight: '600' },
    // An unresolved reference. Underlined rather than recoloured: a squiggle already carries
    // the error, and a second red would make an ordinary in-progress edit look broken.
    '.cm-tok-invalid':     { textDecoration: 'underline wavy', textDecorationColor: 'var(--danger, #d16969)' },

    // ── Mini-notation (merula's `.merula`) ──
    //
    // The six token kinds merula's grammar has that no other language here does. Same CSS
    // variables — and therefore the same colours — as the Merula window's own editor
    // (`merula/editor/merula-cm.ts`), because a file must not change appearance depending on
    // which window it is open in.
    //
    // A note and a chord are the *pitch* material and share a hue; the chord is italic + bold
    // because `'maj7` is a modifier on the note beside it, not a separate voice. A sound name
    // is the other half of the alphabet — what plays, versus what pitch — so it gets the
    // cyan. The island brackets and a `$splice` are the seams where mini-notation meets host
    // code, which is the thing you look for when a pattern misbehaves, so they are the accent
    // and the only bold ones. `~` and `_` are structure, not content: muted on purpose, so a
    // dense pattern reads as its sounds rather than as its rests.
    '.cm-tok-note':        { color: 'var(--grv-syntax-note, #e5c07b)' },
    '.cm-tok-chord':       { color: 'var(--grv-syntax-note, #e5c07b)', fontStyle: 'italic', fontWeight: '600' },
    '.cm-tok-sound':       { color: 'var(--grv-syntax-sound, #56b6c2)' },
    '.cm-tok-island':      { color: 'var(--accent, #56b6c2)', fontWeight: '700' },
    '.cm-tok-splice':      { color: 'var(--accent, #56b6c2)', fontWeight: '600' },
    '.cm-tok-mininote':    { color: 'var(--text-muted)' },

    // ── Semantic-token MODIFIERS ──
    //
    // Layered on top of a class rather than replacing it, so `mut count` keeps its identifier
    // colour and gains the mark. Deliberately restrained: these ride on top of a colour that is
    // already carrying meaning, so anything loud here fights it.
    '.cm-tokmod-mutable':  { textDecoration: 'underline', textDecorationColor: 'var(--border-strong, #555)', textUnderlineOffset: '2px' },
    '.cm-tokmod-unsafe':   { backgroundColor: 'var(--danger-bg, rgba(209,90,90,0.12))', borderRadius: '2px' },
    '.cm-tokmod-deprecated': { textDecoration: 'line-through', opacity: '0.75' },
    '.cm-tokmod-async':    { fontStyle: 'italic' },
    '.cm-tokmod-documentation': { opacity: '0.9' },

    // ── Namespace palette ──
    //
    // `.cm-tok-ns-0…N` — a colour per namespace FAMILY rather than per token kind (a
    // JSP taglib prefix, an XML namespace). Categorical, not semantic: see
    // `namespace-palette.ts` for what the hues do and do not mean.
    ...namespaceThemeSpec,

    // ── Autocomplete + hover docs ──
    //
    // The row is a three-column line, laid out rather than concatenated: what it IS (the icon),
    // what it is CALLED (the label, with the typed part marked), what its SHAPE is (the
    // signature), and — pushed to the right edge — where it comes FROM. Reading a member list is
    // scanning one of those columns at a time, and a run-on line of `name name(String) : void`
    // makes every one of them harder to find.
    '.cm-tooltip-autocomplete': {
      backgroundColor: 'var(--bg-elevated)',
      border: '1px solid var(--border-subtle)', borderRadius: 'var(--radius-md)',
      boxShadow: '0 8px 24px rgba(0,0,0,0.28)',
      overflow: 'hidden',
    },
    '.cm-tooltip-autocomplete > ul': {
      // Twelve or so rows: enough to hold the answer, short enough that the popup does not become
      // the window. Past that it scrolls, which is what the ranking is for.
      maxHeight: '19em',
      fontFamily: 'var(--font-code)', fontSize: 'var(--font-size-sm)',
    },
    '.cm-tooltip-autocomplete > ul > li': {
      display: 'flex', alignItems: 'baseline', gap: '0.5em',
      padding: '3px 9px 3px 6px', color: 'var(--text-primary)',
      lineHeight: '1.45',
    },
    '.cm-tooltip-autocomplete > ul > li[aria-selected]': {
      backgroundColor: 'var(--accent-subtle)', color: 'var(--text-primary)',
    },
    // The label never shrinks: it is the thing being chosen. Everything else on the row gives way
    // to it, and the signature is what truncates when the popup runs out of width.
    '.cm-completionLabel': { color: 'var(--text-primary)', flex: '0 0 auto', whiteSpace: 'pre' },
    '.cm-completionDetail': {
      color: 'var(--text-muted)', fontStyle: 'normal', margin: '0',
      flex: '0 1 auto', minWidth: '0',
      overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
    },
    '.cm-completionOrigin': {
      marginLeft: 'auto', paddingLeft: '1.2em',
      flex: '0 0 auto',
      color: 'var(--text-subtle, var(--text-muted))', opacity: '0.75',
      fontSize: '0.92em',
    },
    '.cm-completionMatchedText': { color: 'var(--accent)', textDecoration: 'none', fontWeight: '700' },
    // Still offered — it exists, and you may be reading old code — and struck through, which is
    // the same mark the editor puts on a deprecated symbol in the text itself.
    '.cm-tooltip-autocomplete > ul > li.cm-completion-deprecated .cm-completionLabel': {
      textDecoration: 'line-through', opacity: '0.7',
    },
    // CodeMirror ships a glyph for its own dozen completion types and none for this one, so an
    // annotation would render with an empty icon column — a gap in a list where every other row has
    // a mark, which reads as a broken row rather than as a kind. The `@` is the same character that
    // asked for the list.
    '.cm-completionIcon-annotation::after': { content: "'@'" },
    // A member that does not exist yet — the row offers to WRITE it, and the glyph is the only
    // thing on the line that says so before it is accepted.
    '.cm-completionIcon-generate::after': { content: "'+'" },
    '.cm-completionIcon-generate': { color: 'var(--accent)' },
    '.cm-completionIcon': { opacity: '0.85', paddingRight: '0.35em' },

    // ── The documentation panel beside the list ──
    //
    // The same card the hover tooltip draws, from the same backend answer — see
    // `completion-item`. Sized so it cannot outgrow the popup it hangs off: a panel taller than
    // the screen is a panel whose top you cannot read.
    '.cm-tooltip.cm-completionInfo': {
      backgroundColor: 'var(--bg-elevated)',
      border: '1px solid var(--border-subtle)', borderRadius: 'var(--radius-md)',
      boxShadow: '0 8px 24px rgba(0,0,0,0.28)',
      padding: '0', margin: '0 6px',
      maxWidth: '460px', maxHeight: '340px', overflow: 'auto',
      overscrollBehavior: 'contain',
      fontFamily: 'var(--font-ui-sans)', fontSize: 'var(--font-size-sm)',
    },
    '.cm-completionInfo .cm-hc-info': { padding: '8px 11px 9px' },
    '.cm-completionInfo .cm-hc-info-sig': {
      fontFamily: 'var(--font-code)', color: 'var(--text-primary)', whiteSpace: 'pre-wrap',
    },
    '.cm-completionInfo .cm-hc-info-meta': {
      color: 'var(--text-muted)', fontSize: '0.9em', marginTop: '2px',
    },
    // The member a `generate` row will write, shown before it is accepted.
    '.cm-completionInfo .cm-hc-generate': {
      fontFamily: 'var(--font-code)', fontSize: 'var(--font-size-xs)',
      color: 'var(--text-primary)', margin: '0', whiteSpace: 'pre',
      overflowX: 'auto',
    },
    '.cm-completionInfo .cm-hc-doc': {
      marginTop: '7px', paddingTop: '7px', borderTop: '1px solid var(--border-subtle)',
      color: 'var(--text-secondary, var(--text-primary))',
    },
  },
  { dark: true },
);
