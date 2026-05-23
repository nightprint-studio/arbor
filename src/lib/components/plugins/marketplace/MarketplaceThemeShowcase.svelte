<!--
  MarketplaceThemeShowcase — slice-of-Arbor live preview + labelled palette
  swatches for a marketplace theme entry.

  Pure render: drinks `theme.preview` (6 CSS-var colours) and renders a small
  but information-dense replica of Arbor's chrome — title bar, branches
  sidebar, mini graph, sample diff, status bar — so the user can judge fit
  before installing.

  Helper tints (elevated/overlay/border/muted/faint) are derived from
  `--pv-bg`/`--pv-fg` via `color-mix` so the catalog only needs to ship 6
  colours per theme.
-->
<script lang="ts">
  import { Eye, Palette } from 'lucide-svelte';
  import ColorSwatch from '$lib/components/shared/ui/ColorSwatch.svelte';
  import type { MarketplaceTheme } from '$lib/types/marketplace';

  interface Props {
    theme: MarketplaceTheme;
  }
  let { theme }: Props = $props();

  const swatches = $derived([
    { label: 'Background', color: theme.preview.bg      },
    { label: 'Foreground', color: theme.preview.fg      },
    { label: 'Accent',     color: theme.preview.accent  },
    { label: 'Success',    color: theme.preview.success },
    { label: 'Warning',    color: theme.preview.warning },
    { label: 'Error',      color: theme.preview.error   },
  ]);
</script>

<section class="mk-detail-section">
  <h4><Eye size={11} /> Live preview</h4>
  <div
    class="mk-theme-mock-frame"
    style="
      --pv-bg:      {theme.preview.bg};
      --pv-fg:      {theme.preview.fg};
      --pv-accent:  {theme.preview.accent};
      --pv-success: {theme.preview.success};
      --pv-warning: {theme.preview.warning};
      --pv-error:   {theme.preview.error};
    "
  >
    <!-- Title bar -->
    <div class="mk-pv-titlebar">
      <span class="mk-pv-traffic mk-pv-dot-error"></span>
      <span class="mk-pv-traffic mk-pv-dot-warn"></span>
      <span class="mk-pv-traffic mk-pv-dot-ok"></span>
      <span class="mk-pv-title">arbor — <span class="mk-pv-strong">arbor-extensions</span></span>
    </div>

    <div class="mk-pv-body">
      <!-- Sidebar -->
      <aside class="mk-pv-sidebar">
        <div class="mk-pv-sect-h">BRANCHES</div>
        <div class="mk-pv-item mk-pv-active">
          <span class="mk-pv-dot mk-pv-dot-accent"></span>main
        </div>
        <div class="mk-pv-item">
          <span class="mk-pv-dot mk-pv-dot-muted"></span>feat/marketplace
        </div>
        <div class="mk-pv-item">
          <span class="mk-pv-dot mk-pv-dot-muted"></span>fix/oauth-refresh
        </div>
        <div class="mk-pv-sect-h mk-pv-spaced">TAGS</div>
        <div class="mk-pv-item">
          <span class="mk-pv-tag-glyph">▸</span>v1.4.0
        </div>
      </aside>

      <!-- Main content -->
      <div class="mk-pv-main">
        <!-- Graph -->
        <div class="mk-pv-graph">
          <div class="mk-pv-row">
            <span class="mk-pv-node mk-pv-node-accent"></span>
            <span class="mk-pv-msg mk-pv-strong">feat: add marketplace auto-refresh</span>
            <span class="mk-pv-meta">a1b2c3d</span>
          </div>
          <div class="mk-pv-row">
            <span class="mk-pv-node mk-pv-node-secondary"></span>
            <span class="mk-pv-msg">refactor: extract Select widget</span>
            <span class="mk-pv-meta">e4f5g6h</span>
          </div>
          <div class="mk-pv-row">
            <span class="mk-pv-node mk-pv-node-secondary"></span>
            <span class="mk-pv-msg">fix: dropdown flipUp positioning</span>
            <span class="mk-pv-meta">i7j8k9l</span>
          </div>
        </div>

        <!-- Diff -->
        <div class="mk-pv-diff">
          <div class="mk-pv-diff-h">+ src/marketplace/scheduler.rs</div>
          <div class="mk-pv-diff-add">+ const POLL_SECS: u64 = 600;</div>
          <div class="mk-pv-diff-rem">- const POLL_SECS: u64 = 60;</div>
          <div class="mk-pv-diff-ctx">  let interval = read_config();</div>
        </div>
      </div>
    </div>

    <!-- Status bar -->
    <div class="mk-pv-statusbar">
      <span class="mk-pv-status-chip"><span class="mk-pv-dot mk-pv-dot-accent"></span>main</span>
      <span class="mk-pv-status-chip mk-pv-success">✓ 12</span>
      <span class="mk-pv-status-chip mk-pv-warning">⚠ 2</span>
      <span class="mk-pv-status-chip mk-pv-error">✕ 1</span>
      <span class="mk-pv-status-chip mk-pv-muted mk-pv-status-spacer">accent</span>
    </div>
  </div>
</section>

<!-- Swatch row — labelled colour chips so users can grab the exact hex / role
     at a glance even when the mock looks busy. -->
