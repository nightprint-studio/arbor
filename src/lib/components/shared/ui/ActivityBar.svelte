<script module lang="ts">
  import type { Snippet } from 'svelte';

  /**
   * One entry in an activity rail's `topItems` / `bottomItems` list.
   *
   * Three kinds:
   *  • button     — the common icon button (lucide `icon`, `emoji`, or a custom
   *                 `iconSnippet` for brand marks / plugin icons). Fires `onclick`.
   *  • separator  — a thin divider between item groups.
   *  • custom     — an escape hatch rendering an arbitrary snippet (e.g. a combo
   *                 widget) when a flat button can't express the control.
   */
  export interface ActivityRailButton {
    kind?: 'button';
    id: string;
    /** Tooltip text (flown out beside the rail). */
    tooltip?: string;
    /** Optional keybinding hint rendered Arbor-style (Kbd caps) inside the
     *  tooltip — preferred over inlining `· Ctrl+…` into `tooltip`. */
    shortcut?: string;
    /** Lucide-style icon component, rendered at `iconSize`. */
    icon?: any;
    /** Icon size in px (default 18). */
    iconSize?: number;
    /** Emoji-as-icon fallback (used when `icon`/`iconSnippet` are absent). */
    emoji?: string;
    /** Custom icon renderer — brand marks, plugin icons, anything bespoke. */
    iconSnippet?: Snippet;
    /** Lit/accent state — the side-aware active bar is drawn for this button. */
    active?: boolean;
    /**
     * Small corner dot marking "there is something here" without opening the
     * panel — unread items, open findings, a failing check. The tone picks the
     * colour; `true` means `accent`. Purely a marker: pair it with a `tooltip`
     * that says what the dot is about, since a dot alone tells nobody anything.
     */
    dot?: boolean | 'accent' | 'error' | 'warning' | 'success';
    /** Accessible label (defaults to `tooltip`). */
    ariaLabel?: string;
    onclick: () => void;
  }

  export interface ActivityRailSeparator {
    kind: 'separator';
    id?: string;
  }

  export interface ActivityRailCustom {
    kind: 'custom';
    id: string;
    render: Snippet;
  }

  export type ActivityRailItem =
    | ActivityRailButton
    | ActivityRailSeparator
    | ActivityRailCustom;
</script>

<script lang="ts">
  /**
   * ActivityBar — shared shell for a vertical icon rail (left or right).
   *
   * Renders the 38px rail container, top/bottom groups with a flex-1 spacer
   * between them, and the side-aware active accent bar. Each group can be fed
   * two ways:
   *   • declaratively, via `topItems` / `bottomItems` (`ActivityRailItem[]`) —
   *     the external contribution point; the rail renders the buttons itself;
   *   • imperatively, via the `top` / `bottom` snippets — for consumers whose
   *     items are too bespoke for the flat item model (combos, conditional
   *     brand icons, per-item store wiring). A provided items array wins over
   *     the matching snippet.
   *
   * All shared visual rules (`.ab-btn`, `.ab-group`, `.ab-spacer`,
   * `.ab-separator`, `.ab-emoji`) are emitted as `:global()` so consumer-defined
   * snippets can reuse them without restating the styles.
   */
  import { tooltip } from '$lib/actions/tooltip';

  interface Props {
    side?: 'left' | 'right';
    ariaLabel?: string;
    /** Declarative top group. Wins over the `top` snippet when provided. */
    topItems?: ActivityRailItem[];
    /** Declarative bottom group. Wins over the `bottom` snippet when provided. */
    bottomItems?: ActivityRailItem[];
    top?: Snippet;
    bottom?: Snippet;
  }

  let {
    side = 'left',
    ariaLabel,
    topItems,
    bottomItems,
    top,
    bottom,
  }: Props = $props();
</script>

