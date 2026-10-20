/**
 * Theme rules for the Arbor find widget and the highlights it produces.
 *
 * Shared by every editor that installs `createFindPanel` (the code editor and the markdown
 * live-preview editor), so the widget sits in the same place and matches read the same everywhere.
 * Spread into an `EditorView.theme` spec.
 */

export const findThemeSpec = {
  // Arbor's find widget (`FindWidget.svelte`) is not a band across the top: it floats over the
  // editor's top-right corner so opening it does not shove the code down, and leaves room for the
  // overview strip on the right. Scoped with `:has` so the stock panel the Merula / Studio
  // editors still use keeps its band.
  '.cm-panels.cm-panels-top:has(.arbor-find-host)': {
    position: 'absolute', top: '6px', right: '20px', left: 'auto', width: 'auto',
    background: 'transparent', border: 'none', zIndex: '20',
  },
  // Matches must read at a glance in a screenful of text: a fill alone at ~20% disappears into
  // the selection and the syntax colours, so each carries an outline as well. The occurrences of
  // a selected word (`selectionMatch`) are the quiet tier; find-panel hits are louder, and the
  // current one loudest.
  '.cm-searchMatch': {
    backgroundColor: 'color-mix(in srgb, var(--warning) 38%, transparent)',
    outline: '1px solid color-mix(in srgb, var(--warning) 70%, transparent)',
    borderRadius: '2px',
  },
  '.cm-searchMatch.cm-searchMatch-selected': {
    backgroundColor: 'color-mix(in srgb, var(--accent) 55%, transparent)',
    outline: '1px solid var(--accent)',
  },
  '.cm-selectionMatch': {
    backgroundColor: 'color-mix(in srgb, var(--accent) 32%, transparent)',
    outline: '1px solid color-mix(in srgb, var(--accent) 55%, transparent)',
    borderRadius: '2px',
  },
};
