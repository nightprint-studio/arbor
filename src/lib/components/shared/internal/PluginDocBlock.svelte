<!--
  PluginDocBlock — the reading surface for authored documentation HTML: the
  typography baseline *and* the authoring vocabulary the doc pages are written
  against.

  Used by:
    · DocsShell (every product's docs panel) — wraps both topic components and
      runtime `{@html}` topics
    · MarketplacePluginDetail — renders `{@html plugin.doc}` in the detail pane

  Owns two layers:
    · Typography — h1-h4, p, ul/ol/li, strong, kbd, code, pre, table.
    · The design-system utilities a doc page may reach for: `.doc-lead`,
      `.callout`, `.step-list`, `.feature-grid`, `.eyebrow`, `.badge`,
      `.meta-grid`, `.prop-list`, `.matrix`, `.indicator-list`, `.hint`,
      `.chip`, `.stat-row`, `.divider`, plus the Prism token colours.

  The utilities used to live in DocsPanel, scoped to its own `.docs-content`
  wrapper. That made them unusable from every other surface that renders the
  same kind of HTML — a Picus topic or a plugin's `doc.html` got the typography
  but none of the vocabulary — and it tied a shared authoring convention to one
  product's panel. They belong next to the typography they extend: one file
  defines what a doc page may write, and every surface renders it the same way.

  What it does NOT own: layout chrome (the docs scroll container, marketplace's
  outer section).

  Consumers pick how to feed the content:
    · `html`     — for `{@html ...}` blob rendering
    · `children` — for Svelte snippets (DocsPanel's section component path)

  Optional:
    · `card`    — wraps in a bordered/elevated frame (Marketplace)
    · `innerEl` — `bind`-able reference to the inner content div (DocsPanel uses
                  this for highlight injection during search)
-->
<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    /** HTML string to render via `{@html}`. */
    html?:     string;
    /** Alternative to `html`: Svelte children. */
    children?: Snippet;
    /** When true, wraps in a bordered card frame. */
    card?:     boolean;
    /** Bindable reference to the inner content div. */
    innerEl?:  HTMLElement | null;
  }

  let {
    html,
    children,
    card = false,
    innerEl = $bindable(null),
  }: Props = $props();
</script>

<div class="doc-block" class:doc-block-card={card}>
  <div class="doc-block-inner" bind:this={innerEl}>
    {#if html !== undefined}
      {@html html}
    {:else if children}
      {@render children()}
    {/if}
  </div>
</div>

<style>
  .doc-block-card {
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: 12px;
  }
  .doc-block-inner {
    padding: 18px 22px 22px;
    user-select: text;
  }
  /* Re-enable text selection for every descendant — the global
     `body { user-select: none }` rule wins over the inherited value
     unless we restate it on the children. */
  .doc-block-inner :global(*) { user-select: text; }

  /* ── Typography ──────────────────────────────────────────────────────
     All rules use :global() so they reach `{@html}` content and child
     Svelte components alike. */

  .doc-block :global(h1) {
    font-size: 19px;
    font-weight: 700;
    color: var(--text-primary);
    margin: 0 0 14px;
    padding-bottom: 10px;
    border-bottom: 1px solid var(--border-subtle);
    letter-spacing: -0.2px;
  }
  .doc-block :global(h2) {
    font-size: var(--font-size-xs);
    font-weight: 700;
    color: var(--text-secondary);
    margin: 22px 0 10px;
    padding-bottom: 6px;
    border-bottom: 1px solid var(--border-subtle);
    text-transform: uppercase;
    letter-spacing: 0.7px;
  }
  .doc-block :global(h2 code) {
    text-transform: none;
    letter-spacing: 0;
    font-size: var(--font-size-xs);
  }
  .doc-block :global(h3) {
    font-size: var(--font-size-sm);
    font-weight: 700;
    color: var(--text-primary);
    margin: 16px 0 6px;
    letter-spacing: 0.2px;
  }
  .doc-block :global(h4) {
    font-size: var(--font-size-2xs);
    font-weight: 700;
    color: var(--text-muted);
    margin: 12px 0 4px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }
  .doc-block :global(h1):first-child,
  .doc-block :global(h2):first-child,
  .doc-block :global(h3):first-child { margin-top: 0; }

  .doc-block :global(p) {
    font-size: var(--font-size-sm);
    color: var(--text-secondary);
    line-height: 1.65;
    margin: 0 0 10px;
  }

  .doc-block :global(ul),
  .doc-block :global(ol) {
    margin: 0 0 12px;
    padding-left: 20px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .doc-block :global(li) {
    font-size: var(--font-size-sm);
    color: var(--text-secondary);
    line-height: 1.5;
  }

  .doc-block :global(strong) { color: var(--text-primary); font-weight: 600; }

  .doc-block :global(kbd) {
    display: inline-block;
    font-family: var(--font-code);
    font-size: var(--font-size-2xs);
    background: var(--bg-overlay);
    border: 1px solid var(--border);
    border-bottom-width: 2px;
    padding: 1px 5px;
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    white-space: nowrap;
  }
  .doc-block :global(code) {
    font-family: var(--font-code);
    font-size: var(--font-size-xs);
    background: var(--bg-overlay);
    padding: 1px 4px;
    border-radius: var(--radius-sm);
    color: var(--accent);
    /* Long inline tokens (`s(RolandTR808_bd …)`, `<Machine>_<drum>`) must break
       rather than force the prose column wider than the panel. */
    overflow-wrap: anywhere;
  }
  .doc-block :global(pre) {
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    padding: 12px 14px;
    overflow-x: auto;
    margin: 0 0 14px;
  }
  .doc-block :global(pre code) {
    background: none;
    padding: 0;
    font-size: var(--font-size-xs);
    color: var(--text-secondary);
    border-radius: 0;
  }

  .doc-block :global(table) {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--font-size-xs);
    margin: 10px 0 14px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    overflow: hidden;
  }
  .doc-block :global(th) {
    text-align: left;
    padding: 7px 12px;
    font-size: var(--font-size-3xs);
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--text-muted);
    background: rgba(255, 255, 255, 0.02);
    border-bottom: 1px solid var(--border);
  }
  .doc-block :global(td) {
    padding: 6px 12px;
    color: var(--text-secondary);
    border-bottom: 1px solid var(--border-subtle);
    vertical-align: top;
    line-height: 1.55;
    /* Break long code-y cell content so a wide row can't stretch the table past
       the panel (the `width: 100%` table can then shrink to the column). */
    overflow-wrap: anywhere;
  }
  .doc-block :global(tbody tr:last-child td) { border-bottom: none; }
  /* Hover feedback for the denser comparison tables the reference pages use. */
  .doc-block :global(tbody tr:hover) { background: rgba(255, 255, 255, 0.015); }
  .doc-block :global(a) { color: var(--accent); }

  /* ═══════════════════════════════════════════════════════════════════
     Doc authoring vocabulary — the classes a doc page may write.
     ═══════════════════════════════════════════════════════════════════ */

  /* ── Lead paragraph ───────────────────────────────────────────────
     `!important` because it is still a `<p>`: without it the baseline
     paragraph rule above wins on the properties they share. */
  .doc-block :global(.doc-lead) {
    font-size: var(--font-size-md) !important;
    color: var(--text-secondary) !important;
    border-left: 3px solid var(--accent);
    padding: 8px 0 8px 14px !important;
    margin-bottom: 18px !important;
    line-height: 1.75 !important;
  }

  /* ── Callout ─────────────────────────────────────────────────────── */
  .doc-block :global(.callout) {
    display: flex;
    gap: 8px;
    background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    padding: 10px 14px;
    margin: 12px 0;
    color: var(--text-secondary);
    font-size: var(--font-size-sm);
    line-height: 1.6;
  }
  .doc-block :global(.callout.accent) { border-left: 3px solid var(--accent); }

  /* ── Feature grid ─────────────────────────────────────────────────── */
  .doc-block :global(.feature-grid) {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(190px, 1fr));
    gap: 8px;
    margin: 12px 0;
  }
  .doc-block :global(.feature-grid.two-col) { grid-template-columns: repeat(2, 1fr); }
  .doc-block :global(.feature-card) {
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 5px;
    transition: border-color var(--transition-fast), background var(--transition-fast);
  }
  .doc-block :global(.feature-card:hover) {
    border-color: var(--border);
    background: var(--bg-overlay);
  }
  .doc-block :global(.feature-card.accent) { border-top: 2px solid var(--accent); padding-top: 10px; }
  .doc-block :global(.fc-title) {
    font-size: var(--font-size-sm);
    font-weight: 600;
    color: var(--text-primary);
  }
  .doc-block :global(.fc-title kbd) { font-size: var(--font-size-3xs); padding: 0 3px; }
  .doc-block :global(.fc-desc) {
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    line-height: 1.6;
  }
  .doc-block :global(.fc-eyebrow) {
    font-size: var(--font-size-3xs);
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--accent);
    margin-bottom: -2px;
  }

  /* ── Step list (numbered visual steps) ────────────────────────────── */
  .doc-block :global(ol.step-list) {
    padding-left: 0;
    list-style: none;
    counter-reset: step-counter;
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 12px 0;
  }
  .doc-block :global(ol.step-list > li) {
    counter-increment: step-counter;
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 9px 14px 9px 12px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    font-size: var(--font-size-sm);
    color: var(--text-secondary);
    line-height: 1.6;
  }
  .doc-block :global(ol.step-list > li::before) {
    content: counter(step-counter);
    flex-shrink: 0;
    width: 20px;
    height: 20px;
    margin-top: 1px;
    background: var(--accent);
    color: #fff;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: var(--font-size-2xs);
    font-weight: 700;
  }

  /* ── Indicator legend (graph symbols) ─────────────────────────────── */
  .doc-block :global(.indicator-list) {
    list-style: none;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 7px;
    margin: 10px 0;
  }
  .doc-block :global(.indicator-list > li) {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: var(--font-size-sm);
    color: var(--text-secondary);
  }
  .doc-block :global(.ind) {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    flex-shrink: 0;
    display: inline-block;
  }
  .doc-block :global(.ind-bright) { background: var(--accent); box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 25%, transparent); }
  .doc-block :global(.ind-dimmed) { background: var(--text-muted); }
  .doc-block :global(.ind-head)   { background: transparent; box-shadow: 0 0 0 2px var(--accent); }
  .doc-block :global(.ind-merge)  { background: var(--accent); border-radius: 2px; transform: rotate(45deg); width: 10px; height: 10px; }
  .doc-block :global(.ind-wip)    { background: transparent; border: 2px dashed var(--border); }
  .doc-block :global(.ind-amber)  { background: var(--color-stash); }

  /* ── Chips ────────────────────────────────────────────────────────── */
  .doc-block :global(.chip) {
    display: inline-block;
    font-family: var(--font-code);
    font-size: var(--font-size-2xs);
    padding: 1px 6px;
    border-radius: var(--radius-sm);
    font-weight: 500;
    vertical-align: middle;
  }
  .doc-block :global(.chip-local)  { background: color-mix(in srgb, var(--accent) 20%, transparent);      color: var(--accent); }
  .doc-block :global(.chip-remote) { background: color-mix(in srgb, var(--color-stash) 20%, transparent); color: var(--color-stash); }
  .doc-block :global(.chip-tag)    { background: color-mix(in srgb, var(--color-tag) 20%, transparent);   color: var(--color-tag); }
  .doc-block :global(.chip-head)   { background: color-mix(in srgb, var(--success) 20%, transparent);     color: var(--success); }
  .doc-block :global(.chip-row) {
    display: inline-flex;
    flex-wrap: wrap;
    gap: 4px;
    align-items: center;
    vertical-align: middle;
  }

  /* ── Eyebrow: accent-coloured label before a heading ──────────────── */
  .doc-block :global(.eyebrow) {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: var(--font-size-3xs);
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.7px;
    color: var(--accent);
    background: rgba(77, 120, 204, 0.12);
    padding: 3px 8px;
    border-radius: var(--radius-lg);
    margin: 0 0 6px;
  }

  /* ── Badges: tiny inline labels ───────────────────────────────────── */
  .doc-block :global(.badge) {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-size: var(--font-size-3xs);
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.4px;
    padding: 1px 6px;
    border-radius: var(--radius-lg);
    line-height: 1.6;
    vertical-align: middle;
    white-space: nowrap;
  }
  .doc-block :global(.badge-req)    { background: color-mix(in srgb, var(--error) 16%, transparent);      color: var(--error); }
  .doc-block :global(.badge-opt)    { background: color-mix(in srgb, var(--text-muted) 18%, transparent); color: var(--text-muted); }
  .doc-block :global(.badge-destr)  { background: color-mix(in srgb, var(--error) 20%, transparent);      color: var(--error); }
  .doc-block :global(.badge-async)  { background: color-mix(in srgb, var(--color-tag) 20%, transparent);  color: var(--color-tag); }
  .doc-block :global(.badge-new)    { background: color-mix(in srgb, var(--success) 20%, transparent);    color: var(--success); }
  .doc-block :global(.badge-beta)   { background: color-mix(in srgb, var(--warning) 20%, transparent);    color: var(--warning); }
  .doc-block :global(.badge-accent) { background: color-mix(in srgb, var(--accent) 20%, transparent);     color: var(--accent); }

  /* ── Meta grid: compact key/value facts ───────────────────────────── */
  .doc-block :global(dl.meta-grid) {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 0;
    margin: 10px 0 14px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    padding: 4px 14px;
  }
  .doc-block :global(dl.meta-grid > dt) {
    color: var(--text-muted);
    font-size: var(--font-size-2xs);
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.4px;
    padding: 8px 18px 8px 0;
    border-bottom: 1px dashed var(--border-subtle);
    align-self: center;
  }
  .doc-block :global(dl.meta-grid > dd) {
    color: var(--text-secondary);
    font-size: var(--font-size-sm);
    margin: 0;
    padding: 8px 0;
    border-bottom: 1px dashed var(--border-subtle);
    line-height: 1.55;
  }
  .doc-block :global(dl.meta-grid > dt:last-of-type),
  .doc-block :global(dl.meta-grid > dd:last-of-type) { border-bottom: none; }

  /* ── Prop list: label + description rows ──────────────────────────
   * This used to be `display: grid` on `<li>` with two columns, but CSS Grid
   * promotes EVERY child (and every anonymous text run) into its own grid
   * item — so an inline `<code>` / `<strong>` inside a description created
   * extra rows and shattered the description into a list of false labels.
   *
   * The float below avoids that: the first `<strong>` / `<code>` floats left
   * with a fixed width and the remaining inline content wraps to its right,
   * staying aligned across lines because the float reserves the column.
   */
  .doc-block :global(ul.prop-list) {
    list-style: none;
    padding: 0;
    margin: 10px 0 14px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .doc-block :global(ul.prop-list > li) {
    padding: 8px 12px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    font-size: var(--font-size-sm);
    color: var(--text-secondary);
    line-height: 1.55;
  }
  /* Clearfix so the floated label doesn't escape its `<li>`. */
  .doc-block :global(ul.prop-list > li::after) {
    content: "";
    display: block;
    clear: both;
  }
  .doc-block :global(ul.prop-list > li > code:first-child),
  .doc-block :global(ul.prop-list > li > strong:first-child) {
    float: left;
    width: 130px;
    margin-right: 14px;
    color: var(--accent);
    font-size: var(--font-size-xs);
    font-weight: 700;
    padding-top: 1px;
  }

  /* ── Matrix table: support comparison ─────────────────────────────── */
  .doc-block :global(table.matrix td.yes)     { color: var(--success); font-weight: 700; text-align: center; }
  .doc-block :global(table.matrix td.no)      { color: var(--text-disabled); text-align: center; }
  .doc-block :global(table.matrix td.partial) { color: var(--warning); font-weight: 700; text-align: center; }
  .doc-block :global(table.matrix th:not(:first-child)),
  .doc-block :global(table.matrix td:not(:first-child)) { text-align: center; }

  /* ── Hint: small inline note (lighter than a callout) ─────────────── */
  .doc-block :global(.hint) {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 7px 12px;
    background: rgba(77, 120, 204, 0.06);
    border-left: 2px solid rgba(77, 120, 204, 0.45);
    border-radius: 0 4px 4px 0;
    font-size: var(--font-size-xs);
    color: var(--text-secondary);
    margin: 8px 0;
    line-height: 1.55;
  }
  .doc-block :global(.hint::before) {
    content: 'i';
    color: var(--accent);
    font-weight: 700;
    font-style: italic;
    font-family: var(--font-code);
    flex-shrink: 0;
    width: 12px;
    height: 12px;
    background: rgba(77, 120, 204, 0.18);
    border-radius: 50%;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: var(--font-size-3xs);
    margin-top: 2px;
  }

  /* ── Stat row: numeric chips ──────────────────────────────────────── */
  .doc-block :global(.stat-row) {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    margin: 10px 0 14px;
  }
  .doc-block :global(.stat) {
    flex: 1 1 120px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .doc-block :global(.stat-value) {
    font-size: var(--font-size-lg);
    font-weight: 700;
    color: var(--text-primary);
    font-family: var(--font-code);
  }
  .doc-block :global(.stat-label) {
    font-size: var(--font-size-3xs);
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--text-muted);
  }

  /* ── Divider with label ───────────────────────────────────────────── */
  .doc-block :global(.divider) {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 18px 0 10px;
    font-size: var(--font-size-3xs);
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.7px;
    color: var(--text-muted);
  }
  .doc-block :global(.divider::before),
  .doc-block :global(.divider::after) {
    content: '';
    flex: 1;
    height: 1px;
    background: var(--border-subtle);
  }

  /* ── Prism syntax tokens ──────────────────────────────────────────
     Bound to the theme's `--syntax-*` tokens so code samples follow the
     palette the user picked; the fallbacks are JetBrains Darcula, for the
     older presets that predate those tokens. */
  .doc-block :global(.token.comment),
  .doc-block :global(.token.prolog),
  .doc-block :global(.token.doctype),
  .doc-block :global(.token.cdata)       { color: var(--syntax-comment, #6a9153); font-style: italic; }
  .doc-block :global(.token.string),
  .doc-block :global(.token.attr-value),
  .doc-block :global(.token.selector),
  .doc-block :global(.token.regex)       { color: var(--syntax-string, #6aab73); }
  .doc-block :global(.token.keyword),
  .doc-block :global(.token.boolean),
  .doc-block :global(.token.constant),
  .doc-block :global(.token.important)   { color: var(--syntax-keyword, #cc7832); font-weight: normal; }
  .doc-block :global(.token.number)      { color: var(--syntax-number, #6897bb); }
  .doc-block :global(.token.function),
  .doc-block :global(.token.class-name)  { color: var(--syntax-function, #ffc66d); }
  .doc-block :global(.token.property),
  .doc-block :global(.token.attr-name)   { color: var(--syntax-number, #9876aa); }
  .doc-block :global(.token.operator),
  .doc-block :global(.token.entity)      { color: var(--text-secondary); }
  .doc-block :global(.token.punctuation) { color: var(--text-muted); }
  .doc-block :global(.token.builtin)     { color: var(--syntax-type, #6897bb); }
  .doc-block :global(.token.variable)    { color: var(--text-secondary); }
  .doc-block :global(.token.parameter)   { color: var(--text-secondary); }
  .doc-block :global(.token.namespace)   { color: var(--syntax-number, #9876aa); }
  .doc-block :global(.token.tag)         { color: var(--syntax-function, #e8bf6a); }
</style>
