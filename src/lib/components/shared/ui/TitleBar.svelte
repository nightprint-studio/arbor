<script module lang="ts">
  import type { Snippet } from 'svelte';
  import type { DropdownItem } from './Dropdown.svelte';
  import type { TooltipInput } from '$lib/stores/tooltip.svelte';

  /**
   * One configurable button in the title bar's right cluster.
   *
   * Two flavours, picked by whether `menu` is set:
   *  • plain   → fires `onclick` (toggle a panel, open an overlay, …)
   *  • menu    → opens a `Dropdown` built from `menu` items
   */
  export interface TitleBarButton {
    /** Persistent "lit" state — accent colour even when the menu is closed. */
    active?: boolean;
    /** Click handler (plain mode — ignored when `menu` is set). */
    onclick?: () => void;
    /** Tooltip text (or full tooltip options); defaults to the slot's canonical label. */
    tooltip?: TooltipInput;
    /** Lucide component override; falls back to the slot's canonical icon. */
    icon?: any;
    /** When set, the button opens a dropdown built from these items. */
    menu?: DropdownItem[];
    /** CSS width for the dropdown panel (default `220px`). */
    menuWidth?: string;
    /** Upper cap on the dropdown panel height (lets long lists scroll). */
    menuMaxHeight?: number;
    /** Accessible label override (defaults to the canonical label). */
    ariaLabel?: string;
  }
</script>

<script lang="ts">
  /**
   * TitleBar — app-agnostic window chrome shell.
   *
   * Renders the standard top bar skeleton (drag region · brand · hamburger ·
   * leading content · spacer · right cluster · window controls) and leaves
   * every Arbor/merula-specific piece to the consumer:
   *
   *  • `logo`            — app mark snippet (omit → no brand slot)
   *  • `menu`/`hamburger`— structured `DropdownItem[]` OR a custom control
   *                        (omit both → no hamburger)
   *  • `leading`         — free content after the hamburger (workspace/project
   *                        switcher, plugin-left items, …)
   *  • `trailing`        — free content at the head of the right cluster
   *                        (run controls, status pills, layout toggles, …)
   *  • `actions`         — free-form buttons just before the named buttons
   *  • `docs` / `commandPalette` / `settings` — optional built-in buttons; each
   *                        omitted slot renders nothing. `settings` may carry a
   *                        `menu` to open a dropdown instead of firing onclick.
   *  • `settingsContent` — escape hatch: a fully custom settings control
   *                        (used when the menu needs a shape a flat
   *                        `DropdownItem[]` can't express)
   *  • `windowControls`  — min/max/close slot (consumer passes its own)
   *
   * No domain imports live here — only the shared `Dropdown`, lucide glyphs and
   * the tooltip action (both already standard inside `shared/ui/`).
   */
  import Dropdown from './Dropdown.svelte';
  import { AlignJustify, BookOpen, Command, Settings } from 'lucide-svelte';
  // Title bar sits at the very top — tooltips fly downward so they're never
  // clipped by the window edge.
  import { tooltipBottom as tooltip } from '$lib/actions/tooltip';

  interface Props {
    /** App mark at the far left. Omit → no brand slot. */
    logo?: Snippet;
    logoTooltip?: string;
    /** Structured hamburger menu. Omit (and no `hamburger`) → no hamburger. */
    menu?: DropdownItem[];
    menuWidth?: string;
    menuTooltip?: string;
    /** Escape hatch: a fully custom hamburger control (wins over `menu`). */
    hamburger?: Snippet;
    /** Free content after the hamburger, before the draggable spacer. */
    leading?: Snippet;
    /** Free content at the head of the right cluster. */
    trailing?: Snippet;
    /** Free-form buttons just before the named buttons. */
    actions?: Snippet;
    /** Built-in documentation toggle button. */
    docs?: TitleBarButton;
    /** Built-in command-palette toggle button. */
    commandPalette?: TitleBarButton;
    /** Built-in settings button (plain or dropdown). */
    settings?: TitleBarButton;
    /** Escape hatch replacing the structured settings button. */
    settingsContent?: Snippet;
    /** Window controls slot — consumer passes its own min/max/close. */
    windowControls?: Snippet;
    class?: string;
  }

  let {
    logo, logoTooltip, menu, menuWidth = '280px', menuTooltip = 'Main menu',
    hamburger, leading, trailing, actions,
    docs, commandPalette, settings, settingsContent,
    windowControls, class: cls = '',
  }: Props = $props();
</script>

