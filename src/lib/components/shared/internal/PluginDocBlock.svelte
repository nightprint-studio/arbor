<!--
  PluginDocBlock — typography baseline for plugin / docs authored HTML.

  Used by:
    · DocsPanel    — wraps both static section components and `{@html plugin.doc}`
    · MarketplacePluginDetail — renders `{@html plugin.doc}` in the detail pane

  Owns the typography rules that the two surfaces were duplicating (h1-h4, p,
  ul/ol/li, strong, kbd, code, pre, table). Both consumers get identical baseline
  reading experience; future tweaks land in one place.

  What it does NOT own: layout chrome (DocsPanel's scroll container, marketplace's
  outer section), and the docs design-system utilities (`.callout`,
  `.feature-grid`, `.step-list`, `.eyebrow`, `.badge`, `.matrix`, `.prop-list`,
  `.indicator-list`, `.hint`, `.chip`) — those are DocsPanel-internal authoring
  conventions and stay scoped there. They reach through the widget via DocsPanel's
  outer `.docs-content` wrapper so existing static section components keep
  rendering with the same chrome.

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
    font-size: 11px;
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
    font-size: 11px;
  }
  .doc-block :global(h3) {
    font-size: 12px;
    font-weight: 700;
    color: var(--text-primary);
    margin: 16px 0 6px;
    letter-spacing: 0.2px;
  }
  .doc-block :global(h4) {
    font-size: 10px;
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
    font-size: 10px;
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
    font-size: 11px;
    background: var(--bg-overlay);
    padding: 1px 4px;
    border-radius: var(--radius-sm);
    color: var(--accent);
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
    font-size: 11px;
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
    font-size: 9.5px;
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
  }
  .doc-block :global(tbody tr:last-child td) { border-bottom: none; }
  .doc-block :global(a) { color: var(--accent); }
</style>