<section class="mk-detail-section">
  <h4><Palette size={11} /> Palette</h4>
  <div class="mk-pv-swatch-row">
    {#each swatches as sw (sw.label)}
      <ColorSwatch color={sw.color} label={sw.label} />
    {/each}
  </div>
</section>

<style>
  /* Section header — mirrors `.mk-detail-section h4` from the parent modal so
     the showcase reads identically next to the other sections (Permissions,
     Source, Tags, …). */
  .mk-detail-section h4 {
    margin: 0 0 8px;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.6px;
    color: var(--text-muted);
    font-weight: 600;
  }

  /* ── Theme live preview — slice-of-Arbor mock UI ───────────────────────
     A small but information-dense replica of Arbor's chrome (title bar +
     branches sidebar + graph + diff + status bar) rendered with the
     theme's 6 preview colours via the `--pv-*` custom properties set
     inline on `.mk-theme-mock-frame`. Helper tints are derived with
     `color-mix` from `--pv-bg`/`--pv-fg` so we get readable elevated
     surfaces and muted text without needing extra catalog fields. */
  .mk-theme-mock-frame {
    /* Derived helpers — kept inside the scope so themes don't leak. */
    --pv-elevated: color-mix(in srgb, var(--pv-fg)  6%, var(--pv-bg));
    --pv-overlay:  color-mix(in srgb, var(--pv-fg) 10%, var(--pv-bg));
    --pv-border:   color-mix(in srgb, var(--pv-fg) 18%, transparent);
    --pv-muted:    color-mix(in srgb, var(--pv-fg) 60%, transparent);
    --pv-faint:    color-mix(in srgb, var(--pv-fg) 42%, transparent);

    background: var(--pv-bg);
    color: var(--pv-fg);
    border: 1px solid var(--pv-border);
    border-radius: var(--radius-md);
    overflow: hidden;
    font-family: var(--font-ui-sans);
    font-size: 11px;
    line-height: 1.4;
    user-select: none;
    box-shadow: 0 4px 14px rgba(0, 0, 0, 0.22);
  }

  /* Title bar */
  .mk-pv-titlebar {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    background: var(--pv-elevated);
    border-bottom: 1px solid var(--pv-border);
  }
  .mk-pv-traffic {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .mk-pv-dot-error { background: var(--pv-error);   }
  .mk-pv-dot-warn  { background: var(--pv-warning); }
  .mk-pv-dot-ok    { background: var(--pv-success); }
  .mk-pv-title {
    margin-left: 6px;
    color: var(--pv-muted);
    font-size: 10.5px;
  }
  .mk-pv-strong { color: var(--pv-fg); font-weight: 600; }

  /* Body — sidebar + main */
  .mk-pv-body {
    display: grid;
    grid-template-columns: 140px 1fr;
    min-height: 200px;
  }

  /* Sidebar */
  .mk-pv-sidebar {
    background: var(--pv-elevated);
    border-right: 1px solid var(--pv-border);
    padding: 8px 6px;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .mk-pv-sect-h {
    font-size: 9px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--pv-faint);
    padding: 4px 6px 2px;
  }
  .mk-pv-sect-h.mk-pv-spaced { margin-top: 8px; }
  .mk-pv-item {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 6px;
    border-radius: 4px;
    color: var(--pv-fg);
    font-size: 10.5px;
  }
  .mk-pv-item.mk-pv-active {
    background: color-mix(in srgb, var(--pv-accent) 18%, transparent);
    color: var(--pv-accent);
  }
  .mk-pv-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .mk-pv-dot-accent { background: var(--pv-accent); }
  .mk-pv-dot-muted  { background: var(--pv-muted);  }
  .mk-pv-tag-glyph {
    color: var(--pv-warning);
    width: 8px;
    text-align: center;
    font-size: 9px;
  }

  /* Main column — graph + diff */
  .mk-pv-main {
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }

  /* Graph */
  .mk-pv-graph {
    background: var(--pv-overlay);
    border: 1px solid var(--pv-border);
    border-radius: 4px;
    padding: 6px 8px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .mk-pv-row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 10.5px;
    min-width: 0;
  }
  .mk-pv-node {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    flex-shrink: 0;
    box-shadow: inset 0 0 0 1.5px var(--pv-bg);
  }
  .mk-pv-node-accent    { background: var(--pv-accent); }
  .mk-pv-node-secondary { background: var(--pv-muted);  }
  .mk-pv-msg {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .mk-pv-meta {
    color: var(--pv-faint);
    font-family: var(--font-mono);
    font-size: 9.5px;
    flex-shrink: 0;
  }

  /* Diff */
  .mk-pv-diff {
    background: var(--pv-overlay);
    border: 1px solid var(--pv-border);
    border-radius: 4px;
    padding: 6px 8px;
    font-family: var(--font-mono);
    font-size: 10.5px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .mk-pv-diff-h   { color: var(--pv-accent);  font-weight: 600; margin-bottom: 2px; }
  .mk-pv-diff-add { color: var(--pv-success); }
  .mk-pv-diff-rem { color: var(--pv-error);   }
  .mk-pv-diff-ctx { color: var(--pv-muted);   }

  /* Status bar */
  .mk-pv-statusbar {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 4px 10px;
    background: var(--pv-elevated);
    border-top: 1px solid var(--pv-border);
    font-size: 10px;
  }
  .mk-pv-status-chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--pv-fg);
  }
  .mk-pv-status-chip.mk-pv-success { color: var(--pv-success); }
  .mk-pv-status-chip.mk-pv-warning { color: var(--pv-warning); }
  .mk-pv-status-chip.mk-pv-error   { color: var(--pv-error);   }
  .mk-pv-status-chip.mk-pv-muted   { color: var(--pv-accent);  }
  .mk-pv-status-spacer { margin-left: auto; }

  /* Palette swatch row — labelled chips for each preview colour. */
  .mk-pv-swatch-row {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
    gap: 6px;
  }
</style>