{#snippet namedButton(btn: TitleBarButton, fallbackIcon: any, fallbackLabel: string)}
  {@const Icon = btn.icon ?? fallbackIcon}
  {#if btn.menu}
    <Dropdown
      items={btn.menu}
      position="fixed"
      direction="down"
      width={btn.menuWidth ?? '220px'}
      maxHeight={btn.menuMaxHeight}
    >
      {#snippet trigger({ open, toggle })}
        <button
          class="tb-icon"
          class:active={btn.active || open}
          onclick={toggle}
          use:tooltip={btn.tooltip ?? fallbackLabel}
          aria-label={btn.ariaLabel ?? fallbackLabel}
          aria-haspopup="menu"
          aria-expanded={open}
        >
          <Icon size={18} />
        </button>
      {/snippet}
    </Dropdown>
  {:else}
    <button
      class="tb-icon"
      class:active={btn.active}
      onclick={btn.onclick}
      use:tooltip={btn.tooltip ?? fallbackLabel}
      aria-label={btn.ariaLabel ?? fallbackLabel}
      aria-pressed={btn.active}
    >
      <Icon size={18} />
    </button>
  {/if}
{/snippet}

<div class="tb {cls}" data-tauri-drag-region role="banner">
  {#if logo}
    <div class="tb-nodrag tb-brand" use:tooltip={logoTooltip ?? ''}>
      {@render logo()}
    </div>
  {/if}

  {#if hamburger}
    <div class="tb-nodrag">{@render hamburger()}</div>
  {:else if menu}
    <div class="tb-nodrag">
      <Dropdown items={menu} position="fixed" direction="down" width={menuWidth}>
        {#snippet trigger({ open, toggle })}
          <button
            class="tb-hamburger"
            class:active={open}
            onclick={toggle}
            use:tooltip={menuTooltip}
            aria-label="Open main menu"
            aria-haspopup="menu"
            aria-expanded={open}
          >
            <AlignJustify size={20} strokeWidth={2} />
          </button>
        {/snippet}
      </Dropdown>
    </div>
  {/if}

  {#if leading}
    <div class="tb-nodrag tb-leading">{@render leading()}</div>
  {/if}

  <!-- Draggable region so the user can grab the empty middle. -->
  <div class="tb-spacer" data-tauri-drag-region></div>

  <div class="tb-right tb-nodrag">
    {#if trailing}{@render trailing()}{/if}
    {#if actions}{@render actions()}{/if}
    {#if docs}{@render namedButton(docs, BookOpen, 'Documentation')}{/if}
    {#if commandPalette}{@render namedButton(commandPalette, Command, 'Command palette')}{/if}
    {#if settingsContent}
      {@render settingsContent()}
    {:else if settings}
      {@render namedButton(settings, Settings, 'Settings')}
    {/if}
    {#if windowControls}
      <div class="tb-sep"></div>
      {@render windowControls()}
    {/if}
  </div>
</div>

<style>
  .tb {
    display: flex;
    align-items: center;
    height: var(--titlebar-h, 42px);
    background: var(--bg-elevated);
    flex-shrink: 0;
    overflow: visible;
    position: relative;
    z-index: 100;
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.04);
    transition: height var(--anim-dur-base) ease;
  }

  /* Each interactive cluster opts out of the window-drag region and lays its
     children out inline. (Plain flex, not `display: contents`, so the wrapper
     keeps a measurable box for the Dropdown anchor + its own padding.) */
  .tb-nodrag {
    -webkit-app-region: no-drag;
    display: flex;
    align-items: center;
  }

  .tb-brand {
    padding: 0 8px;
    flex-shrink: 0;
  }

  .tb-leading {
    gap: 4px;
    min-width: 0;
  }

  .tb-spacer {
    flex: 1;
    min-width: 40px;
    height: 100%;
  }

  .tb-right {
    height: 100%;
    flex-shrink: 0;
  }

  /* ── Hamburger trigger ──────────────────────────────────────────────── */
  .tb-hamburger {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 42px;
    height: 42px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-secondary);
    transition: background var(--transition-fast), color var(--transition-fast);
    -webkit-app-region: no-drag;
  }
  .tb-hamburger:hover,
  .tb-hamburger.active {
    background: var(--bg-overlay);
    color: var(--text-primary);
  }

  /* ── Named / icon buttons (docs · palette · settings) ───────────────── */
  .tb-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 34px;
    height: 34px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-secondary);
    transition: background var(--transition-fast), color var(--transition-fast);
    -webkit-app-region: no-drag;
  }
  .tb-icon:hover { background: var(--bg-hover); color: var(--text-primary); }
  .tb-icon.active { color: var(--accent); }

  .tb-sep {
    width: 1px;
    height: 18px;
    background: var(--border);
    flex-shrink: 0;
    margin: 0 4px;
  }

  /* Compact title bar — shrinks icon buttons to keep the chrome proportional
     when the host lowers `--titlebar-h`. Mirrors the previous Arbor rule. */
  :global([data-compact-title-bar="true"]) .tb-icon {
    width: 26px;
    height: 26px;
  }
  :global([data-compact-title-bar="true"]) .tb-sep {
    height: 14px;
  }
</style>