{#snippet railItem(item: ActivityRailItem)}
  {#if item.kind === 'separator'}
    <div class="ab-separator" role="separator"></div>
  {:else if item.kind === 'custom'}
    {@render item.render()}
  {:else}
    {@const Icon = item.icon}
    <button
      class="ab-btn"
      class:ab-active={item.active}
      use:tooltip={item.shortcut ? { content: item.tooltip ?? '', shortcut: item.shortcut } : (item.tooltip ?? '')}
      aria-pressed={item.active}
      aria-label={item.ariaLabel ?? item.tooltip}
      onclick={item.onclick}
    >
      {#if item.iconSnippet}
        {@render item.iconSnippet()}
      {:else if item.emoji}
        <span class="ab-emoji">{item.emoji}</span>
      {:else if Icon}
        <Icon size={item.iconSize ?? 18} />
      {/if}
      {#if item.dot}
        <span
          class="ab-dot"
          data-tone={item.dot === true ? 'accent' : item.dot}
          aria-hidden="true"
        ></span>
      {/if}
    </button>
  {/if}
{/snippet}

<div
  class="activity-bar"
  data-side={side}
  role="navigation"
  aria-label={ariaLabel ?? (side === 'right' ? 'Right Activity Bar' : 'Activity Bar')}
>
  <div class="ab-group ab-top">
    {#if topItems}
      {#each topItems as item, i (item.id ?? `i:${i}`)}{@render railItem(item)}{/each}
    {:else if top}{@render top()}{/if}
  </div>

  <div class="ab-spacer"></div>

  <div class="ab-group ab-bottom">
    {#if bottomItems}
      {#each bottomItems as item, i (item.id ?? `i:${i}`)}{@render railItem(item)}{/each}
    {:else if bottom}{@render bottom()}{/if}
  </div>
</div>

<style>
  /* All rules are :global() because consumer snippets render the actual
     <button class="ab-btn"> elements in the consumer's CSS scope, which
     wouldn't match scoped descendant selectors written here. The class
     names are unique to ActivityBar so global scoping is safe. */

  /*
   * The rail carries the second half of the product's corner glow: the title bar's tint
   * runs across the top, and this one turns it down the left edge, so the two read as one
   * corner instead of as two unrelated stripes.
   *
   * Driven by `--product-tint` on the document root (see `routes/+page.svelte`) — with the
   * variable unset the mix resolves to transparent and this costs nothing, which is why
   * there is no prop and no class to keep in step.
   *
   * Stops in PIXELS so the glow is a fixed physical size: on a tall window a percentage
   * would run the colour most of the way down the rail, which is a coloured sidebar, not a
   * corner. Only the LEFT rail is tinted — the right one is nowhere near the corner, and
   * colouring both would just be two stripes again.
   */
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
  :global(.activity-bar[data-side='left']) {
    background-image: linear-gradient(
      180deg,
      color-mix(in srgb, var(--product-tint, transparent) 13%, transparent) 0px,
      color-mix(in srgb, var(--product-tint, transparent) 4%, transparent) 70px,
      transparent 190px
    );
    background-repeat: no-repeat;
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

  /* Translucent, like the title bar's: the top of this rail carries the product tint, and
     an opaque hover fill would cut a grey square out of it. Mixed from `--text-primary`, so
     the same rule lightens on a dark theme and darkens on a light one. */
  :global(.activity-bar .ab-btn:hover) {
    background: color-mix(in srgb, var(--text-primary) 9%, transparent);
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

  /* Corner marker — "there is something here". Ringed in the rail's own
     background so it stays legible over an active button's tinted fill. */
  :global(.activity-bar .ab-dot) {
    position: absolute;
    top: 5px;
    right: 5px;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    border: 1.5px solid var(--bg-elevated);
    background: var(--accent);
  }
  :global(.activity-bar .ab-dot[data-tone="error"])   { background: var(--error); }
  :global(.activity-bar .ab-dot[data-tone="warning"]) { background: var(--warning); }
  :global(.activity-bar .ab-dot[data-tone="success"]) { background: var(--success); }

  /* Emoji-as-icon fallback (plugin actions whose `icon` is a single emoji). */
  :global(.activity-bar .ab-emoji) {
    font-size: var(--font-size-xl);
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
