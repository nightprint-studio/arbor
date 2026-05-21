<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    class?: string;
    /** Lucide icon or any element shown left of the title */
    icon?: Snippet;
    /** Panel title (rendered uppercase, small-caps style) */
    title: string;
    /** Optional count badge next to the title */
    count?: number | null;
    /** Action buttons on the right side of the header */
    actions?: Snippet;
    /**
     * Optional second row below the header:
     * search bar, filter chips, tab bar, etc.
     */
    toolbar?: Snippet;
    /** Main scrollable content */
    children: Snippet;
    /** Optional fixed footer below the scrollable body */
    footer?: Snippet;
    /** Whether the body scrolls (default true) */
    scrollable?: boolean;
    /**
     * When true, the default header is not rendered. Use this when an
     * outer chrome (e.g. `BottomPanelHeader` for a bottom-docked panel)
     * provides the header instead, so we don't render a duplicate bar.
     */
    hideHeader?: boolean;
  }

  let {
    icon,
    title,
    count = null,
    actions,
    toolbar,
    children,
    footer,
    scrollable = true,
    hideHeader = false,
    class: extraClass = '',
  }: Props = $props();
</script>

<div class="panel-shell {extraClass}">
  <!-- ── Header (skipped when an outer chrome owns the title bar) ── -->
  {#if !hideHeader}
    <div class="ps-header">
      <div class="ps-left">
        {#if icon}
          <span class="ps-icon">{@render icon()}</span>
        {/if}
        <span class="ps-title">{title}</span>
        {#if count != null && count > 0}
          <span class="ps-count">{count}</span>
        {/if}
      </div>
      {#if actions}
        <div class="ps-actions">
          {@render actions()}
        </div>
      {/if}
    </div>
  {/if}

  <!-- ── Optional toolbar (search / filters / tabs) ── -->
  {#if toolbar}
    <div class="ps-toolbar">
      {@render toolbar()}
    </div>
  {/if}

  <!-- ── Body ── -->
  <div class="ps-body" class:scrollable>
    {@render children()}
  </div>

  <!-- ── Footer (fixed, non-scrolling) ── -->
  {#if footer}
    <div class="ps-footer">
      {@render footer()}
    </div>
  {/if}
</div>

<style>
  .panel-shell {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
    background: var(--bg-base);
  }

  /* ── Header ──
     Default header is transparent (inherits --bg-base from .panel-shell)
     so sidebar panels like the main branches/reflog panel stay flat.
     The PluginPanel (and any other panel wanting chrome) opts in via
     `class="plugin-panel-shell"` — see override below. */
  .ps-header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 8px 0 12px;
    height: 34px;
    min-height: 34px;
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
  }

  .ps-left {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 1;
    min-width: 0;
    overflow: hidden;
  }

  .ps-icon {
    display: flex;
    align-items: center;
    flex-shrink: 0;
    color: var(--accent);
  }

  .ps-title {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.3px;
    color: var(--text-secondary);
    text-transform: uppercase;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex-shrink: 1;
  }

  .ps-count {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 16px;
    height: 14px;
    padding: 0 4px;
    background: rgba(255, 255, 255, 0.06);
    border-radius: 999px;
    font-size: 10px;
    font-weight: 600;
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .ps-actions {
    display: flex;
    align-items: center;
    gap: 2px;
    flex-shrink: 0;
  }

  /* ── Toolbar (below header) ── */
  .ps-toolbar {
    flex-shrink: 0;
    border-bottom: 1px solid var(--border-subtle);
  }

  /* ── Body ── */
  .ps-body {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .ps-body.scrollable {
    overflow-y: auto;
  }

  /* ── Footer ── */
  .ps-footer {
    flex-shrink: 0;
    border-top: 1px solid var(--border-subtle);
  }

  /* ── PluginPanel opt-in chrome variant ──
     The plugin manager explicitly wants the floating-card look (rounded
     borders + elevated header/footer + body as a --bg-base card). Keep
     scoped to avoid leaking the style to regular sidebar panels
     (branches, reflog, issues, …). */
  :global(.panel-shell.plugin-panel-shell) {
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
  }
  :global(.plugin-panel-shell .ps-header) {
    background: var(--modal-chrome-bg);
    border-bottom: none; /* card edge below provides the divider */
  }
  :global(.plugin-panel-shell .ps-toolbar) {
    background: var(--modal-chrome-bg);
    border-bottom-color: var(--modal-chrome-border);
  }
  :global(.plugin-panel-shell .ps-footer) {
    background: var(--modal-chrome-bg);
    border-top: none; /* same reason as the header */
  }
  /* Body card — the equivalent of `.content-area` in MrModal: --bg-base
     with rounded corners + 4px gutter on left/right (and bottom when no
     footer is present). Mirrors the ThemeEditorModal / MR detail rhythm. */
  :global(.plugin-panel-shell .ps-body) {
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
    margin: 0 4px 4px;
  }

  /*
   * Global helper classes for buttons inside PanelShell headers.
   * Use class="ps-btn" on any <button> inside ps-actions or ps-toolbar.
   * Use class="ps-btn ps-btn-accent" for accent-colored create/add buttons.
   */
  :global(.ps-btn) {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-muted);
    padding: 0;
    flex-shrink: 0;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  :global(.ps-btn:hover:not(:disabled)) {
    background: var(--bg-hover);
    color: var(--text-primary);
  }
  :global(.ps-btn:disabled) {
    opacity: 0.35;
    cursor: not-allowed;
  }
  :global(.ps-btn-accent) {
    color: var(--accent);
  }
  :global(.ps-btn-accent:hover:not(:disabled)) {
    background: var(--accent-subtle);
    color: var(--accent-hover);
  }
  /* Destructive action variant — used by Stop / Cancel buttons that
     terminate a running process. The hover background mirrors the accent
     subtle pattern so the affordance feels consistent. */
  :global(.ps-btn-danger) {
    color: var(--error);
  }
  :global(.ps-btn-danger:hover:not(:disabled)) {
    background: var(--error-subtle);
    color: color-mix(in srgb, var(--error) 75%, var(--text-primary));
  }
  /* Positive primary action variant — used by Run / Play buttons. The
     same green appears elsewhere (success notifications, status icons)
     so a single click feels semantically aligned across the IDE. */
  :global(.ps-btn-success) {
    color: var(--success);
  }
  :global(.ps-btn-success:hover:not(:disabled)) {
    background: var(--success-subtle);
    color: color-mix(in srgb, var(--success) 75%, var(--text-primary));
  }
  :global(.ps-btn-active) {
    color: var(--accent);
    background: var(--accent-subtle);
  }
</style>
