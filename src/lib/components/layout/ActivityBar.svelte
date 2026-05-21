<script lang="ts">
  /**
   * ActivityBar — shared shell used by both <ActivityBarLeft> and
   * <ActivityBarRight>. Renders the 38px rail container, top/bottom groups
   * with a flex-1 spacer between them, and the side-aware active accent bar.
   *
   * The two consumers differ only in *what* they put in the groups (built-in
   * + plugin items on the left; plugin-only on the right) and in *which*
   * edge the active accent bar is drawn on — both controlled here via
   * `data-side`. All shared visual rules (`.ab-btn`, `.ab-group`,
   * `.ab-spacer`, `.ab-separator`, `.ab-emoji`) are emitted as `:global()`
   * so consumer-defined snippets can use them without restating the styles.
   */
  import type { Snippet } from 'svelte';

  interface Props {
    side?: 'left' | 'right';
    ariaLabel?: string;
    top?: Snippet;
    bottom?: Snippet;
  }

  let {
    side = 'left',
    ariaLabel,
    top,
    bottom,
  }: Props = $props();
</script>

<div
  class="activity-bar"
  data-side={side}
  role="navigation"
  aria-label={ariaLabel ?? (side === 'right' ? 'Right Activity Bar' : 'Activity Bar')}
>
  <div class="ab-group ab-top">
    {#if top}{@render top()}{/if}
  </div>

  <div class="ab-spacer"></div>

  <div class="ab-group ab-bottom">
    {#if bottom}{@render bottom()}{/if}
  </div>
</div>

<style>
  /* All rules are :global() because consumer snippets render the actual
     <button class="ab-btn"> elements in the consumer's CSS scope, which
     wouldn't match scoped descendant selectors written here. The class
     names are unique to ActivityBar so global scoping is safe. */

  :global(.activity-bar) {
    display: flex;
    flex-direction: column;
    width: 38px;
    flex-shrink: 0;
    height: 100%;
    background: var(--bg-elevated);
    overflow: hidden;
    user-select: none;
  }

  :global(.activity-bar .ab-group) {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 3px;
    padding: 6px 0;
  }

  :global(.activity-bar .ab-spacer) { flex: 1; }

  /* ── Standard button ────────────────────────────────────────────────────── */
  :global(.activity-bar .ab-btn) {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 34px;
    height: 34px;
    border: none;
    background: transparent;
    color: var(--text-secondary);
    border-radius: var(--radius-md);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
    position: relative;
  }

  :global(.activity-bar .ab-btn:hover) {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  :global(.activity-bar .ab-btn.ab-active) {
    color: var(--accent);
    background: var(--accent-subtle);
  }

  /* Side-aware active accent bar — IntelliJ style. The bar always sits on
     the edge ADJACENT to the panel the button activates: left edge on the
     left rail, right edge on the right rail. The border-radius rounds the
     two corners pointing AWAY from that edge. */
  :global(.activity-bar[data-side="left"] .ab-btn.ab-active::before) {
    content: '';
    position: absolute;
    left: 0;
    top: 8px;
    bottom: 8px;
    width: 3px;
    background: var(--accent);
    border-radius: 0 3px 3px 0;
  }

  :global(.activity-bar[data-side="right"] .ab-btn.ab-active::before) {
    content: '';
    position: absolute;
    right: 0;
    top: 8px;
    bottom: 8px;
    width: 3px;
    background: var(--accent);
    border-radius: 3px 0 0 3px;
  }

  /* Emoji-as-icon fallback (plugin actions whose `icon` is a single emoji). */
  :global(.activity-bar .ab-emoji) {
    font-size: 16px;
    line-height: 1;
  }

  /* Visual separator between plugin item groups (registered via
     `arbor.ui.add_separator()`). */
  :global(.activity-bar .ab-separator) {
    width: 28px;
    height: 1px;
    background: var(--border-subtle);
    margin: 4px 0;
    flex-shrink: 0;
  }

  /* PluginIcon (lucide / emoji wrapper) inherits color from the parent
     button so .ab-active turns it accent-coloured automatically. */
  :global(.activity-bar .ab-btn svg),
  :global(.activity-bar .ab-btn .ab-icon) { color: inherit; }
</style>
